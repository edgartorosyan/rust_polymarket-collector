//! WebSocket client for Polymarket CLOB orderbook data.
//!
//! Connects to `wss://ws-subscriptions-clob.polymarket.com/ws/market` and
//! subscribes to orderbook events for the given CLOB token IDs (Up / Down
//! outcome tokens).
//!
//! Two event types are emitted by the server:
//! - **`book`** — full orderbook snapshot sent immediately after subscribing.
//! - **`price_change`** — incremental update to a single price level; a size
//!   of `0` means the level was removed.
//!
//! # Usage
//! ```rust
//! ws_orderbook::stream_orderbook(&[up_token_id, down_token_id], |ev| {
//!     match ev {
//!         OrderbookEvent::Snapshot(s) => println!("snap {} bids {} asks", s.asset_id, s.bids.len(), s.asks.len()),
//!         OrderbookEvent::Update(u)   => println!("upd {} {} @ {}", u.asset_id, u.side, u.price),
//!     }
//! }).await?;
//! ```

use anyhow::bail;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};

const CLOB_WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawLevel {
    price: String,
    size: String,
}

#[derive(Debug, Deserialize)]
struct RawEvent {
    event_type: String,
    #[serde(default)]
    asset_id: String,
    #[serde(default)]
    market: String,
    /// Present on `book` events — full bid ladder.
    #[serde(default)]
    bids: Vec<RawLevel>,
    /// Present on `book` events — full ask ladder.
    #[serde(default)]
    asks: Vec<RawLevel>,
    /// Present on `price_change` events.
    price: Option<String>,
    side: Option<String>,
    size: Option<String>,
    timestamp: Option<String>,
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// A single price level in the orderbook.
#[derive(Debug, Clone)]
pub struct Level {
    pub price: f64,
    pub size: f64,
}

/// Full orderbook snapshot for one asset, sent once on subscription.
#[derive(Debug, Clone)]
pub struct BookSnapshot {
    pub asset_id: String,
    /// Condition ID of the market.
    pub market: String,
    /// Bid levels, best (highest) price first.
    pub bids: Vec<Level>,
    /// Ask levels, best (lowest) price first.
    pub asks: Vec<Level>,
    /// Millisecond Unix timestamp from the server.
    pub timestamp_ms: u64,
}

/// Incremental update to a single price level.
///
/// A `size` of `0.0` means the level was removed from the book.
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub asset_id: String,
    /// Condition ID of the market.
    pub market: String,
    /// `"BUY"` (bid side) or `"SELL"` (ask side).
    pub side: String,
    pub price: f64,
    /// New resting size. `0.0` = level removed.
    pub size: f64,
    /// Millisecond Unix timestamp from the server.
    pub timestamp_ms: u64,
}

/// An orderbook event emitted by [`stream_orderbook`].
#[derive(Debug, Clone)]
pub enum OrderbookEvent {
    /// Full snapshot received immediately after subscribing (one per asset).
    Snapshot(BookSnapshot),
    /// Incremental price-level change.
    Update(BookUpdate),
}

// ─── Public API ───────────────────────────────────────────────────────────────

/// Subscribe to orderbook events for `asset_ids` (Up/Down token IDs) via the
/// Polymarket CLOB market WebSocket.
///
/// - On connection the server sends one [`OrderbookEvent::Snapshot`] per asset.
/// - Subsequent [`OrderbookEvent::Update`] events carry incremental changes.
/// - Reconnects automatically with a 3-second delay on any error; the caller
///   will receive a fresh snapshot on each reconnect.
pub async fn stream_orderbook(
    asset_ids: &[&str],
    mut on_event: impl FnMut(OrderbookEvent),
) -> anyhow::Result<()> {
    if asset_ids.is_empty() {
        bail!("asset_ids must not be empty");
    }

    let subscribe_msg = serde_json::json!({
        "auth": {},
        "markets": [],
        "assets_ids": asset_ids,
    })
    .to_string();

    loop {
        match connect_async(CLOB_WS_URL).await {
            Err(e) => {
                eprintln!("[ws_orderbook] connect error: {e}, retrying in 3s...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            Ok((mut ws, _)) => {
                if let Err(e) = ws.send(Message::Text(subscribe_msg.clone().into())).await {
                    eprintln!("[ws_orderbook] subscribe error: {e}, reconnecting...");
                    continue;
                }

                eprintln!("[ws_orderbook] subscribed to {} asset(s)", asset_ids.len());

                while let Some(msg) = ws.next().await {
                    let text = match msg {
                        Ok(Message::Text(t)) => t.to_string(),
                        Ok(Message::Close(_)) => {
                            eprintln!("[ws_orderbook] server closed, reconnecting...");
                            break;
                        }
                        Err(e) => {
                            eprintln!("[ws_orderbook] read error: {e}, reconnecting...");
                            break;
                        }
                        _ => continue,
                    };

                    // The server sends a JSON array of events per frame.
                    let raw_events: Vec<RawEvent> = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    for ev in raw_events {
                        let timestamp_ms = ev
                            .timestamp
                            .as_deref()
                            .and_then(|t| t.parse::<u64>().ok())
                            .unwrap_or(0);

                        match ev.event_type.as_str() {
                            "book" => {
                                let bids = parse_levels(&ev.bids);
                                let asks = parse_levels(&ev.asks);
                                on_event(OrderbookEvent::Snapshot(BookSnapshot {
                                    asset_id: ev.asset_id,
                                    market: ev.market,
                                    bids,
                                    asks,
                                    timestamp_ms,
                                }));
                            }
                            "price_change" => {
                                let price = ev
                                    .price
                                    .as_deref()
                                    .and_then(|p| p.parse::<f64>().ok())
                                    .unwrap_or(f64::NAN);
                                let size = ev
                                    .size
                                    .as_deref()
                                    .and_then(|s| s.parse::<f64>().ok())
                                    .unwrap_or(0.0);
                                on_event(OrderbookEvent::Update(BookUpdate {
                                    asset_id: ev.asset_id,
                                    market: ev.market,
                                    side: ev.side.unwrap_or_default(),
                                    price,
                                    size,
                                    timestamp_ms,
                                }));
                            }
                            _ => {}
                        }
                    }
                }

                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
}

fn parse_levels(raw: &[RawLevel]) -> Vec<Level> {
    raw.iter()
        .filter_map(|l| {
            let price = l.price.parse::<f64>().ok()?;
            let size = l.size.parse::<f64>().ok()?;
            Some(Level { price, size })
        })
        .collect()
}
