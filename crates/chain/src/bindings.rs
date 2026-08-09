//! Alloy `sol!` bindings from Foundry artifacts under `apps/contracts/out/`.
//!
//! Run `cd apps/contracts && forge build` before compiling this crate so the
//! JSON artifacts exist (they are gitignored).

use alloy::sol;

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    SubwayMarket,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/contracts/out/SubwayMarket.sol/SubwayMarket.json"
    )
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    MockUSDC,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/contracts/out/MockUSDC.sol/MockUSDC.json"
    )
);

sol!(
    #[allow(missing_docs)]
    #[sol(rpc)]
    IERC20,
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/contracts/out/IERC20.sol/IERC20.json"
    )
);
