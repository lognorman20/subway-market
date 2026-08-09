// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {SubwayMarket} from "../src/SubwayMarket.sol";
import {MockUSDC} from "./MockUSDC.sol";
import {IERC20} from "../src/interfaces/IERC20.sol";

/// @dev Helper so tests can act as distinct traders without forge-std cheatcodes.
contract MarketTrader {
    IERC20 public immutable usdc;
    SubwayMarket public immutable market;

    constructor(address usdc_, address market_) {
        usdc = IERC20(usdc_);
        market = SubwayMarket(market_);
    }

    function buy(uint256 marketId, SubwayMarket.Side side, uint256 amount) external {
        usdc.approve(address(market), amount);
        market.buy(marketId, side, amount);
    }

    function sell(uint256 marketId, SubwayMarket.Side side, uint256 amount) external {
        market.sell(marketId, side, amount);
    }

    function claim(uint256 marketId) external {
        market.claim(marketId);
    }
}

contract SubwayMarketTest {
    uint256 internal constant ONE_USDC = 1_000_000;

    MockUSDC internal usdc;
    SubwayMarket internal market;
    MarketTrader internal yesTrader;
    MarketTrader internal noTrader;

    function setUp() public {
        usdc = new MockUSDC();
        market = new SubwayMarket(address(usdc), address(this));
        yesTrader = new MarketTrader(address(usdc), address(market));
        noTrader = new MarketTrader(address(usdc), address(market));
    }

    function test_create_buy_resolve_claim_happyPath() public {
        uint256 marketId = market.createMarket("train-7", "trip-2026-08-08", bytes("route-A"));
        require(marketId == 0, "first market id");

        usdc.mint(address(yesTrader), 100 * ONE_USDC);
        usdc.mint(address(noTrader), 50 * ONE_USDC);

        yesTrader.buy(marketId, SubwayMarket.Side.Yes, 100 * ONE_USDC);
        noTrader.buy(marketId, SubwayMarket.Side.No, 50 * ONE_USDC);

        (,,, SubwayMarket.Outcome outcome, uint256 yesPool, uint256 noPool,) = market.markets(marketId);
        require(outcome == SubwayMarket.Outcome.Unresolved, "still open");
        require(yesPool == 100 * ONE_USDC, "yes pool");
        require(noPool == 50 * ONE_USDC, "no pool");

        market.resolve(marketId, SubwayMarket.Outcome.Yes);

        uint256 yesBefore = usdc.balanceOf(address(yesTrader));
        yesTrader.claim(marketId);
        uint256 yesAfter = usdc.balanceOf(address(yesTrader));

        require(yesAfter - yesBefore == 150 * ONE_USDC, "yes winner payout");

        // Losing side keeps zero redeemable balance after resolution.
        uint256 noBefore = usdc.balanceOf(address(noTrader));
        try noTrader.claim(marketId) {
            revert("loser should not claim");
        } catch {}
        require(usdc.balanceOf(address(noTrader)) == noBefore, "no payout for loser");
    }

    function test_sell_before_resolve() public {
        uint256 marketId = market.createMarket("train-3", "trip-sell", bytes(""));

        usdc.mint(address(yesTrader), 40 * ONE_USDC);
        yesTrader.buy(marketId, SubwayMarket.Side.Yes, 40 * ONE_USDC);
        yesTrader.sell(marketId, SubwayMarket.Side.Yes, 15 * ONE_USDC);

        require(usdc.balanceOf(address(yesTrader)) == 15 * ONE_USDC, "partial sell refund");
        require(market.yesBalance(marketId, address(yesTrader)) == 25 * ONE_USDC, "remaining shares");
    }
}
