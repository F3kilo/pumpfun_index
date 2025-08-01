use std::pin::Pin;

use futures_util::Stream;
use solana_commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_pubkey::{Pubkey, pubkey};
use solana_pubsub_client::nonblocking::pubsub_client;
use solana_pubsub_client::pubsub_client::PubsubAccountClientSubscription;
use solana_rpc_client_types::config::{
    RpcAccountInfoConfig, RpcBlockSubscribeConfig, RpcBlockSubscribeFilter,
};
use solana_rpc_client_types::response::{Response, RpcBlockUpdate};
use solana_transaction_status_client_types::{TransactionDetails, UiTransactionEncoding};
use tokio::sync::broadcast::Sender;

const PUMPFUN_ADDR_STR: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
// const PUMPFUN_ADDR: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");

type BlocksStream = Pin<Box<dyn Stream<Item = Response<RpcBlockUpdate>> + Send>>;

pub struct Indexer {
    client: pubsub_client::PubsubClient,
    pumpfun_blocks_stream: BlocksStream,
}

impl Indexer {
    pub async fn run(api_url: &str, pumpfun_ops_sender: Sender<PumpfunOp>) -> anyhow::Result<()> {
        let filter = RpcBlockSubscribeFilter::MentionsAccountOrProgram(PUMPFUN_ADDR_STR.into());

        let config = RpcBlockSubscribeConfig {
            commitment: Some(CommitmentConfig {
                commitment: CommitmentLevel::Finalized,
            }),
            encoding: Some(UiTransactionEncoding::JsonParsed),
            transaction_details: Some(TransactionDetails::Full),
            ..Default::default()
        };

        let client = pubsub_client::PubsubClient::new(api_url).await?;

        let (pumpfun_blocks_stream, _) = client.block_subscribe(filter, Some(config)).await?;

        
    }
}
