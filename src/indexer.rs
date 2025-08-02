use std::sync::Arc;

use pumpfun::PumpFun;
use pumpfun::common::stream::{PumpFunEvent, Subscription};
use pumpfun::common::types::{Cluster, PriorityFee};
use solana_commitment_config::CommitmentConfig;
use solana_keypair::Keypair;
use tokio::sync::mpsc::Sender;

pub struct Indexer {
    client: PumpFun,
}

impl Indexer {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: PumpFun::new(
                Arc::new(Keypair::new()),
                Cluster::mainnet(CommitmentConfig::confirmed(), PriorityFee::default()),
            ),
        })
    }

    pub async fn subscribe(
        &self,
        pumpfun_ops_sender: Sender<PumpFunEvent>,
    ) -> anyhow::Result<Subscription> {
        let subscription = self
            .client
            .subscribe(
                Some(CommitmentConfig::confirmed()),
                move |_, mb_event, mb_error, _| {
                    tracing::trace!("Received event: {mb_event:?}");

                    if let Some(err) = mb_error {
                        tracing::warn!("Error in subscription: {}", err);
                        return;
                    }

                    if let Some(event) = mb_event {
                        tracing::trace!("Sending Pumpfun event: {event:?}");

                        let sender_clone = pumpfun_ops_sender.clone();
                        tokio::spawn(async move {
                            if let Err(e) = sender_clone.send(event).await {
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
