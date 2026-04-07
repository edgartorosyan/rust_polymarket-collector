//! Perpetual updown market monitor.
//!
//! [`run_updown_monitor`] loops forever: it discovers the active market window,
//! resolves the strike price, streams CLOB price ticks until the window
//! expires, then immediately searches for the next window and starts again.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::chainlink_client::ChainlinkClient;
use crate::polymarket_client::PolymarketClient;
use crate::services::polymarket_updown_service::find_active_updown_market;
use crate::services::ws_updown_beat::{stream_clob_prices_until, ClobPriceTick};

/// A CLOB price tick enriched with the current market context.
///
/// `up_id` / `down_id` change when a new market window starts — detect
/// rotation by comparing `up_id` to the previously seen value.
pub struct UpdownTick {
    /// Token ID of the "Up" outcome for the current window.
    pub up_id: String,
    /// Token ID of the "Down" outcome for the current window.
    pub down_id: String,
    /// Unix timestamp when the current window expires.
    pub end_ts: f64,
    /// Strike price for the current window.
    ///
    /// Resolution order:
    /// 1. `GammaMarket.group_line` — set by Polymarket from the Chainlink oracle
    /// 2. `GammaMarket.line`       — fallback for older / manual markets
    /// 3. `ChainlinkClient.price_at(start_ts)` — if Gamma didn't carry the field
    ///    and the window has already opened (Binance kline as last resort)
    /// 4. `None` — future window not yet opened; will remain `None` this session
    pub strike_price: Option<f64>,
    /// The raw CLOB price update.
    pub inner: ClobPriceTick,
}

fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}

/// Runs a perpetual monitor loop for the given `coin` / `window_minutes` pair.
///
/// For each market window the function:
/// 1. Discovers the active updown market (retries every 10 s if none is live).
/// 2. Resolves the strike price (Gamma → Chainlink REST → None).
/// 3. Resolves the Up / Down token IDs from the CLOB.
/// 4. Calls `on_tick` for every price update until the window expires.
/// 5. Waits 5 s then repeats from step 1.
///
/// Returns `Err` only if a non-retryable API failure occurs.
pub async fn run_updown_monitor<F>(
    polymarket: &PolymarketClient,
    chainlink: &ChainlinkClient,
    coin: &str,
    window_minutes: u64,
    mut on_tick: F,
) -> anyhow::Result<()>
where
    F: FnMut(UpdownTick),
{
    loop {
        // ── 1. Discover the active market, retrying until one is live ─────────
        let market = loop {
            match find_active_updown_market(polymarket, coin, window_minutes, 3).await? {
                Some(m) => break m,
                None => {
                    eprintln!("[monitor] no active {coin} {window_minutes}m market found, retrying in 10s...");
                    tokio::time::sleep(Duration::from_secs(10)).await;
                }
            }
        };

        eprintln!("[monitor] market: {} (ends {})", market.title, market.end_date);

        // ── 2. Resolve strike price ───────────────────────────────────────────
        //
        // Primary: Polymarket already recorded it in the Gamma payload.
        let strike_price = match market
            .event
            .markets
            .first()
            .and_then(|m| m.group_line.or(m.line))
        {
            Some(p) => {
                eprintln!("[monitor] strike price (gamma): ${p:.2}");
                Some(p)
            }
            // Fallback: fetch from Chainlink REST / Binance, but only if the
            // window has already opened — a future window has no data yet.
            None if market.start_ts <= now_secs() => {
                let p = chainlink.price_at(coin, market.start_ts as u64).await;
                match p {
                    Some(v) => eprintln!("[monitor] strike price (api fallback): ${v:.2}"),
                    None => eprintln!("[monitor] strike price: unavailable"),
                }
                p
            }
            None => {
                eprintln!("[monitor] strike price: window not yet open");
                None
            }
        };

        // ── 3. Resolve Up / Down token IDs from the CLOB ─────────────────────
        let clob = polymarket.get_market(&market.condition_id).await?;

        let up_id = clob
            .tokens
            .iter()
            .find(|t| t.outcome.eq_ignore_ascii_case("up"))
            .map(|t| t.token_id.clone())
            .expect("no Up token found");

        let down_id = clob
            .tokens
            .iter()
            .find(|t| t.outcome.eq_ignore_ascii_case("down"))
            .map(|t| t.token_id.clone())
            .expect("no Down token found");

        eprintln!("[monitor] up={up_id}  down={down_id}");

        // ── 4. Stream until market expires ────────────────────────────────────
        let up = up_id.clone();
        let down = down_id.clone();
        let end = market.end_ts;

        stream_clob_prices_until(
            &[up_id.as_str(), down_id.as_str()],
            end,
            |tick| {
                on_tick(UpdownTick {
                    up_id: up.clone(),
                    down_id: down.clone(),
                    end_ts: end,
                    strike_price,
                    inner: tick,
                });
            },
        )
        .await?;

        // ── 5. Window closed — hunt for the next one ──────────────────────────
        eprintln!("[monitor] market expired, searching for next window...");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
