# ADR-0089 — the fold is the truth; the EVM is its window and its hand

**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05, on top of ADR-0087 and ADR-0088: Model Positions are not to be "a mere copy of
ERC-20" (ERC-20 の単なるコピーにせず); `ModelPosition`, `ModelRegistry` and `ModelAMM` are to be
**native primitives of the MISAKA EVM** — the thing that is MISAKA's own reason to exist
(Misaka 独自の存在意義); the current ERC is to be reshaped into an EVM design specialised for
trading model positions; and Hyperliquid's HyperEVM is the reference.
**Builds on:** ADR-0020 (the selected-parent EVM lane — live on testnet-10, testnet-11 and
devnet from genesis, inert on mainnet), ADR-0022 (the EVM snapshot at the pruning point),
ADR-0023 (Lane 2; per-lane precompile profile; the frozen security labels), ADR-0087 (the market
is a balance in the state fold; implemented behind `palw_model_market`), ADR-0088 (lines, versions,
roles, usage; the market keyed by line), ADR-0059 (carve, never mint), `docs/misaka-evm-design-v0.4.md` §3.2 (system
ops), §8 (fees), §9 (the UTXO ⇄ EVM bridge), §17 (invariants).
**Amends:** ADR-0087 Decision 3 (two more ways to make the same two moves — from the EVM),
Decision 8 (what a participant reads gains the EVM's surfaces); design v0.4 §9.1's supply
invariant (it names the market's sinks and the writer's escrow). ADR-0087 Decision 5 (no
transfer) is not amended: this ADR is the proof that the EVM cannot manufacture one.

## 0. The sentence this ADR is

The PALW state fold holds every fact about a model — its class, its line and its owner, the
versions in force and how each was used, its curve, every position — and that is where they stay. The EVM gets **three windows**
(precompiles that return the fold's rows as facts) and **one hand** (a system contract whose
calls become fold actions, applied after the block), and every class gets an address at which
the ERC-20 *read* surface works and the ERC-20 *transfer* surface does not exist. That is what
HyperEVM did with an order book. MISAKA does it with a model registry — lines, their versions,
their owners, their usage — and a curve: things no other EVM can read as consensus facts, because
no other chain holds them as consensus.

## 1. The reference: what HyperEVM built (read 2026-09-05, Hyperliquid's own docs)

| HyperEVM (Hyperliquid) | what it settled |
|---|---|
| "The HyperEVM consists of EVM blocks built as part of Hyperliquid's execution, inheriting all security from HyperBFT consensus." Cancun without blobs; chain id 999 / 998; HYPE is gas; base **and** priority fees burned. | The EVM is not a second chain and has no bridge trust: its blocks are part of L1 execution. |
| **Read precompiles** from `0x0000000000000000000000000000000000000800` upward (`L1Read.sol`: position 0x800, spotBalance 0x801, vaultEquity 0x802, withdrawable 0x803, delegations 0x804, delegatorSummary 0x805, markPx 0x806, oraclePx 0x807, spotPx 0x808, l1BlockNumber 0x809, perpAssetInfo 0x80a, spotInfo 0x80b, tokenInfo 0x80c, tokenSupply 0x80d, bbo 0x80e, accountMarginSummary 0x80f, coreUserExists 0x810). "The values are guaranteed to match the latest HyperCore state at the time the EVM block is constructed." Gas `2000 + 65 × (input_len + output_len)`; invalid inputs "return an error and consume all gas passed into the precompile call frame". | Core state is read **synchronously**, as a fact of the block, with no oracle in between. |
| **CoreWriter** at `0x3333333333333333333333333333333333333333`, `sendRawAction(bytes)`: byte 1 = encoding version, bytes 2–4 = action id, the rest = ABI data; action ids 1 limit order … 17 outcome operation; "burns ~25,000 gas before emitting a log", a basic call "~47000"; "order actions and vault transfers sent from CoreWriter are delayed onchain for a few seconds" — "to prevent any potential latency advantages for using HyperEVM to bypass the L1 mempool". | Core state is written **asynchronously**, by an action the core executes after the block, with no return value in the EVM and a deliberate delay. |
| **System addresses**: each token's is `0x20` followed by zeros and the token index big-endian (`0x20000000000000000000000000000000000000c8` for index 200); HYPE's is `0x2222222222222222222222222222222222222222`. EVM → Core is an ERC-20 `transfer` to the system address; Core → EVM is a system transaction calling `transfer(recipient, amount)` on the *linked* contract. "A token's system address must have the total non-system balance on the other side." "There are currently no checks that the system address has sufficient supply or that the contract is a valid ERC20, so be careful when sending funds." | An asset crosses by moving to a known address; linking is explicit; the other side's supply is the system address's balance — and the docs say plainly what is not checked. |
| **Dual blocks**: fast blocks every 1 s at 3M gas, slow blocks every 1 min at 30M gas, interleaved in one increasing sequence of EVM block numbers; a user opts in with `{"type": "evmUserModify", "usingBigBlocks": true}`; the mempool "accepts only the next 8 nonces for each address" and prunes after a day. | Throughput and latency are allocated separately. |
| The same 20-byte address is the account on both sides. | Identity is shared, so bridging is a transfer to a system address and nothing more. |

Sources: `hyperliquid.gitbook.io/hyperliquid-docs/for-developers/hyperevm` (overview),
`…/hyperevm/interacting-with-hypercore` (precompiles, CoreWriter, action table),
`…/hyperevm/hypercore-less-than-greater-than-hyperevm-transfers` (system addresses, linking),
`…/hyperevm/dual-block-architecture`. Read on 2026-09-05; numbers move with their releases and
are cited here for the shape, not the values.

## 2. What exists here

* **The MISAKA EVM is ADR-0020's lane, and it is live on the PALW network.** revm pinned to
  `SpecId::SHANGHAI`; chain id `0x4D534B`; `EVM_NATIVE_SCALE = 10^10` (sompi → wei);
  `EVM_GAS_LIMIT = 30,000,000` per chain block, `MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK = 128 KiB`;
  one EVM block per selected-chain block (`evm_number(B) = evm_number(selected_parent(B)) + 1`);
  header fields `evm_payload_hash` and `evm_commitment_root`, version-gated (`EVM_HEADER_VERSION
  = 2`). `Params::evm_activation_daa_score` is `0` on `TESTNET_PARAMS` and `DEVNET_PARAMS`,
  `u64::MAX` on mainnet and simnet — and `TESTNET11_PARAMS` is `..TESTNET_PARAMS`
  (`consensus/core/src/config/params.rs:4951`), so testnet-11, the PALW RC, runs the EVM lane from genesis, and the fleet
  builds it (`contrib/local-desktop-join/scripts/misaka-desktop-node.sh:405`: `cargo build --release -p kaspad --features evm`) —
  and since 2026-08-21 so does every build: `kaspad/Cargo.toml:93` is `default = ["evm"]`
  ("the EVM lane is part of the node, not a bolt-on"), a non-`evm` build refuses an EVM-active
  network at boot (`ConfigError::EvmLaneRequiresEvmBuild`, `kaspad/src/daemon.rs:133`), and ADR-0020's
  "secp-free by default" and ADR-0087 §1's "an optional, non-default feature" are both stale
  against the tree. The lane is still a separate secp256k1 signature domain (ADR-0023's label
  for it: `CLASSICAL-ECC` / `ETH-COMPATIBLE`); an EVM tx is not a Kaspa transaction but a raw
  EIP-2718 envelope in `Block.evm_payload.transactions`, recovered to a 20-byte address by
  `TxEnvelope::recover_signer` (`kaspa-evm/src/tx.rs:196-213`), reaching a block through
  `eth_sendRawTransaction` → `evm_mempool` → the payload.
* **The EVM already has system addresses and a bridge, and one registration seam.**
  `register_all_misaka_precompiles(handler, f003_active)` (`kaspa-evm/src/precompiles.rs:23`)
  is the single place consensus and `eth_call` install MISAKA's handlers, so the two cannot
  diverge. `0x…F001` `WMISAKA_ADDRESS` is declared and deployed nowhere (the genesis state root
  is the empty trie); `0x…F003` ML-DSA-87 verify exists (`F003_VERIFY_GAS = 500,000`) and is
  inert on every network; `0x…F002` `MISAKA_WITHDRAW` is live and is **not a precompile but a
  call-frame intercept** — `kaspa-evm/src/withdraw.rs:1-8`: "the stateless precompile ABI
  receives only `(input, gas_limit, ctx)` and cannot see `msg.sender` / `msg.value`". It costs
  `F002_WITHDRAW_GAS = 9,000`, journals a `MisakaWithdraw(address,uint256,bytes)` log, and the
  executor scans the block's committed logs into `WithdrawOp`s that `apply_evm_bridge_effects`
  (`consensus/src/processes/evm/mod.rs:211-258`) turns into a synthetic UTXO output under the
  domain `MISAKA_EVM_SYNTHETIC_OUTPOINT_V2`, *in the accepting block's own UTXO diff*; user
  faults revert the tx and the block stands. That log-scan → op → diff path is the only channel
  today by which the EVM writes anything outside itself. Deposits are two-step: an
  `EVM_DEPOSIT_LOCK` output (a push-and-drop no-op over an ML-DSA P2KH refund script,
  `crypto/txscript/src/script_class.rs:295-311`) on subnet `0x20`, then a producer-selected `DepositClaim {
  deposit_outpoint, evm_address, amount_sompi, claim_tip_sompi }` system op in a block's
  payload — the only `EvmSystemOp` kind there is — verified against `selected_parent(B)`'s
  UTXO view, crediting `(amount − claim_tip) × EVM_NATIVE_SCALE`. System ops run before user
  txs (v0.4 §3.2), at most 256 claims / 64 KiB / 10M system gas per EVM block (§9.2). Supply:
  `UTXO_balances + EVM_deposit_locks + EVM_native_balances + burned == issued` (§9.1), checked
  in O(1) by the `evm_total_native_balance` accumulator.
* **The order inside one chain block is fixed and is the order this ADR needs.** For a
  selected-chain candidate `B`, `calculate_utxo_state_relatively` runs `calculate_utxo_state`,
  then `evm_chain_context_step` (claims, mergeset acceptance execution, `evm_commitment_root`,
  and the bridge's UTXO side-effects folded into the block's context *before*
  `verify_expected_utxo_state`), then the UTXO verification, then the PALW transition
  (`apply_palw_transition_v6`) — `consensus/src/pipeline/virtual_processor/processor.rs:1435` before `:1625`. The EVM's result for `B`
  is a function of `B`'s parents and `B`'s system ops, never of `B`'s own fold (v0.4 I2, I9);
  the fold for `B` may therefore take the EVM's output as an input without a cycle.
* **The market is in the fold, implemented.** ADR-0087: `PalwModelMarketV1`, position rows
  keyed by `(class_id, holder: Hash64)`, `holder` = the payout payload of an ML-DSA-87 key
  (the identity a bond pays); objects `ModelBuy` / `ModelSell` on carrier transactions; a buy
  pays the class's sink (`OP_RETURN ‖ "MSKMDL01" ‖ class_id`, unspendable); a sell writes a
  `PalwPayoutV2` the coinbase honours; the rows enter the state root only when non-empty; the
  market opens at its first buy; `palw_model_market: Option<ForkActivation>`, `None` everywhere.
  ADR-0088 keys the market by *line* (a class, an owner, a name; the founding line's key is the
  class id) and adds, per line, the owner/developer/maintainer roles, the versions with their
  declared hashes and chain-counted usage, proposals and declared evaluations — all fold rows,
  all bond-signed on the UTXO side.
* **Two key domains, no shared address.** A fold identity is a 64-byte hash of an ML-DSA-87
  key; an EVM account is a 20-byte secp256k1 address. Hyperliquid's "same address on both
  sides" does not exist here and cannot be made to: ADR-0023 FD-SUPPLY calls a move from the
  PQ domain into Lane 2 "a security downgrade" that wallets must display.
* **Prior art that is not revived, and a coupling that does not exist.** The Token Program
  of 2026-08-10/11 (`consensus/core/src/token.rs`: an SPL-style `TokenAccount { balance, nonce }`
  ledger inside consensus, ML-DSA-87 transfer and burn, subnets `0x30`/`0x31`) is in the tree
  behind inert fences (`TokenParams::INERT` on every network) and never touched the EVM — its
  own design rejected "an EVM predeploy" (案 C) in favour of the consensus ledger. This ADR adds
  no ledger: the ledger is ADR-0087's, and the EVM gets views of it. And today there is no
  coupling at all: no `palw` symbol under `kaspa-evm/`, `consensus/core/src/evm/`,
  `consensus/src/processes/evm/` or `rpc/eth/`, and no `Evm` symbol in any of the 74
  `palw_*.rs` files; the one place the two meet is `misaka-palw-derive`'s runner, which executes
  model-written initcode "on an ephemeral, isolated state … never against the chain's EVM
  state and never inside the node process". The fold's rows do travel with a pruned node: the
  pruning-point snapshot flow serves a `PruningPointPalwStateMessage` beside the EVM's
  (`protocol/flows/src/v8/request_pruning_point_snapshots.rs:13`), so a window opened after pruned IBD has something
  to look at.

## 3. The requirement, and what "native" has to mean

The operator's three words name three things the fold already holds: the **registry** (class
rows; lines, versions, roles, usage — ADR-0056/0067/0088), the **AMM** (the curve, the reserve,
the fees — ADR-0087 Decisions 2–4), the **position** (a balance keyed by holder — ADR-0087
Decision 1).
"Native primitives of the EVM" can mean two things, and only one of them is right:

