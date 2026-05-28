//! Portfolio Personal (PPI) broker integration.
//!
//! Implements `BrokerApiClient` for Argentina's PPI brokerage.
//! API docs: https://itatppi.github.io/ppi-official-api-docs/api/documentacionRest/

mod mapper;
mod models;

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::{debug, info, warn};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use tokio::sync::{Mutex, RwLock};

use models::{
    PpiAccount, PpiLoginResponse, PpiMovement, PpiPortfolioResponse, PpiRefreshRequest,
};

use crate::broker::{
    BrokerAccount, BrokerApiClient, BrokerBrokerage, BrokerConnection, BrokerConnectionBrokerage,
    BrokerHoldingsResponse, PaginatedUniversalActivity, PaginationDetails,
};
use wealthfolio_core::errors::{Error, Result};
use wealthfolio_core::secrets::SecretStore;

pub use mapper::{map_ppi_activity, map_ppi_cash, map_ppi_holding};

const PPI_API_KEY_SECRET: &str = "ppi_api_key";
const PPI_API_SECRET_SECRET: &str = "ppi_api_secret";
const PPI_REFRESH_TOKEN_SECRET: &str = "ppi_refresh_token";

const BASE_URL: &str = "https://clientapi.portfoliopersonal.com";
const API_VERSION: &str = "1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

const PPI_CONNECTION_ID: &str = "PPI_CONNECTION";
const PPI_BROKERAGE_SLUG: &str = "PPI";
const PPI_BROKERAGE_NAME: &str = "Portfolio Personal Inversiones";

#[derive(Debug, Clone)]
struct CachedToken {
    access_token: String,
}

pub struct PpiApiClient {
    client: Client,
    secret_store: Arc<dyn SecretStore>,
    authorized_client: String,
    client_key: String,
    token_cache: RwLock<Option<CachedToken>>,
    refresh_lock: Mutex<()>,
}

