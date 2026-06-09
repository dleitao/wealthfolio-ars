//! Inflation service — stores and retrieves Argentine IPC (CPI) monthly data.

use async_trait::async_trait;
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::errors::Result;

/// One monthly inflation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InflationRecord {
    /// "YYYY-MM"
    pub period: String,
    /// Monthly inflation rate (e.g., 0.045 = 4.5%)
    pub monthly_rate: f64,
    pub source: String,
    pub fetched_at: String,
}

/// Raw point from ArgentinaDatos `/finanzas/indices/inflacion`.
#[derive(Debug, Deserialize)]
pub struct ArgentinaDatosInflationPoint {
    pub fecha: String,
    pub valor: f64,
}

#[async_trait]
pub trait InflationRepositoryTrait: Send + Sync {
    async fn upsert_records(&self, records: &[InflationRecord]) -> Result<()>;
    fn get_all(&self) -> Result<Vec<InflationRecord>>;
}

pub struct InflationService {
    repository: Arc<dyn InflationRepositoryTrait>,
}

impl InflationService {
    pub fn new(repository: Arc<dyn InflationRepositoryTrait>) -> Self {
        Self { repository }
    }

    /// Store (upsert) a batch of records.
    pub async fn store(&self, records: &[InflationRecord]) -> Result<()> {
        self.repository.upsert_records(records).await
    }

    /// Return all stored monthly records ordered by period.
    pub fn get_all(&self) -> Result<Vec<InflationRecord>> {
        self.repository.get_all()
    }

    /// Convert a raw ArgentinaDatos point to an `InflationRecord`.
    pub fn convert_point(point: ArgentinaDatosInflationPoint) -> Option<InflationRecord> {
        // fecha format: "YYYY-MM-DD" → period: "YYYY-MM"
        let date = NaiveDate::parse_from_str(&point.fecha, "%Y-%m-%d").ok()?;
        let period = date.format("%Y-%m").to_string();
        Some(InflationRecord {
            period,
            monthly_rate: point.valor / 100.0, // API returns percentage
            source: "ARGENTINA_DATOS".to_string(),
            fetched_at: Utc::now().to_rfc3339(),
        })
    }
}
