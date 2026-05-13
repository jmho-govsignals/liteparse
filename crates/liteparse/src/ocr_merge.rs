use crate::ocr::{OcrEngine, OcrOptions};
use crate::types::{Page, TextItem};
use image::{ImageBuffer, Rgba};
use pdfium::Library;
use rayon::prelude::*;

/// Rendered page image ready for OCR.
struct RenderedPage {
    page_index: usize,
    page_number: usize,
    rgb_bytes: Vec<u8>,
    width: u32,
    height: u32,
}

/// Run OCR on pages that need it and merge results into text_items.
///
/// OCR is triggered when a page has fewer than 100 characters of native text
/// or has embedded images. Rendering is sequential (PDFium constraint),
/// but OCR runs in parallel across `num_workers` threads.
pub fn ocr_and_merge_pages(
    pages: &mut [Page],
    pdf_path: &str,
    dpi: f32,
    ocr_engine: &dyn OcrEngine,
    ocr_language: &str,
    num_workers: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let lib = Library::init();
    let document = lib.load_document(pdf_path, None)?;

    // Phase 1: Identify pages needing OCR and render bitmaps (sequential — PDFium is single-threaded)
    let mut rendered: Vec<RenderedPage> = Vec::new();

    for (page_index, page) in pages.iter().enumerate() {
        let text_length: usize = page.text_items.iter().map(|item| item.text.len()).sum();
        let page_obj = document.page((page.page_number - 1) as i32)?;
        let has_images = !page_obj.image_bounds(25.0, 0.9).is_empty();

        if text_length >= 100 && !has_images {
            continue;
        }

        eprintln!(
            "[ocr] page {} needs OCR (text_length={}, has_images={})",
            page.page_number, text_length, has_images
        );

        let bitmap = page_obj.render(dpi)?;
        let width = bitmap.width() as u32;
        let height = bitmap.height() as u32;
        let rgba = bitmap.to_rgba();

        // Convert RGBA to RGB for tesseract
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, rgba).ok_or("failed to create image buffer")?;
        let rgb_img = image::DynamicImage::ImageRgba8(img).to_rgb8();
        let rgb_bytes = rgb_img.into_raw();

        rendered.push(RenderedPage {
            page_index,
            page_number: page.page_number,
            rgb_bytes,
            width,
            height,
        });
    }

    if rendered.is_empty() {
        return Ok(());
    }

    eprintln!(
        "[ocr] running OCR on {} pages with {} workers",
        rendered.len(),
        num_workers
    );

    // Phase 2: Run OCR in parallel using rayon thread pool
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_workers)
        .build()
        .map_err(|e| format!("failed to create rayon thread pool: {}", e))?;

    let ocr_results: Vec<(usize, usize, Vec<crate::ocr::OcrResult>)> = pool.install(|| {
        rendered
            .par_iter()
            .filter_map(|rp| {
                let options = OcrOptions {
                    language: ocr_language.to_string(),
                };
                match ocr_engine.recognize(&rp.rgb_bytes, rp.width, rp.height, &options) {
                    Ok(results) => Some((rp.page_index, rp.page_number, results)),
                    Err(e) => {
                        eprintln!("[ocr] failed for page {}: {}", rp.page_number, e);
                        None
                    }
                }
            })
            .collect()
    });

    // Phase 3: Merge OCR results back into pages (sequential)
    let scale_factor = 72.0 / dpi;

    for (page_index, page_number, results) in ocr_results {
        if results.is_empty() {
            continue;
        }

        let page = &mut pages[page_index];
        let mut added = 0;

        for r in &results {
            if r.confidence <= 0.1 {
                continue;
            }

            let ocr_x = r.bbox[0] * scale_factor;
            let ocr_y = r.bbox[1] * scale_factor;
            let ocr_w = (r.bbox[2] - r.bbox[0]) * scale_factor;
            let ocr_h = (r.bbox[3] - r.bbox[1]) * scale_factor;

            if overlaps_existing_text(&page.text_items, ocr_x, ocr_y, ocr_w, ocr_h, 2.0) {
                continue;
            }

            let cleaned = clean_ocr_table_artifacts(&r.text);
            if cleaned.is_empty() {
                continue;
            }

            page.text_items.push(TextItem {
                text: cleaned,
                x: ocr_x,
                y: ocr_y,
                width: ocr_w,
                height: ocr_h,
                rotation: 0.0,
                font_name: Some("OCR".to_string()),
                font_size: Some(ocr_h),
                font_height: None,
                font_ascent: None,
                font_descent: None,
                font_weight: None,
                font_flags: None,
                text_width: None,
                font_is_buggy: false,
                mcid: None,
                fill_color: None,
                stroke_color: None,
                confidence: Some((r.confidence * 1000.0).round() / 1000.0),
            });
            added += 1;
        }

        if added > 0 {
            eprintln!(
                "[ocr] added {} text items from OCR on page {}",
                added, page_number
            );
        }
    }

    Ok(())
}

/// Check if an OCR bounding box overlaps with any existing text item.
fn overlaps_existing_text(
    items: &[TextItem],
    ocr_x: f32,
    ocr_y: f32,
    ocr_w: f32,
    ocr_h: f32,
    tolerance: f32,
) -> bool {
    for item in items {
        let item_right = item.x + item.width;
        let item_bottom = item.y + item.height;

        let overlap_x = ocr_x < item_right + tolerance && ocr_x + ocr_w > item.x - tolerance;
        let overlap_y = ocr_y < item_bottom + tolerance && ocr_y + ocr_h > item.y - tolerance;

        if overlap_x && overlap_y {
            return true;
        }
    }
    false
}

/// Clean common OCR artifacts from table border misreads.
/// OCR often misreads vertical table border lines as bracket-like characters.
fn clean_ocr_table_artifacts(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    // Strip leading/trailing border artifact characters: | [ ] ( ) { }
    let without_artifacts: &str = trimmed
        .trim_start_matches(['|', '[', ']', '(', ')', '{', '}'])
        .trim_end_matches(['|', '[', ']', '(', ')', '{', '}'])
        .trim();

    if without_artifacts.is_empty() {
        return trimmed.to_string();
    }

    // Only use cleaned version if core content looks numeric-ish
    // This avoids incorrectly stripping brackets from content like "(note)"
    let is_numeric_ish = without_artifacts
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | ' ' | '%' | '-' | '+' | '*' | '/'))
        || without_artifacts == "N/A"
        || without_artifacts == "Z"
        || without_artifacts == "-";

    if is_numeric_ish {
        without_artifacts.to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_ocr_table_artifacts() {
        assert_eq!(clean_ocr_table_artifacts("44520]"), "44520");
        assert_eq!(clean_ocr_table_artifacts("|123"), "123");
        assert_eq!(clean_ocr_table_artifacts("0.3|"), "0.3");
        assert_eq!(clean_ocr_table_artifacts("(note)"), "(note)");
        assert_eq!(clean_ocr_table_artifacts("|hello|"), "|hello|");
        assert_eq!(clean_ocr_table_artifacts("N/A"), "N/A");
        assert_eq!(clean_ocr_table_artifacts(""), "");
        assert_eq!(clean_ocr_table_artifacts("|||"), "|||");
    }
}
