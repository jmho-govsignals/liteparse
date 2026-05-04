# src/conversion/

Multi-format document conversion to PDF using external tools.

## Files

### convertToPdf.ts
**Converts non-PDF documents to PDF for parsing.**

LiteParse's core parsing works on PDFs. This module extends support to 50+ formats by converting them to PDF first using system tools.

**Supported Formats:**

| Category | Extensions |
|----------|------------|
| Office | .doc, .docx, .docm, .dot, .dotm, .dotx, .odt, .ott, .rtf, .pages |
| Presentations | .ppt, .pptx, .pptm, .pot, .potm, .potx, .odp, .otp, .key |
| Spreadsheets | .xls, .xlsx, .xlsm, .xlsb, .ods, .ots, .csv, .tsv, .numbers |
| Images | .jpg, .jpeg, .png, .gif, .bmp, .tiff, .tif, .webp, .svg |
| Web | .htm, .html, .xhtml (not yet implemented) |

**External Dependencies:**
- **LibreOffice** - For office documents and spreadsheets
  - macOS: `brew install --cask libreoffice`
  - Ubuntu: `apt-get install libreoffice`
- **ImageMagick** - For images
  - macOS: `brew install imagemagick`
  - Ubuntu: `apt-get install imagemagick`
  - Windows: LiteParse resolves the executable path and validates that the binary is actually ImageMagick, avoiding false matches against the built-in `C:\Windows\System32\convert.exe`

**Key Functions:**

`convertToPdf(filePath)` - Main entry point for file path input
- Returns `ConversionResult` with `pdfPath` and `originalExtension`
- Returns `ConversionPassthrough` with `content` for text-based formats (e.g. .txt, .csv)
- Returns `ConversionError` with `message` and `code` on failure
- If already PDF, returns path unchanged

`convertBufferToPdf(data)` - Entry point for buffer input
- Detects format from magic bytes, writes to temp file, then delegates to `convertToPdf`
- Used when `LiteParse.parse()` receives a non-PDF `Buffer` or `Uint8Array`

`cleanupConversionFiles(pdfPath)` - Removes temp files after parsing
- Only deletes files in the configured temp directory (`LITEPARSE_TMPDIR` or OS default)
- Called by `LiteParse.parse()` after processing

`getTmpDir()` - Returns the temp directory for LiteParse operations
- Respects the `LITEPARSE_TMPDIR` environment variable
- Falls back to `os.tmpdir()`

`guessFileExtension(filePath)` - Detects format from extension or magic bytes
- Checks file extension first
- Falls back to magic byte detection (PDF, PNG, JPEG, ZIP-based)
- Returns `null` when format cannot be determined

`guessExtensionFromBuffer(data)` - Detects format from raw bytes using magic bytes
- Supports PDF, PNG, JPEG, TIFF (both endians), and ZIP-based formats
- Returns `null` when format cannot be determined (e.g. plain text)
- Used by `convertBufferToPdf` to determine the temp file extension

**Design Decisions:**

1. **External tools over native parsers**: Using LibreOffice and ImageMagick provides broad format support with minimal code. Trade-off is external dependency requirement.

2. **Subprocess-based**: Conversion runs as separate process to isolate crashes and memory issues.

3. **Temporary file management**: Converted PDFs go to the configured temp directory (`LITEPARSE_TMPDIR` env var or OS default), cleaned up after parsing.

4. **Timeout handling**: 2 minutes for LibreOffice, 1 minute for ImageMagick.

5. **Executable validation on Windows**: Image conversion validates the resolved executable path and probes the binary with `-version` so Windows `convert.exe` is never mistaken for ImageMagick.

6. **Magic byte fallback**: Handles files without extensions or with wrong extensions.

**Error Codes:**
- `FILE_NOT_FOUND` - Input file doesn't exist
- `UNSUPPORTED_FORMAT` - Format not supported
- `CONVERSION_ERROR` - Conversion process failed
