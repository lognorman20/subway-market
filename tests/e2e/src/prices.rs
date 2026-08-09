//! WAD price helpers for LMSR assertions.

use anyhow::{bail, Context, Result};

use crate::outcome::HALF_WAD;

pub fn parse_wad(s: &str) -> Result<u128> {
    s.parse::<u128>()
        .with_context(|| format!("parse WAD '{s}'"))
}

/// Fresh market should be ~50/50 (exact HALF_WAD, or within `tol_bps` of half).
pub fn assert_near_half(early_wad: &str, late_wad: &str, tol_bps: u128) -> Result<()> {
    let early = parse_wad(early_wad)?;
    let late = parse_wad(late_wad)?;
    let half = parse_wad(HALF_WAD)?;
    let tol = half.saturating_mul(tol_bps) / 10_000;
    if early.abs_diff(half) > tol {
        bail!("early price {early_wad} not near 50% (tol {tol_bps} bps)");
    }
    if late.abs_diff(half) > tol {
        bail!("late price {late_wad} not near 50% (tol {tol_bps} bps)");
    }
    Ok(())
}

pub fn price_moved_up(before: &str, after: &str) -> Result<bool> {
    Ok(parse_wad(after)? > parse_wad(before)?)
}

pub fn price_moved_down(before: &str, after: &str) -> Result<bool> {
    Ok(parse_wad(after)? < parse_wad(before)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn half_is_half() {
        assert_near_half(HALF_WAD, HALF_WAD, 0).unwrap();
    }

    #[test]
    fn moved_up_down() {
        assert!(price_moved_up("100", "200").unwrap());
        assert!(price_moved_down("200", "100").unwrap());
        assert!(!price_moved_up("200", "100").unwrap());
    }
}
