#![allow(dead_code)]

use reqwest::Client;

use super::{
    error::PolymarketError,
    models::{
        ClobMarket, GammaEvent, GammaEventsParams, GammaMarket, GammaMarketsParams, LastTradePriceResponse,
        MarketsParams, MarketsResponse, MidpointResponse, OrderBook, PriceResponse, SpreadResponse,
        TickSizeResponse, TradesParams, TradesResponse,
    },
};

const CLOB_BASE_URL: &str = "https://clob.polymarket.com";
const GAMMA_BASE_URL: &str = "https://gamma-api.polymarket.com";
const POLYMARKET_BASE_URL: &str = "https://polymarket.com";

#[derive(Clone)]
pub struct PolymarketClient {
    http: Client,
    clob_base: String,
    gamma_base: String,
}

impl PolymarketClient {
    pub fn new() -> Self {
        Self {
            http: Client::new(),
            clob_base: CLOB_BASE_URL.to_string(),
            gamma_base: GAMMA_BASE_URL.to_string(),
        }
    }

    pub fn with_base_urls(clob_base: impl Into<String>, gamma_base: impl Into<String>) -> Self {
        Self {
            http: Client::new(),
            clob_base: clob_base.into(),
            gamma_base: gamma_base.into(),
        }
    }

    async fn get_clob<T>(&self, path: &str, query: &[(&str, String)]) -> Result<T, PolymarketError>
    where
        T: serde::de::DeserializeOwned,
    {
        crate::metrics::record_http_request("polymarket_clob");
        let url = format!("{}{}", self.clob_base, path);
        let resp = self.http.get(&url).query(query).send().await?;
        Self::parse_response(resp).await
    }

    async fn get_gamma<T>(&self, path: &str, query: &[(&str, String)]) -> Result<T, PolymarketError>
    where
        T: serde::de::DeserializeOwned,
    {
        crate::metrics::record_http_request("polymarket_gamma");
        let url = format!("{}{}", self.gamma_base, path);
        let resp = self.http.get(&url).query(query).send().await?;
        Self::parse_response(resp).await
    }

    async fn parse_response<T>(resp: reqwest::Response) -> Result<T, PolymarketError>
    where
        T: serde::de::DeserializeOwned,
    {
        let status = resp.status();
        if !status.is_success() {
            let message = resp.text().await.unwrap_or_default();
            return Err(PolymarketError::Api {
                status: status.as_u16(),
                message,
            });
        }
        let bytes = resp.bytes().await.map_err(|e| PolymarketError::Deserialize(e.to_string()))?;
        serde_json::from_slice::<T>(&bytes).map_err(|e| {
            let raw = String::from_utf8_lossy(&bytes);
            PolymarketError::Deserialize(format!("{e}\nraw body: {raw}"))
        })
    }

    // ─── CLOB: Markets ───────────────────────────────────────────────────────

    pub async fn get_markets(&self, params: MarketsParams) -> Result<MarketsResponse, PolymarketError> {
        let mut q: Vec<(&str, String)> = vec![];
        if let Some(cursor) = params.next_cursor {
            q.push(("next_cursor", cursor));
        }
        if let Some(limit) = params.limit {
            q.push(("limit", limit.to_string()));
        }
        self.get_clob("/markets", &q).await
    }

    pub async fn get_market(&self, condition_id: &str) -> Result<ClobMarket, PolymarketError> {
        self.get_clob(&format!("/markets/{condition_id}"), &[]).await
    }

    // ─── CLOB: Order Book ────────────────────────────────────────────────────

    pub async fn get_order_book(&self, token_id: &str) -> Result<OrderBook, PolymarketError> {
        self.get_clob(&format!("/book"), &[("token_id", token_id.to_string())]).await
    }

    // ─── CLOB: Price ─────────────────────────────────────────────────────────

    /// side: "BUY" or "SELL"
    pub async fn get_price(&self, token_id: &str, side: &str) -> Result<PriceResponse, PolymarketError> {
        self.get_clob(
            "/price",
            &[("token_id", token_id.to_string()), ("side", side.to_string())],
        )
        .await
    }

    // ─── CLOB: Spread ────────────────────────────────────────────────────────

    pub async fn get_spread(&self, token_id: &str) -> Result<SpreadResponse, PolymarketError> {
        self.get_clob("/spread", &[("token_id", token_id.to_string())]).await
    }

    // ─── CLOB: Midpoint ──────────────────────────────────────────────────────

    pub async fn get_midpoint(&self, token_id: &str) -> Result<MidpointResponse, PolymarketError> {
        self.get_clob("/midpoint", &[("token_id", token_id.to_string())]).await
    }

    // ─── CLOB: Last Trade Price ──────────────────────────────────────────────

    pub async fn get_last_trade_price(&self, token_id: &str) -> Result<LastTradePriceResponse, PolymarketError> {
        self.get_clob("/last-trade-price", &[("token_id", token_id.to_string())]).await
    }

    // ─── CLOB: Tick Size ─────────────────────────────────────────────────────

