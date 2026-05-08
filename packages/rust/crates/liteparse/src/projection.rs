use std::collections::{BTreeMap, HashMap, HashSet};
use crate::types::*;

const FLOATING_SPACES: usize = 2;
const COLUMN_SPACES: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SnapKind {
    Left,
    Right,
    Center,
}

fn compute_median_textbox_size(items: &[ProjectedTextItem]) -> (f32, f32) {
    if items.is_empty() {
        return (0.0, 0.0);
    }

    // Match TS behavior: median width is computed as average character width.
    let mut widths: Vec<f32> = items
        .iter()
        .filter_map(|item| {
            if item.item.width <= 0.0 {
                return None;
            }
            let char_len = item.item.text.chars().count();
            if char_len == 0 {
                return None;
            }
            Some(item.item.width / char_len as f32)
        })
        .collect();
    let mut heights: Vec<f32> = items
        .iter()
        .filter_map(|item| {
            if item.item.height > 0.0 {
                Some(item.item.height)
            } else {
                None
            }
        })
        .collect();

    if widths.is_empty() {
        widths.push(1.0);
    }
    if heights.is_empty() {
        heights.push(1.0);
    }

    widths.sort_by(|a, b| a.total_cmp(b));
    heights.sort_by(|a, b| a.total_cmp(b));
    
    let mid = widths.len() / 2;
    
    let median_width = if widths.len().is_multiple_of(2) {
        (widths[mid - 1] + widths[mid]) / 2.0
    } else {
        widths[mid]
    };
    
    let median_height = if heights.len().is_multiple_of(2) {
        (heights[mid - 1] + heights[mid]) / 2.0
    } else {
        heights[mid]
    };

    (median_width, median_height)
}

fn canonical_rotation(rotation: f32) -> i32 {
    let mut r = rotation;
    while r < 0.0 {
        r += 360.0;
    }
    while r >= 360.0 {
        r -= 360.0;
    }

    let candidates = [0.0f32, 90.0, 180.0, 270.0];
    let mut best = 0.0f32;
    let mut best_delta = f32::INFINITY;
    for c in candidates {
        let delta = (r - c).abs();
        if delta < best_delta {
            best_delta = delta;
            best = c;
        }
    }

    if best_delta <= 2.0 {
        best as i32
    } else {
        r.round() as i32
    }
}

fn handle_rotation_reading_order(items: &mut [ProjectedTextItem], page_height: f32) {
    if !items.iter().any(|b| canonical_rotation(b.item.rotation) != 0) {
        return;
    }

    // Group all items by rotation value.
    let mut groups_by_rotation: HashMap<i32, Vec<usize>> = HashMap::new();
    for (idx, bbox) in items.iter().enumerate() {
        let r = canonical_rotation(bbox.item.rotation);
        groups_by_rotation.entry(r).or_default().push(idx);
    }

    // Build group list sorted by each group's minimum X.
    let mut bbox_groups: Vec<Vec<usize>> = groups_by_rotation.into_values().collect();
    for group in &mut bbox_groups {
        group.sort_by(|a, b| {
            items[*a]
                .item
                .y
                .total_cmp(&items[*b].item.y)
        });
    }

    bbox_groups.sort_by(|a, b| {
        let min_x_a = a
            .iter()
            .map(|idx| items[*idx].item.x)
            .fold(f32::INFINITY, |acc, v| acc.min(v));
        let min_x_b = b
            .iter()
            .map(|idx| items[*idx].item.x)
            .fold(f32::INFINITY, |acc, v| acc.min(v));
        min_x_a.total_cmp(&min_x_b)
    });

    for group_idx in 0..bbox_groups.len() {
        let group = bbox_groups[group_idx].clone();
        if group.is_empty() {
            continue;
        }

        let group_rotation = canonical_rotation(items[group[0]].item.rotation);
        if group_rotation != 90 && group_rotation != 270 {
            continue;
        }

        // Check if non-rotated/other-rotated items visually overlap this group (X and Y overlap).
        let mut global_overlap = false;
        'outer: for (other_idx, other_bbox) in items.iter().enumerate() {
            let other_rot = canonical_rotation(other_bbox.item.rotation);
            if other_rot == group_rotation {
                continue;
            }

            for group_item_idx in &group {
                if *group_item_idx == other_idx {
                    continue;
                }
                let b = &items[*group_item_idx].item;
                let x_overlap = b.x >= other_bbox.item.x && b.x <= other_bbox.item.x + other_bbox.item.width;
                let y_overlap = b.y < other_bbox.item.y + other_bbox.item.height
                    && b.y + b.height > other_bbox.item.y;
                if x_overlap && y_overlap {
                    global_overlap = true;
                    break 'outer;
                }
            }
        }

        if global_overlap {
            for idx in &group {
                if items[*idx].d != 0.0 {
                    items[*idx].item.y += items[*idx].d;
                    items[*idx].d = 0.0;
                }
                items[*idx].item.rotation = 0.0;
                items[*idx].rotated = true;
            }
        } else {
            let group_max_x = group
                .iter()
                .map(|idx| items[*idx].item.x + items[*idx].item.width)
                .fold(f32::NEG_INFINITY, |acc, v| acc.max(v));

            let mut delta_y = 0.0f32;
            if group_idx != 0 {
                let previous_group = &bbox_groups[group_idx - 1];
                let previous_group_max_y = previous_group
                    .iter()
                    .map(|idx| items[*idx].item.y + items[*idx].item.height)
                    .fold(f32::NEG_INFINITY, |acc, v| acc.max(v));
                delta_y = previous_group_max_y + page_height;
            }

            if group_rotation == 90 {
                for idx in &group {
                    let new_x = items[*idx].item.y.round();
                    let new_y = items[*idx].item.x + delta_y;
                    let new_w = items[*idx].item.height;
                    let new_h = items[*idx].item.width;
                    items[*idx].item.x = new_x;
                    items[*idx].item.y = new_y;
                    items[*idx].item.width = new_w;
                    items[*idx].item.height = new_h;
                    items[*idx].item.rotation = 0.0;
                    items[*idx].rotated = true;
                }
            }

            if group_rotation == 270 {
                let max_y = group
                    .iter()
                    .map(|idx| items[*idx].item.y + items[*idx].item.height)
                    .fold(f32::NEG_INFINITY, |acc, v| acc.max(v));
                for idx in &group {
                    let new_x = (max_y - items[*idx].item.y - items[*idx].item.height).round();
                    let new_y = items[*idx].item.x + delta_y;
                    let new_w = items[*idx].item.height;
                    let new_h = items[*idx].item.width;
                    items[*idx].item.x = new_x;
                    items[*idx].item.y = new_y;
                    items[*idx].item.width = new_w;
                    items[*idx].item.height = new_h;
                    items[*idx].item.rotation = 0.0;
                    items[*idx].rotated = true;
                }
            }

            let global_delta = delta_y + group_max_x + page_height;
            for other_group_idx in (group_idx + 1)..bbox_groups.len() {
                for idx in &bbox_groups[other_group_idx] {
                    let rot = canonical_rotation(items[*idx].item.rotation);
                    if rot == 90 || rot == 270 {
                        items[*idx].d += global_delta;
                    } else {
                        items[*idx].item.y += global_delta;
                    }
                }
            }
        }
    }

    // Handle 180-degree rotation conservatively.
    // Unlike TS, we don't have extractor-provided rx/ry fields, so normalize to unrotated
    // and preserve local ordering by x.
    for group in &bbox_groups {
        if group.is_empty() {
            continue;
        }
        let rotation = canonical_rotation(items[group[0]].item.rotation);
        if rotation == 180 {
            let mut sorted = group.clone();
            sorted.sort_by(|a, b| {
                items[*a]
                    .item
                    .x
                    .total_cmp(&items[*b].item.x)
            });
            for idx in sorted {
                items[idx].item.rotation = 0.0;
                items[idx].rotated = true;
            }
        }
    }

    items.sort_by(|a, b| a.item.y.total_cmp(&b.item.y));
}

