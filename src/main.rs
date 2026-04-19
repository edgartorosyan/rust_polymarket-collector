//! Polymarket updown-market collector — binary entrypoint.
//!
//! Wires two concurrent streams into a single strategy on one Tokio task:
//! - [`ChainlinkClient::stream_prices`] — live oracle price ("price to beat").
//! - [`run_updown_monitor`] — perpetual loop over updown market windows,
//!   emitting CLOB best-bid/ask ticks for the Up and Down outcome tokens.
//!   Accepts an optional `OrderbookHandler` to additionally stream the full
//!   Up/Down orderbooks; passed as `None` here — wire a handler when a
//!   strategy needs book depth.
//!
//! Both futures share a `RefCell<PrintStrategy>`. This is safe because
//! `tokio::try_join!` polls both on the current task — no cross-thread access
//! — so `borrow_mut()` calls are serialised by the scheduler. If either
//! future is moved to a separate task, switch to `Arc<Mutex<_>>`.
//!
//! See `CLAUDE.md` at the repo root for architecture and conventions.

mod chainlink_client;
mod config;
mod db;
mod metrics;
mod polymarket_client;
mod services;
mod strategies;

use std::cell::RefCell;

use chainlink_client::ChainlinkClient;
use config::Config;
use polymarket_client::PolymarketClient;
use services::market_monitor::{UpdownTick, run_updown_monitor};
use strategies::print::PrintStrategy;
use strategies::store::StoreStrategy;

use crate::services::market_monitor::OrderbookHandler;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();

    let cfg = Config::from_env()?;

    db::init(&cfg.database_url).await?;
    eprintln!("[db] pool ready");

    metrics::init(&cfg.metrics_bind).await?;

    let chainlink = ChainlinkClient::new();
    let polymarket = PolymarketClient::new();

    // RefCells give each closure shared handles to the strategies without
    // requiring Send — try_join! polls both on the same task, so borrow_mut()
    // never panics at runtime.
    let print_strategy = RefCell::new(PrintStrategy::new());
    let store_strategy = RefCell::new(StoreStrategy::new("BTC", "15m").await?);

    tokio::try_join!(
        chainlink.stream_prices(&["btc/usd"], |tick| {
            print_strategy.borrow_mut().on_chainlink(&tick);
            store_strategy.borrow_mut().on_chainlink(&tick);
        }),
        run_updown_monitor(
            &polymarket,
            &chainlink,
            "btc",
            15,
            |tick: UpdownTick| {
                print_strategy.borrow_mut().on_clob(&tick);
                store_strategy.borrow_mut().on_clob(&tick);
            },
            cfg.with_orderbook.then(|| OrderbookHandler {
                on_up: Box::new(|ev| {
                    store_strategy.borrow_mut().on_orderbook_up(&ev);
                }),
                on_down: Box::new(|ev| {
                    store_strategy.borrow_mut().on_orderbook_down(&ev);
                }),
            }),
        ),
    )?;

    Ok(())
}