* **Move the truth into the EVM** — system contracts holding the reserve, the positions and
  the class table in keccak-MPT state, with Solidity-shaped rules. Rejected: the lane is
  `u64::MAX` on mainnet, so the market would not exist where the EVM does not ("optional
  feature" is no longer the reason — the lane is a default build since 2026-08-21 — the network
  fence is); its keys are secp256k1, so every position would leave the PQ domain;
  ADR-0087's M1–M8 and ADR-0088's L1–L10 are fold invariants proven against fold objects, and a
  version, a role change, a transfer and a certification are ML-DSA-87 bond signatures that no
  EVM account can produce; and a second copy of the class table is the two-products split
  ADR-0077 R0 exists to close.
* **Keep the truth in the fold and make the EVM its window and its hand** — the HyperEVM
  shape: precompiles that return fold rows, a writer whose calls become fold actions applied
  after the block, and per-class addresses at which a wallet's ERC-20 reader works. "Native"
  then means what it means on HyperEVM: implemented by the node's executor, no bytecode, the
  truth elsewhere. This is what the ADR decides.

And the ERC is reshaped, not copied: the position's token face keeps ERC-20's *read* half
(`name`, `symbol`, `decimals`, `totalSupply`, `balanceOf`, the events) and replaces its
*transfer* half with the curve (`price`, `quoteBuy`, `quoteSell`, `buy`, `sell`). It answers
`supportsInterface(ERC-20) == false` on purpose. An ERC-20 tool that only reads works; one that
transfers fails at the selector, loudly, and no contract can be written around it (§4 D6).

