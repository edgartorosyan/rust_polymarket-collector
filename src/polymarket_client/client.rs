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
        let url = format!("{}{}", self.clob_base, path);
        let resp = self.http.get(&url).query(query).send().await?;
        Self::parse_response(resp).await
    }

    async fn get_gamma<T>(&self, path: &str, query: &[(&str, String)]) -> Result<T, PolymarketError>
    where
        T: serde::de::DeserializeOwned,
    {
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
}

impl Default for PolymarketClient {
    fn default() -> Self {
        Self::new()
    }
}
