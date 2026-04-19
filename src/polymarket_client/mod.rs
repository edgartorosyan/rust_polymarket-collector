//! Polymarket REST client.
//!
//! Wraps three surfaces behind one [`PolymarketClient`]:
//! - **CLOB** REST (`https://clob.polymarket.com`) — markets, order book, price,
//!   spread, midpoint, last-trade-price, tick-size, trades.
//! - **Gamma** REST (`https://gamma-api.polymarket.com`) — market / event
//!   metadata, used for discovery via slug or filter.
//! - **Event page scrape** (`https://polymarket.com/event/{slug}`) — extracts
//!   the strike price from the embedded `__NEXT_DATA__` JSON, which is more
//!   reliable than Gamma's `group_line`/`line` fields for freshly-opened
//!   windows (see `scrape_strike_price`).
//!
//! Many endpoint methods on `PolymarketClient` are unused by `main.rs` today
//! but are kept for future strategies — hence the crate-level `dead_code`
//! allowance in `client.rs` / `models.rs`.

pub mod client;
pub mod error;
pub mod models;
pub(crate) mod serde_utils;

pub use client::PolymarketClient;
#[allow(unused_imports)]
pub use error::PolymarketError;