impl PpiApiClient {
    pub fn new(
        secret_store: Arc<dyn SecretStore>,
        authorized_client: String,
        client_key: String,
    ) -> Self {
        Self {
            client: Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("Failed to build HTTP client"),
            secret_store,
            authorized_client,
            client_key,
            token_cache: RwLock::new(None),
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
            HeaderValue::from_str(&format!("Bearer {}", token))
                .expect("Invalid token for header"),
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

    async fn get_access_token(&self) -> Result<String> {
        {
            let cache = self.token_cache.read().await;
            if let Some(ref t) = *cache {
                return Ok(t.access_token.clone());
            }
        }

        let _guard = self.refresh_lock.lock().await;

        {
            let cache = self.token_cache.read().await;
            if let Some(ref t) = *cache {
                return Ok(t.access_token.clone());
            }
        }

        if let Ok(Some(refresh_token)) = self.secret_store.get_secret(PPI_REFRESH_TOKEN_SECRET) {
            match self.refresh_access_token(&refresh_token).await {
                Ok(token) => return Ok(token),
                Err(e) => warn!("PPI refresh token failed, re-logging in: {}", e),
            }
        }

        self.login().await
    }

    async fn login(&self) -> Result<String> {
        let api_key = self
            .secret_store
            .get_secret(PPI_API_KEY_SECRET)?
            .ok_or_else(|| Error::Unexpected("PPI API key not configured".to_string()))?;
        let api_secret = self
            .secret_store
            .get_secret(PPI_API_SECRET_SECRET)?
            .ok_or_else(|| Error::Unexpected("PPI API secret not configured".to_string()))?;

        let url = self.api_url("Account/LoginApi");
        let resp: PpiLoginResponse = self
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
            .map_err(|e| Error::Unexpected(format!("PPI login request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| Error::Unexpected(format!("PPI login HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI login parse error: {}", e)))?;

        let access_token = resp
            .access_token
            .ok_or_else(|| Error::Unexpected("PPI login returned no access_token".to_string()))?;

        if let Some(ref refresh) = resp.refresh_token {
            if let Err(e) = self.secret_store.set_secret(PPI_REFRESH_TOKEN_SECRET, refresh) {
                warn!("Failed to store PPI refresh token: {}", e);
            }
        }

        *self.token_cache.write().await = Some(CachedToken {
            access_token: access_token.clone(),
        });

        info!("PPI login successful");
        Ok(access_token)
    }

    async fn refresh_access_token(&self, refresh_token: &str) -> Result<String> {
        let url = self.api_url("Account/RefreshToken");
        let body = PpiRefreshRequest { refresh_token: refresh_token.to_string() };

        let resp: PpiLoginResponse = self
            .client
            .post(&url)
            .header(CONTENT_TYPE, "application/json")
            .header("AuthorizedClient", &self.authorized_client)
            .header("ClientKey", &self.client_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI refresh request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| Error::Unexpected(format!("PPI refresh HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI refresh parse error: {}", e)))?;

        let access_token = resp
            .access_token
            .ok_or_else(|| Error::Unexpected("PPI refresh returned no access_token".to_string()))?;

        if let Some(ref new_refresh) = resp.refresh_token {
            if let Err(e) = self.secret_store.set_secret(PPI_REFRESH_TOKEN_SECRET, new_refresh) {
                warn!("Failed to store rotated PPI refresh token: {}", e);
            }
        }

        *self.token_cache.write().await = Some(CachedToken {
            access_token: access_token.clone(),
        });

        debug!("PPI token refreshed");
        Ok(access_token)
    }

    /// Fetch the first account number from Account/Accounts.
    async fn get_account_number(&self) -> Result<String> {
        let token = self.get_access_token().await?;
        let url = self.api_url("Account/Accounts");

        let accounts: Vec<PpiAccount> = self
            .client
            .get(&url)
            .headers(self.auth_headers(&token))
            .send()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI accounts request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| Error::Unexpected(format!("PPI accounts HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI accounts parse error: {}", e)))?;

        accounts
            .into_iter()
            .next()
            .and_then(|a| a.account_number)
            .ok_or_else(|| Error::Unexpected("PPI returned no accounts".to_string()))
    }
}

#[async_trait]
impl BrokerApiClient for PpiApiClient {
    async fn list_connections(&self) -> Result<Vec<BrokerConnection>> {
        Ok(vec![BrokerConnection {
            id: PPI_CONNECTION_ID.to_string(),
            brokerage: Some(BrokerConnectionBrokerage {
                id: Some(PPI_BROKERAGE_SLUG.to_string()),
                slug: Some(PPI_BROKERAGE_SLUG.to_string()),
                name: Some(PPI_BROKERAGE_NAME.to_string()),
                display_name: Some(PPI_BROKERAGE_NAME.to_string()),
                aws_s3_logo_url: None,
                aws_s3_square_logo_url: None,
            }),
            connection_type: Some("read".to_string()),
            status: Some("connected".to_string()),
            disabled: false,
            disabled_date: None,
            updated_at: None,
            name: Some(PPI_BROKERAGE_NAME.to_string()),
        }])
    }

    async fn list_accounts(&self, _authorization_ids: Option<Vec<String>>) -> Result<Vec<BrokerAccount>> {
        let account_number = self.get_account_number().await?;
        Ok(vec![BrokerAccount {
            id: Some(account_number.clone()),
            name: Some(PPI_BROKERAGE_NAME.to_string()),
            account_number: Some(account_number),
            account_type: Some("SECURITIES".to_string()),
            currency: Some("ARS".to_string()),
            brokerage_authorization: Some(PPI_CONNECTION_ID.to_string()),
            institution_name: Some(PPI_BROKERAGE_NAME.to_string()),
            sync_enabled: true,
            ..Default::default()
        }])
    }

    async fn list_brokerages(&self) -> Result<Vec<BrokerBrokerage>> {
        Ok(vec![BrokerBrokerage {
            id: Some(PPI_BROKERAGE_SLUG.to_string()),
            slug: Some(PPI_BROKERAGE_SLUG.to_string()),
            name: Some(PPI_BROKERAGE_NAME.to_string()),
            display_name: Some(PPI_BROKERAGE_NAME.to_string()),
            url: Some("https://portfoliopersonal.com".to_string()),
            enabled: true,
        }])
    }

    async fn get_account_activities(
        &self,
        account_id: &str,
        start_date: Option<&str>,
        end_date: Option<&str>,
        offset: Option<i64>,
        limit: Option<i64>,
    ) -> Result<PaginatedUniversalActivity> {
        let token = self.get_access_token().await?;
        let url = self.api_url("Account/Movements");

        let mut req = self
            .client
            .get(&url)
            .headers(self.auth_headers(&token))
            .query(&[("accountNumber", account_id)]);

        if let Some(from) = start_date {
            req = req.query(&[("dateFrom", from)]);
        }
        if let Some(to) = end_date {
            req = req.query(&[("dateTo", to)]);
        }

        let movements: Vec<PpiMovement> = req
            .send()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI movements request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| Error::Unexpected(format!("PPI movements HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI movements parse error: {}", e)))?;

        let activities: Vec<_> = movements.into_iter().filter_map(map_ppi_activity).collect();
        let total = activities.len() as i64;

        Ok(PaginatedUniversalActivity {
            data: activities,
            pagination: Some(PaginationDetails {
                offset,
                limit,
                total: Some(total),
                has_more: Some(false), // PPI returns all movements at once
            }),
        })
    }

    async fn get_account_holdings(&self, account_id: &str) -> Result<BrokerHoldingsResponse> {
        let token = self.get_access_token().await?;
        let url = self.api_url("Account/BalancesAndPositions");

        let portfolio: PpiPortfolioResponse = self
            .client
            .get(&url)
            .headers(self.auth_headers(&token))
            .query(&[("accountNumber", account_id)])
            .send()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI portfolio request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| Error::Unexpected(format!("PPI portfolio HTTP error: {}", e)))?
            .json()
            .await
            .map_err(|e| Error::Unexpected(format!("PPI portfolio parse error: {}", e)))?;

        let positions: Vec<_> = portfolio
            .grouped_instruments
            .into_iter()
            .flat_map(|g| g.instruments)
            .filter_map(map_ppi_holding)
            .collect();

        // Use "INMEDIATA" (spot) balances only to avoid double-counting settlements
        let balances: Vec<_> = portfolio
            .grouped_availability
            .into_iter()
            .flat_map(|g| g.availability)
            .filter(|e| e.settlement.as_deref() == Some("INMEDIATA"))
            .map(map_ppi_cash)
            .collect();

        Ok(BrokerHoldingsResponse {
            account: None,
            balances: Some(balances),
            positions: Some(positions),
            option_positions: None,
        })
    }
}
