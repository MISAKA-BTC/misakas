// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMisakaModelAMM
/// @notice The window onto a line's curve (ADR-0087 Decisions 2–4, keyed by line since
///         ADR-0088 Decision 9), served natively at
///         `0x000000000000000000000000000000000000F011` (ADR-0089 Decisions 1–2).
///
///         One curve per LINE: a constant-product curve over the MSK reserve (ADR-0090: the
///         reserve is the seed plus every net leg since; there is no virtual reserve, and the
///         product `reserve × units` is taken from the row at each move) — formerly plus a virtual
///         reserve `V`, holding a fixed supply of position units and no MSK at opening. The
///         price at any moment is `(mskReserve + V) / positionUnits` and there is no other
///         price. The market OPENS AT ITS FIRST BUY; a line whose market never opened has
///         `exists == false` and a zero row. The founding line's market is keyed by the class
///         id (ADR-0088 D9), so for a class with only its founding line every value here is
///         ADR-0087's byte for byte.
///
///         Every read is a row of `fold(selected_parent(B))` for the EVM block `B` the call
///         runs in. The quotes call the same `palw_model_buy_quote_v1` /
///         `palw_model_sell_quote_v1` the fold calls, so a quote is the fold's arithmetic and
///         not a re-implementation of it — but it is a quote against the SELECTED PARENT'S
///         row. What a trade actually gets is decided in `fold(B)` after every carrier-borne
///         buy and sell of `B` and every EVM action queued before it (ADR-0089 Decision 6);
///         pass the quote as `min` and let the fold refuse rather than fill worse.
///
///         UNITS: MSK amounts are in SOMPI (1 MSK = 1e8 sompi; the facade's `msg.value` is the
///         only wei quantity anywhere in this family). Position amounts are in UNITS, 10^6
///         units = one position. Every 64-byte id is two `bytes32` words, high half first.
///
///         Unknown line ⇒ the zero row. Malformed input reverts and consumes the frame's gas.
///         Below the `palw_model_evm` fence the address is an empty account.
interface IMisakaModelAMM {
    /// The market row of the line.
    ///   openedDaa       DAA score of the first buy
    ///   mskReserve      MSK the curve holds, sompi — funded by sinks, drained by sells, never
    ///                   a spendable output
    ///   positionUnits   units still in the curve
    ///   soldUnits       units ever bought, cumulative (a sell does not reduce it)
    ///   burned          sompi burned by the fee, cumulative
    ///   ownerPaid       sompi paid to the line's owner (the 1 % leg), cumulative
    ///   contributorPaid sompi paid to an adopted contributor out of that leg, cumulative
    ///   closedToBuys    set when the line left Active (retired) — sells continue, buys are
    ///                   refused (ADR-0087 D7, per line since ADR-0088 D6)
    ///   exists          false until the market is seeded (ADR-0090 D2)
    ///   buybackSompi    sompi the MINING REWARD has put into the curve, cumulative — 5 % of every
    ///                   block's escrowed worker reward on this line, at the claim's Final
    ///                   (ADR-0091 D1/D2); no leg, no holder
    ///   retiredUnits    units the reward's buys took out of the curve — the chain's, for good:
    ///                   positionUnits + every holder's + retiredUnits = totalSupply (ADR-0091 D4)
    function market(bytes32 lineA, bytes32 lineB)
        external
        view
        returns (
            uint64 openedDaa,
            uint64 mskReserve,
            uint64 positionUnits,
            uint64 soldUnits,
            uint64 burned,
            uint64 ownerPaid,
            uint64 contributorPaid,
            bool closedToBuys,
            bool exists,
            uint64 buybackSompi,
            uint64 retiredUnits
        );

    /// The current price, in sompi per whole position (10^6 units).
    function price(bytes32 lineA, bytes32 lineB) external view returns (uint64 sompiPerPosition);

    /// Quote a buy paying `mskInSompi` gross into the line's curve.
    ///   unitsOut    units the curve would give
    ///   burn        sompi burned (5 % of the leg, `burnPermille`)
    ///   leg         sompi to the owner and adopted contributor (1 %, `legPermille`)
    ///   net         sompi that reach the reserve (94 %)
    ///   priceAfter  sompi per position after the fill
    function quoteBuy(bytes32 lineA, bytes32 lineB, uint64 mskInSompi)
        external
        view
        returns (uint64 unitsOut, uint64 burn, uint64 leg, uint64 net, uint64 priceAfter);

    /// Quote a sell of `unitsIn` units back to the line's curve.
    ///   mskOutSompi sompi the curve releases gross
    ///   burn        sompi burned (5 %)
    ///   leg         sompi to the owner and adopted contributor (1 %)
    ///   net         sompi the seller receives (94 %)
    ///   priceAfter  sompi per position after the fill
    function quoteSell(bytes32 lineA, bytes32 lineB, uint64 unitsIn)
        external
        view
        returns (uint64 mskOutSompi, uint64 burn, uint64 leg, uint64 net, uint64 priceAfter);

    /// The curve's network constants (ADR-0090: 500,000 whole positions of supply, one unit each,
    /// `V` = 1,000 MSK, 50 ‰ burn, 10 ‰ leg). Read them; do not hard-code them.
    ///   supplyUnits      units every market opens with
    ///   unitsPerPosition 10^6
    ///   seedMinSompi     ADR-0090: the least seed that opens a market, sompi (the third word
    ///                    carried the virtual reserve before ADR-0090 retired it)
    ///   burnPermille     fee on every MSK leg, burned
    ///   legPermille      fee on every MSK leg, to the owner (shared with an adopted contributor)
    function constants()
        external
        view
        returns (uint64 supplyUnits, uint64 unitsPerPosition, uint64 seedMinSompi, uint16 burnPermille, uint16 legPermille);
}
