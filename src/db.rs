//! PostgreSQL connection pool — single global `sqlx::PgPool` behind a
//! `OnceLock`. Call [`init`] exactly once at startup (see `main.rs`) and use
//! [`pool`] anywhere to borrow the shared pool.
//!
//! No schema / migrations ship with the crate yet; persistence is scaffolded
//! but not wired into the tick handlers. Max 5 connections.

use std::sync::OnceLock;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

static POOL: OnceLock<PgPool> = OnceLock::new();

/// Initialise the global connection pool. Call once at startup.
pub async fn init(database_url: &str) -> anyhow::Result<()> {
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    POOL.set(pool)
        .expect("db::init() called more than once");

    Ok(())
}

/// Get a reference to the global pool. Panics if [`init`] was not called.
pub fn pool() -> &'static PgPool {
    POOL.get().expect("database pool not initialised — call db::init() first")
}
