-- Funding/identity state (backend may write). Not a trade projection.
ALTER TABLE agents
    ADD COLUMN IF NOT EXISTS bankroll_usdc BIGINT NOT NULL DEFAULT 0;
