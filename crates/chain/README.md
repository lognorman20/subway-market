# Chain crate

Alloy bindings + tx helpers for the LMSR `SubwayMarket` (and MockUSDC/IERC20).

## Prereq

```bash
cd apps/contracts && forge build
```

Artifacts under `apps/contracts/out/` are gitignored; `sol!` loads them at compile time.

## Env (decision #8)

| Var | Role |
|-----|------|
| `MONAD_RPC_URL` | JSON-RPC endpoint (anvil / Monad) |
| `SUBWAY_MARKET_ADDRESS` | Deployed market |
| `USDC_ADDRESS` | Settlement token (MockUSDC locally) |
| `PLATFORM_SIGNER_KEY` | One platform key — signs all create/buy/sell/resolve |
| `PLATFORM_SIGNER_ADDRESS` | Must match key-derived address |

## Helpers

- `ChainConfig::from_env`
- `ChainClient::{create_market,buy,sell,resolve_market,...}`
- `deploy_local` / `mint_mock_usdc` for anvil tests

## Tests

```bash
cargo test -p chain
```

Anvil integration tests spawn `anvil` via alloy when on PATH; skip cleanly if missing.
