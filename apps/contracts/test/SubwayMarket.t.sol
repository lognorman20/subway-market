// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {SubwayMarket} from "../src/SubwayMarket.sol";
import {MockUSDC} from "../src/mocks/MockUSDC.sol";
import {LMSRMath} from "../src/LMSRMath.sol";

/// @dev Platform-signer stand-in: holds USDC, calls market on behalf of accounts.
contract PlatformCaller {
    MockUSDC public immutable usdc;
    SubwayMarket public immutable market;

    constructor(address usdc_, address market_) {
        usdc = MockUSDC(usdc_);
        market = SubwayMarket(market_);
    }

    function createMarket(
        string calldata trainId,
        string calldata tripId,
        string calldata stopId,
        uint64 frozenEtaUnix,
        uint256 liquidityB
    ) external returns (uint256) {
        uint256 subsidy = LMSRMath.requiredSubsidy(liquidityB);
        usdc.approve(address(market), subsidy);
        return market.createMarket(trainId, tripId, stopId, frozenEtaUnix, liquidityB);
    }

    function buy(uint256 marketId, address account, SubwayMarket.Side side, uint256 shares)
        external
    {
        uint256 quote = market.quoteBuy(marketId, side, shares);
        usdc.approve(address(market), quote);
        market.buy(marketId, account, side, shares);
    }

    function sell(uint256 marketId, address account, SubwayMarket.Side side, uint256 shares)
        external
    {
        market.sell(marketId, account, side, shares);
    }

    function resolveMarket(uint256 marketId, SubwayMarket.Outcome outcome) external {
        market.resolveMarket(marketId, outcome);
    }

    function resolveMarketFromArrival(uint256 marketId, uint64 actualArrivalUnix) external {
        market.resolveMarketFromArrival(marketId, actualArrivalUnix);
    }

    function claim(uint256 marketId, address account) external {
        market.claim(marketId, account);
    }
}

