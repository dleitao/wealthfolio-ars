//! PPI (Portfolio Personal Inversiones) market data provider.
//!
//! Provides real-time and historical quotes for BYMA instruments.
//! Auth: JWT Bearer obtained via login with 4 headers (ApiKey, ApiSecret,
//! AuthorizedClient, ClientKey). Sensitive credentials (ApiKey, ApiSecret)
//! are fetched from the keyring on each login via `CredFetcher`; the
//! application identifiers (AuthorizedClient, ClientKey) are stored as fields.
//!
//! API: https://itatppi.github.io/ppi-official-api-docs/api/documentacionRest/

use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use crate::errors::MarketDataError;
use crate::models::{Coverage, InstrumentKind, ProviderInstrument, Quote, QuoteContext, SearchResult};
use crate::provider::{MarketDataProvider, ProviderCapabilities, RateLimit};

const PROVIDER_ID: &str = "PPI";
const BASE_URL: &str = "https://clientapi.portfoliopersonal.com";
const API_VERSION: &str = "1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SETTLEMENT: &str = "INMEDIATA";
// PPI instrument types tried in order until one succeeds
const INSTRUMENT_TYPES: &[&str] = &["CEDEARS", "BONOS", "ACCIONES", "LETRAS", "FONDOS"];

// ── Auth models ──────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct RefreshRequest {
    #[serde(rename = "refreshToken")]
    refresh_token: String,
}

// ── Market data models ───────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PpiQuote {
    date: Option<String>,
    price: Option<f64>,
    #[serde(rename = "openingPrice")]
    opening_price: Option<f64>,
    max: Option<f64>,
    min: Option<f64>,
    #[serde(rename = "volumeAmount")]
    volume_amount: Option<f64>,
}

#[derive(Deserialize)]
struct PpiSearchEntry {
    ticker: Option<String>,
    description: Option<String>,
    #[serde(rename = "type")]
    instrument_type: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

// ── Provider ─────────────────────────────────────────────────────────────────

/// Fetches sensitive login credentials (ApiKey + ApiSecret) from the keyring.
/// Called only on login/re-login, never stored in the struct.
type CredFetcher =
    Box<dyn Fn() -> Result<(String, String), MarketDataError> + Send + Sync>;

pub struct PpiMarketDataProvider {
    client: Client,
    cred_fetcher: CredFetcher,
    authorized_client: String,
    client_key: String,
    token: RwLock<Option<String>>,
    refresh_token: RwLock<Option<String>>,
    refresh_lock: Mutex<()>,
}

impl PpiMarketDataProvider {
    pub fn new(
        cred_fetcher: CredFetcher,
        authorized_client: String,
        client_key: String,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("Failed to build HTTP client"),
            cred_fetcher,
            authorized_client,
            client_key,
            token: RwLock::new(None),
            refresh_token: RwLock::new(None),
            refresh_lock: Mutex::new(()),
        }
    }

    fn api_url(&self, path: &str) -> String {
        format!("{}/api/{}/{}", BASE_URL, API_VERSION, path)
    }

