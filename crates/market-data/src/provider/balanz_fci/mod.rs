//! Balanz FCI market data provider.
//!
//! Fetches daily VCP (valor de cuotaparte) for Balanz Capital mutual funds
//! from the public Balanz website API (https://balanz.com/api-web).
//! No authentication required.
//!
//! Supported tickers (Balanz internal codes with class suffix):
//! BCACCA, BCAHA, INSTITUA, OPPORA.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, REFERER, USER_AGENT};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::errors::MarketDataError;
use crate::models::{Coverage, InstrumentKind, ProviderInstrument, Quote, QuoteContext};
use crate::provider::{MarketDataProvider, ProviderCapabilities, RateLimit};

const PROVIDER_ID: &str = "BALANZ_FCI";
const BASE_URL: &str = "https://balanz.com/api-web/v1/funds";
const REFERER_URL: &str = "https://balanz.com/inversiones/fondos/rentabilidades-fci/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

// Maps Balanz internal ticker codes (with class suffix) → CodFondo ID used by the API.
fn cod_fondo_for(ticker: &str) -> Option<u32> {
    match ticker {
        "BCACCA"   => Some(7),  // Fondo Acciones Clase A
        "BCAHA"    => Some(3),  // Fondo Ahorro Clase A
        "INSTITUA" => Some(11), // Fondo Institucional (Inflation Linked) Clase A
        "OPPORA"   => Some(12), // Fondo Renta Fija Opportunity Clase A
        _ => None,
    }
}

// ── API models ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct HistoryRequest {
    id: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HistoryEntry {
    fecha: String,
    valor_cuotaparte: f64,
}

// ── Provider ─────────────────────────────────────────────────────────────────

pub struct BalanzFciProvider {
    client: Client,
}

impl BalanzFciProvider {
    pub fn new() -> Self {
        let mut default_headers = HeaderMap::new();
        default_headers.insert(
            USER_AGENT,
            HeaderValue::from_static(
                "Mozilla/5.0 (X11; Linux x86_64; rv:126.0) Gecko/20100101 Firefox/126.0",
            ),
        );
        default_headers.insert(ACCEPT, HeaderValue::from_static("application/json, text/plain, */*"));
        default_headers.insert(REFERER, HeaderValue::from_static(REFERER_URL));
        default_headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .default_headers(default_headers)
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    async fn fetch_history(&self, cod_fondo: u32) -> Result<Vec<HistoryEntry>, MarketDataError> {
        let url = format!("{}/history", BASE_URL);
        let body = HistoryRequest { id: cod_fondo };

        self.client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("History request failed: {}", e),
            })?
            .error_for_status()
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("History HTTP error: {}", e),
            })?
            .json()
            .await
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("History parse error: {}", e),
            })
    }
}

impl Default for BalanzFciProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn entry_to_quote(entry: HistoryEntry, currency: &str) -> Option<Quote> {
    let date = NaiveDate::parse_from_str(
        entry.fecha.get(..10).unwrap_or(&entry.fecha),
        "%Y-%m-%d",
    )
    .ok()?;
    let ts = date.and_hms_opt(17, 0, 0)?.and_utc();
    let close = Decimal::try_from(entry.valor_cuotaparte).ok()?;
    if close.is_zero() || entry.valor_cuotaparte <= 0.0 {
        return None;
    }
    Some(Quote::new(ts, close, currency.to_string(), PROVIDER_ID.to_string()))
}

#[async_trait]
impl MarketDataProvider for BalanzFciProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn priority(&self) -> u8 {
        6 // Lower priority than PPI (5); PPI is tried first for XBUE instruments.
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            instrument_kinds: &[InstrumentKind::Equity],
            coverage: Coverage::argentina_only(),
            supports_latest: true,
            supports_historical: true,
            supports_search: false,
            supports_profile: false,
        }
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit {
            requests_per_minute: 30,
            max_concurrency: 2,
            min_delay: Duration::from_millis(500),
        }
    }

    async fn get_latest_quote(
        &self,
        context: &QuoteContext,
        instrument: ProviderInstrument,
    ) -> Result<Quote, MarketDataError> {
        let ticker = instrument.to_symbol_string();
        let cod_fondo = cod_fondo_for(&ticker).ok_or_else(|| MarketDataError::NotSupported {
            operation: format!("ticker {}", ticker),
            provider: PROVIDER_ID.to_string(),
        })?;
        let currency = context.currency_hint.as_deref().unwrap_or("ARS");

        let entries = self.fetch_history(cod_fondo).await?;
        entries
            .into_iter()
            .rev()
            .find_map(|e| entry_to_quote(e, currency))
            .ok_or_else(|| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("No valid VCP for {}", ticker),
            })
    }

    async fn get_historical_quotes(
        &self,
        context: &QuoteContext,
        instrument: ProviderInstrument,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Quote>, MarketDataError> {
        let ticker = instrument.to_symbol_string();
        let cod_fondo = cod_fondo_for(&ticker).ok_or_else(|| MarketDataError::NotSupported {
            operation: format!("ticker {}", ticker),
            provider: PROVIDER_ID.to_string(),
        })?;
        let currency = context.currency_hint.as_deref().unwrap_or("ARS");

        let entries = self.fetch_history(cod_fondo).await?;
        let quotes = entries
            .into_iter()
            .filter_map(|e| {
                let q = entry_to_quote(e, currency)?;
                if q.timestamp >= start && q.timestamp <= end {
                    Some(q)
                } else {
                    None
                }
            })
            .collect();

        Ok(quotes)
    }
}
