//! PPI (Portfolio Personal Inversiones) XLSX importer.
//!
//! One sheet per currency wallet (Pesos, Dolar MEP, Dolar Cable, …).
//! Skip sheet "Instrumentos".
//! Columns per sheet: Fecha | Descripción | Cantidad | Precio | Importe | Saldo | Moneda

use rust_decimal::Decimal;
use std::collections::HashSet;
use std::str::FromStr;

use super::activities_model::ActivityImport;

const COL_FECHA: usize = 0;
const COL_DESCRIPCION: usize = 1;
const COL_CANTIDAD: usize = 2;
const COL_PRECIO: usize = 3;
const COL_IMPORTE: usize = 4;

/// Heuristic: Argentine sovereign/corporate bond tickers are typically 4 chars: 2 uppercase letters + 2 digits.
/// Examples: AL35, AL30, GD30, GD35, AE38, TX28. CEDEARs and equities are longer or different pattern.
fn looks_like_ars_bond(ticker: &str) -> bool {
    let b = ticker.as_bytes();
    b.len() == 4
        && b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && b[2].is_ascii_digit()
        && b[3].is_ascii_digit()
}

pub fn parse_ppi_xlsx(
    sheets: &[(String, Vec<Vec<String>>)],
    account_id: Option<String>,
) -> (Vec<ActivityImport>, Vec<String>) {
    let mut activities = Vec::new();
    let mut errors = Vec::new();
    // Tracks SELLs seen in wallet sheets: (date, ticker, abs_qty) — used to dedup Instrumentos VENTAs
    let mut wallet_sells: HashSet<(String, String, String)> = HashSet::new();

    // First pass: wallet sheets (skip Instrumentos)
    for (sheet_name, rows) in sheets {
        if sheet_name.to_uppercase() == "INSTRUMENTOS" {
            continue;
        }
        let currency = map_ppi_sheet_currency(sheet_name);

        for (i, row) in rows.iter().enumerate().skip(1) {
            if row.len() < 5 {
                continue;
            }
            let desc = row[COL_DESCRIPCION].trim().to_string();
            if desc.is_empty() {
                continue;
            }
            // Track SELLs for Instrumentos deduplication
            if desc.to_uppercase().starts_with("VENTA ") {
                let ticker = desc["VENTA ".len()..].trim().replace(' ', "");
                if let Some(date) = parse_ppi_date(row[COL_FECHA].trim()) {
                    if let Some(qty) = parse_decimal(&row[COL_CANTIDAD]).map(|d| d.abs()) {
                        wallet_sells.insert((date, ticker, qty.normalize().to_string()));
                    }
                }
            }
            match parse_ppi_row(row.as_slice(), i + 1, &currency, account_id.clone()) {
                Ok(Some(activity)) => activities.push(activity),
                Ok(None) => {}
                Err(e) => errors.push(format!("Sheet '{}' row {}: {}", sheet_name, i + 1, e)),
            }
        }
    }

    // Second pass: Instrumentos sheet — VENTA only, deduplicated against wallet sells.
    // Instrumentos columns: Fecha(0) | Descripción(1) | Especie(2) | Cantidad(3) | Precio(4) | Moneda(5)
    // It contains all transactions (BUY+SELL) but prices are often 0.
    // We skip COMPRA here because wallet sheets already have those with correct prices.
    for (sheet_name, rows) in sheets {
        if sheet_name.to_uppercase() != "INSTRUMENTOS" {
            continue;
        }
        for (i, row) in rows.iter().enumerate().skip(1) {
            if row.len() < 6 {
                continue;
            }
            if !row[1].trim().to_uppercase().starts_with("VENTA") {
                continue;
            }
            let ticker = row[1].trim().splitn(2, ' ')
                .nth(1)
                .unwrap_or("")
                .trim()
                .replace(' ', "");
            if ticker.is_empty() {
                continue;
            }
            let date_raw = row[0].trim();
            let date = match parse_ppi_date(date_raw) {
                Some(d) => d,
                None => {
                    errors.push(format!("Sheet '{}' row {}: invalid date '{}'", sheet_name, i + 1, date_raw));
                    continue;
                }
            };
            let qty_abs = parse_decimal(&row[3]).map(|d| d.abs());
            let qty_key = qty_abs.map(|q| q.normalize().to_string()).unwrap_or_default();
            if wallet_sells.contains(&(date.clone(), ticker.clone(), qty_key)) {
                continue;
            }
            let unit_price = parse_decimal(&row[4]).filter(|&p| p != Decimal::ZERO);
            let currency = {
                let moneda = row[5].trim().to_lowercase();
                if moneda.is_empty() || moneda.contains("peso") || moneda == "ars" {
                    "ARS".to_string()
                } else {
                    "USD".to_string()
                }
            };
            let instrument_type = if looks_like_ars_bond(&ticker) { Some("Bond".to_string()) } else { None };
            activities.push(ActivityImport {
                id: None,
                date,
                symbol: ticker.clone(),
                activity_type: "SELL".to_string(),
                quantity: qty_abs,
                unit_price,
                currency,
                fee: None,
                amount: None,
                comment: Some(format!("VENTA {}", ticker)),
                account_id: account_id.clone(),
                account_name: None,
                symbol_name: None,
                exchange_mic: Some("XBUE".to_string()),
                provider_id: None,
                provider_symbol: None,
                quote_ccy: None,
                instrument_type,
                quote_mode: None,
                errors: None,
                warnings: None,
                duplicate_of_id: None,
                duplicate_of_line_number: None,
                is_draft: true,
                is_valid: true,
                line_number: Some((i + 1) as i32),
                fx_rate: None,
                subtype: None,
                asset_id: None,
                isin: None,
                force_import: false,
                is_external: None,
            });
        }
    }

    (activities, errors)
}

