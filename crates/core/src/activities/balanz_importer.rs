//! Balanz XLSX importer.
//!
//! Sheet: "movimientos"
//! Columns: Descripcion | Ticker | Tipo Instrumento | Concertacion | Cantidad | Precio | Liquidacion | Moneda | Importe

use rust_decimal::Decimal;
use std::str::FromStr;

use super::activities_model::ActivityImport;

const COL_DESCRIPCION: usize = 0;
const COL_TICKER: usize = 1;
const COL_TIPO_INSTRUMENTO: usize = 2;
const COL_CONCERTACION: usize = 3;
const COL_CANTIDAD: usize = 4;
const COL_PRECIO: usize = 5;
const COL_LIQUIDACION: usize = 6;
const COL_MONEDA: usize = 7;
const COL_IMPORTE: usize = 8;

fn extract_boleto_id(desc: &str) -> Option<u64> {
    // "Boleto / 5684749 / APCOLFUT / 4 / $" → 5684749
    desc.split(" / ").nth(1)?.trim().parse().ok()
}

fn balanz_instrument_type(tipo: &str) -> Option<&'static str> {
    match tipo.trim().to_uppercase().as_str() {
        "BONOS" | "LETRAS" | "ON" | "OBLIGACIONES NEGOCIABLES" => Some("Bond"),
        "ACCIONES" | "CEDEARS" => Some("Equity"),
        "FCI" | "FONDOS" => Some("ETF"),
        _ => None,
    }
}

pub fn parse_balanz(rows: &[Vec<String>], account_id: Option<String>) -> (Vec<ActivityImport>, Vec<String>) {
    let mut activities = Vec::new();
    let mut errors = Vec::new();

    // First pass: build ticker → full fund name from "Liquidación de Suscripción" rows.
    // Example desc: "Liquidación de Suscripción / 743258 / BALANZ CAPITAL ACCIONES ARGENTINAS  A"
    // The ticker in those rows (space-stripped) matches the ticker on the BUY row.
    let fci_names: std::collections::HashMap<String, String> = rows
        .iter()
        .skip(1)
        .filter(|r| r.len() >= 9)
        .filter_map(|r| {
            let desc = r[COL_DESCRIPCION].trim().to_uppercase();
            if desc.starts_with("LIQUIDACI") && desc.contains("SUSCRIPCI") {
                let ticker = r[COL_TICKER].trim().replace(' ', "");
                if ticker.is_empty() { return None; }
                let name = r[COL_DESCRIPCION]
                    .trim()
                    .splitn(3, " / ")
                    .nth(2)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())?;
                Some((ticker, name))
            } else {
                None
            }
        })
        .collect();

    // First pass: build boleto_id → principal for APCOLCON (caución apertura) rows.
    // APCOLFUT (vencimiento) uses this to compute net interest = importe_fut - principal.
    let caucion_principals: std::collections::HashMap<u64, Decimal> = rows
        .iter()
        .skip(1)
        .filter(|r| r.len() >= 9)
        .filter_map(|r| {
            let upper = r[COL_DESCRIPCION].trim().to_uppercase();
            if upper.starts_with("BOLETO") && upper.contains("APCOLCON") {
                let id = extract_boleto_id(r[COL_DESCRIPCION].trim())?;
                let principal = parse_decimal(&r[COL_IMPORTE]).map(|d| d.abs())?;
                Some((id, principal))
            } else {
                None
            }
        })
        .collect();

    // Skip header row
    for (i, row) in rows.iter().enumerate().skip(1) {
        if row.len() < 9 {
            continue;
        }
        let desc = row[COL_DESCRIPCION].trim().to_string();
        if desc.is_empty() {
            continue;
        }
        match parse_balanz_row(row.as_slice(), i + 1, account_id.clone(), &fci_names, &caucion_principals) {
            Ok(Some(activity)) => activities.push(activity),
            Ok(None) => {} // skipped row
            Err(e) => errors.push(format!("Row {}: {}", i + 1, e)),
        }
    }
    let has_deposits = activities.iter().any(|a| a.activity_type == "DEPOSIT");
    if !has_deposits && !activities.is_empty() {
        log::warn!(
            "[Balanz] No deposit activities found — performance will be calculated from cost basis (HOLDINGS mode fallback)"
        );
    }

    (activities, errors)
}

