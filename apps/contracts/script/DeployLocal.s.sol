// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console2} from "forge-std/Script.sol";
import {MockUSDC} from "../src/mocks/MockUSDC.sol";
import {SubwayMarket} from "../src/SubwayMarket.sol";

/// @notice Local anvil deploy: MockUSDC + SubwayMarket.
/// @dev Platform defaults to anvil account #0; override via PLATFORM_SIGNER_ADDRESS.
///      Seeds platform with MockUSDC + MAX approve so createMarket subsidy works out of the box.
contract DeployLocal is Script {
    address constant ANVIL_ACCOUNT_0 = 0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266;
    /// @dev 1_000_000 USDC (6 decimals) — covers many LMSR open subsidies without re-mint.
    uint256 constant PLATFORM_SEED_USDC = 1_000_000 * 1e6;

    function run() external {
        address platform = vm.envOr("PLATFORM_SIGNER_ADDRESS", ANVIL_ACCOUNT_0);

        vm.startBroadcast();
        MockUSDC usdc = new MockUSDC();
        SubwayMarket market = new SubwayMarket(address(usdc), platform);
        // Fund platform for LMSR open subsidies. Approve from broadcast key (anvil #0 /
        // PLATFORM_SIGNER_KEY) — must match `platform` for createMarket transferFrom.
        usdc.mint(platform, PLATFORM_SEED_USDC);
        usdc.approve(address(market), type(uint256).max);
        vm.stopBroadcast();

        console2.log("USDC_ADDRESS=%s", address(usdc));
        console2.log("SUBWAY_MARKET_ADDRESS=%s", address(market));
        console2.log("PLATFORM_SIGNER_ADDRESS=%s", platform);
        console2.log("PLATFORM_SEEDED_USDC=%s", PLATFORM_SEED_USDC);
    }
}
