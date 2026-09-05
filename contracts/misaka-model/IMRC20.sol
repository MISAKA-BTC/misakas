// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMRC20 — MISAKA Request for Comment 20: a model position
/// @notice One native facade per LINE (ADR-0089 Decision 3), at
///         `0x4d50 ‖ blake2b_512("misaka-evm/model-position-facade/v1" ‖ line_id)[..18]`
///         ("MP" and 18 bytes of a hash Solidity cannot compute — read the address from
///         `IMisakaModelRegistry.facadeOf`). ERC-20's READ half, kept, and the CURVE in place of
///         its TRANSFER half. There is no bytecode at the address: `EXTCODESIZE` is 0,
///         `EXTCODEHASH` the empty-code hash; the view selectors are served as a precompile
///         from `fold(selected_parent(B))`, and `buy` / `sell` are call-frame intercepts —
///         the same two `IMisakaModelWriter` actions (ids 1 and 2) with the line filled in by
///         the address. One write path, two doors.
///
///         WHAT IS ABSENT BY DEFINITION (ADR-0087 Decision 5, ADR-0089 Decision 4): there is
///         no transfer, so there is no `Transfer` event and no `Approval` event, and the four
///         ERC-20 transfer-half selectors REVERT `NonTransferable()`:
///           - `transfer(address,uint256)`                  0xa9059cbb
///           - `transferFrom(address,address,uint256)`      0x23b872dd
///           - `approve(address,uint256)`                   0x095ea7b3
///           - `allowance(address,address)`                 0xdd62ed3e
///         and `supportsInterface(0x36372b07)` — the ERC-20 interface id — is FALSE, on
///         purpose. An ERC-20 tool that only reads works; one that transfers fails at the
///         selector, loudly; and no contract can be written around it, because A HOLDER IS THE
///         SIGNING ACCOUNT AND ONLY THE SIGNING ACCOUNT: `buy` and `sell` revert
///         `NotAnAccount()` unless `msg.sender == tx.origin` and `msg.sender` has no code, so
///         no contract — no wrapper, no delegate, no multisig, no account-abstraction wallet —
///         ever holds or moves a position, and a position's only counterparty is the curve.
///         `supportsInterface(type(IMRC20).interfaceId)` is true.
///
///         UNITS. `decimals()` is 6: one position is 10^6 units, and every unit amount here is
///         ADR-0087's on the wire. MSK amounts in the views and events are in SOMPI (1 MSK =
///         1e8 sompi), the fold's unit. `msg.value` on `buy` is the one WEI quantity: it must
///         be a NONZERO MULTIPLE of 1e10 wei (`EVM_NATIVE_SCALE`, the F002 rule) so the fold's
///         sompi leg is exact; `buy` reverts `BadValue()` otherwise.
///
///         SETTLEMENT (ADR-0089 Decision 6). A call to `buy` / `sell` in the EVM block of chain
///         block `B` QUEUES an action and returns nothing: the call's own receipt carries only
///         `IMisakaModelWriter.ActionQueued` (from `0x…F013`). The fold applies the action at
///         `fold(B)` — after every carrier-borne buy and sell of `B`, in queue order, quoted on
///         the row as it then stands, `min` checked, refused rather than partially filled — and
///         the outcome SETTLES in the EVM block of `C`, the selected child of `B`: a filled buy
///         burns the escrowed value into the line's sink output, a refused buy is refunded to
///         the account, a filled sell credits the account `net × 1e10` wei. `Bought` / `Sold`
///         / `Refused` are emitted BY THIS FACADE AT `C`, in a system receipt at index 0 of
///         `C`'s EVM block, never at the call — and `balanceOf` reflects the fill at `C`.
///
///         Below the `palw_model_evm` fence, and at any prefixed address that names no line,
///         the address is an empty account.
interface IMRC20 {
    // ---- ERC-20's read half, kept ----

    /// "MISAKA Model Position <8 hex of line id>" (the exact text is the native code's;
    /// ADR-0089 §10 leaves it open beyond the shape).
    function name() external view returns (string memory);

    /// "MP-<8 hex of line id>" (same caveat as `name`).
    function symbol() external view returns (string memory);

    /// 6 — one position is 10^6 units (ADR-0087 Decision 1).
    function decimals() external view returns (uint8);

    /// `PALW_MODEL_SUPPLY_UNITS_V1`, in units, fixed at the market's opening: the curve's
    /// units plus every holder's in either namespace.
    function totalSupply() external view returns (uint256);

