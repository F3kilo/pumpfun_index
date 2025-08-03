use std::fmt;

use borsh::{BorshDeserialize, BorshSerialize};
use pumpfun::common::stream::PumpFunEvent;
use serde::{Deserialize, Serialize};
use sqlx::types::chrono::{DateTime, Utc};

/// Candle with open, close, high, low and volume.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Candle {
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
}

/// Trade events time resolution.
#[derive(Debug, Clone, Copy, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "resolution")]
pub enum Resolution {
    S1,
    M1,
    M5,
    M15,
    H1,
    D1,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Resolution::S1 => write!(f, "S1"),
            Resolution::M1 => write!(f, "M1"),
            Resolution::M5 => write!(f, "M5"),
            Resolution::M15 => write!(f, "M15"),
            Resolution::H1 => write!(f, "H1"),
            Resolution::D1 => write!(f, "D1"),
        }
    }
}

impl Resolution {
    /// Convert resolution to seconds.
    pub fn as_seconds(&self) -> u64 {
        match self {
            Resolution::S1 => 1,
            Resolution::M1 => 60,
            Resolution::M5 => 300,
            Resolution::M15 => 900,
            Resolution::H1 => 3600,
            Resolution::D1 => 86400,
        }
    }

    /// Convert resolution to milliseconds.
    pub fn as_millis(&self) -> u64 {
        self.as_seconds() * 1000
    }

    /// All available resolutions.
    pub fn all() -> [Resolution; 6] {
        [
            Resolution::S1,
            Resolution::M1,
            Resolution::M5,
            Resolution::M15,
            Resolution::H1,
            Resolution::D1,
        ]
    }

    /// Align timestamp to the closest resolution step.
    pub fn align_datetime(&self, timestamp: DateTime<Utc>) -> DateTime<Utc> {
        let ts_millis = timestamp.timestamp_millis() as u64 / self.as_millis() * self.as_millis();
        DateTime::from_timestamp_millis(ts_millis as i64).expect("correct datetime")
    }
}

/// Trade event info.
#[derive(Debug, Clone)]
pub struct TradeInfo {
    pub mint_acc: String,
    pub sol_amount: u64,
    pub token_amount: u64,
}

/// Price data with timestamp.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct TradeOhlcv {
    pub timestamp: u64,
    pub candle: Candle,
}

/// Token metadata.
#[derive(BorshSerialize, BorshDeserialize, Debug, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

/// Indexed pumpfun event.
/// Index may be used for debugging.
#[derive(Debug)]
pub struct IndexedPumpfunEvent {
    pub _index: u64,
    pub event: PumpFunEvent,
}
