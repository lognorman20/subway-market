// @generated manually — run `diesel migration run` to apply, then refresh if needed.

diesel::table! {
    bus_events (event_id) {
        id -> Int8,
        event_id -> Text,
        diff_id -> Nullable<Text>,
        event_type -> Text,
        subject -> Text,
        payload -> Jsonb,
        actual_unix -> Nullable<Int8>,
        received_at -> Timestamptz,
    }
}

diesel::table! {
    tracked_trains (dedupe_key) {
        dedupe_key -> Text,
        updated_at -> Timestamptz,
        phase -> Text,
        route_id -> Text,
        trip_id -> Text,
        nyct_train_id -> Nullable<Text>,
        first_seen_at -> Nullable<Timestamptz>,
        start_stop_id -> Nullable<Text>,
        target_stop_id -> Nullable<Text>,
        frozen_at -> Nullable<Timestamptz>,
        expected_arrival_unix -> Nullable<Int8>,
        remaining_stops -> Nullable<Int4>,
        arrival_unix -> Nullable<Int8>,
        diff_seconds -> Nullable<Int8>,
        actual_source -> Nullable<Text>,
        action -> Nullable<Text>,
        side -> Nullable<Text>,
        reason -> Nullable<Text>,
        diff_id -> Nullable<Text>,
        freeze_event_id -> Nullable<Text>,
        diff_event_id -> Nullable<Text>,
        status_event_id -> Nullable<Text>,
    }
}

diesel::allow_tables_to_appear_in_same_query!(bus_events, tracked_trains,);
