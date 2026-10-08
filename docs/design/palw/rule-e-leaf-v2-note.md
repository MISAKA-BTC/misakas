# Rule E and the L2 fork-choice leaf — what leaf v2 must carry (FINX → L2FC, 2026-10-09)

> **INTERNAL — not for publication.** Rule E (ADR-0175) is the fix for an internal fork-choice finding; this note lives on the
> unpushed branch `fin/palw-finality-consistency` and goes to lane L2FC by hand. It describes what rule E reads, not why it was
> needed.

Lane L2FC's design (`docs/design/palw/rfc9-l2-fork-choice.md` on `rfc9/l2-fork-choice`, §5.4) states the contract: *the leaf is
versioned by what the in-force comparator reads; a comparator fence that needs more ships its leaf version under the same fence.*
This note is the input for that leaf version. Words as in the RFC-0012 record (MEASURED, DERIVED, PROPOSED, GAP).

## 1. What rule E reads (the node's comparator, as implemented)

Code: `consensus/core/src/palw_fork_choice_rule_e_v1.rs` (pure), `consensus/src/pipeline/virtual_processor/palw_rule_e.rs` (the
node's inputs) and `consensus/core/src/palw_fork_choice_rule_e_leaf_v2.rs` (the leaf). For two tips `a`, `b` with common
selected-chain ancestor `F`:

| input | definition | where it lives |
|---|---|---|
| exclusive claims of `a` | claims `a`'s chain accepted above `F` (`accepted_blue_score > bs(F)`) whose id `b`'s state does not hold | state (claims) + `bs(F)` (headers) |
| participation of `a` | distinct executor bonds of `a`'s exclusive **attempt** claims (mergeset attempts included — ADR-0058) registered in `F`'s registry | state (claim records; `F`'s registry) |
| economic keys of `a` | `(frontier, safe, live)` over the exclusive claims, each claim priced at `a`'s state by the fold's own expressions, F-W's per-bond cap over the exclusive set | state (priced records) |
| participation counts? | `min(daa(a), daa(b)) − daa(F) ≥ W_p` = 20 | headers |
| even split | `max(1, ⌈bonds_len(F)/3⌉)` | `F`'s leaf (v1 field) |
| ties | GHOSTDAG order at an even split; else strict-win's shallow question; else the incumbent | headers + which tip is the incumbent (node-local: both ways) |

The node computes every side through `palw_rule_e_side_from_records_v1` over `PalwRuleEClaimRecordV1`s — the same records the
leaf commits — so node and client cannot price a claim differently. (This definition replaced "carrying block outside the other
tip's past" on 2026-10-09 so that a header-verified client can evaluate it; they differ only where the other tip's fold refused a
merged attempt this tip accepted, which then counts for this tip.)

## 2. Why the v1 leaf cannot feed it

The v1 leaf carries the state's **absolute** keys. Rule E never reads them: every key is a sum over a pair-dependent subset of
claims, and a claim both tips hold must be subtractable from both sides.

## 3. Leaf v2 (IMPLEMENTED, agreed with L2FC2 2026-10-09)

`PalwForkChoiceLeafV2` — fixed-size borsh, 334 bytes: `leaf_version = 2`, the v1 fields in v1 order and offsets (`block`,
`daa_score`, `blue_score`, `safe_frontier_blue_score`, `safe_frontier`, `safe_weight`, `bounded_immature`, `bonds_len`), then
`window_floor_blue_score`, `window_len`, `window_root`, `registry_root`. Derived from the state (no new state field; `state_root()`
unchanged). `PalwForkChoiceLeafV2::of(state, params)` is `Some` iff rule E is in force at the state's own point; which leaf a state
commits under the envelope (and the commitment fence) is L2FC's.

| tree | leaves | opens |
|---|---|---|
| window | one `PalwRuleEClaimRecordV1` per claim accepted above `blue_score − finality_depth`, sorted by `(accepted_blue_score, claim_id)`: `{claim_id, accepted_blue_score, bond, attempt, status, safe_weight, live_weight, capped, bond_collateral}` priced at this state | the suffix above `bs(F)` — complete: it runs to `window_len` and starts at index 0 or at one boundary record at or below `bs(F)`; `F` below the floor is refused (sealed: STOP) |
| registry | the state's bond keys, sorted | membership, and absence by adjacency |

Trees: RFC 6962's shape over keyed BLAKE2b-512, separate keys per tree for leaves, inner nodes and the empty tree.

API for the client (L2FC2): `verify_window_suffix_v1`, `verify_registry_membership_v1`, `palw_rule_e_pair_from_openings_v1`,
`palw_rule_e_participation_counts_from_daas_v1`, `palw_rule_e_even_split_min_v1`, then `palw_rule_e_decide_v1` /
`palw_rule_e_order_v1` with the tie callbacks both ways. For op 203: `prove_window_suffix_v1`, `prove_registry_membership_v1`.

## 4. The client's procedure for one pair

1. L1 headers / leaf walk of both branches down to `F`; `bs(F)`, `daa(F)` and `F`'s leaf v2.
2. Each tip's window suffix above `bs(F)` (verified, complete).
3. Each attempt record's bond: membership in `F`'s registry.
4. `palw_rule_e_pair_from_openings_v1` → the node's pair (asserted equal in the pipeline tests).
5. `palw_rule_e_decide_v1` for each orientation and tie answer; STOP unless all agree.

Cost (DERIVED): ~150 bytes a record; exclusive claims per side are the attempts above the fork (tens per 100 DAA at today's rate;
the node refuses past 16,384, and so must the client); a registry proof is `O(log n)` hashes per participating bond.

## 5. What stays node-local (the client STOPs on it, as in L2FC §5.3)

* the node's sink-search continuation bounds (`PALW_RULE_E_MAX_SCORED_V1`, `PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1`): a node under a
  flood may not weigh a lighter tip a client can; the client's answer is the rule's, not a particular node's;
* the relay's per-peer budget below the merge-depth root;
* the DNS BFT veto while the overlay is not retired (rule E must be armed at or below the retirement — validation refuses otherwise).

## 6. Allocations and integration items

None. Leaf v2 rides `palw_fork_choice_rule_e_v1` per L2FC's contract, under L2FC's envelope key (the version prefix and the length
separate v1 and v2). Integration item (whoever merges second): rule E's validator also refuses unless
`palw_fork_choice_commitment_v1` is armed at or below it.
