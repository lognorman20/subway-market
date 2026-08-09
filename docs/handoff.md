# Handoff — subway-market

Where the next agent picks up after foundation.

## Status

Repo: `subway-market` (this tree).  
Reference: sibling `../mta-btc`.

**Foundation + train bus path + thin backend/frontend/contracts + observability are in place.** Extend; do not re-scaffold.

Do not re-scaffold the spine. Extend it.

## Product

Agentic prediction market: AI agents trade on whether a randomly selected MTA subway train runs late. Rain.xyz seeds agent capital; settlement is USDC on Monad. Humans set strategy and bankroll only. On-chain contracts handle trading/settlement; agents may later buy richer data or tips from other agents.

## Stack (settled)

| Piece | Choice |
| ----- | ------ |
| Monorepo | Cargo workspace (not Turborepo/Nest for backend) |
| Lanes | `ingest`, `domain`, `ledger`, `backend`, `frontend` |
| Contracts | Foundry under `apps/contracts` (not a Cargo member) |
| Shared crates | `dto`, `transport`, `db`, `chain` |
| Bus | NATS via `crates/transport` |
| DB | Postgres + Diesel; **ledger sole writer**; migrations in `crates/db` |
| Frontend | HTMX (axum + templates) |
| Renames | `api` → `backend`, `web` → `frontend`, `bus` → `transport` |

Lane isolation: no cross-lane crate deps. Shared surface = `dto` + `transport` (+ `db` reads). Serialize edits on `dto`, `transport`, `db`, `chain`, and ops files (`justfile`, `docker-compose.yml`, `.env.example`). See root [`AGENTS.md`](../AGENTS.md).

## App implementation briefs

**ingest** — Largely **cp from `../mta-btc`**: GTFS-RT poll/parse/emit for watchable routes, trip snapshots, vehicle updates. Producer only — `transport::emit` on `subway.ingest.*`, no freeze math, no DB. Keep subway/MTA signal path only; drop unrelated feed tasks from the source tree.

**domain** — Largely **cp from `../mta-btc`**: freeze/diff engine, RAM working set, cold-start replay from ledger. Consume ingest, emit freeze/diff. Wire subjects/types through `dto`/`transport`; no new rules until the port compiles.

**ledger** — Heavy **cp from `../mta-btc` ledger**, **trains only**: append `bus_events`, project **`tracked_trains`**. Diesel + `crates/db` migrations (sole writer). **No trade tables or trade projections.** New work = naming + Diesel/`db` wiring. Ignore mta-btc trade / market-resolution paths.

**backend** — HTTP for agents + frontend. Axum; request/response shapes in `dto`. Read Postgres via `db`; tx helpers via `chain`. May listen on the bus for live status. No schema writes, no MTA poll.

**frontend** — HTMX via axum + templates. Call `backend` over HTTP. No NATS, no DB writes, no browser signing. Humans set strategy/bankroll only.

**contracts** — Foundry Solidity on Monad: market create, buy/sell, USDC settle, oracle resolve. Local anvil/`forge`; ABIs → `crates/chain`. Not a Cargo member.

## Suggested next work

1. **Serial:** Fill `crates/dto` subjects/events for the train ingest → domain → ledger path (mirror mta-btc contracts shape; `mta.*` → `subway.*`). Shared-crate edits stay one agent at a time.
2. **Parallel (after dto stable):** port **ingest**, **domain**, and **ledger** (trains-only) from `mta-btc` into `crates/*` + thin `apps/*`. One agent per lane; do not touch other lanes' files.
3. Later: **backend**, **frontend**, **contracts** (may need another serial `dto` API stub pass first).
4. Full-stack smoke (`just up` / `just start`) only when someone asks for integration — not as routine lane sign-off. Lane done gate: `cargo check -p <lib> -p <lib>-app` (+ tests for that package).

## Constraints to honor

- Prefer **composer-2.5** for delegated/subagent implementation work unless the user names another model; do not silently swap models.
- Caveman mode for user chat until they say `stop caveman` or `normal mode`.
- Port only the subway/MTA train-tracking path from mta-btc — not venue trading or other product surfaces.
- No git commits unless the user explicitly asks.
- No software installs/upgrades without explicit permission in the current message.
- Prefer event-based waits for long background work (completion notifications / lifecycle events), not arbitrary sleep/poll loops.

## Ops reminder

```bash
just up      # docker + build + migrate
just start   # bus apps with ready-file gates
just check   # cargo check --workspace (no docker)
```

Copy `.env.example` → `.env` as needed. Details: root [`README.md`](../README.md).
