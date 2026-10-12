// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMisakaModelAMM
/// @notice The window onto a line's curve (ADR-0087 Decisions 2–4, keyed by line since
///         ADR-0088 Decision 9), served natively at
///         `0x000000000000000000000000000000000000F011` (ADR-0089 Decisions 1–2).
///
///         One curve per LINE: a constant-product curve `X × positionUnits` with `X = mskReserve
///         + V`, the product taken from the row at each move. Below `palw_model_virtual_v1`
///         (ADR-0090) `V` is zero: the reserve is the seed plus every net leg since, and a market
///         OPENS BY ITS SEED (a line whose seed has not reached the floor has `exists == false`).
///         **Past `palw_model_virtual_v1` (ADR-0162) every line's market is open from the line's
///         creation on a virtual reserve `V` = 10,000,000 MSK (`constants()`' third word):** X = V
///         and the whole supply in the curve at the opening, a first price of `V / 500,000` = 20
///         MSK, and a seed optional — any amount, before the first trade, locked for good. `V`
///         prices and is never paid: a sell is paid only out of the real reserve above the locked
///         seed. A market opened before the fence keeps `V = 0`. The price at any moment is
///         `(mskReserve + V) / positionUnits` and there is no other price. The founding line's
///         market is keyed by the class id (ADR-0088 D9).
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
///         only wei quantity anywhere in this family). Position amounts are in UNITS, one unit =
///         one position (ADR-0090). Every 64-byte id is two `bytes32` words, high half first.
///
///         Unknown line ⇒ the zero row. Malformed input reverts and consumes the frame's gas.
///         Below the `palw_model_evm` fence the address is an empty account.
interface IMisakaModelAMM {
    /// The market row of the line.
    ///   openedDaa       DAA score the market opened at — its seed's (ADR-0090), or past
    ///                   ADR-0162 the line's creation (the fence's, for a line older than it)
    ///   mskReserve      MSK the curve holds, sompi — funded by sinks, drained by sells, never
    ///                   a spendable output
    ///   positionUnits   units still in the curve
    ///   soldUnits       units ever bought, cumulative (a sell does not reduce it)
    ///   burned          sompi burned by the fee, cumulative
    ///   ownerPaid       sompi paid to the line's owner (the owner leg, `legPermille`), cumulative
    ///   contributorPaid sompi paid to an adopted contributor out of that leg, cumulative
    ///   closedToBuys    set when the line left Active (retired) — sells continue, buys are
    ///                   refused (ADR-0087 D7, per line since ADR-0088 D6); past the 2026-09-23
    ///                   audit fence also set while the model registry has not admitted the
    ///                   line's class (P-B3: the writer reverts `ClassNotEligible()` then). Past
    ///                   ADR-0162 it is the live trading gate: set until the class is approved
    ///                   (status and lifecycle exactly Active), and while the line is retired
    ///   exists          false until the market is seeded (ADR-0090 D2). Below the 2026-09-23
    ///                   audit fence (testnet-11) a pledge-only row (ADR-0094) also reads true;
    ///                   past it (testnet-12) only a seeded market does (P10). Past ADR-0162
    ///                   true for every line from its creation
    ///   buybackSompi    sompi the MINING REWARD has put into the curve, cumulative — 5 % of a
    ///                   claim's escrowed worker reward on this line, at the claim's Final
    ///                   (ADR-0091 D1/D2); no leg, no holder. Past ADR-0162 only while the line
    ///                   trades (before approval the miner keeps the whole reward)
    ///   retiredUnits    units the reward's buys took out of the curve — the chain's, for good:
    ///                   positionUnits + every holder's + retiredUnits = totalSupply (ADR-0091 D4)
    /// Past ADR-0162 a TWELFTH word follows, `virtualSompi` — the virtual reserve this market
    /// opened on (0 for one opened before the fence); every earlier word keeps its offset, so this
    /// eleven-word declaration still decodes. `IMisakaModelAMMVirtual.market` declares all twelve.
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

    /// The current price, in sompi per whole position (one unit): `(mskReserve + V) / positionUnits`.
    function price(bytes32 lineA, bytes32 lineB) external view returns (uint64 sompiPerPosition);

    /// Quote a buy paying `mskInSompi` gross into the line's curve.
    ///   unitsOut    units the curve would give
    ///   burn        sompi burned (`burnPermille`: 50 ‰ = 5 %)
    ///   leg         sompi to the owner and adopted contributor (`legPermille`: 50 ‰ = 5 % past
    ///               ADR-0114's `palw_model_leg_v2`, 10 ‰ = 1 % before it)
    ///   net         sompi that reach the reserve (the rest: 90 %, or 94 % before ADR-0114)
    ///   priceAfter  sompi per position after the fill
    function quoteBuy(bytes32 lineA, bytes32 lineB, uint64 mskInSompi)
        external
        view
        returns (uint64 unitsOut, uint64 burn, uint64 leg, uint64 net, uint64 priceAfter);

    /// Quote a sell of `unitsIn` units back to the line's curve.
    ///   mskOutSompi sompi the curve releases GROSS — `burn + leg + net`. Past ADR-0162's fence
    ///               only: below it the precompile answers the NET here a second time (the
    ///               2026-09-25 Position review's #2, fixed with the fence so no execution result
    ///               moves on a network that has not crossed it); a portable caller reads the parts
    ///   burn        sompi burned (`burnPermille`)
    ///   leg         sompi to the owner and adopted contributor (`legPermille`)
    ///   net         sompi the seller receives (the rest)
    ///   priceAfter  sompi per position after the fill
    function quoteSell(bytes32 lineA, bytes32 lineB, uint64 unitsIn)
        external
        view
        returns (uint64 mskOutSompi, uint64 burn, uint64 leg, uint64 net, uint64 priceAfter);

    /// The curve's network constants (500,000 whole positions of supply, one unit each, a 50 ‰
    /// burn, and a 50 ‰ owner leg past ADR-0114 — 10 ‰ before it). Read them; do not hard-code them.
    ///   supplyUnits      units every market opens with (500,000)
    ///   unitsPerPosition 1 (ADR-0090: a position is whole)
    ///   seedMinSompi     THE THIRD WORD MEANS TWO THINGS. Below ADR-0162's fence: the least seed
    ///                    that opens a market (ADR-0090; 1,000,000 MSK past ADR-0120, 100,000 MSK
    ///                    before it; a smaller payment is collected as a pledge, ADR-0094). Past
    ///                    it: the VIRTUAL RESERVE every market opens on, 10,000,000 MSK — no seed
    ///                    opens anything there, and the least seed is zero. The word carried the
    ///                    virtual reserve before ADR-0090 too; `market()` answering twelve words is
    ///                    how a caller tells the two apart
    ///   burnPermille     fee on every MSK leg, burned
    ///   legPermille      fee on every MSK leg, to the owner (shared with an adopted contributor)
    function constants()
        external
        view
        returns (uint64 supplyUnits, uint64 unitsPerPosition, uint64 seedMinSompi, uint16 burnPermille, uint16 legPermille);
}

/// @title IMisakaModelAMMVirtual
/// @notice ADR-0162: the AMM window's `market()` as it answers past `palw_model_virtual_v1` —
///         IMisakaModelAMM's eleven words and the market's own virtual reserve appended as a
///         twelfth (`virtualSompi`: 10,000,000 MSK for a market opened on it, 0 for one opened
///         before the fence). Call it only where the fence is in force: below it the window
///         answers eleven words and this declaration does not decode.
interface IMisakaModelAMMVirtual {
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
            uint64 retiredUnits,
            uint64 virtualSompi
        );
}