    pub async fn get_tick_size(&self, token_id: &str) -> Result<TickSizeResponse, PolymarketError> {
        self.get_clob("/tick-size", &[("token_id", token_id.to_string())]).await
    }

    // ─── CLOB: Trades ────────────────────────────────────────────────────────

    pub async fn get_trades(&self, params: TradesParams) -> Result<TradesResponse, PolymarketError> {
        let mut q: Vec<(&str, String)> = vec![];
        if let Some(market) = params.market {
            q.push(("market", market));
        }
        if let Some(asset_id) = params.asset_id {
            q.push(("asset_id", asset_id));
        }
        if let Some(limit) = params.limit {
            q.push(("limit", limit.to_string()));
        }
        if let Some(before) = params.before {
            q.push(("before", before));
        }
        if let Some(after) = params.after {
            q.push(("after", after));
        }
        if let Some(cursor) = params.next_cursor {
            q.push(("next_cursor", cursor));
        }
        self.get_clob("/trades", &q).await
    }

    // ─── Gamma: Markets ──────────────────────────────────────────────────────

    pub async fn get_gamma_markets(
        &self,
        params: GammaMarketsParams,
    ) -> Result<Vec<GammaMarket>, PolymarketError> {
        let mut q: Vec<(&str, String)> = vec![];
        if let Some(v) = params.limit           { q.push(("limit", v.to_string())); }
        if let Some(v) = params.offset          { q.push(("offset", v.to_string())); }
        if let Some(v) = params.order           { q.push(("order", v)); }
        if let Some(v) = params.ascending       { q.push(("ascending", v.to_string())); }
        if let Some(v) = params.active          { q.push(("active", v.to_string())); }
        if let Some(v) = params.closed          { q.push(("closed", v.to_string())); }
        if let Some(v) = params.archived        { q.push(("archived", v.to_string())); }
        if let Some(v) = params.liquidity_num_min { q.push(("liquidity_num_min", v.to_string())); }
        if let Some(v) = params.liquidity_num_max { q.push(("liquidity_num_max", v.to_string())); }
        if let Some(v) = params.volume_num_min  { q.push(("volume_num_min", v.to_string())); }
        if let Some(v) = params.volume_num_max  { q.push(("volume_num_max", v.to_string())); }
        if let Some(v) = params.start_date_min  { q.push(("start_date_min", v)); }
        if let Some(v) = params.start_date_max  { q.push(("start_date_max", v)); }
        if let Some(v) = params.end_date_min    { q.push(("end_date_min", v)); }
        if let Some(v) = params.end_date_max    { q.push(("end_date_max", v)); }
        if let Some(v) = params.tag             { q.push(("tag", v)); }
        if let Some(v) = params.related_tags    { q.push(("related_tags", v.to_string())); }
        self.get_gamma("/markets", &q).await
    }

    pub async fn get_gamma_market(&self, condition_id: &str) -> Result<GammaMarket, PolymarketError> {
        self.get_gamma(&format!("/markets/{condition_id}"), &[]).await
    }

    pub async fn get_gamma_market_by_slug(&self, slug: &str) -> Result<GammaMarket, PolymarketError> {
        let mut results: Vec<GammaMarket> =
            self.get_gamma("/markets", &[("slug", slug.to_string())]).await?;
        results.pop().ok_or_else(|| PolymarketError::Api {
            status: 404,
            message: format!("no market found with slug '{slug}'"),
        })
    }

    // ─── Gamma: Events ───────────────────────────────────────────────────────

    pub async fn get_gamma_events(
        &self,
        params: GammaEventsParams,
    ) -> Result<Vec<GammaEvent>, PolymarketError> {
        let mut q: Vec<(&str, String)> = vec![];
        if let Some(v) = params.limit     { q.push(("limit", v.to_string())); }
        if let Some(v) = params.offset    { q.push(("offset", v.to_string())); }
        if let Some(v) = params.order     { q.push(("order", v)); }
        if let Some(v) = params.ascending { q.push(("ascending", v.to_string())); }
        if let Some(v) = params.active    { q.push(("active", v.to_string())); }
        if let Some(v) = params.closed    { q.push(("closed", v.to_string())); }
        if let Some(v) = params.archived  { q.push(("archived", v.to_string())); }
        if let Some(v) = params.tag       { q.push(("tag", v)); }
        if let Some(v) = params.slug      { q.push(("slug", v)); }
        self.get_gamma("/events", &q).await
    }

    pub async fn get_gamma_event(&self, event_id: &str) -> Result<GammaEvent, PolymarketError> {
        self.get_gamma(&format!("/events/{event_id}"), &[]).await
    }

    pub async fn get_gamma_event_by_slug(&self, slug: &str) -> Result<Option<GammaEvent>, PolymarketError> {
        let mut results: Vec<GammaEvent> =
            self.get_gamma("/events", &[("slug", slug.to_string())]).await?;
        Ok(results.pop())
    }

    // ─── Scrape: Strike Price from Event Page ───────────────────────────

