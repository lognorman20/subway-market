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

## Deploy / smoke (local anvil)

Order from repo root (do **not** need full stack / `just start`):

1. **Anvil** (foreground or background):

   ```bash
   just anvil
   # → anvil --host 127.0.0.1 --port 8545
   ```

2. **Deploy** MockUSDC + SubwayMarket (separate terminal):

   ```bash
   just deploy-local
   ```

   Uses `MONAD_RPC_URL` (default `http://127.0.0.1:8545`) and `PLATFORM_SIGNER_KEY` (default public anvil #0). Prints:

   ```text
   USDC_ADDRESS=0x...
   SUBWAY_MARKET_ADDRESS=0x...
   ```

   Paste into `.env` (or let the recipe update an existing `.env`). Never commit `.env`.

   Also mints **1_000_000 MockUSDC** to the platform and (when broadcast key == platform) approves `SubwayMarket` MAX — so live `createMarket` / FE opens work without a separate mint step.

3. **Tests** (no anvil required):

   ```bash
   cd apps/contracts && forge test
   ```

Constructor wiring: `SubwayMarket(usdc, platformSigner)` — platform = broadcast sender (anvil #0). Each `createMarket` pulls `b * ln(2)` USDC subsidy from the platform via `transferFrom`.

## Monad / anvil

- Local: `just anvil` → `http://127.0.0.1:8545` (`MONAD_RPC_URL` in `.env`).
- Testnet: set `MONAD_RPC_URL` to Monad testnet RPC; deploy addresses manually into `.env`.
- `foundry.toml` includes `monad` RPC placeholder — chain lane binds via alloy in `crates/chain`.

## Ownership

Ops slice owns `just anvil` / `just deploy-local` + this deploy section. Chain lane owns contract logic. Coordinate schema changes with `dto` event types.
