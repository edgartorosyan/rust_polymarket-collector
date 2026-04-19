//! Chainlink oracle client — WebSocket price feed + REST historical prices.
//!
//! # WebSocket (live prices)
//! Connects to `wss://ws-live-data.polymarket.com` and streams the same
//! Chainlink oracle prices Polymarket uses to determine the Price to Beat for
//! updown markets.
//!
//! # REST (historical prices)
//! [`ChainlinkClient::price_at`] returns the oracle price at a given Unix
//! timestamp.  It tries the official Chainlink Data Streams REST API first
//! (requires `CHAINLINK_API_KEY` / `CHAINLINK_API_SECRET` env vars), then
//! falls back to Binance 1-minute klines (free, no credentials needed).

use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::error::ChainlinkError;
use super::models::{ChainlinkTick, WsEnvelope};

type HmacSha256 = Hmac<Sha256>;

const WS_URL: &str = "wss://ws-live-data.polymarket.com";
const CL_REST_BASE: &str = "https://api.dataengine.chain.link";
const BINANCE_BASE: &str = "https://api.binance.com/api/v3";

// ─── Symbol / feed mappings ───────────────────────────────────────────────────

fn cl_feed_id(coin: &str) -> &'static str {
    match coin.to_ascii_lowercase().as_str() {
        "btc" => "0x00037da06d56d083fe599397a4769a042d63aa73dc4ef57709d31e9971a5b439",
        "eth" => "0x000359843a543ee2fe414dc14c7e7920ef10f4372990b79d6361cdc0dd1ba782",
        "sol" => "0x00039d9e45394f473ab1d3f49520e5b1721c01e4f1d23e4083b9e73d06b99e0c",
        _ => "0x00037da06d56d083fe599397a4769a042d63aa73dc4ef57709d31e9971a5b439",
    }
}

fn binance_symbol(coin: &str) -> &'static str {
    match coin.to_ascii_lowercase().as_str() {
        "btc" => "BTCUSDT",
        "eth" => "ETHUSDT",
        "sol" => "SOLUSDT",
        _ => "BTCUSDT",
    }
}

// ─── Chainlink Data Streams REST helpers ─────────────────────────────────────

