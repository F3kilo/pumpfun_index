use std::collections::BTreeMap;

use sqlx::types::chrono::{DateTime, Utc};

use crate::cache::{self, Cache};
use crate::db::Db;
use crate::model::{Candle, Resolution, TokenMetadata, TradeInfo};

/// Storage layer to unify work with DB and cache.
#[derive(Clone)]
pub struct Storage {
    db: Db,
    cache: Cache,
}

impl Storage {
    /// Create new storage.
    pub async fn new(db: Db, cache: Cache) -> Self {
        Self { db, cache }
    }

    /// Get tokens list with metadata.
    pub async fn get_tokens(&self) -> Result<Vec<(String, TokenMetadata)>, anyhow::Error> {
        self.db.get_tokens().await
    }

    /// Read trades history.
    pub async fn trades_since(
        &self,
        mint_acc: &str,
        from_timestamp: DateTime<Utc>,
        resolution: Resolution,
    ) -> anyhow::Result<BTreeMap<DateTime<Utc>, Candle>> {
        let cache_start = Utc::now() - cache::RETENTION_PERIOD;
        if from_timestamp > cache_start {
            match self
                .cache
                .trades_since(&mint_acc, from_timestamp, resolution)
                .await
            {
                Ok(cached) => return Ok(cached),
                Err(e) => {
                    tracing::error!("Failed to read trades from cache: {e}");
                }
            };
        }

        self.db
            .trades_since(mint_acc, from_timestamp, resolution)
            .await
    }

    /// Read last trade of the token with given resolution.
    /// If not found in cache, try to read from DB.
    pub async fn last_trade(
        &self,
        mint_acc: &str,
        resolution: Resolution,
    ) -> anyhow::Result<(DateTime<Utc>, Candle)> {
        match self.cache.last_trade(mint_acc, resolution).await {
            Ok(candle) => return Ok(candle),
            Err(e) => tracing::error!("Failed to read last trade from cache: {e}"),
        };

        self.db.last_trade(mint_acc, resolution).await
    }

    /// Insert new trade.
    /// Try to insert into cache and DB.
    pub async fn insert_trade(
        &self,
        timestamps: &[DateTime<Utc>],
        info: TradeInfo,
    ) -> anyhow::Result<()> {
        let (cache_result, db_result) = tokio::join!(
            self.cache.insert_trade(timestamps, info.clone()),
            self.db.insert_trade(timestamps, info)
        );

        if let Err(e) = cache_result {
            tracing::error!("Failed to insert trade into cache: {e}");
        }

        if let Err(e) = db_result {
            tracing::error!("Failed to insert trade into db: {e}");
        }

        Ok(())
    }

    /// Get token metadata.
    pub async fn get_token_metadata(&self, mint_acc: &str) -> anyhow::Result<TokenMetadata> {
        self.db.get_token(mint_acc).await
    }

    /// Insert token metadata.
    pub async fn insert_token_metadata(
        &self,
        mint_acc: String,
        metadata: Option<TokenMetadata>,
    ) -> anyhow::Result<()> {
        self.db.insert_token(mint_acc, metadata).await
    }
}
