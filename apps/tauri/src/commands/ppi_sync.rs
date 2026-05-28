//! Tauri commands for Portfolio Personal (PPI) broker sync.

use std::sync::Arc;

use log::{error, info};
use tauri::{AppHandle, Emitter, State};
use wealthfolio_connect::{
    NoOpProgressReporter, PpiApiClient, SyncConfig, SyncOrchestrator, SyncProgressPayload,
    SyncProgressReporter, SyncResult,
};

use crate::{
    context::ServiceContext,
    events::{BROKER_SYNC_COMPLETE, BROKER_SYNC_ERROR, BROKER_SYNC_START},
    secret_store::KeyringSecretStore,
};

const PPI_API_KEY_SECRET: &str = "ppi_api_key";
const PPI_API_SECRET_SECRET: &str = "ppi_api_secret";
const PPI_AUTHORIZED_CLIENT_SECRET: &str = "ppi_authorized_client";
const PPI_CLIENT_KEY_SECRET: &str = "ppi_client_key";

// ─────────────────────────────────────────────────────────────────────────────
// Progress Reporter
// ─────────────────────────────────────────────────────────────────────────────

struct TauriProgressReporter {
    app_handle: AppHandle,
}

impl SyncProgressReporter for TauriProgressReporter {
    fn report_progress(&self, payload: SyncProgressPayload) {
        let _ = self.app_handle.emit("sync-progress", &payload);
    }

    fn report_sync_start(&self) {
        let _ = self.app_handle.emit(BROKER_SYNC_START, ());
    }

    fn report_sync_complete(&self, result: &SyncResult) {
        if result.success {
            let _ = self.app_handle.emit(BROKER_SYNC_COMPLETE, result);
        } else {
            let _ = self.app_handle.emit(
                BROKER_SYNC_ERROR,
                serde_json::json!({ "error": result.message }),
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Commands
// ─────────────────────────────────────────────────────────────────────────────

/// Save PPI credentials to keyring.
///
/// Does NOT validate credentials against PPI — call `sync_ppi_data` to verify.
#[tauri::command]
pub async fn save_ppi_credentials(
    api_key: String,
    api_secret: String,
    authorized_client: String,
    client_key: String,
    _state: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    use wealthfolio_core::secrets::SecretStore;
    KeyringSecretStore
        .set_secret(PPI_API_KEY_SECRET, &api_key)
        .map_err(|e| e.to_string())?;
    KeyringSecretStore
        .set_secret(PPI_API_SECRET_SECRET, &api_secret)
        .map_err(|e| e.to_string())?;
    KeyringSecretStore
        .set_secret(PPI_AUTHORIZED_CLIENT_SECRET, &authorized_client)
        .map_err(|e| e.to_string())?;
    KeyringSecretStore
        .set_secret(PPI_CLIENT_KEY_SECRET, &client_key)
        .map_err(|e| e.to_string())?;
    info!("PPI credentials saved to keyring");
    Ok(())
}

/// Delete PPI credentials from keyring.
#[tauri::command]
pub async fn delete_ppi_credentials(
    _state: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    use wealthfolio_core::secrets::SecretStore;
    let _ = KeyringSecretStore.delete_secret(PPI_API_KEY_SECRET);
    let _ = KeyringSecretStore.delete_secret(PPI_API_SECRET_SECRET);
    let _ = KeyringSecretStore.delete_secret(PPI_AUTHORIZED_CLIENT_SECRET);
    let _ = KeyringSecretStore.delete_secret(PPI_CLIENT_KEY_SECRET);
    let _ = KeyringSecretStore.delete_secret("ppi_refresh_token");
    info!("PPI credentials removed from keyring");
    Ok(())
}

/// Check if PPI credentials are fully configured (all 4 required).
#[tauri::command]
pub async fn get_ppi_credentials_status(
    _state: State<'_, Arc<ServiceContext>>,
) -> Result<bool, String> {
    use wealthfolio_core::secrets::SecretStore;
    let configured = [
        PPI_API_KEY_SECRET,
        PPI_API_SECRET_SECRET,
        PPI_AUTHORIZED_CLIENT_SECRET,
        PPI_CLIENT_KEY_SECRET,
    ]
    .iter()
    .all(|key| {
        KeyringSecretStore
            .get_secret(key)
            .map(|v| v.is_some())
            .unwrap_or(false)
    });
    Ok(configured)
}

/// Sync PPI data (activities + holdings) for all mapped accounts.
///
/// Non-blocking: returns immediately after spawning the background task.
/// Results are delivered via events:
/// - `broker:sync-start` — emitted when sync begins
/// - `broker:sync-complete` — emitted with SyncResult on success
/// - `broker:sync-error` — emitted with error message on failure
#[tauri::command]
pub async fn sync_ppi_data(
    app: AppHandle,
    state: State<'_, Arc<ServiceContext>>,
) -> Result<(), String> {
    info!("[PPI] Starting PPI data sync...");
    let context = state.inner().clone();
    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        match perform_ppi_sync(&context, Some(&app_handle)).await {
            Ok(_) => info!("[PPI] Sync completed successfully"),
            Err(e) => error!("[PPI] Sync failed: {}", e),
        }
    });

    Ok(())
}

/// Core PPI sync logic (can be called from scheduler or command).
pub async fn perform_ppi_sync(
    context: &Arc<ServiceContext>,
    app: Option<&AppHandle>,
) -> Result<SyncResult, String> {
    use wealthfolio_core::secrets::SecretStore;
    let secret_store = Arc::new(KeyringSecretStore);

    let authorized_client = secret_store
        .get_secret(PPI_AUTHORIZED_CLIENT_SECRET)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "PPI AuthorizedClient not configured. Please save credentials first.".to_string())?;

    let client_key = secret_store
        .get_secret(PPI_CLIENT_KEY_SECRET)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "PPI ClientKey not configured. Please save credentials first.".to_string())?;

    let client = PpiApiClient::new(secret_store, authorized_client, client_key);
    let sync_service = context.sync_service();

    if let Some(app_handle) = app {
        let reporter = Arc::new(TauriProgressReporter {
            app_handle: app_handle.clone(),
        });
        let orchestrator = SyncOrchestrator::new(sync_service, reporter, SyncConfig::default());
        orchestrator.sync_all(&client).await
    } else {
        let reporter = Arc::new(NoOpProgressReporter);
        let orchestrator = SyncOrchestrator::new(sync_service, reporter, SyncConfig::default());
        orchestrator.sync_all(&client).await
    }
}
