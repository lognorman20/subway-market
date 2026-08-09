// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "./interfaces/IERC20.sol";
import {LMSRMath} from "./LMSRMath.sol";

/// @title SubwayMarket
/// @notice Binary LMSR market: will the train arrive early or late?
/// @dev Custodial: platform signer is the on-chain caller; `account` holds shares.
///      Resolution: arrival <= frozenEtaUnix → Early (exact-on-time counts early); else Late.
///      Not production-audited.
contract SubwayMarket {
    string public constant VERSION = "0.2.0-lmsr";

    enum Side {
        Early,
        Late
    }

    enum Outcome {
        Unresolved,
        Early,
        Late
    }

    struct Market {
        string trainId;
        string tripId;
        string stopId;
        uint64 frozenEtaUnix;
        uint256 liquidityB;
        uint256 qEarly;
        uint256 qLate;
        Outcome outcome;
        bool exists;
    }

    IERC20 public immutable usdc;
    address public owner;
    /// @notice Platform signer — sole operator for create / buy / sell / resolve.
    address public platform;

    uint256 public nextMarketId;
    mapping(uint256 => Market) public markets;
    mapping(uint256 => mapping(address => uint256)) public earlyBalance;
    mapping(uint256 => mapping(address => uint256)) public lateBalance;
    mapping(uint256 => mapping(address => bool)) public claimed;

    event MarketCreated(
        uint256 indexed marketId,
        string trainId,
        string tripId,
        string stopId,
        uint64 frozenEtaUnix,
        uint256 liquidityB,
        uint256 subsidy
    );
    event Buy(
        uint256 indexed marketId,
        address indexed account,
        Side side,
        uint256 shares,
        uint256 cost
    );
    event Sell(
        uint256 indexed marketId,
        address indexed account,
        Side side,
        uint256 shares,
        uint256 proceeds
    );
    event MarketResolved(uint256 indexed marketId, Outcome outcome, uint64 actualArrivalUnix);
    event Claimed(uint256 indexed marketId, address indexed account, uint256 amount);
    event PlatformUpdated(address indexed platform);
    event OwnershipTransferred(address indexed owner);

    error NotOwner();
    error NotPlatform();
    error ZeroAddress();
    error MarketNotFound();
    error MarketAlreadyResolved(uint256 marketId);
    error MarketOpen(uint256 marketId);
    error ZeroAmount();
    error InsufficientShares();
    error InvalidOutcome();
    error AlreadyClaimed();
    error TransferFailed();
    error ZeroLiquidity();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyPlatform() {
        if (msg.sender != platform) revert NotPlatform();
        _;
    }

    constructor(address usdc_, address platform_) {
        if (usdc_ == address(0) || platform_ == address(0)) revert ZeroAddress();
        usdc = IERC20(usdc_);
        owner = msg.sender;
        platform = platform_;
    }

    function setPlatform(address platform_) external onlyOwner {
        if (platform_ == address(0)) revert ZeroAddress();
        platform = platform_;
        emit PlatformUpdated(platform_);
    }

    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owner = newOwner;
        emit OwnershipTransferred(newOwner);
    }

    /// @notice Pure resolution rule: on or before frozen ETA → Early; strictly after → Late.
    function outcomeFromArrival(uint64 frozenEtaUnix, uint64 actualArrivalUnix)
        public
        pure
        returns (Outcome)
    {
        return actualArrivalUnix <= frozenEtaUnix ? Outcome.Early : Outcome.Late;
    }

    /// @notice Open LMSR market; pulls b*ln(2) USDC subsidy from platform.
    function createMarket(
        string calldata trainId,
        string calldata tripId,
        string calldata stopId,
        uint64 frozenEtaUnix,
        uint256 liquidityB
    ) external onlyPlatform returns (uint256 marketId) {
        if (liquidityB == 0) revert ZeroLiquidity();

        uint256 subsidy = LMSRMath.requiredSubsidy(liquidityB);
        if (!usdc.transferFrom(msg.sender, address(this), subsidy)) revert TransferFailed();

        marketId = nextMarketId++;
        markets[marketId] = Market({
            trainId: trainId,
            tripId: tripId,
            stopId: stopId,
            frozenEtaUnix: frozenEtaUnix,
            liquidityB: liquidityB,
            qEarly: 0,
            qLate: 0,
            outcome: Outcome.Unresolved,
            exists: true
        });

        emit MarketCreated(marketId, trainId, tripId, stopId, frozenEtaUnix, liquidityB, subsidy);
    }

    /// @notice Buy `shares` of Early or Late for `account`. Platform pays USDC.
    function buy(uint256 marketId, address account, Side side, uint256 shares)
        external
        onlyPlatform
    {
        if (account == address(0)) revert ZeroAddress();
        if (shares == 0) revert ZeroAmount();
        Market storage market = _openMarket(marketId);

        bool early = side == Side.Early;
        uint256 costUsdc =
            LMSRMath.buyCost(market.qEarly, market.qLate, market.liquidityB, early, shares);
        if (!usdc.transferFrom(msg.sender, address(this), costUsdc)) revert TransferFailed();

        if (early) {
            market.qEarly += shares;
            earlyBalance[marketId][account] += shares;
        } else {
            market.qLate += shares;
            lateBalance[marketId][account] += shares;
        }

        emit Buy(marketId, account, side, shares, costUsdc);
    }

    /// @notice Sell `shares` for `account`. Platform receives USDC.
    function sell(uint256 marketId, address account, Side side, uint256 shares)
        external
        onlyPlatform
    {
        if (account == address(0)) revert ZeroAddress();
        if (shares == 0) revert ZeroAmount();
        Market storage market = _openMarket(marketId);

        bool early = side == Side.Early;
        if (early) {
            if (earlyBalance[marketId][account] < shares) revert InsufficientShares();
        } else if (lateBalance[marketId][account] < shares) {
            revert InsufficientShares();
        }

        uint256 proceeds =
            LMSRMath.sellProceeds(market.qEarly, market.qLate, market.liquidityB, early, shares);

        if (early) {
            earlyBalance[marketId][account] -= shares;
            market.qEarly -= shares;
        } else {
            lateBalance[marketId][account] -= shares;
            market.qLate -= shares;
        }

        if (!usdc.transfer(msg.sender, proceeds)) revert TransferFailed();
        emit Sell(marketId, account, side, shares, proceeds);
    }

    /// @notice Resolve to Early or Late only. Emits actualArrivalUnix=0 when unknown.
    function resolveMarket(uint256 marketId, Outcome outcome) external onlyPlatform {
        if (outcome != Outcome.Early && outcome != Outcome.Late) revert InvalidOutcome();
        Market storage market = _market(marketId);
        if (market.outcome != Outcome.Unresolved) revert MarketAlreadyResolved(marketId);
        market.outcome = outcome;
        emit MarketResolved(marketId, outcome, 0);
    }

    /// @notice Resolve from actual arrival vs frozen ETA (exact-on-time → Early).
    function resolveMarketFromArrival(uint256 marketId, uint64 actualArrivalUnix)
        external
        onlyPlatform
    {
        Market storage market = _market(marketId);
        if (market.outcome != Outcome.Unresolved) revert MarketAlreadyResolved(marketId);
        Outcome outcome = outcomeFromArrival(market.frozenEtaUnix, actualArrivalUnix);
        market.outcome = outcome;
        emit MarketResolved(marketId, outcome, actualArrivalUnix);
    }

    /// @notice Redeem winning shares 1:1 USDC for `account`.
    function claim(uint256 marketId, address account) external {
        if (account == address(0)) revert ZeroAddress();
        Market storage market = _market(marketId);
        if (market.outcome == Outcome.Unresolved) revert MarketOpen(marketId);
        if (claimed[marketId][account]) revert AlreadyClaimed();

        uint256 payout;
        if (market.outcome == Outcome.Early) {
            payout = earlyBalance[marketId][account];
            earlyBalance[marketId][account] = 0;
            lateBalance[marketId][account] = 0;
        } else {
            payout = lateBalance[marketId][account];
            lateBalance[marketId][account] = 0;
            earlyBalance[marketId][account] = 0;
        }

        if (payout == 0) revert InsufficientShares();
        claimed[marketId][account] = true;
        if (!usdc.transfer(account, payout)) revert TransferFailed();
        emit Claimed(marketId, account, payout);
    }

    /// @notice Marginal Early price in WAD (1e18 = 100%). After resolve: 1e18 or 0.
    function priceEarly(uint256 marketId) external view returns (uint256) {
        Market storage market = _market(marketId);
        if (market.outcome == Outcome.Early) return LMSRMath.WAD;
        if (market.outcome == Outcome.Late) return 0;
        return LMSRMath.priceEarly(market.qEarly, market.qLate, market.liquidityB);
    }

    /// @notice Marginal Late price in WAD.
    function priceLate(uint256 marketId) external view returns (uint256) {
        Market storage market = _market(marketId);
        if (market.outcome == Outcome.Late) return LMSRMath.WAD;
        if (market.outcome == Outcome.Early) return 0;
        return LMSRMath.WAD - LMSRMath.priceEarly(market.qEarly, market.qLate, market.liquidityB);
    }

    /// @notice Quote USDC cost to buy `shares` without executing.
    function quoteBuy(uint256 marketId, Side side, uint256 shares) external view returns (uint256) {
        Market storage market = _openMarket(marketId);
        return LMSRMath.buyCost(
            market.qEarly, market.qLate, market.liquidityB, side == Side.Early, shares
        );
    }

    /// @notice Quote USDC proceeds to sell `shares` without executing.
    function quoteSell(uint256 marketId, Side side, uint256 shares) external view returns (uint256) {
        Market storage market = _openMarket(marketId);
        return LMSRMath.sellProceeds(
            market.qEarly, market.qLate, market.liquidityB, side == Side.Early, shares
        );
    }

    function _market(uint256 marketId) internal view returns (Market storage market) {
        market = markets[marketId];
        if (!market.exists) revert MarketNotFound();
    }

    function _openMarket(uint256 marketId) internal view returns (Market storage market) {
        market = _market(marketId);
        if (market.outcome != Outcome.Unresolved) revert MarketAlreadyResolved(marketId);
    }
}
