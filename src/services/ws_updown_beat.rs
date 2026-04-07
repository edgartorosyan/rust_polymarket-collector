//! WebSocket client for Polymarket CLOB market prices (Up / Down tokens).
//!
//! Connects to `wss://ws-subscriptions-clob.polymarket.com/ws/market` and
//! subscribes to price-change events for Up and Down token IDs. Each event
//! carries the updated best bid / ask for one asset.
//!
//! For the Chainlink oracle feed (price to beat) see [`crate::chainlink_client`].

use std::collections::HashMap;

use anyhow::bail;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const CLOB_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PriceChangeItem {
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    best_bid: String,
    #[serde(default)]
    best_ask: String,
}

#[derive(Debug, Deserialize)]
struct ClobLevel {
    price: String,
    #[allow(dead_code)]
    size: String,
}

/// Top-level message from the CLOB market WS — always a single JSON object.
///
/// `price_change` messages bundle all updated assets in `price_changes`.
/// `book` messages carry a full or partial orderbook for one `asset_id`.
#[derive(Debug, Deserialize)]
struct ClobMessage {
    event_type: String,
    #[serde(default)]
    market: String,
    timestamp: Option<String>,
    #[serde(default)]
    price_changes: Vec<PriceChangeItem>,
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    bids: Vec<ClobLevel>,
    #[serde(default)]
    asks: Vec<ClobLevel>,
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// Current best bid / ask for one Up or Down outcome token.
///
/// Emitted for every `price_change` event (one tick per asset in the batch)
/// and for every `book` event (best prices derived from the last bid/ask level).
#[derive(Debug, Clone)]
pub struct ClobPriceTick {
    /// CLOB token ID (the Up or Down outcome token).
    pub asset_id: String,
    /// Condition ID of the market this token belongs to.
    pub market: String,
    /// Current best bid price (0–1, USDC per share).
    pub best_bid: f64,
    /// Current best ask price (0–1, USDC per share).
    pub best_ask: f64,
    /// Millisecond Unix timestamp from the server.
    pub timestamp_ms: u64,
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn build_subscribe_msg(asset_ids: &[&str]) -> String {
    serde_json::json!({
        "auth": {},
        "markets": [],
        "assets_ids": asset_ids,
    })
    .to_string()
}

fn parse_timestamp(raw: &Option<String>) -> u64 {
    raw.as_deref()
        .and_then(|t| t.parse::<u64>().ok())
        .unwrap_or(0)
}

/// The CLOB WS sends either a single JSON object `{...}` (for `price_change`)
/// or a JSON array `[{...}, ...]` (for `book` snapshots). Parse both forms
/// into a flat `Vec<ClobMessage>`.
fn parse_clob_frame(text: &str) -> Result<Vec<ClobMessage>, serde_json::Error> {
    if text.starts_with('[') {
        serde_json::from_str::<Vec<ClobMessage>>(text)
    } else {
        serde_json::from_str::<ClobMessage>(text).map(|m| vec![m])
    }
}

fn dispatch_message(
    msg: ClobMessage,
    last_ask: &mut HashMap<String, f64>,
    on_tick: &mut impl FnMut(ClobPriceTick),
) {
    let timestamp_ms = parse_timestamp(&msg.timestamp);

    match msg.event_type.as_str() {
        "price_change" => {
            for item in msg.price_changes {
                let best_bid = item.best_bid.parse::<f64>().unwrap_or(f64::NAN);
                let best_ask = item.best_ask.parse::<f64>().unwrap_or(f64::NAN);
                if last_ask.get(&item.asset_id).copied() == Some(best_ask) {
                    continue;
                }
                last_ask.insert(item.asset_id.clone(), best_ask);
                on_tick(ClobPriceTick {
                    asset_id: item.asset_id,
                    market: msg.market.clone(),
                    best_bid,
                    best_ask,
                    timestamp_ms,
                });
            }
        }
        "book" => {
            let best_bid = msg
                .bids
                .last()
                .and_then(|l| l.price.parse::<f64>().ok())
                .unwrap_or(f64::NAN);
            let best_ask = msg
                .asks
                .last()
                .and_then(|l| l.price.parse::<f64>().ok())
                .unwrap_or(f64::NAN);
            if best_bid.is_finite() || best_ask.is_finite() {
                if last_ask.get(&msg.asset_id).copied() != Some(best_ask) {
                    last_ask.insert(msg.asset_id.clone(), best_ask);
                    on_tick(ClobPriceTick {
                        asset_id: msg.asset_id,
                        market: msg.market,
                        best_bid,
                        best_ask,
                        timestamp_ms,
                    });
                }
            }
        }
        _ => {}
    }
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Subscribe to live price-level updates for `asset_ids` (Up/Down token IDs).
///
/// Fires `on_tick` for every `price_change` or `book` event received.
/// Reconnects automatically with a 3-second delay on any error.
pub async fn stream_clob_prices(
    asset_ids: &[&str],
    mut on_tick: impl FnMut(ClobPriceTick),
) -> anyhow::Result<()> {
    if asset_ids.is_empty() {
        bail!("asset_ids must not be empty");
    }

    let subscribe_msg = build_subscribe_msg(asset_ids);
    let mut last_ask: HashMap<String, f64> = HashMap::new();

    loop {
        last_ask.clear();

        match connect_async(CLOB_WS_URL).await {
            Err(e) => {
                eprintln!("[clob] connect error: {e}, retrying in 3s...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            Ok((mut ws, _)) => {
                if let Err(e) = ws.send(Message::Text(subscribe_msg.clone().into())).await {
                    eprintln!("[clob] subscribe error: {e}, reconnecting...");
                    continue;
                }
                eprintln!("[clob] subscribed to {} asset(s)", asset_ids.len());

                while let Some(raw) = ws.next().await {
                    let text = match raw {
                        Ok(Message::Text(t)) => t.to_string(),
                        Ok(Message::Close(_)) => {
                            eprintln!("[clob] server closed, reconnecting...");
                            break;
                        }
                        Err(e) => {
                            eprintln!("[clob] read error: {e}, reconnecting...");
                            break;
                        }
                        _ => continue,
                    };

                    match parse_clob_frame(&text) {
                        Ok(msgs) => {
                            for msg in msgs {
                                dispatch_message(msg, &mut last_ask, &mut on_tick);
                            }
                        }
                        Err(e) => eprintln!("[clob] parse error: {e}\n  raw: {text}"),
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

/// Like [`stream_clob_prices`] but exits cleanly once `end_ts_secs` (Unix
/// seconds) is reached. Use this when monitoring a single market window so the
/// caller can switch to the next market without cancelling the whole task.
pub async fn stream_clob_prices_until(
    asset_ids: &[&str],
    end_ts_secs: f64,
    mut on_tick: impl FnMut(ClobPriceTick),
) -> anyhow::Result<()> {
    if asset_ids.is_empty() {
        bail!("asset_ids must not be empty");
    }

    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs_f64((end_ts_secs - now_secs).max(0.0));

    let subscribe_msg = build_subscribe_msg(asset_ids);
    let mut last_ask: HashMap<String, f64> = HashMap::new();

    loop {
        if tokio::time::Instant::now() >= deadline {
            return Ok(());
        }

        last_ask.clear();

        match connect_async(CLOB_WS_URL).await {
            Err(e) => {
                eprintln!("[clob] connect error: {e}, retrying in 3s...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            Ok((mut ws, _)) => {
                if let Err(e) = ws.send(Message::Text(subscribe_msg.clone().into())).await {
                    eprintln!("[clob] subscribe error: {e}, reconnecting...");
                    continue;
                }
                eprintln!("[clob] subscribed to {} asset(s) (until expiry)", asset_ids.len());

                loop {
                    let raw = tokio::select! {
                        _ = tokio::time::sleep_until(deadline) => {
                            eprintln!("[clob] market deadline reached, closing stream");
                            let _ = ws.close(None).await;
                            return Ok(());
                        }
                        msg = ws.next() => match msg {
                            None => break,
                            Some(m) => m,
                        }
                    };

                    let text = match raw {
                        Ok(Message::Text(t)) => t.to_string(),
                        Ok(Message::Close(_)) => {
                            eprintln!("[clob] server closed, reconnecting...");
                            break;
                        }
                        Err(e) => {
                            eprintln!("[clob] read error: {e}, reconnecting...");
                            break;
                        }
                        _ => continue,
                    };

                    match parse_clob_frame(&text) {
                        Ok(msgs) => {
                            for msg in msgs {
                                dispatch_message(msg, &mut last_ask, &mut on_tick);
                            }
                        }
                        Err(e) => eprintln!("[clob] parse error: {e}\n  raw: {text}"),
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}
