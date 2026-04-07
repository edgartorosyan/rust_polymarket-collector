#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use super::serde_utils::deserialize_opt_f64_or_str;

// ─── CLOB Market ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClobMarket {
    pub condition_id: String,
    pub question_id: String,
    pub tokens: Vec<Token>,
    pub rewards: Rewards,
    pub minimum_order_size: f64,
    pub minimum_tick_size: f64,
    #[serde(default)] pub description: String,
    pub category: Option<String>,
    pub end_date_iso: Option<String>,
    pub game_start_time: Option<String>,
    #[serde(default)] pub question: String,
    #[serde(default)] pub market_slug: String,
    pub min_incentive_size: Option<String>,
    pub max_incentive_spread: Option<String>,
    #[serde(default)] pub active: bool,
    #[serde(default)] pub closed: bool,
    #[serde(default)] pub seconds_delay: u64,
    #[serde(default)] pub icon: String,
    #[serde(default)] pub fpmm: String,
    #[serde(default)] pub accepting_orders: bool,
    pub accepting_order_timestamp: Option<String>,
    #[serde(default)] pub cyom: bool,
    #[serde(default)] pub enable_order_book: bool,
    #[serde(default)] pub neg_risk: bool,
    pub neg_risk_market_id: Option<String>,
    pub neg_risk_request_id: Option<String>,
    #[serde(default)] pub is_50_50_outcome: bool,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub token_id: String,
    pub outcome: String,
    pub price: f64,
    pub winner: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rewards {
    pub rates: Option<Vec<RewardRate>>,
    pub min_size: f64,
    pub max_spread: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardRate {
    pub asset_address: String,
    pub rewards_daily_rate: f64,
}

// ─── Markets List Response ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketsResponse {
    pub limit: u64,
    pub count: u64,
    pub next_cursor: String,
    pub data: Vec<ClobMarket>,
}

// ─── Order Book ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub market: String,
    pub asset_id: String,
    pub hash: String,
    pub timestamp: String,
    pub bids: Vec<OrderLevel>,
    pub asks: Vec<OrderLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLevel {
    pub price: String,
    pub size: String,
}

// ─── Price ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceResponse {
    pub price: String,
}

// ─── Spread ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadResponse {
    pub spread: String,
}

// ─── Midpoint ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidpointResponse {
    pub mid: String,
}

// ─── Last Trade Price ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastTradePriceResponse {
    pub price: String,
}

// ─── Tick Size ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickSizeResponse {
    pub minimum_tick_size: String,
}