fn clean_projected_items(items: &mut Vec<ProjectedTextItem>, page_width: f32) {
    // Rust equivalent of cleanRawText margin cleanup.
    // Keep this conservative: only remove likely margin line numbers when they appear isolated.
    let midpoint = page_width * 0.5;
    let margin_left = midpoint - 5.0;
    let margin_right = midpoint + 20.0;

    let mut has_non_margin_by_line: HashMap<i32, bool> = HashMap::new();
    for item in items.iter() {
        let line_key = item.item.y.round() as i32;
        if !item.is_margin_line_number {
            has_non_margin_by_line.insert(line_key, true);
        }
    }

    items.retain(|item| {
        let line_key = item.item.y.round() as i32;
        let line_has_content = has_non_margin_by_line.get(&line_key).copied().unwrap_or(false);
        let center = item.item.x + item.item.width * 0.5;
        let text = item.item.text.trim();
        let looks_like_line_number = {
            let chars: Vec<char> = text.chars().collect();
            if chars.is_empty() || chars.len() > 3 {
                false
            } else {
                let mut digit_count = 0usize;
                let mut valid = true;
                for (idx, c) in chars.iter().enumerate() {
                    if c.is_ascii_digit() {
                        digit_count += 1;
                    } else if *c == 'O' && idx == chars.len() - 1 {
                        // OCR confusion 0->O
                    } else {
                        valid = false;
                        break;
                    }
                }
                valid && (1..=2).contains(&digit_count)
            }
        };

        let likely_margin = item.is_margin_line_number
            || (center > margin_left
                && center < margin_right
                && looks_like_line_number
                && item.item.width < 15.0);

        !(likely_margin && !line_has_content)
    });
}

