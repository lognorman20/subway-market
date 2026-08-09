//! DB-backed trade / PnL projection tests (Postgres on :5433).

use chrono::Utc;
use diesel::prelude::*;
use diesel_async::pooled_connection::deadpool::Pool;
use diesel_async::pooled_connection::AsyncDieselConnectionManager;
use diesel_async::{AsyncPgConnection, RunQueryDsl};
use dto::{
    AgentId, BusEvent, EventMeta, MarketId, MarketOpened, MarketOutcome, MarketResolved,
    MarketSide, TradeAction, TradeFilled, TradeId,
};
use ledger::writer::DbWriter;
use uuid::Uuid;

fn database_url() -> String {
    std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://subway:subway@127.0.0.1:5433/subway_ledger".into())
}

async fn pool() -> Pool<AsyncPgConnection> {
    let config = AsyncDieselConnectionManager::<AsyncPgConnection>::new(database_url());
    Pool::builder(config)
        .max_size(2)
        .build()
        .expect("pool")
}

async fn seed_agent(pool: &Pool<AsyncPgConnection>, aid: &str) {
    use db::schema::agents::dsl::*;
    let mut conn = pool.get().await.expect("conn");
    diesel::insert_into(agents)
        .values((
            agent_id.eq(aid),
            owner_wallet.eq("0xseed"),
            display_name.eq(Some("test")),
            api_key_hash.eq("hash"),
            created_at.eq(Utc::now()),
        ))
        .on_conflict(agent_id)
        .do_nothing()
        .execute(&mut conn)
        .await
        .expect("seed agent");
}

fn opened(market_id: &str, dedupe: &str) -> BusEvent {
    BusEvent::MarketOpened(MarketOpened {
        meta: EventMeta::new(),
        market_id: MarketId(market_id.into()),
        dedupe_key: dedupe.into(),
        trip_id: "t1".into(),
        route_id: "A".into(),
        train_id: Some("A1".into()),
        start_stop_id: "A01".into(),
        start_eta_unix: 1_699_999_900,
        stop_id: "A99".into(),
        frozen_eta_unix: 1_700_000_000,
        liquidity_b: "100000000".into(),
        chain_tx_hash: "0xopen".into(),
        opened_at: Utc::now(),
    })
}

fn fill(
    market_id: &str,
    agent_id: &str,
    action: TradeAction,
    shares: &str,
    usdc: i64,
    price_wad: &str,
) -> BusEvent {
    BusEvent::TradeFilled(TradeFilled {
        meta: EventMeta::new(),
        trade_id: TradeId::new(),
        market_id: MarketId(market_id.into()),
        agent_id: AgentId(agent_id.into()),
        side: MarketSide::Early,
        action,
        shares: shares.into(),
        usdc_amount: usdc,
        price_wad: price_wad.into(),
        chain_tx_hash: format!("0x{}", Uuid::new_v4().simple()),
        filled_at: Utc::now(),
    })
}

fn resolved(market_id: &str, outcome: MarketOutcome) -> BusEvent {
    BusEvent::MarketResolved(MarketResolved {
        meta: EventMeta::new(),
        market_id: MarketId(market_id.into()),
        outcome,
        actual_arrival_unix: 1_700_000_100,
        chain_tx_hash: "0xresolve".into(),
        resolved_at: Utc::now(),
    })
}

#[tokio::test]
async fn db_buy_fill_updates_pnl() {
    let pool = pool().await;
    let writer = DbWriter::new(pool.clone());

    let mid = format!("m-buy-{}", Uuid::new_v4().simple());
    let aid = format!("a-buy-{}", Uuid::new_v4().simple());
    let dedupe = format!("dk-{}", Uuid::new_v4().simple());

    seed_agent(&pool, &aid).await;
    writer
        .insert(dto::MARKET_OPENED, &opened(&mid, &dedupe))
        .await
        .expect("open");
    writer
        .insert(
            dto::TRADE_FILLED,
            &fill(
                &mid,
                &aid,
                TradeAction::Buy,
                "1000000",
                500_000,
                "500000000000000000",
            ),
        )
        .await
        .expect("buy");

    let mut conn = pool.get().await.unwrap();
    let (cost, realized, unrealized, total, early_shares): (i64, i64, i64, i64, String) =
        db::schema::agent_market_pnl::table
            .select((
                db::schema::agent_market_pnl::cost_basis_usdc,
                db::schema::agent_market_pnl::realized_pnl_usdc,
                db::schema::agent_market_pnl::unrealized_pnl_usdc,
                db::schema::agent_market_pnl::total_pnl_usdc,
                db::schema::agent_market_pnl::early_shares,
            ))
            .filter(db::schema::agent_market_pnl::agent_id.eq(&aid))
            .filter(db::schema::agent_market_pnl::market_id.eq(&mid))
            .first(&mut conn)
            .await
            .expect("pnl row");

    assert_eq!(early_shares, "1000000");
    assert_eq!(cost, 500_000);
    assert_eq!(realized, 0);
    assert_eq!(unrealized, 0);
    assert_eq!(total, 0);

    let trade_count: i64 = db::schema::trades::table
        .filter(db::schema::trades::market_id.eq(&mid))
        .count()
        .get_result(&mut conn)
        .await
        .unwrap();
    assert_eq!(trade_count, 1);
}

