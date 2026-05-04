# src/engines/pdf/

PDF parsing engines for loading documents and extracting content.

## Files

### interface.ts
**Defines the PdfEngine contract and data types.**

**PdfEngine Interface:**
```typescript
interface PdfEngine {
  name: string;
  loadDocument(input: string | Uint8Array): Promise<PdfDocument>;
  extractPage(doc: PdfDocument, pageNum: number): Promise<PageData>;
  extractAllPages(doc, maxPages?, targetPages?): Promise<PageData[]>;
  renderPageImage(doc, pageNum, dpi): Promise<Buffer>;
  close(doc: PdfDocument): Promise<void>;
}
```

`loadDocument` accepts either a file path or raw PDF bytes as a `Uint8Array`. When given bytes, the document is loaded with zero disk I/O.

**Key Data Types:**
- `PdfDocument` - Loaded document with `numPages`, `data` (Uint8Array), `metadata`
- `PageData` - Extracted page: `pageNum`, `width`, `height`, `textItems`, `images`, `annotations`
- `Image` - Embedded image with position, dimensions, and optional OCR data
- `Annotation` - PDF annotation (links, etc.)

---

### pdfjs.ts
**Default PDF engine using Mozilla's PDF.js library.**

**Key Responsibilities:**
- Load PDF documents with CMap and font support
- Extract text items with precise coordinates and font information
- Handle coordinate transformations (PDF space → viewport space)
- Parse `targetPages` syntax (e.g., "1-5,10,15-20")
- Delegate screenshot rendering to PDFium for quality

**Coordinate Transformation:**
PDF uses a bottom-left origin with Y pointing up. PDF.js provides transformation matrices that this code processes using:
- `multiplyMatrices()` - Combine viewport and item transforms
- `applyTransformation()` - Apply matrix to points
- `singularValueDecompose2dScale()` - Extract scale factors via SVD
- `getRotation()` - Extract rotation angle from matrix

**Text Decoding:**
Handles special PDF.js markers for problematic ("buggy") fonts — fonts whose ToUnicode/encoding maps glyphs to control characters or Private Use Area code points:
- Our patched PDF.js emits markers in the format `:->|>_<glyphId>_<fontCharCode>@<glyphName>@<|<-:` for buggy font glyphs
- The glyph name comes from the font's `/Differences` or `/Encoding` dictionary
- `decodeBuggyFontMarkers()` resolves glyph names to Unicode via the `ADOBE_GLYPH_MAP` (a subset of the Adobe Glyph List)
- Falls back to ASCII char code for glyphs in range 32-126
- Handles underscore-separated composite glyph names (e.g., `f_i` → "fi")
- Handles `uniXXXX` glyph name convention
- Also handles pipe-separated characters: `|a| |b| |c|` → `abc`
- `stripControlChars()` maps Windows-1252 C1 range (0x80-0x9F) to proper Unicode and decomposes Unicode ligatures (U+FB00-FB06) to plain text

**Garbled Font Detection:**
Some PDFs have fonts with corrupted or missing ToUnicode mappings, causing PDF.js to output characters mapped to unexpected Unicode code points. The `isGarbledFontOutput()` function detects this by identifying:

| Pattern | Unicode Range | Indicator |
|---------|---------------|-----------|
| Private Use Area | U+E000-U+F8FF | Fonts map unmapped glyphs here |
| Arabic + Latin Extended mix | U+0600-U+08FF + U+0100-U+1EFF | Script mixing in English text |
| Rare scripts | Syriac, Thaana, NKo, Samaritan | U+0700-U+083F |
| Specials | U+FFF0-U+FFFF | Replacement chars, invalid markers |
| Box Drawing/Shapes | U+2500-U+25FF | Shouldn't appear in running text |

When garbled text is detected:
1. The text item is filtered out
2. Its bounding box is saved to `PageData.garbledTextRegions`
3. OCR runs on the page, but only OCR results overlapping these regions are used
4. Spatial deduplication prevents OCR from overwriting good PDF text

This allows targeted OCR replacement of only the corrupted text while preserving high-quality PDF text extraction elsewhere.

**Design Decisions:**
- **Stores PDF path and data**: Keeps both the file path (when available) and the raw `Uint8Array` data for PDFium rendering. Buffer input skips file reads entirely.
- **Filters off-page items**: Removes text with negative coords or beyond page bounds
- **Zero-size filtering**: Skips text items with 0 width/height

---

### pdfium-renderer.ts
**High-quality screenshot renderer using native PDFium library.**

Used for generating page images for OCR and the `screenshot` command. Provides better quality than PDF.js canvas rendering, especially for documents with inline images.

**Key Features:**
- Native C++ PDFium via `@hyzyla/pdfium` WASM binding
- Sharp for image processing (PNG output with configurable compression)
- DPI-based scaling (72 DPI baseline)
- Lazy initialization (only loads when first needed)

**Methods:**
- `init()` - Initialize PDFium library (called automatically)
- `renderPageToBuffer(pdfInput, pageNumber, dpi)` - Render page to PNG buffer. Accepts a file path (`string`), `Buffer`, or `Uint8Array`.
- `close()` - Cleanup PDFium resources

**Design Decision:**
Separate from PdfJsEngine because:
1. PDFium renders better quality images (important for OCR accuracy)
2. PDF.js is better for text extraction (mature parsing)
3. Separation allows independent optimization of each concern
