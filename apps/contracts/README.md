# Contracts (Foundry)

Solidity LMSR market contracts for subway-market. **Not** a Cargo workspace member.

## Contracts

| Contract | Role |
| -------- | ---- |
| `SubwayMarket` | Binary **early/late** LMSR market; USDC settlement; custodial platform signer |
| `LMSRMath` | Fixed-point LMSR cost / price helpers (WAD) |
| `IERC20` | Minimal settlement token interface |
| `MockUSDC` | 6-decimal deployable mock (`src/mocks/`) |

Flow: `createMarket` → `buy` / `sell` (platform signer, shares on `account`) → `resolveMarket` / `resolveMarketFromArrival` → `claim`.

Sides: **Early** / **Late** only. Resolution rule: actual arrival `<= frozenEtaUnix` → Early (exact-on-time = early); strictly after → Late. Winner shares redeem 1:1 USDC.

ABI JSON after build: `out/SubwayMarket.sol/SubwayMarket.json` (for `crates/chain` alloy bindings).

## Local dev

```bash
cd apps/contracts
forge build
forge test
```

## Monad / anvil

- Local: `anvil` on `http://127.0.0.1:8545` (set `MONAD_RPC_URL` in `.env` for remote Monad testnet).
- `foundry.toml` includes `monad` RPC placeholder — wire when chain lane adds alloy bindings in `crates/chain`.
- Deploy `MockUSDC` + `SubwayMarket(usdc, platformSigner)`. Platform seeds `b * ln(2)` USDC on each `createMarket`.

## Ownership

Chain lane owns contract logic + deploy scripts. Coordinate schema changes with `dto` event types.
