use core::fmt;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use egui::scroll_area::ScrollArea;
use egui::{Color32, Response};
use egui_plot::{BoxElem, BoxPlot, BoxSpread, Legend, Plot};
use serde::{Deserialize, Serialize};
use std::sync::mpsc::{self, Receiver, Sender};

pub struct PumpIndexFront {
    indexer_addr: String,
    tokens: BTreeMap<String, String>,
    selected_token: Option<String>,
    error_msg: String,
    prices: BTreeMap<DateTime<Utc>, TradeOhlcv>,
    event_tx: Sender<AsyncEvent>,
    event_rx: Receiver<AsyncEvent>,
}

enum AsyncEvent {
    GotTokens(Vec<(String, TokenMetadata)>),
    GotTrade(TradeOhlcv),
    Error(String),
}

impl Default for PumpIndexFront {
    fn default() -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        Self {
            indexer_addr: "localhost:33987".into(),
            tokens: BTreeMap::default(),
            selected_token: None,
            error_msg: String::new(),
            prices: BTreeMap::default(),
            event_tx,
            event_rx,
        }
    }
}

impl PumpIndexFront {
    /// Called once before the first frame.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Default::default()
    }

    pub fn refresh_tokens(&mut self) {
        let addr = format!("http://{}/tokens", self.indexer_addr);

        execute_async(query_tokens(addr, self.event_tx.clone()));
    }

    fn draw_plot(&mut self, ui: &mut egui::Ui) -> Response {
        let token = self
            .selected_token
            .as_ref()
            .map(String::as_str)
            .unwrap_or("Unknown token");

        let box_plot = BoxPlot::new(
            token,
            self.prices
                .iter()
                .map(|(datetime, trade)| {
                    let color = if trade.candle.open > trade.candle.close {
                        Color32::RED
                    } else {
                        Color32::GREEN
                    };

                    let timestamp = datetime.timestamp_millis();

                    let quart1 = trade.candle.open.min(trade.candle.close);
                    let quart2 = trade.candle.open.max(trade.candle.close);
                    let median = (quart1 + quart2) / 2.0;

                    BoxElem::new(
                        timestamp as f64,
                        BoxSpread::new(trade.candle.low, quart1, median, quart2, trade.candle.high),
                    )
                    .fill(color)
                    .name(datetime.to_string())
                })
                .collect(),
        );

        let name = format!("{} chart", token);
        Plot::new(name)
            .legend(Legend::default())
            .allow_zoom(true)
            .allow_drag(false)
            .allow_scroll(false)
            .show(ui, |plot_ui| {
                plot_ui.box_plot(box_plot);
            })
            .response
    }
}

async fn query_tokens(addr: String, tx: Sender<AsyncEvent>) {
    let response = match reqwest::get(addr).await {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AsyncEvent::Error(format!("Failed to get tokens: {e}")));
            return;
        }
    };

    let tokens_list = match response.json::<Vec<(String, TokenMetadata)>>().await {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AsyncEvent::Error(format!("Failed to parse tokens: {e}")));
            return;
        }
    };

    let _ = tx.send(AsyncEvent::GotTokens(tokens_list));
}

/// Token metadata.
#[derive(Debug, Serialize, Deserialize)]
pub struct TokenMetadata {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}

impl eframe::App for PumpIndexFront {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AsyncEvent::GotTokens(tokens) => {
                    self.tokens = tokens
                        .into_iter()
                        .map(|t| (t.0, format!("{} | ({})", t.1.symbol, t.1.name)))
                        .collect();
                }
                AsyncEvent::GotTrade(trade) => {
                    let datetime = DateTime::from_timestamp_millis(trade.timestamp as i64 * 1000)
                        .expect("correct datetime");
                    self.prices.insert(datetime, trade);
                }
                AsyncEvent::Error(msg) => {
                    self.error_msg = msg;
                }
            }
        }

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Pumpfun tokens indexer");

            ui.separator();

            ScrollArea::vertical().show(ui, |ui| {
                if !self.prices.is_empty() {
                    self.draw_plot(ui);
                }

                ui.horizontal_wrapped(|ui| {
                    if !self.error_msg.is_empty() {
                        ui.label(&self.error_msg);
                    }

                    ui.separator();

                    ui.horizontal(|ui| {
                        ui.label("Select token");
                        if ui.button("Refresh").clicked() {
                            self.refresh_tokens();
                        }
                    });

                    ui.separator();

                    for (token_addr, token_name) in &self.tokens {
                        if ui.button(token_name).clicked() {
                            if self.selected_token.as_ref().map(String::as_str) != Some(token_addr)
                            {
                                start_listen_price(
                                    &self.indexer_addr,
                                    token_addr,
                                    Resolution::M1,
                                    self.event_tx.clone(),
                                );
                            }
                        };
                    }
                });
            });

            ui.separator();

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                powered_by_egui_and_eframe(ui);
                egui::warn_if_debug_build(ui);
            });
        });
    }
}

fn powered_by_egui_and_eframe(ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.label("Powered by ");
        ui.hyperlink_to("egui", "https://github.com/emilk/egui");
        ui.label(" and ");
        ui.hyperlink_to(
            "eframe",
            "https://github.com/emilk/egui/tree/master/crates/eframe",
        );
        ui.label(".");
    });
}

fn start_listen_price(
    indexer_addr: &str,
    token_addr: &str,
    resolution: Resolution,
    tx: Sender<AsyncEvent>,
) {
    let url = format!(
        "ws://{}/chart_data_ws/{}/{}",
        indexer_addr, token_addr, resolution
    );

    execute_async(listen_price(url, tx));
}

/// Trade events time resolution.
#[derive(Debug, Clone, Copy)]
pub enum Resolution {
    S1,
    M1,
    M5,
    M15,
    H1,
    D1,
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

/// Candle with open, close, high, low and volume.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct Candle {
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
}

/// Price data with timestamp.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub struct TradeOhlcv {
    pub timestamp: u64,
    pub candle: Candle,
}

async fn listen_price(url: String, tx: Sender<AsyncEvent>) {
    let options = ewebsock::Options::default();
    // see documentation for more options
    let (_sender, receiver) = match ewebsock::connect(url, options) {
        Ok(r) => r,
        Err(e) => {
            let _ = tx.send(AsyncEvent::Error(format!("Failed to connect: {e}")));
            return;
        }
    };

    while let Some(ewebsock::WsEvent::Message(ewebsock::WsMessage::Text(text))) =
        receiver.try_recv()
    {
        
        let trade = match serde_json::from_str(&text) {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(AsyncEvent::Error(format!("Failed to parse trade: {e}")));
                return;
            }
        };
        let _ = tx.send(AsyncEvent::GotTrade(trade));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn execute_async<F: Future<Output = ()> + Send + 'static>(f: F) {
    // this is stupid... use any executor of your choice instead
    std::thread::spawn(move || futures::executor::block_on(f));
}

#[cfg(target_arch = "wasm32")]
fn execute_async<F: Future<Output = ()> + 'static>(f: F) {
    wasm_bindgen_futures::spawn_local(f);
}