    fn auth_headers(&self, token: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token)).expect("Invalid token"),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "AuthorizedClient",
            HeaderValue::from_str(&self.authorized_client).expect("Invalid AuthorizedClient"),
        );
        headers.insert(
            "ClientKey",
            HeaderValue::from_str(&self.client_key).expect("Invalid ClientKey"),
        );
        headers
    }

    async fn get_token(&self) -> Result<String, MarketDataError> {
        {
            let t = self.token.read().await;
            if let Some(ref tok) = *t {
                return Ok(tok.clone());
            }
        }
        let _guard = self.refresh_lock.lock().await;
        {
            let t = self.token.read().await;
            if let Some(ref tok) = *t {
                return Ok(tok.clone());
            }
        }
        let refresh = self.refresh_token.read().await.clone();
        if let Some(rt) = refresh {
            match self.do_refresh(&rt).await {
                Ok(tok) => return Ok(tok),
                Err(e) => warn!("[PPI] Refresh failed, re-logging in: {}", e),
            }
        }
        self.do_login().await
    }

    async fn do_login(&self) -> Result<String, MarketDataError> {
        let (api_key, api_secret) = (self.cred_fetcher)()?;
        let url = self.api_url("Account/LoginApi");
        let resp: LoginResponse = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("AuthorizedClient", &self.authorized_client)
            .header("ClientKey", &self.client_key)
            .header("ApiKey", &api_key)
            .header("ApiSecret", &api_secret)
            .body("{}")
            .send()
            .await
            .map_err(|e| ppi_err(format!("Login failed: {}", e)))?
            .error_for_status()
            .map_err(|e| ppi_err(format!("Login HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| ppi_err(format!("Login parse error: {}", e)))?;

        let access = resp
            .access_token
            .ok_or_else(|| ppi_err("Login returned no access_token".to_string()))?;
        if let Some(rt) = resp.refresh_token {
            *self.refresh_token.write().await = Some(rt);
        }
        *self.token.write().await = Some(access.clone());
        info!("[PPI] Login successful");
        Ok(access)
    }

    async fn do_refresh(&self, refresh_token: &str) -> Result<String, MarketDataError> {
        let url = self.api_url("Account/RefreshToken");
        let body = RefreshRequest { refresh_token: refresh_token.to_string() };
        let resp: LoginResponse = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("AuthorizedClient", &self.authorized_client)
            .header("ClientKey", &self.client_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ppi_err(format!("Refresh failed: {}", e)))?
            .error_for_status()
            .map_err(|e| ppi_err(format!("Refresh HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| ppi_err(format!("Refresh parse error: {}", e)))?;

        let access = resp
            .access_token
            .ok_or_else(|| ppi_err("Refresh returned no access_token".to_string()))?;
        if let Some(rt) = resp.refresh_token {
            *self.refresh_token.write().await = Some(rt);
        }
        *self.token.write().await = Some(access.clone());
        debug!("[PPI] Token refreshed");
        Ok(access)
    }

    async fn invalidate_token(&self) {
        *self.token.write().await = None;
    }

    /// Try instrument types in order, returning the first successful price and the matched type.
    /// Retries once with a fresh token on 401.
    async fn fetch_current(&self, ticker: &str) -> Result<(PpiQuote, &'static str), MarketDataError> {
        let url = self.api_url("MarketData/Current");
        let mut retried = false;

        'token_loop: loop {
            let token = self.get_token().await?;
            let mut last_err = ppi_err(format!("No type matched for {}", ticker));

            for &instrument_type in INSTRUMENT_TYPES {
                let resp = self
                    .client
                    .get(&url)
                    .headers(self.auth_headers(&token))
                    .query(&[("Ticker", ticker), ("Type", instrument_type), ("Settlement", SETTLEMENT)])
                    .send()
                    .await
                    .map_err(|e| ppi_err(format!("Current price request failed: {}", e)))?;

                if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
                    self.invalidate_token().await;
                    if !retried {
                        retried = true;
                        warn!("[PPI] 401 on current price for {}; re-logging in and retrying", ticker);
                        continue 'token_loop;
                    }
                    return Err(ppi_err("Unauthorized after token refresh".to_string()));
                }

                let body: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| ppi_err(format!("Current price parse error: {}", e)))?;

                // PPI returns {"error": "..."} or {"date": ..., "price": ...}
                if body.get("error").is_none() {
                    let quote = serde_json::from_value(body)
                        .map_err(|e| ppi_err(format!("Current price deserialize error: {}", e)))?;
                    return Ok((quote, instrument_type));
                }
                last_err = ppi_err(body["error"].as_str().unwrap_or("unknown error").to_string());
            }

            return Err(last_err);
        }
    }

    /// Retries once with a fresh token on 401.
    async fn fetch_historical(
        &self,
        ticker: &str,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<(Vec<PpiQuote>, &'static str), MarketDataError> {
        let url = self.api_url("MarketData/Search");
        let date_from = from.format("%Y-%m-%dT00:00:00").to_string();
        let date_to = to.format("%Y-%m-%dT23:59:59").to_string();
        let mut retried = false;

        'token_loop: loop {
            let token = self.get_token().await?;
            let mut last_err = ppi_err(format!("No type matched for {}", ticker));

            for &instrument_type in INSTRUMENT_TYPES {
                let resp = self
                    .client
                    .get(&url)
                    .headers(self.auth_headers(&token))
                    .query(&[
                        ("Ticker", ticker),
                        ("Type", instrument_type),
                        ("Settlement", SETTLEMENT),
                        ("DateFrom", &date_from),
                        ("DateTo", &date_to),
                    ])
                    .send()
                    .await
                    .map_err(|e| ppi_err(format!("Historical request failed: {}", e)))?;

                if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
                    self.invalidate_token().await;
                    if !retried {
                        retried = true;
                        warn!("[PPI] 401 on historical for {}; re-logging in and retrying", ticker);
                        continue 'token_loop;
                    }
                    return Err(ppi_err("Unauthorized after token refresh".to_string()));
                }

                let text = resp
                    .text()
                    .await
                    .map_err(|e| ppi_err(format!("Historical read error: {}", e)))?;

                let body: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => {
                        // Non-JSON response (e.g. HTML error page) — try next instrument type
                        last_err = ppi_err(format!("Historical parse error: not JSON for type={}", instrument_type));
                        continue;
                    }
                };

                // Error response is an object {"error": "..."}, success is an array [...]
                if body.is_array() {
                    let quotes = serde_json::from_value(body)
                        .map_err(|e| ppi_err(format!("Historical deserialize error: {}", e)))?;
                    return Ok((quotes, instrument_type));
                }
                if let Some(err) = body.get("error") {
                    last_err = ppi_err(err.as_str().unwrap_or("unknown error").to_string());
                }
            }

            return Err(last_err);
        }
    }
}

