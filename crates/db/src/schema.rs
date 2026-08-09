// @generated manually — run `diesel migration run` to apply, then refresh if needed.

diesel::table! {
    agent_market_pnl (agent_id, market_id) {
        agent_id -> Text,
        market_id -> Text,
        early_shares -> Text,
        late_shares -> Text,
        cost_basis_usdc -> Int8,
        realized_pnl_usdc -> Int8,
        unrealized_pnl_usdc -> Int8,
        total_pnl_usdc -> Int8,
        last_mark_wad -> Nullable<Text>,
        resolved -> Bool,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    agents (agent_id) {
        agent_id -> Text,
        owner_wallet -> Text,
        display_name -> Nullable<Text>,
        api_key_hash -> Text,
        created_at -> Timestamptz,
        bankroll_usdc -> Int8,
    }
}

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
    markets (market_id) {
        market_id -> Text,
        dedupe_key -> Text,
        trip_id -> Text,
        route_id -> Text,
        train_id -> Nullable<Text>,
        stop_id -> Text,
        frozen_eta_unix -> Int8,
        liquidity_b -> Text,
        q_early -> Text,
        q_late -> Text,
        outcome -> Text,
        opened_event_id -> Nullable<Text>,
        resolved_event_id -> Nullable<Text>,
        chain_create_tx -> Nullable<Text>,
        chain_resolve_tx -> Nullable<Text>,
        opened_at -> Timestamptz,
        resolved_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    positions (agent_id, market_id, side) {
        agent_id -> Text,
        market_id -> Text,
        side -> Text,
        shares -> Text,
        cost_basis_usdc -> Int8,
        updated_at -> Timestamptz,
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

diesel::table! {
    trades (trade_id) {
        trade_id -> Text,
        event_id -> Text,
        market_id -> Text,
        agent_id -> Text,
        side -> Text,
        action -> Text,
        shares -> Text,
        usdc_amount -> Int8,
        price_wad -> Text,
        chain_tx_hash -> Text,
        filled_at -> Timestamptz,
    }
}

diesel::joinable!(agent_market_pnl -> agents (agent_id));
diesel::joinable!(agent_market_pnl -> markets (market_id));
diesel::joinable!(positions -> agents (agent_id));
diesel::joinable!(positions -> markets (market_id));
diesel::joinable!(trades -> agents (agent_id));
diesel::joinable!(trades -> markets (market_id));

diesel::allow_tables_to_appear_in_same_query!(
    agent_market_pnl,
    agents,
    bus_events,
    markets,
    positions,
    tracked_trains,
    trades,
);
