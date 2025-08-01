use sqlx::types::chrono::{NaiveDateTime, Utc};
use sqlx::{PgPool, Row, migrate::Migrator, types::chrono::DateTime};

use crate::model::{Candle, Resolution, TradeInfo, TradeOhlcv};
use crate::pump_handler::CandlesStorage;
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

    pub async fn get_tokens(&self) -> Result<Vec<String>, anyhow::Error> {
        let rows = sqlx::query("SELECT (mint) FROM token")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|row| row.get(0)).collect())
    }

    pub async fn trades_since(
        &self,
        mint_acc: String,
        timestamp: DateTime<Utc>,
        resolution: Resolution,
    ) -> anyhow::Result<Vec<TradeOhlcv>> {
        let rows = sqlx::query(
            "SELECT datetime, open_price, close_price, high_price, low_price, volume 
        FROM trades 
        WHERE datetime >= $1 AND resol = $2 AND mint_acc = $3
        ORDER BY datetime",
        )
        .bind(timestamp)
        .bind(resolution)
        .bind(mint_acc)
        .fetch_all(&self.pool)
        .await?;

        let mut trades = Vec::with_capacity(rows.len());
        for row in rows {
            let datetime = (&row).get::<NaiveDateTime, _>(0).and_utc();
            let open = row.get::<f64, _>(1);
            let close = row.get::<f64, _>(2);
            let high = row.get::<f64, _>(3);
            let low = row.get::<f64, _>(4);
            let volume = row.get::<f64, _>(5);

            trades.push(TradeOhlcv {
                timestamp: datetime.timestamp_millis() as u64 / 1000,
                candle: Candle {
                    open,
                    close,
                    high,
                    low,
                    volume,
                },
            });
        }

        Ok(trades)
    }
}

impl CandlesStorage for Db {
    async fn insert(&self, timestamps: &[DateTime<Utc>], info: TradeInfo) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;

        let mint_acc: Vec<_> = (0..timestamps.len())
            .map(|_| info.mint_acc.clone())
            .collect();
        let resol = Resolution::all();

        let price = (info.sol_amount as f64) / (info.token_amount as f64);
        if !price.is_finite() {
            anyhow::bail!("Bad price: {} / {}", info.sol_amount, info.token_amount);
        };

        let open_price: Vec<_> = (0..timestamps.len()).map(|_| price).collect();
        let close_price: Vec<_> = (0..timestamps.len()).map(|_| price).collect();
        let high_price: Vec<_> = (0..timestamps.len()).map(|_| price).collect();
        let low_price: Vec<_> = (0..timestamps.len()).map(|_| price).collect();
        let volume: Vec<_> = (0..timestamps.len())
            .map(|_| info.token_amount as f64)
            .collect();

        sqlx::query("INSERT INTO token (mint) VALUES ($1) ON CONFLICT DO NOTHING")
            .bind(&info.mint_acc)
            .execute(&mut *tx)
            .await?;

        sqlx::query(
            "INSERT INTO trades 
        (
            datetime,
            mint_acc,
            resol,
            open_price,
            close_price,
            high_price,
            low_price,
            volume
        )
        SELECT * FROM UNNEST
        (
            $1::timestamp[],
            $2::varchar[],
            $3::resolution[],
            $4::decimal[],
            $5::decimal[],
            $6::decimal[],
            $7::decimal[],
            $8::decimal[]
        )
        
        ON CONFLICT (datetime, mint_acc, resol) DO UPDATE SET
            open_price = trades.open_price,
            close_price = EXCLUDED.close_price,
            high_price = GREATEST(trades.high_price, EXCLUDED.high_price),
            low_price = LEAST(trades.low_price, EXCLUDED.low_price),
            volume = trades.volume + EXCLUDED.volume",
        )
        .bind(timestamps)
        .bind(mint_acc)
        .bind(resol)
        .bind(open_price)
        .bind(close_price)
        .bind(high_price)
        .bind(low_price)
        .bind(volume)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }
}
