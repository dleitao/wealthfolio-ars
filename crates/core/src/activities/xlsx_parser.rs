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
/// - Sheet "movimientos" → Balanz
/// - Sheet "Instrumentos" → PPI
pub fn parse_xlsx(content: &[u8], account_id: Option<String>) -> Result<ParsedXlsxResult, String> {
    let cursor = Cursor::new(content.to_vec());
    let mut wb: Xlsx<_> = open_workbook_from_rs(cursor)
        .map_err(|e| format!("Failed to open XLSX: {}", e))?;

    let sheet_names = wb.sheet_names().to_owned();

    let format = detect_format(&sheet_names);

    match format.as_str() {
        "BALANZ" => {
            let rows = read_sheet(&mut wb, "movimientos")?;
            let (activities, errors) = parse_balanz(&rows, account_id);
            Ok(ParsedXlsxResult { format, activities, errors })
        }
        "PPI" => {
            let mut sheets = Vec::new();
            for name in &sheet_names {
                if let Ok(rows) = read_sheet(&mut wb, name) {
                    sheets.push((name.clone(), rows));
                }
            }
            let (activities, errors) = parse_ppi_xlsx(&sheets, account_id);
            Ok(ParsedXlsxResult { format, activities, errors })
        }
        _ => Ok(ParsedXlsxResult {
            format,
            activities: vec![],
            errors: vec!["Unrecognized XLSX format. Expected Balanz or PPI format.".to_string()],
        }),
    }
}

fn detect_format(sheet_names: &[String]) -> String {
    let names_upper: Vec<String> = sheet_names.iter().map(|s| s.to_uppercase()).collect();
    if names_upper.iter().any(|n| n == "MOVIMIENTOS") {
        return "BALANZ".to_string();
    }
    if names_upper.iter().any(|n| n == "INSTRUMENTOS") {
        return "PPI".to_string();
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
