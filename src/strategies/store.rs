//! Persists ticks to Postgres.
//!
//! Mirrors [`PrintStrategy`]: maintains the same running state (latest
//! chainlink price, current up/down asks, market rotation) but emits rows
//! into `updown_market` + `market_ticks` instead of stdout.
//!
//! # Why a channel?
//! Strategy callbacks are synchronous; `sqlx` inserts are async. On each tick
//! the strategy pushes a [`StoreEvent`] onto an unbounded `mpsc` and a
//! background task drains it. Serialising through one task means:
//! - ticks are inserted in the order they arrive (no interleaving);
//! - the market row is always inserted before any of its ticks (so
//!   foreign-key resolution can't lose the race).
//!
//! [`PrintStrategy`]: super::print::PrintStrategy

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use tokio::sync::mpsc::{self, UnboundedSender};
use uuid::Uuid;

use crate::chainlink_client::ChainlinkTick;
use crate::db;
use crate::services::market_monitor::{MarketMeta, UpdownOrderbookEvent, UpdownTick};
use crate::services::ws_orderbook::{Level, OrderbookEvent};

// ─── Channel payloads ────────────────────────────────────────────────────────

enum StoreEvent {
    /// Fired once per market-window rotation.
    MarketStart(Arc<MarketMeta>),
    /// Fired when the market's strike price is (re)resolved after the initial
    /// insert — e.g. the background scraper finally sees a real `openPrice`.
    StrikeUpdate { slug: String, strike_price: f64 },
    /// Fired for every CLOB tick.
    Tick {
        slug: String,
        coin_price: Option<f64>,
        up: Option<f64>,
        down: Option<f64>,
        pct: Option<f64>,
        milliseconds_left: i64,
        ts: DateTime<Utc>,
    },
    /// Fired for every full orderbook snapshot (incremental `price_change`
    /// updates are discarded at this layer — the snapshots table only models
    /// full books).
    OrderbookSnapshot {
        slug: String,
        /// "UP" or "DOWN" — matches the CHECK constraint on `orderbook_snapshots.outcome`.
        outcome: &'static str,
        bids: serde_json::Value,
        asks: serde_json::Value,
        ts: DateTime<Utc>,
    },
}

// ─── Public strategy ─────────────────────────────────────────────────────────

pub struct StoreStrategy {
    tx: UnboundedSender<StoreEvent>,
    chainlink_price: f64,
    up_ask: f64,
    down_ask: f64,
    current_market: Option<Arc<MarketMeta>>,
    /// Last strike we told the DB about — detect when the background
    /// refresh upgrades it so we can push a `StrikeUpdate` event.
    last_published_strike: Option<f64>,
}

impl StoreStrategy {
    /// Resolves `market_type_id` from `(coin, window_size)` and spawns the
    /// background insert task. Call after `db::init`.
    pub async fn new(coin: &str, window_size: &str) -> anyhow::Result<Self> {
        let market_type_id: i16 = sqlx::query_scalar(
            "SELECT id FROM updown_market_type WHERE coin = $1 AND window_size = $2",
        )
        .bind(coin)
        .bind(window_size)
        .fetch_one(db::pool())
        .await?;

        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(drain_loop(rx, market_type_id));

        Ok(Self {
            tx,
            chainlink_price: f64::NAN,
            up_ask: f64::NAN,
            down_ask: f64::NAN,
            current_market: None,
            last_published_strike: None,
        })
    }

    /// Call for every Chainlink oracle tick — updates the cached reference price.
    pub fn on_chainlink(&mut self, tick: &ChainlinkTick) {
        self.chainlink_price = tick.value;
    }

    /// Call for every CLOB price tick — serialises a row into the channel.
    pub fn on_clob(&mut self, tick: &UpdownTick) {
        self.ensure_current_market(&tick.market);

        // The background scrape may upgrade the strike after MarketStart —
        // push a StrikeUpdate so the DB row reflects the authoritative value.
        let current_strike = tick.market.strike_price();
        if current_strike != self.last_published_strike {
            if let Some(p) = current_strike {
                let _ = self.tx.send(StoreEvent::StrikeUpdate {
                    slug: tick.market.slug.clone(),
                    strike_price: p,
                });
            }
            self.last_published_strike = current_strike;
        }

        if tick.inner.asset_id == tick.market.up_id {
            self.up_ask = tick.inner.best_ask;
        } else if tick.inner.asset_id == tick.market.down_id {
            self.down_ask = tick.inner.best_ask;
        }

        let pct = match current_strike {
            Some(strike) if self.chainlink_price.is_finite() && self.chainlink_price != 0.0 => {
                Some((self.chainlink_price - strike) / self.chainlink_price * 100.0)
            }
            _ => None,
        };

        let now_ms = Utc::now().timestamp_millis();
        let end_ms = (tick.market.end_ts * 1000.0) as i64;

        let _ = self.tx.send(StoreEvent::Tick {
            slug: tick.market.slug.clone(),
            coin_price: finite(self.chainlink_price),
            up: finite(self.up_ask),
            down: finite(self.down_ask),
            pct,
            milliseconds_left: (end_ms - now_ms).max(0),
            ts: Utc::now(),
        });
    }

    /// Call for every orderbook event on the Up outcome token. Only full
    /// snapshots are persisted; incremental updates are dropped.
    pub fn on_orderbook_up(&mut self, ev: &UpdownOrderbookEvent) {
        self.on_orderbook(ev, "UP");
    }

    /// Call for every orderbook event on the Down outcome token. Only full
    /// snapshots are persisted; incremental updates are dropped.
    pub fn on_orderbook_down(&mut self, ev: &UpdownOrderbookEvent) {
        self.on_orderbook(ev, "DOWN");
    }

