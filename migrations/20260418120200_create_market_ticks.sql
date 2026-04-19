-- One row per CLOB / oracle tick observed for a market window.
-- High-volume table — expect thousands of rows per window.
-- Nullable numerics: store NULL for values that haven't arrived yet
-- (e.g. chainlink tick before the first CLOB tick).

CREATE TABLE market_ticks (
    id           UUID              PRIMARY KEY DEFAULT gen_random_uuid(),
    market_id    UUID              NOT NULL REFERENCES updown_market(id) ON DELETE CASCADE,
    coin_price   DOUBLE PRECISION,
    up           DOUBLE PRECISION,
    down         DOUBLE PRECISION,
    pct               DOUBLE PRECISION,
    milliseconds_left BIGINT            NOT NULL,
    ts                TIMESTAMPTZ       NOT NULL
);

CREATE INDEX idx_market_ticks_market_ts ON market_ticks (market_id, ts DESC);
CREATE INDEX idx_market_ticks_ts        ON market_ticks (ts);
