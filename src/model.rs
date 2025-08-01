use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Candle {
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "resolution")]
pub enum Resolution {
    S1,
    M1,
    M5,
    M15,
    H1,
    D1,
}

impl Resolution {
    pub fn to_seconds(&self) -> u64 {
        match self {
            Resolution::S1 => 1,
            Resolution::M1 => 60,
            Resolution::M5 => 300,
            Resolution::M15 => 900,
            Resolution::H1 => 3600,
            Resolution::D1 => 86400,
        }
    }

    pub fn to_millis(&self) -> u64 {
        self.to_seconds() * 1000
    }

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
}

#[derive(Debug)]
pub struct TradeInfo {
    pub mint_acc: String,
    pub sol_amount: u64,
    pub token_amount: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TradeOhlcv {
    pub timestamp: u64,
    pub candle: Candle,
}