//! Tauri commands for Argentine-specific features: inflation data sync and ARS FX rates.

use std::sync::Arc;

use chrono::NaiveDate;
use log::{info, warn};
use rust_decimal::Decimal;
use serde::Deserialize;
use tauri::{AppHandle, State};
use wealthfolio_core::fx::NewExchangeRate;
use wealthfolio_core::inflation::{ArgentinaDatosInflationPoint, InflationRecord, InflationService};
use wealthfolio_market_data::ArgentinaDatosProvider;

use crate::context::ServiceContext;
use crate::events::{emit_portfolio_trigger_recalculate, PortfolioRequestPayload};

// ─────────────────────────────────────────────────────────────────────────────
// ARS FX rate sync (DolarAPI today + ArgentinaDatos history)
// ─────────────────────────────────────────────────────────────────────────────

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
            warn!("[ARS] Failed to fetch ArgentinaDatos {}: {}", endpoint, e);
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

/// Fetch today's ARS rates from DolarAPI and full historical ARS rates from ArgentinaDatos,
/// then persist them via the FX service. Emits a portfolio recalculation trigger when done.
///
/// This function is shared by the PPI sync flow and the standalone `sync_ars_rates` command
/// so that Balanz users (who don't use PPI) also get accurate historical FX rates for
/// cost-basis calculations.
pub(crate) async fn sync_dolar_ars_rates(context: &Arc<ServiceContext>, app: &AppHandle) {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();

    let (oficial, mep, ccl) = tokio::join!(
        fetch_dolar_rate(&client, "oficial"),
        fetch_dolar_rate(&client, "bolsa"),
        fetch_dolar_rate(&client, "contadoconliqui"),
    );

    let pairs = [
        (oficial, "ARS_OFICIAL"),
        (mep, "ARS_MEP"),
        (ccl, "ARS_CCL"),
    ];

    let mut saved = false;
    let fx_service = context.fx_service();

    for (rate_opt, to_currency) in pairs {
        match rate_opt {
            Some(rate) => {
                let new_rate = NewExchangeRate {
                    from_currency: "USD".to_string(),
                    to_currency: to_currency.to_string(),
                    rate,
                    source: "DOLAR_API".to_string(),
                };
                match fx_service.add_exchange_rate(new_rate).await {
                    Ok(_) => {
                        info!("[ARS] USD/{} rate saved: {} ARS per USD", to_currency, rate);
                        saved = true;
                    }
                    Err(e) => warn!("[ARS] Failed to save USD/{} rate: {}", to_currency, e),
                }
            }
            None => warn!("[ARS] DolarAPI returned no valid {} rate", to_currency),
        }
    }

    if let Some(ars_to_usd) = oficial.or(mep).or(ccl).and_then(|r| Decimal::ONE.checked_div(r)) {
        let new_rate = NewExchangeRate {
            from_currency: "ARS".to_string(),
            to_currency: "USD".to_string(),
            rate: ars_to_usd,
            source: "DOLAR_API".to_string(),
        };
        match fx_service.add_exchange_rate(new_rate).await {
            Ok(_) => {
                info!("[ARS] ARS/USD rate saved: {} USD per ARS", ars_to_usd);
                saved = true;
            }
            Err(e) => warn!("[ARS] Failed to save ARS/USD rate: {}", e),
        }
    }

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

        match fx_service
            .save_historical_fx_quotes("USD", to_currency, history, "ARGENTINA_DATOS")
            .await
        {
            Ok(n) => {
                info!("[ARS] Saved {} historical USD/{} quotes", n, to_currency);
                saved = true;
            }
            Err(e) => warn!("[ARS] Failed to save historical USD/{} quotes: {}", to_currency, e),
        }

        if to_currency == "ARS_OFICIAL" && !ars_usd.is_empty() {
            if let Err(e) = fx_service
                .save_historical_fx_quotes("ARS", "USD", ars_usd, "ARGENTINA_DATOS")
                .await
            {
                warn!("[ARS] Failed to save historical ARS/USD quotes: {}", e);
            }
        }
    }

    if saved {
        let payload = PortfolioRequestPayload::builder().account_ids(None).build();
        emit_portfolio_trigger_recalculate(app, payload);
    }
}

/// Sync today's ARS/USD rates and full historical series.
/// Useful for Balanz users who don't have PPI configured.
#[tauri::command]
pub async fn sync_ars_rates(
    app: AppHandle,
    state: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    info!("[ARS] Syncing ARS FX rates...");
    sync_dolar_ars_rates(&state, &app).await;
    Ok(())
}

/// Fetch monthly inflation data from ArgentinaDatos and persist it.
#[tauri::command]
pub async fn sync_inflation_data(state: State<'_, Arc<ServiceContext>>) -> Result<usize, String> {
    info!("[Argentina] Syncing inflation data from ArgentinaDatos...");
    let inflation_service = state.inflation_service();
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
    inflation_service.store(&records).await.map_err(|e| e.to_string())?;
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
        .asset_service()
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
        .inflation_service()
        .get_all()
        .map_err(|e| e.to_string())
}