## 4. Decisions

**Decision 1 — Four system addresses in MISAKA's own range, and a facade family.** Following
`0x…F001/F002/F003`:

| address | name | kind |
|---|---|---|
| `0x…F010` | `ModelRegistry` | read precompile |
| `0x…F011` | `ModelAMM` | read precompile (quotes are pure functions of a fold row) |
| `0x…F012` | `ModelPosition` | read precompile |
| `0x…F013` | `ModelWriter` | the hand: `sendAction(bytes)`, escrow account — a call-frame intercept, F002's shape |
| `0x4d50 ‖ blake2b(b"misaka-evm/model-position-facade/v1" ‖ line_id)[..18]` | the line's **MRC-20 facade** | a native contract at a derived address (Decision 3): reads as a precompile, writes as an intercept |

Two mechanisms, chosen by what a call needs to see. The three reads and the facade's view
selectors are stateless precompiles installed through `register_all_misaka_precompiles` with a
handle on the selected parent's fold rows — input in, bytes out. The writer and the facade's
`buy`/`sell` are call-frame intercepts, because "the stateless precompile ABI … cannot see
`msg.sender` / `msg.value`" (`kaspa-evm/src/withdraw.rs:1-8`) and Decisions 4–5 need both.

Hyperliquid's `0x0800…` and `0x2000…`/`0x3333…` ranges are not reused, so a contract ported
from HyperEVM cannot silently call the wrong thing. The facade's 2-byte prefix (`MP`) is
Hyperliquid's `0x20 ‖ token index` idea with a hash where an index would be a free field.
`ModelRegistry.facadeOf(line_id)` and `lineOf(address)` map both ways; an address with the
prefix that names no line behaves as an empty account. A line is ADR-0088's unit of the market —
a class, an owner, a name — and the founding line of a class has the class id as its line id.

**Decision 2 — What a read returns is the fold at the EVM block's selected parent, stated as
Hyperliquid states it.** Every read precompile returns rows of `fold(selected_parent(B))` — the
post-state of the selected parent's transition — because that is the one fold state that is a
function of `B`'s parents alone (v0.4 I2/I9). The guarantee, in the reference's words with the
referent changed: *the values are guaranteed to match the fold's state at the selected parent
of the EVM block.* A read of a class that does not exist, a window that was never opened or a
holder that has no row returns the zero value (not an error) — the zero is itself a fact.
Malformed input reverts and consumes the frame's gas, Hyperliquid's rule. Gas: `2,000 + 65 ×
(input_len + output_len)`, the reference's schedule taken as the starting point and re-measured
against `EVM_GAS_LIMIT` before the fence is armed (§5).

`ModelRegistry` (ADR-0056/0067/0075/0088 rows; every 64-byte id is two `bytes32` words): `classCount()`, `classAt(i)`, `classRow(class)` → `(status, sharePermille, budgetBlocks, canonicalLeaves, isBase, registrantPayload, registeredDaa)`, `certified(class, lane)`, `rootsInForce(class)` → the count and `rootInForceAt(class, i)`; `lineCount()`, `lineAt(i)`, `linesOf(class)` → count and `lineOfClassAt(class, i)`, `line(line)` → `(class, ownerPayload, developerPayload, maintainerPayload, current, versionsPublished, previewCount, contributorPermille, status, nameHash)`, `version(line, n)` → `(root, parent, adoptedFrom, runtimeHash, datasetCommitment, trainingConfigHash, notesHash, publishedDaa, publishedByPayload, status, untilDaa)`, `usage(line, n)` → `(attemptClaims, fpClaims, workLeaves, firstUsedDaa, lastUsedDaa)`, `evaluationCount(line, n)` and `evaluation(line, n, i)` → `(evaluatorId, scorePermille, reportHash, byPayload, postedDaa, isLinesOwn)`, `proposalCount(line)` and `proposal(line, i)` → `(proposalId, root, noteHash, byPayload, postedDaa, adoptedIn)`, `facadeOf(line)`, `lineOf(address)`, `chainDaa()` (the selected parent's DAA score — the fold's clock, the analogue of `l1BlockNumber`). Declared values are returned as declared and labelled so in the Solidity interface; nothing in the EVM treats a score as a fact.

