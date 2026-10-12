// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMisakaModelPosition
/// @notice The window onto positions (ADR-0087 Decision 1: a balance in the state fold, keyed
///         by line and by a 64-byte holder id), served natively at
///         `0x000000000000000000000000000000000000F012` (ADR-0089 Decisions 1–2, 7).
///
///         TWO HOLDER NAMESPACES, AND NEITHER REACHES THE OTHER (ADR-0089 Decision 7). A
///         carrier-side holder is the payout payload of an ML-DSA-87 bond key. An EVM
///         account's holder id is `evm_holder_v1(chain_id, address)` — a keyed BLAKE2b-512
///         over `chain_id ‖ address`, NOT computable in Solidity; `holderIdOf` returns it. A
///         position bought from the EVM is sold from the EVM; one bought by a carrier is sold
///         by a carrier; no object and no action moves units between the two, because that
///         would be the transfer ADR-0087 Decision 5 forbids. An EVM-held position carries
///         ADR-0023's `CLASSICAL-ECC` security label; the fold's own positions keep the PQ
///         domain.
///
///         Every read is a row of `fold(selected_parent(B))` for the EVM block `B` the call
///         runs in: a position bought at chain block `B` is readable one block later, at its
///         selected child (Decision 6). Unknown line or holder ⇒ 0. Amounts are in UNITS
///         (10^6 units = one position). Every 64-byte id is two `bytes32` words, high half
///         first. Malformed input reverts and consumes the frame's gas; below the
///         `palw_model_evm` fence the address is an empty account.
interface IMisakaModelPosition {
    /// Units held on the line by any 64-byte holder id — a bond's payout payload or an EVM
    /// holder id from `holderIdOf`.
    function balanceOf(bytes32 lineA, bytes32 lineB, bytes32 holderA, bytes32 holderB)
        external
        view
        returns (uint64);

    /// Units held on the line by an EVM account: `balanceOf(line, holderIdOf(holder))`.
    function balanceOfAddress(bytes32 lineA, bytes32 lineB, address holder) external view returns (uint64);

    /// The line's supply in units — `PALW_MODEL_SUPPLY_UNITS_V1`, fixed at opening: what the
    /// curve holds plus what every holder of either namespace holds.
    function totalSupply(bytes32 lineA, bytes32 lineB) external view returns (uint64);

    /// Units ever bought on the line, cumulative across both namespaces (a sell does not
    /// reduce it) — the market row's `soldUnits`.
    function sold(bytes32 lineA, bytes32 lineB) external view returns (uint64);

    /// The 64-byte holder id of an EVM account: `evm_holder_v1(chain_id, holder)`. Injective
    /// on (chain id, address) up to the hash; the same address on another chain id is another
    /// holder.
    function holderIdOf(address holder) external view returns (bytes32 holderA, bytes32 holderB);
}
