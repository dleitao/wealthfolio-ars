//! Maps PPI API responses to the universal broker format.

use super::models::{PpiAvailabilityEntry, PpiInstrument, PpiMovement};
use crate::broker::{
    AccountUniversalActivity, AccountUniversalActivityCurrency, AccountUniversalActivitySymbol,
    HoldingsBalance, HoldingsCurrency, HoldingsInnerSymbol, HoldingsPosition, HoldingsSymbol,
};

pub fn map_ppi_activity(mv: PpiMovement) -> Option<AccountUniversalActivity> {
    let desc = mv.description.as_deref()?;
    let (activity_type, needs_review) = classify_movement(desc)?;

    // Retenciones ARS llegan como DIVIDEND/INTEREST con amount negativo — reclasificar como FEE.
    let is_negative = mv.amount.map(|a| a < 0.0).unwrap_or(false);
    let activity_type = if matches!(activity_type, "DIVIDEND" | "INTEREST") && is_negative {
        "FEE"
    } else {
        activity_type
    };

    let currency_code = map_currency(mv.currency.as_deref().unwrap_or("USD"));

    // PPI sometimes returns "Ticker not found" — extract from description instead.
    let ticker = resolve_ticker(mv.ticker.as_deref(), desc);

    Some(AccountUniversalActivity {
        id: None,
        symbol: ticker.map(|sym| AccountUniversalActivitySymbol {
            symbol: Some(sym.clone()),
            raw_symbol: Some(sym),
            currency: Some(AccountUniversalActivityCurrency {
                code: Some(currency_code.clone()),
                ..Default::default()
            }),
            ..Default::default()
        }),
        price: mv.price.filter(|&p| p > 0.0),
        units: mv.quantity.map(f64::abs),
        amount: mv.amount.map(f64::abs),
        currency: Some(AccountUniversalActivityCurrency {
            code: Some(currency_code),
            ..Default::default()
        }),
        activity_type: Some(activity_type.to_string()),
        raw_type: mv.description.clone(),
        trade_date: mv.agreement_date.clone(),
        settlement_date: mv.settlement_date.clone(),
        source_system: Some("PPI".to_string()),
        description: mv.description,
        needs_review,
        ..Default::default()
    })
}

/// Resolve the ticker symbol. PPI returns "Ticker not found" for some movements;
/// in those cases extract from the description (e.g. "Renta / AL30" → "AL30").
fn resolve_ticker(ticker: Option<&str>, desc: &str) -> Option<String> {
    match ticker {
        Some(t) if !t.is_empty() && t != "Ticker not found" => {
            Some(t.replace(' ', "")) // normalize FCI tickers
        }
        _ => extract_ticker_from_desc(desc),
    }
}

/// Extract ticker from description patterns like "Renta / TICKER" or "COMPRA TICKER".
fn extract_ticker_from_desc(desc: &str) -> Option<String> {
    // "Renta / AL30", "Amortización / AL30", "Dividendo / AAPL", etc.
    if let Some(pos) = desc.find(" / ") {
        let after = desc[pos + 3..].trim();
        let ticker = after.split_whitespace().next()?;
        if !ticker.is_empty() {
            return Some(ticker.replace(' ', ""));
        }
    }
    // "COMPRA GGAL", "VENTA MELI", etc.
    let upper = desc.to_uppercase();
    for prefix in &["COMPRA ", "VENTA "] {
        if upper.starts_with(prefix) {
            let ticker = desc[prefix.len()..].split_whitespace().next()?;
            return Some(ticker.to_string());
        }
    }
    None
}

