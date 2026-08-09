//! Average-cost agent×market PnL (locked decision #7).
//!
//! Pure accounting — no DB. Ledger projections call these from bus facts only.

use dto::{MarketOutcome, MarketSide};

/// WAD fixed-point scale (1e18 = 100% probability / $1 face).
pub const WAD: u128 = 1_000_000_000_000_000_000;

/// Open share lot on one binary side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SidePos {
    /// Share quantity in micro-USDC (6 decimals).
    pub shares: i64,
    /// Net USDC paid for open shares (micro-units).
    pub cost_basis_usdc: i64,
}

/// Mutable agent×market PnL state before persistence.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PnlState {
    pub early: SidePos,
    pub late: SidePos,
    pub realized_pnl_usdc: i64,
    /// Canonical Early-side mark in WAD. Late mark = `WAD - early` (binary LMSR).
    pub last_mark_early_wad: Option<u128>,
    pub resolved: bool,
}

impl PnlState {
    pub fn cost_basis_usdc(&self) -> i64 {
        self.early
            .cost_basis_usdc
            .saturating_add(self.late.cost_basis_usdc)
    }

    pub fn early_shares(&self) -> i64 {
        self.early.shares
    }

    pub fn late_shares(&self) -> i64 {
        self.late.shares
    }

    /// Mark-to-market open shares minus cost basis. Zero after resolve.
    pub fn unrealized_pnl_usdc(&self) -> i64 {
        if self.resolved {
            return 0;
        }
        let Some(early_mark) = self.last_mark_early_wad else {
            return 0;
        };
        let late_mark = WAD.saturating_sub(early_mark);
        let early_mv = mark_value_usdc(self.early.shares, early_mark);
        let late_mv = mark_value_usdc(self.late.shares, late_mark);
        early_mv
            .saturating_add(late_mv)
            .saturating_sub(self.cost_basis_usdc())
    }

    pub fn total_pnl_usdc(&self) -> i64 {
        self.realized_pnl_usdc
            .saturating_add(self.unrealized_pnl_usdc())
    }

    pub fn side(&self, side: MarketSide) -> SidePos {
        match side {
            MarketSide::Early => self.early,
            MarketSide::Late => self.late,
        }
    }

    fn side_mut(&mut self, side: MarketSide) -> &mut SidePos {
        match side {
            MarketSide::Early => &mut self.early,
            MarketSide::Late => &mut self.late,
        }
    }
}

/// `shares * mark_wad / WAD` in micro-USDC.
pub fn mark_value_usdc(shares: i64, mark_wad: u128) -> i64 {
    let v = (i128::from(shares)).saturating_mul(mark_wad as i128) / (WAD as i128);
    i64::try_from(v).unwrap_or(if v.is_negative() { i64::MIN } else { i64::MAX })
}

fn set_early_mark(state: &mut PnlState, side: MarketSide, price_wad: u128) {
    let early = match side {
        MarketSide::Early => price_wad.min(WAD),
        MarketSide::Late => WAD.saturating_sub(price_wad.min(WAD)),
    };
    state.last_mark_early_wad = Some(early);
}

/// Buy fill: add shares + cost; refresh mark; unrealized moves.
pub fn apply_buy(
    state: &mut PnlState,
    side: MarketSide,
    shares: i64,
    usdc_amount: i64,
    price_wad: u128,
) {
    debug_assert!(!state.resolved, "buy after resolve");
    let pos = state.side_mut(side);
    pos.shares = pos.shares.saturating_add(shares);
    pos.cost_basis_usdc = pos.cost_basis_usdc.saturating_add(usdc_amount);
    set_early_mark(state, side, price_wad);
}

/// Sell fill: average-cost release → realized; shrink lot; refresh mark.
pub fn apply_sell(
    state: &mut PnlState,
    side: MarketSide,
    shares: i64,
    usdc_amount: i64,
    price_wad: u128,
) {
    debug_assert!(!state.resolved, "sell after resolve");
    let pos = state.side_mut(side);
    let shares_sold = shares.min(pos.shares.max(0));
    let cost_released = if pos.shares <= 0 {
        0
    } else {
        let raw = (i128::from(pos.cost_basis_usdc))
            .saturating_mul(i128::from(shares_sold))
            / i128::from(pos.shares);
        i64::try_from(raw).unwrap_or(i64::MAX)
    };
    let realized_delta = usdc_amount.saturating_sub(cost_released);
    pos.shares = pos.shares.saturating_sub(shares_sold);
    pos.cost_basis_usdc = pos.cost_basis_usdc.saturating_sub(cost_released);
    if pos.shares == 0 {
        pos.cost_basis_usdc = 0;
    }
    state.realized_pnl_usdc = state.realized_pnl_usdc.saturating_add(realized_delta);
    set_early_mark(state, side, price_wad);
}

