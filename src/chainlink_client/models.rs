use serde::Deserialize;

// ─── Wire types (crate-internal) ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct WsEnvelope {
    #[serde(default)]
    pub topic: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    pub payload: Option<PricePayload>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PricePayload {
    pub symbol: String,
    /// Unix timestamp in milliseconds of the Chainlink price observation.
    pub timestamp: u64,
    /// USD price from the Chainlink oracle.
    pub value: f64,
}

// ─── Public types ─────────────────────────────────────────────────────────────

/// A single Chainlink oracle price update received from the WebSocket.
#[derive(Debug, Clone)]
pub struct ChainlinkTick {
    pub symbol: String,
    /// Millisecond Unix timestamp of the Chainlink observation.
    pub timestamp_ms: u64,
    /// USD price.
    pub value: f64,
}