`ModelAMM` (ADR-0087 rows and arithmetic, keyed by line): `market(line)` → `(openedDaa,
mskReserve, positionUnits, soldUnits, burned, ownerPaid, contributorPaid, closedToBuys, exists)`,
`price(line)` (sompi per position), `quoteBuy(line, mskIn)` → `(unitsOut, burn, leg, net,
priceAfter)`, `quoteSell(line, unitsIn)` → `(mskOut, burn, leg, net, priceAfter)`, `constants()`
→ `(supplyUnits, unitsPerPosition, virtualSompi, burnPermille, legPermille)`. The quotes call the
same `palw_model_buy_quote_v1` / `palw_model_sell_quote_v1` the fold calls, so a quote is the
fold's arithmetic and not a re-implementation of it. A quote is a quote against the selected
parent's row; Decision 7 says what a trade actually gets.

`ModelPosition`: `balanceOf(line, holderId)` for any 64-byte holder id, `balanceOfAddress
(line, address)` for the EVM namespace (Decision 7), `totalSupply(line)`, `sold(line)`,
`holderIdOf(address)`.

**Decision 3 — The MRC-20 facade: ERC-20's read half, the curve in place of its transfer half.**
At each line's facade address the executor implements, natively, the interface below; there
is no bytecode, `EXTCODESIZE` returns 0 and `EXTCODEHASH` the empty hash, exactly as for a
precompile, and the registry is how a caller learns that the address is live.

```solidity
interface IMRC20 /* MISAKA Request for Comment 20: a model position */ {
    // ---- ERC-20's read half, kept ----
    function name() external view returns (string memory);      // "MISAKA Model Position <8 hex of line id>"
    function symbol() external view returns (string memory);    // "MP-<8 hex>"
    function decimals() external view returns (uint8);          // 6: one position is 10^6 units (ADR-0087 D1)
    function totalSupply() external view returns (uint256);     // PALW_MODEL_SUPPLY_UNITS_V1, fixed at opening
    function balanceOf(address holder) external view returns (uint256); // EVM-namespace units only (D8)
    // ---- the curve, in place of the transfer half ----
    function lineId() external view returns (bytes32, bytes32); // the line, 64 bytes as two words
    function circulating() external view returns (uint256);     // sold_units, every namespace
    function price() external view returns (uint256);           // sompi per position
    function quoteBuy(uint256 mskInSompi) external view returns (uint256 unitsOut, uint256 priceAfter);
    function quoteSell(uint256 unitsIn) external view returns (uint256 mskOutSompi, uint256 priceAfter);
    function buy(uint256 minUnitsOut) external payable;         // = ModelWriter action 1 (D6); msg.value is the gross leg
    function sell(uint256 unitsIn, uint256 minMskOutSompi) external; // = ModelWriter action 2 (D6)
    // ---- settlement events, emitted at the settling block (D7), never at the call ----
    event Bought(address indexed holder, uint256 mskIn, uint256 unitsOut, uint256 priceAfter);
    event Sold(address indexed holder, uint256 unitsIn, uint256 mskOut, uint256 priceAfter);
    event Refused(address indexed holder, uint8 actionId, uint256 amount, bytes32 reason);
    // ---- absent by definition: transfer, transferFrom, approve, allowance, Transfer, Approval ----
    //      those four selectors revert NonTransferable(); supportsInterface(0x36372b07 /* ERC-20 */) == false
}
```

`decimals() == 6` and `totalSupply()` in units keep ADR-0087's arithmetic on the wire
unchanged; `msg.value` is in wei and must be a multiple of `EVM_NATIVE_SCALE` (the F002 rule),
so the fold's sompi leg is exact. `supportsInterface(type(IMRC20).interfaceId) == true`. A
facade for a line whose market is `closed_to_buys` (ADR-0087 D7; a retired line, ADR-0088 D6)
reverts `buy` at the call and still queues `sell`.

