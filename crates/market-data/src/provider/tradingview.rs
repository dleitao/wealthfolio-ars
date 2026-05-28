//! TradingView Argentina scanner — batch sector/country profiles for XBUE instruments.
//!
//! POST https://scanner.tradingview.com/argentina/scan
//! No auth required. Covers 991 instruments (stocks + CEDEARs/DRs).
//! Does NOT cover bonds or FCIs.

use std::collections::HashMap;

use log::debug;

use crate::models::AssetProfile;

const SCANNER_URL: &str = "https://scanner.tradingview.com/argentina/scan";

pub struct TradingViewArgentinaProvider;

impl TradingViewArgentinaProvider {
    /// Batch-fetch sector + country profiles for a list of XBUE symbols.
    ///
    /// Returns a map of symbol → AssetProfile. Symbols not found in the
    /// TradingView scanner (e.g., bonds, FCIs) are absent from the map.
    pub async fn fetch_profiles_batch(
        http: &reqwest::Client,
        symbols: &[&str],
    ) -> Result<HashMap<String, AssetProfile>, reqwest::Error> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }

        let symbols_json: Vec<serde_json::Value> = symbols
            .iter()
            .map(|s| serde_json::Value::String(s.to_string()))
            .collect();

        let body = serde_json::json!({
            "columns": ["name", "sector", "industry", "country"],
            "filter": [{"left": "name", "operation": "in_range", "right": symbols_json}],
            "range": [0, symbols.len()]
        });

        let data: serde_json::Value = http
            .post(SCANNER_URL)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        let mut result = HashMap::new();

        if let Some(items) = data["data"].as_array() {
            for item in items {
                let d = match item["d"].as_array() {
                    Some(d) => d,
                    None => continue,
                };

                let symbol = match d.first().and_then(|v| v.as_str()) {
                    Some(s) => s.to_string(),
                    None => continue,
                };

                let tv_sector = d.get(1).and_then(|v| v.as_str());
                let tv_country = d.get(3).and_then(|v| v.as_str());
                let mapped_sector = tv_sector.and_then(Self::map_sector).map(String::from);

                debug!(
                    "[TradingView] {} → sector={:?} country={:?}",
                    symbol, mapped_sector, tv_country
                );

                result.insert(
                    symbol,
                    AssetProfile {
                        source: Some("TRADINGVIEW".to_string()),
                        sector: mapped_sector,
                        country: tv_country.map(String::from),
                        ..Default::default()
                    },
                );
            }
        }

        Ok(result)
    }

    /// Maps TradingView sector names to Yahoo-compatible names so the existing
    /// `map_sector_to_gics` function produces the correct GICS category ID.
    fn map_sector(tv_sector: &str) -> Option<&'static str> {
        match tv_sector {
            "Finance" => Some("Financial Services"),
            "Electronic Technology" | "Technology Services" => Some("Information Technology"),
            "Health Technology" | "Health Services" => Some("Health Care"),
            "Energy Minerals" => Some("Energy"),
            "Non-Energy Minerals" | "Process Industries" => Some("Materials"),
            "Consumer Non-Durables" => Some("Consumer Staples"),
            "Consumer Durables" | "Retail Trade" | "Consumer Services" => {
                Some("Consumer Discretionary")
            }
            "Utilities" => Some("Utilities"),
            "Producer Manufacturing"
            | "Transportation"
            | "Commercial Services"
            | "Industrial Services"
            | "Distribution Services" => Some("Industrials"),
            "Communications" => Some("Communication Services"),
            // "Miscellaneous" (ETFs/investment trusts) → no mapping
            _ => None,
        }
    }
}
