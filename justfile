set dotenv-load := true
set dotenv-override := true

export NATS_URL := env_var_or_default("NATS_URL", "nats://127.0.0.1:4223")
export DATABASE_URL := env_var_or_default("DATABASE_URL", "postgres://subway:subway@127.0.0.1:5433/subway_ledger")
export RUST_LOG := env_var_or_default("RUST_LOG", "info,async_nats=warn,diesel=warn")

ready_timeout := env_var_or_default("READY_TIMEOUT", "180")
bus_apps := "ledger domain backend ingest"

# Dev session:
#   just up             # once — docker + build + migrate
#   just start          # run bus apps + lnav (Ctrl-C / q stops apps)
#   just logs           # re-open lnav while apps already running
#   just observe        # TUI over ledger DB (separate from start)
#   just down           # tear down docker (keeps DB volume)
#   just reset          # wipe DB volume + logs, re-run up

default:
    @just --list

# ── session ──────────────────────────────────────────────────────────────────

up: compose-up build migrate-db
    @echo "up done — just start"

# Incremental rebuild if needed, run bus apps, then lnav in this terminal.
# Ctrl-C / quit lnav → stop apps only.
start: build
    #!/usr/bin/env bash
    set -euo pipefail

    cleanup() {
      pkill -f 'target/debug/(ledger|domain|backend|ingest)$' 2>/dev/null || true
      pkill -f 'cargo run -p (ledger|domain|backend|ingest)-app' 2>/dev/null || true
      rm -f logs/*.ready
    }
    trap cleanup EXIT INT TERM

    cleanup
    mkdir -p logs
    for app in {{ bus_apps }}; do
      bin="target/debug/${app}"
      if [[ ! -x "${bin}" ]]; then
        echo "missing ${bin} after build — just up" >&2
        exit 1
      fi
      : > "logs/${app}.log"
      READY_FILE="logs/${app}.ready" "${bin}" >> "logs/${app}.log" 2>&1 &
      just wait-ready "${app}"
    done
    echo "apps up — opening logs (q / Ctrl-C stops)"
    # Ghostty sets TERM=xterm-ghostty; lnav/notcurses needs a known terminfo entry.
    [[ "${TERM:-}" == *ghostty* ]] && export TERM=xterm-256color
    # Bus apps only — observe.log is separate (`just observe`) and must not flood this view.
    lnav -I lnav logs/ledger.log logs/domain.log logs/backend.log logs/ingest.log

# Re-open log viewer while apps already running (optional second terminal)
logs:
    #!/usr/bin/env bash
    set -euo pipefail
    [[ "${TERM:-}" == *ghostty* ]] && export TERM=xterm-256color
    exec lnav -I lnav logs/ledger.log logs/domain.log logs/backend.log logs/ingest.log

start-with-frontend: build
    #!/usr/bin/env bash
    set -euo pipefail

    cleanup() {
      pkill -f 'target/debug/(ledger|domain|backend|ingest|frontend)$' 2>/dev/null || true
      pkill -f 'cargo run -p (ledger|domain|backend|ingest|frontend)-app' 2>/dev/null || true
      rm -f logs/*.ready
    }
    trap cleanup EXIT INT TERM

    cleanup
    mkdir -p logs
    for app in {{ bus_apps }}; do
      bin="target/debug/${app}"
      if [[ ! -x "${bin}" ]]; then
        echo "missing ${bin} after build — just up" >&2
        exit 1
      fi
      : > "logs/${app}.log"
      READY_FILE="logs/${app}.ready" "${bin}" >> "logs/${app}.log" 2>&1 &
      just wait-ready "${app}"
    done
    : > "logs/frontend.log"
    READY_FILE="logs/frontend.ready" target/debug/frontend >> logs/frontend.log 2>&1 &
    just wait-ready frontend
    echo "apps + frontend up — opening logs (q / Ctrl-C stops)"
    [[ "${TERM:-}" == *ghostty* ]] && export TERM=xterm-256color
    lnav -I lnav logs/ledger.log logs/domain.log logs/backend.log logs/ingest.log logs/frontend.log

down:
    #!/usr/bin/env bash
    set -euo pipefail
    pkill -f 'target/debug/(ledger|domain|backend|ingest|frontend)$' 2>/dev/null || true
    pkill -f 'cargo run -p (ledger|domain|backend|ingest|frontend)-app' 2>/dev/null || true
    rm -f logs/*.ready
    docker compose down
    echo "down (DB volume kept)"

reset:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ "${CONFIRM:-}" != "yes" ]]; then
      echo "WARNING: Deletes Postgres volume and log files." >&2
      echo "Type 'reset' to confirm, or Ctrl-C to abort:" >&2
      read -r reply
      if [[ "$reply" != "reset" ]]; then
        echo "aborted (expected 'reset', got '${reply:-<empty>}')" >&2
        exit 1
      fi
    fi
    pkill -f 'target/debug/(ledger|domain|backend|ingest|frontend)$' 2>/dev/null || true
    rm -f logs/*.ready logs/*.log
    docker compose down -v
    just up
    echo "reset done — just start"

# ── building blocks ──────────────────────────────────────────────────────────

compose-up:
    docker compose up -d --wait

build:
    cargo build --workspace

migrate-db:
    cargo run -p ledger-app --bin ledger-migrate

wait-ready app:
    #!/usr/bin/env bash
    set -euo pipefail
    f=logs/{{ app }}.ready
    echo "wait ${f} (≤{{ ready_timeout }}s)"
    SECONDS=0
    until [[ -f "$f" ]]; do
      (( SECONDS < {{ ready_timeout }} )) || { echo "timeout: $f" >&2; tail -n 40 "logs/{{ app }}.log" 2>/dev/null >&2 || true; exit 1; }
      sleep 0.25
    done
    echo "ready {{ app }} (${SECONDS}s)"

run app:
    mkdir -p logs
    cargo run -p {{ app }}-app --bin {{ app }} 2>&1 | tee logs/{{ app }}.log

ingest:
    just run ingest

domain:
    just run domain

ledger:
    just run ledger

backend:
    just run backend

frontend:
    just run frontend

# Glance TUI over ledger Postgres (not on the bus; separate from `just start`)
observe:
    #!/usr/bin/env bash
    set -euo pipefail
    [[ "${TERM:-}" == *ghostty* ]] && export TERM=xterm-256color
    mkdir -p logs
    exec cargo run -p observe-app --bin observe 2>> logs/observe.log

check:
    cargo check --workspace

test:
    cargo test --workspace
