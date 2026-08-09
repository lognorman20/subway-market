CREATE TABLE IF NOT EXISTS agents (
    agent_id TEXT PRIMARY KEY,
    owner_wallet TEXT NOT NULL,
    display_name TEXT NULL,
    api_key_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS agents_owner_wallet_idx ON agents (owner_wallet);

CREATE TABLE IF NOT EXISTS markets (
    market_id TEXT PRIMARY KEY,
    dedupe_key TEXT NOT NULL UNIQUE,
    trip_id TEXT NOT NULL,
    route_id TEXT NOT NULL,
    train_id TEXT NULL,
    stop_id TEXT NOT NULL,
    frozen_eta_unix BIGINT NOT NULL,
    liquidity_b TEXT NOT NULL,
    q_early TEXT NOT NULL DEFAULT '0',
    q_late TEXT NOT NULL DEFAULT '0',
    outcome TEXT NOT NULL DEFAULT 'unresolved',
    opened_event_id TEXT NULL,
    resolved_event_id TEXT NULL,
    chain_create_tx TEXT NULL,
    chain_resolve_tx TEXT NULL,
    opened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ NULL,
    CONSTRAINT markets_outcome_check CHECK (outcome IN ('unresolved', 'early', 'late'))
);

CREATE INDEX IF NOT EXISTS markets_route_opened_at_idx ON markets (route_id, opened_at DESC);
CREATE INDEX IF NOT EXISTS markets_outcome_idx ON markets (outcome);

CREATE TABLE IF NOT EXISTS trades (
    trade_id TEXT PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    market_id TEXT NOT NULL REFERENCES markets (market_id),
    agent_id TEXT NOT NULL REFERENCES agents (agent_id),
    side TEXT NOT NULL,
    action TEXT NOT NULL,
    shares TEXT NOT NULL,
    usdc_amount BIGINT NOT NULL,
    price_wad TEXT NOT NULL,
    chain_tx_hash TEXT NOT NULL,
    filled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT trades_side_check CHECK (side IN ('early', 'late')),
    CONSTRAINT trades_action_check CHECK (action IN ('buy', 'sell'))
);

CREATE INDEX IF NOT EXISTS trades_market_filled_at_idx ON trades (market_id, filled_at DESC);
CREATE INDEX IF NOT EXISTS trades_agent_filled_at_idx ON trades (agent_id, filled_at DESC);

CREATE TABLE IF NOT EXISTS positions (
    agent_id TEXT NOT NULL REFERENCES agents (agent_id),
    market_id TEXT NOT NULL REFERENCES markets (market_id),
    side TEXT NOT NULL,
    shares TEXT NOT NULL DEFAULT '0',
    cost_basis_usdc BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (agent_id, market_id, side),
    CONSTRAINT positions_side_check CHECK (side IN ('early', 'late'))
);

CREATE TABLE IF NOT EXISTS agent_market_pnl (
    agent_id TEXT NOT NULL REFERENCES agents (agent_id),
    market_id TEXT NOT NULL REFERENCES markets (market_id),
    early_shares TEXT NOT NULL DEFAULT '0',
    late_shares TEXT NOT NULL DEFAULT '0',
    cost_basis_usdc BIGINT NOT NULL DEFAULT 0,
    realized_pnl_usdc BIGINT NOT NULL DEFAULT 0,
    unrealized_pnl_usdc BIGINT NOT NULL DEFAULT 0,
    total_pnl_usdc BIGINT NOT NULL DEFAULT 0,
    last_mark_wad TEXT NULL,
    resolved BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (agent_id, market_id)
);

CREATE INDEX IF NOT EXISTS agent_market_pnl_agent_idx ON agent_market_pnl (agent_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS agent_market_pnl_market_idx ON agent_market_pnl (market_id);
