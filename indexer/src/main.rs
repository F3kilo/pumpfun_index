use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use db::Db;
use serde::Deserialize;
use sqlx::types::chrono::{DateTime, Utc};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::DefaultMakeSpan;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::cache::Cache;
use crate::indexer::Indexer;
use crate::model::{Candle, Resolution, TradeOhlcv};
use crate::pump_handler::PumpHandler;
use crate::storage::Storage;

mod cache;
mod db;
mod indexer;
mod model;
mod pump_handler;
mod storage;

/// State shared between app clients.
struct AppState {
    storage: Storage,
    _indexer: Indexer,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("Starting...");

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

    tracing::info!("Tracing initialized.");

    // Init db connection.
    let conn_str = std::env::var("POSTGRES_CONN_STR")?;
    let db = Db::new(conn_str).await?;
    if let Err(e) = db.init().await {
        tracing::info!("Failed to apply migrations: {e}");
    } else {
        tracing::info!("Migrations applied.");
    };
    tracing::info!("Db initialized.");

    // Init redis connection.
    let conn_str = std::env::var("REDIS_CONN_STR")?;
    let cache = Cache::new(&conn_str).await?;
    tracing::info!("Cache initialized.");

    let storage = Storage::new(db, cache).await;
    tracing::info!("Storage initialized.");

    // Channel to push events from pumpfun to PumpHandler.
    let (tx, rx) = mpsc::channel(1024);

    // Start indexer and event handler.
    let indexer = Indexer::new()?;
    tracing::info!("Indexer initialized.");

    let _subscription = indexer.subscribe(tx).await?;
    tokio::spawn(PumpHandler::run(storage.clone(), rx));
    tracing::info!("PumpHandler initialized.");

    let state = Arc::new(AppState {
        storage,
        _indexer: indexer,
    });

    // CORS are not required for test task.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let serve_dir = ServeDir::new("static");

    let router = Router::new()
        .route("/chart_data_ws/{token}/{resolution}", get(chart_data_ws))
        .route("/tokens", get(get_tokens))
        .nest_service("/static", serve_dir.clone())
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

    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, router.layer(TraceLayer::new_for_http())).await?;

    Ok(())
}

/// Get list of tokens request handler.
async fn get_tokens(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let tokens_result = state.storage.get_tokens().await;

    match tokens_result {
        Ok(tokens) => Json(tokens).into_response(),
        Err(e) => {
            tracing::info!("Failed to get tokens: {e}.");
            Json(format!("Failed to get tokens: {e}.")).into_response()
        }
    }
}

/// Chart params for a WebSocket request handler.
#[derive(Deserialize, Debug)]
struct ChartWsPathParams {
    token: String,
    resolution: Resolution,
}

/// Upgrade HTTP connection into WebSocket.
async fn chart_data_ws(
    Path(path): Path<ChartWsPathParams>,
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| async move {
        let result = handle_websocket(path.token, path.resolution, socket, state).await;
        if let Err(e) = result {
            tracing::warn!("WS connection failure: {e}.");
        }
    })
}

/// History point for a chart.
const POINTS_PER_CHART: usize = 100;

/// Refresh interval for a WebSocket connection.
const PRICE_WS_REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// WebSocket connection handler.
async fn handle_websocket(
    token: String,
    resolution: Resolution,
    mut socket: WebSocket,
    state: Arc<AppState>,
) -> anyhow::Result<()> {
    // Get history data at the start.
    let to_timestamp = Utc::now();
    let step = Duration::from_secs(resolution.as_seconds());
    let from_timestamp = resolution.align_datetime(
        to_timestamp - Duration::from_secs(POINTS_PER_CHART as u64 * resolution.as_seconds()),
    );

    let db_candles = state
        .storage
        .trades_since(&token, from_timestamp, resolution)
        .await
        .inspect_err(|e| tracing::info!("Failed to read prices history: {e}."))
        .unwrap_or_default();

    let candles = interpolate_candles(from_timestamp, to_timestamp, step, db_candles);
    dbg!(&candles);

    for price in candles {
        let json_price = sqlx::types::Json::from(price).encode_to_string()?;
        socket.send(Message::Text(json_price.into())).await?;
    }

    // Send last trade data to the client periodically.
    loop {
        tokio::time::sleep(PRICE_WS_REFRESH_INTERVAL).await;

        let current_timestamp = resolution.align_datetime(Utc::now());
        let (last_db_ts, last_db_candle) = match state.storage.last_trade(&token, resolution).await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::info!("Failed to read last price: {e}.");
                Default::default()
            }
        };

        let candle = if current_timestamp >= last_db_ts && current_timestamp < last_db_ts + step {
            last_db_candle
        } else {
            Candle {
                open: last_db_candle.close,
                close: last_db_candle.close,
                high: last_db_candle.close,
                low: last_db_candle.close,
                volume: 0.0,
            }
        };

        let trade = TradeOhlcv {
            timestamp: current_timestamp.timestamp_millis() as u64 / 1000,
            candle,
        };

        let json_trade = sqlx::types::Json::from(trade).encode_to_string()?;
        socket.send(Message::Text(json_trade.into())).await?;
    }
}

// Fill gaps in trade events.
fn interpolate_candles(
    mut from_timestamp: DateTime<Utc>,
    to_timestamp: DateTime<Utc>,
    step: Duration,
    db_candles: BTreeMap<DateTime<Utc>, Candle>,
) -> Vec<TradeOhlcv> {
    let mut prices = Vec::new();
    while from_timestamp <= to_timestamp {
        let Some((db_timestamp, db_candle)) = db_candles
            .range(..=from_timestamp)
            .next_back()
            .map(|(ts, canlde)| (*ts, *canlde))
        else {
            from_timestamp += step;
            continue;
        };

        let candle = if from_timestamp >= db_timestamp && from_timestamp < db_timestamp + step {
            db_candle
        } else {
            Candle {
                open: db_candle.close,
                close: db_candle.close,
                high: db_candle.close,
                low: db_candle.close,
                volume: 0.0,
            }
        };

        prices.push(TradeOhlcv {
            timestamp: from_timestamp.timestamp_millis() as u64 / 1000,
            candle,
        });

        from_timestamp += step;
    }

    prices
}
