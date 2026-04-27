-- Speed up filters like "pct > X" and range scans on milliseconds_left.
-- pct is nullable; partial index skips NULLs to keep it small.

CREATE INDEX idx_market_ticks_pct
    ON market_ticks (pct)
    WHERE pct IS NOT NULL;

CREATE INDEX idx_market_ticks_milliseconds_left
    ON market_ticks (milliseconds_left);
