# RFC-0009 L2 — verifying the PALW fork choice without a full node (lane L2FC, 2026-10-09)

Lane L2FC · branch `rfc9/l2-fork-choice` (from the integration head `b676927de`; integration `b8ae9412b` merged 2026-10-10, with the
user's design changes ADR-0175–0177 and rule E renumbered ADR-0178) · closes the L2 DESIGN_GAP left by C1r2
([`rfc-0009-remote-record.md`](rfc-0009-remote-record.md), round 2) · RFC: [0009 §「検証の 3 層と fork-choice gate」](../../rfc/0009-palw-remote-miner.md).

Words as in the RFC-0012 record: **MEASURED** (printed by a test on this branch, named), **DERIVED** (arithmetic on shipped constants or
on a MEASURED number), **ESTIMATE** (a stated assumption), **PROPOSED**, **GAP**. Status words as in the integration matrix.
Nothing here ran against a live node or network.

## 0. The answer

A remote client can verify the PALW fork choice only if three separate things hold, and each needs its own mechanism:

1. **Values bound to an authenticated root.** The comparator's inputs (safe frontier, safe weight, bounded immature → live total)
   must be read out of a root the chain commits. Today they cannot be: the ADR-0043 state root is a flat preimage in which
   `safe_weight` sits mid-preimage behind Some-only blocks, so no opening of it is addressable. **PROPOSED: a versioned commitment —
   the header commits `H(fork-choice leaf ‖ ADR-0043 root)` past a dormant fence — whose opening is 356 B and O(1). The leaf
   reserves a versioned weight-allocation slot (ADR-0176 D3, §2.5), so a bond-budget-capped weight is committed later without a new
   envelope version, and it carries no model-availability condition (ADR-0177, §2.6).**
2. **Transition validity of that root.** A correct opening of a root on a valid-header branch proves nothing if the root itself is
   not the fold of that branch's history. The fold's inputs are the full node's (UTXO acceptance, mergeset GHOSTDAG, the DAA window,
   the EVM lane, round verdicts, attempt admission); there is no PALW-only replay. **PROPOSED for `VERIFIED_REMOTE`: an attested root
   (a trusted checkpoint, as the user's ruling allows) — a signed statement "block B's post-state commits root R" from issuers the user
   chose, checked against the opening and against B's chain children when they exist.** Full re-execution is the FULL_NODE path (a
   pruned node from a checkpoint; costs in §4). A succinct proof of the fold is research; the commitment of (1) is its interface.
3. **The same rule, over every candidate the network shows.** The client never computes an order of its own: it evaluates the node's
   own decision functions (`palw_fork_authority_v2`) on the verified inputs, in both directions, with every input it cannot verify
   taken both ways; it chooses only when every in-force variant agrees, and STOPs otherwise. Hidden tips are caught only by a second
   independent peer (or an attestation naming a block no peer showed).

Until (1)–(3) hold for the view at hand the label stays `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` (or lower). On testnet-12 as shipped,
the DNS BFT gate runs in front of the PALW comparator (armed from DAA 0, not retired), but it refuses only a candidate that abandons a
**confirmed** DNS-final anchor in the overlay's **Active** stage; live testnet-12's overlay is in **Bootstrap** (nothing confirmed), so
the gate never refuses there and the comparator decides. The client reads that state from the attestation (the issuer's node's gate
facts) and resolves a conflict by the comparator only when no attested Active-stage anchor stands on one side alone (§5.3).

A fourth fact comes free with (1): past the fence a header's committed root opens to a leaf that **names the block whose post-state it
commits — its selected parent**. L1 cannot check GHOSTDAG's selected-parent choice (it does not serve the other parents' headers), so a
peer could route a view through a merged block whose own root nobody ever checked. Walking openings down from an attested block proves
the path is that block's selected chain (§2.4); the client reads a path (fork point, finality seal, a DNS anchor's side, D2's base, L3
below the attested block) only where it was walked.

## 1. What a full node actually decides

The authority is `palw_fork_choice::compare_palw_candidates_v1` (frontier, safe weight, live total, hash), but no node calls it alone.
A sink move that is not an extension of the node's previous sink passes `dns_reorg_outcome`, in this order:

