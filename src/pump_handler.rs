use pumpfun::common::stream::{CreateEvent, PumpFunEvent, TradeEvent};
use sqlx::types::chrono::{DateTime, Utc};
use tokio::sync::mpsc::Receiver;

use crate::model::{Resolution, TradeInfo};

pub trait CandlesStorage {
    async fn insert(&self, timestamps: &[DateTime<Utc>], info: TradeInfo) -> anyhow::Result<()>;
}

pub struct PumpHandler;

impl PumpHandler {
    pub async fn run(storage: impl CandlesStorage, mut pumpfun_ops_sender: Receiver<PumpFunEvent>) {
        while let Some(event) = pumpfun_ops_sender.recv().await {
            if let Err(e) = Self::handle_event(event, &storage).await {
                tracing::warn!("Failed to handle event: {e}");
            }
        }
    }

    async fn handle_event(
        event: PumpFunEvent,
        storage: &impl CandlesStorage,
    ) -> anyhow::Result<()> {
        match event {
            // PumpFunEvent::Create(create) => Self::handle_create(storage, create).await, // todo!
            PumpFunEvent::Trade(trade) => Self::handle_trade(storage, trade).await,
            _ => Ok(()),
        }
    }

    async fn handle_create(
        storage: &impl CandlesStorage,
        create: CreateEvent,
    ) -> anyhow::Result<()> {
        todo!()
    }

    async fn handle_trade(storage: &impl CandlesStorage, trade: TradeEvent) -> anyhow::Result<()> {
        let times = Resolution::all()
            .iter()
            .map(|res| {
                // bind time to timestamp
                let timestamp_millis = trade.timestamp as u64 / res.to_seconds() * res.to_millis();
                DateTime::from_timestamp_millis(timestamp_millis as _).expect("correct datetime")
            })
            .collect::<Vec<_>>();

        let trade_info = TradeInfo {
            mint_acc: trade.mint.to_string(),
            sol_amount: trade.sol_amount,
            token_amount: trade.token_amount,
        };

        storage.insert(&times, trade_info).await?;

        Ok(())
    }
}
