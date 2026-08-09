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
    pub start_stop_id: String,
    pub start_stop_label: String,
    pub target_stop_label: String,
    pub start_time_label: String,
    pub target_time_label: String,
    pub journey_label: String,
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
    /// Traded-side marginal price 0–100 (from `price_wad`).
    pub chart_pct: f64,
}

/// Dual-series SVG chart: early + late option prices over fills.
#[derive(Debug, Clone)]
pub struct PriceChart {
    pub early_points: String,
    pub late_points: String,
    pub empty: bool,
    pub last_early_cx: String,
    pub last_early_cy: String,
    pub last_late_cx: String,
    pub last_late_cy: String,
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
    pub start_stop_id: String,
    pub start_stop_label: String,
    pub target_stop_label: String,
    pub start_time_label: String,
    pub target_time_label: String,
    pub journey_label: String,
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
    pub chart_early_points: String,
    pub chart_late_points: String,
    pub chart_empty: bool,
    pub chart_last_early_cx: String,
    pub chart_last_early_cy: String,
    pub chart_last_late_cx: String,
    pub chart_last_late_cy: String,
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
        let start_stop_label = crate::stops::format_stop_label(&row.start_stop_id);
        let target_stop_label = crate::stops::format_stop_label(&row.stop_id);
        let start_time_label = crate::stops::format_unix_et(row.start_eta_unix);
        let target_time_label = crate::stops::format_unix_et(row.frozen_eta_unix);
        let journey_label = format!("{start_stop_label} → {target_stop_label}");
        Self {
            market_id: row.market_id.clone(),
            route_id: row.route_id.clone(),
            trip_id: row.trip_id.clone(),
            stop_id: row.stop_id.clone(),
            start_stop_id: row.start_stop_id.clone(),
            start_stop_label,
            target_stop_label,
            start_time_label,
            target_time_label,
            journey_label,
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
                "No active market yet. Domain picks one random eligible train (≥3 stops ahead)."
                    .into(),
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
    let chart = price_chart(&tape, price_early_wad, price_late_wad);
    let start_stop_label = crate::stops::format_stop_label(&market.start_stop_id);
    let target_stop_label = crate::stops::format_stop_label(&market.stop_id);
    let start_time_label = crate::stops::format_unix_et(market.start_eta_unix);
    let target_time_label = crate::stops::format_unix_et(market.frozen_eta_unix);
    let journey_label = format!("{start_stop_label} → {target_stop_label}");

    MarketDetailView {
        market_id: market.market_id.0.clone(),
        route_id: market.route_id.clone(),
        trip_id: market.trip_id.clone(),
        stop_id: market.stop_id.clone(),
        start_stop_id: market.start_stop_id.clone(),
        start_stop_label,
        target_stop_label,
        start_time_label,
        target_time_label,
        journey_label,
        train_id: market
            .train_id
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "—".into()),
        frozen_eta_label: crate::stops::format_unix_et(market.frozen_eta_unix),
        outcome: outcome.clone(),
        outcome_label: outcome_label(&outcome),
        is_open,
        price_early_pct: format_wad_percent(price_early_wad),
        price_late_pct: format_wad_percent(price_late_wad),
        price_early_wad: price_early_wad.to_string(),
        price_late_wad: price_late_wad.to_string(),
        opened_at: market.opened_at.to_rfc3339(),
        chart_early_points: chart.early_points,
        chart_late_points: chart.late_points,
        chart_empty: chart.empty,
        chart_last_early_cx: chart.last_early_cx,
        chart_last_early_cy: chart.last_early_cy,
        chart_last_late_cx: chart.last_late_cx,
        chart_last_late_cy: chart.last_late_cy,
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

/// Plot geometry for dual early/late price series (viewBox 0 0 640 280).
const CHART_VB_W: f64 = 640.0;
const CHART_VB_H: f64 = 280.0;
const CHART_PAD_L: f64 = 52.0;
const CHART_PAD_R: f64 = 20.0;
const CHART_PAD_T: f64 = 18.0;
const CHART_PAD_B: f64 = 36.0;

fn chart_plot_size() -> (f64, f64) {
    (
        CHART_VB_W - CHART_PAD_L - CHART_PAD_R,
        CHART_VB_H - CHART_PAD_T - CHART_PAD_B,
    )
}

fn chart_xy(i: usize, n: usize, pct: f64) -> (f64, f64) {
    let (plot_w, plot_h) = chart_plot_size();
    let x = if n <= 1 {
        CHART_PAD_L
    } else {
        CHART_PAD_L + (i as f64) * plot_w / ((n - 1) as f64)
    };
    let y = CHART_PAD_T + plot_h * (1.0 - pct.clamp(0.0, 100.0) / 100.0);
    (x, y)
}

/// Build early + late price series from fills (newest-first tape) + live mark.
///
/// Each fill's `price_wad` is the traded side; the other side is the binary
/// complement (`100 - p`). Series starts at open 50/50, ends at live chain mark.
pub fn price_chart(
    trades: &[TradeTapeRow],
    price_early_wad: &str,
    price_late_wad: &str,
) -> PriceChart {
    let live_early = wad_to_percent_f64(price_early_wad).unwrap_or(50.0);
    let live_late = wad_to_percent_f64(price_late_wad).unwrap_or(100.0 - live_early);

    // Chronological: open baseline → each fill → live mark.
    let mut series: Vec<(f64, f64)> = vec![(50.0, 50.0)];
    for t in trades.iter().rev() {
        let p = t.chart_pct.clamp(0.0, 100.0);
        let (early, late) = if t.side == "late" {
            (100.0 - p, p)
        } else {
            (p, 100.0 - p)
        };
        series.push((early, late));
    }
    let empty = trades.is_empty();
    if let Some(&(e, l)) = series.last() {
        if (e - live_early).abs() > 0.05 || (l - live_late).abs() > 0.05 {
            series.push((live_early, live_late));
        } else {
            // Snap last point to live mark so endpoint matches odds cards.
            let last = series.len() - 1;
            series[last] = (live_early, live_late);
        }
    }
    // Need ≥2 points for a visible segment.
    if series.len() == 1 {
        series.push(series[0]);
    }

    let n = series.len();
    let mut early_parts = Vec::with_capacity(n);
    let mut late_parts = Vec::with_capacity(n);
    for (i, &(early, late)) in series.iter().enumerate() {
        let (xe, ye) = chart_xy(i, n, early);
        let (xl, yl) = chart_xy(i, n, late);
        early_parts.push(format!("{xe:.2},{ye:.2}"));
        late_parts.push(format!("{xl:.2},{yl:.2}"));
    }
    let (last_ex, last_ey) = chart_xy(n - 1, n, series[n - 1].0);
    let (last_lx, last_ly) = chart_xy(n - 1, n, series[n - 1].1);

    PriceChart {
        early_points: early_parts.join(" "),
        late_points: late_parts.join(" "),
        empty,
        last_early_cx: format!("{last_ex:.2}"),
        last_early_cy: format!("{last_ey:.2}"),
        last_late_cx: format!("{last_lx:.2}"),
        last_late_cy: format!("{last_ly:.2}"),
    }
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
                start_stop_id: "A01".into(),
                start_eta_unix: 0,
                stop_id: "s2".into(),
                frozen_eta_unix: 1_700_000_000,
                outcome: "early".into(),
                price_early_wad: HALF_WAD.into(),
                price_late_wad: HALF_WAD.into(),
            },
            MarketSummary {
                market_id: "1".into(),
                route_id: "B".into(),
                trip_id: "t1".into(),
                start_stop_id: "A01".into(),
                start_eta_unix: 0,
                stop_id: "s1".into(),
                frozen_eta_unix: 1_700_000_100,
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
        let chart = price_chart(&[], HALF_WAD, HALF_WAD);
        assert!(chart.empty);
        // Midline y for 50% in padded plot.
        let (_, mid_y) = chart_xy(0, 2, 50.0);
        assert!(chart.early_points.contains(&format!("{mid_y:.2}")));
        assert!(chart.late_points.contains(&format!("{mid_y:.2}")));
    }

    #[test]
    fn dual_series_mirrors_on_early_fill() {
        let tape = vec![TradeTapeRow {
            trade_id: "t1".into(),
            side: "early".into(),
            action: "buy".into(),
            shares_label: "1".into(),
            usdc_label: "1 USDC".into(),
            price_pct: "60.0%".into(),
            filled_at: "2026-01-01T00:00:00Z".into(),
            actor_type: "agent".into(),
            actor_label: "a".into(),
            chart_pct: 60.0,
        }];
        // Live mark matches last fill so series is open → fill.
        let early_wad = "600000000000000000";
        let late_wad = "400000000000000000";
        let chart = price_chart(&tape, early_wad, late_wad);
        assert!(!chart.empty);
        assert_ne!(chart.early_points, chart.late_points);
        // Last point early above 50%, late below.
        let last_ey: f64 = chart.last_early_cy.parse().unwrap();
        let last_ly: f64 = chart.last_late_cy.parse().unwrap();
        let (_, mid_y) = chart_xy(0, 2, 50.0);
        assert!(last_ey < mid_y, "early 60% should plot above mid");
        assert!(last_ly > mid_y, "late 40% should plot below mid");
    }
}
