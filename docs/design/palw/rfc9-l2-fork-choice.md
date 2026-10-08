# RFC-0009 L2 — verifying the PALW fork choice without a full node (lane L2FC, 2026-10-09)

Lane L2FC · branch `rfc9/l2-fork-choice` (from the integration head `b676927de`) · closes the L2 DESIGN_GAP left by C1r2
([`rfc-0009-remote-record.md`](rfc-0009-remote-record.md), round 2) · RFC: [0009 §「検証の 3 層と fork-choice gate」](../../rfc/0009-palw-remote-miner.md).

Words as in the RFC-0012 record: **MEASURED** (printed by a test on this branch, named), **DERIVED** (arithmetic on shipped constants or
on a MEASURED number), **ESTIMATE** (a stated assumption), **PROPOSED**, **GAP**. Status words as in the integration matrix.
Nothing here ran against a live node or network.

## 0. The answer

A remote client can verify the PALW fork choice only if three separate things hold, and each needs its own mechanism:

1. **Values bound to an authenticated root.** The comparator's inputs (safe frontier, safe weight, bounded immature → live total)
   must be read out of a root the chain commits. Today they cannot be: the ADR-0043 state root is a flat preimage in which
   `safe_weight` sits mid-preimage behind Some-only blocks, so no opening of it is addressable. **PROPOSED: a versioned commitment —
   the header commits `H(fork-choice leaf ‖ ADR-0043 root)` past a dormant fence — whose opening is ~270 B and O(1).**
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
the DNS BFT veto runs in front of the PALW comparator (armed from DAA 0, not retired), so a **conflict between two branches is a STOP**
there whatever their keys; conflict resolution by the comparator applies where the overlay is retired (§5.3).

## 1. What a full node actually decides

The authority is `palw_fork_choice::compare_palw_candidates_v1` (frontier, safe weight, live total, hash), but no node calls it alone.
A sink move that is not an extension of the node's previous sink passes `dns_reorg_outcome`, in this order:

| Step | Rule | Inputs | Light-client view |
|---|---|---|---|
| 1 | DNS BFT gate (`dns_bft_gate_refusal`) unless `palw_dns_retirement` is active at the incumbent's DAA | the node's overlay state | not verifiable here → any conflict STOPs while it can run |
| 2 | `decide_deep_reorg_v2` — or, past `palw_reorg_strict_economic_win` (read at the incumbent's DAA), `palw_reorg_strict_economic_win_v1` with the shallow-GHOSTDAG tie callback | both candidates' `PalwCandidateOrderV1`; a tie: depth ≤ 2 DAA, DAA not lowered, GHOSTDAG order | orders: from verified openings; the tie callback: evaluated both ways |
| 3 | ADR-0065 D2 frontier provenance (`palw_frontier_provenance`, `None` on t12) | bonds minted on the challenger's branch after the fork; their panel seats | bounded by `bonds_len` in the leaf (§2.1): if the bound cannot reach a quorum, D2 cannot veto |
| — | finality: a candidate below the node's finality point is never weighed | finality depth (600 blue on t12) | a conflict whose fork is ≥ `finality_depth` below either tip is a sealed split → STOP |
| — | IBD commit (`decide_ibd_commit_v2` / `palw_ibd_commit_strict_economic_v1`) | orders | evaluated with the same inputs |

An extension of the sink is GHOSTDAG's (blue work), and the deep-reorg gate is asked only against the node's *previous* sink, so a
node's canonical chain is path-dependent: two honest nodes can hold two sinks (the open finality/fork-choice analysis, ADR-0175 draft,
may change the comparator itself — §5.4). A light client therefore cannot claim "the canonical chain"; it can claim exactly this:

> **L2 holds for tip T** iff T's chain is the only chain any configured peer shows past the checkpoint, or T *robustly dominates*
> every other shown candidate — every in-force decision function, evaluated on attested inputs in both directions with every
> unverifiable input taken both ways, keeps T and refuses the other — and nothing node-local (DNS veto, D2 beyond its bound, the
> finality seal) can intervene.

## 2. The commitment (consensus, dormant fence)

### 2.1 The leaf — what the comparator reads, and what binds it

`PalwForkChoiceLeafV1` (new module `consensus/core/src/palw_fork_choice_commitment_v1.rs`), fixed-size borsh, 194 bytes:

| Field | Type | Source in `PalwChainStateV2` | Why |
|---|---|---|---|
| `leaf_version` | u16 = 1 | — | the opening format the client parses; a comparator that reads more bumps it under its own fence |
| `block`, `daa_score`, `blue_score` | Hash64, u64, u64 | `last_point` | whose post-state this is: the client checks `block` against the candidate it weighs |
| `safe_frontier_blue_score`, `safe_frontier` | u64, Hash64 | `safe_frontier()` | comparator key 1 (and the frontier block, for reachability checks) |
| `safe_weight` | u128 | `safe_weight()` | key 2 |
| `bounded_immature` | u128 | `bounded_immature()` | key 3 (`live_total` is constructed by `PalwCandidateOrderV1::new`, never carried) |
| `bonds_len` | u64 | `bonds_iter().count()` | D2's bound: the registry is append-only (ADR-0065 D5), so bonds minted after a fork ≤ `bonds_len(tip) − bonds_len(checkpoint)` |

