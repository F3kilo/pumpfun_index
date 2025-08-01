use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use db::Db;
use pumpfun::common::stream::Subscription;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::TraceLayer;
use tracing_subscriber::fmt::format;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::indexer::Indexer;
use crate::pump_handler::PumpHandler;

mod cache;
mod db;
mod indexer;
mod model;
mod pump_handler;

/// State shared between app clients.
struct AppState {
    db: Db,
    indexer: Indexer,
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

    let (tx, rx) = mpsc::channel(1024);

    let indexer = Indexer::new()?;
    let _subscription = indexer.subscribe(tx).await?;
    tokio::spawn(PumpHandler::run(db.clone(), rx));

    let state = Arc::new(AppState { db, indexer });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let serve_dir = ServeDir::new("assets");

    let router = Router::new()
        .route("/chart_data_ws/{token}", get(chart_data_ws))
        .route("/tokens", get(get_tokens))
        .nest_service("/assets", serve_dir.clone())
        .fallback_service(serve_dir)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        )
        .layer(cors)
        .with_state(state);

    let addr = "0.0.0.0:33987";
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to init TCP listener");

    tracing::debug!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router.layer(TraceLayer::new_for_http())).await?;

    Ok(())
}

async fn get_tokens(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tokens_result = state.db.get_tokens().await;

    match tokens_result {
        Ok(tokens) => Json(tokens).into_response(),
        Err(e) => {
            tracing::info!("Failed to get tokens: {e}.");
            Json(format!("Failed to get tokens: {e}.")).into_response()
        }
    }
}

/// Upgrade HTTP connection into WebSocket.
async fn chart_data_ws(
    Path(token): Path<String>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async {
        let result = handle_websocket(token, socket, state).await;
        if let Err(e) = result {
            tracing::warn!("WS connection failure: {e}.");
        }
    })
}

async fn handle_websocket(
    token: String,
    mut socket: WebSocket,
    mut state: Arc<AppState>,
) -> anyhow::Result<()> {
    let mut last_timestamp = Utc::now();
    let timestamp = last_timestamp - Duration::from_secs(1000);
    let prices = match state
        .db
        .trades_since(token.clone(), timestamp, model::Resolution::M1)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            tracing::info!("Failed to read prices history: {e}.");
            vec![]
        }
    };

    if let Some(last_price) = prices.last() {
        last_timestamp = DateTime::from_timestamp_millis(last_price.timestamp as i64 * 1000)
            .expect("correct datetime");
    }

    for price in prices {
        let json_price = sqlx::types::Json::from(price).encode_to_string()?;
        socket.send(Message::Text(json_price.into())).await?;
    }

    loop {
        let prices = match state
            .db
            .trades_since(token.clone(), last_timestamp, model::Resolution::M1)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::info!("Failed to read prices history: {e}.");
                vec![]
            }
        };

        if let Some(last_price) = prices.last() {
            last_timestamp = DateTime::from_timestamp_millis(last_price.timestamp as i64 * 1000)
                .expect("correct datetime");
        }

        for price in prices {
            let json_price = sqlx::types::Json::from(price).encode_to_string()?;
            socket.send(Message::Text(json_price.into())).await?;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
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
