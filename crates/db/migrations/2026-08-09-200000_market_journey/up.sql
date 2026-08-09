-- Start/target journey fields for human-readable market UI.
ALTER TABLE markets
    ADD COLUMN IF NOT EXISTS start_stop_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS start_eta_unix BIGINT NOT NULL DEFAULT 0;
