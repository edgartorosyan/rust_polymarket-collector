//! Perpetual updown market monitor.
//!
//! [`run_updown_monitor`] loops forever: it discovers the active market window,
//! resolves the strike price, streams CLOB price ticks until the window
//! expires, then immediately searches for the next window and starts again.
//!
//! An optional [`OrderbookHandler`] can be supplied to also subscribe to the
//! full orderbook for the Up and Down tokens; when present, the price-tick
//! and orderbook streams run concurrently for the lifetime of each window.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::chainlink_client::ChainlinkClient;
use crate::polymarket_client::PolymarketClient;
use crate::services::polymarket_updown_service::find_active_updown_market;
use crate::services::ws_orderbook::{stream_orderbook_until, OrderbookEvent};
use crate::services::ws_updown_beat::{stream_clob_prices_until, ClobPriceTick};

/// An orderbook event enriched with the current market context.
///
/// Mirrors [`UpdownTick`] — callbacks receive the raw [`OrderbookEvent`] plus
/// an `Arc<MarketMeta>` so strategies can key by slug / condition ID, detect
/// window rotation via `Arc::ptr_eq`, and read the strike price.
pub struct UpdownOrderbookEvent {
    pub market: Arc<MarketMeta>,
    pub inner: OrderbookEvent,
}

/// Optional sink for orderbook events on the currently-monitored market.
///
/// When passed into [`run_updown_monitor`] the monitor opens a second CLOB WS
/// subscription (full book) alongside the price-tick stream and dispatches
/// each event to `on_up` or `on_down` based on which outcome token it belongs
/// to. Callbacks are boxed so `run_updown_monitor` stays non-generic on them;
/// the `'a` lifetime lets closures capture non-`'static` references (e.g. a
/// `&RefCell<StoreStrategy>` owned by `main`).
pub struct OrderbookHandler<'a> {
    pub on_up: Box<dyn FnMut(UpdownOrderbookEvent) + 'a>,
    pub on_down: Box<dyn FnMut(UpdownOrderbookEvent) + 'a>,
}

/// Bold green + reset for stderr (ANSI; ignored by non-TTY consumers).
const SCRAPE_STRIKE_EMPH: &str = "\x1b[1;32m";
const ANSI_RESET: &str = "\x1b[0m";

/// All market-window-level context associated with a tick.
///
/// Immutable for the lifetime of one window — a fresh `Arc<MarketMeta>` is
/// constructed at each rotation and cloned into every tick, so strategies can
/// detect rotation with `Arc::ptr_eq` (cheap) instead of comparing strings.
///
/// `strike_price` is the one mutable piece: Polymarket often doesn't stamp
/// `openPrice` until a few seconds into the window, so the CLOB stream starts
/// with `strike_price() == None` and a background task upgrades it once the
/// scrape succeeds. f64 bits are packed into an `AtomicU64` with `NaN` as the
/// "not yet known" sentinel — avoids a mutex on the hot tick path.
pub struct MarketMeta {
    pub slug: String,
    pub title: String,
    pub condition_id: String,
    /// Token ID of the "Up" outcome.
    pub up_id: String,
    /// Token ID of the "Down" outcome.
    pub down_id: String,
    /// Unix timestamp (seconds) when the window opened.
    pub start_ts: f64,
    /// Unix timestamp (seconds) when the window expires.
    pub end_ts: f64,
    strike_price_bits: AtomicU64,
}

impl MarketMeta {
    pub fn new(
        slug: String,
        title: String,
        condition_id: String,
        up_id: String,
        down_id: String,
        start_ts: f64,
        end_ts: f64,
        strike_price: Option<f64>,
    ) -> Self {
        Self {
            slug,
            title,
            condition_id,
            up_id,
            down_id,
            start_ts,
            end_ts,
            strike_price_bits: AtomicU64::new(encode_strike(strike_price)),
        }
    }

    /// Current strike price (`None` until the window's `openPrice` is scraped).
    pub fn strike_price(&self) -> Option<f64> {
        decode_strike(self.strike_price_bits.load(Ordering::Relaxed))
    }

    /// Atomically publish a new strike price. Safe to call from any task.
    pub fn set_strike_price(&self, v: Option<f64>) {
        self.strike_price_bits
            .store(encode_strike(v), Ordering::Relaxed);
    }
}

fn encode_strike(v: Option<f64>) -> u64 {
    match v {
        Some(p) => p.to_bits(),
        None => f64::NAN.to_bits(),
    }
}

fn decode_strike(bits: u64) -> Option<f64> {
    let v = f64::from_bits(bits);
    if v.is_nan() { None } else { Some(v) }
}

