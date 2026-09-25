# 07 — Finality

> Normative. RFC 2119 keywords. Citations `path:line` are to the t12 reference (`rcore/int-3` @
> `a0af3c92`, see `PROVENANCE.md`) and were checked with `grep -n`/`sed`; anything not read is
> marked **[unverified]**. Rule IDs `FINAL-R*`, invariant IDs `INV-FINAL-*`. Fork choice is
> chapter 06; the full invariant list is `09-invariants.md`; attacks are `10-attack-model.md`.

## 1. Purpose

"Final" is the most overloaded word in t12. A claim is `Final` when its challenge window closes; a
block is behind the "finality point" 600 blue scores down; an anchor is "DNS-final" when bonded
validators voted for it; the EVM bridge's `finalized` is the pruning point. None of these is the
same fact, and only one of them is irreversible in any sense. This chapter separates them, names
the single thing misaka-next calls **finalized** — the anchor that a node will never reorganise
below — and states when it advances, what it guarantees, what it costs to break, and how deep a
node may prune.

The rule of thumb for a reader: **settled** is a property of a claim *on a branch*; **finalized** is
a property of a *node's* chain and never reverts; **pruned** is history the node has thrown away and
cannot re-validate. Each level is a strictly stronger promise than the one before.

## 2. Concepts and types

### 2.1 The ladder

```text
  accepted ──► verified ──► safe ──────────────────► finalized ────────────► pruned
  (block on    (panel       (buried by d_bury, no     (the node's anchor:       (below the anchor's own
   a branch)    licensed:    unreleased dispute;       FINAL-R2..R5;             claim frontier;
                VerifiedClaim) 06 §3.3; clock-free)    never reverts)            FINAL-R10)
                    │
                    └──► settled (FinalClaim: challenge window closed on SafeDaa; branch-relative;
                                  moves money, seeds and rights, never weight or finality)
```

| Level | Type | Scope | Can it revert? | What it authorises |
| --- | --- | --- | --- | --- |
| accepted | `UnverifiedClaim` in a block | one branch | yes, with the branch | nothing (06 §2.4) |
| verified | `VerifiedClaim` | one branch | yes, with the branch; or voided by a court | `live` weight (06) |
| safe | a verified entry, buried and undisputed (06 §3.3) | one branch | **yes, with the branch** | `safe` weight (06); counts as a safe anchor (FINAL-R2) |
| settled | `FinalClaim` | one branch | **yes, with the branch** | the vesting row, a seed leaf, rights (03 §2.5) |
| chain-finalized | `ChainAnchor` = `block_finalized_anchor(V)` | one branch | **yes, with the branch** | `ChainFinalizedDaa` for the branch's children (01 §2.4) |
| finalized | `FinalizedAnchor` | the node | **no** (INV-FINAL-01) | admissibility (06 FORK-R1), confirmations, the node's retention |
| pruned | `PruningPoint` | committed by headers | no, and not re-validatable | deletion of history |

A `FinalClaim` is *not* irreversible: it is final on its branch. If fork choice later selects a
branch that does not contain it, it is gone together with everything the branch did. So is a
chain anchor. Only the node's finalized anchor survives every fork-choice outcome, because fork
choice is not allowed to look at a candidate that does not contain it. *Safe* and *settled* are
independent: an entry can be safe before its claim is `Final`, or `Final` before it is buried.
Finality counts only safe anchors, so it reads no clock (FINAL-R2).

### 2.2 Types

```rust
/// The finalized anchor a chain's own state computes (FINAL-R3). Pure, per chain; reverts with it.
/// `own_frontier` is the claim frontier (§2.3) computed in the anchor block's OWN state.
pub struct ChainAnchor { pub block: BlockId, pub daa: ChainFinalizedDaa, pub safe_anchor_count: u64, pub own_frontier: BlockId }

/// The node's irreversible boundary (FINAL-R4). `daa` is the only producer of `FinalizedDaa`,
/// which no consensus state transition may read (01 §2.4).
pub struct FinalizedAnchor { pub block: BlockId, pub daa: FinalizedDaa, pub safe_anchor_count: u64, pub own_frontier: BlockId }

/// Consensus parameters of this chapter (in the ruleset fingerprint; fixed at genesis).
pub struct FinalityParams {
    pub k_final: u64,     // safe anchors that must follow a block before it is finalizable; k_final >= D_SAFE
    pub d_final: Weight,  // verified weight (work clock Ω) that must follow it; d_final >= d_bury
    pub m_prune: u64,     // safe anchors kept below the anchor's own claim frontier (IBD margin)
}

/// Two anchors neither of which contains the other. Never resolved by consensus (FINAL-R9).
pub struct FinalityConflict { pub ours: FinalizedAnchor, pub theirs: ChainAnchor }
```

