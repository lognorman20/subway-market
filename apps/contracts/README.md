# Contracts (Foundry)

Solidity market contracts for subway-market. **Not** a Cargo workspace member.

## Contracts

| Contract | Role |
| -------- | ---- |
| `SubwayMarket` | Binary YES/NO market on train lateness; parimutuel USDC pool |
| `IERC20` | Minimal settlement token interface |
| `MockUSDC` | 6-decimal test token (`test/`) |

Flow: `createMarket` → `buy` / `sell` → `resolve` (resolver) → `claim` (winners).

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
- Deploy with a live USDC (or `MockUSDC` on anvil); pass token + resolver addresses to `SubwayMarket` constructor.

## Ownership

Chain lane owns contract logic + deploy scripts. Coordinate schema changes with `dto` event types.
