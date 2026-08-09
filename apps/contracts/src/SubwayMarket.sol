// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "./interfaces/IERC20.sol";

/// @title SubwayMarket
/// @notice Lean binary market for MTA train lateness (YES = late, NO = on time).
/// @dev Scaffold for local anvil / Monad — not production-audited.
contract SubwayMarket {
    string public constant VERSION = "0.1.0";

    enum Side {
        Yes,
        No
    }

    enum Outcome {
        Unresolved,
        Yes,
        No
    }

    struct Market {
        string trainId;
        string dedupeKey;
        bytes metadata;
        Outcome outcome;
        uint256 yesPool;
        uint256 noPool;
        bool exists;
    }

    IERC20 public immutable usdc;
    address public owner;
    address public resolver;

    uint256 public nextMarketId;
    mapping(uint256 => Market) public markets;
    mapping(uint256 => mapping(address => uint256)) public yesBalance;
    mapping(uint256 => mapping(address => uint256)) public noBalance;
    mapping(uint256 => mapping(address => bool)) public claimed;

    event MarketCreated(
        uint256 indexed marketId,
        string trainId,
        string dedupeKey,
        bytes metadata
    );
    event Buy(uint256 indexed marketId, address indexed account, Side side, uint256 amount);
    event Sell(uint256 indexed marketId, address indexed account, Side side, uint256 amount);
    event MarketResolved(uint256 indexed marketId, Outcome outcome);
    event Claimed(uint256 indexed marketId, address indexed account, uint256 amount);
    event ResolverUpdated(address indexed resolver);
    event OwnershipTransferred(address indexed owner);

    error NotOwner();
    error NotResolver();
    error ZeroAddress();
    error MarketNotFound();
    error MarketAlreadyResolved(uint256 marketId);
    error MarketOpen(uint256 marketId);
    error ZeroAmount();
    error InsufficientShares();
    error InvalidOutcome();
    error AlreadyClaimed();
    error TransferFailed();

    modifier onlyOwner() {
        if (msg.sender != owner) revert NotOwner();
        _;
    }

    modifier onlyResolver() {
        if (msg.sender != resolver) revert NotResolver();
        _;
    }

    constructor(address usdc_, address resolver_) {
        if (usdc_ == address(0) || resolver_ == address(0)) revert ZeroAddress();
        usdc = IERC20(usdc_);
        owner = msg.sender;
        resolver = resolver_;
    }

    function setResolver(address resolver_) external onlyOwner {
        if (resolver_ == address(0)) revert ZeroAddress();
        resolver = resolver_;
        emit ResolverUpdated(resolver_);
    }

    function transferOwnership(address newOwner) external onlyOwner {
        if (newOwner == address(0)) revert ZeroAddress();
        owner = newOwner;
        emit OwnershipTransferred(newOwner);
    }

    /// @notice Open a market keyed by train + dedupe identifiers (off-chain indexing).
    function createMarket(
        string calldata trainId,
        string calldata dedupeKey,
        bytes calldata metadata
    ) external returns (uint256 marketId) {
        marketId = nextMarketId++;
        markets[marketId] = Market({
            trainId: trainId,
            dedupeKey: dedupeKey,
            metadata: metadata,
            outcome: Outcome.Unresolved,
            yesPool: 0,
            noPool: 0,
            exists: true
        });
        emit MarketCreated(marketId, trainId, dedupeKey, metadata);
    }

    /// @notice Stake USDC on YES (late) or NO (on time). One share per 1 USDC unit.
    function buy(uint256 marketId, Side side, uint256 amount) external {
        Market storage market = _openMarket(marketId);
        if (amount == 0) revert ZeroAmount();
        if (!usdc.transferFrom(msg.sender, address(this), amount)) revert TransferFailed();

        if (side == Side.Yes) {
            yesBalance[marketId][msg.sender] += amount;
            market.yesPool += amount;
        } else {
            noBalance[marketId][msg.sender] += amount;
            market.noPool += amount;
        }

        emit Buy(marketId, msg.sender, side, amount);
    }

    /// @notice Exit stake before resolution at 1:1 USDC.
    function sell(uint256 marketId, Side side, uint256 amount) external {
        Market storage market = _openMarket(marketId);
        if (amount == 0) revert ZeroAmount();

        if (side == Side.Yes) {
            if (yesBalance[marketId][msg.sender] < amount) revert InsufficientShares();
            yesBalance[marketId][msg.sender] -= amount;
            market.yesPool -= amount;
        } else {
            if (noBalance[marketId][msg.sender] < amount) revert InsufficientShares();
            noBalance[marketId][msg.sender] -= amount;
            market.noPool -= amount;
        }

        if (!usdc.transfer(msg.sender, amount)) revert TransferFailed();
        emit Sell(marketId, msg.sender, side, amount);
    }

    /// @notice Oracle path — resolver picks the winning side after trip completes.
    function resolve(uint256 marketId, Outcome outcome) external onlyResolver {
        if (outcome != Outcome.Yes && outcome != Outcome.No) revert InvalidOutcome();

        Market storage market = _market(marketId);
        if (market.outcome != Outcome.Unresolved) revert MarketAlreadyResolved(marketId);
        market.outcome = outcome;
        emit MarketResolved(marketId, outcome);
    }

    /// @notice Winners redeem parimutuel share of the combined pool.
    function claim(uint256 marketId) external {
        Market storage market = _market(marketId);
        if (market.outcome == Outcome.Unresolved) revert MarketOpen(marketId);
        if (claimed[marketId][msg.sender]) revert AlreadyClaimed();

        uint256 payout;
        uint256 totalPool = market.yesPool + market.noPool;

        if (market.outcome == Outcome.Yes) {
            uint256 shares = yesBalance[marketId][msg.sender];
            if (shares == 0) revert InsufficientShares();
            payout = (shares * totalPool) / market.yesPool;
            yesBalance[marketId][msg.sender] = 0;
        } else {
            uint256 shares = noBalance[marketId][msg.sender];
            if (shares == 0) revert InsufficientShares();
            payout = (shares * totalPool) / market.noPool;
            noBalance[marketId][msg.sender] = 0;
        }

        claimed[marketId][msg.sender] = true;
        if (!usdc.transfer(msg.sender, payout)) revert TransferFailed();
        emit Claimed(marketId, msg.sender, payout);
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
