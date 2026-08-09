# Go-time orchestration — subway-market

**Single source of truth** for `/worktree-orchestrate` on the trading + funding slice. Foundation (train bus, thin backend/frontend, contract scaffold) is landed — see [`handoff.md`](handoff.md) and [`AGENTS.md`](../AGENTS.md).

**Acceptance bar:** [E2E 1](#e2e-1--market-lifecycle) + [E2E 2](#e2e-2--human-buysell-moves-prices) **required** (local/CI). [E2E 3](#e2e-3--rain-fund--agent-trades) **required for go-time done** only when all `RAIN_*` secrets are present in `.env`; else **skip E2E 3 + report** (do not block on missing Rain creds). Stack up for E2E: anvil, deploy, docker, lanes, frontend, `FUNDING_MODE` as appropriate.

**Orchestrator skill:** [`worktree-orchestrate`](/Users/logno/.cursor/skills/worktree-orchestrate/SKILL.md) — read it before dispatch.

---

## Orchestrator start here

**Announce:** `Using worktree-orchestrate — sequential slices → light review → merge into cheese.`

### Preflight (block dispatch until pass)

| # | Check | Fail action |
|---|-------|-------------|
| 1 | Read this file, [`AGENTS.md`](../AGENTS.md), [`worktree-orchestrate` SKILL](/Users/logno/.cursor/skills/worktree-orchestrate/SKILL.md) | Read first |
| 2 | Parent checkout on **`cheese`** — never implement on `main` | `git checkout cheese` |
| 3 | **This prompt committed on `cheese` tip** — orchestrator ensures latest `docs/go_time_prompt.md` (+ locked `AGENTS.md` / `.env.example` if changed) is on `cheese` before slice 1. If dirty/uncommitted, parent commits or merges doc locks onto `cheese` first (human may not have pre-committed). | Commit/merge docs → recite tip SHA |
| 4 | Record **`cheese` tip SHA** — every dispatch cites it; after each merge cite new tip | `git rev-parse HEAD` |
| 5 | No stale `feat/go-time-*` blocking merge (delete or merge abandoned branches) | Clean up |
| 6 | `.env` from `.env.example`; secrets never committed | Copy if missing |
| 7 | `forge`, `cast`, `cargo`, `just`, docker on PATH — **assume installed**; **no** `brew`/`npm`/`pip` install without human ask in current message | Report blocked; do not install |

### Hands-off / cook mode (orchestrator)

**Default:** agent cooks — **do not ask human** unless a hard stop below fires.

| Rule | Behavior |
|------|----------|
| **Decisions** | Pick **simplest default** from this file + [`AGENTS.md`](../AGENTS.md). Log choice in merge commit body (`Merge … — default: <what>`). |
| **Questions** | Forbidden mid-slice. Re-read locked [Decisions](#decisions-locked) + [Plain-English notes](#plain-english-notes-locked-defaults). |
| **Tools** | Assume `forge`, `cast`, `cargo`, `just`, docker already on PATH. Missing tool → **stop + report** (no self-install). |
| **Env** | Copy `.env.example` → `.env` if missing; use safe local defaults (`FUNDING_MODE=local`, anvil RPC, public anvil signer #0). Never commit `.env`. |
| **E2E** | E2E 1–2 **always** run at slice 11 / final smoke. E2E 3 Rain leg: run only if **all** `RAIN_TEAM_ID`, `RAIN_USER_ID`, `RAIN_API_KEY`, `RAIN_COLLATERAL_CONTRACT_ID` non-empty; else skip + report `E2E 3 skipped: RAIN_* missing`. |
| **Human wallet** | **CLI wallet = done bar** for local/CI human auth (E2E 2 / slice 11). Phantom browser connect = best-effort manual only (slice 8 smoke). |
| **Stop only when** | (1) missing secret orchestrator cannot invent (`RAIN_*` for E2E 3, prod keys), (2) merge conflict unresolvable after [protocol](#merge-conflict-protocol) one retry, (3) required tool not on PATH. |

### Dispatch order (locked — no skips)

**Canonical sequence — implement in this exact order:**

```
1 → 2 → 3 → 4 → 5 → 6 → 7 → (8 ∥ 9 ∥ 10) → 11
```

| # | Slice | Gate before next |
|---|-------|------------------|
| **1** | LMSR contract | `forge test` green; merged → `cheese` |
| **2** | Anvil + deploy ops | deploy-local doc'd; merged |
| **3** | Chain bindings | `cargo test -p chain`; merged |
| **4** | DTO + DB schema | migrate applies; merged |
| **5** | Ledger trade + PnL | `cargo test -p ledger`; merged |
| **6** | Backend core API | `cargo test -p backend`; merged |
| **7** | Auto market lifecycle | **MANDATORY before 8/9 E2E** — `cargo test -p domain -p backend`; merged |
| **8** | Frontend market UI | ∥ after **7** merges (may start after 6 if API stable) |
| **9** | Rain testnet path | ∥ after 6; required for E2E 3 when `RAIN_*` present |
| **10** | Observe trade views | ∥ after 5; non-blocking |
| **11** | E2E harness | **LAST** — needs 1–8 merged; 9 for E2E 3 Rain |

**Do not skip slice 7** for the 8/9 parallel window. Slice 7 blocks E2E 1.

| Phase | Rule |
|-------|------|
| **1–6** | Strictly sequential — shared contracts + schema |
| **7** | Run **immediately after 6** — blocks E2E 1 until merged. Do **not** skip 7 for the slice-8/9 parallel window |
| **8 ∥ 9 ∥ 10** | After **7** merges: frontend (8) and Rain (9) in parallel; observe (10) may already be in flight after **5** |
| **11** | **Last** — needs slices **1–8** merged; slice **9 required** for E2E 3 go-time acceptance |

### Loop reminder

`dispatch (composer-2.5)` → spot-check → **one** light review (`cursor-grok-4.5-high`) → fix once → merge → workspace done gate → next slice. No `just start` per slice. Full stack smoke **only** in slice 11 / final E2E.

### Slice 1 — first dispatch packet

Task: `subagent_type: best-of-n-runner`, `model: composer-2.5`, `run_in_background: true`. Replace `<cheese-tip-sha>` with output of `git rev-parse HEAD` on `cheese`.

```
# Implement SLICE 1 — LMSR contract

Stay in this worktree/branch. Ship code; don't spawn reviewers.

Plan: /Users/logno/Documents/work/github/subway-market/docs/go_time_prompt.md (Slice 1 section)
Repo: /Users/logno/Documents/work/github/subway-market
Integration branch (merge later): cheese
Feature branch: feat/go-time-01-lmsr
Start from tip: <cheese-tip-sha>

## Ownership (HARD)
Own: apps/contracts/src/**, apps/contracts/test/**, apps/contracts/foundry.toml, apps/contracts/README.md
Do NOT: crates/*, justfile, .env.example, other lanes
Avoid shared/ops unless this slice owns them.

## Done gate
cd apps/contracts && forge test

No just start / just up / full-stack smoke.

## Commits
feat(chain): …

## Final report
1. Branch + tip SHA
2. Commits
3. Implemented vs deferred
4. Shared-file touches
5. forge test results
6. Worktree path or "removed, branch retained"
```

Later slices: same skeleton from [Implementer prompt skeleton](#implementer-prompt-skeleton) + [`worktree-orchestrate/prompts.md`](/Users/logno/.cursor/skills/worktree-orchestrate/prompts.md); fill branch/slug/owns/done gate from slice table below.

---

## Orchestrator instructions

### Branch model

| Role | Where | Branch |
|------|-------|--------|
| **Parent orchestrator** | Main checkout | **`cheese`** (integration branch — see [Decisions](#decisions-locked)) — never implement here |
| **Implementers** | Git worktrees | `feat/go-time-NN-<slug>` per slice below |

After each slice: merge `feat/*` → **`cheese`**; cite new tip SHA; dispatch next slice from that tip.

### Models (hard allowlist)

| Work | Task `model` |
|------|--------------|
| Implement, fix, merge, post-merge build fix | `composer-2.5` |
| Light review only | `cursor-grok-4.5-high` |

Pass `model` explicitly. No other models. No multi-model review panels.

### Loop (per slice)

```
dispatch implementer (composer-2.5, best-of-n-runner, background)
  → spot-check branch / tip SHA / --stat
  → ONE light review (cursor-grok-4.5-high, read-only)
  → REQUEST_CHANGES → fix once on feat branch → re-light once
  → merge into integration
  → workspace done gate on touched packages
  → next slice (sequential default)
```

**Do not** default to deep review (thermo / interrogate / blast-radius / `ce-code-review`). **Do not** use `just start` as a slice done gate.

### Mode: sequential (default for go-time)

Shared contracts and schema land first — **one slice at a time** until slice 6 merges.

| After slice | Can parallel | Notes |
|-------------|--------------|-------|
| 5 (ledger) | 10 (observe) | Non-blocking; merge anytime after 5 |
| 6 (backend core) | 8 (frontend) ∥ 9 (Rain) | **Run slice 7 first** after 6 before treating parallel window as unblocked for E2E |
| 7 (lifecycle) | — | Sequential gate for E2E 1 — not parallel with 8/9 until merged |
| 8 + 9 + 10 | Each other | After 7 merges (and their deps): 8∥9∥10 ok |
| 11 (E2E harness) | — | **Last** — needs 1–8; slice 9 for E2E 3 Rain acceptance |

See [Dispatch order (locked)](#dispatch-order-locked) for the canonical `1→…→11` sequence.

### Merge conflict protocol

On `git merge` conflict during slice integration:

1. **`git merge --abort`** on parent `cheese` checkout — never leave conflicted tree.
2. On **feat branch**: `git fetch` + `git merge cheese` (or `git rebase cheese` if branch is linear).
3. **Fix only paths owned by this slice** (`Owns` column in slice table). Do not edit other lanes' files to "win".
4. **Shared path conflict** (`crates/dto`, `crates/transport`, `crates/db`, `crates/chain`, `justfile`, `docker-compose.yml`, `.env.example`, `apps/contracts`):
   - **Serialize:** if another slice owns the conflicting hunk, finish/merge that slice first, then retry.
   - **Both intents needed:** resolve keeping **both** behaviors (combine fields, both migrations, both subjects) — not pick-one deletion.
   - One implementer at a time on shared paths — never parallel-edit.
5. Re-run slice **done gate** + workspace check on feat branch.
6. Re-merge into `cheese`. **Do not push** unless human asks.
7. **Escalate to human** only if: same lines edited incompatibly by two slices **and** one retry (steps 2–6) failed to produce a coherent merge.

Aligns with [`worktree-orchestrate`](/Users/logno/.cursor/skills/worktree-orchestrate/SKILL.md): default abort+fix on feat branch; `fix-merge-conflicts` skill patterns if user explicitly said "resolve conflicts".

### Shared serialization (one agent at a time)

No parallel edits on:

- `crates/dto`
- `crates/transport`
- `crates/db` (migrations)
- `crates/chain`
- `justfile`, `docker-compose.yml`, `.env.example`
- `apps/contracts`

Slices that own shared paths **block** all other slices until merged.

### Implementer prompt skeleton

Use templates from [`worktree-orchestrate/prompts.md`](/Users/logno/.cursor/skills/worktree-orchestrate/prompts.md). Every dispatch must include: integration branch, `feat/<slug>`, start tip SHA, **Own / Do NOT**, done gate, commit style `feat(<lane>): …`.

### Commit style

`feat(<lane>): <imperative summary>` on feature branch. Lane = primary owner (`chain`, `ledger`, `backend`, `frontend`, `domain`, `observe`, or `ops` for justfile/deploy).

---

## Product (preserve)

Agentic prediction market: will a randomly selected MTA train run **early** or **late** to a named stop? **Binary only** — no on-time outcome. Humans set strategy and bankroll only. **Agents** trade via HTTP API (discovered from `llms.txt`). **Rain.xyz** on-ramps human-funded bankroll (ACH → USDC). **Settlement** is USDC on Monad (local: anvil + `MockUSDC`; testnet: Monad testnet + deployed addresses).

### Market resolution (binary only)

| Outcome | Rule |
|---------|------|
| **Early** | Arrival at target stop **on or before** `frozen_eta_unix` at freeze. **Exactly on schedule = early.** |
| **Late** | Arrival **strictly after** `frozen_eta_unix`. |

- No on-time outcome, void, grace window, or dead zone.
- On-chain `resolveMarket` accepts only `early` or `late`; winner → 1, loser → 0.
- Remove/lower any on-time threshold in domain freeze/arrival logic.

### Boundaries (do not blur)

| Actor | Owns |
|-------|------|
| Human | **Phantom wallet connect** on frontend (human identity/session); create agent; fund bankroll; paste `AGENT_API_KEY` into agent runtime |
| Agent runtime | LLM with tools — reads `llms.txt`, reasons, calls subway-market HTTP (`Authorization: Bearer AGENT_API_KEY`). **Not** a scripted HTTP bot. **Never** calls Rain. |
| subway-market platform | **Two auth paths:** (A) humans — Phantom wallet connect + backend session; (B) agents — Bearer `AGENT_API_KEY` + `llms.txt`. Buy/sell, bankroll ledger (Postgres), Monad tx via **platform signer** (humans/agents never sign LMSR txs), market lifecycle, Rain webhook/poll |
| Rain | Funding + spend rails only (sandbox). **Not** agent registration, **not** Monad signing, **not** buy/sell API |

Rain sandbox rails are Base/Ethereum/etc — **no Monad in sandbox**. Bridge or later rail for testnet; agents still trade subway-market HTTP only.

### Agent PnL per market

Per **agent × market** (USDC); ledger is source of truth; backend exposes; agents read own row via Bearer auth.

| Term | Meaning |
|------|---------|
| **Cost basis** | Net USDC paid for shares held — **average-cost** (locked in `dto`) |
| **Unrealized PnL** | Mark-to-market open shares minus cost basis (LMSR mid / last trade) |
| **Realized PnL** | Locked from sells + resolve payout minus settled cost basis |
| **Total PnL** | `realized + unrealized` while open; = realized after resolve |

Ledger sole writer to table **`agent_market_pnl`** (locked name). No second accounting path. Frontend/observe read projections only.

### Funding modes

One abstraction — flip via env, **no forked code paths**.

| `FUNDING_MODE` | Bankroll | Chain | Rain |
|----------------|----------|-------|------|
| `local` (default dev/CI) | `SEED_BANKROLL_USDC` / Postgres seed | anvil @ `MONAD_RPC_URL`, `MockUSDC` | ignored |
| `testnet` | Rain on-ramp + ledger | Monad testnet RPC + deployed addresses | sandbox `RAIN_*` |

Env knobs: `.env.example` — `FUNDING_MODE`, `SEED_BANKROLL_USDC`, `MONAD_RPC_URL`, `SUBWAY_MARKET_ADDRESS`, `USDC_ADDRESS`, `PLATFORM_SIGNER_KEY`, `PLATFORM_SIGNER_ADDRESS`, `RAIN_*` (never commit secrets).

### Share custody (hybrid)

| Layer | Role |
|-------|------|
| **On-chain** | LMSR share balances / settlement truth — `buy`/`sell`/`resolveMarket` via platform signer |
| **DB (ledger)** | Trade tape, positions, PnL projections, bankroll — sole writer = ledger lane |

Both stay in sync: backend submits chain tx, then emits bus events; ledger projects from bus. No DB-only share accounting without matching on-chain state.

### Agent onboarding

1. Human connects **Phantom wallet** on frontend (human identity/session — not for on-chain LMSR signing).
2. Human creates agent → receives `AGENT_API_KEY` (HTTP only — no agent chain keys).
3. Human funds: `local` = seed; `testnet` = Rain payment route.
4. Human pastes key + `llms.txt` URL into Claude Code–like agent runtime.
5. Agent discovers API, reasons, buy/sell via Bearer auth. Platform executes Monad + ledger.

Agents **never** call Rain.

### Auth (two identity paths — locked)

| Path | Who | Mechanism | Use |
|------|-----|-----------|-----|
| **A — Human** | Browser / frontend | **Phantom wallet connect** → backend session (wallet signature / SIWS-style validation — slice 8 + backend implement) | Human buy/sell on site; bankroll/PnL tied to connected wallet |
| **B — Agent** | Agent runtime (out-of-band) | `Authorization: Bearer AGENT_API_KEY` + `llms.txt` | Automated trading via HTTP API |

**Platform signer unchanged:** `backend`/`chain` still signs all `createMarket` / `buy` / `sell` / `resolveMarket` with `PLATFORM_SIGNER_KEY`. Phantom/CLI wallet = human **identity/auth** on the frontend, not browser-signed LMSR txs. Humans and agents may hit the same buy/sell routes; auth middleware distinguishes path A vs B.

**Verify #11 / E2E 2:** Monad is EVM — CLI keypair suffices for human identity. See [E2E 2](#e2e-2--human-buysell-moves-prices) and [Q11 verify](#plain-english-notes-locked-defaults).

### Frontend UI

Axum + HTMX + askama; HTTP to backend only — no contract addresses in frontend, **no browser-signed LMSR txs**, no NATS/DB. **Phantom wallet connect** required for human login/session before buy/sell. Short-horizon binary market UX: chart, 50/50 open, live price, buy/sell, trade tape. **No Polymarket naming** in code, routes, or copy.

### Local full-stack smoke order (integration only — not slice done gate)

1. `just anvil`
2. `just deploy-local`
3. `just up` (docker + migrate)
4. Seed bankroll or Rain fund per mode
5. `just start`
6. E2E 1 → E2E 2 → E2E 3

Docker = NATS + Postgres only. Anvil on host.

---

## Ordered implementation slices

Dispatch in order per [Decisions](#decisions-locked) below.

---

### Slice 1 — LMSR contract

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-01-lmsr` |
| **Owns** | `apps/contracts/src/**`, `apps/contracts/test/**`, `apps/contracts/foundry.toml`, `apps/contracts/README.md` |
| **Does NOT touch** | `crates/*`, `justfile`, `.env.example`, other lanes |
| **Depends on** | `cheese` tip at kickoff |

**Implement**

- Replace parimutuel `SubwayMarket` scaffold with **LMSR** ([how LMSR works](https://www.cultivatelabs.com/crowdsourced-forecasting-guide/how-does-logarithmic-market-scoring-rule-lmsr-work)).
- `createMarket` — liquidity param, share accounting, market id for train/trip/stop context.
- `resolveMarket` — **`early` or `late` only**; compare actual arrival to `frozen_eta_unix`; on-time or early → `early`, strictly after → `late`.
- `buy` / `sell` — LMSR price updates, share mint/burn; platform signer is on-chain caller (custodial).
- Rename sides from Yes/No to **early/late** in contract API where applicable.
- TDD: `forge test` covers create → buy/sell → resolve (exact-on-time → early) → claim/settle.

**Done gate**

```bash
cd apps/contracts && forge test
```

**Verify**

- Unit: happy path buy/sell before resolve; resolve sets 0/1 prices; **exact schedule time → early**.
- No Cargo changes in this slice.

**Commit** `feat(chain): …` (contracts lane)

---

### Slice 2 — Anvil + deploy ops

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-02-deploy-ops` |
| **Owns** | `justfile`, `.env.example`, `apps/contracts/script/**` (deploy script), `apps/contracts/README.md` (deploy section) |
| **Does NOT touch** | `crates/*`, lane apps, `SubwayMarket.sol` logic (unless deploy interface requires constructor args only) |
| **Depends on** | Slice 1 merged |

**Implement**

- `just anvil` — host anvil `127.0.0.1:8545`.
- `just deploy-local` — `forge script` deploy `MockUSDC` from `apps/contracts/src/` (or `src/mocks/` — **confirmed**, not `test/`-only) + LMSR `SubwayMarket` → write `SUBWAY_MARKET_ADDRESS`, `USDC_ADDRESS` into `.env` or stdout for paste.
- Extend `.env.example`: `MONAD_RPC_URL`, `SUBWAY_MARKET_ADDRESS`, `USDC_ADDRESS`, `PLATFORM_SIGNER_KEY`, `PLATFORM_SIGNER_ADDRESS` (placeholders only).
- Document smoke order in `apps/contracts/README.md`.

**Done gate**

```bash
# Manual or scripted: anvil up → deploy-local succeeds → forge test still passes
cd apps/contracts && forge test
```

**Verify**

- Deploy script runs against local anvil; addresses documented.
- Slice 1 tests still green.

**Commit** `feat(ops): …`

---

### Slice 3 — Chain bindings

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-03-chain-bindings` |
| **Owns** | `crates/chain/**` |
| **Does NOT touch** | `apps/contracts` (read ABI from `out/`), other lanes, `db` |
| **Depends on** | Slices 1–2 merged |

**Implement**

- Alloy bindings from `apps/contracts/out/SubwayMarket.sol/SubwayMarket.json` (+ IERC20/MockUSDC as needed).
- Tx helpers: `create_market`, `buy`, `sell`, `resolve_market` (early/late enum).
- Config from env: `MONAD_RPC_URL`, `SUBWAY_MARKET_ADDRESS`, `USDC_ADDRESS`, `PLATFORM_SIGNER_KEY`, `PLATFORM_SIGNER_ADDRESS`.
- Unit/integration tests against anvil where feasible without full stack.

**Done gate**

```bash
cargo check -p chain && cargo test -p chain
```

**Verify**

- Can connect anvil, read contract, submit test tx in `chain` tests (or documented manual script).

**Commit** `feat(chain): …`

---

### Slice 4 — DTO + DB schema

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-04-dto-db` |
| **Owns** | `crates/dto/**`, `crates/db/migrations/**`, `crates/db/src/**` (schema models only — no ledger handlers) |
| **Does NOT touch** | `crates/ledger`, `crates/backend`, lane handlers, **`crates/transport`** (bus subject strings live in `dto` only; transport crate is generic NATS wiring) |
| **Depends on** | Slice 3 merged (chain types inform IDs/enums) |

**Implement**

- `dto`: agents, markets, trades, positions, `agent_market_pnl` shapes; API request/response stubs; early/late enums; **average-cost** PnL fields; `EventMeta` on bus payloads; stable bus subjects (see [Decisions](#decisions-locked)).
- `db` migrations: tables for agents, markets, trades, positions, `agent_market_pnl`; serde-stable fields.
- Bus event types + subjects: lifecycle intents (`subway.market.open_requested`, `subway.market.resolve_requested`) and fill/open/resolve facts (`subway.trade.filled`, `subway.market.opened`, `subway.market.resolved`).

**Done gate**

```bash
cargo check -p dto -p db && cargo test -p dto -p db
```

**Verify**

- Migration applies cleanly: `just migrate-db` (or `cargo run -p ledger-app -- migrate` per repo convention).
- Diesel schema matches `dto` types.

**Commit** `feat(dto): …` / `feat(db): …` (one or two commits ok)

---

### Slice 5 — Ledger trade + PnL projections

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-05-ledger-trades` |
| **Owns** | `crates/ledger/**` (trade + PnL handlers), `apps/ledger/**` |
| **Does NOT touch** | `crates/backend`, `crates/frontend`, `crates/dto`/`db` except bugfix with orchestrator ok |
| **Depends on** | Slice 4 merged |

**Implement**

- Bus handlers: consume `subway.trade.filled`, `subway.market.opened`, `subway.market.resolved` (emitted by backend after on-chain success).
- `agent_market_pnl` projection: position, cost basis, realized, unrealized, total, last mark, resolved flag.
- Increment/recompute from same events as trades — no forked accounting.
- Resolve handler: unrealized → 0, realized absorbs settlement.

**Done gate**

```bash
cargo check -p ledger -p ledger-app && cargo test -p ledger
```

**Verify**

- Unit tests: fill → PnL update; sell → realized change; resolve → unrealized zeroed.

**Commit** `feat(ledger): …`

---

### Slice 6 — Backend core (auth, API, local funding)

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-06-backend-api` |
| **Owns** | `crates/backend/**`, `apps/backend/**` |
| **Does NOT touch** | `crates/ledger` writers, `crates/frontend`, `crates/domain` lifecycle, Rain client (slice 9) |
| **Depends on** | Slices 3–5 merged |

**Implement**

- `GET /llms.txt` — auth, buy/sell, markets, bankroll, PnL, errors.
- **Agent auth:** CRUD + `Authorization: Bearer AGENT_API_KEY`.
- **Human auth:** Phantom wallet connect → session (wallet signature validation; slice 8 owns connect UX, backend owns session issuance/validation).
- Buy/sell: validate bankroll, chain tx via `chain` (**platform signer** — neither humans nor agents hold chain keys), emit `subway.trade.filled` for ledger projection.
- Market create/resolve tx: submitted here when handling domain lifecycle intents (slice 7); emit `subway.market.opened` / `subway.market.resolved`.
- Markets list/detail: prices, history.
- PnL read API (agent own row; human sees owned agents).
- `FUNDING_MODE=local`: seed bankroll (`SEED_BANKROLL_USDC` / seed command) — no Rain.

**Done gate**

```bash
cargo check -p backend -p backend-app && cargo test -p backend
```

**Verify**

- HTTP unit/integration tests: auth reject/accept; buy/sell mocked or anvil-backed; PnL read returns projection shape.
- No `just start` required.

**Commit** `feat(backend): …`

---

### Slice 7 — Auto market lifecycle

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-07-market-lifecycle` |
| **Owns** | `crates/domain/**`, `apps/domain/**`, `crates/backend/**` (chain submit + bus emit), `crates/dto/**` (lifecycle subjects if not in slice 4) |
| **Does NOT touch** | `crates/frontend`, `crates/ledger` internals, contract source |
| **Depends on** | Slices 3, 5, 6 merged |

**Implement**

- **Domain** picks train on freeze/end (random **≥3 stops away**), decides open vs resolve from arrival vs `frozen_eta_unix`, emits `subway.market.open_requested` / `subway.market.resolve_requested`.
- **Backend** consumes intents → `createMarket` / `resolveMarket` on chain (platform signer) → emits `subway.market.opened` / `subway.market.resolved`; ledger records open/resolved market.
- Open binary market: early vs late to stop X; prices ~50/50.
- On arrival: compare actual time to `frozen_eta_unix` → early (≤, including exact) or late (>).
- Humans and agents trade the **same** market via backend HTTP.
- No duplicate orchestration — domain decides, backend executes chain + bus facts.

**Done gate**

```bash
cargo check -p domain -p domain-app -p backend -p backend-app && cargo test -p domain -p backend
```

**Verify**

- Unit/integration: freeze event → domain emits open_requested → backend creates market; arrival → resolve_requested → backend resolves early/late; no manual market creation in test.

**Commit** `feat(domain): …` and/or `feat(backend): …`

---

### Slice 8 — Frontend market UI

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-08-frontend-markets` |
| **Owns** | `crates/frontend/**`, `apps/frontend/**` |
| **Does NOT touch** | backend routes (consume only), chain, db, contracts |
| **Depends on** | Slice 6 merged (API shapes stable) |

**Implement**

- **Phantom wallet connect** UX — human must connect wallet before buy/sell; session sent to backend (not `AGENT_API_KEY`).
- Active/next market view: train context, early/late sides, ~50/50 open.
- Price chart, live odds, buy/sell controls (human session auth), trade tape (size, time, side, actor).
- Per-agent PnL on market view (realized / unrealized / total).
- Real HTTP to backend — not placeholder buttons.

**Done gate**

```bash
cargo check -p frontend -p frontend-app && cargo test -p frontend
```

**Verify**

- Template/handler tests.
- **Human auth (#11) — manual path:** create or import an EVM key via CLI (`cast wallet new`, `cast wallet import`, or anvil default key) → import private key into **Phantom** or **MetaMask** → open local frontend → connect wallet → complete backend session challenge → buy/sell controls enabled. Wallet is **human identity only** — LMSR txs still use platform signer.
- Full UI + session + price-move proof in [E2E 2](#e2e-2--human-buysell-moves-prices).

**Commit** `feat(frontend): …`

**Parallel:** can start after slice 6 merges (parallel with slice 7 if API stable; lifecycle E2E needs slice 7).

---

### Slice 9 — Rain testnet path

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-09-rain` |
| **Owns** | Rain client in `crates/backend/**` (or dedicated module), funding webhook/poll, frontend funding UX if needed |
| **Does NOT touch** | agent HTTP trade path, `chain` LMSR, ledger PnL math |
| **Depends on** | Slice 6 merged |
| **Required?** | **Yes** — E2E 3 acceptance requires Rain sandbox funding path (local seed still used for CI / E2E 1–2) |

**Implement**

- Rain client: [`rain-sandbox-trial.mintlify.site`](https://rain-sandbox-trial.mintlify.site/reference/rain-api), `Api-Key: $RAIN_API_KEY`.
- Payment route ACH→USDC on **Base** (Rain has no Monad rail). Destination = `RAIN_DESTINATION_ADDRESS` (compliance; not anvil #0). See [`raingentic-monad-builder-one-pager.pdf`](raingentic-monad-builder-one-pager.pdf) for Monad Circle USDC + faucet.
- Webhook or poll; credit Postgres bankroll; **interim treasury mirror** (MockUSDC mint on anvil / Circle faucet|CCTP on Monad testnet).
- `FUNDING_MODE=testnet` path only; `local` unchanged.
- Scoped virtual cards secondary — not main on-ramp.

**Done gate**

```bash
cargo check -p backend -p backend-app && cargo test -p backend
```

**Verify**

- Mock/sandbox tests for Rain client; manual sandbox fund optional.

**Commit** `feat(backend): …`

**Parallel:** with slice 8 after slice 6.

---

### Slice 10 — Observe trade views

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-10-observe-trades` |
| **Owns** | `crates/observe/**`, `apps/observe/**` |
| **Does NOT touch** | writers, backend, frontend |
| **Depends on** | Slice 5 merged (PnL projection exists) |

**Implement**

- Read-only TUI: trades, markets, agent×market PnL column.
- Postgres read via `db` — no NATS.

**Done gate**

```bash
cargo check -p observe -p observe-app && cargo test -p observe
```

**Verify**

- Unit tests for row formatting; optional manual `just observe`.

**Commit** `feat(observe): …`

**Parallel:** anytime after slice 5; non-blocking.

---

### Slice 11 — Full-stack E2E harness

| Field | Value |
|-------|-------|
| **Branch** | `feat/go-time-11-e2e-harness` |
| **Owns** | `tests/e2e/**` (create package), root `Cargo.toml` workspace member `tests/e2e` → package name **`e2e`**, harness docs in this file's E2E section |
| **Does NOT touch** | lane business logic except test fixtures |
| **Depends on** | Slices 1–8 merged; **slice 9 required** for E2E 3 Rain leg |

**Implement**

- Automated runners for E2E 1, 2, 3 (see below) under `tests/e2e`.
- E2E 3 agent leg: **composer-2.5** Task/subagent (`model: composer-2.5`), `AGENT_API_KEY`, `llms.txt` URL — not scripted trade payloads.
- CI default: `FUNDING_MODE=local` for E2E 1–2; E2E 3 Rain leg runs `FUNDING_MODE=testnet` (manual or scheduled — not optional for go-time done).

**Done gate**

```bash
# Harness package checks + documented e2e command passes locally
cargo check -p e2e && cargo test -p e2e
# Full stack: just anvil → deploy-local → up → start → tests/e2e suite
```

**Verify**

- All three E2E scenarios pass (criteria below).
- **E2E 2 human auth (#11) — automated path:** script creates EVM wallet via `cast wallet new` (or uses a known anvil/cast test key) → completes backend session challenge (sign message with `cast wallet sign` or alloy) → buy/sell against open market. Frontend connect may be stubbed in `tests/e2e` when `FUNDING_MODE=local` — session cookie/header from signed challenge is enough to prove human auth path; full browser Phantom connect is manual-only (see slice 8 verify).

**Commit** `feat(e2e): …`

---

## Review & verify protocol (orchestrator)

### Verify matrix (per slice + final)

| Slice | Done gate (required) | Spot-check | Full E2E |
|-------|----------------------|------------|----------|
| 1 | `cd apps/contracts && forge test` | ABI has early/late, LMSR | — |
| 2 | `forge test` + deploy-local manual | `.env.example` knobs | — |
| 3 | `cargo check -p chain && cargo test -p chain` | env config reads | — |
| 4 | `cargo check -p dto -p db && cargo test -p dto -p db` | `just migrate-db` | — |
| 5 | `cargo check -p ledger -p ledger-app && cargo test -p ledger` | PnL unit tests | — |
| 6 | `cargo check -p backend -p backend-app && cargo test -p backend` | auth + buy/sell mocked | — |
| 7 | `cargo check -p domain -p domain-app -p backend -p backend-app && cargo test -p domain -p backend` | lifecycle intent → chain | — |
| 8 | `cargo check -p frontend -p frontend-app && cargo test -p frontend` | Phantom manual optional | — |
| 9 | `cargo check -p backend -p backend-app && cargo test -p backend` | Rain mock tests | — |
| 10 | `cargo check -p observe -p observe-app && cargo test -p observe` | `just observe` optional | — |
| 11 | `cargo check -p e2e && cargo test -p e2e` | — | **E2E 1–2 required**; E2E 3 if `RAIN_*` set |

**Final smoke (slice 11 only — not per-slice gate):**

```bash
just anvil          # host terminal or background
just deploy-local   # fills SUBWAY_MARKET_ADDRESS, USDC_ADDRESS in .env
just up && just start-with-frontend   # or documented equivalent
# E2E 1 → E2E 2 → (E2E 3 if RAIN_* present)
cargo test -p e2e -- --nocapture   # or harness script from slice 11
```

| E2E | When | `FUNDING_MODE` | Pass criteria |
|-----|------|----------------|---------------|
| **1** | After 1,2,5,6,7,8 merged | `local` | Market opens ~50/50; resolve early/late; exact-on-time → early |
| **2** | After E2E 1 or open-market fixture | `local` | CLI wallet session → buy/sell moves LMSR prices; platform signer on-chain |
| **3** | `RAIN_*` all set + slice 9 merged | `testnet` | Rain fund → agent (composer-2.5 + `AGENT_API_KEY`) buy/sell; PnL updates |
| **3 skip** | Any `RAIN_*` empty | — | Report `E2E 3 skipped`; go-time partial (1–2 still required) |

**Human auth verify (locked):**

| Path | Done bar? | Steps |
|------|-----------|-------|
| **CLI wallet (local/CI)** | **Yes** — E2E 2 + slice 11 | `cast wallet new` or anvil #0 → sign backend session challenge → HTTP buy/sell |
| **Phantom browser** | Best-effort manual | Import CLI key → Phantom/MetaMask → frontend connect → session → buy/sell in browser |
| **LMSR txs** | N/A — not human wallet | Always `PLATFORM_SIGNER_KEY` |

### Light review rubric

One `cursor-grok-4.5-high` reviewer per slice. **Blocking only:**

| Check | Fail if |
|-------|---------|
| Ownership | Touched paths outside slice **Owns** |
| Done gate | No evidence of `cargo check`/`test` or `forge test` |
| Product rules | On-time outcome, Rain from agent, cross-lane deps |
| Shared files | Undeclared `dto`/`db`/`transport`/ops edits |
| Secrets | Committed keys or `.env` |

Return `APPROVE` or `REQUEST_CHANGES` with `path:line`. Fix once → re-light once → merge or escalate.

### When to run E2E

| Scenario | Run after | `FUNDING_MODE` |
|----------|-----------|----------------|
| **E2E 1** | Slices 1, 2, 5, 6, 7, 8 | `local` |
| **E2E 2** | E2E 1 fixture or slice 8 + open market | `local` |
| **E2E 3** (CI / local legs) | Slices 1–8 + harness | `local` (seed bankroll — E2E 1–2 style) |
| **E2E 3** (go-time acceptance) | Slices 1–9 + harness + **all `RAIN_*` in `.env`** | `testnet` — Rain sandbox |
| **E2E 3 skip** | Any `RAIN_*` empty | — — report skip; E2E 1–2 still required |

Do **not** run full E2E as per-slice done gate. Slice 11 owns repeatable E2E automation.

### E2E 1 — Market lifecycle

**Given:** train bus flowing; lifecycle wired; LMSR deployed.

1. Random train **≥3 stops** from target.
2. `createMarket` on chain; ledger records market.
3. Frontend shows active market (early/late, ~50/50).
4. Arrival → early or late per resolution rules → `resolveMarket`.

**Pass:** on-chain market exists; UI matches backend/chain; resolve never void/on-time; **exact schedule → early**; frontend shows winner/loser prices.

### E2E 2 — Human buy/sell moves prices

**Given:** open market; `FUNDING_MODE=local` + seeded bankroll; human identity via **wallet session** (not `AGENT_API_KEY`).

**Monad wallet = EVM wallet (locked):** Monad is EVM-compatible. A CLI wallet is a normal Ethereum keypair — e.g. `cast wallet new`, anvil default keys, or `cast wallet import`. No special Monad key format.

**Two verify paths (both valid for #11):**

| Path | When | Steps |
|------|------|-------|
| **(a) Manual** | Slice 8 smoke, local dev | CLI key → import into **Phantom** or **MetaMask** → connect on local frontend → backend session → buy/sell in browser |
| **(b) Automated** | `tests/e2e` / CI (`FUNDING_MODE=local`) | `cast wallet new` (or fixed anvil test key) → sign backend session challenge (`cast wallet sign` or alloy) → HTTP buy/sell with session; frontend connect may be stubbed |

**Scenario steps:**

1. Establish human session — manual: connect imported wallet on frontend; automated: sign challenge, hold session cookie/header.
2. Buy one side → that price **up**, other **down**.
3. Sell → side price **down**.
4. Buy other side → that price **up**.

**Pass:** ledger + on-chain custody update (**platform signer** — human wallet does **not** sign LMSR txs); API and UI prices move correctly (LMSR); human **session** auth works (wallet signature validated by backend); not placeholder UI. Production UX = Phantom connect in browser; local/CI may use CLI key + session challenge without full browser wallet extension.

### E2E 3 — Rain fund → agent trades

**Human:** create agent → fund via Rain sandbox (`FUNDING_MODE=testnet`) → paste `AGENT_API_KEY` + `llms.txt`.

**Agent:** composer-2.5 subagent reads `llms.txt`, reasons, buy + sell via Bearer auth — **not** Rain, **not** hardcoded scripts.

**Pass:** bankroll credited; buy + sell succeed; ledger projections match; on-chain/anvil custody matches; agent×market PnL updates (unrealized on trade; unrealized → 0 on resolve).

| Run | Funding |
|-----|---------|
| E2E 1–2 / routine CI | `local` — seed only — **always required** |
| **Go-time done (E2E 3)** | `testnet` + all `RAIN_*` set — Rain sandbox + agent assertions |
| **Go-time partial** | E2E 1–2 pass; E2E 3 skipped because `RAIN_*` missing — report to human |

Go-time **full** acceptance needs E2E 3 when Rain creds exist. Missing `RAIN_*` → skip E2E 3, ship E2E 1–2, report gap.

### Workspace verify (after each merge)

```bash
# Touched packages from slice
cargo check -p <libs> -p <apps> && cargo test -p <libs>

# After shared/contracts merges
just check   # or workspace equivalent
cd apps/contracts && forge test
```

Break on integration → fix on integration checkout (`composer-2.5`), not on merged feat branch.

### PnL verification checklist (E2E 3)

- Fill → position, cost basis, unrealized, total update; realized unchanged unless sell.
- Mark move → unrealized/total change; realized unchanged.
- Resolve → realized absorbs settlement; unrealized 0; total = realized.

---

## Decisions (locked)

Human answers locked — orchestrators implement; do not re-ask.

| # | Decision |
|---|----------|
| 1 | **Integration branch:** `cheese`. Merge `feat/*` → `cheese`, not `main` (unless told otherwise). |
| 2 | **Share custody:** **Hybrid** — on-chain LMSR shares/balances **and** DB ledger projections. On-chain = settlement truth; DB = PnL, tape, queries, observe. |
| 3 | **Trading keys:** **One platform signer** for all on-chain txs. Backend/`chain` signs all buy/sell/create/resolve. Humans authenticate via Phantom session; agents hold only `AGENT_API_KEY` for HTTP — neither holds chain keys. |
| 4 | **Lifecycle owner:** **Domain** picks train + decides open/resolve → emits bus intents. **Backend** submits `createMarket` / `resolveMarket` + emits fact events. Slice 7 = domain + backend + dto subjects. |
| 5 | **E2E 3 acceptance:** Rain sandbox **when `RAIN_*` present.** E2E 1–2 always required (local seed). Missing `RAIN_*` → skip E2E 3 + report; do not block E2E 1–2. |
| 6 | **MockUSDC (CONFIRMED):** Lives in `apps/contracts/src/` (or `src/mocks/`) — **confirmed.** `just deploy-local` must not rely on `test/`-only helpers. |
| 7 | **Cost basis:** **Average-cost** in `dto` / ledger. |
| 8 | **Env names:** `PLATFORM_SIGNER_KEY`, `PLATFORM_SIGNER_ADDRESS`, `SUBWAY_MARKET_ADDRESS`, `USDC_ADDRESS`, `AGENT_API_KEY` (agent HTTP only — backend issues; E2E/harness sets in env) (+ `MONAD_RPC_URL`, `FUNDING_MODE`, `RAIN_*`, `SEED_BANKROLL_USDC`). See `.env.example`. |
| 13 | **Bus subjects in `dto` only:** Lifecycle + trade subject strings (`subway.market.*`, `subway.trade.*`) defined in `crates/dto`. Do **not** edit `crates/transport` for new subjects — generic `emit`/`subscribe` only. |
| 9 | **E2E harness path:** `tests/e2e` (slice 11 owns). |
| 10 | **Bus vs HTTP (CONFIRMED):** Ledger sole DB writer; **projects from bus only** (no backend direct writes to trade/PnL tables). Consumes **NATS bus**. Backend emits `subway.trade.filled`, `subway.market.opened`, `subway.market.resolved` after on-chain success. Domain emits `subway.market.open_requested`, `subway.market.resolve_requested`. Agents/humans = HTTP to backend only — never bus. |
| 11 | **Human identity (CHANGED):** **Two auth paths.** Humans: **Phantom wallet connect** on frontend → backend session (wallet signature validation). Human trading identity = connected wallet session — **not** `AGENT_API_KEY`. Agents: unchanged — `Authorization: Bearer AGENT_API_KEY` + `llms.txt`. Same buy/sell routes allowed; auth middleware distinguishes human session vs agent Bearer. Platform signer still executes all chain txs. |
| 12 | **Slice 7 vs domain:** Resolved by #4 — lifecycle is **not** backend-only; domain owns train pick + open/resolve decisions. |

### Plain-English notes (locked defaults)

**Q6 — MockUSDC (CONFIRMED):** MockUSDC belongs in `apps/contracts/src/` (or `src/mocks/`), not `test/`-only. `just deploy-local` deploys it like a real contract. Local `USDC_ADDRESS` points at that deploy.

**Q10 — Bus vs HTTP (CONFIRMED):** Agents and humans never touch NATS. They call backend HTTP. Backend talks to chain, then publishes facts on the bus. Ledger **projects from bus only** — sole DB writer for trades, PnL, positions. Domain publishes lifecycle *requests*; backend turns them into chain tx + fact events.

**Q11 — Human identity (CHANGED):** Humans log in via **Phantom wallet connect** on the frontend (production UX). Backend validates wallet signature and issues a session for buy/sell. `AGENT_API_KEY` is **agent-only** — human pastes it into the agent runtime, not into the site for trading. Platform signer still submits all LMSR txs; Phantom/CLI wallet is **human identity only**, not on-chain trade signing.

**Q11 — How to verify (locked):**

1. **CLI wallet is enough for Monad:** EVM keypair (`cast wallet new`, anvil defaults, `cast wallet import`) — same as Ethereum.
2. **Manual verify (slice 8 / local dev):** generate or import CLI key → import into Phantom or MetaMask → connect on frontend → sign session challenge → buy/sell in browser.
3. **Automated verify (E2E 2 / slice 11):** `cast wallet new` → sign backend session challenge (`cast wallet sign` or alloy) → buy/sell over HTTP with session; stub frontend connect in `tests/e2e` when `FUNDING_MODE=local`.
4. **Not in scope for human wallet:** LMSR `buy`/`sell`/`createMarket`/`resolveMarket` — those stay on **platform signer** (`PLATFORM_SIGNER_KEY`).

---

## Non-blocking notes (do not re-ask)

| Item | Default |
|------|---------|
| Rain → Monad | **Authoritative Monad sheet:** [`docs/raingentic-monad-builder-one-pager.pdf`](raingentic-monad-builder-one-pager.pdf). Rain sandbox rails = Base/Eth/… (**no Monad**). Monad testnet USDC = Circle `0x534b2f3A21130d7a60830c2Df862319e593943A3` ([faucet.circle.com](https://faucet.circle.com/)). Real bridge later = Circle CCTP Base→Monad. Go-time Path C: ACH→Base (`RAIN_DESTINATION_ADDRESS`, not anvil #0) + Postgres bankroll + **interim** MockUSDC mint (anvil) / Circle faucet or CCTP (Monad testnet). |
| Scoped virtual cards (Rain) | Secondary; ACH payment route is primary on-ramp |
| `tests/e2e` package | Does not exist yet — slice 11 creates it + workspace member |
| `cheese` vs `main` | Same repo; **`cheese` is integration** — orchestrator checks out `cheese`, not `main` |

---

## Current gaps (pre-go-time)

| Area | State | Blocks |
|------|-------|--------|
| `SubwayMarket.sol` | Parimutuel — **LMSR rewrite** (slice 1) | E2E 1–3 |
| `just anvil` / `just deploy-local` | Not in justfile (slice 2) | E2E 1–3 |
| `crates/chain` | Placeholder (slice 3) | E2E 1–3 |
| Trade tables / PnL | Not started (slices 4–5) | E2E 2–3 |
| Backend trading API | `/health`, `/trains` only (slice 6) | E2E 2–3 |
| Auto lifecycle | Not wired (slice 7) | E2E 1 |
| Frontend markets | Trains only (slice 8) | E2E 1–2 |
| Rain client | Env stubs (slice 9 — **required**) | E2E 3 go-time acceptance |
| Composer-2.5 harness | Not landed (slice 11) | E2E 3 |

---

## References

- LMSR: https://www.cultivatelabs.com/crowdsourced-forecasting-guide/how-does-logarithmic-market-scoring-rule-lmsr-work
- Rain sandbox: https://rain-sandbox-trial.mintlify.site/reference/rain-api
- Lane rules: [`AGENTS.md`](../AGENTS.md)
- Env: [`.env.example`](../.env.example)
- Contracts: [`apps/contracts/README.md`](../apps/contracts/README.md)
- Orchestration skill: [`worktree-orchestrate`](/Users/logno/.cursor/skills/worktree-orchestrate/SKILL.md)