**Decision 4 — There is no `Transfer` because there is no transfer, and the EVM cannot build
one.** ADR-0087 Decision 5 is a fold fact: no object moves units between holders. On the EVM
that fact has to survive two constructions that would rebuild a transfer from the outside —
the wrapper (a contract buys positions and issues its own transferable shares) and the
delegate (a contract sells on a holder's behalf). Both die on one rule: **a holder is the
signing account and only the signing account** — `ModelWriter` and every facade write refuse a
call unless `msg.sender == tx.origin` and `EXTCODESIZE(msg.sender) == 0`
(`NotAnAccount`), so no contract ever holds or moves a position, and a position's only
counterparty is the curve. The cost is stated: no contract wallet, no multisig, no account
abstraction can hold a position on this lane; EIP-7702 is not in Shanghai and, if a later fork
admits it, the code-size clause is what keeps the rule. Hyperliquid lets contracts hold core
balances and act through CoreWriter; MISAKA cannot, because ADR-0087's constraint is that a
position never passes between persons, and a contract that holds one is a person-shaped hole
in that constraint.

**Decision 5 — `ModelWriter` is CoreWriter's shape with two actions.** `sendAction(bytes data)
payable`: byte 1 = encoding version (`1`), bytes 2–4 = action id big-endian, the rest = ABI:

| id | action | ABI | value |
|---|---|---|---|
| 1 | buy | `(bytes32 lineA, bytes32 lineB, uint256 minUnitsOut)` | `msg.value` = the gross MSK leg, wei, a multiple of `EVM_NATIVE_SCALE` |
| 2 | sell | `(bytes32 lineA, bytes32 lineB, uint256 unitsIn, uint256 minMskOutSompi)` | 0 |
| 3–255 | reserved | — | — |

No action founds a line, publishes a version, sets a role, registers a class or certifies a
family: those are ML-DSA-87 bond signatures on the UTXO side (ADR-0056, 0075, 0088), and an EVM
account has no bond. The call burns `PALW_EVM_WRITER_GAS_V1 = 25,000` gas before emitting
`ActionQueued(address indexed account, uint8 actionId, bytes data)` (Hyperliquid's ~25,000 and
~47,000 for a basic call, taken as the starting schedule; F002's in-tree figure is 9,000). The
block's **action list** is built the way F002's withdrawals are: the executor scans the
committed logs of the accepted txs for `ActionQueued` from `0x…F013` and decodes each into a
`PalwEvmMarketActionV1` in log order — a reverted tx leaves no log and therefore no action, and
a forged log from any other address is not the writer's. A buy's `msg.value` is held in the
writer's own account (`0x…F013`) — the **escrow** — until the settling block; a sell holds
nothing, because the units are the fold's and are debited there. At most
`PALW_EVM_MARKET_ACTIONS_PER_BLOCK_V1 = 128` actions per EVM block: the 129th reverts at the
call. Malformed data, an unknown line, `msg.value` not a multiple of the scale, or a sell of
zero revert at the call — the same "user-input fault ⇒ tx revert, block valid" class as F002.
`facade.buy` / `facade.sell` are the two actions with the line filled in by the address, and
nothing else: one write path, two doors.

**Decision 6 — The hand moves after the block, in the fold, and settles one block later.**

| step | where | what happens |
|---|---|---|
| **emit** | `EVM(B)` — the EVM block of chain block `B`, executing the accepted payloads of `B`'s mergeset | the writer validates the call, escrows a buy's value, appends `PalwEvmMarketActionV1 { seq, account, action, line, amount, min }` to the block's **action list**, emits `ActionQueued` |
| **apply** | `fold(B)` — `apply_palw_transition_v6` gains the action list as an input | after every carrier-borne object of `B` (ADR-0087's `ModelBuy`/`ModelSell` in acceptance order), the EVM actions in `seq` order: each is quoted on the row *as it then stands*, the `min` is checked, the row and the holder's units move (a buy credits `evm_holder(account)`, a sell debits it) or the action is **refused** with a reason; the outcome is written to the delta as `PalwEvmSettlementV1 { seq, account, line, outcome: Filled { units, sompi, priceAfter } \| Refused { reason } }` — a state-root collection that enters the root only when non-empty (ADR-0087's rule) |
| **settle** | `EVM(C)`, `C = the selected child of B` — `evm_chain_context_step(C)` runs system ops before user txs (v0.4 §3.2) | a second `EvmSystemOp` kind, `MarketSettle { seq, account, line, outcome }`: the producer of `C` puts `fold(B)`'s settlement list into `C`'s payload `system_ops` in `seq` order, and validation requires the set to equal the list derived from the selected parent exactly — a missing, extra or reordered op disqualifies `C` as a bad claim does — so v0.4 I2 (the EVM result is a function of the parents and the system ops) holds unchanged and `system_ops_root` commits them. A filled buy **burns the escrow** (writer balance −= gross) and `apply_evm_bridge_effects` materialises the line's **sink output** (`OP_RETURN ‖ MSKMDL01 ‖ line`, value = gross) into `C`'s own UTXO diff under a new outpoint domain `MISAKA_EVM_MARKET_SINK_V1` — F002's mechanism with the sink script in place of the withdrawal script; a refused buy **refunds the escrow** to `account`; a filled sell **credits** `account` with `net × EVM_NATIVE_SCALE`, a `DepositClaim`-shaped credit with no lock and no tip; each emits `Bought` / `Sold` / `Refused` from the facade in a system receipt at index 0 of `C`'s EVM block |
| **visible** | `EVM(C)` and after | precompiles at `C` read `fold(B)`, which already holds the moved row: a position bought at `B` is readable, and settled, one chain block later |

Order and price: carriers first, then EVM actions, so an EVM trade is always quoted after
the block's UTXO-side trades — the MISAKA form of Hyperliquid's "delayed onchain for a few
seconds", and for the same reason: the EVM is not a faster door to the same book. `min`
protections refuse, never partially fill (ADR-0087 M5), and a refusal costs the caller its gas
and nothing else. The escrow's shape is what makes refusal safe: the value never reached the
fold, so a refused buy is a refund inside the EVM at `C`, not a coinbase payout.

**Decision 7 — Two holder namespaces, and neither reaches the other.** An EVM account's holder
id is `evm_holder_v1(chain_id, address) = blake2b_512_keyed(b"misaka-palw/model-market/holder/evm/v1", chain_id ‖ address)`,
a `Hash64` in the same `holder` field ADR-0087 keys positions by. A position bought from the
EVM is sold from the EVM; one bought by a carrier is sold by a carrier; no object moves units
between an ML-DSA holder and an EVM holder, because that would be the transfer ADR-0087 D5
forbids, in a costume. Payouts differ by namespace: a carrier holder's sell is a `PalwPayoutV2`
the coinbase honours (unchanged); an EVM holder's sell is a settlement credit at `C`
(Decision 6), and **the coinbase never emits it** — the fold's payout table gains
`PalwPayoutKindV2::EvmCredit { address }` beside the payload kind, and the coinbase skips that
kind while the EVM step consumes it. The security label of an EVM-held position is ADR-0023's
`CLASSICAL-ECC`, and `getPalwModelPositions` returns it per row so a wallet shows it; the
fold's own positions keep the PQ domain. The owner's and an adopted contributor's legs (ADR-0087 D4, ADR-0088 D8) are unchanged by
which door the trade came through: they are fold payouts to bonds' payloads.

**Decision 8 — Supply, stated with the new pools in it.** Design v0.4 §9.1 becomes

```
UTXO_balances + EVM_deposit_locks + EVM_native_balances (the writer's escrow included) + burned == issued
```

with two consequences the fold's books make visible: a filled EVM buy moves value from
`EVM_native` (the escrow) to `UTXO_balances` (a dead sink output) at `C`, so between `B` and `C`
the fold's `msk_reserve` says "received" one block before the sink exists — a one-block
receivable, the same lag a carrier's escrowed reward has before its coinbase; and a filled
EVM sell raises `EVM_native` by `net` at `C` and `issued` by the same, exactly as the coinbase
raises `issued` for a carrier holder's sell — the sink outputs on the UTXO side are dead by
script, and ADR-0087 M2 (`Σ msk_in = reserve + Σ payouts + burned + registrant + author`) is
the identity that says the re-issue never exceeds what was sunk. The
`evm_total_native_balance` accumulator (v0.4 §4.2) takes the escrow burn and the credits as
system-op deltas, so the O(1) check stays O(1).

**Decision 9 — The fence, and where it can be armed.** `palw_model_evm: Option<ForkActivation>`
on the params, top level, bare; `validate_palw_v2` refuses it unless `palw_model_market` is
armed at or before it and `evm_activation_daa_score ≤` it — on mainnet the lane is `u64::MAX`,
so the market's EVM face waits for ADR-0020's mainnet activation and says so; on testnet-11 and
devnet both preconditions hold today. Below the fence the four addresses and the facades are
empty accounts (the F003 idiom: "below the fence the handler is not registered, so a call is
byte-identical to calling an empty account"), the writer accepts nothing, the transition takes
an empty action list, and the fingerprint moves only where the flag is set. The lane's own
fences are bare DAA scores (`evm_*_activation_daa_score`, five of them); this one is the PALW
idiom's `Option<ForkActivation>` because `for_each_fence` visits it, and the executor reads the
same value through `Params::palw_model_evm_active_at(daa)`, so there is one switch, not two. No state-root
version bump: the settlement list and the EVM-namespace rows enter the root only when non-empty
(ADR-0087's implementation rule; ADR-0088 D11).

**Decision 10 — What HyperEVM has that this ADR does not take, and why.**
* *The asset crossing by system address.* A position never crosses: the facade has no supply,
  holds none, and links to nothing — there is no "total non-system balance on the other side"
  to keep, and so nothing that "is not checked". Only MSK crosses, and it crosses by the bridge
  that exists (deposit lock/claim, F002, and now the escrow/settlement pair).
* *Contracts as core actors.* Refused by Decision 4.
* *Dual blocks.* The EVM block cadence is the selected chain's, and the chain's interval is
  `header.bits`' (ADR-0071 as amended); a market action is one small tx, and the registry's
  writes do not run in the EVM.
* *The same address on both sides.* Impossible across ML-DSA-87 and secp256k1 (§2); Decision 7
  is the honest replacement.
* *Priority fees burned.* v0.4 §8 pays them to the payload miner; unchanged.

**Decision 11 — Lane 1 is the same primitives with post-quantum keys.** ADR-0023's Lane 1
(`PQ-AUTHENTICATED`) shares one engine with Lane 2 and differs by "auth profile, chain/lane
domain, precompile policy, and bridge policy" (acceptance condition 3). The four addresses and
the facade family are the model-market entry of both lanes' precompile profiles; a Lane 1
holder id is `evm_holder_v1(lane 1's chain id, its ML-DSA-derived address)` — a third
namespace, PQ-labelled, still never crossing. The writer's action list and the fold's
settlement list are FD-XLANE's outbox and inbox, one anchor apart, which is why this ADR needs
no synchronous cross-lane call and adds none.

**Decision 12 — What ships beside the node.** `contracts/misaka-model/` with `IMisakaModelRegistry.sol`,
`IMisakaModelAMM.sol`, `IMisakaModelPosition.sol`, `IMRC20.sol`, `IMisakaModelWriter.sol` and
the address constants — the `L1Read.sol` role; `eth_call` against the precompiles and facades
works from any Shanghai toolchain at chain id `0x4D534B` with no MISAKA-specific RPC; receipts
of the settling block carry the facade events under the standard log format
(`docs/evm-differences-from-ethereum.md`'s rule: the receipts *root* is MISAKA's, the log
fields are standard); `getPalwModelMarket` gains `evm_namespace_units`, `getPalwModelPositions`
gains the label; `misaka palw model-buy|model-sell` gain `--via evm` for the operator's own
account; the explorer's Model Market page shows one curve with two doors.

## 5. What this costs, stated before it is measured

* **Gas, per action (user-paid):** a basic writer call ≈ 47,000 (Hyperliquid's measured figure
  for the same shape: ~25,000 burned + the call and the log; F002 charges 9,000 for a
  comparable intercept, so the 25,000 is a ceiling to measure down from), so 128 actions ≈ 6.0M
  of the block's 30M. A facade `balanceOf` read ≈ `2,000 + 65 × (36 + 32)` = 6,420; a `quoteBuy` ≈
  `2,000 + 65 × (100 + 192)` ≈ 21,000.
* **System gas, per settlement (nobody pays; capped):** 25,000 per settlement op, the deposit
  claim's figure; 128 × 25,000 = 3.2M, and 256 claims × 25,000 = 6.4M, together 9.6M under the
  existing `MAX_SYSTEM_GAS_PER_EVM_BLOCK` = 10M — which is how 128 was chosen.
* **Fold:** O(1) per action, after the carriers; the quote is the existing pure function.
* **State:** one settlement row (≈ 120 bytes) per action, dropped when `C` consumes it; one
  position row per (class, EVM holder), ADR-0087's ≈ 150 bytes.
* **Bytes on the wire:** an action is one EVM tx (≈ 200 bytes) in a 128 KiB payload; no
  carrier, no chunk group.
* **Latency:** emit at `B`, applied at `fold(B)`, settled and visible at `C` — one chain block,
  which at the RC's cadence is the interval `bits` sets. The reference's "a few seconds" is a
  design choice made for the same reason; here it is one block for a structural one.
* **The fee table is ADR-0087's, unchanged**, with ADR-0088 D8's owner/contributor split of
  the leg: 5 % burned, 1 % to the line's owner (and its adopted contributor), 94 % net, 12 %
  round trip before slippage — from either door.

## 6. Security — the four principles, checked before it is built

*A free field is a free draw; silence is not a verdict; weight is what certification buys; the
chain never takes the host's word* (README §"Security amendments").

| # | attack | what stops it, and the residual |
|---|---|---|
| A1 | **A wrapper**: a contract buys positions and sells its own transferable shares. | Decision 4: a contract cannot be a holder (`NotAnAccount`). Residual: off-chain custody of a key is outside every protocol; stated, not closed. |
| A2 | **Reentrancy** through a precompile or the writer. | Reads are pure functions of the selected parent's fold and make no calls; the writer makes no calls and returns nothing; settlement runs as system ops before any user tx (v0.4 §3.2). Nothing re-enters. |
| A3 | **Front-running the curve inside a block**: an accepting miner orders EVM txs to trade ahead of a buy. | The accepting block's EVM order is the mergeset's canonical acceptance order, not the miner's choice of sequence; `min` protections refuse; carriers trade first by rule, so the EVM door is never the faster one (Decision 6). Residual: ordinary EIP-1559 priority competition among EVM txs, as on every EVM. |
| A4 | **Escrow stranding**: value enters the writer and no settlement ever comes. | Every action in `EVM(B)`'s list has an outcome in `fold(B)` (filled or refused) and every outcome has a system op in `EVM(C)`; a chain block with a non-empty list and a child that omits the ops is `CommitmentMismatch` → disqualified. The F002 residual-locking test (`kaspa-evm/src/executor.rs:1417-1427`) is the template: "no value stuck", asserted; `apply_evm_bridge_effects` is where the sink lands, beside the withdrawals. Residual: a reorg that drops `B` drops the escrow with the block, as it drops a withdrawal — the existing reversed-diff path. |
| A5 | **A fake `tx.origin`**: an account that is a contract under a future fork (EIP-7702). | The code-size clause in Decision 4; the fork that admits 7702 must re-decide it explicitly. |
| A6 | **Gas griefing**: fill the block with actions that will be refused. | Each costs its caller the full call gas; refusals are free for the fold (O(1)) and for the settling block (a refund op under the system-gas cap). |
| A7 | **Supply**: mint through the credit path. | An EVM credit is written only by the fold from a filled sell, whose `net` came off the reserve ADR-0087 M2 bounds; the coinbase skips `EvmCredit`; the accumulator check (Decision 8) is O(1) and every node runs it. |
| A8 | **The chain takes the host's word**: a precompile answering from RPC state. | A read is served from the consensus store's fold row for the selected parent — the same rows `evm_commitment_root` is computed over; an executor answering from anywhere else produces a different commitment and is disqualified. |
| A9 | **Cross-namespace laundering**: buy on the EVM, sell on the UTXO side to move MSK across the bridge without the bridge. | No object crosses namespaces (Decision 7); the only MSK path is the bridge's own, and a sell pays the namespace it was bought in. |
| A10 | **Security downgrade by default**: a wallet routing a PQ holder into the EVM door. | Positions never move between doors; a wallet cannot route what does not move. ADR-0023 FD-SUPPLY's display rule applies to the *label* of an EVM-held position, which `getPalwModelPositions` carries. |
| A11 | **Weight**: an EVM action bearing on fork choice. | It bears on nothing but a market row: no share, no budget, no ticket, no blue work (ADR-0087 §2's "a position grants nothing but the right to sell it back"). |

## 7. Invariants the tests must hold

* **E1 (window).** For every EVM block `B` and every precompile call, the returned rows equal
  `fold(selected_parent(B))`'s; two nodes with the same store answer byte-identically; a call
  below the fence is an empty-account call.
* **E2 (one hand).** The only writes the EVM can cause in the fold are the two actions; a
  property test over the writer's decoder finds no third; the facade's `buy`/`sell` reach the
  same code path as `sendAction` with ids 1 and 2.
* **E3 (no transfer, from either side).** ADR-0087 M3 holds over the object set *and* the
  action set; the facade's four ERC-20 transfer selectors revert; `supportsInterface(ERC-20)`
  is false; a call with `msg.sender != tx.origin` or non-empty code is refused.
* **E4 (order).** In `fold(B)`, every carrier-borne market object applies before every EVM
  action, and EVM actions apply in `seq` order; a golden trace with both kinds fixes the prices.
* **E5 (settlement is total).** `|actions(B)| == |settlements(B)| == |settlement ops(C)|`; a
  filled buy's escrow burn equals its sink output's value; a refused buy's refund equals its
  escrow; a filled sell's credit equals `net × EVM_NATIVE_SCALE`.
* **E6 (supply).** Decision 8's identity holds at every chain block, including across `B`/`C`
  with the receivable stated; the accumulator equals the sum of EVM balances.
* **E7 (namespaces).** No fold transition changes an ML-DSA holder's units by an EVM action or
  an EVM holder's units by a carrier object; `evm_holder_v1` is injective on (chain id,
  address) up to the hash.
* **E8 (ADR-0087 M1–M8 and ADR-0088 L1–L10 hold unchanged** with EVM-originated moves in the
  recorded sequences.
* **E9 (the fence).** Fingerprint unchanged where `None`; arming without `palw_model_market` or
  on a lane-inert preset is refused at validation; a chain with the fence armed and no action
  ever emitted has the same state root as one without the fence.
* **E10 (reorg).** Dropping `B` drops its action list, its settlement rows and its escrow with
  the block's reversed diff; re-adding it reproduces them.
* **E11 (bounds).** The 129th action in a block reverts; system gas per EVM block never exceeds
  the cap with claims and settlements together.
* **E12 (labels).** Every EVM-namespace row returned over RPC carries `CLASSICAL-ECC`.

## 8. Order of work

1. Fold: the action input to `apply_palw_transition_v6`, `evm_holder_v1`, the settlement
   collection, `PalwPayoutKindV2::EvmCredit`, E4–E8.
2. Executor: the three read precompiles, the writer with escrow, the facade family, the
   settlement system ops with their receipt and logs, E1–E3, E5, E11.
3. Processor: the action list carried from `evm_chain_context_step(B)` into `fold(B)`, and the
   settlement ops derived for `C`; the commitment covers both; E10.
4. Params: the fence and its two preconditions; the fingerprint pin; E9.
5. RPC/CLI/explorer; the Solidity interfaces; the wallet label; E12.
6. Devnet drill (both lanes of the market on one class: carrier buy, EVM buy, EVM sell, refused
   buy, reorg across `B`/`C`); then testnet-11 by activation.

## 9. Implementation record (2026-09-05, `palw-adr0088-0089-impl`)

**Landed — §8 items 1–5 (fold, executor, processor, params, RPC/CLI and the Solidity
interfaces); item 6 (the devnet drill) is not.** The fold half is commit `bf4ed1d8`; the rest is
the branch's closing commit.

| where | what |
|---|---|
| `consensus/core/src/evm/model_market.rs` | Decision 1's addresses (`0x…F010`–`0x…F013` through `system_address`) and `facade_address_v1` (`0x4d50` ‖ 18 bytes of BLAKE2b over the line id); `evm_holder_v1(chain_id, address)` (Decision 7); `PalwEvmMarketActionV1` / `PalwEvmSettlementV1` with the refusal codes; `PalwEvmMarketFencesV1`; the view `PalwEvmViewV1` (classes, lines, versions, proposals, evaluations, markets, positions — flattened rows the doors answer from and nothing else); `synthetic_market_sink_txid`; the bounds (128 actions an EVM block, 25,000 gas a settlement, 25,000 gas a writer call). |
| `palw_state_v2.rs` | `evm_settlements` on the state (root only when non-empty; carriage tail `0x89`); `PalwTransitionExtrasV1 { evm_market_active, evm_actions }`; slot 1c′ drains the selected parent's settlement list, slot 3c applies the block's actions after every carrier-borne market object (E4) through `model_buy_v1` / `model_sell_v1(.., pay_net_via_coinbase = false)` — an EVM buy is a position under the EVM holder id, an EVM sell's net is the settlement row and never a coinbase payout; a refused action is a settlement with a reason, never a fault of the block; `evm_view_v1(chain_id, base_class_id)`. |
| `config/params.rs`, `palw_mode_v2.rs` | `palw_model_evm: Option<ForkActivation>`; `validate_palw_v2` refuses it before `palw_model_market` (`ModelEvmBeforeMarket`) and on a preset whose EVM lane is inert (`ModelEvmOnInertLane`); `palw_evm_market_fences_at(daa)`; pinned in the fingerprint; `None` on every preset. |
| `consensus/core/src/evm/mod.rs` | `EvmSystemOp::MarketSettle(PalwEvmSettlementV1)`; `EvmExecutionResult.market_actions` (not committed — the fold recomputes nothing from it; the settlements it produces are). |
| `kaspa-evm/src/model_market.rs` | The intercept: `MarketHandlers` wrapping `handler.execution.call` (the F002 seam's shape) — the three read doors decode the ABI and answer from the view; the facade family answers ERC-20's read half, refuses its transfer half with `NonTransferable`, and routes `buy`/`sell` to the writer's path; the writer charges its gas, refuses a static frame, a caller that is not `tx.origin` or has code (Decision 4), a dormant market, an unknown line, a market closed to buys, a value that is not whole sompi, the 129th action, then moves the escrow caller → writer in the tx's journal and emits `ActionQueued` — the two unwind together on a revert; `decode_action_log` (strict: address, topics, lengths, zero padding) and `settlement_log` (`Bought` / `Sold` / `Refused` from the line's facade). |
| `kaspa-evm/src/executor.rs`, `precompiles.rs`, `sim.rs`, `trace.rs` | `EvmBlockInput.market: EvmMarketInput { palw_view, fences, expected_settlements, chain_id }`; the `MarketSettle` ops validated equal, in order and in count, to the selected parent's list (E5) and applied before any user tx — a filled buy burns the writer's escrow, a refused buy reroutes it to the account, a filled sell credits the net — their events in ONE system receipt at index 0 (user receipt indices unchanged, `accepted_tx_count` counts user txs); the committed `ActionQueued` logs scanned into `market_actions` in log order; Decision 8's accumulator `+ credited − burned`; `register_all_misaka_precompiles(handler, f003_active, market)`; `EthCallEnv { palw_view, market_fences, chain_id_for_holders }` so `eth_call` registers the same doors. |
| `consensus/src/processes/evm/mod.rs`, `virtual_processor/processor.rs`, `body_validation_in_isolation.rs` | `evm_chain_context_step` builds the view and the fences from the selected parent's fold and the block's DAA, hands the parent's settlement list as `expected_settlements`, validates the payload's ops (`validate_evm_market_settlements`), materialises a filled buy's escrow as a sink output (`apply_evm_market_effects`, `synthetic_market_sink_txid`) and feeds `market_actions` into `PalwTransitionExtrasV1.evm_actions`; the EVM pre-execution pipeline is bypassed while the fence is armed (a block's fold input is its own); the template builder pushes the selected parent's `MarketSettle` ops and mirrors the sink effects; the system-op cap split (claims ≤ 256, settlements ≤ 128, E11). |
| `kaspad/src/eth_rpc.rs`, `consensus/core/src/api/mod.rs`, `consensus/mod.rs`, `session.rs` | `palw_evm_view_v1()` — the tip's view and the fences at the tip's DAA — set on the head `EthCallEnv`; a historical block's env carries no view (recorded below). |
| `contracts/misaka-model/` | `MisakaModelAddresses.sol`, `IMisakaModelRegistry.sol`, `IMisakaModelAMM.sol`, `IMisakaModelPosition.sol`, `IMRC20.sol`, `IMisakaModelWriter.sol`, a README — 64-byte ids as two `bytes32` words; compiled with solc 0.8.28. |

**Tests.** `palw_state_v2::tests::evm_market` (4): a buy from the EVM is a position in the EVM
namespace and a settlement the child carries (E2, E7); a sell from the EVM credits through its
settlement and never the coinbase (E2, E6); a refused action is a settlement and not a fault of the
block (E5); the settlement tail rides the carriage only while a list waits (E10's revert side).
`evm::model_market` (3): the addresses are where the ADR puts them, the holder id binds the chain
and the account (E7), the view answers by id and by facade (E1). `kaspa_evm::model_market` (4):
selectors and the interface id excluding ERC-20 (E3), the action record round-trips through the
log, `sendAction` calldata decodes strictly, the settlement log names the facade and the outcome.
`kaspa_evm::executor` (2): the writer escrows and the block lists its actions in order — a buy, a
sell, a torn buy reverted, and the same calls finding an empty account while the fence is dormant
(E2, E9, E11's revert shape); a settlement block burns, refunds and credits exactly the parent's
list, the system receipt at index 0 ahead of the user's, the supply identity across a deposit, a
burn, a credit and a basefee burn (E5, E6), and a short, altered or unexpected list refused whole.
The params test pins E9. E4 and E8 hold by the fold's slot order and by ADR-0087's and ADR-0088's
suites running unchanged over the same state (1,868 green). E10's forward side (dropping `B`) and
E12 are not tested — see below.

**What the implementation taught.** (1) `PalwPayoutKindV2::EvmCredit` (§8 item 1) was not needed:
the settlement row *is* the credit, and the executor pays it at `C`; a payout kind would have been
a second copy of the same number. (2) The settlement list has to live in the fold's state for one
block (not only in `B`'s result) so that `C`'s validation, `C`'s template and a node that replays
`B` from its delta all read the same list. (3) The action list must be scanned from the *committed*
logs, not from the writer's own counter — a tx that queued and then failed leaves no log, and the
counter is resynchronised from the scan at every tx. (4) The escrow moves inside the tx's journal,
so a revert after the queue (out of gas in the same frame is impossible, but the shape is kept)
unwinds both the value and the log together.

**Not landed.** The devnet drill (§8 item 6: carrier buy, EVM buy, EVM sell, refused buy, reorg
across `B`/`C`); a historical `eth_call` sees the market's doors closed — the fold is kept only at
the tip and a past view would need a replay this RPC does not run (`eth_rpc.rs` says so at the
site); the `CLASSICAL-ECC` label (E12) — the fold keys a position by holder id alone and cannot
tell an EVM-derived id from an ML-DSA one, so the RPC has nothing to label with; the wallet label;
the explorer; arming on testnet-11.

## 10. What is deliberately not decided

* The gas schedule's final numbers (`2,000 + 65·n`, 25,000, 47,000) — Hyperliquid's are the
  starting values; MISAKA's are set from a measurement against `EVM_GAS_LIMIT` before arming.
* Whether ADR-0088's *declared* rows (evaluations, dataset and config hashes) should be served
  by the precompile at all, or left to the explorer; the registry's shape carries them either
  way, labelled as declarations.
* A Lane 1 (PQ-EVM) writer that can carry bond-signed objects (a version, a role change) —
  possible only where the lane's native auth *is* the bond's key (ADR-0023 O-03), and its own ADR.
* Whether a contract wallet could ever hold a position under a stricter rule (for instance, a
  wallet whose only owner is one EOA and whose code is a known hash) — the operator's legal
  reading of 交換業 decides whether "held by a contract" is "held by a person", and nothing
  technical does.
* The facade's `name()`/`symbol()` text beyond the shape above.

## 11. Number hygiene

This is ADR-0089. The README's next free number was 0089 after 0088's row; it becomes 0090 with
this row. First written on `docs/adr-0089-evm-model-primitives`; revised with ADR-0088's
2026-09-05 rewrite on `palw-adr0088-0089-impl`, where 0087 → 0088 → 0089 read in one line.