Every leaf field is already part of the ADR-0043 root except `bonds_len`, which is derived from it; the leaf adds no state.

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
* **Fence** (PROPOSED name, Lead to allocate): `Params::palw_fork_choice_commitment_v1: Option<ForkActivation>`, `None` on every preset,
  refused when armed (`validate_palw_fork_choice_commitment_v1`) until the release that arms it; hashed into the params id only when
  `Some`, so no live id moves.
* **Sites** that compare or produce a header root (all in `kaspa-consensus`): chain validation (`header.palw_state_root != parent_root`),
  the template stamp, `palw_state_proof_v1` (op 202), `import_pruning_point_palw_state` (the witness child's root), and
  `native_settlement`'s roots chain (it holds only delta roots; it rebuilds each leaf backwards from the sink's by reverting the
  `Weights` / `Frontier` / `LastPoint` / `Bond` entries of each delta — `PalwForkChoiceLeafV1::parent_by_delta`).

Why not a block inside the ADR-0043 preimage: it needs a new state field (a carriage tail and a delta entry to journal the switch),
the opening would be the whole preimage (≈ 4–8 KB), and its offset would depend on every future Some-only block staying in front of
it. Why not a tree over every collection now: L2 needs four scalars; a tree-shaped L3 is a separate, larger change (the leaf version
leaves room: a v2 leaf can add a collection-tree root).

### 2.3 The opening

`PalwForkChoiceOpeningV1 { leaf: PalwForkChoiceLeafV1, inner_root: Hash64 }` (258 B on the wire). The client:

1. obtains the committed root R for block B's post-state from **an attestation of (B, R)** (§3) and, when B has a chain child C on a
   verified view, also from C's header (`C.palw_state_root`, C hashing to its id, C naming B as a parent) — the two must agree;
2. checks `B.daa_score ≥ F` per its own ruleset (else the state predates the commitment: no opening exists, L2 cannot hold);
3. checks `envelope(leaf, inner_root) == R`, `leaf.block == B`, `leaf.daa_score == B.daa_score`, `leaf.blue_score == B.blue_score`;
4. builds the order with `PalwCandidateOrderV1::new(leaf.safe_frontier_blue_score, leaf.safe_weight, leaf.bounded_immature, B)`.

L3 composes: past the fence, an op-202 proof is checked against `inner_root` (the opened ADR-0043 root), not against the header root.

**The tip's own post-state is committed by no header until a child is mined** (the header commits its selected parent's state —
non-circular by design). So a candidate tip is weighed through its attestation; a child header, when present, is the cross-check.

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
| **D. Attested root** (trusted checkpoint at a bounded lag) | per candidate: one attestation (≈ 4.9 KB, ML-DSA-87) + one opening (258 B) | §6 | sound **relative to the issuers' trust**: the issuer re-executed (it runs a full node); the client verifies the binding, the chain linkage, freshness and the comparator | **RECOMMENDED for `VERIFIED_REMOTE`** |
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
* **Binding:** the opening must hash to `committed_root` (§2.3); a chain child of `block` on any verified view must commit the same
  root, or the issuer is contradicted by the chain and the attestation is refused (named, so the user can drop the issuer).
* **Placement:** `block` must be on some verified view. An attestation of a block outside every view means some peer is hiding a tip
  (or the issuer is on another branch): STOP.
* **Restart:** nothing is carried over; the verdict is a pure function of the checkpoint, the attestations and the bytes served now.

## 5. Applying the comparator

### 5.1 Candidates

From ≥ `min_peers` (default 2) independent peers, each view L1-verified from the same checkpoint (existing `verify_header_chain_v1`).
Views are grouped by containment into maximal chains; each maximal chain's tip is a candidate. One candidate → §4's lag rule.
Several → every candidate must be weighable (attested at its tip, opening verified); an unweighable candidate is **not** treated as a
loser — the node that refuses an unweighable challenger is weighing with its own state, which the client does not have — so it STOPs.

### 5.2 Robust dominance (the SAME functions, never a new order)

For candidates c and o with verified orders, `c` robustly dominates `o` iff, for every variant the client's ruleset may have in force
at either tip's DAA:

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
| DNS BFT gate | `palw_dns_retirement` active at both tips' DAA (or no `dns_params`) | STOP: "a DNS-final veto decides" (testnet-12 today) |
| D2 frontier provenance | fence inactive at both tips' DAA, or for the chosen c: `!palw_minted_seats_can_reach_quorum_v1(bonds_len(c) − bonds_len(checkpoint), panel)` | STOP |
| finality seal | the fork point is < `finality_depth` blue below each tip | STOP: sealed split |

