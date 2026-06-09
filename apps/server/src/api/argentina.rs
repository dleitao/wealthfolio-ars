//! Argentine-specific endpoints: inflation data sync and XBUE sector enrichment.

use std::sync::Arc;

use axum::{extract::State, routing::{get, post}, Json, Router};
use tracing::{error, info};
use wealthfolio_core::inflation::{ArgentinaDatosInflationPoint, InflationRecord, InflationService};
use wealthfolio_market_data::ArgentinaDatosProvider;

use crate::{
    error::{ApiError, ApiResult},
    main_lib::AppState,
};

/// Return all stored monthly inflation records ordered by period.
async fn get_inflation_data(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<InflationRecord>>> {
    let records = state.inflation_service.get_all().map_err(ApiError::from)?;
    Ok(Json(records))
}

/// Fetch monthly IPC data from ArgentinaDatos and persist it.
/// Returns the number of records stored.
async fn sync_inflation_data(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<usize>> {
    info!("[Argentina] Syncing inflation data from ArgentinaDatos...");
    let provider = ArgentinaDatosProvider::new();
    let points = provider
        .get_inflation()
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let total = points.len();
    let records: Vec<InflationRecord> = points
        .into_iter()
        .filter_map(|p| {
            InflationService::convert_point(ArgentinaDatosInflationPoint {
                fecha: p.fecha,
                valor: p.valor,
            })
        })
        .collect();

    let stored = records.len();
    state
        .inflation_service
        .store(&records)
        .await
        .map_err(ApiError::from)?;

    info!(
        "[Argentina] Inflation sync complete: {} fetched, {} stored",
        total, stored
    );
    Ok(Json(stored))
}

/// Batch-fetch TradingView Argentina sector profiles and classify all XBUE assets.
/// Returns (classified_count, not_found_count).
async fn sync_argentina_sectors(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<(usize, usize)>> {
    info!("[Argentina] Syncing XBUE sector data from TradingView...");
    let result = state
        .asset_service
        .enrich_xbue_sectors()
        .await
        .map_err(|e| {
            error!("[Argentina] Sector sync failed: {}", e);
            ApiError::from(e)
        })?;
    Ok(Json(result))
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/argentina/inflation", get(get_inflation_data))
        .route("/argentina/inflation/sync", post(sync_inflation_data))
        .route("/argentina/sectors/sync", post(sync_argentina_sectors))
}
