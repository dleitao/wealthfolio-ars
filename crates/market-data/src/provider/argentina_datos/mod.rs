//! ArgentinaDatos market data provider.
//!
//! Provides historical FX rates (ARS oficial, MEP) and inflation data
//! from the public ArgentinaDatos API (https://api.argentinadatos.com).
//! No authentication required. Rate limit: ~30 rpm.

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::warn;

use crate::errors::MarketDataError;
use crate::models::{Coverage, InstrumentKind, ProviderInstrument, Quote, QuoteContext};
use crate::provider::{MarketDataProvider, ProviderCapabilities, RateLimit};

const PROVIDER_ID: &str = "ARGENTINA_DATOS";
const BASE_URL: &str = "https://api.argentinadatos.com/v1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize)]
struct DolarQuote {
    fecha: String,
    compra: Option<f64>,
    venta: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct InflationPoint {
    pub fecha: String,
    pub valor: f64,
}

/// ArgentinaDatos provider for historical ARS exchange rates and inflation data.
///
/// Supports all Argentine FX pseudo-currencies as `InstrumentId::Fx { base: "ARS_XXX", quote: "USD" }`:
/// - `ARS_OFICIAL` — official BNA rate
/// - `ARS_MEP` — MEP rate
/// - `ARS_CCL` — contado con liquidación
/// - `ARS_BLUE` — informal/blue rate
/// - `ARS_MAYORISTA` — wholesale rate
/// - `ARS_TARJETA` — card rate
/// - `ARS_CRIPTO` — USDT/crypto implied rate
pub struct ArgentinaDatosProvider {
    client: Client,
}

impl ArgentinaDatosProvider {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    fn casa_for(base: &str) -> Option<&'static str> {
        match base {
            "ARS_OFICIAL" => Some("oficial"),
            "ARS_MEP" => Some("mep"),
            "ARS_CCL" => Some("ccl"),
            "ARS_BLUE" => Some("blue"),
            "ARS_MAYORISTA" => Some("mayorista"),
            "ARS_TARJETA" => Some("tarjeta"),
            "ARS_CRIPTO" => Some("cripto"),
            _ => None,
        }
    }

    /// Fetch historical inflation data (monthly IPC).
    ///
    /// Returns a vector of `(period, monthly_rate)` where period is "YYYY-MM".
    pub async fn get_inflation(&self) -> Result<Vec<InflationPoint>, MarketDataError> {
        let url = format!("{}/finanzas/indices/inflacion", BASE_URL);
        let points: Vec<InflationPoint> = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("Inflation request failed: {}", e),
            })?
            .error_for_status()
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("Inflation HTTP error: {}", e),
            })?
            .json()
            .await
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("Inflation parse error: {}", e),
            })?;
        Ok(points)
    }
}

impl Default for ArgentinaDatosProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for ArgentinaDatosProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            instrument_kinds: &[InstrumentKind::Fx],
            coverage: Coverage::global_best_effort(),
            supports_latest: false,
            supports_historical: true,
            supports_search: false,
            supports_profile: false,
            supports_dividends: false,
        }
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit {
            requests_per_minute: 30,
            max_concurrency: 2,
            min_delay: Duration::from_millis(200),
        }
    }

    async fn get_latest_quote(
        &self,
        _context: &QuoteContext,
        _instrument: ProviderInstrument,
    ) -> Result<Quote, MarketDataError> {
        Err(MarketDataError::NotSupported {
            operation: "latest_quote".to_string(),
            provider: PROVIDER_ID.to_string(),
        })
    }

    async fn get_historical_quotes(
        &self,
        _context: &QuoteContext,
        instrument: ProviderInstrument,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Quote>, MarketDataError> {
        let (from_currency, casa) = match &instrument {
            ProviderInstrument::FxPair { from, .. } => {
                let casa = Self::casa_for(from.as_ref()).ok_or_else(|| {
                    MarketDataError::NotSupported {
                        operation: format!("fx base {}", from),
                        provider: PROVIDER_ID.to_string(),
                    }
                })?;
                (from.to_string(), casa)
            }
            _ => {
                return Err(MarketDataError::NotSupported {
                    operation: "non-fx instrument".to_string(),
                    provider: PROVIDER_ID.to_string(),
                })
            }
        };

        let url = format!("{}/cotizaciones/dolares/{}", BASE_URL, casa);
        let raw: Vec<DolarQuote> = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("Request failed: {}", e),
            })?
            .error_for_status()
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("HTTP error: {}", e),
            })?
            .json()
            .await
            .map_err(|e| MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: format!("Parse error: {}", e),
            })?;

        let quotes = raw
            .into_iter()
            .filter_map(|q| {
                let date = NaiveDate::parse_from_str(&q.fecha, "%Y-%m-%d").ok()?;
                let dt = date.and_hms_opt(0, 0, 0)?.and_utc();
                if dt < start || dt > end {
                    return None;
                }
                // Use venta (sell rate) as the close price; fall back to compra
                let rate = q.venta.or(q.compra)?;
                let close = Decimal::try_from(rate).ok()?;
                if close.is_zero() {
                    warn!(
                        provider = PROVIDER_ID,
                        currency = from_currency,
                        date = %date,
                        "Skipping zero-rate quote"
                    );
                    return None;
                }
                Some(Quote::new(dt, close, "USD".to_string(), PROVIDER_ID.to_string()))
            })
            .collect();

        Ok(quotes)
    }
}
