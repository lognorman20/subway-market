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

# ── E2E harness (slice 11) ───────────────────────────────────────────────────

# Unit/harness pieces only (no full stack required).
e2e-unit:
    cargo check -p e2e
    cargo test -p e2e

# Full-stack suite. Prereq: just anvil → deploy-local → up → start (or start-with-frontend).
# E2E 1–2 need backend FUNDING_MODE=local. E2E 3 needs FUNDING_MODE=testnet + RAIN_*.
e2e: e2e-unit
    #!/usr/bin/env bash
    set -euo pipefail
    export E2E_FULL=1
    export E2E3_HARNESS_TRADES="${E2E3_HARNESS_TRADES:-1}"
    echo "E2E 1–2 → backend=${BACKEND_URL:-http://127.0.0.1:8080} (expect FUNDING_MODE=local)"
    cargo test -p e2e --test e2e1_lifecycle --test e2e2_human_trade -- --nocapture --test-threads=1
    echo "E2E 3 → expect FUNDING_MODE=testnet + RAIN_* (soft-skip on sandbox/network failures)"
    cargo test -p e2e --test e2e3_rain_agent -- --nocapture

# Composer-2.5 agent leg handoff (after E2E 3 wrote .e2e/agent-handoff.env).
# Parent orchestrator: spawn Task model=composer-2.5 with scripts/e2e-agent-prompt.md.
e2e-agent:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(git rev-parse --show-toplevel)"
    handoff="${root}/.e2e/agent-handoff.env"
    prompt="${root}/tests/e2e/scripts/e2e-agent-prompt.md"
    if [[ ! -f "${handoff}" ]]; then
      echo "missing ${handoff} — run just e2e (E2E 3) first, or set E2E3_HARNESS_TRADES=0 after fund" >&2
      exit 1
    fi
    echo "=== agent handoff ==="
    cat "${handoff}"
    echo ""
    echo "=== composer-2.5 prompt ==="
    echo "Spawn Task/subagent model=composer-2.5 with:"
    echo "  ${prompt}"
    echo "Load env from ${handoff}; read llms.txt; buy+sell via Bearer AGENT_API_KEY."

# ── chain / anvil ────────────────────────────────────────────────────────────

# Host anvil on 127.0.0.1:8545 (foreground; Ctrl-C stops).
anvil:
    anvil --host 127.0.0.1 --port 8545

# Deploy MockUSDC + SubwayMarket to local anvil. Prints env lines for .env paste.
# Requires: just anvil (separate terminal / background). Uses anvil account #0.
deploy-local:
    #!/usr/bin/env bash
    set -euo pipefail
    root="$(git rev-parse --show-toplevel)"
    rpc="${MONAD_RPC_URL:-http://127.0.0.1:8545}"
    # foundry.toml etherscan.monad expands this; empty is fine for local.
    export MONAD_ETHERSCAN_KEY="${MONAD_ETHERSCAN_KEY:-}"
    # Public anvil #0 — local/dev only. Never use on testnet/mainnet.
    anvil0_pk="0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
    pk="${PLATFORM_SIGNER_KEY:-$anvil0_pk}"
    # Reject truncated/invalid keys (common when copying old docs); fall back to anvil #0.
    if [[ ! "${pk}" =~ ^0x[0-9a-fA-F]{64}$ ]]; then
      echo "deploy-local: PLATFORM_SIGNER_KEY invalid (need 0x + 64 hex); using anvil #0" >&2
      pk="${anvil0_pk}"
    fi
    cd "${root}/apps/contracts"
    echo "deploy-local → ${rpc}"
    out="$(forge script script/DeployLocal.s.sol:DeployLocal \
      --rpc-url "${rpc}" \
      --private-key "${pk}" \
      --broadcast \
      -vv)"
    printf '%s\n' "${out}"
    usdc="$(printf '%s\n' "${out}" | sed -n 's/.*USDC_ADDRESS=\(0x[0-9a-fA-F]*\).*/\1/p' | tail -1)"
    market="$(printf '%s\n' "${out}" | sed -n 's/.*SUBWAY_MARKET_ADDRESS=\(0x[0-9a-fA-F]*\).*/\1/p' | tail -1)"
    if [[ -z "${usdc}" || -z "${market}" ]]; then
      echo "deploy-local: failed to parse USDC_ADDRESS / SUBWAY_MARKET_ADDRESS from forge output" >&2
      exit 1
    fi
    echo ""
    echo "# paste into .env (never commit .env):"
    echo "USDC_ADDRESS=${usdc}"
    echo "SUBWAY_MARKET_ADDRESS=${market}"
    echo "# DeployLocal also minted 1_000_000 MockUSDC to platform + approved SubwayMarket"
    env_file="${root}/.env"
    if [[ -f "${env_file}" ]]; then
      tmp="$(mktemp)"
      # Drop prior values then append fresh deploy addresses.
      grep -vE '^(USDC_ADDRESS|SUBWAY_MARKET_ADDRESS)=' "${env_file}" > "${tmp}" || true
      printf 'USDC_ADDRESS=%s\nSUBWAY_MARKET_ADDRESS=%s\n' "${usdc}" "${market}" >> "${tmp}"
      mv "${tmp}" "${env_file}"
      echo "updated ${env_file}"
    else
      echo "no .env at repo root — copy .env.example → .env then re-run, or paste lines above"
    fi
    # Verify seed (broadcast platform). Fail soft if cast/rpc hiccup — deploy already succeeded.
    platform="${PLATFORM_SIGNER_ADDRESS:-0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266}"
    if bal="$(cast call "${usdc}" "balanceOf(address)(uint256)" "${platform}" --rpc-url "${rpc}" 2>/dev/null)"; then
      echo "platform MockUSDC balance: ${bal}"
    fi