`FinalizedDaa` has exactly one constructor, `FinalizedAnchor::daa`. It is a node fact, read by
confirmations, admissibility and the node's own retention. `ChainFinalizedDaa` has exactly one
constructor, `chain_finalized_daa(parent_view)`, which is `block_finalized_anchor(parent_view).daa`
(01 §2.4). Every state rule that needs "final" time reads that one. Two honest nodes compute the
same `ChainFinalizedDaa` for the same block, whatever anchors they hold themselves.

### 2.3 Safe anchors and the claim frontier

A **safe anchor** is a chain block carrying an attempt whose claim entry is safe (06 §3.3). The
**safe-anchor count** of a block `B` on a view `V` is the number of safe anchors on `V`'s chain
strictly after `B`. Counts are the depth unit of this chapter, for two reasons. First, safety is
clock-free: it is burial by verified weight with no unreleased dispute. A count therefore cannot be
stretched by blocks that carry no verified work, unlike a DAA or blue-score distance, and it cannot
be hurried by a branch whose `SafeDaa` runs ahead. Second, each safe anchor is paid for in two
different ways:

* **On the honest chain**, a safe anchor costs an inference, a won lottery and a licence by seats
  drawn by stake.
* **On a branch the adversary produces**, it costs a bucket token, a fake root's hashes, and a licence
  from the adversary's own seats, which forms with probability `P_cap(s)` per claim (10 §2.1 C11).

On the second kind of branch, compute does not bound the rate; the bucket and `P_cap` do (10 §2.2
H2). *[Synthesis edit, review: the draft counted `FinalClaim` anchors, whose timing reads `SafeDaa`,
and said each costs an inference.]*

The **claim frontier** of a block `X` is the deepest block `F` on `X`'s chain such that every claim
accepted at or below `F` is **resolved** in `X`'s state. Resolved means terminal (`VoidedClaim`,
`ConvictedClaim`) or `FinalClaim` with `conviction_ends` elapsed. The frontier covers every claim:
unverified claims, verified claims with open disputes, and `Final` claims still convictable. It never
retreats, and one claim can hold it for at most `D_max + W_conviction` (03 CLAIM-R13). It is not a
fork key (06 §3.3).

## 3. The design argument

### 3.1 What must be irreversible, and why only a depth rule

