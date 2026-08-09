# Agent guide — subway-market

Working rules for agents changing this repo. Lane boundaries matter from day one.

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
2. **Transport roles:** ingest = producer (`emit`). Domain, ledger, backend = consumers with one `EventHandler` entry each (when wired).
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
- Caveman mode stays on (terse, no fluff) until the user says "stop caveman" or "normal mode".
- Port only the subway/MTA train-tracking path from `mta-btc` — not venue trading or other product surfaces.
- Do not create git commits unless the user explicitly asks.
- Do not install or upgrade software without explicit permission in the current message.

## Learned Workspace Facts

- Product: agentic prediction market on whether a randomly selected MTA train runs late; Rain.xyz funds agent bankrolls; settlement in USDC on Monad; humans set strategy and bankroll only.
- Architecture mirrors sibling repo `../mta-btc` (Cargo workspace, NATS, Justfile, lane isolation); port train-tracking from `mta-btc` via `cp` with minimal edits.
- `ingest` and `domain` lanes are largely `mta-btc` copies; `ledger` copies mta-btc train path only (`bus_events`, `tracked_trains`) — no trade tables or trade projections yet.
- Stack: Rust lanes (`backend`, `frontend`, `ingest`, `domain`, `ledger`), Diesel + Postgres, Foundry `apps/contracts` on Monad; shared crates are `dto`, `transport`, `db`, `chain`.
- Settled renames: `api` → `backend`, `web` → `frontend`, `bus` → `transport` (crate).
- Foundation spine is done; next work is lane ports starting with `dto` subjects/events, then parallel `ingest` / `domain` / `ledger`.
