# Chain crate

Alloy bindings + tx helpers for the LMSR `SubwayMarket` (and MockUSDC/IERC20).

## ABI artifacts (vendored)

`sol!` loads committed JSON under [`abi/`](abi/):

- `abi/SubwayMarket.json`
- `abi/MockUSDC.json`
- `abi/IERC20.json`

`cargo check -p chain` works without `apps/contracts/out/` (gitignored).

### Refresh after contract changes

```bash
cd apps/contracts && forge build
cp out/SubwayMarket.sol/SubwayMarket.json ../../crates/chain/abi/SubwayMarket.json
cp out/MockUSDC.sol/MockUSDC.json ../../crates/chain/abi/MockUSDC.json
cp out/IERC20.sol/IERC20.json ../../crates/chain/abi/IERC20.json
```

Commit the updated `abi/*.json` with the chain change.

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

Anvil integration tests spawn `anvil` via alloy when on PATH; skip cleanly if missing. Deploy uses bytecode from the vendored Foundry JSON (no `out/` required at test time).
