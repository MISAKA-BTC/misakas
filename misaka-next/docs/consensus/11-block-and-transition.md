# 11 — Block structure and the per-block transition

> Normative. RFC 2119 keywords. Rule IDs `BLK-R*`, invariant IDs `INV-BLK-*`. Citations `path:line`
> are to the t12 reference (`rcore/int-3` @ `a0af3c92`, see `PROVENANCE.md`) and were checked with
> `grep -n`/`sed`; anything not read is marked **[unverified]**. *Added in review: no chapter
> specified block structure or the order of a block's state transition, and the pure functions of
> 01–08 could not be composed deterministically without both.*

## 1. Purpose

Chapters 01–08 define pure functions: a clock step, an admission, a licence check, a sweep, a
subsidy. A chain is a sequence of blocks, and a block applies many of those functions at once, to
its own content and to the blocks it merges. The result depends on the order. Take two examples. If
a block's own licences fed the `SafeDaa` its own sweeps read, `Final` would depend on whether the
sweep ran before or after the licence. If a merged attempt were admitted before the block's own,
the two would race for the last bucket token differently on two nodes.

This chapter fixes three things: what a block is, which blocks can be on a selected chain, and
the one ordered function that turns a parent's state into a child's. Every other chapter's rule
runs inside it. t12's fold (`apply_palw_transition_v4`) is the reference for the order. Where next
differs, §6 says why.

## 2. Concepts and types

### 2.1 Lanes

| Lane | Carries | Claims a slot (01 DAA-R4) | Paid (08 ECON-R16) | Weight (06) |
| --- | --- | --- | --- | --- |
| attempt | one lottery ticket for one class, plus transactions and objects | yes, if it is on a selected chain | iff its attempt is admitted | only through a verified claim |
| heartbeat | transactions and objects; a `2^24`-hash price | yes; it MUST tick | never | none |
| receipt | panel receipts and other objects | no | never | none |
| round | execution-lane transactions | no | never | none |

There is no block-level work in any lane (03 CLAIM-R3, 06 FORK-R9).

### 2.2 The block

```rust
pub struct Header {
    pub network: NetworkDomain, pub version: u16,
    pub parents: Vec<BlockId>,            // DAG parents; the selected parent is derived (BLK-R4)
    pub timestamp: Timestamp,
    pub clock: HeaderClockFields,         // daa_score, clock_slot (01 DAA-R5)
    pub lane: Lane,
    pub lane_proof: LaneProof,            // attempt: execution commitment + carried ticket target + signature;
                                          // heartbeat: its 2^24-hash proof
    pub pruning_point: BlockId,           // 07 FINAL-R10
    pub state_root: Hash64,               // the chain state after this block's transition (§4)
    pub body_root: Hash64,
}
pub struct Block { pub header: Header, pub transactions: Vec<Tx>, pub objects: Vec<ConsensusObject> }

pub struct MergeSet { pub ordered: Vec<BlockId> }       // BLK-R4
pub enum ChainEligibility { Eligible, Ineligible(BlockRefusal) }
```

**The DAG.** A block names parents. Its **selected parent** is the best of them under 06's
comparator, not the heaviest by blue work (BLK-R4). Its **mergeset** is every block in its past
that is not in its selected parent's past. A block's **selected chain** is the chain of selected
parents back to genesis. State is folded along that chain only. A merged block contributes its
transactions, objects and attempt to the block that merges it.

**Chain eligibility.** A block can be valid as a DAG member and still unfit to be on any selected
chain. Its header is valid, but its own transition, run as a chain block, refuses. t12 has the same
distinction: an unadmitted attempt is "disqualified from chain" rather than invalid
(`consensus/core/src/palw_attempt_v2.rs:1129-1131`). Such a block may be merged by others. It is
never a selected parent, so its clock fields, its licences and its coinbase are never part of any
chain.

## 3. Normative rules

**BLK-R1 (header validity is header-local).** A header MUST be validated from itself and its
parents' headers alone: shape, network and version, signature under the lane's context, the clock
fields (01 DAA-R5), the lane proof and the pins derivable from the header (04 POL-R2, POL-R3). No
state is read at the header stage. *Because:* INV-DAA-06; a header that needed state could not be
relayed or checked by a node that has not yet folded its past.

**BLK-R2 (a lost ticket is not a block).** An attempt header MUST carry the ticket target it claims
to be under. Header validation MUST refuse it unless `ticket(execution_commitment) ≤ carried target`.
The carried target is checked against `target_in_force(SafeDaa(B))` in the transition (§4 step 1).
*Because:* `failed_lottery_blue_weight`. A lost-lottery header never enters the DAG, costs no relay
and weighs nothing (04 Q1, OQ-13).

