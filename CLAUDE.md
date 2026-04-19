# CLAUDE.md

Context for agents working in this repo. Keep responses tight — this is a small, focused crate.

## What this project is

Rust collector / live-monitor for **Polymarket "Up/Down" crypto prediction markets** (BTC / ETH / SOL, fixed windows such as 15m and 5m). It:

1. Discovers the currently-open market window for a `{coin, window_minutes}` pair via the Gamma API (slug pattern: `{coin}-updown-{window}m-{unix_ts}` where `unix_ts` is aligned to `window_secs`).
2. Resolves the window's **strike price** ("Price to Beat") — see resolution order below.
3. Streams live best-bid/ask ticks for the **Up** and **Down** outcome tokens from the CLOB WebSocket.
4. In parallel, streams the underlying coin's **Chainlink oracle** price (the same feed Polymarket uses to settle).
5. Feeds both streams into a `Strategy` object that decides what to do (currently just prints; DB persistence scaffolding is in `src/db.rs` and unused-but-wired).

Purpose is observability of updown markets and building strategies on top — **not** order placement. There is no signing / trading code.

## Layout

```
src/
├── main.rs                       # wires ChainlinkClient + PolymarketClient + run_updown_monitor + a Strategy
├── db.rs                         # sqlx PgPool, OnceLock global (init() once, pool() anywhere)
├── polymarket_client/            # REST (CLOB + Gamma) + event-page HTML scrape
│   ├── client.rs                 #   endpoint methods; scrape_strike_price parses __NEXT_DATA__
│   ├── models.rs                 #   ClobMarket, GammaMarket, GammaEvent, UpdownMarket, query params
│   ├── serde_utils.rs            #   deserialize_opt_f64_or_str — Gamma returns some fields as string vs number inconsistently
│   └── error.rs                  #   PolymarketError (Http, Api{status,msg}, Deserialize)
├── chainlink_client/             # oracle prices: live WS + historical REST with Binance fallback
│   ├── client.rs                 #   stream_prices (WS), price_at (REST → Binance klines fallback)
│   └── models.rs                 #   ChainlinkTick {symbol, timestamp_ms, value}
├── services/
│   ├── market_monitor.rs         # run_updown_monitor — perpetual loop; rotates market on window close
│   ├── polymarket_updown_service.rs  # slug construction, find_active_updown_market, resolve_outcome
│   ├── ws_updown_beat.rs         # stream_clob_prices / stream_clob_prices_until (best bid/ask stream)
│   └── ws_orderbook.rs           # stream_orderbook / stream_orderbook_until — full book snapshots + price_change updates
└── strategies/
    └── print.rs                  # PrintStrategy — stdout printer; flags when up_ask+down_ask ≤ 0.99
```

## Key concepts

### Strike price resolution order (in `market_monitor::run_updown_monitor`)

1. **HTML scrape** of the event page (`PolymarketClient::scrape_strike_price`) — reads `__NEXT_DATA__`, prefers the `crypto-prices` React Query's `openPrice`, falls back to `eventMetadata.priceToBeat` in the same blob. This is the primary source because Gamma's `group_line`/`line` are often missing/stale for brand-new windows.
2. **Gamma payload** — `GammaMarket.group_line` (recurring updown markets) or `.line` (older/manual).
3. **Chainlink REST / Binance fallback** — only if the window is already open; `ChainlinkClient::price_at` tries Chainlink Data Streams (requires `CHAINLINK_API_KEY`+`CHAINLINK_API_SECRET`), falls back to Binance 1m kline open price.
4. `None` — window not yet open and Gamma didn't carry the field; stays `None` for the session.

Do not reorder without a reason — each fallback exists because the one before it is occasionally wrong or absent.

### Market window rotation

`run_updown_monitor` loops forever. Downstream code detects rotation by comparing `UpdownTick.up_id` with the previously-seen value (see `PrintStrategy::last_up_id`). `stream_clob_prices_until` (and `stream_orderbook_until`, when wired) closes cleanly at `end_ts`; the monitor then discovers the next window.

### Concurrency model

`main.rs` uses `tokio::try_join!` on two single-task futures that share a `RefCell<PrintStrategy>` — intentionally **not** `Arc<Mutex<_>>`. Both futures poll on the same task, so `borrow_mut()` never overlaps. If you add a future that needs `Send`, this pattern breaks; switch to `Arc<Mutex<_>>` only then.

Inside `run_updown_monitor`, when an `OrderbookHandler` is supplied, `stream_clob_prices_until` and `stream_orderbook_until` are joined with `tokio::try_join!` for the window's lifetime. They share no state — `on_tick`, `on_up`, `on_down` are borrow-split into disjoint `&mut` references — so no `RefCell` is needed at this layer.

### Orderbook handler (optional)

`run_updown_monitor` takes `orderbook_handler: Option<OrderbookHandler>`. The struct holds two boxed `FnMut(OrderbookEvent)` fields — `on_up` and `on_down` — so the monitor stays non-generic on the callback types. Pass `None` for the original price-tick-only behaviour. Pass `Some(OrderbookHandler { on_up: Box::new(|ev| ...), on_down: Box::new(|ev| ...) })` to also stream full book depth for the current window; events are routed by asset ID.

