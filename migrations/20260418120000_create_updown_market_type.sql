-- Lookup table for (coin, window) combinations.
-- IDs are explicit (no sequence) so other tables can hard-code FKs safely.

CREATE TABLE updown_market_type (
    id          SMALLINT    PRIMARY KEY,
    coin        VARCHAR(8)  NOT NULL CHECK (coin IN ('BTC', 'ETH', 'SOL', 'XRP')),
    window_size VARCHAR(8)  NOT NULL CHECK (window_size IN ('15m', '5m')),
    UNIQUE (coin, window_size)
);

INSERT INTO updown_market_type (id, coin, window_size) VALUES
    (1, 'BTC', '15m'),
    (2, 'BTC', '5m'),
    (3, 'ETH', '15m'),
    (4, 'ETH', '5m'),
    (5, 'SOL', '15m'),
    (6, 'SOL', '5m'),
    (7, 'XRP', '15m'),
    (8, 'XRP', '5m');
