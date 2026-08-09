//! Alloy `sol!` bindings from vendored Foundry artifacts in `crates/chain/abi/`.
//!
//! After changing contracts: `cd apps/contracts && forge build`, then copy the
//! JSON artifacts into `abi/` (see README).

use alloy::sol;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    SubwayMarket,
    concat!(env!("CARGO_MANIFEST_DIR"), "/abi/SubwayMarket.json")
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    MockUSDC,
    concat!(env!("CARGO_MANIFEST_DIR"), "/abi/MockUSDC.json")
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    IERC20,
    concat!(env!("CARGO_MANIFEST_DIR"), "/abi/IERC20.json")
);