### Database (`src/db.rs`)

Global `OnceLock<PgPool>`, initialised at startup from `DATABASE_URL`. `db::pool()` panics if `init()` wasn't called. Max 5 connections. Currently no schema/migrations in-tree — persistence is scaffolded but not wired into the tick handlers yet.

## Build / run

```bash
cargo build
cargo run              # requires .env with DATABASE_URL (Postgres must be running)
cargo check            # fast type-check, preferred during iteration
RUST_LOG=debug cargo run
```

Rust edition **2024** (see `Cargo.toml`). No tests in-tree yet — `cargo test` is a no-op.

Required env (see `.env`):
- `DATABASE_URL` — Postgres connection string. **Required** (startup panics otherwise).
- `RUST_LOG` — `tracing_subscriber` filter. Defaults to `info` when using `env-filter`.
- `CHAINLINK_API_KEY` / `CHAINLINK_API_SECRET` — optional; unlocks authoritative `price_at`. Without them, Binance 1m klines are used.
- `CLOB_BASE_URL` / `GAMMA_BASE_URL` — optional overrides (wire via `PolymarketClient::with_base_urls`; not wired to env by default).
- `WITH_ORDERBOOK` — `true` / `1` / `yes` / `on` to enable the second CLOB WS subscription for full Up/Down orderbooks (persisted to `orderbook_snapshots`). Defaults to `false`.

## External APIs / endpoints touched

- CLOB REST: `https://clob.polymarket.com` — `/markets`, `/book`, `/price`, `/spread`, `/midpoint`, `/last-trade-price`, `/tick-size`, `/trades`
- Gamma REST: `https://gamma-api.polymarket.com` — `/markets`, `/events`
- Polymarket WWW: `https://polymarket.com/event/{slug}` — HTML scrape for `__NEXT_DATA__`
- CLOB WS: `wss://ws-subscriptions-clob.polymarket.com/ws/market` — subscribe with `{auth:{}, markets:[], assets_ids:[...]}`; two event types: `book` (snapshot) and `price_change` (incremental). Frames are either a single object or a JSON array — `parse_clob_frame` handles both.
- Polymarket live-data WS: `wss://ws-live-data.polymarket.com` — `topic: "crypto_prices_chainlink"`. Respond to `type: "ping"` with `{action: "pong"}`.
- Chainlink Data Streams REST: `https://api.dataengine.chain.link/api/v1/reports` — HMAC-SHA256 signed (`cl_sign` in `chainlink_client/client.rs`).
- Binance: `https://api.binance.com/api/v3/klines` — unauthenticated fallback for historical prices.

## Conventions in this codebase

- **Logs** go to `stderr` via `eprintln!` with a `[tag]` prefix (`[monitor]`, `[clob]`, `[chainlink]`, `[ws_orderbook]`, `[db]`). Strategy output goes to `stdout`. `tracing` is initialised but used sparingly (only the `warn!` in `polymarket_updown_service`).
- **Errors**: library modules return typed errors (`PolymarketError`, `ChainlinkError`); service / app layer uses `anyhow::Result` and `bail!`.
- **WS reconnect**: every streamer is an infinite loop with 3-second backoff on any error. Callers never see disconnects — they get a fresh snapshot / tick stream.
- **Time**: mixed `f64` Unix seconds (market window boundaries) and `u64` millisecond timestamps (WS payloads). Don't unify without a reason — the mix matches what each API returns.
- **`#![allow(dead_code)]`** on `polymarket_client/client.rs` and `models.rs`: many REST endpoints are wrapped but unused by `main.rs`; they're kept for future strategies. Don't prune them.
- **Deserialization quirk**: Gamma returns `liquidity`, `volume`, `group_line`, `line` as either number or quoted string depending on endpoint nesting. Use `deserialize_opt_f64_or_str` for any new `Option<f64>` fields on `GammaMarket`.

## Adding a new strategy

1. Create `src/strategies/<name>.rs` with a struct holding whatever state it needs.
2. Expose methods matching the tick types you care about — `on_chainlink(&ChainlinkTick)` and/or `on_clob(&UpdownTick)` are the conventions `PrintStrategy` uses.
3. Add `pub mod <name>;` to `src/strategies/mod.rs`.
4. In `main.rs`, swap the `RefCell::new(PrintStrategy::new())` for your strategy and wire the closures to its methods.

No trait-based dispatch currently — strategies are concrete types selected at compile time in `main.rs`. Keep it that way unless you need runtime selection.

## Things NOT to do

- Don't add order-placement / signing code here without explicit ask — this crate is read-only observation by design.
- Don't replace `eprintln!` with `tracing` wholesale; the logs are intentionally informal and unstructured.
- Don't refactor the strike-price fallback chain to "simplify" — each source exists for a documented failure mode.
- Don't commit `.env` (it's not gitignored explicitly — only `/target` is — but the file holds credentials).
