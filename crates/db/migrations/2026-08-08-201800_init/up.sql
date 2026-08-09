CREATE TABLE IF NOT EXISTS bus_events (
    id BIGSERIAL NOT NULL,
    event_id TEXT PRIMARY KEY,
    diff_id TEXT NULL,
    event_type TEXT NOT NULL,
    subject TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    actual_unix BIGINT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS bus_events_subject_received_at_idx ON bus_events (subject, received_at);
CREATE INDEX IF NOT EXISTS bus_events_diff_id_idx ON bus_events (diff_id) WHERE diff_id IS NOT NULL;

CREATE TABLE IF NOT EXISTS tracked_trains (
    dedupe_key TEXT PRIMARY KEY,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    phase TEXT NOT NULL,
    route_id TEXT NOT NULL,
    trip_id TEXT NOT NULL,
    nyct_train_id TEXT NULL,
    first_seen_at TIMESTAMPTZ NULL,
    start_stop_id TEXT NULL,
    target_stop_id TEXT NULL,
    frozen_at TIMESTAMPTZ NULL,
    expected_arrival_unix BIGINT NULL,
    remaining_stops INTEGER NULL,
    arrival_unix BIGINT NULL,
    diff_seconds BIGINT NULL,
    actual_source TEXT NULL,
    action TEXT NULL,
    side TEXT NULL,
    reason TEXT NULL,
    diff_id TEXT NULL,
    freeze_event_id TEXT NULL,
    diff_event_id TEXT NULL,
    status_event_id TEXT NULL,
    CONSTRAINT tracked_trains_phase_check CHECK (phase IN ('watching', 'frozen', 'arrived', 'acted'))
);

CREATE INDEX IF NOT EXISTS tracked_trains_phase_updated_at_idx ON tracked_trains (phase, updated_at DESC);
CREATE INDEX IF NOT EXISTS tracked_trains_route_updated_at_idx ON tracked_trains (route_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS tracked_trains_trip_id_idx ON tracked_trains (trip_id);
CREATE INDEX IF NOT EXISTS tracked_trains_diff_id_idx ON tracked_trains (diff_id) WHERE diff_id IS NOT NULL;