fn classify_movement(desc: &str) -> Option<(&'static str, bool)> {
    let upper = desc.to_uppercase();

    if upper.starts_with("COMPRA") {
        return Some(("BUY", false));
    }
    if upper.starts_with("VENTA") {
        return Some(("SELL", false));
    }
    if upper.starts_with("DIVIDENDO") {
        return Some(("DIVIDEND", false));
    }
    if upper.starts_with("RENTA") || upper.starts_with("INTEREST PAYMENT") {
        return Some(("INTEREST", false));
    }
    if upper.starts_with("AMORTIZACI") {
        return Some(("INTEREST", false));
    }
    if upper.starts_with("LIQUIDACI") && upper.contains("RESCATE") {
        return Some(("SELL", false));
    }
    if upper.starts_with("INGRESO DE FONDOS") {
        return Some(("DEPOSIT", false));
    }
    if upper.starts_with("RETIRO DE FONDOS") {
        return Some(("WITHDRAWAL", false));
    }
    // FCI subscriptions
    if upper.starts_with("LIQUIDACI") && upper.contains("SUSCRIPCI") {
        return Some(("BUY", false));
    }
    // Cash blocks and FX conversions — operational noise, skip
    if upper.starts_with("BLOQUEO MONETARIO")
        || upper.starts_with("DESBLOQUEO MONETARIO")
        || upper.contains("MONEDAS NRO")
    {
        return None;
    }
    // Skip internal transfers and manual movements
    if upper.starts_with("MOVIMIENTO MANUAL") || upper.starts_with("CANJE") {
        return None;
    }

    Some(("UNKNOWN", true))
}

pub fn map_currency(currency: &str) -> String {
    let lower = currency.to_lowercase();
    if lower.contains("peso") || lower == "ars" {
        "ARS".to_string()
    } else {
        "USD".to_string()
    }
}

pub fn map_ppi_holding(instrument: PpiInstrument) -> Option<HoldingsPosition> {
    let ticker = instrument.ticker.filter(|s| !s.is_empty())?;
    let quantity = instrument.quantity.filter(|&q| q != 0.0)?;
    let currency_code = map_currency(instrument.currency.as_deref().unwrap_or("ARS"));

    Some(HoldingsPosition {
        symbol: Some(HoldingsSymbol {
            symbol: Some(HoldingsInnerSymbol {
                symbol: Some(ticker.clone()),
                raw_symbol: Some(ticker),
                currency: Some(HoldingsCurrency {
                    code: Some(currency_code),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        units: Some(quantity),
        price: instrument.price,
        ..Default::default()
    })
}

pub fn map_ppi_cash(entry: PpiAvailabilityEntry) -> HoldingsBalance {
    let currency_code = entry.symbol.as_deref().unwrap_or("ARS").to_string();
    let amount = entry.amount;
    HoldingsBalance {
        currency: Some(HoldingsCurrency {
            code: Some(currency_code),
            ..Default::default()
        }),
        cash: amount,
        buying_power: amount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::models::PpiMovement;

    #[test]
    fn fci_subscription_classified_as_buy() {
        assert_eq!(
            classify_movement("Liquidación de Suscripción / 663951 / ALP.LAT.K"),
            Some(("BUY", false))
        );
    }

    #[test]
    fn bloqueo_desbloqueo_monedas_skipped() {
        assert_eq!(
            classify_movement("Bloqueo Monetario por Solicitud de Suscripción de FCI ALP.LAT.K"),
            None
        );
        assert_eq!(
            classify_movement("Desbloqueo Monetario por Liquidación de Suscripción / 663951 / ALP.LAT.K"),
            None
        );
        assert_eq!(
            classify_movement("Débito/Crédito de Monedas Nro. 126262 / 16/4/2024"),
            None
        );
    }

    #[test]
    fn negative_dividend_becomes_fee() {
        let mv = PpiMovement {
            description: Some("Dividendo en efectivo / AAPL".to_string()),
            amount: Some(-12.09),
            ..Default::default()
        };
        let result = map_ppi_activity(mv).unwrap();
        assert_eq!(result.activity_type.as_deref(), Some("FEE"));
        assert_eq!(result.amount, Some(12.09));
    }

    #[test]
    fn negative_interest_becomes_fee() {
        let mv = PpiMovement {
            description: Some("Renta / AL30".to_string()),
            amount: Some(-133.33),
            ..Default::default()
        };
        let result = map_ppi_activity(mv).unwrap();
        assert_eq!(result.activity_type.as_deref(), Some("FEE"));
    }

    #[test]
    fn positive_dividend_stays_dividend() {
        let mv = PpiMovement {
            description: Some("Dividendo en efectivo / AAPL".to_string()),
            amount: Some(0.53),
            ..Default::default()
        };
        let result = map_ppi_activity(mv).unwrap();
        assert_eq!(result.activity_type.as_deref(), Some("DIVIDEND"));
    }
}