fn form_lines(
    items: &mut Vec<ProjectedTextItem>,
    median_width: f32,
    median_height: f32,
    page_width: f32,
) -> Vec<Vec<ProjectedTextItem>> {
    // Y-tolerance for sorting: items within this threshold are considered same line
    // This handles:
    // 1. Floating point precision issues between columns (e.g., 334.7400 vs 334.7399)
    // 2. Subscripts/superscripts which are typically offset by 3-5 units from their base characters
    // Using a fraction of medianHeight to scale with document font size.
    let y_sort_tolerance: f32 = (median_height * 0.5).max(5.0);
    
    // Note: We keep whitespace items as they may be needed for proper word separation.
    // The spacing calculation handles gaps between items.

    // For two-column documents, detect and mark margin line numbers
    // These are short numeric items positioned between columns (near the page midpoint)
    // They should not be merged with column content
    if page_width > 0.0 {
        let midpoint = page_width / 2.0;
        let margin_left = midpoint - 5.0;
        let margin_right = midpoint + 20.0;

        fn is_margin_line_number_text(text: &str) -> bool {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return false;
            }
            let chars: Vec<char> = trimmed.chars().collect();
            if chars.len() > 3 {
                return false;
            }

            let mut digit_count = 0usize;
            for (idx, c) in chars.iter().enumerate() {
                if c.is_ascii_digit() {
                    digit_count += 1;
                } else if *c == 'O' && idx == chars.len() - 1 {
                    // OCR confusion: 0 -> O
                } else {
                    return false;
                }
            }
            (1..=2).contains(&digit_count)
        }

        for item in items.iter_mut() {
            let center = item.item.x + item.item.width / 2.0;

            // Check if item is in the margin zone and looks like a number
            if center > margin_left &&
               center < margin_right &&
               is_margin_line_number_text(&item.item.text) &&
               item.item.width < 15.0 
            {
                item.is_margin_line_number = true;
            }
        }
    }

    // Sort by y then x. Snap y to a grid so items on the same visual line
    // get identical keys — this keeps the comparator transitive (total order),
    // which Rust's sort requires.
    let snap_y = |y: f32| -> i64 {
        if y_sort_tolerance > 0.0 {
            (y / y_sort_tolerance).round() as i64
        } else {
            (y * 1000.0).round() as i64
        }
    };
    items.sort_by(|a, b| {
        let ya = snap_y(a.item.y);
        let yb = snap_y(b.item.y);
        ya.cmp(&yb).then_with(|| a.item.x.total_cmp(&b.item.x))
    });

    fn can_merge(prev: &ProjectedTextItem, cur: &ProjectedTextItem, y_tolerance: f32, h_tolerance: f32) -> bool {
        if (cur.item.y - prev.item.y).abs() <= y_tolerance &&
           (cur.item.height - prev.item.height).abs() <= h_tolerance {
            let delta_x = cur.item.x - (prev.item.x + prev.item.width);
            return (-0.5..0.0).contains(&delta_x) || (0.0..0.1).contains(&delta_x);
        }

        false
    }

    fn merge_bbox(prev: &ProjectedTextItem, cur: &ProjectedTextItem) -> (f32, f32, f32, f32) {
        let x1 = prev.item.x.min(cur.item.x);
        let y1 = prev.item.y.min(cur.item.y);
        let x2 = (prev.item.x + prev.item.width).max(cur.item.x + cur.item.width);
        let y2 = (prev.item.y + prev.item.height).max(cur.item.y + cur.item.height);
        (x1, y1, x2 - x1, y2 - y1)
    }

    // Merge continuous bbox items in a single linear pass.
    // This avoids repeated middle removals (O(n^2) in worst case) on large OCR outputs.
    let merge_y_tolerance = 0.1;
    let merge_h_tolerance = 0.1;

    let mut merged_items: Vec<ProjectedTextItem> = Vec::with_capacity(items.len());
    for cur in items.drain(..) {
        let should_merge = merged_items
            .last()
            .map(|prev| can_merge(prev, &cur, merge_y_tolerance, merge_h_tolerance))
            .unwrap_or(false);

        if should_merge {
            if let Some(prev) = merged_items.last_mut() {
                let merged = merge_bbox(prev, &cur);
                prev.item.text.push_str(&cur.item.text);
                prev.item.x = merged.0;
                prev.item.y = merged.1;
                prev.item.width = merged.2;
                prev.item.height = merged.3;
            }
        } else {
            merged_items.push(cur);
        }
    }

    *items = merged_items;

    // try to find the bounding box that forms a line and group items by line
    let mut lines: Vec<Vec<ProjectedTextItem>> = Vec::new();
    let mut current_line: Vec<ProjectedTextItem> = Vec::new();
    let mut current_line_min_y = f32::INFINITY;
    let mut current_line_max_y = f32::NEG_INFINITY;
    for item in items.drain(..) {
        if !current_line.is_empty() {
            let mut line_collide = false;
            for line_item in current_line.iter() {
                let overlap_length = (line_item.item.x + line_item.item.width).min(item.item.x + item.item.width) - line_item.item.x.max(item.item.x);

                // Use a minimum threshold to tolerate small overalps common in PDFs due to:
                // - character spacing and kerning
                // - floating point precision issues in text extraction
                // - adjacent items with slightly overlapping boxes
                // We want to detect true collisions, not adjacent text
                if overlap_length > f32::max(5.0, median_width / 3.0) {
                    line_collide = true;
                    break;
                }
            }

            // Don't merge margin line numbers with regular content
            let cur_line_has_margin = current_line.iter().any(|i| i.is_margin_line_number);
            let cur_item_has_margin = item.is_margin_line_number;
            let margin_mismatch = cur_line_has_margin != cur_item_has_margin;

            // For rotated text, use y-tolerance based merging since heights may be inconsistent
            let y_tolerance_merge = if item.rotated {
                (median_height * 2.0).max(20.0)
            } else {
                0.0
            };
            let y_within_tolerance =
                item.rotated && (item.item.y - current_line_min_y).abs() < y_tolerance_merge;

            if !line_collide && !margin_mismatch && 
                (
                    y_within_tolerance || 
                    (item.item.y + item.item.height * 0.5 >= current_line_min_y && item.item.y + item.item.height * 0.5 <= current_line_max_y) || 
                    (item.item.y >= current_line_min_y && item.item.y <= current_line_max_y)
                ) 
            {
                current_line_min_y = current_line_min_y.min(item.item.y);
                current_line_max_y = current_line_max_y.max(item.item.y + item.item.height);
                current_line.push(item);
            } else {
                lines.push(std::mem::take(&mut current_line));
                current_line_min_y = item.item.y;
                current_line_max_y = item.item.y + item.item.height;
                current_line.push(item);
            }
        } else {
            current_line_min_y = item.item.y;
            current_line_max_y = item.item.y + item.item.height;
            current_line.push(item);
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    // sort each line by x
    for line in lines.iter_mut() {
        line.sort_by(|a, b| a.item.x.total_cmp(&b.item.x));
    }

    // sort lines by y
    lines.sort_by(|a, b| {
        let ay = a.first().map(|v| v.item.y).unwrap_or(f32::INFINITY);
        let by = b.first().map(|v| v.item.y).unwrap_or(f32::INFINITY);
        ay.total_cmp(&by)
    });

    // merge 'words'
    const MERGE_THRESHOLD: f32 = 1.0;

    // Pattern to detect standalone numeric values.
    // Matches: numbers with optional commas, decimal points, dollar signs, percentages, negatives.
    fn looks_like_table_number(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.chars().count() < 2 {
            return false;
        }

        let mut chars = trimmed.chars().peekable();
        if matches!(chars.peek(), Some('$')) {
            chars.next();
        }
        if matches!(chars.peek(), Some('-')) {
            chars.next();
        }

        let mut has_digit = false;
        let mut has_decimal = false;
        for c in chars {
            if c.is_ascii_digit() {
                has_digit = true;
            } else if c == ',' {
                continue;
            } else if c == '.' {
                if has_decimal {
                    return false;
                }
                has_decimal = true;
            } else if c == '%' {
                // Percent is only valid as trailing char.
                return has_digit && trimmed.ends_with('%');
            } else {
                return false;
            }
        }

        has_digit
    }

    for line in lines.iter_mut() {
        let mut merged_line: Vec<ProjectedTextItem> = Vec::with_capacity(line.len());
        for item in line.drain(..) {
            if let Some(prev) = merged_line.last_mut() {
                let both_are_numbers = looks_like_table_number(&prev.item.text)
                    && looks_like_table_number(&item.item.text);

                let delta_x = item.item.x - prev.item.x - prev.item.width;

                if !both_are_numbers && delta_x <= MERGE_THRESHOLD {
                    prev.item.width = item.item.x + item.item.width - prev.item.x;
                    prev.item.text.push_str(&item.item.text);
                    continue;
                }

                let prev_len = prev.item.text.chars().count().max(1) as f32;
                let avg_char_width = prev.item.width / prev_len;
                if !both_are_numbers && delta_x < avg_char_width {
                    prev.item.width = item.item.x + item.item.width - prev.item.x;
                    if !prev.item.text.ends_with(' ') {
                        prev.item.text.push(' ');
                    }
                    prev.item.text.push_str(&item.item.text);
                    continue;
                }
            }

            merged_line.push(item);
        }

        *line = merged_line;
    }

    // Merge overlapping lines when there is no horizontal bbox overlap.
    let mut i = 1usize;
    while i < lines.len() {
        let (previous_min_y, previous_max_y) = {
            let previous = &lines[i - 1];
            let min_y = previous
                .iter()
                .map(|v| v.item.y)
                .fold(f32::INFINITY, |a, b| a.min(b));
            let max_y = previous
                .iter()
                .map(|v| v.item.y + v.item.height)
                .fold(f32::NEG_INFINITY, |a, b| a.max(b));
            (min_y, max_y)
        };

        let (current_min_y, current_max_y) = {
            let current = &lines[i];
            let min_y = current
                .iter()
                .map(|v| v.item.y)
                .fold(f32::INFINITY, |a, b| a.min(b));
            let max_y = current
                .iter()
                .map(|v| v.item.y + v.item.height)
                .fold(f32::NEG_INFINITY, |a, b| a.max(b));
            (min_y, max_y)
        };

        // Do the two lines overlap vertically?
        let lines_overlap = previous_max_y > current_min_y && previous_min_y < current_max_y;

        if lines_overlap {
            let bbox_overlap = {
                let previous = &lines[i - 1];
                let current = &lines[i];
                current.iter().any(|bbox| {
                    previous.iter().any(|prev_bbox| {
                        (bbox.item.x >= prev_bbox.item.x
                            && bbox.item.x <= prev_bbox.item.x + prev_bbox.item.width)
                            || (prev_bbox.item.x >= bbox.item.x
                                && prev_bbox.item.x <= bbox.item.x + bbox.item.width)
                    })
                })
            };

            if !bbox_overlap {
                let mut current = lines.remove(i);
                lines[i - 1].append(&mut current);
                lines[i - 1].sort_by(|a, b| a.item.x.total_cmp(&b.item.x));
                continue;
            }
        }

        i += 1;
    }

    // Insert blank lines for vertical gaps between lines.
    // Uses the same approach as TS: if the gap between two lines exceeds
    // a reference height, insert proportional blank lines.
    let mut i = 1;
    while i < lines.len() {
        let prev_metrics = representative_line_metrics(&lines[i - 1], median_height);
        let cur_metrics = representative_line_metrics(&lines[i], median_height);
        let y_delta = cur_metrics.0 - prev_metrics.1; // cur_top - prev_bottom
        let reference_height = median_height.max(prev_metrics.2.min(cur_metrics.2));

        if y_delta > reference_height {
            let num_blank = ((y_delta / reference_height).round() as usize).saturating_sub(1);
            let to_insert = num_blank.clamp(1, 10);
            for _ in 0..to_insert {
                lines.insert(i, Vec::new());
                i += 1;
            }
        }
        i += 1;
    }

    lines
}

/// Returns (top, bottom, height) for representative items in a line,
/// filtering out items much shorter than median height.
fn representative_line_metrics(
    line: &[ProjectedTextItem],
    global_median_height: f32,
) -> (f32, f32, f32) {
    if line.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let min_representative = global_median_height * 0.5;
    let has_representative = line.iter().any(|b| b.item.height >= min_representative);

    let top = line
        .iter()
        .filter(|b| !has_representative || b.item.height >= min_representative)
        .map(|b| b.item.y)
        .fold(f32::INFINITY, f32::min);
    let bottom = line
        .iter()
        .filter(|b| !has_representative || b.item.height >= min_representative)
        .map(|b| b.item.y + b.item.height)
        .fold(f32::NEG_INFINITY, f32::max);
    (top, bottom, bottom - top)
}

#[derive(Clone, Debug, Default)]
struct BoxMeta {
    left_anchor: Option<i32>,
    right_anchor: Option<i32>,
    center_anchor: Option<i32>,
    snap: Option<SnapKind>,
    should_space: usize,
    force_unsnapped: bool,
    rendered: bool,
    projected_x: usize,
}

fn anchor_key(x: f32) -> i32 {
    (x * 4.0).round() as i32
}

fn anchor_to_x(key: i32) -> f32 {
    key as f32 / 4.0
}

fn trim_end_len(s: &str) -> usize {
    s.trim_end().len()
}

fn trim_end_in_place(s: &mut String) {
    let n = trim_end_len(s);
    s.truncate(n);
}

fn line_space_end(raw_line: &str, should_space: usize) -> usize {
    let mut space_end = 0usize;
    if !raw_line.ends_with(' ') {
        space_end = should_space;
    }
    if should_space > 1 {
        let trailing_spaces = raw_line.len().saturating_sub(trim_end_len(raw_line));
        if trailing_spaces < should_space {
            space_end = should_space - trailing_spaces;
        }
    }
    space_end
}

fn can_render_bbox(meta_line: &[BoxMeta], idx: usize) -> bool {
    for m in meta_line.iter().take(idx) {
        if !m.rendered {
            return false;
        }
    }
    true
}

fn merge_nearby_anchor_groups(collection: &mut HashMap<i32, Vec<(usize, usize)>>) {
    const MERGE_TOLERANCE: i32 = 8; // 2 units in quarter-point anchor key space

    let sorted_keys: Vec<i32> = {
        let mut keys: Vec<i32> = collection.keys().copied().collect();
        keys.sort_unstable();
        keys
    };

    for (i, anchor) in sorted_keys.iter().enumerate() {
        if !collection.contains_key(anchor) {
            continue;
        }
        for next_anchor in sorted_keys.iter().skip(i + 1) {
            if !collection.contains_key(next_anchor) {
                continue;
            }
            if next_anchor - anchor > MERGE_TOLERANCE {
                break;
            }

            let current_len = collection.get(anchor).map(|v| v.len()).unwrap_or(0);
            let next_len = collection.get(next_anchor).map(|v| v.len()).unwrap_or(0);

            if next_len > current_len {
                if let Some(cur_items) = collection.remove(anchor) {
                    if let Some(next_items) = collection.get_mut(next_anchor) {
                        next_items.extend(cur_items);
                    }
                }
                break;
            } else if let Some(next_items) = collection.remove(next_anchor)
                && let Some(cur_items) = collection.get_mut(anchor)
            {
                cur_items.extend(next_items);
            }
        }
    }
}

fn update_forward_anchor_right_bound(
    snap_map: &[i32],
    forward_anchor: &mut BTreeMap<i32, usize>,
    right_bound: i32,
    anchor_target: usize,
) {
    const POSITION_TOLERANCE: i32 = 8; // 2 units in quarter-point anchor key space

    for (idx, anchor) in snap_map.iter().enumerate().rev() {
        if *anchor < right_bound {
            return;
        }

        let entry = forward_anchor.entry(*anchor).or_insert(0);
        if anchor_target > *entry {
            *entry = anchor_target;
        }

        let mut j = idx;
        while j > 0 {
            let nearby_anchor = snap_map[j - 1];
            if *anchor - nearby_anchor > POSITION_TOLERANCE {
                break;
            }
            let nearby_entry = forward_anchor.entry(nearby_anchor).or_insert(0);
            if anchor_target > *nearby_entry {
                *nearby_entry = anchor_target;
            }
            j -= 1;
        }
    }
}

fn compress_wide_spaces(line: &str, min_run: usize, replace_with: usize) -> String {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b' ' {
            let start = i;
            while i < bytes.len() && bytes[i] == b' ' {
                i += 1;
            }
            let run_len = i - start;
            if run_len >= min_run {
                out.push_str(&" ".repeat(replace_with));
            } else {
                out.push_str(&" ".repeat(run_len));
            }
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn fix_sparse_blocks(raw_lines: &mut [String], start: usize, end: usize) {
    let mut total = 0usize;
    let mut whitespace = 0usize;

    for line in raw_lines.iter_mut().take(end).skip(start) {
        trim_end_in_place(line);
        if line.is_empty() {
            continue;
        }
        total += line.len();
        whitespace += line.chars().filter(|c| c.is_whitespace()).count();
    }

    if total >= 500 && (whitespace as f32 / total as f32) > 0.8 {
        for line in raw_lines.iter_mut().take(end).skip(start) {
            if line.is_empty() {
                continue;
            }
            *line = compress_wide_spaces(line, COLUMN_SPACES, FLOATING_SPACES);
        }
    }
}

fn project_to_grid(page: &Page, mut projection_boxes: Vec<ProjectedTextItem>) -> (Vec<ProjectedTextItem>, String) {
    // Step 1a: Filter out items that are purely dots
    let mut dot_count = 0usize;
    projection_boxes.iter().for_each(|item| {
        if item.item.text.chars().all(|c| c == '.' || c == '·' || c == '•') {
            dot_count += 1;
        }
    });

    // If there are many dot-only items (likely a dotted ruler), remove them in-place.
    // Use a relative threshold (5%) and an absolute threshold (100) to avoid removing
    // dots on small pages.
    if dot_count > 100 && (dot_count as f64) > (projection_boxes.len() as f64) * 0.05 {
        projection_boxes.retain(|item| {
            !item.item.text.chars().all(|c| c == '.' || c == '·' || c == '•')
        });
    }

    // Step 1b: Round dimensions (matching TS buildBbox Math.round on w/h)
    for item in projection_boxes.iter_mut() {
        item.item.width = item.item.width.round();
        item.item.height = item.item.height.round();
    }

    // Step 1c: Compute median distances
    let (median_width, median_height) = compute_median_textbox_size(&projection_boxes);

    // Step 1c: Handle reading order rotations
    handle_rotation_reading_order(&mut projection_boxes, page.page_height);

    // Step 1d: Form lines of boxes
    let mut lines = form_lines(&mut projection_boxes, median_width, median_height, page.page_width);
    if lines.is_empty() {
        return (Vec::new(), String::new());
    }

    let mut meta: Vec<Vec<BoxMeta>> = lines
        .iter()
        .map(|line| vec![BoxMeta::default(); line.len()])
        .collect();

    // Anchor extraction
    let mut anchor_left: HashMap<i32, Vec<(usize, usize)>> = HashMap::new();
    let mut anchor_right: HashMap<i32, Vec<(usize, usize)>> = HashMap::new();
    let mut anchor_center: HashMap<i32, Vec<(usize, usize)>> = HashMap::new();

    for (line_idx, line) in lines.iter().enumerate() {
        for (box_idx, bbox) in line.iter().enumerate() {
            let left_key = anchor_key(bbox.item.x);
            let right_key = anchor_key(bbox.item.x + bbox.item.width);
            let center_key = anchor_key(bbox.item.x + bbox.item.width * 0.5);
            anchor_left.entry(left_key).or_default().push((line_idx, box_idx));
            anchor_right.entry(right_key).or_default().push((line_idx, box_idx));
            anchor_center.entry(center_key).or_default().push((line_idx, box_idx));
        }
    }

    merge_nearby_anchor_groups(&mut anchor_left);
    merge_nearby_anchor_groups(&mut anchor_right);
    merge_nearby_anchor_groups(&mut anchor_center);

    // Keep non-singletons only.
    anchor_left.retain(|_, v| v.len() >= 2);
    anchor_right.retain(|_, v| v.len() >= 2);
    anchor_center.retain(|_, v| v.len() >= 2);

    // Populate per-item anchor candidates.
    for (anchor, members) in &anchor_left {
        for (li, bi) in members {
            meta[*li][*bi].left_anchor = Some(*anchor);
        }
    }
    for (anchor, members) in &anchor_right {
        for (li, bi) in members {
            meta[*li][*bi].right_anchor = Some(*anchor);
        }
    }
    for (anchor, members) in &anchor_center {
        for (li, bi) in members {
            meta[*li][*bi].center_anchor = Some(*anchor);
        }
    }

    // Resolve duplicates by choosing strongest anchor (tie-break: left > right > center).
    for line_idx in 0..lines.len() {
        for box_idx in 0..lines[line_idx].len() {
            let left_count = meta[line_idx][box_idx]
                .left_anchor
                .and_then(|k| anchor_left.get(&k).map(|v| v.len()))
                .unwrap_or(0);
            let right_count = meta[line_idx][box_idx]
                .right_anchor
                .and_then(|k| anchor_right.get(&k).map(|v| v.len()))
                .unwrap_or(0);
            let center_count = meta[line_idx][box_idx]
                .center_anchor
                .and_then(|k| anchor_center.get(&k).map(|v| v.len()))
                .unwrap_or(0);

            if left_count == 0 && right_count == 0 && center_count == 0 {
                continue;
            }

            let kind = if left_count >= right_count && left_count >= center_count {
                SnapKind::Left
            } else if right_count >= left_count && right_count >= center_count {
                SnapKind::Right
            } else {
                SnapKind::Center
            };
            meta[line_idx][box_idx].snap = Some(kind);
        }
    }

    // Compute spacing hints between neighboring boxes (after snaps are assigned).
    for line_idx in 0..lines.len() {
        for box_idx in 0..lines[line_idx].len() {
            if box_idx == 0 {
                meta[line_idx][box_idx].should_space = 0;
                continue;
            }
            let prev = &lines[line_idx][box_idx - 1].item;
            let cur = &lines[line_idx][box_idx].item;
            let x_delta = cur.x - (prev.x + prev.width);

            let mut should_space = 0usize;
            if x_delta > 2.0 {
                should_space = 1;
                let prev_len = prev.text.chars().count().max(1) as f32;
                let prev_char_width = (prev.width / prev_len).max(0.1);
                if x_delta > prev_char_width * 2.0 {
                    let column_gap_threshold = page.page_width * 0.1;
                    let same_column = x_delta < column_gap_threshold;

                    let cur_snap = meta[line_idx][box_idx].snap;
                    let prev_snap = meta[line_idx][box_idx - 1].snap;
                    let cur_is_left_snap = cur_snap == Some(SnapKind::Left);
                    let prev_is_right_snap = prev_snap == Some(SnapKind::Right);
                    let both_snapped = cur_snap.is_some() && prev_snap.is_some();
                    let force_unsnapped = meta[line_idx][box_idx].force_unsnapped;

                    if (!force_unsnapped && x_delta > prev_char_width * 8.0)
                        || cur_is_left_snap
                        || prev_is_right_snap
                        || both_snapped
                    {
                        should_space = if same_column { FLOATING_SPACES } else { COLUMN_SPACES };
                    } else {
                        should_space = if same_column { 1 } else { FLOATING_SPACES };
                    }
                }
            }
            meta[line_idx][box_idx].should_space = should_space;
        }
    }

    let mut left_snaps: Vec<i32> = anchor_left.keys().copied().collect();
    let mut right_snaps: Vec<i32> = anchor_right.keys().copied().collect();
    let mut center_snaps: Vec<i32> = anchor_center.keys().copied().collect();
    left_snaps.sort_unstable();
    right_snaps.sort_unstable();
    center_snaps.sort_unstable();

    let mut floating_set: HashSet<i32> = HashSet::new();
    for (line_idx, line) in lines.iter().enumerate() {
        for (box_idx, bbox) in line.iter().enumerate() {
            if meta[line_idx][box_idx].snap.is_none() {
                floating_set.insert(anchor_key(bbox.item.x));
            }
        }
    }
    let mut floating_snaps: Vec<i32> = floating_set.into_iter().collect();
    floating_snaps.sort_unstable();

    let mut forward_left: BTreeMap<i32, usize> = BTreeMap::new();
    let mut forward_right: BTreeMap<i32, usize> = BTreeMap::new();
    let mut forward_center: BTreeMap<i32, usize> = BTreeMap::new();
    let mut forward_floating: BTreeMap<i32, usize> = BTreeMap::new();

    let mut raw_lines = vec![String::new(); lines.len()];

    let mut has_changed = true;
    while has_changed || !left_snaps.is_empty() || !right_snaps.is_empty() || !center_snaps.is_empty() {
        has_changed = false;

        // Render floating/unsnapped first when they are not blocked by earlier pending snap columns.
        for line_idx in 0..lines.len() {
            for box_idx in 0..lines[line_idx].len() {
                if meta[line_idx][box_idx].rendered {
                    continue;
                }

                if !meta[line_idx][box_idx].force_unsnapped {
                    if meta[line_idx][box_idx].snap.is_some() {
                        continue;
                    }

                    let x_key = anchor_key(lines[line_idx][box_idx].item.x);
                    let center_key = anchor_key(
                        lines[line_idx][box_idx].item.x + lines[line_idx][box_idx].item.width * 0.5,
                    );
                    if left_snaps.first().copied().is_some_and(|v| v < x_key)
                        || right_snaps.first().copied().is_some_and(|v| v < x_key)
                        || center_snaps.first().copied().is_some_and(|v| v < center_key)
                    {
                        continue;
                    }
                }

                if !can_render_bbox(&meta[line_idx], box_idx) {
                    break;
                }

                let (bbox_x, bbox_w, bbox_text) = {
                    let b = &lines[line_idx][box_idx].item;
                    (b.x, b.width, b.text.clone())
                };
                let mut target_x = ((bbox_x / median_width).round() as isize)
                    .max(0)
                    .min(COLUMN_SPACES as isize) as usize;

                let x_key = anchor_key(bbox_x);
                let last_snap_left = forward_left
                    .range(..=x_key)
                    .map(|(_, v)| *v)
                    .max()
                    .unwrap_or(0);

                let line_max = last_snap_left.max(trim_end_len(&raw_lines[line_idx]) + meta[line_idx][box_idx].should_space);
                if target_x < line_max {
                    target_x = line_max;
                }

                if !meta[line_idx][box_idx].force_unsnapped {
                    let floating_key = anchor_key(bbox_x);
                    if let Some(floating_anchor) = forward_floating.get(&floating_key).copied()
                        && target_x < floating_anchor
                    {
                        let adjusted = floating_anchor.min(target_x + 4);
                        if adjusted > target_x {
                            target_x = adjusted;
                        }
                    }
                }

                trim_end_in_place(&mut raw_lines[line_idx]);
                let before_len = raw_lines[line_idx].len();
                if target_x > before_len {
                    raw_lines[line_idx].push_str(&" ".repeat(target_x - before_len));
                }
                let start_x = raw_lines[line_idx].len();
                raw_lines[line_idx].push_str(&bbox_text);

                meta[line_idx][box_idx].rendered = true;
                meta[line_idx][box_idx].projected_x = start_x;
                lines[line_idx][box_idx].rendered = true;
                lines[line_idx][box_idx].num_spaces = start_x.saturating_sub(before_len);
                has_changed = true;

                let next_should_space = if box_idx + 1 < lines[line_idx].len() {
                    meta[line_idx][box_idx + 1].should_space
                } else {
                    0
                };
                let right_bound = anchor_key(bbox_x + bbox_w);
                let target_len = raw_lines[line_idx].len() + next_should_space;

                update_forward_anchor_right_bound(&left_snaps, &mut forward_left, right_bound, target_len);
                update_forward_anchor_right_bound(&right_snaps, &mut forward_right, right_bound, target_len);
                update_forward_anchor_right_bound(&floating_snaps, &mut forward_floating, right_bound, target_len);
            }
        }

        let left_first = left_snaps.first().copied();
        let right_first = right_snaps.first().copied();
        let center_first = center_snaps.first().copied();

        let next_kind = match (left_first, right_first, center_first) {
            (None, None, None) => None,
            (Some(_), None, None) => Some(SnapKind::Left),
            (None, Some(_), None) => Some(SnapKind::Right),
            (None, None, Some(_)) => Some(SnapKind::Center),
            (Some(l), Some(r), None) => Some(if l <= r { SnapKind::Left } else { SnapKind::Right }),
            (Some(l), None, Some(c)) => Some(if l <= c { SnapKind::Left } else { SnapKind::Center }),
            (None, Some(r), Some(c)) => Some(if r <= c { SnapKind::Right } else { SnapKind::Center }),
            (Some(l), Some(r), Some(c)) => {
                if l <= r && l <= c {
                    Some(SnapKind::Left)
                } else if r <= l && r <= c {
                    Some(SnapKind::Right)
                } else {
                    Some(SnapKind::Center)
                }
            }
        };

        let Some(kind) = next_kind else {
            continue;
        };

        let current_anchor = match kind {
            SnapKind::Left => left_snaps.first().copied(),
            SnapKind::Right => right_snaps.first().copied(),
            SnapKind::Center => center_snaps.first().copied(),
        };

        let Some(current_anchor) = current_anchor else {
            continue;
        };

        let mut turn_items: Vec<(usize, usize)> = Vec::new();
        for line_idx in 0..lines.len() {
            for box_idx in 0..lines[line_idx].len() {
                if meta[line_idx][box_idx].rendered {
                    continue;
                }
                let matches = match kind {
                    SnapKind::Left => meta[line_idx][box_idx].left_anchor == Some(current_anchor),
                    SnapKind::Right => meta[line_idx][box_idx].right_anchor == Some(current_anchor),
                    SnapKind::Center => meta[line_idx][box_idx].center_anchor == Some(current_anchor),
                };
                if matches {
                    turn_items.push((line_idx, box_idx));
                }
            }
        }

        if turn_items.is_empty() {
            match kind {
                SnapKind::Left => {
                    left_snaps.remove(0);
                }
                SnapKind::Right => {
                    right_snaps.remove(0);
                }
                SnapKind::Center => {
                    center_snaps.remove(0);
                }
            }
            continue;
        }

        has_changed = true;

        let mut target_x = ((anchor_to_x(current_anchor) / median_width).round() as isize)
            .max(0)
            .min(COLUMN_SPACES as isize) as usize;

        let line_max = match kind {
            SnapKind::Left => turn_items
                .iter()
                .map(|(li, bi)| raw_lines[*li].len() + line_space_end(&raw_lines[*li], meta[*li][*bi].should_space) + 1)
                .max()
                .unwrap_or(0),
            SnapKind::Right => turn_items
                .iter()
                .map(|(li, bi)| {
                    let bbox = &lines[*li][*bi].item;
                    let x_key = anchor_key(bbox.x);
                    let last_snap_left = forward_left
                        .range(..=x_key)
                        .map(|(_, v)| *v)
                        .max()
                        .unwrap_or(0);
                    let left_bound = last_snap_left.max(trim_end_len(&raw_lines[*li]) + meta[*li][*bi].should_space);
                    left_bound + bbox.text.chars().count()
                })
                .max()
                .unwrap_or(0),
            SnapKind::Center => turn_items
                .iter()
                .map(|(li, bi)| {
                    let text_half = lines[*li][*bi].item.text.chars().count() / 2;
                    raw_lines[*li].len() + text_half + line_space_end(&raw_lines[*li], meta[*li][*bi].should_space)
                })
                .max()
                .unwrap_or(0),
        };

        if target_x < line_max {
            target_x = line_max;
        }

        match kind {
            SnapKind::Left => {
                if let Some(v) = forward_left.get(&current_anchor).copied() {
                    target_x = target_x.max(v);
                }
                forward_left.insert(current_anchor, target_x);
            }
            SnapKind::Right => {
                if let Some(v) = forward_right.get(&current_anchor).copied() {
                    target_x = target_x.max(v);
                }
                forward_right.insert(current_anchor, target_x);
            }
            SnapKind::Center => {
                if let Some(v) = forward_center.get(&current_anchor).copied() {
                    target_x = target_x.max(v);
                }
                forward_center.insert(current_anchor, target_x);
            }
        }

        for (line_idx, box_idx) in turn_items {
            let (bbox_x, bbox_w, bbox_text) = {
                let b = &lines[line_idx][box_idx].item;
                (b.x, b.width, b.text.clone())
            };
            match kind {
                SnapKind::Left => {
                    let before = raw_lines[line_idx].len();
                    if target_x > before {
                        raw_lines[line_idx].push_str(&" ".repeat(target_x - before));
                    }
                    let start_x = raw_lines[line_idx].len();
                    raw_lines[line_idx].push_str(&bbox_text);
                    meta[line_idx][box_idx].projected_x = start_x;
                    lines[line_idx][box_idx].num_spaces = start_x.saturating_sub(before);
                }
                SnapKind::Right => {
                    trim_end_in_place(&mut raw_lines[line_idx]);
                    let text_len = bbox_text.chars().count();
                    let before = raw_lines[line_idx].len();
                    let trim_len = trim_end_len(&raw_lines[line_idx]);
                    if target_x > trim_len + text_len {
                        let pad = target_x - raw_lines[line_idx].len() - text_len;
                        raw_lines[line_idx].push_str(&" ".repeat(pad));
                    }
                    let start_x = raw_lines[line_idx].len();
                    raw_lines[line_idx].push_str(&bbox_text);
                    meta[line_idx][box_idx].projected_x = start_x;
                    lines[line_idx][box_idx].num_spaces = start_x.saturating_sub(before);
                }
                SnapKind::Center => {
                    let text_half = bbox_text.chars().count() / 2;
                    let before = raw_lines[line_idx].len();
                    if target_x > raw_lines[line_idx].len() + text_half {
                        let pad = target_x - raw_lines[line_idx].len() - text_half;
                        raw_lines[line_idx].push_str(&" ".repeat(pad));
                    }
                    let start_x = raw_lines[line_idx].len();
                    raw_lines[line_idx].push_str(&bbox_text);
                    meta[line_idx][box_idx].projected_x = start_x;
                    lines[line_idx][box_idx].num_spaces = start_x.saturating_sub(before);
                }
            }

            meta[line_idx][box_idx].rendered = true;
            lines[line_idx][box_idx].rendered = true;

            let next_should_space = if box_idx + 1 < lines[line_idx].len() {
                meta[line_idx][box_idx + 1].should_space
            } else {
                0
            };
            let right_bound = anchor_key(bbox_x + bbox_w);
            let target_len = raw_lines[line_idx].len() + next_should_space;
            update_forward_anchor_right_bound(&left_snaps, &mut forward_left, right_bound, target_len);
            update_forward_anchor_right_bound(&right_snaps, &mut forward_right, right_bound, target_len);
            update_forward_anchor_right_bound(&floating_snaps, &mut forward_floating, right_bound, target_len);
        }

        match kind {
            SnapKind::Left => {
                left_snaps.remove(0);
            }
            SnapKind::Right => {
                right_snaps.remove(0);
            }
            SnapKind::Center => {
                center_snaps.remove(0);
            }
        }
    }

    // Fallback: render anything still not rendered to avoid data loss.
    for line_idx in 0..lines.len() {
        for box_idx in 0..lines[line_idx].len() {
            if meta[line_idx][box_idx].rendered {
                continue;
            }
            if !raw_lines[line_idx].is_empty() && !raw_lines[line_idx].ends_with(' ') {
                raw_lines[line_idx].push(' ');
            }
            let start_x = raw_lines[line_idx].len();
            raw_lines[line_idx].push_str(&lines[line_idx][box_idx].item.text);
            meta[line_idx][box_idx].rendered = true;
            meta[line_idx][box_idx].projected_x = start_x;
            lines[line_idx][box_idx].rendered = true;
        }
    }

    let raw_line_count = raw_lines.len();
    fix_sparse_blocks(&mut raw_lines, 0, raw_line_count);

    // Persist projected positions and flatten in line order.
    let mut flattened: Vec<ProjectedTextItem> = Vec::with_capacity(lines.iter().map(|l| l.len()).sum());
    for (line_idx, line) in lines.into_iter().enumerate() {
        for (box_idx, mut item) in line.into_iter().enumerate() {
            item.item.x = meta[line_idx][box_idx].projected_x as f32;
            item.item.y = line_idx as f32;
            item.force_unsnapped = meta[line_idx][box_idx].force_unsnapped;
            item.num_spaces = meta[line_idx][box_idx].should_space;

            if let Some(snap) = meta[line_idx][box_idx].snap {
                match snap {
                    SnapKind::Left => {
                        item.snap = Snap::Left;
                        item.anchor = Anchor::Left;
                    }
                    SnapKind::Right => {
                        item.snap = Snap::Right;
                        item.anchor = Anchor::Right;
                    }
                    SnapKind::Center => {
                        item.snap = Snap::Center;
                        item.anchor = Anchor::Center;
                    }
                }
            }
            flattened.push(item);
        }
    }

    clean_projected_items(&mut flattened, page.page_width);

    let text = raw_lines.into_iter()
        .map(|l| l.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let text = clean_rendered_text(&text);

    (flattened, text)
}

/// Post-rendering text cleanup:
/// - Remove top margin (leading empty lines)
/// - Remove bottom margin (trailing empty lines)
/// - Remove left margin (consistent leading whitespace)
/// - Replace null characters with spaces
fn clean_rendered_text(text: &str) -> String {
    let text = text.replace('\0', " ");
    let lines: Vec<&str> = text.split('\n').collect();

    // Find bounds of content and minimum left indentation
    let mut min_x: Option<usize> = None;
    let mut min_y: Option<usize> = None;
    let mut max_y: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let leading = line.len() - line.trim_start().len();
        min_x = Some(min_x.map_or(leading, |m: usize| m.min(leading)));
        if min_y.is_none() {
            min_y = Some(i);
        }
        max_y = Some(i);
    }

    let (min_x, min_y, max_y) = match (min_x, min_y, max_y) {
        (Some(x), Some(y1), Some(y2)) => (x, y1, y2),
        _ => return String::new(),
    };

    lines[min_y..=max_y]
        .iter()
        .map(|line| {
            if line.len() > min_x {
                &line[min_x..]
            } else {
                line.trim_end()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn project_pages_to_grid(pages: Vec<Page>) -> Vec<ParsedPage> {
    pages.into_iter().map(|page| {
        let projection_boxes = page.text_items.iter().map(|item| {
            ProjectedTextItem {
                item: item.clone(),
                snap: Snap::Left,
                anchor: Anchor::Left,
                is_dup: false,
                rendered: false,
                num_spaces: 0,
                force_unsnapped: false,
                is_margin_line_number: false,
                rotated: false,
                d: 0.0,
            }
        }).collect();

        let (projected_items, text) = project_to_grid(&page, projection_boxes);
        ParsedPage {
            page_number: page.page_number,
            page_width: page.page_width,
            page_height: page.page_height,
            text,
            text_items: projected_items.into_iter().map(|proj| proj.item).collect(),
        }
    }).collect()
}