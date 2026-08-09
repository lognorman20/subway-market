# E2E harness (slice 11)

Package name: `e2e`. Workspace member: `tests/e2e`.

## Commands

```bash
# Unit / harness pieces (no stack)
cargo check -p e2e && cargo test -p e2e

# Full stack
just anvil                 # host terminal / background
just deploy-local          # harness auto-mints MockUSDC to platform via anvil #1
just up                    # docker postgres+nats (or reuse host ports)
# start ledger+backend (FUNDING_MODE=local for E2E 1–2); avoid ingest/domain flood during harness
just e2e                   # E2E_FULL=1; runs 1 → 2 → 3

# Composer-2.5 agent leg (orchestrator parent after E2E 3 fund handoff)
just e2e-agent
```

## Env

| Var | Meaning |
|-----|---------|
| `E2E_FULL=1` | Enable full-stack integration tests |
| `BACKEND_URL` | Default `http://127.0.0.1:8080` |
| `NATS_URL` | Default `nats://127.0.0.1:4223` |
| `E2E_TIMEOUT_SECS` | Bounded waits (default 120) |
| `E2E3_HARNESS_TRADES` | Default `1` — harness reads `llms.txt` then buy/sell; set `0` to defer trades to `just e2e-agent` |
| `RAIN_*` | All required for E2E 3 live Rain; else skip + report |

Ready waits: ready-files / HTTP `/health` / NATS `market.opened` / poll predicates — **no arbitrary sleeps**.

## Scenarios

1. **E2E 1** — emit `open_requested` (≥3 stops) → ~50/50 → resolve exact → early
2. **E2E 2** — CLI wallet session → seed → buy/sell moves LMSR (platform signer)
3. **E2E 3** — Rain fund → agent Bearer trades + PnL; composer-2.5 via `just e2e-agent`

### E2E 3 Rain live skip

If Rain sandbox returns unexpected payloads / network errors (e.g. `Rain decode: …`), harness **soft-skips** after honest attempt, writes `.e2e/agent-handoff.env` when possible, and stays green. Re-run with healthy Rain sandbox + `FUNDING_MODE=testnet` backend for go-time full acceptance.

Handoff file: repo-root `.e2e/agent-handoff.env` (gitignored).
