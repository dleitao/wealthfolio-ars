//! XLSX parsing with auto-detection for Balanz and PPI (Portfolio Personal) formats.

use calamine::{open_workbook_from_rs, Data, Reader, Xlsx};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

use super::activities_model::ActivityImport;
use super::balanz_importer::parse_balanz;
use super::ppi_xlsx_importer::parse_ppi_xlsx;

/// Result of parsing an XLSX file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedXlsxResult {
    /// Detected format: "BALANZ", "PPI", or "UNKNOWN"
    pub format: String,
    pub activities: Vec<ActivityImport>,
    pub errors: Vec<String>,
}

/// Parse an XLSX file from raw bytes.
///
/// Auto-detects the broker format by inspecting sheet names:
/// - Sheet "movimientos" (or containing Balanz column headers) → Balanz
/// - Sheet "Instrumentos" → PPI
pub fn parse_xlsx(content: &[u8], account_id: Option<String>) -> Result<ParsedXlsxResult, String> {
    let cursor = Cursor::new(content.to_vec());
    let mut wb: Xlsx<_> = open_workbook_from_rs(cursor)
        .map_err(|e| format!("Failed to open XLSX: {}", e))?;

    let sheet_names = wb.sheet_names().to_owned();

    // Read all sheets upfront so we can inspect headers for fallback detection.
    let mut all_sheets: Vec<(String, Vec<Vec<String>>)> = sheet_names
        .iter()
        .filter_map(|name| read_sheet(&mut wb, name).ok().map(|rows| (name.clone(), rows)))
        .collect();

    let format = detect_format(&sheet_names, &all_sheets);

    match format.as_str() {
        "BALANZ" => {
            // Find the sheet: by name first, then by header content.
            let rows = all_sheets
                .iter()
                .find(|(name, _)| name.to_uppercase() == "MOVIMIENTOS")
                .or_else(|| all_sheets.iter().find(|(_, rows)| is_balanz_sheet(rows)))
                .map(|(_, rows)| rows.clone())
                .unwrap_or_default();
            let (activities, errors) = parse_balanz(&rows, account_id);
            Ok(ParsedXlsxResult { format, activities, errors })
        }
        "PPI" => {
            let sheets: Vec<(String, Vec<Vec<String>>)> = all_sheets.drain(..).collect();
            let (activities, errors) = parse_ppi_xlsx(&sheets, account_id);
            Ok(ParsedXlsxResult { format, activities, errors })
        }
        _ => {
            let names_list = sheet_names.join(", ");
            Ok(ParsedXlsxResult {
                format,
                activities: vec![],
                errors: vec![format!(
                    "Formato XLSX no reconocido. Hojas encontradas: [{}]. Se esperaba una hoja 'movimientos' (Balanz) o 'Instrumentos' (PPI).",
                    names_list
                )],
            })
        }
    }
}

/// Check if a sheet looks like a Balanz "movimientos" sheet by inspecting header columns.
fn is_balanz_sheet(rows: &[Vec<String>]) -> bool {
    if let Some(header) = rows.first() {
        let headers_upper: Vec<String> = header.iter().map(|h| h.trim().to_uppercase()).collect();
        // Balanz columns: Descripcion, Ticker, Tipo Instrumento, Concertacion, Cantidad, Precio, Moneda, Importe
        let required = ["DESCRIPCION", "TICKER", "CANTIDAD", "PRECIO", "MONEDA", "IMPORTE"];
        required.iter().all(|col| headers_upper.iter().any(|h| h.contains(col)))
    } else {
        false
    }
}

fn detect_format(sheet_names: &[String], sheets: &[(String, Vec<Vec<String>>)]) -> String {
    let names_upper: Vec<String> = sheet_names.iter().map(|s| s.to_uppercase()).collect();
    if names_upper.iter().any(|n| n == "MOVIMIENTOS") {
        return "BALANZ".to_string();
    }
    if names_upper.iter().any(|n| n == "INSTRUMENTOS") {
        return "PPI".to_string();
    }
    // Fallback: detect Balanz by column headers in any sheet
    if sheets.iter().any(|(_, rows)| is_balanz_sheet(rows)) {
        return "BALANZ".to_string();
    }
    "UNKNOWN".to_string()
}

fn read_sheet(wb: &mut Xlsx<Cursor<Vec<u8>>>, name: &str) -> Result<Vec<Vec<String>>, String> {
    let range = wb
        .worksheet_range(name)
        .map_err(|e| format!("Failed to read sheet '{}': {}", name, e))?;

    let rows = range
        .rows()
        .map(|row| row.iter().map(cell_to_string).collect())
        .collect();
    Ok(rows)
}

fn cell_to_string(cell: &Data) -> String {
    match cell {
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{:.0}", f)
            } else {
                format!("{}", f)
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => {
            // Excel serial date — format as string so downstream parsers see something
            format!("{}", dt)
        }
        Data::Empty | Data::Error(_) => String::new(),
    }
}
