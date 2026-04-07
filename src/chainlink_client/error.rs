use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChainlinkError {
    #[error("WebSocket connection failed: {0}")]
    WebSocket(String),

    #[error("symbols list must not be empty")]
    EmptySymbols,
}