| Step | Rule | Inputs | Light-client view |
|---|---|---|---|
| 1 | DNS BFT gate (`dns_bft_gate_refusal`) unless `palw_dns_retirement` is active at the incumbent's DAA; it refuses only a candidate abandoning a confirmed anchor in the Active stage | the node's overlay state (stage, confirmed anchor) | the issuer's attested gate facts: Bootstrap / nothing confirmed → the comparator decides; an Active anchor on one side only → STOP |
| 2 | `decide_deep_reorg_v2` — or, past `palw_reorg_strict_economic_win` (read at the incumbent's DAA), `palw_reorg_strict_economic_win_v1` with the shallow-GHOSTDAG tie callback | both candidates' `PalwCandidateOrderV1`; a tie: depth ≤ 2 DAA, DAA not lowered, GHOSTDAG order | orders: from verified openings; the tie callback: evaluated both ways |
| 3 | ADR-0065 D2 frontier provenance (`palw_frontier_provenance`, `None` on t12) | bonds minted on the challenger's branch after the fork; their panel seats | counted exactly from two verified leaves (the tip's and the fork block's, §2.1): if the count cannot reach a quorum, D2 cannot veto |
| — | finality: a candidate below the node's finality point is never weighed | finality depth (600 blue on t12) | a conflict whose fork is ≥ `finality_depth` below either tip is a sealed split → STOP |
| — | IBD commit (`decide_ibd_commit_v2` / `palw_ibd_commit_strict_economic_v1`) | orders | evaluated with the same inputs |

Every rule above is applied by a node to its own previous sink, with its own stores, so a node's answer is node-local; and ADR-0178
(rule E, adopted 2026-10-09; lane FINX) changes the comparator itself (§5.4). A light client therefore cannot claim "the canonical
chain"; it can claim exactly this:

> **L2 holds for tip T** iff T's chain is the only chain any configured peer shows past the checkpoint, or T *robustly dominates*
> every other shown candidate — every in-force decision function, evaluated on attested inputs in both directions with every
> unverifiable input taken both ways, keeps T and refuses the other — and nothing node-local (DNS veto, D2 beyond its bound, the
> finality seal) can intervene.

## 2. The commitment (consensus, dormant fence)

### 2.1 The leaf — what the comparator reads, and what binds it

`PalwForkChoiceLeafV1` (new module `consensus/core/src/palw_fork_choice_commitment_v1.rs`), fixed-size borsh, 292 bytes:

| Field | Type | Source in `PalwChainStateV2` | Why |
|---|---|---|---|
| `leaf_version` | u16 = 1 | — | the opening format the client parses; a comparator that reads more bumps it under its own fence |
| `block`, `daa_score`, `blue_score` | Hash64, u64, u64 | `last_point` | whose post-state this is: the client checks `block` against the candidate it weighs |
| `safe_frontier_blue_score`, `safe_frontier` | u64, Hash64 | `safe_frontier()` | comparator key 1 (and the frontier block, for reachability checks) |
| `safe_weight` | u128 | `safe_weight()` | key 2 |
| `bounded_immature` | u128 | `bounded_immature()` | key 3 (`live_total` is constructed by `PalwCandidateOrderV1::new`, never carried) |
| `bonds_len` | u64 | `bonds_iter().count()` | D2's count: the registry is append-only (ADR-0065 D5; a bond row is only ever removed by a delta undo), so the bonds minted on a branch after its fork = `bonds_len(tip) − bonds_len(fork block)`, both read off verified leaves |
| `weight_allocation` | `PalwWeightAllocationSlotV1` (98 B: `version` u16, `allocation_root` Hash64, `capped_safe_weight` u128, `capped_bounded_immature` u128) | `NONE` (version 0, all zero) in this tree | ADR-0176 D3: which versioned bond-budget allocation the comparator's weights are read through (§2.5) |

Every leaf field is already part of the ADR-0043 root except `bonds_len`, which is derived from it, and the slot, which is all zero until
a bond-budget allocation exists; the leaf adds no state.

### 2.2 The root a header commits

```text
below the fence:   header.palw_state_root = state.state_root()                                  (byte-identical to today)
at/past the fence: header.palw_state_root = BLAKE2b-512_keyed("misaka-palw/state-root/fork-choice/v1",
                                                               borsh(leaf(state)) ‖ state.state_root())
```

* **The fence is keyed on the committed state's own point** — `state.last_point.daa_score ≥ F` — not on the committing header, so the
  commitment form is a property of the state: every chain child of a block commits its post-state in one form, and the template,
  the validator, the proof server, the carriage import and the native-settlement walk compute the same value through one helper,
  `palw_committed_state_root_v1(state, fence)`.
* **Nothing else moves.** `PalwChainStateV2::state_root()` (the ADR-0043 preimage), the carriage, the deltas, the stores and the delta
  roots keep their bytes; only what a header commits changes. No object tag, delta number, carriage tail or preimage block is taken.
* **Fence** (allocated by the Lead, registry §2): `Params::palw_fork_choice_commitment_v1: Option<ForkActivation>`, `None` on every
  preset, collapsed from `Some(never())`, refused when armed (`validate_palw_fork_choice_commitment_v1`, inside `validate_palw_v2`) until
  the full-activation release arms it; hashed into both fingerprints only when `Some`, so no live id moves; a `palw_fences_v1` entry and
  a fork-id probe arm.
* **Sites** that compare or produce a header root (all in `kaspa-consensus`): chain validation (`header.palw_state_root != parent_root`),
  the template stamp, `palw_state_proof_v1` (op 202), `import_pruning_point_palw_state` (the witness child's root), and
  `native_settlement`'s roots chain (`native_roots_chain_v1`: it holds only delta roots; it rebuilds each leaf backwards from the
  sink's by reverting the `Weights` / `Frontier` / `LastPoint` / `Bond` entries of each delta — `PalwForkChoiceLeafV1::parent_by`; the
  leaf part of a delta is kept in the row cache only where the fence is configured, boxed, so a dormant row keeps its size).

Why not a block inside the ADR-0043 preimage: it needs a new state field (a carriage tail and a delta entry to journal the switch),
the opening would be the whole preimage (≈ 4–8 KB), and its offset would depend on every future Some-only block staying in front of
it. Why not a tree over every collection now: L2 needs four scalars; a tree-shaped L3 is a separate, larger change (the leaf version
leaves room: a v2 leaf can add a collection-tree root).

### 2.3 The opening

`PalwForkChoiceOpeningV1 { leaf: PalwForkChoiceLeafV1, inner_root: Hash64 }` (356 B on the wire). The client:

1. obtains the committed root R for block B's post-state from **an attestation of (B, R)** (§3) and, when B has a chain child C on a
   verified view, also from C's header (`C.palw_state_root`, C hashing to its id, C naming B as a parent) — the two must agree;
2. checks `B.daa_score ≥ F` per its own ruleset (else the state predates the commitment: no opening exists, L2 cannot hold);
3. checks that the leaf's weight-allocation slot is a version it reads and canonical (§2.5), `envelope(leaf, inner_root) == R`,
   `leaf.block == B`, `leaf.daa_score == B.daa_score`, `leaf.blue_score == B.blue_score`;
4. builds the order with `PalwCandidateOrderV1::new(leaf.safe_frontier_blue_score, w_safe, w_immature, B)`, where `(w_safe,
   w_immature)` are the leaf's own weights at slot version 0 and the slot's budget-capped weights at a version ≥ 1
   (`PalwForkChoiceLeafV1::order`, the one constructor every reader uses).

L3 composes: past the fence, an op-202 proof is checked against `inner_root` (the opened ADR-0043 root), not against the header root.

### 2.4 The leaf names the selected parent (selected-chain verification)

A valid chain block's header commits its **selected parent's** post-state, and past the fence that commitment is an envelope whose leaf
names the block (`leaf.block`). So for two consecutive headers P, C on a view, an opening with `leaf.block == P` that hashes to
`C.palw_state_root` proves P is C's selected parent — **provided C's root is itself true**. An attested block D's root is (its issuer's
node computed D's post-state, which a node does only after every header root on D's selected chain passed its check — a mismatch
disqualifies the block and every chain descendant before any delta is written, and op 203 serves no opening without the delta). By
induction, walking openings from D downward (`verify_selected_chain_v1`) proves the walked path is D's selected chain and that every
root on it is true.