#[tokio::test]
async fn db_sell_fill_changes_realized() {
    let pool = pool().await;
    let writer = DbWriter::new(pool.clone());

    let mid = format!("m-sell-{}", Uuid::new_v4().simple());
    let aid = format!("a-sell-{}", Uuid::new_v4().simple());
    let dedupe = format!("dk-{}", Uuid::new_v4().simple());

    seed_agent(&pool, &aid).await;
    writer
        .insert(dto::MARKET_OPENED, &opened(&mid, &dedupe))
        .await
        .expect("open");
    writer
        .insert(
            dto::TRADE_FILLED,
            &fill(
                &mid,
                &aid,
                TradeAction::Buy,
                "1000000",
                500_000,
                "500000000000000000",
            ),
        )
        .await
        .expect("buy");
    writer
        .insert(
            dto::TRADE_FILLED,
            &fill(
                &mid,
                &aid,
                TradeAction::Sell,
                "500000",
                300_000,
                "500000000000000000",
            ),
        )
        .await
        .expect("sell");

    let mut conn = pool.get().await.unwrap();
    let (cost, realized, early_shares): (i64, i64, String) = db::schema::agent_market_pnl::table
        .select((
            db::schema::agent_market_pnl::cost_basis_usdc,
            db::schema::agent_market_pnl::realized_pnl_usdc,
            db::schema::agent_market_pnl::early_shares,
        ))
        .filter(db::schema::agent_market_pnl::agent_id.eq(&aid))
        .filter(db::schema::agent_market_pnl::market_id.eq(&mid))
        .first(&mut conn)
        .await
        .expect("pnl");

    assert_eq!(early_shares, "500000");
    assert_eq!(cost, 250_000);
    assert_eq!(realized, 50_000);
}

#[tokio::test]
async fn db_resolve_zeroes_unrealized() {
    let pool = pool().await;
    let writer = DbWriter::new(pool.clone());

    let mid = format!("m-res-{}", Uuid::new_v4().simple());
    let aid = format!("a-res-{}", Uuid::new_v4().simple());
    let dedupe = format!("dk-{}", Uuid::new_v4().simple());

    seed_agent(&pool, &aid).await;
    writer
        .insert(dto::MARKET_OPENED, &opened(&mid, &dedupe))
        .await
        .expect("open");
    // Buy below mark so unrealized is positive after a mark-up trade.
    writer
        .insert(
            dto::TRADE_FILLED,
            &fill(
                &mid,
                &aid,
                TradeAction::Buy,
                "1000000",
                400_000,
                "500000000000000000",
            ),
        )
        .await
        .expect("buy");

    {
        let mut conn = pool.get().await.unwrap();
        let unrealized: i64 = db::schema::agent_market_pnl::table
            .select(db::schema::agent_market_pnl::unrealized_pnl_usdc)
            .filter(db::schema::agent_market_pnl::agent_id.eq(&aid))
            .filter(db::schema::agent_market_pnl::market_id.eq(&mid))
            .first(&mut conn)
            .await
            .unwrap();
        assert_eq!(unrealized, 100_000);
    }

    writer
        .insert(dto::MARKET_RESOLVED, &resolved(&mid, MarketOutcome::Early))
        .await
        .expect("resolve");

    let mut conn = pool.get().await.unwrap();
    let (realized, unrealized, total, resolved_flag, early_shares): (i64, i64, i64, bool, String) =
        db::schema::agent_market_pnl::table
            .select((
                db::schema::agent_market_pnl::realized_pnl_usdc,
                db::schema::agent_market_pnl::unrealized_pnl_usdc,
                db::schema::agent_market_pnl::total_pnl_usdc,
                db::schema::agent_market_pnl::resolved,
                db::schema::agent_market_pnl::early_shares,
            ))
            .filter(db::schema::agent_market_pnl::agent_id.eq(&aid))
            .filter(db::schema::agent_market_pnl::market_id.eq(&mid))
            .first(&mut conn)
            .await
            .expect("pnl after resolve");

    assert!(resolved_flag);
    assert_eq!(early_shares, "0");
    assert_eq!(unrealized, 0);
    assert_eq!(realized, 600_000);
    assert_eq!(total, 600_000);

    let outcome: String = db::schema::markets::table
        .select(db::schema::markets::outcome)
        .filter(db::schema::markets::market_id.eq(&mid))
        .first(&mut conn)
        .await
        .unwrap();
    assert_eq!(outcome, "early");
}
