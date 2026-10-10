# Rule E and the L2 fork-choice leaf — what leaf v2 must carry (FINX → L2FC, 2026-10-09; revised 2026-10-10)

> **INTERNAL — not for publication.** Rule E (ADR-0178) is the fix for an internal fork-choice finding; this note lives on the
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
| economic keys of `a` | `(frontier, safe, live)` over the exclusive claims, each claim priced at `a`'s state by the fold's own expressions (or, past `palw_bond_budget_v1`, by BUDGET's allocation), F-W's per-bond cap over the exclusive set, and each budgeted `(bond, version, window)` at most its `F_max` on both keys (ADR-0176 D3) | state (priced records) |
| participation counts? | `min(daa(a), daa(b)) − daa(F) ≥ W_p` = 20 | headers |
| even split | `max(1, ⌈bonds_len(F)/3⌉)` | `F`'s leaf (v1 field) |
| ties | GHOSTDAG order at an even split; else strict-win's shallow question; else the incumbent | headers + which tip is the incumbent (node-local: both ways) |

Every side goes through one accumulator over `PalwRuleEClaimRecordV1`s — the records the leaf commits. A client calls
`palw_rule_e_side_from_records_v1` on its verified records; a node streams the same records from its state
(`PalwChainStateV2::palw_rule_e_side_v1`, pinned equal by `a_record_reads_the_allocation_and_the_streamed_side_is_the_records_side`) —
so node and client cannot price or sum a claim differently. (This definition replaced "carrying block outside the other
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
| window | one `PalwRuleEClaimRecordV1` per claim accepted above `blue_score − finality_depth`, sorted by `(accepted_blue_score, claim_id)`: `{claim_id, accepted_blue_score, bond, attempt, status, safe_weight, live_weight, capped, bond_collateral, budget}` priced at this state, borsh (`budget: Option<{version: u16, window: u64, final_weight_ceiling: u128}>`, `None` until BUDGET's engine governs the claim) | the suffix above `bs(F)` — complete: it runs to `window_len`; its first record is the last one at or below `bs(F)` when the window has one (at index 0 or later; required whenever the range does not start at 0), and no other opened record lies at or below `bs(F)`; `F` below the floor is refused (sealed: STOP) |
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

Cost (DERIVED): ~150 bytes a record (+27 with budget terms); a registry proof is `O(log n)` hashes per participating bond. A window
holds every claim of the last finality depth: tens per 100 DAA at today's rate, but under ADR-0160 S a 13,000 MSK bond issues
`u·ρ/20` claims a DAA (1 at ρ = 10, 10 at ρ = 100, 100 at ρ = 1,000), so nine such bonds at ρ = 100 put ~27,000 records in a
300-DAA window. **The node no longer refuses on volume** (a refusal keeps the incumbent, so volume would buy a veto); the bound
`PALW_RULE_E_MAX_EXCLUSIVE_CLAIMS_V1` = 16,384 now applies only where records are materialized — `PalwForkChoiceLeafV2::of` is
`None` past it, and a client STOPs on an opening past it.

## 5. What stays node-local (the client STOPs on it, as in L2FC §5.3)

* the node's sink-search continuation bounds (`PALW_RULE_E_MAX_SCORED_V1`, `PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1`): a node under a
  flood may not weigh a lighter tip a client can; the client's answer is the rule's, not a particular node's;
* the relay's per-peer budget below the merge-depth root;
* the DNS BFT veto while the overlay is not retired (rule E must be armed at or below the retirement — validation refuses otherwise).

## 5a. For L2FC to decide (2026-10-10)

1. **The window at high ρ.** `of` returning `None` past 16,384 records cannot stand inside an envelope every block must carry.
   PROPOSED: keep per-claim records (the claim-id difference needs them), build the window tree incrementally — a leaf changes only
   at a claim's acceptance, its phase transitions and its fall below the floor, so each block costs `O(changes · log n)` — and size
   the bound from ADR-0160 S's ceilings over a finality depth instead of a constant. Until then a leaf v2 cannot carry a busy
   window (nine 13,000 MSK bonds: fine at ρ = 25, ~6,750 records; past the bound at ρ = 100).
2. **The allocation.** Past `palw_bond_budget_v1` a record carries BUDGET's weights and terms (`PalwRuleEBondBudgetV1`, selected by
   `palw_rule_e_bond_budget_v1` — the one place, read by the node, the IBD path and `palw_rule_e_window_v1` alike). Nothing for the
   client to do: it reads the terms from the verified records.
3. **Fixed 2026-10-10:** `palw_rule_e_verify_window_above_v1` refused a valid opening whose boundary record sat at index 0 (one
   record at or below `bs(F)`); a client built against the 10-09 code must take the fix.

## 6. Allocations and integration items

None. The record encoding gained `budget` on 2026-10-10 (nothing was armed; leaf roots computed before then differ). Leaf v2 rides
`palw_fork_choice_rule_e_v1` per L2FC's contract, under L2FC's envelope key (the version prefix and the length
separate v1 and v2). Integration item (whoever merges second): rule E's validator also refuses unless
`palw_fork_choice_commitment_v1` is armed at or below it.