### 5.4 A comparator that changes (ADR-0175 draft, rule E)

The draft's keys are fork-relative (work accepted above the fork point) and add bonded participation above the fork. The v1 leaf
carries absolute keys, so it cannot feed E. The contract: **the leaf is versioned by what the in-force comparator reads; a comparator
fence that needs more ships its leaf version under the same fence.** For E that is (PROPOSED, not built): a Merkle-sum accumulator of
`Final` pwu keyed by acceptance position (fork-relative safe weight = a range sum, O(log n) opening), the frontier as the maximum key
of the accumulator above the fork, and participation from header data — every attempt header above the fork on each branch (signed
by its bond; L1 already verifies the signature) plus the bond registry at the fork (an L3 opening). The client's robust evaluation
then calls E's decision function with those inputs; nothing else in §4–§5 changes.

## 6. Cost per verified view (testnet-12)

| Item | Size / time | Basis |
|---|---|---|
| chain headers, per DAA | ≈ 8 KB/DAA (≈ 5.6 MB/day) | DERIVED from C1r2's ~5–6 MB/day at ~29 DAA/h; an attempt header ≈ 8 KB (ML-DSA-87 key 2,592 B + signature 4,627 B), a heartbeat ≈ 0.7 KB |
| opening | 258 B | DERIVED (194-byte leaf + 64-byte inner root) |
| attestation | ≈ 4.9 KB | DERIVED (ML-DSA-87 signature 4,627 B + fields) |
| one refresh, 2 peers, 1 candidate | ≈ 21 KB per DAA ≈ 15 MB/day | DERIVED: 2 × 8 KB headers + opening + attestation |
| each extra candidate (conflict) | + ≈ 5.2 KB | DERIVED |
| cold start from a checkpoint ≤ 1,000 headers back | ≈ 2.4 MB | DERIVED (≈ 300 DAA × 8 KB) |
| CPU per refresh | see the MEASURED table below | `l2fc_cost_per_view` (ignored test, `misaka-palw-remote`) |
| option A, per chain block | one `state_root()` over the state | MEASURED below on a synthetic state of t12's carriage size |

MEASURED (filled by `cargo test -p misaka-palw-remote --lib l2fc_cost -- --ignored --nocapture`): see §9.

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

* consensus-core: `palw_fork_choice_commitment_v1` (leaf, envelope, opening verify, `parent_by_delta`), the fence field + validator.
* consensus: the five sites through `palw_committed_state_root_v1`; `palw_fork_choice_opening_v1(block)` on the consensus API.
* RPC: `getPalwForkChoiceOpening` — **op number requested from the Lead (proposed 203, from C1's reserved 203–209)**.
* client (`misaka-palw-remote::l2`): attestation, opening verification, candidate grouping, robust dominance, the L2 verdict, labels.
* tests: §10.

**Allocations requested:** the fence name (`palw_fork_choice_commitment_v1`) and the op number. No object tag, delta number, carriage
tail or preimage block is needed.

## 9. Measurements

(Filled at the end of the lane.)

## 10. Tests (mandatory list → test)

| Required | Test |
|---|---|
| a raw-blue-work-heavier but PALW-losing fork is never chosen | `l2::tests::a_heavier_blue_work_fork_that_loses_the_palw_order_is_never_chosen` |
| a correct Merkle proof of a non-canonical state is refused | `l2::tests::a_correct_proof_of_a_non_canonical_state_is_refused` |
| a hidden competing tip is detected through a second peer | `l2::tests::a_hidden_tip_is_found_through_a_second_peer_or_an_attestation` |
| a stale checkpoint is refused | `l2::tests::a_stale_attestation_or_checkpoint_is_refused_by_name` |
| restart with a different peer reaches the same verdict | `l2::tests::a_restarted_client_with_another_peer_reaches_the_same_verdict` |
| commitment: dormant = byte-identical, envelope binds every key, parent-by-delta | `palw_fork_choice_commitment_v1::tests::*` |
| the node commits and serves it past the fence (construction = validation) | `t12_fork_choice_commitment` (pipeline) |

## 11. Residuals (stated)

* Trust: `VERIFIED_REMOTE` rests on the attestation issuers having re-executed the fold. Independent verification is FULL_NODE.
* Eclipse: a tip that no configured peer shows and no issuer attested is invisible; `min_peers` and issuer diversity reduce it.
* Path dependence: nodes may hold different sinks for the same DAG (§1); the client then STOPs rather than guess.
* testnet-12 today: the DNS veto makes every conflict a STOP; the fence is dormant everywhere, so until it is armed no header commits a
  leaf and L2 stays `HEADER_VERIFIED_FORK_CHOICE_UNVERIFIED` past the checkpoint (C1r2's checkpoint-is-the-decision-point rule remains).
