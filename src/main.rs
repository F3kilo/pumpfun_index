use axum::Router;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::routing::get;
use db::Db;
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::types::chrono::Utc;
use std::time::Duration;
use tokio::sync::broadcast;
use tower_http::services::ServeDir;
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod indexer;
mod db;

/// State shared between app clients.
struct AppState {
    db: Db,
    prices_rx: broadcast::Receiver<Price>,
}

impl Clone for AppState {
    fn clone(&self) -> Self {
        Self {
            db: self.db.clone(),
            prices_rx: self.prices_rx.resubscribe(),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into()
            }),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    if let Err(e) = dotenv::dotenv() {
        tracing::info!("Failed to load .env file: {e}");
    };

    let conn_str = std::env::var("POSTGRES_CONN_STR")?;
    let db = Db::new(conn_str).await?;
    if let Err(e) = db.init().await {
        // This may happen if db and tables already exist.
        tracing::info!("Failed to apply migrations: {e}");
    } else {
        tracing::info!("Migrations applied.");
    };

    let db_clone = db.clone();
    let (tx, rx) = broadcast::channel(1024);
    tokio::spawn(prices_update_routine(db_clone, tx));

    let state = AppState { db, prices_rx: rx };
    let router = Router::new()
        .route("/price_ws", get(price_ws))
        .fallback_service(ServeDir::new("assets"))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .with_state(state);

    let addr = "0.0.0.0:33987";
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to init TCP listener");

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router.layer(TraceLayer::new_for_http())).await?;

    Ok(())
}

/// How often to query prices from a service.
const QUERY_PERIOD: Duration = Duration::from_secs(1);

async fn prices_update_routine(db: Db, tx: broadcast::Sender<Price>) {
    let mut last_timestamp = Option::<u64>::None;
    // let agent = Agent::new().await.expect("Failed to create Agent.");

    // loop {
    //     if let Err(e) = update_price(&agent, &db, &tx, &mut last_timestamp).await {
    //         tracing::warn!("Routine failure: {e}");
    //     }
    //     tokio::time::sleep(QUERY_PERIOD).await;
    // }
}

// async fn update_price(
//     agent: &Agent,
//     db: &Db,
//     tx: &broadcast::Sender<Price>,
//     last_timestamp: &mut Option<u64>,
// ) -> anyhow::Result<()> {
    // let price = match scrap::query_price(agent).await {
    //     Ok(p) => p,
    //     Err(e) => {
    //         anyhow::bail!("Failed to query price: {e}.");
    //     }
    // };

    // if last_timestamp.unwrap_or_default() < price.bitcoin.last_updated_at {
    //     let _ = tx.send(price);
    //     if let Err(e) = db.push_price(price).await {
    //         anyhow::bail!("Failed to update price: {e}.");
    //     };
    //     *last_timestamp = Some(price.bitcoin.last_updated_at);
    // }

//     Ok(())
// }

/// Upgrade HTTP connection into WebSocket.
async fn price_ws(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async {
        let result = handle_socket(socket, state).await;
        if let Err(e) = result {
            tracing::warn!("WS connection failure: {e}.");
        }
    })
}

async fn handle_socket(mut socket: WebSocket, mut state: AppState) -> anyhow::Result<()> {
    let timestamp = (Utc::now().timestamp_millis() / 1000) - 600;
    let prices = match state.db.prices_since(timestamp).await {
        Ok(p) => p,
        Err(e) => {
            tracing::info!("Failed to read prices history: {e}.");
            vec![]
        }
    };

    for price in prices {
        let json_price = Json::from(price).encode_to_string()?;
        socket.send(Message::Text(json_price.into())).await?;
    }

    while let Ok(price) = state.prices_rx.recv().await {
        let json_price = Json::from(price).encode_to_string()?;
        socket.send(Message::Text(json_price.into())).await?;
    }

    Ok(())
}

/// Price data.
#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
struct Price {
    /// BTC info.
    pub bitcoin: PriceInfo,
}

/// USD value.
#[derive(Default, Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PriceInfo {
    /// Price value.
    pub usd: f64,

    /// Last update unix timestamp.
    pub last_updated_at: u64,
}
