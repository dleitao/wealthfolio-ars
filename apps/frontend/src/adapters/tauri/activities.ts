// Tauri-specific activity commands
import type { ActivityImport, ParseConfig, ParsedCsvResult } from "@/lib/types";
import { invoke, logger } from "./core";

export interface ParsedXlsxResult {
  format: string;
  activities: ActivityImport[];
  errors: string[];
}

/**
 * Parse a CSV file with the given configuration.
 * Tauri implementation: reads file as ArrayBuffer and invokes parse_csv command.
 */
export const parseCsv = async (file: File, config: ParseConfig): Promise<ParsedCsvResult> => {
  try {
    const buffer = await file.arrayBuffer();
    const content = Array.from(new Uint8Array(buffer));
    return await invoke<ParsedCsvResult>("parse_csv", { content, config });
  } catch (err) {
    logger.error("Error parsing CSV file:", err);
    throw err;
  }
};

/**
 * Parse an XLSX file (Balanz or PPI format).
 * Tauri implementation: reads file as ArrayBuffer and invokes parse_xlsx_file command.
 */
export const parseXlsx = async (
  file: File,
  accountId?: string,
): Promise<ParsedXlsxResult> => {
  try {
    const buffer = await file.arrayBuffer();
    const content = Array.from(new Uint8Array(buffer));
    return await invoke<ParsedXlsxResult>("parse_xlsx_file", {
      content,
      accountId: accountId ?? null,
    });
  } catch (err) {
    logger.error("Error parsing XLSX file:", err);
    throw err;
  }
};
