// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {LMSRMath} from "../src/LMSRMath.sol";

/// @dev Direct math checks (empty market cost ≈ b ln2, prices move).
contract LMSRMathTest {
    uint256 internal constant ONE = 1_000_000;
    uint256 internal constant WAD = 1e18;

    function test_requiredSubsidy_isBLn2() public pure {
        uint256 b = 100 * ONE;
        uint256 sub = LMSRMath.requiredSubsidy(b);
        // b * ln(2) ≈ 69.314718 USDC
        require(sub > 69 * ONE, "sub lo");
        require(sub < 70 * ONE, "sub hi");
    }

    function test_emptyMarket_priceIsHalf() public pure {
        uint256 b = 50 * ONE;
        uint256 p = LMSRMath.priceEarly(0, 0, b);
        require(p > WAD / 2 - WAD / 1000, "lo");
        require(p < WAD / 2 + WAD / 1000, "hi");
    }

    function test_buyCost_positiveAndIncreasesWithSize() public pure {
        uint256 b = 100 * ONE;
        uint256 c1 = LMSRMath.buyCost(0, 0, b, true, 1 * ONE);
        uint256 c10 = LMSRMath.buyCost(0, 0, b, true, 10 * ONE);
        require(c1 > 0, "c1");
        require(c10 > c1, "convex");
        // First share near 0.5 USDC
        require(c1 > 400_000 && c1 < 600_000, "near half");
    }

    function test_sellRoundTrip_lossyToMarket() public pure {
        uint256 b = 100 * ONE;
        uint256 shares = 5 * ONE;
        uint256 cost = LMSRMath.buyCost(0, 0, b, true, shares);
        uint256 proceeds = LMSRMath.sellProceeds(shares, 0, b, true, shares);
        // Ceil buy / floor sell → proceeds <= cost
        require(proceeds <= cost, "round trip");
    }
}
