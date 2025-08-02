use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use pumpfun::PumpFun;
use pumpfun::common::stream::Subscription;
use pumpfun::common::types::{Cluster, PriorityFee};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use tokio::sync::mpsc::Sender;

use crate::model::IndexedPumpfunEvent;

/// Pumpfun event indexer.
pub struct Indexer {
    client: PumpFun,
}

impl Indexer {
    /// Create new indexer.
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: PumpFun::new(
                Arc::new(Keypair::new()),
                Cluster::mainnet(CommitmentConfig::confirmed(), PriorityFee::default()),
            ),
        })
    }

    /// Subscribe to events.
    /// Captured events will be sent to the given sender.
    /// Returns subscription. It will stop event capture task on drop.
    pub async fn subscribe(
        &self,
        pumpfun_ops_sender: Sender<IndexedPumpfunEvent>,
    ) -> anyhow::Result<Subscription> {
        let index = AtomicU64::new(0);
        let subscription = self
            .client
            .subscribe(
                Some(CommitmentConfig::confirmed()),
                move |_, mb_event, mb_error, _| {
                    tracing::trace!("Received event: {mb_event:?}");

                    if let Some(err) = mb_error {
                        let error_str = err.to_string();
                        if error_str.contains("Unknown event:") {
                            return;
                        }

                        tracing::warn!("Subscription event error: {}", err);
                        return;
                    }

                    if let Some(event) = mb_event {
                        let idx = index.fetch_add(1, Ordering::Relaxed);
                        let idx_event = IndexedPumpfunEvent { _index: idx, event };

                        let sender_clone = pumpfun_ops_sender.clone();
                        tokio::spawn(async move {
                            if let Err(e) = sender_clone.send(idx_event).await {
                                tracing::warn!("Failed to send Pumpfun event: {e}");
                            }
                        });
                    }
                },
            )
            .await
            .map_err(|e| anyhow::Error::msg(format!("Failed to subscribe: {}", e)))?;

        Ok(subscription)
    }
}
