//! Early / late sides and resolution outcomes matching `SubwayMarket.sol`.

/// Binary LMSR side — matches Solidity `SubwayMarket.Side`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Side {
    Early = 0,
    Late = 1,
}

impl Side {
    /// Solidity / ABI encoding as `uint8`.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Parse from contract `uint8`.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Early),
            1 => Some(Self::Late),
            _ => None,
        }
    }
}

impl From<Side> for u8 {
    fn from(value: Side) -> Self {
        value.as_u8()
    }
}

/// Market resolution — matches Solidity `SubwayMarket.Outcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Outcome {
    Unresolved = 0,
    Early = 1,
    Late = 2,
}

impl Outcome {
    /// Solidity / ABI encoding as `uint8`.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    /// Parse from contract `uint8`.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Unresolved),
            1 => Some(Self::Early),
            2 => Some(Self::Late),
            _ => None,
        }
    }

    /// Outcomes allowed for `resolveMarket` (not `Unresolved`).
    pub const fn is_resolvable(self) -> bool {
        matches!(self, Self::Early | Self::Late)
    }
}

impl From<Outcome> for u8 {
    fn from(value: Outcome) -> Self {
        value.as_u8()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_matches_solidity_order() {
        assert_eq!(Side::Early.as_u8(), 0);
        assert_eq!(Side::Late.as_u8(), 1);
        assert_eq!(Side::from_u8(0), Some(Side::Early));
        assert_eq!(Side::from_u8(2), None);
    }

    #[test]
    fn outcome_matches_solidity_order() {
        assert_eq!(Outcome::Unresolved.as_u8(), 0);
        assert_eq!(Outcome::Early.as_u8(), 1);
        assert_eq!(Outcome::Late.as_u8(), 2);
        assert!(Outcome::Early.is_resolvable());
        assert!(!Outcome::Unresolved.is_resolvable());
    }
}
