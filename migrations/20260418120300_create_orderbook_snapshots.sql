-- One row per CLOB orderbook snapshot (full book) for a market window.
-- bids / asks are JSONB arrays of Level objects in server-delivered order:
--   [ { "price": 0.41, "size": 120.0 }, { "price": 0.4, "size": 500.0 }, ... ]
-- `outcome` disambiguates the Up vs Down token within the same market window
-- (both tokens of a market share `market_id`).

CREATE TABLE orderbook_snapshots (
    id         UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    market_id  UUID         NOT NULL REFERENCES updown_market(id) ON DELETE CASCADE,
    outcome    VARCHAR(4)   NOT NULL CHECK (outcome IN ('UP', 'DOWN')),
    bids       JSONB        NOT NULL,
    asks       JSONB        NOT NULL,
    ts         TIMESTAMPTZ  NOT NULL
);

CREATE INDEX idx_orderbook_snapshots_market_ts ON orderbook_snapshots (market_id, ts DESC);
CREATE INDEX idx_orderbook_snapshots_ts        ON orderbook_snapshots (ts);