// ─── Trades ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub taker_order_id: String,
    pub market: String,
    pub asset_id: String,
    pub side: String,
    pub size: String,
    pub fee_rate_bps: String,
    pub price: String,
    pub status: String,
    pub match_time: String,
    pub last_update: String,
    pub outcome: String,
    pub bucket_index: u64,
    pub owner: String,
    pub maker_orders: Vec<MakerOrder>,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakerOrder {
    pub order_id: String,
    pub maker_address: String,
    pub matched_amount: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradesResponse {
    pub limit: u64,
    pub count: u64,
    pub next_cursor: String,
    pub data: Vec<Trade>,
}

// ─── Gamma Market ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaMarket {
    pub id: String,
    pub question: String,
    pub condition_id: String,
    pub slug: String,
    pub resolution_source: Option<String>,
    pub end_date: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_f64_or_str")]
    pub liquidity: Option<f64>,
    pub start_date: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub outcomes: Option<String>,
    pub outcome_prices: Option<String>,
    #[serde(default, deserialize_with = "deserialize_opt_f64_or_str")]
    pub volume: Option<f64>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub new: Option<bool>,
    pub featured: Option<bool>,
    pub restricted: Option<bool>,
    pub group_item_title: Option<String>,
    pub group_item_threshold: Option<String>,
    pub question_id: Option<String>,
    pub enable_order_book: Option<bool>,
    pub order_price_min_tick_size: Option<f64>,
    pub volume_num: Option<f64>,
    pub liquidity_num: Option<f64>,
    pub end_date_iso: Option<String>,
    pub start_date_iso: Option<String>,
    pub has_reviewed_dates: Option<bool>,
    #[serde(rename = "volume24hr")]
    pub volume_24hr: Option<f64>,
    pub seconds_delay: Option<u64>,
    pub category: Option<String>,
    pub uma_bond: Option<String>,
    pub uma_reward: Option<String>,
    #[serde(rename = "volume24hrClob")]
    pub volume_24hr_clob: Option<f64>,
    pub volume_clob: Option<f64>,
    pub liquidity_clob: Option<f64>,
    pub accepting_orders: Option<bool>,
    pub neg_risk: Option<bool>,
    pub neg_risk_market_id: Option<String>,
    pub ready: Option<bool>,
    pub funded: Option<bool>,
    pub cyom: Option<bool>,
    pub competitive: Option<f64>,
    pub approved: Option<bool>,
    pub rewards_min_size: Option<f64>,
    pub rewards_max_spread: Option<f64>,
    pub spread: Option<f64>,
    pub last_trade_price: Option<f64>,
    pub best_bid: Option<f64>,
    pub best_ask: Option<f64>,
    pub automatically_active: Option<bool>,
    pub clear_book_on_start: Option<bool>,
    pub show_all_outcomes: Option<bool>,
    /// Primary strike price for recurring Up/Down markets.
    #[serde(default, deserialize_with = "deserialize_opt_f64_or_str")]
    pub group_line: Option<f64>,
    /// Secondary strike price (older/manual markets).
    #[serde(default, deserialize_with = "deserialize_opt_f64_or_str")]
    pub line: Option<f64>,
}

// ─── Gamma Event ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GammaEvent {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    pub description: Option<String>,
    pub start_date: Option<String>,
    pub creation_date: Option<String>,
    pub end_date: Option<String>,
    pub icon: Option<String>,
    pub image: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub new: Option<bool>,
    pub featured: Option<bool>,
    pub restricted: Option<bool>,
    pub liquidity: Option<f64>,
    pub volume: Option<f64>,
    pub competitive: Option<f64>,
    #[serde(rename = "volume24hr")]
    pub volume_24hr: Option<f64>,
    pub enable_order_book: Option<bool>,
    pub comment_count: Option<u64>,
    pub tags: Option<Vec<EventTag>>,
    pub markets: Vec<GammaMarket>,
    pub cyom: Option<bool>,
    pub series: Option<Vec<serde_json::Value>>,
    /// Populated by Gamma only after the market window resolves.
    pub event_metadata: Option<EventMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventMetadata {
    /// Chainlink oracle price of the underlying coin at window open (e.g. 95.89 for SOL/USD).
    /// Only present after the market resolves.
    pub price_to_beat: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventTag {
    pub id: String,
    pub label: String,
    pub slug: String,
}

// ─── Query Params ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct MarketsParams {
    pub next_cursor: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct TradesParams {
    pub market: Option<String>,
    pub asset_id: Option<String>,
    pub limit: Option<u64>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct GammaMarketsParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub order: Option<String>,
    pub ascending: Option<bool>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub liquidity_num_min: Option<f64>,
    pub liquidity_num_max: Option<f64>,
    pub volume_num_min: Option<f64>,
    pub volume_num_max: Option<f64>,
    pub start_date_min: Option<String>,
    pub start_date_max: Option<String>,
    pub end_date_min: Option<String>,
    pub end_date_max: Option<String>,
    pub tag: Option<String>,
    pub related_tags: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct GammaEventsParams {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub order: Option<String>,
    pub ascending: Option<bool>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub archived: Option<bool>,
    pub tag: Option<String>,
    pub slug: Option<String>,
}

// ─── Updown Market ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct UpdownMarket {
    pub slug: String,
    pub title: String,
    pub condition_id: String,
    pub end_date: String,
    /// Unix timestamp of the window open (strike price is set at this moment).
    pub start_ts: f64,
    /// Unix timestamp of the window end.
    pub end_ts: f64,
    pub closed: bool,
    /// Full event payload from the Gamma API.
    pub event: GammaEvent,
}