fn parse_ppi_row(
    row: &[String],
    line: usize,
    currency: &str,
    account_id: Option<String>,
) -> Result<Option<ActivityImport>, String> {
    let desc = row[COL_DESCRIPCION].trim();
    let upper = desc.to_uppercase();

    let (activity_type, symbol) = classify_ppi(desc, &upper)?;
    let Some(activity_type) = activity_type else { return Ok(None); };

    let date_raw = row[COL_FECHA].trim();
    let date = parse_ppi_date(date_raw)
        .ok_or_else(|| format!("invalid date '{}'", date_raw))?;

    let cantidad = parse_decimal(&row[COL_CANTIDAD]);
    let quantity = cantidad.map(|d| d.abs());
    let unit_price = parse_decimal(&row[COL_PRECIO]).filter(|&p| p != Decimal::ZERO);
    let raw_importe = parse_decimal(&row[COL_IMPORTE]);
    let amount = raw_importe.map(|d| d.abs());

    // PPI records each dividend/interest/amortization twice:
    // once as a positive receipt in the USD wallet, and once as a negative
    // ARS withholding tax in the Pesos sheet. The negative-importe entry must
    // be imported as FEE (not DIVIDEND/INTEREST) so the ARS cash balance is
    // correctly reduced instead of inflated.
    let activity_type =
        if matches!(activity_type, "DIVIDEND" | "INTEREST")
            && raw_importe.map(|v| v < Decimal::ZERO).unwrap_or(false)
        {
            "FEE"
        } else {
            activity_type
        };

    // Normalize FCI tickers (remove spaces)
    let symbol = symbol.replace(' ', "");
    let has_symbol = matches!(activity_type, "BUY" | "SELL" | "DIVIDEND" | "INTEREST") && !symbol.is_empty();
    let exchange_mic = if has_symbol { Some("XBUE".to_string()) } else { None };
    let instrument_type = if has_symbol && looks_like_ars_bond(&symbol) {
        Some("Bond".to_string())
    } else {
        None
    };

    // For BUY/SELL, importe includes commissions (importe = qty * precio + comisión).
    // Extract the implicit fee so the snapshot engine accounts for it correctly.
    let fee = if matches!(activity_type, "BUY" | "SELL") {
        match (amount, quantity, unit_price) {
            (Some(amt), Some(qty), Some(price)) if qty > Decimal::ZERO => {
                let commission = amt - qty * price;
                if commission > rust_decimal::Decimal::new(1, 4) { Some(commission) } else { None }
            }
            _ => None,
        }
    } else {
        None
    };

    Ok(Some(ActivityImport {
        id: None,
        date,
        symbol,
        activity_type: activity_type.to_string(),
        quantity,
        unit_price,
        currency: currency.to_string(),
        fee,
        amount,
        comment: Some(desc.to_string()),
        account_id,
        account_name: None,
        symbol_name: None,
        exchange_mic,
        provider_id: None,
        provider_symbol: None,
        quote_ccy: None,
        instrument_type,
        quote_mode: None,
        errors: if activity_type == "UNKNOWN" {
            let mut m = std::collections::HashMap::new();
            m.insert("activityType".to_string(), vec![format!("Tipo de movimiento no reconocido: '{}'", desc)]);
            Some(m)
        } else {
            None
        },
        warnings: None,
        duplicate_of_id: None,
        duplicate_of_line_number: None,
        is_draft: true,
        is_valid: activity_type != "UNKNOWN",
        line_number: Some(line as i32),
        fx_rate: None,
        subtype: None,
        asset_id: None,
        isin: None,
        force_import: false,
        is_external: None,
    }))
}