/// Build the HMAC-SHA256 request signature required by the Chainlink Data
/// Streams REST API.
///
/// ```text
/// message   = "{METHOD} {PATH} {sha256_hex(body)} {api_key} {timestamp_ms}"
/// signature = HMAC-SHA256(key = api_secret, msg = message)  →  hex
/// ```
fn cl_sign(method: &str, path: &str, api_key: &str, api_secret: &str, ts_ms: u64) -> String {
    let body_hash = hex::encode(Sha256::digest(b""));
    let message = format!("{method} {path} {body_hash} {api_key} {ts_ms}");
    let mut mac =
        HmacSha256::new_from_slice(api_secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(message.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Parse `benchmarkPrice` (int192, scaled 1e18) from a v3 `fullReport` hex blob.
fn parse_benchmark_price(full_report_hex: &str) -> Option<f64> {
    let hex = full_report_hex.trim_start_matches("0x");
    if hex.len() < 512 {
        return None;
    }
    // Skip 32-byte context (64 hex chars), benchmarkPrice is slot 6.
    let slot = &hex[64 + 6 * 64..64 + 7 * 64];
    let raw = u128::from_str_radix(&slot[slot.len().saturating_sub(32)..], 16).ok()?;
    Some(raw as f64 / 1e18)
}

// ─── Client ───────────────────────────────────────────────────────────────────

pub struct ChainlinkClient {
    http: reqwest::Client,
}

impl ChainlinkClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    // ── REST: historical price ────────────────────────────────────────────────

    /// Returns the Chainlink oracle price of `coin` at `ts_secs` (Unix seconds).
    ///
    /// Tries the Chainlink Data Streams REST API first (requires
    /// `CHAINLINK_API_KEY` + `CHAINLINK_API_SECRET` env vars).  Falls back to
    /// the Binance 1-minute open price for the candle that contains `ts_secs`.
    pub async fn price_at(&self, coin: &str, ts_secs: u64) -> Option<f64> {
        if let (Ok(key), Ok(secret)) = (
            std::env::var("CHAINLINK_API_KEY"),
            std::env::var("CHAINLINK_API_SECRET"),
        ) {
            if let Some(p) = self.cl_price_at(coin, ts_secs, &key, &secret).await {
                return Some(p);
            }
        }
        self.binance_price_at(coin, ts_secs).await
    }

    async fn cl_price_at(
        &self,
        coin: &str,
        ts_secs: u64,
        api_key: &str,
        api_secret: &str,
    ) -> Option<f64> {
        let feed_id = cl_feed_id(coin);
        let path = format!("/api/v1/reports?feedID={feed_id}&timestamp={ts_secs}");

        let ts_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let sig = cl_sign("GET", &path, api_key, api_secret, ts_ms);
        let url = format!("{CL_REST_BASE}{path}");

        crate::metrics::record_http_request("chainlink_rest");
        let json: serde_json::Value = self
            .http
            .get(&url)
            .header("Authorization", api_key)
            .header("X-Authorization-Timestamp", ts_ms.to_string())
            .header("X-Authorization-Signature-SHA256", sig)
            .send()
            .await
            .ok()?
            .json()
            .await
            .ok()?;

        parse_benchmark_price(json["report"]["fullReport"].as_str()?)
    }

    async fn binance_price_at(&self, coin: &str, ts_secs: u64) -> Option<f64> {
        let symbol = binance_symbol(coin);
        let start_ms = ts_secs * 1000;
        let url = format!(
            "{BINANCE_BASE}/klines?symbol={symbol}&interval=1m&startTime={start_ms}&limit=1"
        );
        crate::metrics::record_http_request("binance_rest");
        // Kline response: [[open_time, open, high, low, close, ...]]
        let rows: Vec<Vec<serde_json::Value>> =
            self.http.get(&url).send().await.ok()?.json().await.ok()?;
        rows.first()?.get(1)?.as_str()?.parse().ok()
    }

    // ── WebSocket: live price stream ──────────────────────────────────────────

    /// Subscribe to Chainlink price updates for a single `symbol`
    /// (e.g. `"btc/usd"`) and call `on_tick` for every update.
    pub async fn stream_price(
        &self,
        symbol: &str,
        mut on_tick: impl FnMut(ChainlinkTick),
    ) -> anyhow::Result<()> {
        let subscribe_msg = serde_json::json!({
            "action": "subscribe",
            "subscriptions": [{
                "topic": "crypto_prices_chainlink",
                "type": "*",
                "filters": serde_json::json!({ "symbol": symbol }).to_string()
            }]
        })
        .to_string();

        loop {
            match connect_async(WS_URL).await {
                Err(e) => {
                    eprintln!("[chainlink] connect error: {e}, retrying in 3s...");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                Ok((mut ws, _)) => {
                    if let Err(e) = ws.send(Message::Text(subscribe_msg.clone().into())).await {
                        eprintln!("[chainlink] subscribe error: {e}, reconnecting...");
                        continue;
                    }
                    eprintln!("[chainlink] subscribed to crypto_prices_chainlink/{symbol}");
                    Self::pump(&mut ws, &mut on_tick).await;
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
    }

    /// Subscribe to **multiple symbols** simultaneously on a single connection.
    /// `symbols` should be lowercase, e.g. `["btc/usd", "eth/usd"]`.
    pub async fn stream_prices(
        &self,
        symbols: &[&str],
        mut on_tick: impl FnMut(ChainlinkTick),
    ) -> anyhow::Result<()> {
        if symbols.is_empty() {
            return Err(ChainlinkError::EmptySymbols.into());
        }

        let subscriptions: Vec<_> = symbols
            .iter()
            .map(|s| {
                serde_json::json!({
                    "topic": "crypto_prices_chainlink",
                    "type": "*",
                    "filters": serde_json::json!({ "symbol": s }).to_string()
                })
            })
            .collect();

        let subscribe_msg = serde_json::json!({
            "action": "subscribe",
            "subscriptions": subscriptions
        })
        .to_string();

        loop {
            match connect_async(WS_URL).await {
                Err(e) => {
                    eprintln!("[chainlink] connect error: {e}, retrying in 3s...");
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                Ok((mut ws, _)) => {
                    if let Err(e) = ws.send(Message::Text(subscribe_msg.clone().into())).await {
                        eprintln!("[chainlink] subscribe error: {e}, reconnecting...");
                        continue;
                    }
                    eprintln!("[chainlink] subscribed to {} symbol(s)", symbols.len());
                    Self::pump(&mut ws, &mut on_tick).await;
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
            }
        }
    }

    /// Drive the receive loop until the connection drops, calling `on_tick` for
    /// every price update and responding to server keep-alive pings.
    async fn pump<S>(ws: &mut S, on_tick: &mut impl FnMut(ChainlinkTick))
    where
        S: futures_util::Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>>
            + futures_util::Sink<Message>
            + Unpin,
    {
        while let Some(msg) = ws.next().await {
            let text = match msg {
                Ok(Message::Text(t)) => t.to_string(),
                Ok(Message::Close(_)) => {
                    eprintln!("[chainlink] server closed, reconnecting...");
                    break;
                }
                Err(e) => {
                    eprintln!("[chainlink] read error: {e}, reconnecting...");
                    break;
                }
                _ => continue,
            };

            let envelope: WsEnvelope = match serde_json::from_str(&text) {
                Ok(e) => e,
                Err(_) => continue,
            };

            if envelope.kind == "ping" {
                let pong = serde_json::json!({ "action": "pong" }).to_string();
                let _ = ws.send(Message::Text(pong.into())).await;
                continue;
            }

            if envelope.topic == "crypto_prices_chainlink"
                || envelope.topic == "crypto_prices"
            {
                if let Some(payload) = envelope.payload {
                    on_tick(ChainlinkTick {
                        symbol: payload.symbol,
                        timestamp_ms: payload.timestamp,
                        value: payload.value,
                    });
                }
            }
        }
    }
}
