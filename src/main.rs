mod chainlink_client;
mod polymarket_client;
mod services;
mod strategies;

use std::cell::RefCell;

use chainlink_client::ChainlinkClient;
use polymarket_client::PolymarketClient;
use services::market_monitor::{UpdownTick, run_updown_monitor};
use strategies::print::PrintStrategy;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let chainlink = ChainlinkClient::new();
    let polymarket = PolymarketClient::new();

    // RefCell gives each closure a shared handle to the strategy without
    // requiring Send — try_join! polls both on the same task, so borrow_mut()
    // never panics at runtime.
    let strategy = RefCell::new(PrintStrategy::new());

    tokio::try_join!(
        chainlink.stream_prices(&["btc/usd"], |tick| {
            strategy.borrow_mut().on_chainlink(&tick);
        }),
        run_updown_monitor(&polymarket, &chainlink, "btc", 15, |tick: UpdownTick| {
            strategy.borrow_mut().on_clob(&tick);
        }),
    )?;

    Ok(())
}