    fn on_orderbook(&mut self, ev: &UpdownOrderbookEvent, outcome: &'static str) {
        self.ensure_current_market(&ev.market);

        let OrderbookEvent::Snapshot(snap) = &ev.inner else { return };

        let _ = self.tx.send(StoreEvent::OrderbookSnapshot {
            slug: ev.market.slug.clone(),
            outcome,
            bids: levels_to_json(&snap.bids),
            asks: levels_to_json(&snap.asks),
            ts: ms_to_utc(snap.timestamp_ms),
        });
    }

    /// Detects market-window rotation by `Arc::ptr_eq` and emits `MarketStart`
    /// exactly once per new window. Called from both `on_clob` and
    /// `on_orderbook` so whichever stream wins the race registers the market.
    fn ensure_current_market(&mut self, market: &Arc<MarketMeta>) {
        let is_new = match &self.current_market {
            None => true,
            Some(prev) => !Arc::ptr_eq(prev, market),
        };
        if !is_new {
            return;
        }
        self.up_ask = f64::NAN;
        self.down_ask = f64::NAN;
        self.current_market = Some(market.clone());
        self.last_published_strike = market.strike_price();
        let _ = self.tx.send(StoreEvent::MarketStart(market.clone()));
    }
}

// ─── Background drain ────────────────────────────────────────────────────────

async fn drain_loop(mut rx: mpsc::UnboundedReceiver<StoreEvent>, market_type_id: i16) {
    let pool = db::pool();
    // slug → id cache, refreshed as markets rotate.
    let mut ids: HashMap<String, Uuid> = HashMap::new();

    while let Some(ev) = rx.recv().await {
        match ev {
            StoreEvent::MarketStart(meta) => match upsert_market(pool, &meta, market_type_id).await {
                Ok(id) => {
                    ids.insert(meta.slug.clone(), id);
                    eprintln!("[store] market {slug} id={id}", slug = meta.slug);
                }
                Err(e) => eprintln!("[store] market upsert failed ({}): {e}", meta.slug),
            },
            StoreEvent::StrikeUpdate { slug, strike_price } => {
                let res = sqlx::query(
                    "UPDATE updown_market SET strike_price = $1 WHERE slug = $2",
                )
                .bind(strike_price)
                .bind(&slug)
                .execute(pool)
                .await;
                match res {
                    Ok(r) if r.rows_affected() > 0 => {
                        eprintln!("[store] strike updated for {slug}: ${strike_price:.4}");
                    }
                    Ok(_) => eprintln!("[store] strike update: no row matched slug {slug}"),
                    Err(e) => eprintln!("[store] strike update failed ({slug}): {e}"),
                }
            }
            StoreEvent::Tick {
                slug,
                coin_price,
                up,
                down,
                pct,
                milliseconds_left,
                ts,
            } => {
                let Some(&market_id) = ids.get(&slug) else {
                    eprintln!("[store] tick dropped: market '{slug}' not yet inserted");
                    continue;
                };

                let res = sqlx::query(
                    "INSERT INTO market_ticks
                        (market_id, coin_price, up, down, pct, milliseconds_left, ts)
                     VALUES ($1, $2, $3, $4, $5, $6, $7)",
                )
                .bind(market_id)
                .bind(coin_price)
                .bind(up)
                .bind(down)
                .bind(pct)
                .bind(milliseconds_left)
                .bind(ts)
                .execute(pool)
                .await;

                if let Err(e) = res {
                    eprintln!("[store] tick insert failed: {e}");
                }
            }
            StoreEvent::OrderbookSnapshot {
                slug,
                outcome,
                bids,
                asks,
                ts,
            } => {
                let Some(&market_id) = ids.get(&slug) else {
                    eprintln!(
                        "[store] orderbook snapshot dropped: market '{slug}' not yet inserted"
                    );
                    continue;
                };

                let res = sqlx::query(
                    "INSERT INTO orderbook_snapshots
                        (market_id, outcome, bids, asks, ts)
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(market_id)
                .bind(outcome)
                .bind(&bids)
                .bind(&asks)
                .bind(ts)
                .execute(pool)
                .await;

                if let Err(e) = res {
                    eprintln!("[store] orderbook snapshot insert failed: {e}");
                }
            }
        }
    }
}

async fn upsert_market(
    pool: &sqlx::PgPool,
    meta: &MarketMeta,
    market_type_id: i16,
) -> sqlx::Result<Uuid> {
    let start = Utc.timestamp_opt(meta.start_ts as i64, 0).single().unwrap_or_else(Utc::now);
    let end = Utc.timestamp_opt(meta.end_ts as i64, 0).single().unwrap_or_else(Utc::now);

    sqlx::query_scalar(
        "INSERT INTO updown_market
            (slug, title, condition_id, start_ts, end_ts, strike_price, market_type_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (slug) DO UPDATE
            SET strike_price = COALESCE(EXCLUDED.strike_price, updown_market.strike_price)
         RETURNING id",
    )
    .bind(&meta.slug)
    .bind(&meta.title)
    .bind(&meta.condition_id)
    .bind(start)
    .bind(end)
    .bind(meta.strike_price())
    .bind(market_type_id)
    .fetch_one(pool)
    .await
}

fn finite(v: f64) -> Option<f64> {
    v.is_finite().then_some(v)
}

fn levels_to_json(levels: &[Level]) -> serde_json::Value {
    serde_json::Value::Array(
        levels
            .iter()
            .map(|l| serde_json::json!({ "price": l.price, "size": l.size }))
            .collect(),
    )
}

fn ms_to_utc(ms: u64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms as i64)
        .single()
        .unwrap_or_else(Utc::now)
}
