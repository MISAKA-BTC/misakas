// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMisakaModelWriter
/// @notice The hand (ADR-0089 Decisions 5–6): CoreWriter's shape with two actions, native at
///         `0x000000000000000000000000000000000000F013`. A call becomes a fold action applied
///         AFTER the block; nothing is returned; the fold's answer arrives one chain block
///         later as a system op. This is the ONLY channel by which the EVM writes anything
///         into the PALW state fold, and it carries exactly two actions.
///
///         DATA LAYOUT:
///           data = [version u8 = 1] ‖ [action id u24, big-endian] ‖ [abi]
///
///           id 1  buy   abi = abi.encode(bytes32 lineA, bytes32 lineB, uint256 minUnitsOut)
///                       msg.value = the GROSS MSK leg in wei — a NONZERO MULTIPLE of 1e10
///                       (`EVM_NATIVE_SCALE`; the F002 rule, so the fold's sompi leg is exact)
///           id 2  sell  abi = abi.encode(bytes32 lineA, bytes32 lineB, uint256 unitsIn,
///                                        uint256 minMskOutSompi)
///           id 3  seed  abi = abi.encode(bytes32 lineA, bytes32 lineB)          (ADR-0090: `msg.value`
///                       is the seed, at least SEED_MIN_SOMPI × 1e10 wei; nothing else)
///                       msg.value = 0
///           3–255       reserved — no action founds a line, publishes a version, sets a
///                       role, registers a class or certifies a family: those are ML-DSA-87
///                       bond signatures on the UTXO side, and an EVM account has no bond.
///
///         Every 64-byte line id is two `bytes32` words, high half first (`lineA` = the high
///         32 bytes). Position amounts are in units (10^6 = one position); `minMskOutSompi` is
///         NET sompi.
///
///         WHO MAY CALL. A holder is the signing account and only the signing account (ADR-0089
///         Decision 4): the call REVERTS `NotAnAccount()` unless `msg.sender == tx.origin` AND
///         `msg.sender` has no code. No contract wallet, multisig or account-abstraction
///         account can hold or move a position on this lane; the code-size clause is what keeps
///         the rule if a later fork admits EIP-7702.
///
///         WHAT ELSE REVERTS AT THE CALL (the "user-input fault ⇒ tx revert, block valid"
///         class): malformed data, an unknown version or action id, an unknown line, a
///         `msg.value` that is zero or not a multiple of 1e10 on a buy or a seed, a seed under
///         SEED_MIN_SOMPI (`SeedTooSmall()`), a nonzero `msg.value` on a sell, a sell of zero units, a buy on a line closed to buys, and the 129th action
///         in one EVM block (`PALW_EVM_MARKET_ACTIONS_PER_BLOCK_V1` = 128). A reverted call
///         leaves no log and therefore no action.
///
///         WHAT THE CALL DOES. It burns `PALW_EVM_WRITER_GAS_V1` (25,000, the starting schedule;
///         re-measured before the fence is armed), moves a buy's `msg.value` into THIS address
///         — the ESCROW — and emits `ActionQueued`. The executor builds the block's action list
///         by scanning the COMMITTED logs of the accepted txs for `ActionQueued` from this
///         address, in log order (F002's mechanism): a forged log from any other address is not
///         the writer's.
///
///         SETTLEMENT TIMING (Decision 6). An action queued in the EVM block of chain block `B`
///         is APPLIED by the fold at `fold(B)` — after every carrier-borne `ModelBuy` /
///         `ModelSell` of `B`, then the EVM actions in queue order, each quoted on the row as
///         it then stands, its `min` checked, filled or REFUSED (never partially filled) — and
///         SETTLED in the EVM block of `C`, the selected child of `B`, by system ops that run
///         before `C`'s user txs: a filled buy's escrow is BURNED into the line's sink output
///         (`OP_RETURN ‖ MSKMDL01 ‖ line`); a refused buy's escrow is REFUNDED to the account;
///         a filled sell CREDITS the account `net × 1e10` wei. The line's facade emits
///         `Bought` / `Sold` / `Refused` at `C` (a system receipt at index 0 of `C`'s EVM
///         block), and the precompiles at `C` already show the moved row. The value never
///         reaches the fold before `C`, which is what makes a refusal a plain refund.
///
///         Carriers first, then EVM actions, so the EVM is never the faster door to the same
///         curve (the MISAKA form of HyperEVM's delayed actions). Below the `palw_model_evm`
///         fence the address is an empty account and accepts nothing.
interface IMisakaModelWriter {
    /// Queue one action. `data` = `[1] ‖ [action id, 3 bytes BE] ‖ abi` as laid out above;
    /// `msg.value` = the gross leg in wei for a buy or the whole seed for a seed (nonzero, a
    /// multiple of 1e10, a seed at least SEED_MIN_SOMPI × 1e10), 0 for a sell. Returns nothing; the outcome is the fold's and settles at `C`.
    function sendAction(bytes calldata data) external payable;

    /// Emitted at the call, once per accepted action, from `0x…F013` only. `account` is the
    /// signing account (`tx.origin`), `actionId` 1 buy / 2 sell / 3 seed, `data` the payload as sent.
    /// The block's action list is exactly these logs, in log order.
    event ActionQueued(address indexed account, uint8 actionId, bytes data);

    /// `msg.sender != tx.origin`, or `msg.sender` has code (ADR-0089 Decision 4). The same
    /// selector `IMRC20` declares for `buy` / `sell` — one rule, one selector, declared in each
    /// interface so that each file stands alone (do not inherit both interfaces into one
    /// contract; nothing real needs to).
    error NotAnAccount();
    /// ADR-0090: the seed is under the network's least seed.
    error SeedTooSmall();
}
