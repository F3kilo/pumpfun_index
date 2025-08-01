use sqlx::types::chrono::NaiveDateTime;
use sqlx::{PgPool, Row, migrate::Migrator, types::chrono::DateTime};

use crate::{Price, PriceInfo};

static MIGRATOR: Migrator = sqlx::migrate!("pg/migrations");

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    pub async fn new(connection_string: String) -> anyhow::Result<Self> {
        Ok(Self {
            pool: PgPool::connect(&connection_string).await?,
        })
    }

    /// Perform migrations.
    pub async fn init(&self) -> anyhow::Result<()> {
        MIGRATOR.run(&self.pool).await?;
        Ok(())
    }

    /// Add price info to DB.
    pub async fn push_price(&self, price: Price) -> anyhow::Result<()> {
        let millis = price.bitcoin.last_updated_at * 1_000;
        let updated_dt = DateTime::from_timestamp_millis(millis as i64)
            .ok_or_else(|| anyhow::Error::msg("Datetime overflow"))?;

        sqlx::query("INSERT INTO prices (datetime, price) VALUES ($1, $2)")
            .bind(updated_dt.naive_utc())
            .bind(price.bitcoin.usd)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Get prices since specified timestamp.
    pub async fn prices_since(&self, timestamp: i64) -> anyhow::Result<Vec<Price>> {
        let millis = timestamp * 1_000;
        let dt = DateTime::from_timestamp_millis(millis as i64)
            .ok_or_else(|| anyhow::Error::msg("Datetime overflow"))?;
        let rows = sqlx::query("SELECT * FROM prices WHERE datetime >= $1 ORDER BY datetime")
            .bind(dt.naive_utc())
            .fetch_all(&self.pool)
            .await?;

        let mut prices = Vec::with_capacity(rows.len());
        for row in rows {
            let last_updated_at =
                row.get::<NaiveDateTime, _>(0).and_utc().timestamp_millis() as u64 / 1000;
            let usd = row.get::<f64, _>(1);

            prices.push(Price {
                bitcoin: PriceInfo {
                    usd,
                    last_updated_at,
                },
            });
        }

        Ok(prices)
    }
}