A node must commit to *something*, or every confirmation it reports is conditional on an unbounded
reorg and pruning is never safe. Proof-of-work commits probabilistically; BFT protocols commit by a
vote. PoL has no global vote in its core (ADR-0127's direction: PALW settles on its own), so the
commitment is a depth rule over the selected chain: a block becomes finalized when enough
independently verified work has settled on top of it that replacing it would cost more than any
reorg can gain (INV-ECON-01's bound applied to history).

### 3.2 Why depth is counted in safe anchors, not DAA, blue score or time

* **Blue score / block count** — advanced by any blue block, including heartbeats and headers whose
  attempt failed admission (06 T12-D2, T12-D5). A depth in blocks is a depth an attacker buys with
  empty blocks.
* **`LocalDaa`** — a branch's own clock. Where DAA can run ahead of real time (any design in which a
  block without verified work advances it), a private branch buries a block faster than the honest
  network does: `private_daa_finality_acceleration`, applied to finality.
* **Wall clock / timestamp TTLs** — not an input a pure function may read, and a timeout that
  *releases* a commitment turns "the validators went quiet" into "the history is open again"
  (t12's DNS veto TTL, §7.2).
* **`SafeDaa`, or anything timed on it (`FinalClaim`)** — a per-branch clock that copies `LocalDaa`'s
  spacing and moves with the branch's own licences (01 §2.3). A depth counted in `Final` claims
  would let a branch whose `SafeDaa` ran ahead finalize first.
* **Safe anchors and verified weight** — each safe anchor is verified work buried under `d_bury` with
  no unreleased dispute, and the work clock `Ω` (06 §3.3) advances only with verified weight. Neither
  reads a clock. A private branch adds either only at the rate its bucket and its own panels allow
  (`P_cap`, §2.3); a faster clock produces neither. Both are required: a count alone can be met by
  many tiny claims, a weight alone by one enormous claim.

### 3.3 Finality is a function of one chain view, so headers can commit to it

If "finalized" depended on which tips the node happened to hold, two nodes could compute different
pruning points for the same block and disagree about a header that commits one. So the predicate is
defined per chain view (`block_finalized_anchor`, a pure function of one chain's state), and the
node's anchor is the deeper of its current anchor and that function applied to the selected tip.
Both are on the selected chain (06 FORK-R1 guarantees the selected tip contains the current anchor),
so "deeper" is well defined.

The same function, applied to a block's selected parent, gives the block its `ChainFinalizedDaa`
(01 §2.4). State rules read it, and every node computes it identically. The node's own anchor is
never an input to a state transition.

What lies *below* the anchor must also be the same on every admissible candidate, or pruning it
would delete something some branch still disputes. A claim accepted below the anchor may still be
pending at the anchor and be resolved differently on two branches that both contain it. So the
history this chapter protects and prunes is bounded by the anchor's **own** claim frontier (§2.3):
the claim frontier computed in the anchor block's own state. Every claim below it was already
resolved (terminal, or `Final` with its conviction window closed) in a state every admissible
candidate shares.

### 3.4 Safety over liveness, stated plainly

If the network stops producing safe claims (no producer, no quorum), finality stops. Nothing times out
into finality and nothing times out *out* of it. A node that finds a peer chain whose finalized
anchor conflicts with its own does not reconcile the two by any rule: it reports a
`FinalityConflict` and keeps its chain. Two partitions that each finalized a different anchor are a
permanent split until an operator supplies a new trust root; the parameters (`k_final`, INV-ECON-01)
exist to make that event cost more than it can pay.

### 3.5 Weak subjectivity is real here

Panel licences are signatures by bonds. A bond that has been withdrawn can sign an alternative past
at no cost: its collateral is gone from the chain it could be slashed on. So a node that joins from
genesis, or has been offline longer than the bond withdrawal delay, cannot tell an honest history
from one re-signed by retired bonds by weight alone. It MUST start from a recent finalized anchor
obtained out of band (FINAL-R11). An exit completes only after its request lies `D_exit` below the
exiting chain's own finalized anchor (02 BOND-R14). So every history a retired bond could forge
forks below the finalized anchor of every node that followed that chain until the exit (FINAL-R12). t12 already has the mechanism (a trusted checkpoint as an admissibility
constraint, `consensus/core/src/config/trusted_checkpoint.rs:1-37`).

## 4. Normative rules

* **FINAL-R1 — Settled is not finalized.** No rule outside the claims chapter's reward release may
  treat a `FinalClaim` as irreversible. Confirmations reported to users, bridge `finalized` labels
  and pruning MUST read the finalized anchor. *Because:* `settled_claim_reverted_with_branch`.
* **FINAL-R2 — The finality predicate.** A block `B` on view `V`'s chain is *finalizable on V* iff
  (a) at least `k_final` safe anchors follow `B` on `V`, and (b) `Ω(V) − Ω(B) >= d_final`. It
  requires `d_final >= d_bury` and `k_final >= D_SAFE` (06 FORK-R16). Nothing else enters: no
  `LocalDaa`, `SafeDaa`, `ChainFinalizedDaa`, blue score, time distance or `Final` status. Safety is
  itself clock-free (06 §3.3), so no clock enters indirectly either. *Because:* §3.2. With
  `k_final >= D_SAFE`, `k_final` safe anchors after a block imply `D_SAFE` licences recorded after
  it, so the `ChainFinalizedDaa` floor of `SafeDaa` never binds after bootstrap (03 §2.6).
* **FINAL-R3 — Block-relative anchor.** `block_finalized_anchor(V)` MUST be the deepest block
  finalizable on `V` (the genesis anchor if none), returned as a `ChainAnchor`, and MUST be a pure
  function of `V`'s chain state. A block `B`'s `ChainFinalizedDaa` MUST be
  `block_finalized_anchor(view at B's selected parent).daa`. *Because:* §3.3;
  `pruning_point_disagreement`. A state rule that read the node's anchor would split two honest
  nodes on one block (01 §2.4).
* **FINAL-R4 — Node anchor advance.** After each selection (chapter 06), the node's anchor MUST
  become `deeper(current, block_finalized_anchor(selected))`. It MUST NOT move for any other reason.
  *Because:* INV-FINAL-02.
* **FINAL-R5 — Never reverts.** The node MUST NOT move its anchor to a block that is not a descendant
  of (or equal to) the current anchor, and chapter 06 MUST NOT admit a candidate that does not
  contain it. *Because:* INV-FINAL-01, `long_range_rewrite`.
* **FINAL-R6 — Monotone along a chain.** For a view `V'` extending `V`, `block_finalized_anchor(V')`
  MUST be a descendant of or equal to `block_finalized_anchor(V)`. This follows from FINAL-R2 given
  that safe flags are permanent (06 W2), so safe-anchor counts and `Ω` only grow along a chain;
  implementations MUST NOT add a condition that breaks it. *Because:* INV-FINAL-03.
* **FINAL-R7 — Only verified work advances finality.** No block without verified work (heartbeat,
  execution-lane, empty, failed or skipped attempt) and no passage of `LocalDaa`, blue score or time
  may by itself make a block finalizable. *Because:* `heartbeat_clock_acceleration`,
  `private_daa_finality_acceleration`.
* **FINAL-R8 — No release.** No timer, TTL, staleness rule or liveness escape may un-finalize an
  anchor, release a veto derived from it, or admit a candidate that does not contain it.
  *Because:* `dns_veto_expires_on_heartbeat_clock`.
* **FINAL-R9 — Conflicts are reported, not resolved.** A chain (from a peer, IBD or a restart) whose
  `block_finalized_anchor` is neither an ancestor nor a descendant of the node's anchor is a
  `FinalityConflict`. The node MUST keep its chain, MUST NOT adopt the other, and MUST surface the
  conflict to the operator. *Because:* §3.4.
* **FINAL-R10 — Pruning is structural.** The pruning point MUST be the block `m_prune` safe
  anchors below the own claim frontier of `block_finalized_anchor(V)` (§2.3, §3.3; genesis if fewer
  exist), computed by a pure function. A header that commits a pruning point MUST commit exactly
  that value. A node MAY retain more (archival) but MUST NOT delete history above the pruning point.
  Every claim accepted at or below it was resolved in the anchor's own state, which every admissible
  candidate shares. Resolved means terminal, or `Final` with `conviction_ends` elapsed. So no branch
  the node may still select can need its evidence. A later conviction reads only rooted state and
  material the accused serves (05 COURT-R10). No pruning depth is compared with a `DaaSpan`: the
  frontier itself is the bound.
  *Because:* `pruning_deletes_evidence`, `pruning_point_disagreement`.
* **FINAL-R11 — Trust root for joining nodes.** This is node policy, and it reads the node's wall
  clock, which is permitted outside consensus. A node whose own anchor last advanced more than
  `W_trust` of wall-clock time ago MUST be given a trusted checkpoint out of band (block id,
  `FinalizedDaa`, ruleset id) and MUST treat it as its anchor (FINAL-R5). A joining node without an
  anchor is in the same position. `W_trust` MUST be at most `D_exit × SLOT_MS`: the least wall-clock
  time in which an exit can complete, because an exit needs `D_exit` ticks of `LocalDaa`, and
  `LocalDaa` never outruns the slot clock (INV-DAA-02). Peer agreement is not a trust root.
  *Because:* §3.5, `long_range_rewrite`.
* **FINAL-R12 — Withdrawal outlasts finalization, structurally.** An exit MUST complete only when
  `ChainFinalizedDaa` has passed its request by `D_exit` (02 BOND-R14). Consider any node that
  followed the chain on which the exit completed. By then its anchor is at or past that chain's
  anchor (FINAL-R4), so everything the account signed before its request lies below that node's
  anchor. `validate_params` needs only `D_exit ≥ D_max + W_conviction` (02 BOND-R14). No rate of
  settlement enters, and in a licence halt the exit waits. *Because:* §3.5. *[Synthesis edit,
  review: the draft compared the delay with "the worst-case time for `k_final` settlements at the
  minimum settlement rate", which is zero in a halt, and used a unit the rule could not check.]*
* **FINAL-R13 — Overlays only move the anchor forward, if they exist.** An external finality
  signal (a validator vote, an operator checkpoint) MAY be accepted only as an input that proposes a
  *deeper* anchor on the selected chain, verified by a pure function of consensus objects; it MUST
  NOT refuse candidates on its own authority, and it MUST obey FINAL-R5 and FINAL-R8.
  *Because:* `dns_gate_node_local_abstain`, `dns_veto_expires_on_heartbeat_clock`.

## 5. Pure functions

```rust
/// FINAL-R2. Pure in (view, block, params).
pub fn is_finalizable(view: &ChainView, block: BlockId, params: &FinalityParams) -> bool;

/// FINAL-R3. The deepest finalizable block on the view's chain.
pub fn block_finalized_anchor(view: &ChainView, params: &FinalityParams) -> ChainAnchor;

/// 01 §2.4. The only mint of ChainFinalizedDaa: the chain anchor of a block's selected parent.
pub fn chain_finalized_daa(parent_view: &ChainView, params: &FinalityParams) -> ChainFinalizedDaa;

/// FINAL-R4/R5/R9. `selected` MUST contain `current` (chapter 06 guarantees it). Node-side.
pub fn advance_finality(current: &FinalizedAnchor, selected: &ChainView, params: &FinalityParams)
    -> Result<FinalizedAnchor, FinalityConflict>;

/// FINAL-R10. What a header at `view`'s tip commits as its pruning point.
pub fn pruning_point(view: &ChainView, params: &FinalityParams) -> BlockId;

/// FINAL-R9 for an external chain: does `theirs` sit on one line with `ours`?
pub fn check_anchor_compatibility(ours: &FinalizedAnchor, theirs: &ChainAnchor, ancestry: &dyn AncestryOracle)
    -> Result<(), FinalityConflict>;
```

`ChainView` here carries, beyond chapter 06's fields, the per-block running values the predicate
reads — for each chain block, the safe-anchor count and `Ω` up to it, and its own claim frontier —
all maintained in the chain state. `AncestryOracle` is a pure query over headers already validated
(chain membership), not a store handle.

Reference semantics:

```text
is_finalizable(V, B) =
       V.contains(B)
    && V.safe_anchors_after(B) >= k_final                        // (a)
    && V.omega() - V.omega_at(B) >= d_final                      // (b)

block_finalized_anchor(V) =
    deepest B on V's chain with is_finalizable(V, B), else genesis
    (with own_frontier = the claim frontier of B's own state, §2.3)

advance_finality(cur, V) =
    let f = block_finalized_anchor(V)
    if V.contains(f) && f.descends_from_or_eq(cur): Ok(f)        // move forward
    elif V.contains(cur) && cur.descends_from_or_eq(f): Ok(cur)  // V's own rule is behind the node
    else: Err(FinalityConflict { ours: cur, theirs: f })

pruning_point(V) = the block m_prune safe anchors below block_finalized_anchor(V).own_frontier, else genesis
chain_finalized_daa(P) = block_finalized_anchor(P).daa
```

Because `is_finalizable` is monotone in `V` (FINAL-R6), `block_finalized_anchor` can be maintained
incrementally; the incremental form MUST return the same value as the definition.

## 6. Invariants upheld

* **INV-FINAL-01** A node's finalized anchor never reverts: every later anchor descends from or
  equals every earlier one, and no selected tip fails to contain it.
  Test `inv_final_01_the_finalized_anchor_never_reverts`.
* **INV-FINAL-02** The node anchor moves only by `advance_finality` on a selected chain; no timer,
  overlay or node-local flag moves it. Test `inv_final_02_only_selection_advances_finality`.
* **INV-FINAL-03** `block_finalized_anchor` is monotone along a chain and a pure function of the
  chain (archival and pruned nodes compute the same value).
  Test `inv_final_03_block_finalized_anchor_is_monotone_and_pure`.
* **INV-FINAL-04** Every claim accepted at or below the finalized anchor's own claim frontier is
  resolved in the anchor's own state: terminal, or `FinalClaim` with `conviction_ends` elapsed. It
  is therefore resolved identically on every admissible candidate.
  Test `inv_final_04_nothing_unresolved_below_the_anchors_own_frontier`.
* **INV-FINAL-05** Neither blocks without verified work nor the passage of `LocalDaa`, `SafeDaa`,
  blue score or time makes a block finalizable. Safe anchors are clock-free (06 §3.3).
  Test `inv_final_05_empty_blocks_do_not_finalize`.
* **INV-FINAL-06** The pruning point is at or below the finalized anchor's own claim frontier, is the
  same pure function that headers commit, and never deletes evidence an unresolved claim on any
  admissible candidate can need.
  Test `inv_final_06_pruning_never_passes_the_anchor`.
* **INV-FINAL-07** A node that has finalized an anchor never leaves it (INV-FINAL-01). For a node
  that has *not yet* finalized it (lagging, joining with an older trust root) to finalize a
  conflicting one, an attacker needs a branch that wins chapter 06 against the honest chain and
  carries `k_final` safe anchors and `d_final` verified weight of its own. On a branch it produces,
  that costs bucket tokens and licences from its own seats (`P_cap`, 10 §2.1 C11), not inference.
  `k_final` and `d_final` MUST make that cost, and the time the bucket needs to admit it, exceed the
  value such a split could release (ties INV-ECON-01).
  Test `inv_final_07_a_conflicting_finalization_costs_more_than_it_pays`.
* Upheld jointly with chapter 06: **INV-FORK-01**, **INV-FORK-07**.

## 7. t12 reference

### 7.1 What t12 calls final

| Notion | Rule in t12 | Unit | Reverts? | Citation |
| --- | --- | --- | --- | --- |
| claim `Final` (PALW settlement) | licensed claim's deadline `max(L + window_challenge_at(L), …)` passes on the branch | branch DAA | with the branch | `consensus/core/src/palw_state_v2.rs:3233-3243`, sweep `:23666`, finalize `:23771` |
| settlement depth | settled anchors at or after a payment's DAA (read-only RPC) | count | with the branch | `docs/adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md` D2 |
| Kaspa finality point | candidates not in the future of the block `finality_depth` blue score below the virtual are ignored | blue score (600) | node-local, no | `consensus/src/processes/block_depth.rs:55-80`; `consensus/src/pipeline/virtual_processor/processor.rs:13704, 13772` |
| `finality_depth` value | `window_challenge / 2` = 1,200 / 2 = 600 | a DAA window used as blue score | — | `consensus/core/src/config/params.rs:2781`, `:1242` |
| pruning point (header) | GHOSTDAG pruning samples at `pruning_depth` blue score, validated in every chain block | blue score (12,002 at the reference; 74,920 *pending* P-1) | no | `consensus/src/processes/pruning.rs:106-156`; `consensus/src/pipeline/virtual_processor/utxo_validation.rs:1070-1072`; depth derivation `params.rs:2697-2717`; values from commit `2c4b6516` on `feat/t12-class-verify-deadline` (*pending*) |
| pruning (local store) | advance refused above the safe frontier | blue score of the frontier | no | `consensus/src/pipeline/pruning_processor/processor.rs:207-229`; `processor.rs:3958-3978`; `consensus/core/src/palw_fork_authority_v2.rs:70-77` |
| DNS-final anchor | > 2/3 of epoch bonded stake attested and precommitted | votes | **yes: released after 120 DAA stale on the incumbent chain** | `docs/adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md:24-29`; `consensus/core/src/dns_bft_v1.rs:75-83`; `consensus/src/pipeline/virtual_processor/dns_bft.rs:530-567`; `consensus/core/src/dns_finality.rs:1552-1554, 1464` |
| bridge labels | `finalized` = pruning point, `safe` = DNS-confirmed anchor | — | — | `docs/adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md:183-185` |
| second clock | economic deadlines also wait for 30 further settled anchors (with a liveness escape) | settled anchors | — | `params.rs:14700-14711`, `:15901`; `palw_state_v2.rs:2224-2229` |
| trusted checkpoint | admissible chains must descend from an operator-supplied block | — | no | `consensus/core/src/config/trusted_checkpoint.rs:1-37` |

What never reverts in t12: the header-committed pruning point (a function of GHOSTDAG, not of PALW)
and, for a synced node, anything more than 600 blue score below its virtual. The DNS-final anchor is
a *time-limited* veto, and a claim's `Final` is branch-relative.

### 7.2 Defects and how next differs

* **T12-F1 — DAA windows used as blue-score depths.** `finality_depth = window_challenge / 2`
  (`params.rs:2781`) is a DAA quantity, and the pruning depth is `max(blue-score bound, DAA claim
  lattice)` compared against blue score (`params.rs:2697-2717`; used at `pruning.rs:143`).
  On t12 DAA advances about once per 120 s (06 §7.4) while blue score advances with every blue block,
  so the same number means different amounts of history in the two units. The in-tree lane test
  names each depth gate and its unit (`consensus/core/tests/hb_fork_weight.rs:197-227`). The ADR-0128
  evidence window likewise adds DAA terms to a blue-score window (`dns_bft_v1.rs:55-61`).
  *next:* one pair of units — safe anchors and verified weight — for finality and pruning
  (FINAL-R2, FINAL-R10).
* **T12-F2 — the header pruning point does not see PALW.** Headers commit a pruning point computed
  from blue score only (`pruning.rs:106-156`, validated at `utxo_validation.rs:1070-1072`); the PALW
  ceiling gates only the node's own pruning store (`pruning_processor/processor.rs:223-228`). A
  header can therefore commit a pruning point above the safe frontier that the node will not prune
  to. *next:* the committed value is the structural one (FINAL-R10).
* **T12-F3 — the only vote-based finality expires.** The DNS BFT gate refuses candidates that
  abandon the DNS-final anchor until the anchor is stale by 120 DAA measured on the incumbent's own
  chain (`dns_bft.rs:532, 554-563`; TTL `dns_finality.rs:1464`); on t12 that is roughly four hours of
  heartbeat slots with no new DNS-final anchor (`hb_fork_weight.rs:229-244` prints the price). It also
  abstains on a node-local evaluation failure (`dns_bft.rs:457, 534-536`). ADR-0128 calls it "a veto,
  … never selects a tip" (`0128:28-29`). *next:* no release (FINAL-R8); overlays only propose a
  deeper anchor (FINAL-R13).
* **T12-F4 — Kaspa finality is node-local and blue-score deep.** The finality point is computed
  from the node's own virtual (`processor.rs:1508-1509`; `block_depth.rs:55-80`) and a candidate that
  violates it is ignored with a warning (`processor.rs:13772`). Recovery from a finality conflict is
  an operator-driven bootstrap path that orders by blue work
  (`protocol/flows/src/flowcontext/bootstrap_recovery.rs:274-300`). *next:* FINAL-R9 (report, never
  auto-resolve) and one comparator (06 FORK-R10) if an operator re-anchors.
* **T12-F5 — "settled" gates money but not history.** A claim becomes `Final` on the branch's DAA
  (`palw_state_v2.rs:3233-3243`), and reward escrow is payable at `Final` (ADR-0042 Decision 10,
  `docs/adr/0042-palw-mainnet-candidate-ruleset.md:479`), but nothing makes that history
  irreversible. *next:* FINAL-R1 separates the two; reward release stays the claims chapter's.
* **Kept from t12:** the pruning ceiling's reason — "history under trial is not prunable"
  (`palw_fork_authority_v2.rs:66-77`) — becomes the anchor's own claim frontier + FINAL-R10; the second
  clock's idea that only verified, licensed events secure anything (`params.rs:14700-14711`) becomes
  the depth unit, counted as safe anchors so that no clock enters; the
  trusted checkpoint as an admissibility constraint becomes FINAL-R11; P-1's observation that a
  pruning point can never move backward, so the horizon must be chosen at genesis (commit `2c4b6516`
  message, *pending*), is kept as a genesis-fixed `FinalityParams`.

### 7.3 Pending deltas

* `feat/t12-class-verify-deadline` (*pending*): pruning depth 12,002 → 74,920 on t12, from the D_cap
  claim lattice `2(600 + 16,000) + 1,200 + 3,000 + 37,520` (commit `2c4b6516`); the Final floor becomes
  `max(L + window_challenge_at(L), H(c))` (`palw_claim_final_floor_v1`). Both remain DAA quantities;
  the first is still applied as a blue-score depth (T12-F1).
* No pending branch changes the DNS BFT gate, the finality point or the pruning processor (checked
  with `git diff --stat rcore/int-3...<branch>` over those files).

## 8. Attacks this chapter defends against

| Attack | t12 | next defence |
| --- | --- | --- |
| `private_daa_finality_acceleration` | **partial** — claim `Final` follows the branch DAA, which the ADR-0142 cursor paces to wall time on t12 (06 §7.4); Kaspa finality and pruning follow blue score, which is not so paced | FINAL-R2, FINAL-R7 |
| `heartbeat_clock_acceleration` | **partial** — heartbeats advance blue score, hence the finality point and pruning samples; the DNS veto TTL runs on DAA they drive (priced in wall clock) | FINAL-R7, FINAL-R8 |
| `dns_veto_expires_on_heartbeat_clock` (this chapter's earlier name: `finality_ttl_release`) | **real** (T12-F3; by design, as a liveness release) | FINAL-R8 |
| `settled_claim_reverted_with_branch` | **real** as a property (T12-F5); harmful only to readers who treat `Final` as irreversible | FINAL-R1 |
| `pruning_point_disagreement` | **partial** (T12-F2: committed and local pruning points can differ; the committed one is uniform) | FINAL-R3, FINAL-R10 |
| `pruning_deletes_evidence` | **closed** for the local store by the ceiling (`pruning_processor/processor.rs:223-228`) | FINAL-R10 |
| `long_range_rewrite` | **partial** — trusted checkpoint exists for bootstrap (`trusted_checkpoint.rs:1-37`); bond-signed history after withdrawal is not otherwise bounded **[relation of t12's withdrawal delay to finalization time unverified]** | FINAL-R5, FINAL-R11, FINAL-R12 |
| `dns_gate_node_local_abstain` | **partial** (06 T12-D6) | FINAL-R13 |

## 9. Open questions for the project owner

1. **`k_final`, `d_final`, `m_prune`.** Options: (a) derive `k_final` from INV-ECON-01 (the private
   cost of `k_final` safe anchors, bucket tokens and self-licences on a private branch (`P_cap`)
   plus the collateral a colluding panel risks, must exceed
   the largest value a reorg of that depth can release); (b) fix `k_final = 30` like t12's second
   clock (`params.rs:14711`); (c) measure on a drill and fix at genesis. *Recommendation:* (a) for the
   rule, (c) for the number, fixed at genesis because the pruning horizon cannot move backward.
2. **Is there any external finality in the core?** Options: (a) none — PoL depth only; (b) a
   validator vote that may only propose a deeper anchor (FINAL-R13); (c) t12's expiring veto.
   *Recommendation:* (a) at launch, (b) later if wanted; never (c).
3. **Reward release at `FinalClaim` versus at finalization.** Options: (a) keep t12's release at
   `Final` (branch-relative; a reorg reverts the payment with the branch); (b) release only below the
   finalized anchor. *Recommendation:* (a) for liveness of honest producers, with wallets and bridges
   required to show finalized depth (FINAL-R1); the claims chapter decides. *Synthesis note:* 02
   BOND-R12 already releases a reward's vesting row only when `ChainFinalizedDaa` passes its expiry, which
   is option (b) for the reward legs, read on the *chain's* anchor, not the node's. The book now
   states (b) normatively in one place, 01 §2.7, and 02, 03 and 08 cite it. The consolidated
   question is OQ-11 in `00-overview.md` §10.
4. **What does a node do on a `FinalityConflict`?** Options: (a) keep its chain and alert; (b) halt
   block production until an operator acts; (c) follow the heavier side. *Recommendation:* (a) for
   validation and relay, (b) for its own production; (c) is forbidden by FINAL-R8.
5. **Finality stall.** If safe claims stop, finality and pruning stop and storage grows. Options:
   (a) accept it (safety first; alert); (b) a liveness escape that finalizes on elapsed `LocalDaa`
   alone. *Recommendation:* (a); (b) re-opens `dns_veto_expires_on_heartbeat_clock`'s flaw (a timer moving finality) in the other direction.
