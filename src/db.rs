use std::collections::BTreeMap;

use sqlx::postgres::PgRow;
use sqlx::types::chrono::{NaiveDateTime, Utc};
use sqlx::{PgPool, Row, migrate::Migrator, types::chrono::DateTime};

use crate::model::{Candle, Resolution, TokenMetadata, TradeInfo};
use crate::pump_handler::CandlesStorage;

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

    pub async fn get_tokens(&self) -> Result<Vec<(String, TokenMetadata)>, anyhow::Error> {
        let rows = sqlx::query("SELECT mint, name, symbol, uri FROM token")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows
            .iter()
            .map(|row| (dbg!(row).get(0), parse_metadata_row(row, 1)))
            .collect())
    }

    pub async fn trades_since_with_one_previous(
        &self,
        mint_acc: String,
        timestamp: DateTime<Utc>,
        resolution: Resolution,
    ) -> anyhow::Result<BTreeMap<DateTime<Utc>, Candle>> {
        let rows = sqlx::query(
            "(
            SELECT datetime, open_price, close_price, high_price, low_price, volume 
            FROM trades 
            WHERE datetime >= $1 AND resol = $2 AND mint_acc = $3
        )
        UNION ALL
        (
            SELECT datetime, open_price, close_price, high_price, low_price, volume 
            FROM trades 
            WHERE datetime < $1 AND resol = $2 AND mint_acc = $3
            ORDER BY datetime DESC
            LIMIT 1
        )
        ORDER BY datetime",
        )
        .bind(timestamp)
        .bind(resolution)
        .bind(mint_acc)
        .fetch_all(&self.pool)
        .await?;

        let mut trades = BTreeMap::new();
        for row in rows {
            let datetime = row.get::<NaiveDateTime, _>(0).and_utc();
            let open = row.get::<f64, _>(1);
            let close = row.get::<f64, _>(2);
            let high = row.get::<f64, _>(3);
            let low = row.get::<f64, _>(4);
            let volume = row.get::<f64, _>(5);

            trades.insert(
                datetime,
                Candle {
                    open,
                    close,
                    high,
                    low,
                    volume,
                },
            );
        }

        Ok(trades)
    }

    pub async fn last_trade(
        &self,
        mint_acc: String,
        resolution: Resolution,
    ) -> anyhow::Result<(DateTime<Utc>, Candle)> {
        let row = sqlx::query(
            "
            SELECT datetime, open_price, close_price, high_price, low_price, volume 
            FROM trades 
            WHERE resol = $1 AND mint_acc = $2
            ORDER BY datetime DESC
            LIMIT 1
            ",
        )
        .bind(resolution)
        .bind(mint_acc)
        .fetch_one(&self.pool)
        .await?;

        let datetime = row.get::<NaiveDateTime, _>(0).and_utc();
        let open = row.get::<f64, _>(1);
        let close = row.get::<f64, _>(2);
        let high = row.get::<f64, _>(3);
        let low = row.get::<f64, _>(4);
        let volume = row.get::<f64, _>(5);

        let candle = Candle {
            open,
            close,
            high,
            low,
            volume,
        };

        Ok((datetime, candle))
    }
}

impl CandlesStorage for Db {
    async fn insert_trade(
        &self,
        timestamps: &[DateTime<Utc>],
        info: TradeInfo,
    ) -> anyhow::Result<()> {
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
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn insert_token_metadata(
        &self,
        mint_acc: String,
        metadata: Option<TokenMetadata>,
    ) -> anyhow::Result<()> {
        if let Some(metadata) = metadata {
            sqlx::query(
                "INSERT INTO token (mint, name, symbol, uri) VALUES ($1, $2, $3, $4) ON CONFLICT (mint) DO UPDATE 
        SET name = EXCLUDED.name, symbol = EXCLUDED.symbol, uri = EXCLUDED.uri",
            )
            .bind(&mint_acc)
            .bind(metadata.name)
            .bind(metadata.symbol)
            .bind(metadata.uri)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query("INSERT INTO token (mint) VALUES ($1) ON CONFLICT DO NOTHING")
                .bind(&mint_acc)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn get_token_metadata(&self, mint_acc: &str) -> anyhow::Result<TokenMetadata> {
        let row = sqlx::query("SELECT name, symbol, uri FROM token WHERE mint = $1")
            .fetch_optional(&self.pool)
            .await?;

        match row {
            Some(row) => Ok(parse_metadata_row(&row, 0)),
            None => anyhow::bail!("Token metadata not found for mint: {}", mint_acc),
        }
    }
}

fn parse_metadata_row(row: &PgRow, fields_offset: usize) -> TokenMetadata {
    let name = row
        .try_get(fields_offset)
        .unwrap_or_else(|_| String::from("unknown"));
    let symbol = row
        .try_get(fields_offset + 1)
        .unwrap_or_else(|_| String::from("NAN"));
    let uri = row
        .try_get(fields_offset + 2)
        .unwrap_or_else(|_| String::from("unknown"));
    TokenMetadata { name, symbol, uri }
}
