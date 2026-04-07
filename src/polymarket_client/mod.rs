pub mod client;
pub mod error;
pub mod models;
pub(crate) mod serde_utils;

pub use client::PolymarketClient;
#[allow(unused_imports)]
pub use error::PolymarketError;
