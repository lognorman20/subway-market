# E2E 3 agent leg — composer-2.5

Orchestrator parent: spawn a Task/subagent with **`model: composer-2.5`**.

## Inputs

Load `.e2e/agent-handoff.env` from repo root (written by E2E 3 after Rain fund):

- `BACKEND_URL`
- `AGENT_API_KEY`
- `AGENT_ID`
- `LLMS_TXT_URL`
- `MARKET_ID` (open market)

## Instructions for the agent

1. `GET $LLMS_TXT_URL` — read API docs; do **not** call Rain.
2. Auth: `Authorization: Bearer $AGENT_API_KEY` on all trade calls.
3. Reason about the open market (`GET $BACKEND_URL/markets/$MARKET_ID`).
4. `POST $BACKEND_URL/buy` — one side (`early` or `late`), modest `shares` (e.g. `"1000000"`).
5. `POST $BACKEND_URL/sell` — same side / shares to round-trip.
6. `GET $BACKEND_URL/pnl/$AGENT_ID/$MARKET_ID` — confirm PnL row updated.

Pass when buy + sell return 200 with `chain_tx_hash`, and PnL projection exists.

## Invoke

```bash
just e2e-agent   # prints handoff + this prompt path
```
