CREATE TABLE IF NOT EXISTS receipts (
    id               TEXT PRIMARY KEY,
    kind             TEXT NOT NULL,
    capability       TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    status           TEXT NOT NULL,
    decision         TEXT NOT NULL,
    reason           TEXT,
    input            TEXT NOT NULL,
    context          TEXT NOT NULL,
    execution        TEXT,
    approval         TEXT,
    sandbox_enforced INTEGER NOT NULL DEFAULT 0,
    isolation_tier   TEXT,
    binary_sha256    TEXT,
    token_mint_id    TEXT,
    jail_config_digest TEXT
);
CREATE INDEX IF NOT EXISTS idx_receipts_capability ON receipts(capability);
CREATE INDEX IF NOT EXISTS idx_receipts_status     ON receipts(status);
CREATE INDEX IF NOT EXISTS idx_receipts_created_at ON receipts(created_at);
