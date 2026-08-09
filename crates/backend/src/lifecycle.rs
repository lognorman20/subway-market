//! Slice 7 seam — domain lifecycle intents → chain create/resolve + bus facts.
//!
//! Not implemented in slice 6. Backend will subscribe to
//! `subway.market.open_requested` / `subway.market.resolve_requested`,
//! call `chain` create/resolve via platform signer, then emit
//! `subway.market.opened` / `subway.market.resolved`.

#![allow(dead_code)]

/// Placeholder so slice 7 has a clear module home.
pub fn lifecycle_ready() -> bool {
    false
}
