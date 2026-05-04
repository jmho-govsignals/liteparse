/**
 * Creates a dataset for regression testing
 *
 * Output structure:
 *   dataset/
 *     data/
 *       doc1.pdf
 *       doc2.docx
 *       ...
 *     metadata.jsonl  (each line: {"file_name": "data/doc1.pdf", "document": "doc1.pdf", "page": 1, "output_json": {...}})
 *
 * Usage:
 *   npx tsx scripts/create-dataset.ts [output-dir] [source-docs-dir]
 *
 * Arguments:
 *   output-dir      - Where to write the dataset (default: ./dataset)
 *   source-docs-dir - Where to read source documents from (default: ./e2e-test-docs)
 *
 * The dataset can be used with compare-dataset.ts to detect output changes.
 */

import { LiteParse } from "../src/lib.js";
import * as fs from "fs/promises";
import * as path from "path";

const DEFAULT_SOURCE_DIR = path.join(import.meta.dirname, "..", "e2e-test-docs");
const DEFAULT_OUTPUT_DIR = path.join(import.meta.dirname, "..", "dataset");

interface DatasetRow {
  file_name: string;
  document: string;
  page: number;
  output_text: string;
  output_json: object;
}

async function findFiles(dir: string, baseDir: string = dir): Promise<string[]> {
  const files: string[] = [];
  const entries = await fs.readdir(dir, { withFileTypes: true });

  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);

    if (entry.isDirectory()) {
      // Recursively search subdirectories
      const subFiles = await findFiles(fullPath, baseDir);
      files.push(...subFiles);
    } else if (entry.isFile()) {
      files.push(fullPath);
    }
  }

  return files;
}

async function processFile(filePath: string, baseDocDir: string): Promise<DatasetRow[]> {
  const relativePath = path.relative(baseDocDir, filePath);

  console.log(`Processing: ${relativePath}`);

  const rows: DatasetRow[] = [];

  try {
    // Parse the document
    const parser = new LiteParse({
      outputFormat: "json",
      ocrEnabled: false, // Disable OCR for deterministic results
      preciseBoundingBox: true,
    });

    const result = await parser.parse(filePath, true);

    // For non-PDF files that return text directly (no pages)
    if (result.pages.length === 0 && result.text) {
      rows.push({
        file_name: `data/${relativePath}`,
        document: relativePath,
        page: 1,
        output_text: result.text,
        output_json: { text: result.text },
      });
      console.log(`  -> 1 text entry`);
      return rows;
    }

    // Create entries for each page
    for (const page of result.pages) {
      const jsonPage = result.json?.pages.find((p) => p.page === page.pageNum);
      rows.push({
        file_name: `data/${relativePath}`,
        document: relativePath,
        page: page.pageNum,
        output_text: page.text,
        output_json: jsonPage || { page: page.pageNum, text: page.text },
      });
    }

    console.log(`  -> ${rows.length} pages`);
  } catch (error) {
    console.error(`  ERROR: ${error instanceof Error ? error.message : error}`);
    // Record the error as a dataset entry (blank result)
    rows.push({
      file_name: `data/${relativePath}`,
      document: relativePath,
      page: 0,
      output_text: "",
      output_json: {
        error: true,
        message: error instanceof Error ? error.message : String(error),
      },
    });
  }

  return rows;
}

async function main() {
  const outputDir = process.argv[2] || DEFAULT_OUTPUT_DIR;
  const sourceDocsDir = process.argv[3] || DEFAULT_SOURCE_DIR;
  const documentsDir = path.join(outputDir, "data");

  // Check if source is already in the output documents folder (skip copy in this case)
  const resolvedSource = path.resolve(sourceDocsDir);
  const resolvedDocuments = path.resolve(documentsDir);
  const skipCopy = resolvedSource === resolvedDocuments;

  console.log("LiteParse Dataset Generator");
  console.log("===========================");
  console.log(`Source: ${sourceDocsDir}`);
  console.log(`Output: ${outputDir}`);
  if (skipCopy) {
    console.log(`(Source is output documents dir - skipping copy)`);
  }
  console.log();

  // Create output directories
  await fs.mkdir(documentsDir, { recursive: true });

  // Find all processable files
  console.log("Finding files...");
  const files = await findFiles(sourceDocsDir);
  console.log(`Found ${files.length} files to process\n`);

  // Process each file and copy to documents folder (unless source is already there)
  const allRows: DatasetRow[] = [];
  const copiedDocs = new Set<string>();

  for (const file of files) {
    const rows = await processFile(file, sourceDocsDir);
    allRows.push(...rows);

    // Copy document file to dataset (only once per document, skip if source is output)
    const relativePath = path.relative(sourceDocsDir, file);
    if (!skipCopy && !copiedDocs.has(relativePath)) {
      const destPath = path.join(documentsDir, relativePath);
      await fs.mkdir(path.dirname(destPath), { recursive: true });
      await fs.copyFile(file, destPath);
      copiedDocs.add(relativePath);
    } else if (skipCopy) {
      copiedDocs.add(relativePath);
    }
  }

  // Validate we actually produced entries
  if (allRows.length === 0) {
    console.error("\nERROR: No dataset entries were generated. Check that source documents exist and are parseable.");
    process.exit(1);
  }

  // Write metadata.jsonl
  const metadataPath = path.join(outputDir, "metadata.jsonl");
  const metadataContent = allRows.map((row) => JSON.stringify(row)).join("\n");
  await fs.writeFile(metadataPath, metadataContent);

  console.log();
  console.log("Dataset generation complete!");
  console.log(`  Total entries: ${allRows.length}`);
  console.log(`  Documents copied: ${copiedDocs.size}`);
  console.log(`  Metadata: ${metadataPath}`);
  console.log(`  Documents: ${documentsDir}`);
  console.log();
  console.log("Use compare-dataset.ts to compare future output against this baseline.");
}

main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
