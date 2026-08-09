//! Market view models + formatting (WAD prices, micro-USDC).

use dto::{AgentMarketPnl, AgentSummary, Market, MarketSummary, Trade};

/// Half WAD = 50% — open-market baseline.
#[allow(dead_code)]
pub const HALF_WAD: &str = "500000000000000000";

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MarketListItem {
    pub market_id: String,
    pub route_id: String,
    pub trip_id: String,
    pub stop_id: String,
    pub outcome: String,
    pub outcome_label: String,
    pub is_open: bool,
    pub price_early_pct: String,
    pub price_late_pct: String,
    pub price_early_wad: String,
    pub price_late_wad: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct TradeTapeRow {
    pub trade_id: String,
    pub side: String,
    pub action: String,
    pub shares_label: String,
    pub usdc_label: String,
    pub price_pct: String,
    pub filled_at: String,
    pub actor_type: String,
    pub actor_label: String,
    /// 0–100 early-side price for chart (sell of late ≈ early complement not used; use trade price).
    pub chart_pct: f64,
}

#[derive(Debug, Clone)]
pub struct PnlRowView {
    pub agent_id: String,
    pub agent_label: String,
    pub realized_label: String,
    pub unrealized_label: String,
    pub total_label: String,
    pub early_shares_label: String,
    pub late_shares_label: String,
}

#[derive(Debug, Clone)]
pub struct AgentOption {
    pub agent_id: String,
    pub label: String,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MarketDetailView {
    pub market_id: String,
    pub route_id: String,
    pub trip_id: String,
    pub stop_id: String,
    pub train_id: String,
    pub frozen_eta_label: String,
    pub outcome: String,
    pub outcome_label: String,
    pub is_open: bool,
    pub price_early_pct: String,
    pub price_late_pct: String,
    pub price_early_wad: String,
    pub price_late_wad: String,
    pub opened_at: String,
    pub chart_points: String,
    pub chart_empty: bool,
    pub trades: Vec<TradeTapeRow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct MarketsPresentation {
    pub active: Option<MarketListItem>,
    pub open: Vec<MarketListItem>,
    pub resolved: Vec<MarketListItem>,
    pub banner: Option<(BannerKind, String)>,
}

impl From<&MarketSummary> for MarketListItem {
    fn from(row: &MarketSummary) -> Self {
        let is_open = row.outcome == "unresolved" || row.outcome.is_empty();
        Self {
            market_id: row.market_id.clone(),
            route_id: row.route_id.clone(),
            trip_id: row.trip_id.clone(),
            stop_id: row.stop_id.clone(),
            outcome: row.outcome.clone(),
            outcome_label: outcome_label(&row.outcome),
            is_open,
            price_early_pct: format_wad_percent(&row.price_early_wad),
            price_late_pct: format_wad_percent(&row.price_late_wad),
            price_early_wad: row.price_early_wad.clone(),
            price_late_wad: row.price_late_wad.clone(),
        }
    }
}

impl MarketsPresentation {
    pub fn from_summaries(mut markets: Vec<MarketSummary>) -> Self {
        // Prefer unresolved; within group keep API order (opened_at desc from backend).
        let mut open = Vec::new();
        let mut resolved = Vec::new();
        for m in markets.drain(..) {
            let item = MarketListItem::from(&m);
            if item.is_open {
                open.push(item);
            } else {
                resolved.push(item);
            }
        }
        let active = open.first().cloned();
        let banner = if open.is_empty() && resolved.is_empty() {
            Some((
                BannerKind::Info,
                "No markets yet. Open markets appear when a train freezes.".into(),
            ))
        } else {
            None
        };
        Self {
            active,
            open,
            resolved,
            banner,
        }
    }
}

pub fn market_detail_view(market: &Market, price_early_wad: &str, price_late_wad: &str, trades: &[Trade]) -> MarketDetailView {
    let outcome = market.outcome.as_str().to_string();
    let is_open = matches!(market.outcome, dto::MarketOutcome::Unresolved);
    let tape: Vec<TradeTapeRow> = trades.iter().map(TradeTapeRow::from).collect();
    let (chart_points, chart_empty) = chart_polyline(&tape);

    MarketDetailView {
        market_id: market.market_id.0.clone(),
        route_id: market.route_id.clone(),
        trip_id: market.trip_id.clone(),
        stop_id: market.stop_id.clone(),
        train_id: market
            .train_id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".into()),
        frozen_eta_label: market.frozen_eta_unix.to_string(),
        outcome: outcome.clone(),
        outcome_label: outcome_label(&outcome),
        is_open,
        price_early_pct: format_wad_percent(price_early_wad),
        price_late_pct: format_wad_percent(price_late_wad),
        price_early_wad: price_early_wad.to_string(),
        price_late_wad: price_late_wad.to_string(),
        opened_at: market.opened_at.to_rfc3339(),
        chart_points,
        chart_empty,
        trades: tape,
    }
}

impl From<&Trade> for TradeTapeRow {
    fn from(t: &Trade) -> Self {
        let agent = t.agent_id.0.clone();
        let short = short_id(&agent);
        let chart_pct = wad_to_percent_f64(&t.price_wad).unwrap_or(50.0);
        Self {
            trade_id: t.trade_id.0.clone(),
            side: t.side.as_str().to_string(),
            action: t.action.as_str().to_string(),
            shares_label: format_micro_usdc_shares(&t.shares),
            usdc_label: format_micro_usdc_i64(t.usdc_amount),
            price_pct: format_wad_percent(&t.price_wad),
            filled_at: t.filled_at.to_rfc3339(),
            actor_type: "agent".into(),
            actor_label: short,
            chart_pct,
        }
    }
}

pub fn pnl_rows_for_market(
    rows: &[AgentMarketPnl],
    market_id: &str,
    agents: &[AgentSummary],
) -> Vec<PnlRowView> {
    rows.iter()
        .filter(|r| r.market_id.0 == market_id)
        .map(|r| {
            let label = agents
                .iter()
                .find(|a| a.agent_id.0 == r.agent_id.0)
                .and_then(|a| a.display_name.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| short_id(&r.agent_id.0));
            PnlRowView {
                agent_id: r.agent_id.0.clone(),
                agent_label: label,
                realized_label: format_micro_usdc_i64(r.realized_pnl_usdc),
                unrealized_label: format_micro_usdc_i64(r.unrealized_pnl_usdc),
                total_label: format_micro_usdc_i64(r.total_pnl_usdc),
                early_shares_label: format_micro_usdc_shares(&r.early_shares),
                late_shares_label: format_micro_usdc_shares(&r.late_shares),
            }
        })
        .collect()
}

pub fn agent_options(agents: &[AgentSummary]) -> Vec<AgentOption> {
    agents
        .iter()
        .map(|a| {
            let label = a
                .display_name
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| short_id(&a.agent_id.0));
            AgentOption {
                agent_id: a.agent_id.0.clone(),
                label,
            }
        })
        .collect()
}

/// SVG polyline points (viewBox 0 0 100 40). Y inverted so higher % is up.
pub fn chart_polyline(trades: &[TradeTapeRow]) -> (String, bool) {
    // Trades arrive newest-first; chart left→right oldest→newest.
    let mut chronological: Vec<f64> = trades.iter().rev().map(|t| t.chart_pct).collect();
    if chronological.is_empty() {
        // Open baseline ~50/50.
        chronological.push(50.0);
        chronological.push(50.0);
        let pts = "0,20 100,20".to_string();
        return (pts, true);
    }
    if chronological.len() == 1 {
        chronological.push(chronological[0]);
    }
    let n = chronological.len();
    let mut parts = Vec::with_capacity(n);
    for (i, pct) in chronological.iter().enumerate() {
        let x = if n == 1 {
            0.0
        } else {
            (i as f64) * 100.0 / ((n - 1) as f64)
        };
        let y = 40.0 - (pct.clamp(0.0, 100.0) / 100.0 * 40.0);
        parts.push(format!("{x:.2},{y:.2}"));
    }
    (parts.join(" "), false)
}

pub fn format_wad_percent(wad: &str) -> String {
    match wad_to_percent_f64(wad) {
        Some(p) => format!("{p:.1}%"),
        None => "—".into(),
    }
}

pub fn wad_to_percent_f64(wad: &str) -> Option<f64> {
    let n: f64 = wad.parse().ok()?;
    Some(n / 1e18 * 100.0)
}

pub fn format_micro_usdc_i64(micro: i64) -> String {
    let sign = if micro < 0 { "-" } else { "" };
    let abs = micro.unsigned_abs();
    let whole = abs / 1_000_000;
    let frac = abs % 1_000_000;
    if frac == 0 {
        format!("{sign}{whole} USDC")
    } else {
        let mut s = format!("{sign}{whole}.{:06}", frac);
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        format!("{s} USDC")
    }
}

pub fn format_micro_usdc_shares(shares: &str) -> String {
    let Ok(n) = shares.parse::<i128>() else {
        return shares.to_string();
    };
    let sign = if n < 0 { "-" } else { "" };
    let abs = n.unsigned_abs();
    let whole = abs / 1_000_000;
    let frac = abs % 1_000_000;
    if frac == 0 {
        format!("{sign}{whole}")
    } else {
        let s = format!("{sign}{whole}.{:06}", frac);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn outcome_label(outcome: &str) -> String {
    match outcome {
        "early" => "Early".into(),
        "late" => "Late".into(),
        "unresolved" | "" => "Open".into(),
        other => other.to_string(),
    }
}

fn short_id(id: &str) -> String {
    let clean = id.trim();
    if clean.len() <= 8 {
        clean.to_string()
    } else {
        format!("{}…", &clean[..8])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wad_half_is_fifty_percent() {
        assert_eq!(format_wad_percent(HALF_WAD), "50.0%");
        assert!((wad_to_percent_f64(HALF_WAD).unwrap() - 50.0).abs() < 1e-9);
    }

    #[test]
    fn micro_usdc_formats() {
        assert_eq!(format_micro_usdc_i64(1_500_000), "1.5 USDC");
        assert_eq!(format_micro_usdc_i64(-250_000), "-0.25 USDC");
        assert_eq!(format_micro_usdc_shares("2000000"), "2");
    }

    #[test]
    fn open_market_becomes_active() {
        let markets = vec![
            MarketSummary {
                market_id: "2".into(),
                route_id: "A".into(),
                trip_id: "t2".into(),
                stop_id: "s2".into(),
                outcome: "early".into(),
                price_early_wad: HALF_WAD.into(),
                price_late_wad: HALF_WAD.into(),
            },
            MarketSummary {
                market_id: "1".into(),
                route_id: "B".into(),
                trip_id: "t1".into(),
                stop_id: "s1".into(),
                outcome: "unresolved".into(),
                price_early_wad: HALF_WAD.into(),
                price_late_wad: HALF_WAD.into(),
            },
        ];
        let p = MarketsPresentation::from_summaries(markets);
        assert_eq!(p.active.as_ref().map(|m| m.market_id.as_str()), Some("1"));
        assert_eq!(p.open.len(), 1);
        assert_eq!(p.resolved.len(), 1);
    }

    #[test]
    fn empty_chart_is_flat_fifty() {
        let (pts, empty) = chart_polyline(&[]);
        assert!(empty);
        assert!(pts.contains("20"));
    }
}
