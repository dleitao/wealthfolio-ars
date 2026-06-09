//! PPI API response models — based on actual API responses.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Auth
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpiLoginResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PpiRefreshRequest {
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Accounts
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PpiAccount {
    pub account_number: Option<String>,
    #[allow(dead_code)]
    pub name: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Movements / Activities
// ─────────────────────────────────────────────────────────────────────────────

/// A single movement from GET /api/1/Account/Movements.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PpiMovement {
    pub agreement_date: Option<String>,
    pub settlement_date: Option<String>,
    pub currency: Option<String>,
    pub amount: Option<f64>,
    pub price: Option<f64>,
    pub description: Option<String>,
    /// May be "Ticker not found" — treat as None in that case.
    pub ticker: Option<String>,
    pub quantity: Option<f64>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Portfolio / Holdings
// ─────────────────────────────────────────────────────────────────────────────

/// Response from GET /api/1/Account/BalancesAndPositions.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PpiPortfolioResponse {
    #[serde(default)]
    pub grouped_availability: Vec<PpiCurrencyAvailability>,
    #[serde(default)]
    pub grouped_instruments: Vec<PpiInstrumentGroup>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PpiCurrencyAvailability {
    #[allow(dead_code)]
    pub currency: Option<String>,
    #[serde(default)]
    pub availability: Vec<PpiAvailabilityEntry>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PpiAvailabilityEntry {
    pub symbol: Option<String>,
    pub amount: Option<f64>,
    pub settlement: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PpiInstrumentGroup {
    #[allow(dead_code)]
    pub name: Option<String>,
    #[serde(default)]
    pub instruments: Vec<PpiInstrument>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PpiInstrument {
    pub ticker: Option<String>,
    pub description: Option<String>,
    pub currency: Option<String>,
    pub price: Option<f64>,
    pub quantity: Option<f64>,
}
