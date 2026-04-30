use crate::chainlink_client::ChainlinkTick;
use crate::services::market_monitor::UpdownTick;

/// Stateful strategy that prints every price update to stdout.
///
/// Tracks the latest Chainlink oracle price, Up / Down best asks, and the
/// strike price for the current market window.  On each CLOB tick it prints
/// a one-liner with all relevant numbers; highlights in red when the combined
/// ask sum drops below 1.00 (potential edge).
///
/// The `pct` column on each line is the signed % distance of the current
/// coin price from the window's strike: `(coin_price - strike) / strike * 100`.
/// Positive → price is above the strike (Up is currently winning), negative →
/// below (Down is currently winning), `0.0000%` → exactly on the line. Prints
/// `n/a` until both the strike and a Chainlink tick are available. The strike
/// is the fixed anchor for the window, so the value is symmetric and directly
/// comparable across ticks within the same window.
pub struct PrintStrategy {
    /// Latest Chainlink oracle price (price to beat).
    chainlink_price: f64,
    /// Best ask of the Up token for the current window.
    up_ask: f64,
    /// Best ask of the Down token for the current window.
    down_ask: f64,
    /// Up token ID of the currently monitored window; used to detect rotation.
    last_up_id: String,
}

impl PrintStrategy {
    pub fn new() -> Self {
        Self {
            chainlink_price: f64::NAN,
            up_ask: f64::NAN,
            down_ask: f64::NAN,
            last_up_id: String::new(),
        }
    }

    fn fmt_price(p: f64) -> String {
        if p.is_finite() { format!("${p:.2}") } else { "n/a".to_string() }
    }

    /// Call this for every Chainlink oracle tick.
    pub fn on_chainlink(&mut self, tick: &ChainlinkTick) {
        self.chainlink_price = tick.value;
        println!(
            "[{}] CHAINLINK  {}  ${:.2}",
            chrono::Utc::now().format("%H:%M:%S"),
            tick.symbol.to_uppercase(),
            tick.value,
        );
    }

    /// Call this for every CLOB price tick.
    pub fn on_clob(&mut self, tick: &UpdownTick) {
        // ── Detect market window rotation ─────────────────────────────────────
        if tick.market.up_id != self.last_up_id {
            self.last_up_id = tick.market.up_id.clone();
            self.up_ask = f64::NAN;
            self.down_ask = f64::NAN;

            println!(
                "[{}] ── NEW MARKET  chainlink {} ──",
                chrono::Utc::now().format("%H:%M:%S"),
                Self::fmt_price(self.chainlink_price),
            );
        }

        // ── Update Up / Down asks ─────────────────────────────────────────────
        if tick.inner.asset_id == tick.market.up_id {
            self.up_ask = tick.inner.best_ask;
        } else {
            self.down_ask = tick.inner.best_ask;
        }

        // ── Print ─────────────────────────────────────────────────────────────
        let now = chrono::Utc::now().timestamp() as f64;
        let secs_left = (tick.market.end_ts - now).max(0.0) as u64;
        let sum = self.up_ask + self.down_ask;
        let chainlink_str = Self::fmt_price(self.chainlink_price);

        // Signed % distance from strike: > 0 → Up winning, < 0 → Down winning.
        // Denominator is the strike (fixed for the window), so the value is a
        // stable reference across ticks and symmetric around the line.
        let pct_str = match tick.market.strike_price() {
            Some(strike) if strike.is_finite() && strike != 0.0 && self.chainlink_price.is_finite() => {
                format!("{:+.4}%", (self.chainlink_price - strike) / strike * 100.0)
            }
            _ => "n/a".to_string(),
        };

        if sum <= 0.99 {
            println!(
                "\x1b[31;1m[{}] BINGO  sum {:.4}  UP {:.4}  DOWN {:.4}  chainlink {}  pct {}  {}s left\x1b[0m",
                chrono::Utc::now().format("%H:%M:%S"),
                sum,
                self.up_ask,
                self.down_ask,
                chainlink_str,
                pct_str,
                secs_left,
            );
        } else {
            println!(
                "[{}] UP {:.4}  DOWN {:.4}  chainlink {}  pct {}  {}s left",
                chrono::Utc::now().format("%H:%M:%S"),
                self.up_ask,
                self.down_ask,
                chainlink_str,
                pct_str,
                secs_left,
            );
        }
    }
}
