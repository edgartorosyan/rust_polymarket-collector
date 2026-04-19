-- One row per discovered market window (unique per slug / condition_id).
-- strike_price is nullable: a future window may be known before its open.

CREATE TABLE updown_market (
    id             UUID              PRIMARY KEY DEFAULT gen_random_uuid(),
    slug           TEXT              NOT NULL UNIQUE,
    title          TEXT              NOT NULL,
    condition_id   TEXT              NOT NULL UNIQUE,
    start_ts       TIMESTAMPTZ       NOT NULL,
    end_ts         TIMESTAMPTZ       NOT NULL,
    strike_price   DOUBLE PRECISION,
    market_type_id SMALLINT          NOT NULL REFERENCES updown_market_type(id)
);

CREATE INDEX idx_updown_market_type_id ON updown_market (market_type_id);
CREATE INDEX idx_updown_market_start_ts  ON updown_market (start_ts);
CREATE INDEX idx_updown_market_end_ts    ON updown_market (end_ts);