    /// Fetch the Polymarket event page HTML and extract the `priceToBeat`
    /// value from the embedded `__NEXT_DATA__` JSON payload.
    ///
    /// `event_slug` is e.g. `"btc-updown-15m-1775595600"`. The trailing unix
    /// timestamp is used to disambiguate which `crypto-prices` query to read
    /// — see [`parse_price_to_beat_from_html`] for why this matters.
    pub async fn scrape_strike_price(&self, event_slug: &str) -> Result<Option<f64>, PolymarketError> {
        let expected_start = event_slug
            .rsplit('-')
            .next()
            .and_then(|s| s.parse::<u64>().ok());

        let url = format!("{}/event/{}", POLYMARKET_BASE_URL, event_slug);
        println!("scraping strike price from: {url}");
        crate::metrics::record_http_request("polymarket_scrape");
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(PolymarketError::Api {
                status: resp.status().as_u16(),
                message: format!("failed to fetch event page: {url}"),
            });
        }

        let html = resp.text().await.map_err(|e| PolymarketError::Deserialize(e.to_string()))?;

        Ok(parse_price_to_beat_from_html(&html, expected_start))
    }
}

/// Extract the strike price ("Price To Beat") from `__NEXT_DATA__` in the HTML.
///
/// The page embeds dehydrated React Query data. For the **active** market the
/// strike price lives in the `"crypto-prices"` query as `openPrice`:
///
/// ```json
/// { "queryKey": ["crypto-prices","price","BTC","<start>","fifteen","<end>"],
///   "state": { "data": { "openPrice": 69988.527, "closePrice": null } } }
/// ```
///
/// The page typically carries **several** `crypto-prices` entries (one per
/// historical window, used for the chart). At a window rotation the
/// just-opened window's entry has `openPrice: null` while previous windows
/// still carry valid numbers — so picking "the first non-null" returns the
/// *previous* window's strike. `expected_start` (the `<start>` embedded in
/// the slug) is matched against `queryKey[3]` to select the entry for the
/// exact window we asked about. Accepts the timestamp as seconds, milliseconds,
/// or either in string form (Polymarket has shipped all three).
///
/// Returns `None` when no matching entry is found. The previous implementation
/// recursed into `eventMetadata.priceToBeat` as a fallback, but that grabbed
/// unrelated markets' values out of the same blob and caused wrong strikes at
/// rotation. Callers should rely on Gamma / Chainlink fallbacks instead.
fn parse_price_to_beat_from_html(html: &str, expected_start: Option<u64>) -> Option<f64> {
    let json_str = extract_next_data_json(html)?;
    let data: serde_json::Value = serde_json::from_str(json_str).ok()?;

    let queries = data
        .pointer("/props/pageProps/dehydratedState/queries")
        .and_then(|v| v.as_array())?;

    let mut first_fallback: Option<f64> = None;

    for q in queries {
        let key = match q.get("queryKey").and_then(|k| k.as_array()) {
            Some(k) => k,
            None => continue,
        };
        if key.first().and_then(|v| v.as_str()) != Some("crypto-prices") {
            continue;
        }

        let open_price = q.pointer("/state/data/openPrice").and_then(|v| v.as_f64());
        let key_start_secs = key.get(3).and_then(parse_ts_any);

        match (expected_start, key_start_secs, open_price) {
            (Some(expected), Some(k), Some(open)) if k == expected => return Some(open),
            (None, _, Some(open)) if first_fallback.is_none() => {
                first_fallback = Some(open);
            }
            _ => {}
        }
    }

    first_fallback
}

/// Parse a `queryKey` timestamp cell into unix **seconds**.
///
/// Polymarket has shipped this field in at least four shapes across versions:
/// - JSON number in seconds   (e.g. `1776548700`)
/// - JSON number in ms        (e.g. `1776548700000`)
/// - JSON string of either    (`"1776548700"` / `"1776548700000"`)
/// - RFC 3339 string          (`"2026-04-18T21:45:00Z"` — currently shipped)
fn parse_ts_any(v: &serde_json::Value) -> Option<u64> {
    // Any unix value ≥ this is unambiguously milliseconds (would be year 2286 in seconds).
    const MS_CUTOFF: u64 = 10_000_000_000;

    if let Some(n) = v.as_u64() {
        return Some(if n >= MS_CUTOFF { n / 1000 } else { n });
    }
    if let Some(s) = v.as_str() {
        if let Ok(n) = s.parse::<u64>() {
            return Some(if n >= MS_CUTOFF { n / 1000 } else { n });
        }
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
            return Some(dt.timestamp() as u64);
        }
    }
    None
}

/// Slice out the JSON content of the `<script id="__NEXT_DATA__">` tag.
fn extract_next_data_json(html: &str) -> Option<&str> {
    let marker = r#"id="__NEXT_DATA__""#;
    let after_marker = &html[html.find(marker)?..];
    let json_start = after_marker.find('>')? + 1;
    let json_slice = &after_marker[json_start..];
    let json_end = json_slice.find("</script>")?;
    Some(&json_slice[..json_end])
}

impl Default for PolymarketClient {
    fn default() -> Self {
        Self::new()
    }
}