/// Resolve: winning shares pay 1:1 micro-USDC; remaining cost settles into realized;
/// unrealized → 0; positions cleared.
pub fn apply_resolve(state: &mut PnlState, outcome: MarketOutcome) {
    if state.resolved {
        return;
    }
    let payout = match outcome {
        MarketOutcome::Early => state.early.shares.max(0),
        MarketOutcome::Late => state.late.shares.max(0),
        MarketOutcome::Unresolved => return,
    };
    let remaining_cost = state.cost_basis_usdc();
    state.realized_pnl_usdc = state
        .realized_pnl_usdc
        .saturating_add(payout.saturating_sub(remaining_cost));
    state.early = SidePos::default();
    state.late = SidePos::default();
    state.resolved = true;
}

/// Parse micro-USDC share quantity string.
pub fn parse_shares(s: &str) -> Result<i64, String> {
    s.parse::<i64>()
        .map_err(|e| format!("invalid shares '{s}': {e}"))
}

/// Parse WAD price string.
pub fn parse_wad(s: &str) -> Result<u128, String> {
    s.parse::<u128>()
        .map_err(|e| format!("invalid wad '{s}': {e}"))
}

pub fn shares_to_string(shares: i64) -> String {
    shares.to_string()
}

pub fn wad_to_string(wad: u128) -> String {
    wad.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HALF: u128 = WAD / 2;

    #[test]
    fn buy_fill_updates_cost_and_unrealized() {
        let mut s = PnlState::default();
        // Buy 1_000_000 early shares for 500_000 USDC at 0.5 mark.
        apply_buy(&mut s, MarketSide::Early, 1_000_000, 500_000, HALF);
        assert_eq!(s.early.shares, 1_000_000);
        assert_eq!(s.cost_basis_usdc(), 500_000);
        assert_eq!(s.realized_pnl_usdc, 0);
        // Mark value = 1e6 * 0.5 = 500_000 → unrealized 0 at entry.
        assert_eq!(s.unrealized_pnl_usdc(), 0);
        assert_eq!(s.total_pnl_usdc(), 0);

        // Mark moves to 0.6 via another buy of tiny size at new price.
        apply_buy(
            &mut s,
            MarketSide::Early,
            0,
            0,
            600_000_000_000_000_000,
        );
        // Force mark without shares: apply_buy with 0 shares still sets mark.
        assert_eq!(s.last_mark_early_wad, Some(600_000_000_000_000_000));
        assert_eq!(s.unrealized_pnl_usdc(), 100_000);
        assert_eq!(s.total_pnl_usdc(), 100_000);
        assert_eq!(s.realized_pnl_usdc, 0);
    }

    #[test]
    fn sell_fill_changes_realized() {
        let mut s = PnlState::default();
        apply_buy(&mut s, MarketSide::Early, 1_000_000, 500_000, HALF);
        // Sell half for 300_000 → cost released 250_000 → realized +50_000.
        apply_sell(&mut s, MarketSide::Early, 500_000, 300_000, HALF);
        assert_eq!(s.early.shares, 500_000);
        assert_eq!(s.early.cost_basis_usdc, 250_000);
        assert_eq!(s.realized_pnl_usdc, 50_000);
        assert_eq!(s.unrealized_pnl_usdc(), 0);
        assert_eq!(s.total_pnl_usdc(), 50_000);
    }

    #[test]
    fn resolve_zeroes_unrealized_and_absorbs_settlement() {
        let mut s = PnlState::default();
        apply_buy(&mut s, MarketSide::Early, 1_000_000, 400_000, HALF);
        // Mark at 0.5 → unrealized = 500_000 - 400_000 = 100_000.
        assert_eq!(s.unrealized_pnl_usdc(), 100_000);

        apply_resolve(&mut s, MarketOutcome::Early);
        // Payout 1_000_000 - cost 400_000 = +600_000 realized; unrealized 0.
        assert!(s.resolved);
        assert_eq!(s.early.shares, 0);
        assert_eq!(s.late.shares, 0);
        assert_eq!(s.cost_basis_usdc(), 0);
        assert_eq!(s.unrealized_pnl_usdc(), 0);
        assert_eq!(s.realized_pnl_usdc, 600_000);
        assert_eq!(s.total_pnl_usdc(), 600_000);
    }

    #[test]
    fn resolve_losing_side_writes_off_cost() {
        let mut s = PnlState::default();
        apply_buy(&mut s, MarketSide::Early, 1_000_000, 400_000, HALF);
        apply_resolve(&mut s, MarketOutcome::Late);
        // Payout 0 - cost 400_000.
        assert_eq!(s.realized_pnl_usdc, -400_000);
        assert_eq!(s.unrealized_pnl_usdc(), 0);
        assert_eq!(s.total_pnl_usdc(), -400_000);
    }

    #[test]
    fn late_price_converts_to_early_mark() {
        let mut s = PnlState::default();
        apply_buy(
            &mut s,
            MarketSide::Late,
            1_000_000,
            400_000,
            400_000_000_000_000_000,
        );
        // Late @ 0.4 → early mark 0.6.
        assert_eq!(s.last_mark_early_wad, Some(600_000_000_000_000_000));
        assert_eq!(s.late.shares, 1_000_000);
        assert_eq!(s.unrealized_pnl_usdc(), 0);
    }
}
