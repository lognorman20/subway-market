use e2e::{ensure_platform_funded, open_market_and_wait, OpenFixture, WaitOpts};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let nats = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4223".into());
    let fix = OpenFixture::new_random();
    println!("emitting trip={} dedupe={}", fix.trip_id, fix.dedupe_key);
    ensure_platform_funded()?;
    let opts = WaitOpts::new(Duration::from_secs(60));
    let market_id = open_market_and_wait(&nats, &fix, opts).await?;
    println!("market_id={market_id}");
    Ok(())
}
