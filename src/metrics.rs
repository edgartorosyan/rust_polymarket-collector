//! Prometheus `/metrics` exporter — OS / process / HTTP / DB stats.
//!
//! Call [`init`] once at startup. That:
//! - installs a `metrics` crate recorder (makes `counter!` / `gauge!` work anywhere),
//! - mounts an axum server at `bind_addr` serving `/metrics` in Prometheus text format,
//! - spawns a 5-second background task that refreshes sysinfo + DB-pool gauges.
//!
//! # Metrics exposed
//!
//! Gauges (refreshed every 5 s):
//! - `process_memory_rss_bytes`
//! - `process_cpu_percent`
//! - `process_disk_read_bytes_total`   (cumulative since process start)
//! - `process_disk_written_bytes_total`
//! - `db_pool_size`                    (open connections, idle + active)
//! - `db_pool_idle_connections`
//!
//! Counter (incremented by [`record_http_request`] at each outbound call):
//! - `http_requests_total{client="…"}` — `client` is e.g. `polymarket_clob`,
//!   `polymarket_gamma`, `polymarket_scrape`, `chainlink_rest`, `binance_rest`.

use std::time::Duration;

use axum::{Router, extract::State, routing::get};
use metrics::{counter, gauge};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sysinfo::{Pid, System};

use crate::db;

/// Install the global Prometheus recorder, spawn the HTTP server and the
/// sysinfo reporter loop. Returns after the listener is bound, but the server
/// keeps running in a detached task for the process's lifetime.
pub async fn init(bind_addr: &str) -> anyhow::Result<()> {
    let handle = PrometheusBuilder::new().install_recorder()?;

    tokio::spawn(reporter_loop(Duration::from_secs(5)));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    let app = Router::new()
        .route("/metrics", get(render))
        .with_state(handle);

    let addr = listener.local_addr()?;
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("[metrics] server error: {e}");
        }
    });

    eprintln!("[metrics] serving /metrics on http://{addr}/metrics");
    Ok(())
}

async fn render(State(handle): State<PrometheusHandle>) -> String {
    handle.render()
}

async fn reporter_loop(interval: Duration) {
    let pid = Pid::from_u32(std::process::id());
    let mut sys = System::new();
    let mut ticker = tokio::time::interval(interval);
    // First tick fires immediately; skip it so the first sample isn't zero-filled.
    ticker.tick().await;

    loop {
        ticker.tick().await;

        sys.refresh_all();

        if let Some(p) = sys.process(pid) {
            gauge!("process_memory_rss_bytes").set(p.memory() as f64);
            gauge!("process_cpu_percent").set(p.cpu_usage() as f64);
            let io = p.disk_usage();
            gauge!("process_disk_read_bytes_total").set(io.total_read_bytes as f64);
            gauge!("process_disk_written_bytes_total").set(io.total_written_bytes as f64);
        }

        let pool = db::pool();
        gauge!("db_pool_size").set(pool.size() as f64);
        gauge!("db_pool_idle_connections").set(pool.num_idle() as f64);
    }
}

/// Bump the `http_requests_total` counter for an outbound HTTP call.
///
/// `client` is a free-form label value identifying the caller (e.g.
/// `"polymarket_clob"`). Keep the cardinality low — don't pass full URLs.
pub fn record_http_request(client: &'static str) {
    counter!("http_requests_total", "client" => client).increment(1);
}