/// A CLOB price tick enriched with the current market context.
pub struct UpdownTick {
    pub market: Arc<MarketMeta>,
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
/// 4. Calls `on_tick` for every price update until the window expires. If an
///    [`OrderbookHandler`] is supplied, a second WS subscription streams the
///    full orderbook concurrently and routes each event to `on_up` / `on_down`.
/// 5. Waits 5 s then repeats from step 1.
///
/// Pass `orderbook_handler = None` to get price ticks only (original behaviour).
///
/// Returns `Err` only if a non-retryable API failure occurs.
pub async fn run_updown_monitor<F>(
    polymarket: &PolymarketClient,
    _chainlink: &ChainlinkClient,
    coin: &str,
    window_minutes: u64,
    mut on_tick: F,
    mut orderbook_handler: Option<OrderbookHandler<'_>>,
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
        // Strategy:
        // - Try the HTML scrape a few times quickly (fast path for windows that
        //   have been open long enough for Polymarket to stamp `openPrice`).
        // - If that fails, accept whatever Gamma has (may be `None`) and kick
        //   off a BACKGROUND task that keeps retrying the scrape; when it
        //   succeeds it updates `MarketMeta.strike_price` atomically, so
        //   in-flight ticks upgrade to the correct value without losing any.
        // - Binance kline fallback removed — it's approximate (±$30) and was
        //   silently producing wrong "strike" values at rotation.
        let initial_strike = scrape_with_retry(polymarket, &market.slug, 3, Duration::from_secs(2)).await;
        let initial_strike = match initial_strike {
            Some(p) => {
                eprintln!("{SCRAPE_STRIKE_EMPH}[monitor] strike price (scraped): ${p:.4}{ANSI_RESET}");
                Some(p)
            }
            None => {
                let gamma = market
                    .event
                    .markets
                    .first()
                    .and_then(|m| m.group_line.or(m.line));
                if let Some(p) = gamma {
                    eprintln!("[monitor] strike price (gamma, tentative): ${p:.4}");
                }
                gamma
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
        let meta = Arc::new(MarketMeta::new(
            market.slug.clone(),
            market.title.clone(),
            market.condition_id.clone(),
            up_id.clone(),
            down_id.clone(),
            market.start_ts,
            market.end_ts,
            initial_strike,
        ));

        // If we didn't get an authoritative strike upfront, keep trying in the
        // background — the openPrice usually appears within 30–60s. Once the
        // scrape succeeds, meta.set_strike_price publishes it atomically and
        // every subsequent tick carries the corrected value.
        if meta.strike_price().is_none() || initial_strike.is_none() {
            spawn_background_strike_refresh(polymarket.clone(), meta.clone(), market.end_ts);
        }

        match orderbook_handler.as_mut() {
            None => {
                stream_clob_prices_until(
                    &[up_id.as_str(), down_id.as_str()],
                    market.end_ts,
                    |tick| {
                        on_tick(UpdownTick {
                            market: meta.clone(),
                            inner: tick,
                        });
                    },
                )
                .await?;
            }
            Some(h) => {
                let assets = [up_id.as_str(), down_id.as_str()];
                let on_up = &mut h.on_up;
                let on_down = &mut h.on_down;
                tokio::try_join!(
                    stream_clob_prices_until(
                        &assets,
                        market.end_ts,
                        |tick| {
                            on_tick(UpdownTick {
                                market: meta.clone(),
                                inner: tick,
                            });
                        },
                    ),
                    stream_orderbook_until(
                        up_id.as_str(),
                        down_id.as_str(),
                        market.end_ts,
                        |ev| {
                            on_up(UpdownOrderbookEvent {
                                market: meta.clone(),
                                inner: ev,
                            });
                        },
                        |ev| {
                            on_down(UpdownOrderbookEvent {
                                market: meta.clone(),
                                inner: ev,
                            });
                        },
                    ),
                )?;
            }
        }

        // ── 5. Window closed — hunt for the next one ──────────────────────────
        eprintln!("[monitor] market expired, searching for next window...");
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

/// Spawn a task that re-scrapes the event page every 3 s until it returns a
/// real `openPrice`, then publishes it via `meta.set_strike_price`. Exits on
/// success, on `end_ts`, or after ~5 minutes — whichever comes first.
fn spawn_background_strike_refresh(
    polymarket: PolymarketClient,
    meta: Arc<MarketMeta>,
    end_ts_secs: f64,
) {
    tokio::spawn(async move {
        let stop_at = tokio::time::Instant::now()
            + Duration::from_secs_f64((end_ts_secs - now_secs()).max(0.0));
        let hard_cap = tokio::time::Instant::now() + Duration::from_secs(300);
        let deadline = stop_at.min(hard_cap);

        loop {
            if tokio::time::Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;

            match polymarket.scrape_strike_price(&meta.slug).await {
                Ok(Some(p)) => {
                    let prev = meta.strike_price();
                    if prev != Some(p) {
                        eprintln!(
                            "{SCRAPE_STRIKE_EMPH}[monitor] strike price (bg update {:?} → ${p:.4}){ANSI_RESET}",
                            prev
                        );
                        meta.set_strike_price(Some(p));
                    }
                    return;
                }
                Ok(None) => continue,
                Err(e) => {
                    eprintln!("[monitor] bg scrape error: {e}");
                }
            }
        }
    });
}

/// Scrape the event page for a strike price, retrying while `openPrice` is
/// still `null` (common in the first few seconds after a rotation). Returns
/// `None` after `attempts` tries so the caller can fall through to Gamma /
/// Chainlink. Delays happen only between attempts, never after the last one.
async fn scrape_with_retry(
    client: &PolymarketClient,
    slug: &str,
    attempts: u32,
    delay: Duration,
) -> Option<f64> {
    for i in 0..attempts {
        match client.scrape_strike_price(slug).await {
            Ok(Some(p)) => return Some(p),
            Ok(None) => {
                if i + 1 < attempts {
                    eprintln!(
                        "[monitor] scrape: openPrice not populated yet, retry {}/{} in {:?}",
                        i + 1,
                        attempts,
                        delay
                    );
                    tokio::time::sleep(delay).await;
                }
            }
            Err(e) => {
                eprintln!("[monitor] scrape error: {e}");
                if i + 1 < attempts {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }
    None
}
