# subway-market — system and operations

How to run the stack locally: Docker infra, bus binaries, lnav, optional observe TUI.

Product/architecture: root [`README.md`](../README.md), lane rules in [`AGENTS.md`](../AGENTS.md).

## Components

| Component | What it is | How it runs |
|-----------|------------|-------------|
| **NATS** | Event broker | Docker Compose |
| **Postgres** | Ledger database (`subway_ledger`) | Docker Compose |
| **ledger** | `subway.>` → DB writer | bus app |
| **domain** | Ingest → freeze/diff | bus app |
| **backend** | HTTP API (agents + frontend) | bus app |
| **ingest** | MTA GTFS-RT poll → emit | bus app |
| **frontend** | HTMX UI | optional (`just start-with-frontend`) |
| **observe** | TUI over ledger DB | `just observe` — on demand, not in `just start` |

Postgres volume survives `just down`; `just reset` wipes it (type `reset`, or `CONFIRM=yes`).

## Environment

Justfile loads `.env` via `dotenv-load` + `dotenv-override` (`.env` wins over a stale shell `RUST_LOG`).

| Variable | Default | Used by |
|----------|---------|---------|
| `NATS_URL` | see `.env.example` | bus apps |
| `DATABASE_URL` | see `.env.example` | ledger, domain replay, backend reads, observe |
| `RUST_LOG` | `info,async_nats=warn,diesel=warn` | all Rust binaries |
| `LOG_FORMAT` | unset (= `json`) | `transport::init_tracing_*`; `pretty` / `text` for compact solo runs |
| `READY_FILE` | set by `just start` to `logs/<app>.ready` | ready gate |
| `READY_TIMEOUT` | `180` | `just wait-ready` |
| `PORT` | backend `8080`, frontend `3000` | HTTP apps |
| `BACKEND_URL` | `http://127.0.0.1:8080` | frontend |

Log levels: **info** for boot, `healthy`, connect/subscribe, freeze/diff outcomes (`detail` field); **warn** for unexpected gaps; **debug** for emit/persist echoes and per-event bus/DB.

## Session lifecycle

```bash
just up      # once — docker healthy, cargo build, migrate
just start   # bus apps + lnav in this terminal
# Ctrl-C or q in lnav → stops apps only (docker stays)
just logs    # re-open lnav while apps keep running
just observe # separate terminal — TUI over Postgres
just down    # stop apps + docker compose down (DB volume kept)
```

`just start` order: **ledger → domain → backend → ingest**. Each gets `READY_FILE=logs/<app>.ready`, stdout/stderr → `logs/<app>.log`, then `wait-ready`. After all ready, opens **lnav** with format dir `-I lnav` on those four logs.

## lnav

- Format pack: [`lnav/formats/installed/rust_tracing_log.json`](../lnav/formats/installed/rust_tracing_log.json)
- Bus apps default to JSON lines (`LOG_FORMAT` unset) so lnav can parse `level`, `timestamp`, `fields/message`, `fields/detail`, route/trip ids.
- Solo runs: `LOG_FORMAT=pretty just ingest` for human text in the terminal.

## Observe

`just observe` runs `observe-app` (not on NATS). Tracing goes to **stderr** / `logs/observe.log` so the TUI owns stdout. Needs `DATABASE_URL` and migrated schema (`just up`).
