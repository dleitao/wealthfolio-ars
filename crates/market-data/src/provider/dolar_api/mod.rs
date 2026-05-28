//! DolarApi provider for real-time Argentine FX rates.
//!
//! Fetches current ARS exchange rates from https://dolarapi.com.
//! No authentication required. Supports latest quotes only (no historical data).

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::errors::MarketDataError;
use crate::models::{Coverage, InstrumentKind, ProviderInstrument, Quote, QuoteContext};
use crate::provider::{MarketDataProvider, ProviderCapabilities, RateLimit};

const PROVIDER_ID: &str = "DOLAR_API";
const BASE_URL: &str = "https://dolarapi.com/v1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
struct DolarApiResponse {
    compra: Option<f64>,
    venta: Option<f64>,
}

/// DolarApi provider for real-time ARS exchange rates.
///
/// Supports all Argentine FX pseudo-currencies as `InstrumentId::Fx { base: "ARS_XXX", quote: "USD" }`:
/// - `ARS_OFICIAL` — official BNA rate
/// - `ARS_MEP` — MEP/bolsa rate
/// - `ARS_CCL` — contado con liquidación
/// - `ARS_BLUE` — informal/blue rate
/// - `ARS_MAYORISTA` — wholesale rate
/// - `ARS_TARJETA` — card rate (oficial + taxes)
/// - `ARS_CRIPTO` — USDT/crypto implied rate
pub struct DolarApiProvider {
    client: Client,
}

impl DolarApiProvider {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    fn endpoint_for(base: &str) -> Option<&'static str> {
        match base {
            "ARS_OFICIAL" => Some("oficial"),
            "ARS_MEP" => Some("bolsa"),
            "ARS_CCL" => Some("contadoconliqui"),
            "ARS_BLUE" => Some("blue"),
            "ARS_MAYORISTA" => Some("mayorista"),
            "ARS_TARJETA" => Some("tarjeta"),
            "ARS_CRIPTO" => Some("cripto"),
            _ => None,
        }
    }
}

impl Default for DolarApiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataProvider for DolarApiProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            instrument_kinds: &[InstrumentKind::Fx],
            coverage: Coverage::global_best_effort(),
            supports_latest: true,
            supports_historical: false,
            supports_search: false,
            supports_profile: false,
        }
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit {
            requests_per_minute: 60,
            max_concurrency: 4,
            min_delay: Duration::from_millis(100),
        }
    }

    async fn get_latest_quote(
        &self,
        _context: &QuoteContext,
        instrument: ProviderInstrument,
    ) -> Result<Quote, MarketDataError> {
        let endpoint = match &instrument {
            ProviderInstrument::FxPair { from, .. } => {
                Self::endpoint_for(from.as_ref()).ok_or_else(|| MarketDataError::NotSupported {
                    operation: format!("fx base {}", from),
                    provider: PROVIDER_ID.to_string(),
                })?
            }
            _ => {
                return Err(MarketDataError::NotSupported {
                    operation: "non-fx instrument".to_string(),
                    provider: PROVIDER_ID.to_string(),
                })
            }
        };

        let url = format!("{}/dolares/{}", BASE_URL, endpoint);
        let resp: DolarApiResponse = self
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

        let rate = resp.venta.or(resp.compra).ok_or_else(|| {
            MarketDataError::ProviderError {
                provider: PROVIDER_ID.to_string(),
                message: "No rate in response".to_string(),
            }
        })?;

        let close = Decimal::try_from(rate).map_err(|e| MarketDataError::ProviderError {
            provider: PROVIDER_ID.to_string(),
            message: format!("Invalid rate value: {}", e),
        })?;

        Ok(Quote::new(
            Utc::now(),
            close,
            "USD".to_string(),
            PROVIDER_ID.to_string(),
        ))
    }

    async fn get_historical_quotes(
        &self,
        _context: &QuoteContext,
        _instrument: ProviderInstrument,
        _start: DateTime<Utc>,
        _end: DateTime<Utc>,
    ) -> Result<Vec<Quote>, MarketDataError> {
        Err(MarketDataError::NotSupported {
            operation: "historical_quotes".to_string(),
            provider: PROVIDER_ID.to_string(),
        })
    }
}