**BLK-R3 (bounded merging).** A block MUST name at most `P_MAX` parents, MUST have a mergeset of at
most `M_MAX` blocks with at most four heartbeats (01 DAA-R9), and MUST NOT merge a block that does
not descend from the chain anchor of its selected parent (07 FINAL-R3). *Because:* merge bounds cap
validation work per block. The anchor condition keeps a block from reaching back below chain
finality, and it is a clock-free merge depth (no blue-score depth survives, OQ-18).

**BLK-R4 (selected parent and mergeset order).** The selected parent MUST be
`select_tip(SafeContext::from_anchor(a), parents, p)`, where `a` is the deepest chain anchor among
the parents' views (06 FORK-R10). The mergeset MUST be ordered topologically, with ties broken by
`BlockId` ascending. That is a function of the block's past alone. *Because:* INV-FORK-06. t12
selects the parent by blue work (`consensus/src/processes/ghostdag/protocol.rs:216-220`), the
quantity T12-D5 shows is header-only.

**BLK-R5 (chain eligibility).** A block is chain-eligible iff its selected parent is chain-eligible
and `apply_block` (§4) succeeds on it as a chain block. That requires all of:

* the carried target equals `target_in_force(SafeDaa(B))`;
* its own attempt, if any, is admitted (03 CLAIM-R2);
* its objects and transactions are valid in the order of §4;
* its coinbase equals the derived one and passes the minted counter (08 ECON-R17);
* its `state_root` and `pruning_point` equal the derived values.

An ineligible block MUST NOT be a selected parent and MAY be merged. *Because:* a won-but-refused
attempt costs a fabricator a few hashes (04 §2.4), so it must not claim a slot, be paid or shape the
chain (INV-BLK-02).

**BLK-R6 (merged attempts).** Each merged attempt MUST be admitted against the live state in
mergeset order (§4 step 7), or skipped. A skipped attempt confers nothing: no claim, no slot, no
subsidy, no weight. Its block's transactions and objects are still applied. *Because:* two merged
blocks may race for one bucket token or one room slot, and the order must decide it the same way
on every node (t12: `consensus/src/pipeline/virtual_processor/processor.rs:11630-11652`).

**BLK-R7 (one transition, one clock reading).** A chain block's state MUST be `apply_block` of §4,
run in exactly that order, reading the one `ClockContext(B)` of 01 DAA-R18. A licence MUST be
recorded in the licence ring at `LocalDaa(B)` in the step that performs it (01 DAA-R12). *Because:*
INV-BLK-01. A rule that read a clock its own block's objects move would depend on step order.

**BLK-R8 (coinbase maturity).** A coinbase output created by chain block `X` MUST be spendable only
in a block `B` whose chain anchor, the anchor of `B`'s selected parent's view, is `X` or a
descendant of `X`. Equivalently, `X` is chain-finalized as `B` sees it. *Because:* coinbase value
leaves consensus control when spent, so its maturity is a value release and reads the chain-final
clock (01 §2.7). t12 used a DAA age with a DAA-only fallback (`consensus/core/src/dns_finality.rs:4123-4144`,
01 census C24), which heartbeats pace.

**BLK-R9 (derive, never carry).** Nothing that is a pure function of chain state MAY be a carried
object that a producer could withhold or forge. This covers panel bindings (05 PANEL-R11), defaults
(05 COURT-R2), maturity, releases and the coinbase. *Because:* `anchor_bind_censorship` and
`producer_defaulted_unsigned_verdicts`. t12 already derives bindings
(`consensus/src/pipeline/virtual_processor/processor.rs:11814-11828`, `:12290`).

**BLK-R10 (the header commits the result).** A header MUST commit `state_root`, the root of the
chain state after its own transition (§4 step 9), and `pruning_point` (07 FINAL-R10). A snapshot
node MUST be able to check both from the committed state. *Because:* INV-DAA-06, INV-FINAL-03.
Archival, pruned and snapshot nodes must agree on every block.

## 4. Pure functions

```rust
/// BLK-R1/R2. Reads headers only.
pub fn check_header(h: &Header, parents: &[&Header], p: &Params) -> Result<HeaderFacts, HeaderRefusal>;
/// BLK-R3/R4. A function of the block's past (headers and chain anchors of the parents' views).
pub fn select_parent_and_mergeset(h: &Header, past: &PastView, p: &Params) -> Result<(BlockId, MergeSet), DagRefusal>;
/// BLK-R5/R7. The whole per-block transition. Pure: no store, network, RPC or wall clock.
pub fn apply_block(parent: &ChainState, block: &Block, merged: &[&Block], order: &MergeSet, p: &Params)
    -> Result<(ChainState, BlockEffects), BlockRefusal>;   // Err = the block is not chain-eligible
```