    /// Units held by an EVM account — the EVM namespace ONLY (ADR-0089 Decision 7). A bond's
    /// position is not visible here; see `IMisakaModelPosition.balanceOf`.
    function balanceOf(address holder) external view returns (uint256);

    // ---- the curve, in place of the transfer half ----

    /// The line this facade is the face of, 64 bytes as two words, high half first.
    function lineId() external view returns (bytes32 lineA, bytes32 lineB);

    /// Units ever bought, every namespace — the market row's `soldUnits` (a sell does not
    /// reduce it).
    function circulating() external view returns (uint256);

    /// Sompi per whole position at the selected parent.
    function price() external view returns (uint256);

    /// Quote a buy of `mskInSompi` sompi gross (`IMisakaModelAMM.quoteBuy`'s `unitsOut` and
    /// `priceAfter`, against the selected parent's row).
    function quoteBuy(uint256 mskInSompi) external view returns (uint256 unitsOut, uint256 priceAfter);

    /// Quote a sell of `unitsIn` units (`IMisakaModelAMM.quoteSell`'s `net` and `priceAfter`).
    function quoteSell(uint256 unitsIn) external view returns (uint256 mskOutSompi, uint256 priceAfter);

    /// Queue a buy: = `IMisakaModelWriter` action 1 with this line. `msg.value` is the GROSS
    /// MSK leg in wei — a nonzero multiple of 1e10 — held in escrow at `0x…F013` until `C`.
    /// `minUnitsOut` (units) is checked in the fold: a worse fill is refused, never partial.
    /// Reverts at the call: `NotAnAccount()`, `BadValue()`, `ClosedToBuys()` (a retired line —
    /// ADR-0087 D7 / ADR-0088 D6), or the block's 128-action cap.
    function buy(uint256 minUnitsOut) external payable;

    /// Queue a sell: = `IMisakaModelWriter` action 2 with this line. `unitsIn` units are the
    /// fold's and are debited there (nothing is escrowed); `minMskOutSompi` is the least NET
    /// sompi accepted, checked in the fold. A retired line still queues sells. Reverts at the
    /// call: `NotAnAccount()`, `BadValue()` (a sell of zero), or the block's 128-action cap.
    function sell(uint256 unitsIn, uint256 minMskOutSompi) external;

    /// ERC-165. True for `type(IMRC20).interfaceId`; FALSE for 0x36372b07 (ERC-20): this is a
    /// position, not a token.
    function supportsInterface(bytes4 interfaceId) external view returns (bool);

    // ---- settlement events, emitted at the settling block C (Decision 6), never at the call ----

    /// A buy filled: `mskIn` sompi gross (the escrow burned into the line's sink), `unitsOut`
    /// units credited to `holder`'s EVM-namespace position, `priceAfter` sompi per position.
    event Bought(address indexed holder, uint256 mskIn, uint256 unitsOut, uint256 priceAfter);

    /// A sell filled: `unitsIn` units debited, `mskOut` NET sompi credited to `holder` in the
    /// EVM (`mskOut × 1e10` wei, a credit with no lock and no tip), `priceAfter` as above.
    event Sold(address indexed holder, uint256 unitsIn, uint256 mskOut, uint256 priceAfter);

    /// An action refused by the fold: `actionId` 1 (buy: `amount` = the gross sompi, refunded
    /// to `holder` at `C`) or 2 (sell: `amount` = `unitsIn`, which never left `holder`).
    /// `reason` is the fold's refusal reason as a `bytes32` tag (the tag set is the native
    /// code's — a `min` not met, a closed market, insufficient units, …). A refusal costs the
    /// caller the call's gas and nothing else.
    event Refused(address indexed holder, uint8 actionId, uint256 amount, bytes32 reason);

    // ---- errors ----

    /// `transfer`, `transferFrom`, `approve`, `allowance`: there is no transfer.
    error NonTransferable();

    /// `buy` / `sell` from anything but the signing account (`msg.sender != tx.origin`, or
    /// `msg.sender` has code). The same selector `IMisakaModelWriter` reverts with — one rule,
    /// declared in each interface so that each file stands alone. A contract that inherits BOTH
    /// interfaces trips solc's "identifier already declared"; nothing real does (the writer is
    /// one address, a facade is one line), so implement the functions and declare it once.
    error NotAnAccount();

    /// `buy` on a line whose market is closed to buys (the line is retired).
    error ClosedToBuys();

    /// `buy` with a `msg.value` that is zero or not a multiple of 1e10 wei; `sell` of zero
    /// units.
    error BadValue();
}
