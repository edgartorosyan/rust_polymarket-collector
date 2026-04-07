//! Market-discovery and orderbook helpers for Polymarket updown markets.
//!
//! Updown markets follow the slug pattern:
//!     `{coin}-updown-{window}m-{start_unix_timestamp}`
//!
//! where start timestamps are multiples of `window_secs` (e.g. 900 for 15-min,
//! 300 for 5-min).

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::NaiveDateTime;
use tracing::warn;

use crate::polymarket_client::{
    PolymarketClient, PolymarketError,
    models::{GammaEvent, UpdownMarket},
};

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Convert an ISO-8601 UTC string (e.g. `"2025-01-01T12:00:00Z"`) to a Unix
/// timestamp in seconds.
fn parse_iso(iso: &str) -> f64 {
    let iso = iso.trim_end_matches('Z');
    let iso = iso.split('.').next().unwrap_or(iso);
    NaiveDateTime::parse_from_str(iso, "%Y-%m-%dT%H:%M:%S")
        .map(|ndt| ndt.and_utc().timestamp() as f64)
        .unwrap_or(0.0)
}

fn window_secs(window_minutes: u64) -> u64 {
    window_minutes * 60
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
}

// ---------------------------------------------------------------------------
// Slug helpers
// ---------------------------------------------------------------------------

/// Return the slug for the updown window that contains the current moment.
///
/// Examples:
/// - `btc-updown-15m-1772894700`
/// - `btc-updown-5m-1772894700`
pub fn current_updown_market_slug(coin: &str, window_minutes: u64) -> String {
    let secs = window_secs(window_minutes);
    let start = (now_unix() / secs) * secs;
    format!("{}-updown-{}m-{}", coin.to_lowercase(), window_minutes, start)
}

// ---------------------------------------------------------------------------
// Gamma API helpers
// ---------------------------------------------------------------------------

/// Find the first active (not yet closed) updown market for `coin`.
///
/// Starts from the current window and walks forward `lookahead` slots
/// until it finds an event with `closed == false`.
///
/// Returns `None` if no active window is found within `lookahead` slots.
pub async fn find_active_updown_market(
    client: &PolymarketClient,
    coin: &str,
    window_minutes: u64,
    lookahead: u64,
) -> Result<Option<UpdownMarket>, PolymarketError> {
    let secs = window_secs(window_minutes);
    let base_start = (now_unix() / secs) * secs;

    for i in 0..lookahead {
        let start = base_start + i * secs;
        let slug = format!("{}-updown-{}m-{}", coin.to_lowercase(), window_minutes, start);

        let event = match client.get_gamma_event_by_slug(&slug).await? {
            Some(e) => e,
            None => continue,
        };

        if event.closed.unwrap_or(true) {
            continue;
        }

        let market = event.markets.first();
        let condition_id = market
            .and_then(|m| Some(m.condition_id.clone()))
            .unwrap_or_default();
        let end_date = event.end_date.clone().unwrap_or_default();
        let end_ts = parse_iso(&end_date);

        return Ok(Some(UpdownMarket {
            slug,
            title: event.title.clone(),
            condition_id,
            end_date,
            start_ts: start as f64,
            end_ts,
            closed: false,
            event,
        }));
    }

    warn!(
        coin,
        window_minutes,
        lookahead,
        "no active updown market found in the next {lookahead} windows"
    );
    Ok(None)
}



// ---------------------------------------------------------------------------
// Resolution helper
// ---------------------------------------------------------------------------

/// Extract the winning outcome from a closed event's `outcome_prices`.
///
/// `outcome_prices` on the first market is a JSON array like `["1","0"]` or
/// `["0","1"]`. `outcomes` is `["Up","Down"]`. The entry with price `"1"` won.
pub fn resolve_outcome(event: &GammaEvent) -> Option<String> {
    let market = event.markets.first()?;

    let prices_raw = market.outcome_prices.as_deref()?;
    let outcomes_raw = market.outcomes.as_deref()?;

    let prices: Vec<String> = serde_json::from_str(prices_raw).ok()?;
    let outcomes: Vec<String> = serde_json::from_str(outcomes_raw).ok()?;

    outcomes
        .into_iter()
        .zip(prices)
        .find(|(_, price)| price.parse::<f64>().ok() == Some(1.0))
        .map(|(outcome, _)| outcome)
}
