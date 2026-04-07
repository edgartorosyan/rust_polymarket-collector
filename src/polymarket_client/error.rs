use thiserror::Error;

#[derive(Debug, Error)]
pub enum PolymarketError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("API error {status}: {message}")]
    Api { status: u16, message: String },

    #[error("Failed to deserialize response: {0}")]
    Deserialize(String),
}
