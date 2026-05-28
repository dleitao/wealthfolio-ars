//! Tauri commands for Argentine-specific features: inflation data sync.

use std::sync::Arc;

use log::info;
use tauri::State;
use wealthfolio_core::inflation::{ArgentinaDatosInflationPoint, InflationRecord, InflationService};
use wealthfolio_market_data::ArgentinaDatosProvider;

use crate::context::ServiceContext;

/// Fetch monthly inflation data from ArgentinaDatos and persist it.
#[tauri::command]
pub async fn sync_inflation_data(state: State<'_, Arc<ServiceContext>>) -> Result<usize, String> {
    info!("[Argentina] Syncing inflation data from ArgentinaDatos...");
    let inflation_service = Arc::clone(&state.inflation_service);
    let provider = ArgentinaDatosProvider::new();

    let points = provider.get_inflation().await.map_err(|e| e.to_string())?;
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
    inflation_service.store(&records).map_err(|e| e.to_string())?;
    info!(
        "[Argentina] Inflation sync complete: {} fetched, {} stored",
        total, stored
    );
    Ok(stored)
}

/// Batch-fetch TradingView Argentina sector profiles and classify all XBUE assets.
/// Returns (classified_count, not_found_count).
#[tauri::command]
pub async fn sync_argentina_sectors(
    state: State<'_, Arc<ServiceContext>>,
) -> Result<(usize, usize), String> {
    info!("[Argentina] Syncing XBUE sector data from TradingView...");
    state
        .asset_service
        .enrich_xbue_sectors()
        .await
        .map_err(|e| e.to_string())
}

/// Return all stored monthly inflation records.
#[tauri::command]
pub async fn get_inflation_data(
    state: State<'_, Arc<ServiceContext>>,
) -> Result<Vec<InflationRecord>, String> {
    state
        .inflation_service
        .get_all()
        .map_err(|e| e.to_string())
}