contract SubwayMarketTest {
    uint256 internal constant ONE_USDC = 1_000_000;
    uint256 internal constant WAD = 1e18;
    /// @dev LMSR liquidity parameter (USDC 6 decimals).
    uint256 internal constant B = 100 * ONE_USDC;

    MockUSDC internal usdc;
    SubwayMarket internal market;
    PlatformCaller internal platform;

    address internal alice = address(0xA11CE);
    address internal bob = address(0xB0B);

    function setUp() public {
        usdc = new MockUSDC();
        // Deploy market with temporary platform; re-point after PlatformCaller exists.
        market = new SubwayMarket(address(usdc), address(this));
        platform = new PlatformCaller(address(usdc), address(market));
        market.setPlatform(address(platform));

        // Seed platform with plenty of USDC for subsidy + trades.
        usdc.mint(address(platform), 1_000_000 * ONE_USDC);
    }

    function test_createMarket_seedsSubsidyAndFiftyFifty() public {
        uint256 beforeBal = usdc.balanceOf(address(platform));
        uint256 marketId = platform.createMarket("7", "trip-1", "stop-42", 1_700_000_000, B);
        require(marketId == 0, "id");

        uint256 subsidy = LMSRMath.requiredSubsidy(B);
        require(usdc.balanceOf(address(platform)) == beforeBal - subsidy, "subsidy pulled");
        require(usdc.balanceOf(address(market)) == subsidy, "subsidy escrowed");

        uint256 pEarly = market.priceEarly(marketId);
        uint256 pLate = market.priceLate(marketId);
        // ~50/50 within 0.1%
        require(pEarly > WAD / 2 - WAD / 1000, "early ~50");
        require(pEarly < WAD / 2 + WAD / 1000, "early ~50 hi");
        require(pLate > WAD / 2 - WAD / 1000, "late ~50");
    }

    function test_buy_movesPriceTowardSide() public {
        uint256 marketId = platform.createMarket("A", "t1", "s1", 1_700_000_100, B);

        uint256 p0 = market.priceEarly(marketId);
        platform.buy(marketId, alice, SubwayMarket.Side.Early, 10 * ONE_USDC);
        uint256 p1 = market.priceEarly(marketId);
        require(p1 > p0, "early price up after early buy");
        require(market.priceLate(marketId) < WAD - p0, "late fell");

        uint256 late0 = market.priceLate(marketId);
        platform.buy(marketId, bob, SubwayMarket.Side.Late, 10 * ONE_USDC);
        require(market.priceLate(marketId) > late0, "late price up after late buy");
        require(market.earlyBalance(marketId, alice) == 10 * ONE_USDC, "alice shares");
        require(market.lateBalance(marketId, bob) == 10 * ONE_USDC, "bob shares");
    }

    function test_sell_movesPriceAwayFromSide() public {
        uint256 marketId = platform.createMarket("B", "t2", "s2", 1_700_000_200, B);
        platform.buy(marketId, alice, SubwayMarket.Side.Early, 20 * ONE_USDC);
        uint256 pAfterBuy = market.priceEarly(marketId);

        uint256 platformBefore = usdc.balanceOf(address(platform));
        platform.sell(marketId, alice, SubwayMarket.Side.Early, 8 * ONE_USDC);
        require(usdc.balanceOf(address(platform)) > platformBefore, "proceeds returned");
        require(market.earlyBalance(marketId, alice) == 12 * ONE_USDC, "remaining");
        require(market.priceEarly(marketId) < pAfterBuy, "early price down after sell");
    }

    function test_resolve_exactOnTime_isEarly() public {
        uint64 frozen = 1_700_000_500;
        uint256 marketId = platform.createMarket("C", "t3", "s3", frozen, B);

        require(
            market.outcomeFromArrival(frozen, frozen) == SubwayMarket.Outcome.Early,
            "exact = early"
        );
        require(
            market.outcomeFromArrival(frozen, frozen - 1) == SubwayMarket.Outcome.Early,
            "before = early"
        );
        require(
            market.outcomeFromArrival(frozen, frozen + 1) == SubwayMarket.Outcome.Late,
            "after = late"
        );

        platform.resolveMarketFromArrival(marketId, frozen);
        (,,,,,,, SubwayMarket.Outcome outcome,) = _marketTuple(marketId);
        require(outcome == SubwayMarket.Outcome.Early, "resolved early");
        require(market.priceEarly(marketId) == WAD, "winner price 1");
        require(market.priceLate(marketId) == 0, "loser price 0");
    }

    function test_resolve_late_and_claim() public {
        uint64 frozen = 1_700_000_600;
        uint256 marketId = platform.createMarket("D", "t4", "s4", frozen, B);

        platform.buy(marketId, alice, SubwayMarket.Side.Early, 5 * ONE_USDC);
        platform.buy(marketId, bob, SubwayMarket.Side.Late, 15 * ONE_USDC);

        platform.resolveMarketFromArrival(marketId, frozen + 30);
        (,,,,,,, SubwayMarket.Outcome outcome,) = _marketTuple(marketId);
        require(outcome == SubwayMarket.Outcome.Late, "late");

        uint256 bobBefore = usdc.balanceOf(bob);
        platform.claim(marketId, bob);
        require(usdc.balanceOf(bob) - bobBefore == 15 * ONE_USDC, "bob 1:1 payout");

        try platform.claim(marketId, alice) {
            revert("early holder must not claim");
        } catch {}
    }

    function test_fullLifecycle_create_buy_sell_resolve_claim() public {
        uint64 frozen = 1_700_000_700;
        uint256 marketId = platform.createMarket("E", "t5", "StopX", frozen, B);

        platform.buy(marketId, alice, SubwayMarket.Side.Early, 12 * ONE_USDC);
        platform.buy(marketId, bob, SubwayMarket.Side.Late, 7 * ONE_USDC);
        platform.sell(marketId, alice, SubwayMarket.Side.Early, 2 * ONE_USDC);

        require(market.earlyBalance(marketId, alice) == 10 * ONE_USDC, "alice after sell");
        require(market.priceEarly(marketId) > WAD / 2, "early favored");

        // Exact-on-time → early
        platform.resolveMarketFromArrival(marketId, frozen);

        uint256 aliceBefore = usdc.balanceOf(alice);
        platform.claim(marketId, alice);
        require(usdc.balanceOf(alice) - aliceBefore == 10 * ONE_USDC, "alice redeem");

        try platform.claim(marketId, bob) {
            revert("late must not claim");
        } catch {}
    }

    function test_resolveMarket_rejectsUnresolvedOutcome() public {
        uint256 marketId = platform.createMarket("F", "t6", "s6", 1_700_000_800, B);
        try platform.resolveMarket(marketId, SubwayMarket.Outcome.Unresolved) {
            revert("should reject");
        } catch {}
    }

    function _marketTuple(uint256 marketId)
        internal
        view
        returns (
            string memory,
            string memory,
            string memory,
            uint64,
            uint256,
            uint256,
            uint256,
            SubwayMarket.Outcome,
            bool
        )
    {
        return market.markets(marketId);
    }
}