fn ppi_err(message: String) -> MarketDataError {
    MarketDataError::ProviderError {
        provider: PROVIDER_ID.to_string(),
        message,
    }
}

/// PPI quotes bonds "per 100 nominal face value" — divide by 100 to get price per unit.
fn bond_price_factor(instrument_type: &str) -> Decimal {
    if instrument_type == "BONOS" || instrument_type == "LETRAS" {
        Decimal::from(100)
    } else {
        Decimal::ONE
    }
}

fn ppi_quote_to_market(ppi: PpiQuote, currency: &str, timestamp: DateTime<Utc>, factor: Decimal) -> Option<Quote> {
    let raw_close = Decimal::try_from(ppi.price.filter(|&p| p > 0.0)?).ok()?;
    let close = raw_close / factor;
    let mut q = Quote::new(timestamp, close, currency.to_string(), PROVIDER_ID.to_string());
    q.open = ppi.opening_price.and_then(|v| Decimal::try_from(v).ok()).map(|p| p / factor);
    q.high = ppi.max.and_then(|v| Decimal::try_from(v).ok()).map(|p| p / factor);
    q.low = ppi.min.and_then(|v| Decimal::try_from(v).ok()).map(|p| p / factor);
    q.volume = ppi.volume_amount.and_then(|v| Decimal::try_from(v).ok());
    Some(q)
}

#[async_trait]
impl MarketDataProvider for PpiMarketDataProvider {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    fn priority(&self) -> u8 {
        5
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            instrument_kinds: &[InstrumentKind::Equity, InstrumentKind::Bond],
            coverage: Coverage::argentina_only(),
            supports_latest: true,
            supports_historical: true,
            supports_search: true,
            supports_profile: false,
            supports_dividends: false,
        }
    }

    fn rate_limit(&self) -> RateLimit {
        RateLimit {
            requests_per_minute: 60,
            max_concurrency: 3,
            min_delay: Duration::from_millis(200),
        }
    }

    async fn get_latest_quote(
        &self,
        context: &QuoteContext,
        instrument: ProviderInstrument,
    ) -> Result<Quote, MarketDataError> {
        let ticker = instrument.to_symbol_string();
        let currency = context.currency_hint.as_deref().unwrap_or("ARS").to_string();

        let (ppi, instrument_type) = self.fetch_current(&ticker).await?;
        let factor = bond_price_factor(instrument_type);
        ppi_quote_to_market(ppi, &currency, Utc::now(), factor)
            .ok_or_else(|| ppi_err(format!("No valid price returned for {}", ticker)))
    }

    async fn get_historical_quotes(
        &self,
        context: &QuoteContext,
        instrument: ProviderInstrument,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Quote>, MarketDataError> {
        let ticker = instrument.to_symbol_string();
        let currency = context.currency_hint.as_deref().unwrap_or("ARS").to_string();

        let (records, instrument_type) = self
            .fetch_historical(&ticker, start.date_naive(), end.date_naive())
            .await?;
        let factor = bond_price_factor(instrument_type);

        let quotes = records
            .into_iter()
            .filter_map(|ppi| {
                let ts = ppi
                    .date
                    .as_deref()
                    .and_then(|d| {
                        NaiveDate::parse_from_str(d, "%Y-%m-%dT%H:%M:%S")
                            .or_else(|_| NaiveDate::parse_from_str(d, "%Y-%m-%d"))
                            .ok()
                    })
                    .and_then(|d| d.and_hms_opt(17, 0, 0))
                    .map(|dt| dt.and_utc())
                    .unwrap_or_else(Utc::now);
                ppi_quote_to_market(ppi, &currency, ts, factor)
            })
            .collect();

        Ok(quotes)
    }

    async fn search(&self, query: &str) -> Result<Vec<SearchResult>, MarketDataError> {
        let token = self.get_token().await?;
        let url = self.api_url("MarketData/SearchInstrument");

        let results: Vec<PpiSearchEntry> = self
            .client
            .get(&url)
            .headers(self.auth_headers(&token))
            .query(&[("ticker", query)])
            .send()
            .await
            .map_err(|e| ppi_err(format!("Search request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| ppi_err(format!("Search HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| ppi_err(format!("Search parse error: {}", e)))?;

        let hits = results
            .into_iter()
            .filter_map(|r| {
                let symbol = r.ticker?;
                let name = r.description.unwrap_or_else(|| symbol.clone());
                let asset_type = r.instrument_type.unwrap_or_else(|| "EQUITY".to_string());
                Some(
                    SearchResult::new(symbol, name, "BCBA", asset_type)
                        .with_currency("ARS")
                        .with_exchange_mic("XBUE")
                        .with_data_source(PROVIDER_ID),
                )
            })
            .collect();

        Ok(hits)
    }
}