`apply_block`, in this order. Each step calls functions owned by other chapters:

```text
0. Clocks (01 DAA-R18). local = check_header_clock(..); chain_final = chain_finalized_daa(parent view)
   (07); safe = safe_daa(parent.licence_ring, D_SAFE, chain_final) (01). ctx = ClockContext{..}.
   Nothing later in this block changes ctx.
1. Epochs and refills at ctx.safe. Close every SafeDaa epoch crossed since the parent, in order:
   step_work_target (04), receipt-target retargets, audits and reclamation (01 DAA-R17). Refill
   every lane bucket to ctx.safe (03 CLAIM-R10).
2. Context. For an attempt: block.carried_target == target_in_force(targets after step 1, ctx.safe)
   (04 POL-R6), else refuse (ineligible).
3. Sweeps. Every deadline that has elapsed at ctx, in ascending (deadline point, kind, key) order,
   kinds Local, Safe, ChainFinal:
     Local      NoRing voids (uncharged)
     Safe       receipt timeouts (redraw, then forfeiting void), challenge_ends → mature → FinalClaim
                (vesting row written, seed leaf minted), conviction_ends closes, court turns and
                defaults, readiness lapses
     ChainFinal vesting releases, exit completions, DA defaults, right maturity
4. Derived bindings (05 PANEL-R11, BLK-R9). For every AwaitingRing claim whose seed ring is now
   fixed (its K-th post-acceptance leaf reached Final at or before step 3), in admission-index
   order: derive_panel → bind, or void NoCapablePanel (uncharged).
5. The block's objects and transactions, in acceptance order: receipts (check_receipt; a claim whose
   receipt set now passes check_licence is verified → VerifiedClaim, and its licence is recorded
   in the ring at ctx.local); accusations, court moves and verdicts (convict); DA sessions and
   answers; registrations, deposits, exit requests; reporter commit and reveal; class objects;
   transactions (a coinbase spend obeys BLK-R8).
6. The block's own attempt: admit (03 CLAIM-R2: pins, bond, class, lottery, bucket, room, exposure;
   the next admission index), then escrow carve (08 ECON-R3). A refusal refuses the block
   (ineligible).
7. Merged blocks in mergeset order: their objects and transactions as in step 5, then their
   attempts, each admitted against the live state or skipped (BLK-R6).
8. Coinbase and supply (08 ECON-R16, ECON-R17). Pay per admitted attempt (own and merged), withhold
   escrows, split, record every mint and burn in the supply ledger, check the minted counter, and
   compare with the block's coinbase.
9. Running totals (06, 07). Append new VerifiedEntries; update Ω and V; set safe flags
   (maintain_safe_flags, 06 §5); count safe anchors; advance the claim frontier; compute this
   view's chain anchor (block_finalized_anchor), which is what the children read as
   ChainFinalizedDaa; compute state_root and the pruning point and compare with the header.
```

Properties, each a test:

* the result depends only on `(parent, block, merged, order, params)`;
* every rule reads the one `ctx`;
* a licence recorded in step 5 moves `SafeDaa` only for the children;
* a claim that reaches `Final` in step 3 can seed a ring that binds in step 4 of the same block;
* a merged attempt is paid iff it was admitted in step 7;
* permuting `merged` without changing `order` changes nothing;
* a block that fails any step is ineligible and changes no state.

## 5. Invariants upheld

* **INV-BLK-01** The per-block transition is one ordered pure function, reading one clock context
  fixed by the parent; archival, pruned and snapshot nodes compute equal state roots.
  Test `inv_blk_01_block_transition_is_one_pure_ordered_function`.
* **INV-BLK-02** An attempt that is not admitted confers nothing on the block that carries it: a
  lost ticket is invalid, a refused own attempt makes the block ineligible, a refused merged
  attempt is skipped; none ticks the clock or is paid.
  Test `inv_blk_02_refused_attempts_tick_nothing_and_are_paid_nothing`.
* Upheld with other chapters: **INV-DAA-06** (BLK-R1, BLK-R10), **INV-CLAIM-04** (BLK-R2, BLK-R5),
  **INV-ECON-02** (step 8), **INV-TIME-07** (BLK-R8), **INV-FORK-06** (BLK-R4).

## 6. t12 reference

**The fold's order.** `apply_palw_transition_v4` is documented as pure
(`consensus/core/src/palw_state_v2.rs:19719-19722`). Its steps are:

| t12 step | line | next step |
| --- | --- | --- |
| 1 context monotonicity | `palw_state_v2.rs:20531` | 0, 2 |
| 1b drain paid payouts, 1d span boundary | `:20544`, `:20577` | 8, 1 |
| 2 deadline sweeps ("everything strictly past is resolved before this block says anything") | `:20583` | 3 |
| 2b–2d retarget, share raise, reclamation | `:20604`, `:20628`, `:20635` | 1 (before the sweeps in next) |
| 3 objects, derived bindings first (`processor.rs:12290`) | `:20641` | 4, 5 |
| 3a–3d activation, budgets, EVM, vesting | `:20687`, `:20693`, `:20674`, `:20679` | 3 (vesting), 5 |
| 4 own work; 4b mergeset work; 4c unbound void | `:20699`, `:20777`, `:20876` | 6, 7, 3–4 |
| 5 frontier observation | `:20894` | 9 |
| 7 round permits | `:20949` | 5 |

**Divergences, and why.**

* **Clock read point.** t12's steps read the block's own DAA score. next reads one `ClockContext`
  fixed by the parent (DAA-R18), so a block's own licences cannot move its own sweeps.
* **Epoch work before the sweeps.** t12 retargets after its sweeps (2b after 2). next closes
  `SafeDaa` epochs and refills first (step 1). The target checked at step 2 must include a boundary
  this block crosses, and a refill must not depend on what the sweeps void.
* **Bindings.** t12 derives bindings in the processor and prepends them to the fold's objects
  (`consensus/src/pipeline/virtual_processor/processor.rs:12290`, called from the acceptance walk at
  `:2053`). next makes the derivation a step of the one function (step 4), bound in the block that
  fixes the claim's ring (04 POL-R9), not the anchor block.
* **Selected parent.** t12: max blue work (`consensus/src/processes/ghostdag/protocol.rs:216-220`).
  next: 06's comparator (BLK-R4).
* **Payment of merged attempts.** t12 decides which merged blues are paid from a parent-state view
  (`processor.rs:5436-5591`), with a stated one-mergeset residual race (`:5540-5546`). next pays
  from the admission result of step 7 itself (step 8).
* **Coinbase maturity.** t12: DAA age with a DAA-only fallback (`consensus/core/src/dns_finality.rs:4123-4144`).
  next: chain finality (BLK-R8).
* **Lost tickets.** t12 admits any attempt digest at Layer 0 and gives it blue work; admission runs
  only in the virtual processor (03 §7). next refuses a lost ticket at the header stage (BLK-R2).

## 7. Attacks this chapter defends against

* `failed_lottery_blue_weight` — real on t12 (03, 04, 06). BLK-R2, BLK-R5, BLK-R6, INV-BLK-02.
* `merged_work_payout_mismatch` — unverified on t12 (catalog: closed); the residual race above is
  the live lead. Step 8, INV-BLK-01.
* `node_local_input_in_fold` — unverified on t12 (catalog: closed). BLK-R7, INV-BLK-01.
* `header_level_and_parent_misread` — unverified on t12 (catalog: closed). BLK-R4.
* `object_poisons_carrying_block` — unverified on t12 (catalog: closed). Step 5 drops a refused
  relayed object; only the block's own attempt and coinbase can make it ineligible.
* `anchor_bind_censorship` — closed at the reference; kept by BLK-R9.

## 8. Open questions for the project owner

**Q11-1. Merge bounds.** `P_MAX`, `M_MAX`. Options: (a) Kaspa-like values (tens of parents, a few
hundred merged blocks); (b) small (a handful), because merged blocks carry no weight in next and
only transactions and objects. *Recommend (b)*: with no blue work, merging exists for throughput
and liveness, and small bounds keep `apply_block` cheap to validate. Measure in the simulator.

**Q11-2. Coinbase maturity on the chain-final clock.** In a licence halt chain finality stops, and
so does every coinbase spend (BLK-R8). Options: (a) accept it, like every other value release
(OQ-4 (a)); (b) a `SafeDaa` span, which also stops in a halt; (c) a `LocalDaa` span, which a private
branch accelerates. *Recommend (a)*: one rule for every release, and the halt is already OQ-4's
question.

**Q11-3. What does `state_root` commit?** Options: (a) the post-state of this block as a chain block
(BLK-R10); (b) the pre-state (the selected parent's), as Kaspa commits its UTXO state
[unverified for t12's exact header field]. *Recommend (a)*: a snapshot node can check the block it
receives, and ineligibility is decided by the same function that produces the root.