/// Returns (activity_type, symbol). activity_type=None → skip row.
fn classify_ppi<'a>(desc: &str, upper: &str) -> Result<(Option<&'a str>, String), String> {
    // COMPRA <TICKER> — take everything after "COMPRA " to handle multi-word tickers (e.g. "BCACC A")
    if upper.starts_with("COMPRA ") {
        let ticker = desc["COMPRA ".len()..].trim().to_string();
        return Ok((Some("BUY"), ticker));
    }
    // VENTA <TICKER>
    if upper.starts_with("VENTA ") {
        let ticker = desc["VENTA ".len()..].trim().to_string();
        return Ok((Some("SELL"), ticker));
    }
    // Dividendo en efectivo / <TICKER>
    if upper.starts_with("DIVIDENDO EN EFECTIVO") {
        let ticker = extract_after_slash(desc);
        return Ok((Some("DIVIDEND"), ticker));
    }
    // Renta / <TICKER>
    if upper.starts_with("RENTA /") || upper.starts_with("RENTA/") {
        let ticker = extract_after_slash(desc);
        return Ok((Some("INTEREST"), ticker));
    }
    // Amortización / <TICKER>
    if upper.starts_with("AMORTIZACI") {
        let ticker = extract_after_slash(desc);
        return Ok((Some("INTEREST"), ticker));
    }
    // Interest payment (INTR) - DISN / <TICKER>  (e.g. "Interest payment (INTR) - DISN / AL30")
    // The segment after " - " is "<payment-code> / <ticker>"; we want only the ticker.
    if upper.starts_with("INTEREST PAYMENT") {
        let after_dash = extract_after_dash(desc);
        let ticker = after_dash
            .rsplit('/')
            .next()
            .unwrap_or(&after_dash)
            .trim()
            .to_string();
        return Ok((Some("INTEREST"), ticker));
    }
    // Liquidación de Rescate / ... — SELL (FCI redemption)
    if upper.starts_with("LIQUIDACI") && upper.contains("N DE RESCATE") {
        // Last segment after '/' is FCI name
        let ticker = desc.rsplit('/').next().unwrap_or("").trim().to_string();
        return Ok((Some("SELL"), ticker));
    }
    // Liquidación de Suscripción / <number> / <ticker> — FCI subscription (BUY)
    if upper.starts_with("LIQUIDACI") && upper.contains("N DE SUSCRIPCI") {
        let ticker = desc.rsplit('/').next().unwrap_or("").trim().to_string();
        return Ok((Some("BUY"), ticker));
    }
    // Ingreso de Fondos — DEPOSIT
    if upper.starts_with("INGRESO DE FONDOS") {
        return Ok((Some("DEPOSIT"), String::new()));
    }
    // Retiro de Fondos — WITHDRAWAL
    if upper.starts_with("RETIRO DE FONDOS") {
        return Ok((Some("WITHDRAWAL"), String::new()));
    }
    // Skip: manual movements, swaps, FCI cash blocks/unblocks, FX conversions
    if upper.starts_with("MOVIMIENTO MANUAL")
        || upper.starts_with("CANJE")
        || upper.starts_with("BLOQUEO MONETARIO")
        || upper.starts_with("DESBLOQUEO MONETARIO")
        || upper.contains("MONEDAS NRO")
    {
        return Ok((None, String::new()));
    }
    // Unknown
    Ok((Some("UNKNOWN"), String::new()))
}

fn extract_after_slash(desc: &str) -> String {
    desc.split_once('/')
        .map(|(_, rest)| rest.trim())
        .unwrap_or("")
        .to_string()
}

fn extract_after_dash(desc: &str) -> String {
    desc.split_once(" - ")
        .map(|(_, rest)| rest.trim())
        .unwrap_or("")
        .to_string()
}