fn parse_balanz_row(
    row: &[String],
    line: usize,
    account_id: Option<String>,
    fci_names: &std::collections::HashMap<String, String>,
    caucion_principals: &std::collections::HashMap<u64, Decimal>,
) -> Result<Option<ActivityImport>, String> {
    let desc = row[COL_DESCRIPCION].trim();
    let upper = desc.to_uppercase();

    // Cauciones bursátiles: pares de boletos APCOLCON (apertura) + APCOLFUT (vencimiento).
    // APCOLCON se ignora (el capital no sale del portfolio).
    // APCOLFUT emite INTEREST con el interés neto = importe_fut - principal_apertura.
    if upper.starts_with("BOLETO") && upper.contains("APCOLCON") {
        return Ok(None);
    }
    if upper.starts_with("BOLETO") && upper.contains("APCOLFUT") {
        let id = extract_boleto_id(desc).ok_or_else(|| "caución: no boleto id".to_string())?;
        let importe_fut = parse_decimal(&row[COL_IMPORTE])
            .ok_or_else(|| "caución: no importe".to_string())?
            .abs();
        let principal = caucion_principals.get(&(id.saturating_sub(1))).copied().unwrap_or(Decimal::ZERO);
        let interes = if principal > Decimal::ZERO { importe_fut - principal } else { importe_fut };
        let date = parse_date_iso(row[COL_LIQUIDACION].trim())
            .ok_or_else(|| format!("caución: invalid liquidacion date '{}'", row[COL_LIQUIDACION].trim()))?;
        let currency = map_balanz_currency(row[COL_MONEDA].trim());
        return Ok(Some(ActivityImport {
            id: None,
            date,
            symbol: String::new(),
            activity_type: "INTEREST".to_string(),
            quantity: None,
            unit_price: None,
            currency,
            fee: None,
            amount: Some(interes),
            comment: Some(desc.to_string()),
            account_id,
            account_name: None,
            symbol_name: None,
            exchange_mic: None,
            provider_id: None,
            provider_symbol: None,
            quote_ccy: None,
            instrument_type: None,
            quote_mode: None,
            errors: None,
            warnings: None,
            duplicate_of_id: None,
            duplicate_of_line_number: None,
            is_draft: true,
            is_valid: true,
            line_number: Some(line as i32),
            fx_rate: None,
            subtype: None,
            asset_id: None,
            isin: None,
            force_import: false,
            is_external: None,
        }));
    }

    let mut activity_type = classify_balanz(&upper)?;
    let Some(ref mut at) = activity_type else { return Ok(None); };

    let ticker = row[COL_TICKER].trim().to_string();
    // Normalize FCI tickers (remove spaces)
    let ticker = ticker.replace(' ', "");

    let date_raw = row[COL_CONCERTACION].trim();
    let date = parse_date_iso(date_raw)
        .ok_or_else(|| format!("invalid date '{}'", date_raw))?;

    let currency = map_balanz_currency(row[COL_MONEDA].trim());

    let quantity = parse_decimal_abs(&row[COL_CANTIDAD]);
    let raw_importe = parse_decimal(&row[COL_IMPORTE]);
    let raw_precio = parse_decimal(&row[COL_PRECIO]);
    let is_no_price = raw_precio.map(|p| p == Decimal::from(-1)).unwrap_or(true);

    // Negative importe reclassifications
    if (*at == "DIVIDEND" || *at == "INTEREST") && raw_importe.map(|v| v < Decimal::ZERO).unwrap_or(false) {
        *at = "FEE";
    }
    // SELL with price sentinel and negative importe = ARS commission leg of MEP/dólar-cable operation.
    // Without this, both ARS-commission and USD-proceeds rows become SELL, creating a short position.
    if *at == "SELL" && is_no_price && raw_importe.map(|v| v < Decimal::ZERO).unwrap_or(false) {
        *at = "FEE";
    }

    let importe = raw_importe.map(|d| d.abs());
    let activity_type = at.to_string();

    // Precio == -1 is a sentinel "no price"
    let unit_price = raw_precio.filter(|&p| p != Decimal::from(-1));

    // For cash activities (DEPOSIT, WITHDRAWAL, FEE), symbol is left empty
    let symbol = if matches!(activity_type.as_str(), "DEPOSIT" | "WITHDRAWAL" | "FEE") {
        String::new()
    } else {
        ticker
    };
    let has_symbol = matches!(activity_type.as_str(), "BUY" | "SELL" | "DIVIDEND" | "INTEREST") && !symbol.is_empty();
    let instrument_type = if has_symbol {
        balanz_instrument_type(row[COL_TIPO_INSTRUMENTO].trim()).map(str::to_string)
    } else {
        None
    };
    // All XBUE instruments (equities, CEDEARs, FCIs) are quoted in ARS.
    // Transaction currency stays as-is in `currency`; quote_ccy is always ARS for XBUE.
    let exchange_mic = if has_symbol { Some("XBUE".to_string()) } else { None };
    let quote_ccy = if has_symbol { Some("ARS".to_string()) } else { None };
    // For FCIs, look up the full fund name so the UI can display it for manual search.
    let symbol_name = if has_symbol {
        fci_names.get(&symbol).cloned()
    } else {
        None
    };

    let is_unknown = activity_type == "UNKNOWN";
    Ok(Some(ActivityImport {
        id: None,
        date,
        symbol,
        activity_type,
        quantity,
        unit_price,
        currency,
        fee: None,
        amount: importe,
        comment: Some(desc.to_string()),
        account_id,
        account_name: None,
        symbol_name,
        exchange_mic,
        provider_id: None,
        provider_symbol: None,
        quote_ccy,
        instrument_type,
        quote_mode: None,
        errors: if is_unknown {
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
        is_valid: !is_unknown,
        line_number: Some(line as i32),
        fx_rate: None,
        subtype: None,
        asset_id: None,
        isin: None,
        force_import: false,
        is_external: None,
    }))
}

fn classify_balanz<'a>(upper: &str) -> Result<Option<&'a str>, String> {
    if upper.starts_with("BOLETO") {
        if upper.contains("COMPRA") {
            return Ok(Some("BUY"));
        }
        if upper.contains("VENTA") {
            return Ok(Some("SELL"));
        }
    }
    if upper.starts_with("DIVIDENDO EN EFECTIVO") {
        // DIVIDEND or FEE — caller checks importe sign
        return Ok(Some("DIVIDEND"));
    }
    if upper.starts_with("SUSCRIPCI") && upper.contains("N DESDE BALANZ") {
        return Ok(Some("BUY"));
    }
    if upper.starts_with("LIQUIDACI") && upper.contains("N DE SUSCRIPCI") {
        return Ok(None); // skip — cash leg of FCI subscription
    }
    if upper.starts_with("LIQUIDACI") && upper.contains("N DE RESCATE") {
        return Ok(Some("SELL")); // FCI redemption
    }
    if upper.starts_with("RECIBO DE COBRO") {
        return Ok(Some("DEPOSIT"));
    }
    if upper.starts_with("COMPROBANTE DE PAGO") {
        return Ok(Some("WITHDRAWAL"));
    }
    if upper.starts_with("MOVIMIENTO MANUAL") {
        // Tax withholdings (retenciones IIGG, BBPP) affect the cash balance → FEE
        if upper.contains("RET ") || upper.contains("RETENCI") {
            return Ok(Some("FEE"));
        }
        return Ok(None);
    }
    if upper.starts_with("RENTA") || upper.starts_with("INTEREST PAYMENT") {
        return Ok(Some("INTEREST"));
    }
    if upper.starts_with("AMORTIZACI") {
        return Ok(Some("INTEREST"));
    }
    Ok(Some("UNKNOWN"))
}

fn map_balanz_currency(moneda: &str) -> String {
    let lower = moneda.to_lowercase();
    if lower.contains("peso") || lower == "ars" {
        "ARS".to_string()
    } else {
        "USD".to_string()
    }
}

fn parse_date_iso(s: &str) -> Option<String> {
    // Accepts "YYYY-MM-DD" or "DD/MM/YYYY"
    let s = s.trim();
    if s.len() == 10 && s.as_bytes()[4] == b'-' {
        return Some(s.to_string());
    }
    if s.len() == 10 && s.as_bytes()[2] == b'/' {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 3 {
            return Some(format!("{}-{:0>2}-{:0>2}", parts[2], parts[1], parts[0]));
        }
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

fn parse_decimal_abs(s: &str) -> Option<Decimal> {
    parse_decimal(s).map(|d| d.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(desc: &str, ticker: &str, tipo: &str, moneda: &str, qty: &str, precio: &str, importe: &str) -> Vec<String> {
        vec![
            desc.to_string(), ticker.to_string(), tipo.to_string(),
            "2026-05-04".to_string(), qty.to_string(), precio.to_string(),
            "2026-05-04".to_string(), moneda.to_string(), importe.to_string(),
        ]
    }

    #[test]
    fn cedear_usd_dividend_gets_xbue_and_ars_quote_ccy() {
        // Dividends paid in "Dólares C.V. 7000" must still be associated with the XBUE asset
        // and have quote_ccy=ARS so they share the same asset candidate key as ARS-denominated buys.
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Dividendo en efectivo / MO", "MO", "Cedears", "Dólares C.V. 7000", "0", "-1", "6.86"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        let a = &activities[0];
        assert_eq!(a.activity_type, "DIVIDEND");
        assert_eq!(a.currency, "USD");
        assert_eq!(a.exchange_mic.as_deref(), Some("XBUE"));
        assert_eq!(a.quote_ccy.as_deref(), Some("ARS"));
        assert_eq!(a.quote_mode, None);
    }

    #[test]
    fn fci_buy_gets_xbue_and_fund_name_from_liquidacion() {
        // FCIs on XBUE: exchange_mic=XBUE so PPI can price them via FONDOS type.
        // Fund name comes from the preceding "Liquidación de Suscripción" row.
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Liquidación de Suscripción / 743258 / BALANZ CAPITAL ACCIONES ARGENTINAS  A", "BCACC A", "Fondos", "Pesos", "3396.43", "152.78", "-518895.83"),
            row("Suscripción desde Balanz", "BCACCA", "Fondos", "Pesos", "3396.43", "152.78", "518895.83"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        let a = &activities[0];
        assert_eq!(a.activity_type, "BUY");
        assert_eq!(a.exchange_mic.as_deref(), Some("XBUE"));
        assert_eq!(a.quote_ccy.as_deref(), Some("ARS"));
        assert_eq!(a.quote_mode, None);
        assert_eq!(a.symbol_name.as_deref(), Some("BALANZ CAPITAL ACCIONES ARGENTINAS  A"));
    }

    #[test]
    fn movimiento_manual_ret_iigg_imported_as_fee() {
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Movimiento Manual / N/D Ret IIGG y BBPP - GGAL", "", "", "Pesos", "0", "-1", "-2454.86"),
            row("Movimiento Manual / N/D Ret IIGG - BYMA", "", "", "Dólares", "0", "-1", "-3.21"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 2);
        assert_eq!(activities[0].activity_type, "FEE");
        assert_eq!(activities[0].currency, "ARS");
        assert_eq!(activities[0].symbol, "");
        assert_eq!(activities[1].activity_type, "FEE");
        assert_eq!(activities[1].currency, "USD");
    }

    #[test]
    fn movimiento_manual_without_ret_is_skipped() {
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Movimiento Manual / Ajuste de cartera", "", "", "Pesos", "0", "-1", "100"),
        ];
        let (activities, _) = parse_balanz(&rows, None);
        assert!(activities.is_empty());
    }

    #[test]
    fn cedear_ars_buy_gets_xbue() {
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Boleto / 2079221 / COMPRA / 1 / CVX / $", "CVX", "Cedears", "Pesos", "58", "16920", "-986703.5"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty());
        assert_eq!(activities.len(), 1);
        let a = &activities[0];
        assert_eq!(a.activity_type, "BUY");
        assert_eq!(a.currency, "ARS");
        assert_eq!(a.exchange_mic.as_deref(), Some("XBUE"));
        assert_eq!(a.quote_ccy.as_deref(), Some("ARS"));
        assert_eq!(a.quote_mode, None);
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
    fn negative_renta_becomes_fee() {
        // RENTA (INTEREST) with negative importe is a withholding tax — must be FEE.
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Renta / AL35", "AL35", "Bonos", "Pesos", "0", "-1", "-420.50"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert_eq!(activities[0].activity_type, "FEE");
        assert_eq!(activities[0].amount, Some(Decimal::from_str("420.50").unwrap()));
    }

    fn caucion_row(desc: &str, liquidacion: &str, moneda: &str, importe: &str) -> Vec<String> {
        vec![
            desc.to_string(), String::new(), String::new(),
            "2026-05-22".to_string(), "0".to_string(), "-1".to_string(),
            liquidacion.to_string(), moneda.to_string(), importe.to_string(),
        ]
    }

    #[test]
    fn mep_venta_ars_commission_becomes_fee_not_sell() {
        // MEP / dólar-cable: el mismo boleto VENTA aparece dos veces:
        //   - una fila en ARS con precio=-1(sentinel) e importe negativo (comisión)
        //   - una fila en USD con precio real e importe positivo (cobro efectivo)
        // Sin este fix ambas se importan como SELL → posición corta en el bono.
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Boleto / 5937562 / VENTA / 0 / AL30 / usd", "AL30", "Bonos", "Pesos",    "-1121", "-1",     "-25.32"),
            row("Boleto / 5937562 / VENTA / 0 / AL30 / usd", "AL30", "Bonos", "Dólares",  "-1121", "0.6418", "716.58"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 2);
        // Fila con precio sentinel y comisión negativa → FEE, sin símbolo
        assert_eq!(activities[0].activity_type, "FEE");
        assert_eq!(activities[0].currency, "ARS");
        assert_eq!(activities[0].symbol, "");
        assert_eq!(activities[0].amount, Some(Decimal::from_str("25.32").unwrap()));
        // Fila con precio real → SELL legítima
        assert_eq!(activities[1].activity_type, "SELL");
        assert_eq!(activities[1].currency, "USD");
        assert_eq!(activities[1].symbol, "AL30");
    }

    #[test]
    fn caucion_apcolcon_is_skipped() {
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            caucion_row("Boleto / 5491004 / APCOLCON / 0 / $", "2026-05-18", "Pesos", "-1000000"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert!(activities.is_empty(), "APCOLCON debe ser ignorado");
    }

    #[test]
    fn caucion_apcolfut_produces_net_interest() {
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            caucion_row("Boleto / 5491004 / APCOLCON / 0 / $", "2026-05-18", "Pesos", "-1000000"),
            caucion_row("Boleto / 5491005 / APCOLFUT / 4 / $", "2026-05-22", "Pesos", "1001898.05"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        let a = &activities[0];
        assert_eq!(a.activity_type, "INTEREST");
        assert_eq!(a.currency, "ARS");
        assert_eq!(a.date, "2026-05-22"); // fecha de Liquidacion, no Concertacion
        assert_eq!(a.symbol, "");
        // interés neto = 1001898.05 - 1000000 = 1898.05
        let expected = Decimal::from_str("1898.05").unwrap();
        assert_eq!(a.amount, Some(expected), "debe ser el interés neto, no el importe completo");
    }

    #[test]
    fn caucion_apcolfut_without_pair_fallsback_to_full_amount() {
        // Solo APCOLFUT sin APCOLCON correspondiente → usa el importe completo como INTEREST
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            caucion_row("Boleto / 5491005 / APCOLFUT / 4 / $", "2026-05-22", "Pesos", "1001898.05"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
        assert_eq!(activities.len(), 1);
        let a = &activities[0];
        assert_eq!(a.activity_type, "INTEREST");
        let expected = Decimal::from_str("1001898.05").unwrap();
        assert_eq!(a.amount, Some(expected));
    }

    #[test]
    fn unknown_activity_marked_invalid_with_error() {
        let rows = vec![
            row("header", "Ticker", "Tipo de Instrumento", "Moneda", "Cantidad", "Precio", "Importe"),
            row("Tipo de movimiento completamente nuevo", "XYZ", "Acciones", "Pesos", "10", "100", "1000"),
        ];
        let (activities, errors) = parse_balanz(&rows, None);
        assert!(errors.is_empty(), "parse errors should be empty: {:?}", errors);
        assert_eq!(activities.len(), 1);
        assert!(!activities[0].is_valid, "UNKNOWN activity must be invalid");
        assert!(activities[0].errors.is_some(), "UNKNOWN activity must have errors");
    }
}
