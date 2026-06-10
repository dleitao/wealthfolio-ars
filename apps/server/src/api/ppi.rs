//! PPI (Portfolio Personal Inversiones) broker sync endpoint.
//!
//! POST /ppi/sync — triggers a non-blocking sync using credentials from the secret store.
//!                  Also fetches and stores the current USD/ARS MEP rate from DolarAPI.

use std::sync::Arc;

use axum::{extract::{Query, State}, http::StatusCode, routing::post, Router};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::{error, info, warn};
use wealthfolio_core::accounts::TrackingMode;
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

#[derive(Deserialize)]
struct ArgentinaDatosRate {
    fecha: String,
    venta: Option<f64>,
    compra: Option<f64>,
}

async fn fetch_argentina_datos_history(
    client: &reqwest::Client,
    endpoint: &str,
) -> Vec<(NaiveDate, Decimal)> {
    let url = format!("https://api.argentinadatos.com/v1/cotizaciones/dolares/{}", endpoint);
    let raw: Vec<ArgentinaDatosRate> = match client.get(&url).send().await {
        Ok(r) => r.json().await.unwrap_or_default(),
        Err(e) => {
            warn!("[PPI] Failed to fetch ArgentinaDatos {}: {}", endpoint, e);
            return Vec::new();
        }
    };
    raw.into_iter()
        .filter_map(|q| {
            let date = NaiveDate::parse_from_str(&q.fecha, "%Y-%m-%d").ok()?;
            let rate = q.venta.or(q.compra).filter(|&v| v > 0.0)?;
            Some((date, Decimal::try_from(rate).ok()?))
        })
        .collect()
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

    let (oficial, mep, ccl) = tokio::join!(
        fetch_dolar_rate(&client, "oficial"),
        fetch_dolar_rate(&client, "bolsa"),
        fetch_dolar_rate(&client, "contadoconliqui"),
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

    if let Some(rate) = ccl {
        let new_rate = NewExchangeRate {
            from_currency: "USD".to_string(),
            to_currency: "ARS_CCL".to_string(),
            rate,
            source: "DOLAR_API".to_string(),
        };
        match state.fx_service.add_exchange_rate(new_rate).await {
            Ok(_) => { info!("[PPI] USD/ARS_CCL rate saved: {} ARS per USD", rate); saved = true; }
            Err(e) => warn!("[PPI] Failed to save USD/ARS_CCL rate: {}", e),
        }
    } else {
        warn!("[PPI] DolarAPI returned no valid CCL rate");
    }

    // Derive ARS→USD using the best available rate so valuation works even when
    // the oficial endpoint is down. Oficial is preferred; MEP and CCL are fallbacks.
    if let Some(ars_to_usd) = oficial.or(mep).or(ccl).and_then(|r| Decimal::ONE.checked_div(r)) {
        let new_rate = NewExchangeRate {
            from_currency: "ARS".to_string(),
            to_currency: "USD".to_string(),
            rate: ars_to_usd,
            source: "DOLAR_API".to_string(),
        };
        match state.fx_service.add_exchange_rate(new_rate).await {
            Ok(_) => { info!("[PPI] ARS/USD rate saved: {} USD per ARS", ars_to_usd); saved = true; }
            Err(e) => warn!("[PPI] Failed to save ARS/USD rate: {}", e),
        }
    }

    // Fetch and store historical ARS rates from ArgentinaDatos so the CurrencyConverter
    // has per-date rates for all historical activity dates.
    let hist_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .unwrap_or_default();

    let (oficial_hist, mep_hist, ccl_hist) = tokio::join!(
        fetch_argentina_datos_history(&hist_client, "oficial"),
        fetch_argentina_datos_history(&hist_client, "bolsa"),
        fetch_argentina_datos_history(&hist_client, "contadoconliqui"),
    );

    for (history, to_currency) in [
        (oficial_hist, "ARS_OFICIAL"),
        (mep_hist, "ARS_MEP"),
        (ccl_hist, "ARS_CCL"),
    ] {
        if history.is_empty() {
            continue;
        }
        let ars_usd: Vec<(NaiveDate, Decimal)> = history
            .iter()
            .filter_map(|(d, r)| Decimal::ONE.checked_div(*r).map(|inv| (*d, inv)))
            .collect();

        match state.fx_service.save_historical_fx_quotes("USD", to_currency, history, "ARGENTINA_DATOS").await {
            Ok(n) => { info!("[PPI] Saved {} historical USD/{} quotes", n, to_currency); saved = true; }
            Err(e) => warn!("[PPI] Failed to save historical USD/{} quotes: {}", to_currency, e),
        }

        if to_currency == "ARS_OFICIAL" && !ars_usd.is_empty() {
            if let Err(e) = state.fx_service.save_historical_fx_quotes("ARS", "USD", ars_usd, "ARGENTINA_DATOS").await {
                warn!("[PPI] Failed to save historical ARS/USD quotes: {}", e);
            }
        }
    }

    if saved {
        trigger_full_portfolio_recalc(state.clone());
    }
}

#[derive(Deserialize, Default)]
struct SyncParams {
    start_date: Option<String>,
    end_date: Option<String>,
}

async fn sync_ppi_data(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SyncParams>,
) -> StatusCode {
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
        // Fetch and store USD/ARS rates alongside the broker sync
        sync_usd_ars_rates(&state).await;

        let client = PpiApiClient::new(state.secret_store.clone(), authorized_client, client_key);

        let reporter = Arc::new(EventBusProgressReporter { event_bus: state.event_bus.clone() });
        let force_full = params.start_date.is_none();
        let sync_config = SyncConfig {
            override_start_date: params.start_date,
            override_end_date: params.end_date,
            force_tracking_mode: Some(TrackingMode::Transactions),
            force_full_history: force_full,
            ..Default::default()
        };
        let orchestrator = SyncOrchestrator::new(
            state.connect_sync_service.clone(),
            reporter,
            sync_config,
        );
        match orchestrator.sync_all(&client).await {
            Ok(_) => info!("[PPI] Sync completed successfully"),
            Err(e) => {
                error!("[PPI] Sync failed: {}", e);
                state.event_bus.publish(ServerEvent::with_payload(
                    BROKER_SYNC_ERROR,
                    serde_json::json!({ "error": e.to_string() }),
                ));
            }
        }
    });

    StatusCode::ACCEPTED
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new().route("/ppi/sync", post(sync_ppi_data))
}
