// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title MisakaModelAddresses
/// @notice Where the MISAKA model market lives on the EVM lane (ADR-0089 Decision 1): the
///         three read precompiles, the one writer, and the prefix of the per-line facade
///         family. All of them are NATIVE code in the node's executor — there is no bytecode
///         at any of these addresses (`EXTCODESIZE` is 0, `EXTCODEHASH` is the empty-code
///         hash), exactly as for `0x…F002` (withdraw) and `0x…F003` (ML-DSA-87 verify).
///
///         Below the `palw_model_evm` fence (Decision 9) every one of them is an empty
///         account: a call succeeds with empty return data (the F003 idiom). A reader that
///         needs to know the window is open checks that `IMisakaModelRegistry.chainDaa()`
///         returns 32 bytes, not that it returns zero.
/// @dev    The truth is the PALW state fold, never EVM storage: every read at these addresses
///         is a row of `fold(selected_parent(B))` for the EVM block `B` the call executes in
///         (Decision 2), and the only writes are the two actions of `IMisakaModelWriter`.
library MisakaModelAddresses {
    /// `IMisakaModelRegistry` — classes, lines, versions, usage, evaluations, proposals.
    address internal constant MODEL_REGISTRY = address(0x000000000000000000000000000000000000F010);

    /// `IMisakaModelAMM` — the curve: market rows, price, quotes, and the curve constants.
    address internal constant MODEL_AMM = address(0x000000000000000000000000000000000000F011);

    /// `IMisakaModelPosition` — position balances by 64-byte holder id or by EVM address.
    address internal constant MODEL_POSITION = address(0x000000000000000000000000000000000000F012);

    /// `IMisakaModelWriter` — the hand: `sendAction(bytes)`. Also the escrow account that
    /// holds a queued buy's `msg.value` until the settling block (Decision 6).
    address internal constant MODEL_WRITER = address(0x000000000000000000000000000000000000F013);

    /// The first two bytes of every line facade address ("MP"). The remaining 18 bytes are
    /// `blake2b_512("misaka-evm/model-position-facade/v1" ‖ line_id)[..18]`. BLAKE2b is NOT
    /// available in Solidity (no precompile, no opcode on this lane), so a facade address
    /// cannot be derived in a contract: read it from `IMisakaModelRegistry.facadeOf(lineA,
    /// lineB)` and, when an address arrives from outside, check it back with
    /// `IMisakaModelRegistry.lineOf(address)`. The prefix alone proves nothing — an address
    /// with the prefix that names no line is an empty account.
    bytes2 internal constant FACADE_PREFIX = 0x4d50;

    /// The EVM lane's chain id (`0x4D534B` spells "MSK"; frozen in ADR-0020). `eth_call`
    /// against the addresses above works from any Shanghai toolchain at this id with no
    /// MISAKA-specific RPC.
    uint256 internal constant CHAIN_ID = 0x4D534B;

    /// wei per sompi (`EVM_NATIVE_SCALE`). A buy's `msg.value` must be a NONZERO MULTIPLE of
    /// it, so the fold's sompi leg is exact (the F002 rule). Every other MSK amount in these
    /// interfaces is in sompi; every position amount is in units (10^6 units = one position).
    uint256 internal constant NATIVE_SCALE_WEI = 1e10;

    // ---- IMisakaModelWriter action encoding (ADR-0089 Decision 5) ----

    /// `data[0]` of every `sendAction` payload.
    uint8 internal constant ACTION_ENCODING_VERSION = 1;

    /// Action ids: `data[1..4]`, a big-endian u24. Ids 3–255 are reserved.
    uint24 internal constant ACTION_BUY = 1;
    uint24 internal constant ACTION_SELL = 2;
    /// ADR-0090: the seed that opens a line's market — `msg.value` is the whole of it.
    uint24 internal constant ACTION_SEED = 3;
    /// ADR-0090: the least seed, in sompi (100,000 MSK); the writer reverts `SeedTooSmall()` under it.
    uint64 internal constant SEED_MIN_SOMPI = 10_000_000_000_000;
    /// ADR-0090: a line's whole supply — 500,000 positions, and a position is one unit (no fraction).
    uint64 internal constant SUPPLY_POSITIONS = 500_000;
    uint64 internal constant UNITS_PER_POSITION = 1;

    /// Actions accepted per EVM block (`PALW_EVM_MARKET_ACTIONS_PER_BLOCK_V1`); the 129th
    /// call in a block reverts.
    uint256 internal constant MAX_ACTIONS_PER_EVM_BLOCK = 128;
}