fn map_ppi_sheet_currency(sheet_name: &str) -> String {
    let lower = sheet_name.to_lowercase();
    if lower.contains("peso") || lower == "ars" {
        "ARS".to_string()
    } else {
        "USD".to_string()
    }
}

fn parse_ppi_date(s: &str) -> Option<String> {
    let s = s.trim();
    // DD/MM/YYYY
    if s.len() == 10 && s.as_bytes()[2] == b'/' {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 3 {
            return Some(format!("{}-{:0>2}-{:0>2}", parts[2], parts[1], parts[0]));
        }
    }
    // YYYY-MM-DD (already ISO)
    if s.len() == 10 && s.as_bytes()[4] == b'-' {
        return Some(s.to_string());
    }
    None
}

fn parse_decimal(s: &str) -> Option<Decimal> {
    let s = s.trim();
    let s = if s.contains('.') && s.contains(',') {
        s.replace('.', "").replace(',', ".")
    } else {
        s.replace(',', ".")
    };
    Decimal::from_str(&s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(name: &str, rows: Vec<Vec<&str>>) -> (String, Vec<Vec<String>>) {
        let string_rows = rows.into_iter().map(|r| r.into_iter().map(String::from).collect()).collect();
        (name.to_string(), string_rows)
    }

    #[test]
    fn negative_dividend_in_pesos_becomes_fee() {
        // In the Pesos sheet, a "Dividendo en efectivo" with negative importe is an
        // ARS withholding tax — it must be imported as FEE, not DIVIDEND.
        let sheets = vec![sheet("Pesos", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["15/05/2026", "Dividendo en efectivo / AAPL", "0", "0", "-12.09", "-242.84", "Pesos"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, "FEE");
    }

    #[test]
    fn positive_dividend_in_usd_wallet_stays_dividend() {
        // In USD wallets, a positive "Dividendo en efectivo" is the actual income.
        let sheets = vec![sheet("DolarCV7000 Ext.", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["15/05/2026", "Dividendo en efectivo / AAPL", "0", "0", "0.53", "1.17", "DolarCV7000 Ext."],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, "DIVIDEND");
    }

    #[test]
    fn negative_renta_in_pesos_becomes_fee() {
        let sheets = vec![sheet("Pesos", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["09/01/2025", "Renta / AL35", "0", "0", "-133.33", "-238.04", "Pesos"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, "FEE");
    }

    #[test]
    fn interest_payment_ticker_extracted_correctly() {
        // "Interest payment (INTR) - DISN / AL30" → ticker must be "AL30", not "DISN / AL30"
        let sheets = vec![sheet("DolarCV10000-Op.", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["10/07/2025", "Interest payment (INTR) - DISN / AL30", "0", "0", "8.14", "232.74", "DolarCV10000-Op."],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, "INTEREST");
        assert_eq!(activities[0].symbol, "AL30");
    }

    #[test]
    fn instrumentos_venta_imported() {
        // VENTA in Instrumentos sheet not present in any wallet sheet must become a SELL.
        let sheets = vec![
            sheet("Pesos", vec![
                vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
                vec!["01/06/2024", "COMPRA AL30", "-100", "55.20", "-5520", "0", "Pesos"],
            ]),
            sheet("Instrumentos", vec![
                vec!["Fecha", "Descripción", "Especie", "Cantidad", "Precio", "Moneda"],
                vec!["15/06/2024", "VENTA AL30", "BONOS ARGENTINA USD 2030 L.A", "-915", "0", ""],
            ]),
        ];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let sells: Vec<_> = activities.iter().filter(|a| a.activity_type == "SELL").collect();
        assert_eq!(sells.len(), 1);
        assert_eq!(sells[0].symbol, "AL30");
        assert_eq!(sells[0].quantity, Some(Decimal::from(915)));
        assert_eq!(sells[0].currency, "ARS");
    }

    #[test]
    fn instrumentos_compra_skipped() {
        // COMPRA in Instrumentos must NOT be imported (wallet sheets have those with prices).
        let sheets = vec![sheet("Instrumentos", vec![
            vec!["Fecha", "Descripción", "Especie", "Cantidad", "Precio", "Moneda"],
            vec!["01/06/2024", "COMPRA", "AL30", "100", "55.20", "ARS"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert!(activities.is_empty(), "COMPRA from Instrumentos should be skipped");
    }

    #[test]
    fn fci_subscription_becomes_buy() {
        // "Liquidación de Suscripción / <number> / <ticker>" is a FCI fund subscription — must be BUY.
        let sheets = vec![sheet("DolarCV7000 Ext.", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["15/04/2024", "Liquidación de Suscripción / 663951 / ALP.LAT.K", "456.44", "1.10", "-502.08", "0", "USD"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, "BUY");
        assert_eq!(activities[0].symbol, "ALP.LAT.K");
        assert_eq!(activities[0].quantity, Some(Decimal::from_str("456.44").unwrap()));
    }

    #[test]
    fn fci_cash_blocks_and_fx_conversions_skipped() {
        let sheets = vec![sheet("DolarCV7000 Ext.", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["15/04/2024", "Bloqueo Monetario por Solicitud de Suscripción de FCI ALP.LAT.K", "0", "0", "0", "0", "USD"],
            vec!["15/04/2024", "Desbloqueo Monetario por Liquidación de Suscripción / 663951 / ALP.LAT.K", "0", "0", "0", "0", "USD"],
            vec!["16/04/2024", "Débito/Crédito de Monedas Nro. 126262 / 16/4/2024", "0", "0", "0", "0", "USD"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert!(activities.is_empty(), "cash block / FX rows should be skipped");
    }

    #[test]
    fn instrumentos_venta_deduped_against_wallet() {
        // If a VENTA with the same (date, ticker, qty) exists in a wallet sheet,
        // the Instrumentos copy must be skipped to avoid a duplicate SELL.
        let sheets = vec![
            sheet("Pesos", vec![
                vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
                vec!["20/08/2024", "VENTA YPFD", "-1", "25000", "25000", "0", "Pesos"],
            ]),
            sheet("Instrumentos", vec![
                vec!["Fecha", "Descripción", "Especie", "Cantidad", "Precio", "Moneda"],
                vec!["20/08/2024", "VENTA YPFD", "YPF", "-1", "0", ""],
            ]),
        ];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        let sells: Vec<_> = activities.iter().filter(|a| a.activity_type == "SELL").collect();
        assert_eq!(sells.len(), 1, "duplicate SELL from Instrumentos must be deduplicated");
        // The wallet version (with price) should survive
        assert_eq!(sells[0].unit_price, Some(Decimal::from(25000)));
    }

    #[test]
    fn parse_decimal_handles_european_thousands() {
        assert_eq!(parse_decimal("1.234,56"), Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(parse_decimal("1234,56"),  Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(parse_decimal("1234.56"),  Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(parse_decimal("0"),        Some(Decimal::ZERO));
        assert_eq!(parse_decimal(""),         None);
    }

    #[test]
    fn instrumentos_venta_usd_bond_gets_usd_currency() {
        // A bond sold in Dólares (e.g. GD30) must come out with currency="USD".
        let sheets = vec![sheet("Instrumentos", vec![
            vec!["Fecha", "Descripción", "Especie", "Cantidad", "Precio", "Moneda"],
            vec!["10/06/2024", "VENTA GD30", "BONOS USD 2030", "-500", "65.20", "Dólares"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].currency, "USD");
    }

    #[test]
    fn instrumentos_venta_empty_moneda_defaults_to_ars() {
        // An empty Moneda column must default to ARS (most trades settle in pesos).
        let sheets = vec![sheet("Instrumentos", vec![
            vec!["Fecha", "Descripción", "Especie", "Cantidad", "Precio", "Moneda"],
            vec!["10/06/2024", "VENTA AL30", "BONOS USD 2030 L.A", "-100", "0", ""],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].currency, "ARS");
    }

    #[test]
    fn unknown_activity_marked_invalid_with_error() {
        let sheets = vec![sheet("Pesos", vec![
            vec!["Fecha", "Descripción", "Cantidad", "Precio", "Importe", "Saldo", "Moneda"],
            vec!["15/05/2026", "Movimiento Desconocido / ALGO", "0", "0", "100", "100", "Pesos"],
        ])];
        let (activities, errors) = parse_ppi_xlsx(&sheets, None);
        assert!(errors.is_empty(), "parse errors should be empty: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert!(!activities[0].is_valid, "UNKNOWN activity must be invalid");
        assert!(activities[0].errors.is_some(), "UNKNOWN activity must have errors");
    }
}
