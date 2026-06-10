use async_trait::async_trait;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use std::sync::Arc;

use wealthfolio_core::inflation::{InflationRecord, InflationRepositoryTrait};
use wealthfolio_core::Result;

use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;
use crate::schema::inflation_monthly;

#[derive(Queryable, Insertable, AsChangeset, Clone)]
#[diesel(table_name = inflation_monthly)]
struct InflationMonthlyDB {
    period: String,
    monthly_rate: f64,
    source: String,
    fetched_at: String,
}

impl From<InflationMonthlyDB> for InflationRecord {
    fn from(db: InflationMonthlyDB) -> Self {
        InflationRecord {
            period: db.period,
            monthly_rate: db.monthly_rate,
            source: db.source,
            fetched_at: db.fetched_at,
        }
    }
}

impl From<&InflationRecord> for InflationMonthlyDB {
    fn from(r: &InflationRecord) -> Self {
        InflationMonthlyDB {
            period: r.period.clone(),
            monthly_rate: r.monthly_rate,
            source: r.source.clone(),
            fetched_at: r.fetched_at.clone(),
        }
    }
}

pub struct InflationRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl InflationRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

#[async_trait]
impl InflationRepositoryTrait for InflationRepository {
    async fn upsert_records(&self, records: &[InflationRecord]) -> Result<()> {
        let rows: Vec<InflationMonthlyDB> = records.iter().map(InflationMonthlyDB::from).collect();
        self.writer
            .exec(move |conn| {
                conn.transaction::<(), diesel::result::Error, _>(|conn| {
                    for row in &rows {
                        diesel::insert_into(inflation_monthly::table)
                            .values(row)
                            .on_conflict(inflation_monthly::period)
                            .do_update()
                            .set((
                                inflation_monthly::monthly_rate.eq(row.monthly_rate),
                                inflation_monthly::source.eq(&row.source),
                                inflation_monthly::fetched_at.eq(&row.fetched_at),
                            ))
                            .execute(conn)?;
                    }
                    Ok(())
                })
                .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    fn get_all(&self) -> Result<Vec<InflationRecord>> {
        let mut conn = get_connection(&self.pool)?;
        let rows = inflation_monthly::table
            .order_by(inflation_monthly::period.asc())
            .load::<InflationMonthlyDB>(&mut conn)
            .map_err(StorageError::from)?;
        Ok(rows.into_iter().map(InflationRecord::from).collect())
    }
}
