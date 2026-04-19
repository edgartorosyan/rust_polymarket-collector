//! Typed environment-variable loader.
//!
//! Call [`Config::from_env`] once at startup after `dotenvy::dotenv()`.
//! envy matches field names against env vars case-insensitively (snake_case
//! field ↔ SCREAMING_SNAKE env var), so `database_url` reads `DATABASE_URL`.
//! Unknown env vars are ignored; missing required fields produce an error.

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Postgres connection string. Required.
    pub database_url: String,

    /// Chainlink Data Streams API key. When both key + secret are set,
    /// `ChainlinkClient::price_at` uses the official REST endpoint; otherwise
    /// it falls back to Binance 1m klines.
    #[serde(default)]
    pub chainlink_api_key: Option<String>,

    #[serde(default)]
    pub chainlink_api_secret: Option<String>,

    /// Bind address for the Prometheus `/metrics` endpoint. Defaults to
    /// `127.0.0.1:9090`. Set to `0.0.0.0:9090` to expose externally.
    #[serde(default = "default_metrics_bind")]
    pub metrics_bind: String,

    /// When `true`, `run_updown_monitor` opens a second CLOB WS subscription
    /// for the full Up/Down orderbooks alongside the price-tick stream and
    /// persists snapshots to the `orderbook_snapshots` table. Defaults to
    /// `false` to keep the default run cheap. Accepts the usual truthy
    /// strings: `true` / `1` / `yes` / `on` (case-insensitive).
    #[serde(default, deserialize_with = "deserialize_bool_flag")]
    pub with_orderbook: bool,
}

fn default_metrics_bind() -> String {
    "127.0.0.1:9090".to_string()
}

fn deserialize_bool_flag<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    let raw = String::deserialize(deserializer)?;
    Ok(matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    ))
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(envy::from_env::<Self>()?)
    }
}
