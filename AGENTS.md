# Agent guide — subway-market

Working rules for agents changing this repo. Lane boundaries matter from day one.

**Go-time orchestration:** parent agents running `/worktree-orchestrate` on LMSR/trading/funding start from [`docs/go_time_prompt.md`](docs/go_time_prompt.md) — **Orchestrator start here** section first. Checkout integration branch **`cheese`** (not `main`); merge slices into **`cheese`**.

## Lane ownership

Work in **one lane** per task. Do not edit other lanes' crates/apps except via shared `dto` / `transport` / `db` APIs.

| Lane | Lib | App |
|------|-----|-----|
| ingest | `crates/ingest` | `apps/ingest` (`ingest-app`) |
| domain | `crates/domain` | `apps/domain` (`domain-app`) |
| ledger | `crates/ledger` | `apps/ledger` (`ledger-app`) |
| backend | `crates/backend` | `apps/backend` (`backend-app`) |
| frontend | `crates/frontend` | `apps/frontend` (`frontend-app`) |
| observe | `crates/observe` | `apps/observe` (`observe-app`) — read-only DB, not on bus |
| chain | `crates/chain` | `apps/contracts` (Foundry, not Cargo) |

Lanes are isolated — build and test what you touch. Do not run `just start` / full-stack smoke as routine lane work.

## Shared (one agent at a time)

| Path | Notes |
|------|-------|
| `crates/dto` | Event types, IDs, subjects (`subway.*`), API stubs. Extend carefully; keep serde stable. |
| `crates/transport` | `connect`, `emit`, `EventHandler`, `run_handler`, `mark_ready`. NATS behind this. |
| `crates/db` | Diesel schema + migrations. **Ledger sole writer** to Postgres. |
| `crates/chain` | Monad / alloy bindings placeholder. |
| `justfile`, `docker-compose.yml`, `.env.example` | Ops. Change only if startup/migrate contract changes. |

Prefer not touching shared paths from lane work. If you must (new field, subject), **serialize** — no parallel edits on `dto`, `transport`, `db`, or ops files.

## Package names

| Kind | Names |
|------|-------|
| Lib crates | `dto`, `transport`, `db`, `chain`, `ingest`, `domain`, `ledger`, `backend`, `frontend`, `observe` |
| App crates | `*-app` (bin names without `-app`: `ingest`, `domain`, …) |
| Migrate | `just migrate-db` → `ledger-migrate` (ledger owns schema) |
| Solo run | `cargo run -p <name>-app` or `just <lane>` |

Workspace-wide: `just check`, `just test`.

## Hard rules

1. **No cross-lane crate deps** (`ingest` ↛ `domain` ↛ `ledger` ↛ `backend` ↛ `frontend`). Shared surface: `dto` + `transport` only. Lanes may **read** Postgres via `db` — never depend on another lane's lib.
2. **Transport roles:** ingest = producer (`emit`). Domain emits lifecycle intents. Backend emits trade/market facts after chain success. Ledger = consumer + sole DB writer (projects from bus only). Agents/humans never use bus — HTTP only. Human HTTP auth = Phantom session; agent HTTP auth = Bearer `AGENT_API_KEY`.
3. **IDs:** every bus payload has `event_id` ([`dto::EventMeta`](crates/dto/src/ids.rs)).
4. **Ready gate:** `READY_FILE=logs/<app>.ready`. Consumers: `transport::mark_ready` after subscribe (`run_handler` does this). Ingest: after NATS connect.
5. **Ledger** is the sole DB writer; owns migrations in `crates/db/migrations/`.
6. **Do not run `just start`** as part of routine lane work.
7. **One agent at a time** on `dto`, `transport`, `db`, `chain`, and ops files.

## Done gate

Before claiming done, touched packages must compile:

```bash
cargo check -p <lib> -p <lib>-app
cargo test -p <lib>
```

No `just up` / `just start` / NATS stack required for sign-off unless your change needs integration smoke.

## Learned User Preferences

