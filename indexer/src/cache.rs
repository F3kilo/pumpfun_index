use std::collections::BTreeMap;
use std::time::Duration;

use redis::Client;
use sqlx::types::chrono::{DateTime, Utc};

use crate::model::{Candle, Resolution, TradeInfo};

#[derive(Clone)]
pub struct Cache {
    redis: Client,
}

const MILLIS_IN_DAY: u64 = 24 * 60 * 60 * 1000;

/// Cache retention period.
pub const RETENTION_PERIOD: Duration = Duration::from_millis(MILLIS_IN_DAY);

impl Cache {
    /// Create new cache instance.
    pub async fn new(conn_str: &str) -> anyhow::Result<Self> {
        let redis = redis::Client::open(conn_str)?;
        let mut connection = redis.get_multiplexed_async_connection().await?;

        let retention = RETENTION_PERIOD.as_millis().to_string();
        redis::cmd("CONFIG")
            .arg(&["SET", "ts-retention-policy", &retention])
            .exec_async(&mut connection)
            .await?;

        Ok(Self { redis })
    }

    /// Insert trade event into cache.
    pub async fn insert_trade(
        &self,
        timestamps: &[DateTime<Utc>],
        info: TradeInfo,
    ) -> anyhow::Result<()> {
        let mut connection = self.redis.get_multiplexed_async_connection().await?;

        let price = (info.sol_amount as f64) / (info.token_amount as f64);
        if !price.is_finite() {
            anyhow::bail!("Bad price: {} / {}", info.sol_amount, info.token_amount);
        };

        for (timestamp, resolution) in timestamps.iter().zip(Resolution::all().iter()) {
            for (mode, policy) in PRICES_POLICIES.iter() {
                let name = Self::ts_name(&info.mint_acc, *resolution, mode);
                let timestamp = timestamp.timestamp_millis();

                redis::cmd("TS.ADD")
                    .arg(&name)
                    .arg(timestamp)
                    .arg(price)
                    .arg("ON_DUPLICATE")
                    .arg(policy)
                    .exec_async(&mut connection)
                    .await?;
            }
        }

        Ok(())
    }

    /// Read last trade event from cache.
    pub async fn last_trade(
        &self,
        mint: &str,
        resolution: Resolution,
    ) -> anyhow::Result<(DateTime<Utc>, Candle)> {
        let mut connection = self.redis.get_multiplexed_async_connection().await?;

        let mut values = [0.0; 5];
        let mut last_timestamp = 0;
        for (idx, (mode, _policy)) in PRICES_POLICIES.iter().enumerate() {
            let name = Self::ts_name(mint, resolution, mode);

            let (timestamp, price) = redis::cmd("TS.GET")
                .arg(&name)
                .query_async::<(i64, f64)>(&mut connection)
                .await?;

            last_timestamp = last_timestamp.max(timestamp);
            values[idx] = price;
        }

        let datetime = DateTime::from_timestamp_millis(last_timestamp).expect("correct datetime");
        let candle = Candle {
            open: values[0],
            high: values[1],
            low: values[2],
            close: values[3],
            volume: values[4],
        };

        Ok((datetime, candle))
    }

    /// Read trades history from cache.
    pub async fn trades_since(
        &self,
        mint_acc: &str,
        from_timestamp: DateTime<Utc>,
        resolution: Resolution,
    ) -> anyhow::Result<BTreeMap<DateTime<Utc>, Candle>> {
        let mut connection = self.redis.get_multiplexed_async_connection().await?;

        let mut trades = BTreeMap::new();

        for (mode, _policy) in PRICES_POLICIES.iter() {
            let name = Self::ts_name(mint_acc, resolution, mode);

            let values = redis::cmd("TS.RANGE")
                .arg(&name)
                .arg(from_timestamp.timestamp_millis())
                .arg("+")
                .query_async::<Vec<(i64, f64)>>(&mut connection)
                .await?;

            for (timestamp, value) in values {
                let datetime =
                    DateTime::from_timestamp_millis(timestamp).expect("correct datetime");
                let trades_entry: &mut Candle = trades.entry(datetime).or_default();

                match *mode {
                    "open" => trades_entry.open = value,
                    "high" => trades_entry.high = value,
                    "low" => trades_entry.low = value,
                    "close" => trades_entry.close = value,
                    "volume" => trades_entry.volume = value,
                    _ => unreachable!(),
                }
            }
        }

        Ok(trades)
    }

    /// Name of the time series for given parameters.
    fn ts_name(mint: &str, resolution: Resolution, mode: &str) -> String {
        format!("trade_{}_{}_{}", mint, resolution, mode)
    }
}

const PRICES_POLICIES: [(&str, &str); 5] = [
    ("open", "FIRST"),
    ("high", "MAX"),
    ("low", "MIN"),
    ("close", "LAST"),
    ("volume", "SUM"),
];
