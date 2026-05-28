//! PPI (Portfolio Personal Inversiones) broker sync endpoint.
//!
//! POST /ppi/sync — triggers a non-blocking sync using credentials from the secret store.
//!                  Also fetches and stores the current USD/ARS MEP rate from DolarAPI.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, routing::post, Router};
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::{error, info, warn};
use wealthfolio_core::fx::NewExchangeRate;
use wealthfolio_connect::{
    PpiApiClient, SyncConfig, SyncOrchestrator, SyncProgressPayload, SyncProgressReporter,
    SyncResult,
};

use crate::{
    api::shared::trigger_full_portfolio_recalc,
    events::{EventBus, ServerEvent, BROKER_SYNC_COMPLETE, BROKER_SYNC_ERROR, BROKER_SYNC_START},
    main_lib::AppState,
};

struct EventBusProgressReporter {
    event_bus: EventBus,
}

impl SyncProgressReporter for EventBusProgressReporter {
    fn report_progress(&self, payload: SyncProgressPayload) {
        self.event_bus.publish(ServerEvent::with_payload(
            "sync-progress",
            serde_json::to_value(&payload).unwrap_or_default(),
        ));
    }

    fn report_sync_start(&self) {
        self.event_bus.publish(ServerEvent::new(BROKER_SYNC_START));
    }

    fn report_sync_complete(&self, result: &SyncResult) {
        if result.success {
            self.event_bus.publish(ServerEvent::with_payload(
                BROKER_SYNC_COMPLETE,
                serde_json::to_value(result).unwrap_or_default(),
            ));
        } else {
            self.event_bus.publish(ServerEvent::with_payload(
                BROKER_SYNC_ERROR,
                serde_json::json!({ "error": result.message }),
            ));
        }
    }
}

#[derive(Deserialize)]
struct DolarApiSingle {
    venta: Option<f64>,
    compra: Option<f64>,
}

async fn fetch_dolar_rate(client: &reqwest::Client, endpoint: &str) -> Option<Decimal> {
    let url = format!("https://dolarapi.com/v1/dolares/{}", endpoint);
    let resp: DolarApiSingle = client
        .get(&url)
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let rate_f64 = resp.venta.or(resp.compra).filter(|&v| v > 0.0)?;
    Decimal::try_from(rate_f64).ok()
}

/// Fetch current ARS_OFICIAL and ARS_MEP rates from DolarAPI and store them separately.
/// Stored as {from="USD", to="ARS_OFICIAL"} and {from="USD", to="ARS_MEP"} (1 USD = rate ARS).
async fn sync_usd_ars_rates(state: &Arc<AppState>) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let (oficial, mep) = tokio::join!(
        fetch_dolar_rate(&client, "oficial"),
        fetch_dolar_rate(&client, "bolsa"),
    );

    let mut saved = false;

    if let Some(rate) = oficial {
        let new_rate = NewExchangeRate {
            from_currency: "USD".to_string(),
            to_currency: "ARS_OFICIAL".to_string(),
            rate,
            source: "DOLAR_API".to_string(),
        };
        match state.fx_service.add_exchange_rate(new_rate).await {
            Ok(_) => { info!("[PPI] USD/ARS_OFICIAL rate saved: {} ARS per USD", rate); saved = true; }
            Err(e) => warn!("[PPI] Failed to save USD/ARS_OFICIAL rate: {}", e),
        }
    } else {
        warn!("[PPI] DolarAPI returned no valid oficial rate");
    }

    if let Some(rate) = mep {
        let new_rate = NewExchangeRate {
            from_currency: "USD".to_string(),
            to_currency: "ARS_MEP".to_string(),
            rate,
            source: "DOLAR_API".to_string(),
        };
        match state.fx_service.add_exchange_rate(new_rate).await {
            Ok(_) => { info!("[PPI] USD/ARS_MEP rate saved: {} ARS per USD", rate); saved = true; }
            Err(e) => warn!("[PPI] Failed to save USD/ARS_MEP rate: {}", e),
        }
    } else {
        warn!("[PPI] DolarAPI returned no valid MEP rate");
    }

    if saved {
        trigger_full_portfolio_recalc(state.clone());
    }
}

async fn sync_ppi_data(State(state): State<Arc<AppState>>) -> StatusCode {
    let authorized_client = match state.secret_store.get_secret("ppi_authorized_client") {
        Ok(Some(v)) if !v.is_empty() => v,
        _ => {
            error!("[PPI] AuthorizedClient not configured");
            return StatusCode::BAD_REQUEST;
        }
    };
    let client_key = match state.secret_store.get_secret("ppi_client_key") {
        Ok(Some(v)) if !v.is_empty() => v,
        _ => {
            error!("[PPI] ClientKey not configured");
            return StatusCode::BAD_REQUEST;
        }
    };

    info!("[PPI] Starting broker data sync (non-blocking)...");

    tokio::spawn(async move {
        // Fetch and store USD/ARS_OFICIAL and USD/ARS_MEP rates alongside the broker sync
        sync_usd_ars_rates(&state).await;

        let client = PpiApiClient::new(state.secret_store.clone(), authorized_client, client_key);
        let reporter = Arc::new(EventBusProgressReporter { event_bus: state.event_bus.clone() });
        let orchestrator = SyncOrchestrator::new(
            state.connect_sync_service.clone(),
            reporter,
            SyncConfig::default(),
        );
        match orchestrator.sync_all(&client).await {
            Ok(_) => info!("[PPI] Sync completed successfully"),
            Err(e) => error!("[PPI] Sync failed: {}", e),
        }
    });

    StatusCode::ACCEPTED
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ppi/sync", post(sync_ppi_data))
}
