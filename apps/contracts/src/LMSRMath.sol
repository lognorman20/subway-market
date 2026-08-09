// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @notice Fixed-point LMSR helpers (WAD = 1e18).
/// @dev Binary cost C(qE,qL) = max(q) + b * ln(e^((qE-max)/b) + e^((qL-max)/b)).
library LMSRMath {
    uint256 internal constant WAD = 1e18;
    /// @dev ln(2) * 1e18
    int256 internal constant LN2_WAD = 693147180559945309;

    error ExpOverflow();
    error LnTooSmall();
    error ZeroLiquidity();
    error InsufficientShares();

    /// @dev USDC (6 decimals) → WAD.
    function toWad(uint256 usdcAmount) internal pure returns (uint256) {
        return usdcAmount * 1e12;
    }

    /// @dev WAD → USDC, round up (charge trader).
    function fromWadCeil(uint256 wadAmount) internal pure returns (uint256) {
        return (wadAmount + 1e12 - 1) / 1e12;
    }

    /// @dev WAD → USDC, round down (pay trader).
    function fromWadFloor(uint256 wadAmount) internal pure returns (uint256) {
        return wadAmount / 1e12;
    }

    /// @dev Minimum subsidy for empty binary LMSR: b * ln(2), in USDC units.
    function requiredSubsidy(uint256 liquidityBUsdc) internal pure returns (uint256) {
        if (liquidityBUsdc == 0) revert ZeroLiquidity();
        return fromWadCeil((toWad(liquidityBUsdc) * uint256(LN2_WAD)) / WAD);
    }

    /// @dev C(qEarly, qLate) in WAD. Inputs are share units (USDC 6 decimals).
    function cost(uint256 qEarlyUsdc, uint256 qLateUsdc, uint256 bUsdc) internal pure returns (uint256) {
        if (bUsdc == 0) revert ZeroLiquidity();
        int256 b = int256(toWad(bUsdc));
        int256 qE = int256(toWad(qEarlyUsdc));
        int256 qL = int256(toWad(qLateUsdc));
        int256 maxQ = qE > qL ? qE : qL;
        // (q-max)/b as WAD-scaled dimensionless: (q-max)*WAD/b
        uint256 sumExp = wadExp(_divFloor((qE - maxQ) * int256(WAD), b))
            + wadExp(_divFloor((qL - maxQ) * int256(WAD), b));
        int256 c = maxQ + (b * wadLnSigned(sumExp)) / int256(WAD);
        return c > 0 ? uint256(c) : 0;
    }

    /// @dev USDC cost to buy `shares` of one side (ceil).
    function buyCost(
        uint256 qEarlyUsdc,
        uint256 qLateUsdc,
        uint256 bUsdc,
        bool early,
        uint256 sharesUsdc
    ) internal pure returns (uint256) {
        if (sharesUsdc == 0) return 0;
        uint256 before_ = cost(qEarlyUsdc, qLateUsdc, bUsdc);
        uint256 after_ = early
            ? cost(qEarlyUsdc + sharesUsdc, qLateUsdc, bUsdc)
            : cost(qEarlyUsdc, qLateUsdc + sharesUsdc, bUsdc);
        if (after_ < before_) return 0;
        return fromWadCeil(after_ - before_);
    }

    /// @dev USDC proceeds from selling `shares` of one side (floor).
    function sellProceeds(
        uint256 qEarlyUsdc,
        uint256 qLateUsdc,
        uint256 bUsdc,
        bool early,
        uint256 sharesUsdc
    ) internal pure returns (uint256) {
        if (sharesUsdc == 0) return 0;
        if (early) {
            if (qEarlyUsdc < sharesUsdc) revert InsufficientShares();
        } else if (qLateUsdc < sharesUsdc) {
            revert InsufficientShares();
        }
        uint256 before_ = cost(qEarlyUsdc, qLateUsdc, bUsdc);
        uint256 after_ = early
            ? cost(qEarlyUsdc - sharesUsdc, qLateUsdc, bUsdc)
            : cost(qEarlyUsdc, qLateUsdc - sharesUsdc, bUsdc);
        if (before_ < after_) return 0;
        return fromWadFloor(before_ - after_);
    }

    /// @dev Marginal price of early in WAD (1e18 = 100%).
    function priceEarly(uint256 qEarlyUsdc, uint256 qLateUsdc, uint256 bUsdc) internal pure returns (uint256) {
        if (bUsdc == 0) revert ZeroLiquidity();
        int256 b = int256(toWad(bUsdc));
        int256 qE = int256(toWad(qEarlyUsdc));
        int256 qL = int256(toWad(qLateUsdc));
        int256 maxQ = qE > qL ? qE : qL;
        uint256 expE = wadExp(_divFloor((qE - maxQ) * int256(WAD), b));
        uint256 expL = wadExp(_divFloor((qL - maxQ) * int256(WAD), b));
        return (expE * WAD) / (expE + expL);
    }

    function _divFloor(int256 a, int256 b) private pure returns (int256) {
        int256 q = a / b;
        if ((a ^ b) < 0 && a % b != 0) q -= 1;
        return q;
    }

    /// @dev e^x with x in WAD. Reduce by integer multiples of ln2, then Taylor.
    function wadExp(int256 x) internal pure returns (uint256) {
        unchecked {
            // Overflow / underflow guards (Solady bounds).
            if (x >= 135305999368893231589) revert ExpOverflow();
            if (x <= -41446531673892822312) return 0;

            // k = floor(x / ln2 + 0.5) ≈ nearest int so rem ∈ (-0.5 ln2, 0.5 ln2]
            int256 k = x / LN2_WAD;
            int256 rem = x - k * LN2_WAD;
            if (rem * 2 > LN2_WAD) {
                ++k;
                rem -= LN2_WAD;
            } else if (rem * 2 < -LN2_WAD) {
                --k;
                rem += LN2_WAD;
            }

            // Taylor e^rem for |rem| <= ~0.347e18
            int256 term = int256(WAD);
            int256 sum = int256(WAD);
            for (uint256 i = 1; i <= 20; ++i) {
                term = (term * rem) / int256(WAD) / int256(uint256(i));
                sum += term;
            }

            if (sum <= 0) return 0;
            uint256 result = uint256(sum);
            if (k >= 0) {
                // Safe: k max ~195 for wadExp domain
                return result << uint256(k);
            }
            uint256 nk = uint256(-k);
            if (nk >= 256) return 0;
            return result >> nk;
        }
    }

    /// @dev Signed ln(x) for x > 0 (WAD-scaled).
    function wadLnSigned(uint256 x) internal pure returns (int256) {
        if (x == 0) revert LnTooSmall();
        unchecked {
            uint256 kk;
            uint256 z = x;
            if (z >= WAD) {
                while (z >= 2 * WAD) {
                    z >>= 1;
                    ++kk;
                }
                return int256(_atanhLn(z)) + int256(kk) * LN2_WAD;
            }
            while (z < WAD) {
                z <<= 1;
                ++kk;
            }
            return int256(_atanhLn(z)) - int256(kk) * LN2_WAD;
        }
    }

    /// @dev ln for z in [WAD, 2*WAD).
    function _atanhLn(uint256 z) private pure returns (uint256) {
        unchecked {
            int256 p = int256(((z - WAD) * WAD) / (z + WAD));
            int256 p2 = (p * p) / int256(WAD);
            int256 term = p;
            int256 sum = term;
            for (uint256 n = 3; n <= 31; n += 2) {
                term = (term * p2) / int256(WAD);
                sum += term / int256(uint256(n));
            }
            return uint256(sum * 2);
        }
    }
}