Without the walk, L1's "names the previous header as *a* parent" lets a peer route a view through a merged block: such a block's own
root is never checked by any node (only chain blocks' roots are), so it can commit a forged state of its parent, and a proof under it
would be "correct". The client therefore reads no path fact it has not walked. Below the fence there is no leaf and no walk: L3 under L2
and conflict resolution do not exist there (C1r2's checkpoint rule remains).

**The tip's own post-state is committed by no header until a child is mined** (the header commits its selected parent's state —
non-circular by design). So a candidate tip is weighed through its attestation; a child header, when present, is the cross-check.

### 2.5 The weight-allocation slot (ADR-0176 D3)

ADR-0176 bounds claims, reward blocks, rewards and **Final weight** per bond over a common DAA window, and D3 requires that, once
`palw_bond_budget_v1` (lane BUDGET, dormant) is armed, every writer and reader — fork choice, DAA, RPC, EVM, remote clients, undo/reorg/IBD
— read the **same versioned allocation**. The commitment must therefore be able to carry a budget-capped weight. It does so without a new
envelope (state-root) version: the v1 leaf reserves a fixed-size slot now.

| `version` | Meaning | What the comparator reads |
|---|---|---|
| 0 (`PALW_WEIGHT_ALLOCATION_NONE_V1`) | no allocation in force at the state's point (every state today); every other slot field is zero — one encoding, refused otherwise | the leaf's `safe_weight`, `bounded_immature` (the fold's) |
| n ≥ 1 | version n of `palw_bond_budget_v1`'s allocation is in force at the state's point | the slot's `capped_safe_weight`, `capped_bounded_immature`: key 2 and key 3's addend as the allocation bounds them; `allocation_root` commits the per-bond allocation they were computed from, so a reader can open one bond's allocation under the same header root |

Rules, all in code:

* **One reader.** `PalwForkChoiceLeafV1::order()` / `comparator_weights()` is the only constructor of the comparator's input from a leaf;
  op 203 serves the leaf bytes, slot included; the remote client orders through the same function. So the node, the RPC and a client
  cannot read different weights for one state.
* **Fail closed on a version this build does not read.** `PALW_WEIGHT_ALLOCATION_READ_MAX_V1 = 0`: `PalwForkChoiceOpeningV1::verify`
  refuses a slot version above it (`WeightAllocation`), even under a root an issuer signed; `decode` refuses a non-canonical version-0
  slot; `parent_by` (the native-settlement walk's leaf rebuild) refuses a non-empty slot, so that walk breaks rather than assumes.
* **The client knows where an allocation is due.** `ForkChoiceRulesV1::bond_budget` (`None` until the fence is in this tree): an attested
  leaf naming version 0 at a point past the fence is refused (it would be weighed by uncapped weights); a conflict where the fence may be
  in force STOPs (`L2StopV1::BondBudgetAllocation`) because this build reads no allocation version.
* **What lane BUDGET does at integration** (one place each): fill the slot in `PalwForkChoiceLeafV1::of` (the helper every producing and
  checking site builds a leaf through) for a state past its fence; define version 1's `allocation_root` and raise
  `PALW_WEIGHT_ALLOCATION_READ_MAX_V1`; set `ForkChoiceRulesV1::bond_budget` from its fence; give `parent_by` the allocation's delta. The
  leaf length, the envelope key and every other field stay.
* **No reward or weight is added by this lane.** The commitment carries weights the fold (or, past `palw_bond_budget_v1`, the
  allocation) already computed; it draws on no budget and creates no credit.

Every later leaf version (FINX's leaf v2 for rule E, §5.4) keeps the v1 fields — the slot included — at their v1 offsets, so the walk
and the allocation read the same bytes whatever comparator is in force.

### 2.6 No model-availability condition (ADR-0177)

The chain does not interfere with model acquisition. No leaf, slot, opening, attestation or L2 verdict carries or reads whether a
registered model can be fetched, served, seeded or leased: the leaf's fields are exactly the table of §2.1 (a fixed 292 bytes, checked
by `the_leaf_is_its_fixed_size_and_round_trips`), the attestation's are §4's, and none of them is an availability fact. A candidate is
never weighed, refused or downgraded because a model is or is not available.

## 3. Transition validity — the options and their costs

The fold `apply_palw_transition_v7` takes, per chain block: the parent PALW state; the PALW objects of the block's *accepted*
transactions (acceptance is UTXO validation: funding, fees, mass); the block's own work (attempt admission, which reads the bond, the
class target and the PoW state); the mergeset's works classified by GHOSTDAG (blues/reds, `merged_non_daa` from the DAA window); the
EVM lane's staged market actions; ADR-0125 round verdicts; the audit beacon source. **There is no PALW-only replay**: re-executing
the fold means running the consensus pipeline.

| Option | What the client needs | testnet-12 cost | Soundness | Verdict |
|---|---|---|---|---|
| **A. Full re-execution from a checkpoint** (a pruned node: `kaspa-consensus` embedded, blocks fetched from untrusted peers) | the pruning-point state (UTXO set, PALW carriage, the headers proof and DAG window) and every block since | PALW carriage ≈ 57 MB and growing (pruned-IBD record); t12's pruning point stays near genesis (pruning depth 74,920 blue ≈ 25k DAA), so a sync today is the whole chain: ≈ 9k DAA × ~3 blocks; per chain block the fold plus one `state_root()` over the whole state (§6, MEASURED) | sound (independent verification) | **the FULL_NODE path**; labelled `FULL_NODE`, not a light client |
| **B. Bounded window** (trusted state at C, re-execute C→T) | the same import at C (UTXO + PALW + DAG window), then ≤ W blocks | dominated by the import (≥ 57 MB); the window saves only replay time; and kaspad imports state only at the pruning point, so a recent-C import is new node work | sound given the trusted C | no saving over A; not built |
| **C. Succinct proof of the fold** (zkVM/STARK per block, aggregated) | a proof per view + the opening | proving a 76k-line fold with ML-DSA-87 verification and full-state BLAKE2b per block: no known prover is near real time | sound | research; the §2 commitment is its interface (a proof of "R is the fold" + the opening = verified keys) |
| **D. Attested root** (trusted checkpoint at a bounded lag) | per candidate: one attestation (≈ 4.9 KB, ML-DSA-87) + one opening (356 B) | §6 | sound **relative to the issuers' trust**: the issuer re-executed (it runs a full node); the client verifies the binding, the chain linkage, freshness and the comparator | **RECOMMENDED for `VERIFIED_REMOTE`** |
| E. Bonded attestation with objective refutation | D, with the issuer's bond slashable for an attested root that disagrees with the chain's own child-header commitment | as D | as D, plus accountability | future (a court object; out of this lane) |

**Recommendation: D for `VERIFIED_REMOTE`, A for `FULL_NODE`, C as research.** D is what the user's ruling calls a trusted checkpoint,
generalized from "the checkpoint is the decision point" (C1r2's rule, lag ≤ 1 DAA, conflicts STOP) to: an attested root whose values
the client opens itself, cross-checks against the chain's own child-header commitment, and feeds to the node's own decision functions.
What D adds over C1r2's rule: the values come from the chain's commitment, not from the issuer's word (an issuer that signs a root
the chain contradicts is caught by the first child header); two attested candidates are resolved by the comparator instead of a STOP
(where nothing node-local can intervene); and an attestation naming a block no peer shows is itself a hidden-tip signal.

An attestation at block D covers every root on D's selected chain: D is valid only if each chain ancestor's child header committed the
true fold (the issuer's node disqualifies a block whose header root disagrees). So L3 facts proven at a header at or before D are
covered; a header past D is covered only if it commits D's attested root (D's child).

## 4. The attestation (the checkpoint, generalized)

`ForkChoiceAttestationV1` (client library, `misaka-palw-remote::l2`): `{ network_id, consensus_params_id, consensus_schedule_id,
block, block_daa, committed_root, leaf_version, issued_at_daa, key_id, signature }`, signed over a domain-separated digest
(`misaka-palw/remote/fork-choice-attestation/v1`) with the primitive injected (ML-DSA-87 in the binaries), exactly as
`SignedCheckpointV1`. Rules:

* **Trust:** a key the client holds before it talks to any node (`--checkpoint-trust signed` key list); an own node over an
  authenticated channel is the 1-of-1 case. `k`-of-`n` issuers may be required (default 1); attestations from different issuers that
  name conflicting blocks are candidates to resolve (§5), not an error.
* **Ruleset:** the three ids must be this build's (a node or issuer on another ruleset is refused, as in L1).
* **Freshness:** `now_daa − issued_at_daa ≤ max_attestation_age_daa` (default 2) — older is `Stale` and refused by name;
  `issued_at_daa > now_daa` is refused. `now_daa` is the conservative tip DAA of the verified views.
* **Lag:** a single-chain view is L2-verified while its tip is ≤ `max_attested_lag_daa` (default 1, C1r2's bound) past the attested
  block on it. For conflict resolution the attestation must be **at each candidate's tip** (lag 0): weighing tips by older states would
  not be the node's inputs.
* **What it states:** the issuer's full node computed `block`'s post-state (so `block`'s whole selected chain passed validation, every
  header root included — §2.4) and that post-state commits `committed_root`. An issuer service signs only roots op 203 served.
* **Binding:** the opening must hash to `committed_root` (§2.3); every header that names `block` as its predecessor on any view must
  commit the same root, or STOP (named: the issuer is contradicted by the chain — unless that header is invalid or off the selected
  chain, which the client cannot tell without a second source, so the user drops the issuer only when another source confirms it).
* **Placement:** `block` must be on some verified view. An attestation of a block outside every view means some peer is hiding a tip
  (or the issuer is on another branch): STOP.
* **Restart:** nothing is carried over; the verdict is a pure function of the checkpoint, the attestations and the bytes served now.

## 5. Applying the comparator

### 5.1 Candidates

From ≥ `min_peers` (default 2) independent peers, each view L1-verified from the same checkpoint (existing `verify_header_chain_v1`).
Views are grouped by containment into maximal chains; each maximal chain's tip is a candidate. One candidate → §4's lag rule.
Several → every candidate must be weighable (attested at its tip, opening verified); an unweighable candidate is **not** treated as a
loser — the node that refuses an unweighable challenger is weighing with its own state, which the client does not have — so it STOPs.
Each candidate's path is then walked (§2.4) from its attested tip down to the deepest fork point it takes part in (and to any DNS anchor
it must show); a path that does not walk is a STOP. On two walked paths the highest shared block is the fork point of the two selected
chains (above it they share nothing), so the finality seal, the anchor's side and D2's base are read off verified data.

### 5.2 Robust dominance (the SAME functions, never a new order)

For candidates c and o with verified orders, `c` robustly dominates `o` iff, for every variant the client's ruleset may have in force
at either tip's DAA **or at the fork point's** (a node's incumbent — whose DAA the node reads its fences at — may stand anywhere between
them):

* `decide_deep_reorg_v2(o, c) == Allow` and `decide_deep_reorg_v2(c, o) == Refuse`;
* where `palw_reorg_strict_economic_win` may be active: `palw_reorg_strict_economic_win_v1(o, c, || x) == Allow` and
  `palw_reorg_strict_economic_win_v1(c, o, || x) == Refuse` for **both** `x = true` and `x = false` (the shallow-GHOSTDAG tie answer
  needs the mergeset and GHOSTDAG, which L1 does not verify);
* the IBD rules likewise (`decide_ibd_commit_v2`, and `palw_ibd_commit_strict_economic_v1` where its fence may be active).

The chosen tip is the unique candidate that robustly dominates every other; none → STOP. `select_palw_tip_v2` must agree (a
consistency assertion). The client links `kaspa-consensus-core` and calls these functions; if a release changes them, the client
follows with the same build.

### 5.3 What can intervene, and the answer for each

| Node-local rule | When the client may resolve a conflict | Otherwise |
|---|---|---|
| DNS BFT gate | the gate cannot run at any tip (retired, or no overlay); or every attestation carries the issuer's gate facts and none names an Active-stage confirmed anchor at or above the checkpoint that is not on every candidate's walked selected chain (**Bootstrap**, live testnet-12: nothing confirmed, the gate never refuses) | STOP: "a DNS-final veto may decide" (also when an attestation carries no gate facts) |
| a bond-budget allocation (ADR-0176 D3, `palw_bond_budget_v1`) | its fence inactive at every tip | STOP (`BondBudgetAllocation`) until this client reads the allocation's slot version (§2.5) |
| D2 frontier provenance | fence inactive at every tip and fork DAA, or for the chosen c and each other o: `!palw_minted_seats_can_reach_quorum_v1(bonds_len(c) − bonds_len(fork(c, o)), panel)` | STOP |
| finality seal | the fork point is < `finality_depth` blue below each tip | STOP: sealed split |
| a comparator the v1 leaf cannot feed (ADR-0178 rule E, `palw_fork_choice_rule_e_v1`) | its fence inactive at every tip and fork DAA | STOP (`LeafV1Insufficient`) |

**The DNS gate in the mode output.** Every established verdict names how the gate was accounted for (`L2DnsGateV1`, printed on the
trust line beside the issuer): one chain — nothing to refuse; the gate cannot run; **attested outside its Active stage or with nothing
confirmed (Bootstrap) — it refuses nothing, the comparator decided** (live testnet-12); or its confirmed anchor stands on every
candidate. The node serves its gate facts with op 203 (`dnsOverlay`, `dnsStageActive`, the confirmed anchor), read from its own DNS
state; an unwritten state reads as nothing confirmed outside Active, which is what the gate itself reads.

### 5.4 A comparator that changes (ADR-0178, rule E)

Rule E (ADR-0178, adopted 2026-10-09; implemented by lane FINX behind `palw_fork_choice_rule_e_v1`) reads inputs the v1 leaf does not
carry. The contract: **the leaf is versioned by what the in-force comparator reads; a comparator fence that needs more ships its leaf
version under the same fence.** E's leaf (v2) is specified and built by lane FINX under `palw_fork_choice_rule_e_v1` (agreed with this
lane 2026-10-09; FINX's note is internal): it keeps the v1 fields at their v1 offsets — **since 2026-10-10 that includes the §2.5 slot,
an integration item for the merge of FINX's leaf v2** — follows them with fixed-size commitments from which a client opens, per
candidate, exactly what E's decision reads, verified by consensus-core functions the node itself uses, adds no state field and leaves
`state_root()` unchanged. Until it lands the client STOPs a conflict wherever E may be in force (`ForkChoiceRulesV1::rule_e`,
`L2StopV1::LeafV1Insufficient`) — a single chain needs no comparator and is unaffected. Since `palw_dns_retirement_v1` requires E armed at
or below it, every network where the DNS gate is retired has E in force: there, v1 openings resolve no conflict, and conflict resolution
waits for leaf v2. The robust evaluation of §5.2 (every node-local input taken both ways), the walk of §2.4 and the attestation of §4
carry over unchanged; only the inputs opened per candidate change. Validation must then require `palw_fork_choice_commitment_v1` armed at
or below E (a clause in E's validator, written where both fields exist).

**Integration note.** `ForkChoiceRulesV1::of` sets `rule_e: None` and `bond_budget: None` because neither fence is in this branch's
`Params`; the merges that bring FINX's and BUDGET's fences set them from `params.palw_fork_choice_rule_e_v1` and
`params.palw_bond_budget_v1`.

## 6. Cost per verified view (testnet-12)

| Item | Size / time | Basis |
|---|---|---|
| chain headers, per DAA | ≈ 8 KB/DAA (≈ 5.6 MB/day) | DERIVED from C1r2's ~5–6 MB/day at ~29 DAA/h; an attempt header ≈ 8 KB (ML-DSA-87 key 2,592 B + signature 4,627 B), a heartbeat ≈ 0.7 KB |
| opening | 356 B | DERIVED (292-byte leaf — 194 B of point and keys, 98 B of the §2.5 slot — + 64-byte inner root) |
| attestation | ≈ 4.9 KB | DERIVED (ML-DSA-87 signature 4,627 B + fields) |
| one refresh, 2 peers, 1 candidate | ≈ 21 KB per DAA ≈ 15 MB/day | DERIVED: 2 × 8 KB headers + opening + attestation |
| each extra candidate (conflict) | + ≈ 5.3 KB | DERIVED |
| cold start from a checkpoint ≤ 1,000 headers back | ≈ 2.4 MB | DERIVED (≈ 300 DAA × 8 KB) |
| CPU per refresh | see the MEASURED table below | `l2fc_cost_per_view` (ignored test, `misaka-palw-remote`) |
| option A, per chain block | one `state_root()` over the state | MEASURED below on a synthetic state of t12's carriage size |

MEASURED: §9.

## 7. Downgrade behaviour

| Condition | Label | Signs / executes? |
|---|---|---|
| own node | `FULL_NODE` | yes |
| L1 + L3 + L2 (§1 statement) | `VERIFIED_REMOTE` | yes |
| L1 + L3, L2 not established (no fresh attestation, lag too large, fewer than `min_peers` views, pre-fence state, unweighable candidate) | `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` | only with `--accept-unverified-state` naming it |
| conflicting views not resolvable (§5.1–§5.3), an attestation naming a block outside every view, an issuer the chain contradicts | **STOP** (no label above `UNVERIFIED_REMOTE`; nothing signed or started, opt-in or not) | no |
| facts read from nodes only | `UNVERIFIED_REMOTE` | only with the opt-in naming it |

The gate is asked before inference and again right before the signature (unchanged).

## 8. Implementation (this lane) and allocations

* consensus-core: `palw_fork_choice_commitment_v1` (leaf, the ADR-0176 D3 weight-allocation slot, envelope, opening verify,
  `parent_by`/`parent_by_delta`, the served types and the DNS gate facts), the fence field (Some-only hashed in both fingerprints, `never()` collapse, refused when armed, a fork-id probe arm,
  a `palw_fences_v1` entry).
* consensus: the five sites through `palw_committed_root_v1`; `palw_fork_choice_openings_v1(blocks)` on the consensus API (the sink, the
  tips, ≤ 16 openings, the DNS gate facts).
* RPC: `getPalwForkChoiceOpening`, **op 203** (allocated), over wRPC and gRPC (fields 1220/1221).
* client (`misaka-palw-remote::l2`): the attestation, opening verification, candidate grouping, the selected-chain walk, robust
  dominance (fork-point DAA included), the DNS Bootstrap handling (named on the trust line, `L2DnsGateV1`), D2 from verified leaves, the
  rule-E STOP hook, the bond-budget hook (§2.5), the L2 verdict with its trust line, L3 under L2 (`l3_root_under_l2_v1`), and `L2StatusV1::EstablishedByAttestation` → `VERIFIED_REMOTE` only with L1 and L3.
* remote miner (`misaka-palw-remote::miner`): `RemoteVerificationV1::fork_choice` (`RemoteForkChoiceV1`: the issuers, an attestation
  channel, the rules) makes `remote_mode_v1` decide L2 by attestation — openings asked of every node (`RemoteNode::fork_choice_openings`,
  op 203), the views weighed (a STOP halts the step), the template checked against the CHOSEN tip, the bond and class proven under the
  attested root; without a fresh attestation the class is `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` with L3 at the tip's header (the
  envelope unwrapped by the predecessor's opening, `verify::l3_root_at_header_v1`). The L2 line (`MinerState::last_l2`) is printed with
  the mode (`palw-remote-miner`'s `mode` event, field `l2`) in every class below `FULL_NODE`: the attestation's issuer and how the DNS
  gate was accounted for, the trusted checkpoint, or why L2 is not verified. The binary implements op 203 for its nodes.
* tests: §10.

**Allocations used:** the fence `palw_fork_choice_commitment_v1` and op 203 (registry §2). No object tag, delta number, carriage tail
or preimage block is taken.

**Not in this lane (stated):** the issuer channel. `palw-remote-miner` configures `fork_choice: None` — there is no issuer service yet
(nor a flag naming one): a service that signs, for the roots op 203 serves, fresh attestations ≤ 2 DAA old, or the user's own node over
an authenticated channel. Until one exists the binary's L2 stays C1r2's checkpoint rule (and past the fence its C1 L3 — a proof against
the header root — fails closed). An attestation needs no new consensus allocation; serving one would take an RPC op (Lead's).

## 9. Measurements

MEASURED by `l2::tests::l2fc_cost_per_view` (`cargo test -p misaka-palw-remote --lib l2fc_cost -- --ignored --nocapture`, run
2026-10-09 on this Mac, **debug build**, through `buildslot.sh` at 3 jobs; log `l2fc-m2-cost.log`). Debug timings overstate a release
binary; they bound it from above.

| Item | MEASURED | Note |
|---|---|---|
| L1 over a cold start of 1,000 heartbeat headers | 1.45 ms (1.45 µs/header) | heartbeats carry no signature; an attempt header adds one ML-DSA-87 verification |
| one ML-DSA-87 verification | 14.6 ms | per attempt header in L1, and per attestation |
| the L2 verdict, two weighed candidates (attestations checked with the toy primitive, both paths walked, robust dominance) | 21.6 µs | the comparator work is negligible; the cost is the signatures |
| a heartbeat header, borsh | 764 B | §6's ≈ 0.7 KB |
| an opening | 258 B | 194-byte leaf + 64-byte inner root, measured before the §2.5 slot; with it 356 B (DERIVED, `the_leaf_is_its_fixed_size_and_round_trips` asserts it) |
| option A, one `state_root()` over a synthetic state of 20,000 bonds (bond table 56.4 MB ≈ t12's carriage) | 151 ms | per chain block re-executed; ≈ 9k DAA × ~3 blocks ⇒ ≈ 70 min of hashing alone for a from-genesis replay (DERIVED) |
| `PalwForkChoiceOpeningV1::of` on that state (what op 203 pays per block) | 95 ms | one `state_root()` plus the bond count |

DERIVED per refresh (2 peers, one chain, an attestation at the tip): L1 over the new headers (µs each, plus 14.6 ms per attempt
header), one attestation signature (14.6 ms), one opening hash and the L2 verdict (µs) — **≈ 15–45 ms per refresh in debug**, dominated
by ML-DSA-87. A conflict adds per candidate one attestation (14.6 ms) and the walk to the fork (one 356-byte opening and one BLAKE2b
per block). Node side: op 203 rebuilds each named block's state (the same walk as op 202) and hashes it once — ≈ 0.1 s per block at
t12's carriage size in debug, at most 16 per request.

## 10. Tests (mandatory list → test)

| Required | Test |
|---|---|
| a raw-blue-work-heavier but PALW-losing fork is never chosen | `l2::tests::a_heavier_blue_work_fork_that_loses_the_palw_order_is_never_chosen` |
| a correct Merkle proof of a non-canonical state is refused | `l2::tests::a_correct_proof_of_a_non_canonical_state_is_refused` |
| a hidden competing tip is detected through a second peer | `l2::tests::a_hidden_tip_is_found_through_a_second_peer_or_an_attestation` |
| a stale checkpoint is refused | `l2::tests::a_stale_attestation_or_checkpoint_is_refused_by_name` |
| restart with a different peer reaches the same verdict | `l2::tests::a_restarted_client_with_another_peer_reaches_the_same_verdict` |
| a view routed through a merged block with a forged root is refused (L3 and conflict) | `l2::tests::a_path_through_a_merged_block_with_a_forged_root_is_refused` |
| DNS gate: Bootstrap → the comparator; a one-sided Active anchor → STOP | `l2::tests::the_dns_gate_in_bootstrap_lets_the_comparator_decide_and_a_one_sided_anchor_stops` |
| an issuer the chain contradicts; an opening that does not hold; below the fence | `l2::tests::an_issuer_the_chain_contradicts_and_an_opening_that_does_not_hold_are_refused` |
| finality seal, D2 from verified leaves, no walk → STOP | `l2::tests::a_sealed_split_and_an_unbounded_frontier_provenance_veto_stop` |
| rule E in force → a conflict STOPs, a single chain does not | `l2::tests::a_comparator_whose_inputs_the_leaf_does_not_carry_stops_a_conflict` |
| ADR-0176 D3: one versioned allocation; an unread version refused; a conflict under the budget fence STOPs; a version-0 leaf past it refused | `l2::tests::a_bond_budget_allocation_is_read_by_one_versioned_slot_or_the_client_stops`, `palw_fork_choice_commitment_v1::tests::the_comparator_reads_the_weights_of_the_allocation_the_slot_names` |
| robust dominance = the intersection of in-force rules; the digest covers every field | `l2::tests::robust_dominance_is_the_intersection_of_the_in_force_rules`, `l2::tests::the_attestation_digest_covers_every_field` |
| commitment: dormant = byte-identical, envelope binds every key (the slot's included), parent-by-delta, fixed 292-byte leaf (ADR-0177: nothing else in it) | `palw_fork_choice_commitment_v1::tests::*` |
| the native-settlement roots chain in both forms (dormant, armed, straddling; flat-where-due and forged refused) | `native_settlement::tests::rfc9_l2_the_roots_chain_reads_each_header_in_its_committed_form` |
| the node commits and serves it past the fence (construction = validation), L1+L2+L3 → `VERIFIED_REMOTE`; slot `NONE`; t12's DNS gate facts non-refusing | `t12_fork_choice_commitment` (pipeline) |
| op 203 round-trips; simnet serves none | `rpc/core` model tests `test_wrpc_serializer_*palw_fork_choice*`; `rpc_tests::sanity_test` (not run here: the integration crate builds the daemon) |
| the remote miner: attested → `VERIFIED_REMOTE` with the issuer named; no attestation → `HEADER_VERIFIED` (L3 through the envelope); a lying class row → STOP | `miner::tests::an_attested_fork_choice_lifts_the_miner_to_verified_remote_and_names_the_issuer` |

## 11. Residuals (stated)

* Trust: `VERIFIED_REMOTE` rests on the attestation issuers having re-executed the fold. Independent verification is FULL_NODE.
* Eclipse: a tip that no configured peer shows and no issuer attested is invisible; `min_peers` and issuer diversity reduce it.
* Node-local inputs: where a node's answer could depend on anything the client cannot verify (§1), the client STOPs rather than guess.
* testnet-12 today: the overlay is in Bootstrap, so the DNS gate would not decide a conflict; but the fence is dormant everywhere, so no
  header commits a leaf and L2 stays `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` past the checkpoint (C1r2's checkpoint-is-the-decision-point
  rule remains).
* Rule E: where it is in force (every network that retires the DNS gate), conflicts STOP until FINX's leaf v2 (§5.4); FINX's leaf v2
  must carry the §2.5 slot at its v1 offset when it is merged.
* Bond budget: where `palw_bond_budget_v1` is in force, conflicts STOP and version-0 leaves are refused until lane BUDGET defines the
  slot's version 1 (§2.5). The fence is not in this tree, so the hook is `None` everywhere.
* A malicious peer can force a STOP (a view through an invalid or merged header next to an attested block, an unweighable tip): STOP is
  the safe answer and costs liveness only; the client never chooses on such a view.
