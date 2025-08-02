use borsh::{BorshDeserialize, BorshSerialize};
use pumpfun::PumpFun;
use pumpfun::common::stream::{CreateEvent, PumpFunEvent, TradeEvent};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_rpc_client_types::config::RpcAccountInfoConfig;
use sqlx::types::chrono::DateTime;
use tokio::sync::mpsc::Receiver;

use crate::model::{IndexedPumpfunEvent, Resolution, TokenMetadata, TradeInfo};
use crate::storage::Storage;

/// Pumpfun event processor.
#[derive(Debug)]
pub struct PumpHandler;

impl PumpHandler {
    /// Run event processing task.
    pub async fn run(storage: Storage, mut pumpfun_ops_sender: Receiver<IndexedPumpfunEvent>) {
        while let Some(event) = pumpfun_ops_sender.recv().await {
            tracing::debug!("Received event");

            let storage = storage.clone();
            tokio::spawn(async move {
                if let Err(e) = Self::handle_event(event, &storage).await {
                    tracing::warn!("Failed to handle event: {e}");
                }
            });
        }

        tracing::error!("Pumpfun handler exited");
    }

    /// Handle pumpfun event.
    /// If token first met in trade/create event, it will be inserted into db with metadata.
    async fn handle_event(idx_event: IndexedPumpfunEvent, storage: &Storage) -> anyhow::Result<()> {
        let result = match idx_event.event {
            PumpFunEvent::Create(create) => Self::handle_create(storage, create).await,
            PumpFunEvent::Trade(trade) => Self::handle_trade(storage, trade).await,
            _ => Ok(()),
        };
        tracing::debug!("Handled event: {}", idx_event.index);
        result
    }

    /// Handle create event.
    async fn handle_create(storage: &Storage, create: CreateEvent) -> anyhow::Result<()> {
        let metadata = Self::query_token_metadata(create.mint)
            .await
            .inspect_err(|e| tracing::warn!("Failed to query token metadata: {e}"))
            .ok();

        storage
            .insert_token_metadata(create.mint.to_string(), metadata)
            .await?;

        Ok(())
    }

    /// Handle trade event.
    async fn handle_trade(storage: &Storage, trade: TradeEvent) -> anyhow::Result<()> {
        let times = Resolution::all()
            .iter()
            .map(|res| {
                // bind time to resolution
                let timestamp_millis = trade.timestamp as u64 / res.to_seconds() * res.to_millis();
                DateTime::from_timestamp_millis(timestamp_millis as _).expect("correct datetime")
            })
            .collect::<Vec<_>>();

        let trade_info = TradeInfo {
            mint_acc: trade.mint.to_string(),
            sol_amount: trade.sol_amount,
            token_amount: trade.token_amount,
        };

        if storage
            .get_token_metadata(&trade_info.mint_acc)
            .await
            .is_err()
        {
            let metadata = Self::query_token_metadata(trade.mint)
                .await
                .inspect_err(|e| tracing::warn!("Failed to query token metadata: {e}"))
                .ok();

            storage
                .insert_token_metadata(trade_info.mint_acc.clone(), metadata)
                .await?;
        }

        storage.insert_trade(&times, trade_info).await?;

        Ok(())
    }

    /// Query token metadata.
    async fn query_token_metadata(mint: Pubkey) -> anyhow::Result<TokenMetadata> {
        let metadata_pda = PumpFun::get_metadata_pda(&mint);
        let resp =
            solana_rpc_client::rpc_client::RpcClient::new("https://api.mainnet-beta.solana.com")
                .get_account_with_config(
                    &metadata_pda,
                    RpcAccountInfoConfig {
                        encoding: Some(UiAccountEncoding::Base64),
                        commitment: Some(CommitmentConfig::confirmed()),
                        ..Default::default()
                    },
                )?;
        let Some(acc) = resp.value else {
            anyhow::bail!("Account value with token metadata not found");
        };
        let mut metadata_acc = MetadataAccount::deserialize(&mut acc.data.as_ref())?;

        metadata_acc.data.name = metadata_acc.data.name.trim_end_matches("\0").to_string();
        metadata_acc.data.symbol = metadata_acc.data.symbol.trim_end_matches("\0").to_string();
        metadata_acc.data.uri = metadata_acc.data.uri.trim_end_matches("\0").to_string();

        Ok(metadata_acc.data)
    }
}

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct MetadataAccount {
    pub key: u8,
    pub update_authority: Pubkey,
    pub mint: Pubkey,
    pub data: TokenMetadata,
    pub primary_sale_happened: bool,
    pub is_mutable: bool,
}
