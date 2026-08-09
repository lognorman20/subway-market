//! `GET /llms.txt` — agent runtime API docs.

use axum::response::IntoResponse;

pub async fn llms_txt() -> impl IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        LLMS_TXT,
    )
}

const LLMS_TXT: &str = r#"# subway-market agent API

Base URL: backend HTTP (default http://127.0.0.1:8080).
Quantities: micro-USDC (6 decimals). Prices: WAD (1e18 = 100%).

## Auth

### Agents (this document's audience)
Authorization: Bearer <AGENT_API_KEY>
Key prefix: smk_
Returned once at POST /agents (human session creates the agent).

### Humans
1. POST /auth/challenge {"address":"0x..."} → challenge_id + message
2. personal_sign message (cast wallet sign --offline / Phantom)
3. POST /auth/verify {"challenge_id","address","signature"} → session_token
4. Authorization: Bearer sms_<token>  OR  X-Session-Token: sms_<token>

Platform signer submits all LMSR txs. Humans/agents never sign buy/sell on chain.

## Agents
POST   /agents              human session; body {display_name?}; returns {agent_id, api_key} once
GET    /agents              human: owned agents; agent Bearer: self
GET    /agents/:id
DELETE /agents/:id          human owner only
GET    /agents/:id/bankroll
POST   /bankroll/seed       FUNDING_MODE=local only; human; {agent_id, amount_usdc?}
POST   /funding/rain        FUNDING_MODE=testnet; human; {agent_id, amount_usdc} → ACH onramp + sandbox simulate
GET    /funding/rain/:id    funding status
POST   /funding/rain/:id/poll  bounded poll until settled → bankroll credit
POST   /webhooks/rain       Rain transfer webhook → bankroll credit

## Markets
GET /markets
GET /markets/:id            includes price_early_wad, price_late_wad
GET /markets/:id/trades     history from trades table

## Trade
POST /buy   {market_id, side: early|late, shares, agent_id?}
POST /sell  {market_id, side, shares, agent_id?}
- Agent Bearer: trades as authenticated agent (agent_id optional/must match).
- Human session: agent_id required and must be owned.
- Validates bankroll, quotes via chain, platform signer executes, emits subway.trade.filled.
- Errors: insufficient_bankroll, unauthorized, etc. (JSON {error, code}).

## PnL
GET /pnl                         agent: own rows; human: owned agents' rows
GET /pnl/:agent_id/:market_id    projection shape from agent_market_pnl

## Errors
All non-2xx JSON: {"error":"...","code":"unauthorized|forbidden|bad_request|not_found|conflict|insufficient_bankroll|unavailable|internal"}

## Funding
FUNDING_MODE=local → SEED_BANKROLL_USDC (whole USDC) credited on agent create; POST /bankroll/seed resets.
FUNDING_MODE=testnet → Rain ACH→USDC on Base (POST /funding/rain + webhook/poll) → Postgres bankroll credit
  + interim treasury mirror (MockUSDC 1:1 to platform on Monad/anvil for LMSR). Rain has no Monad rail —
  mirror is NOT a real bridge. Agents trade this HTTP API only. Never call Rain from agent.
  Set RAIN_DESTINATION_ADDRESS (compliant Base addr; not anvil #0).

## Notes
- Binary market: early vs late only.
- Do not call Rain. Do not hold chain keys.
"#;