- Orca/subagent workers: use `--agent cursor --model composer-2.5` unless the user names another model; do not substitute Claude, Codex, or Grok without asking.
- Orca coordination: use event-based `orca orchestration check --wait` for `worker_done` / escalation / question — not arbitrary sleep or poll loops.
- Human's agent = Claude Code–like LLM runtime (tool use, reads `llms.txt`, reasons over trades) — not a scripted HTTP bot.
- Agent integration tests: E2E harness at `tests/e2e` (slice 11). E2E 1–2 / CI use `FUNDING_MODE=local` + seed bankroll. **Go-time done** requires E2E 3 with Rain sandbox (`FUNDING_MODE=testnet`, slice 9). Agent leg = composer-2.5 subagent + `AGENT_API_KEY` + `llms.txt`.
- Funding modes: one funding abstraction; flip via `FUNDING_MODE` (`local` | `testnet`) + env — never fork code paths. Local/dev = fake money only. Deploy/staging = same binary, `FUNDING_MODE=testnet` + Rain sandbox + Monad testnet.
- Caveman mode stays on (terse, no fluff) until the user says "stop caveman" or "normal mode".
- Port only the subway/MTA train-tracking path from `mta-btc` — not venue trading or other product surfaces; no Polymarket naming in implementation.
- Frontend stack: Axum + HTMX + askama; frontend lane calls backend HTTP only (no Diesel/NATS in frontend).
- Do not create git commits unless the user explicitly asks.
- Do not install or upgrade software without explicit permission in the current message.

## Learned Workspace Facts

- Product: agentic prediction market on whether a randomly selected MTA train is **early or late** (binary only — no on-time); Rain.xyz funds agent bankrolls; settlement in USDC on Monad; humans set strategy and bankroll only.
- Market resolution: **early** or **late** only; arrival ≤ scheduled (`frozen_eta_unix`) → early (exact-on-time counts as early); strictly after → late; no on-time outcome or dead zone.
- Auth (two paths): **Humans** — Phantom wallet connect on frontend → backend session (identity/auth; not LMSR tx signing). **Agents** — `Authorization: Bearer AGENT_API_KEY` + `llms.txt` HTTP only. Platform (`backend` + `chain`) signs all Monad txs with **one platform signer** (`PLATFORM_SIGNER_KEY`); humans/agents never sign LMSR txs. **E2E 2 / #11 verify:** Monad = EVM — CLI key (`cast wallet new` / anvil) is valid human identity; manual = import key into Phantom/MetaMask + frontend connect; automated = sign session challenge in `tests/e2e` (frontend connect stub ok when `FUNDING_MODE=local`). See `docs/go_time_prompt.md` E2E 2 + Q11 verify.
- Agent boundary: Rain on-ramps human-funded bankroll (ACH→USDC); agents trade only via subway-market HTTP API + `llms.txt` — not Rain directly. Hybrid custody: on-chain LMSR shares + DB ledger projections (ledger projects from bus only).
- Funding mode matrix (`FUNDING_MODE` in `.env`): **`local`** — seeded test bankroll (Postgres), `MockUSDC`/anvil at `MONAD_RPC_URL` (default `http://127.0.0.1:8545`), `RAIN_*` ignored, no ACH. **`testnet`** — Rain sandbox (`RAIN_API_BASE_URL`, `RAIN_*` creds) for human on-ramp, Monad testnet via `MONAD_RPC_URL`; agents still trade subway-market HTTP only.
- Agent E2E harness: `tests/e2e` — local stack + seed for E2E 1–2; Rain sandbox path required for E2E 3 acceptance.
- Per-agent×market PnL (realized + unrealized while open) is ledger-owned; backend exposes via API/`llms.txt` — see `docs/go_time_prompt.md`.
- Architecture mirrors sibling repo `../mta-btc` (Cargo workspace, NATS, Justfile, lane isolation); port train-tracking from `mta-btc` via `cp` with minimal edits; `domain` stays separate from `ingest` (replay/isolation pattern).
- `ingest` and `domain` lanes are largely `mta-btc` copies; `ledger` copies mta-btc train path only (`bus_events`, `tracked_trains`) — no trade tables or trade projections yet.
- Stack: Rust lanes (`backend`, `frontend`, `ingest`, `domain`, `ledger`, `observe`), Diesel + Postgres, Foundry `apps/contracts` on Monad; shared crates are `dto`, `transport`, `db`, `chain`.
- Settled renames: `api` → `backend`, `web` → `frontend`, `bus` → `transport` (crate).
- Train bus path plus backend (`/health`, `/trains`), frontend (HTMX), observe TUI, and lean Foundry `SubwayMarket` scaffold are landed; `chain` lane, Rain wallets, and LMSR markets are next (`docs/go_time_prompt.md`).
- Local docker-compose host ports: Postgres `5433`, NATS client `4223`, monitor `8223` (justfile / `.env.example` defaults).
- `observe` lane: read-only Postgres TUI (`just observe`), not on NATS bus.
