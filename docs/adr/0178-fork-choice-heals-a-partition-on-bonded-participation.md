# ADR-0178 — The fork choice heals a partition on bonded participation: one search over every tip, economic keys counted over each branch's exclusive past, and participation first past `W_p`

> **INTERNAL (user decision 2026-10-08).** Finding V6 is kept out of every public text — docs, commit messages, explorer text, issues.
> This draft and its record live on the unpushed branch `fin/palw-finality-consistency`. V6 is named here only as "the stale-incumbent
> comparison"; its mechanics are in the internal record, §3.

**Numbering (2026-10-10):** drafted as ADR-0175; renumbered ADR-0178 because the user's ADR-0175 is "registered models are permanently
immutable". Code, tests and the record say ADR-0178.

**Status:** ACCEPTED — the user adopted rule E (2026-10-09). Implemented behind the dormant fence `palw_fork_choice_rule_e_v1`
(lane FINX successor, 2026-10-09; §9), refused when armed in this binary; it ships armed only in the single full-activation release,
which assigns its height. User decisions already taken (2026-10-08): no public advisory; no early dedicated fence; **the fix ships in
the single full-activation release**; `palw_dns_retirement_v1` may be armed only with rule E at or below it. Record:
[`docs/design/palw/finality-palw-consistency.md`](../design/palw/finality-palw-consistency.md).

**Builds on / amends (if accepted):** ADR-0042 Decision 9 (one comparator), the strict-win reorg rule (`palw_reorg_strict_economic_win`,
lane rcore/f1-strictwin-tie), the pruning-proof strict-economic commit (rcore/hf-pptake2), ADR-0065 D2 (frontier provenance — its "bonds
registered after the fork" set difference; note testnet-12 ships `palw_frontier_provenance: None`, so rule E carries its own copy),
ADR-0160 F-W (licence 1000‰, `Created` 0), ADR-0058 (merged work counts, blue or red).

## 0. The sentence this ADR is

**A node's sink is the best tip in one order that every honest node computes alike from the same blocks — bonded participation in each
branch's exclusive past first, then the economic keys counted over that exclusive past, then the incumbent — so an honest partition that
heals before the seal converges on the side more bonds worked on, and no branch wins by blue work, by arrival order or by being one
block fresher.**

## 1. Context

Devnet r1 (H1, 2026-10-08): a 41-minute partition healed on the wire and stayed split for good. FINX's question: are the GHOSTDAG
finality guard and the PALW fork choice consistent? They are not (record §0, §3; all MEASURED through the pipeline on testnet-12's
rules unless marked):

* **V1** the sink is GHOSTDAG's first acceptable candidate, not the comparator's maximum; a lighter branch is never weighed — not even
  UTXO-validated — though the node holds it and it is the comparator's maximum (`finx_p0_b`);
* **V2** a deep all-economic tie keeps the incumbent on both sides: an honest partition three DAA long with nothing economic anywhere
  never heals, symmetric or two producers against one (`finx_p0_a`);
* **V3** the finality point advances on the node's own heartbeats (600 blue: 154 slots at 3.9 blue a slot, 300 at 2.0 — 5.2–10.2 h)
  while the gate refuses; past the fork the gate is never asked again, even once the competitor strictly wins (`finx_p0_d`);
* **V4** IBD and relay decide the same pair oppositely: an old heavy-side datadir COMMITS the light chain its relay never weighs; an old
  light-side datadir refuses the heavy chain (`finx_p0_c`); **V5** arrival order decides for fresh nodes (`finx_p0_c`);
* **V6** (safety) the stale-incumbent comparison: the gate compares a candidate with the *previous* sink while the economic keys move
  with time; reversals past the shallow window without a bond (`finx_p0_e`, `finx_p0_f`; record §3). On the live chain the DNS BFT
  veto (ADR-0128, armed from DAA 0) bounds its reach to the DNS confirmation lag while the overlay is Active and confirming; RFC-0012's
  proposed retirement removes that bound — **so this fix must be active no later than the DNS retirement**;
* **V7** the relay skips the lighter side's blocks below the merge-depth root (code).

And a bound (record §2.3): no rule that reads only topology, blue work, hashes and the economic keys both refuses an attacker's private
branch and heals an economically tied honest partition — to the lighter node they are the same blocks. Healing needs an input a private
branch lacks: bonded participation is one the chain already has (an attempt is signed by its bond over a challenge that binds its
parents; it cannot be re-signed onto another branch; it is header data).

## 2. Options (MODEL, record §5; both applied challenge windows)

| | C1 | C2 | C3 | F | **E** | K |
|---|---|---|---|---|---|---|
| what | V2-max over every tip + relay below the merge root | + 4× blue-work override | portable weight out | hold finality on a refused heavier branch | C1 + C3 + participation first past `W_p` | E + 2/3-participation finality crossing |
| heals a tied partition | no | no | no | no | **yes, bonds active on both sides** | as E |
| heals r1 | yes, onto the 2-node side | no | no | no | **yes, onto the bond majority** | yes |
| heals past the seal | no | no | no | no | no | > 2/3-bond side only |
| V6 (stale incumbent) | reversed | reversed | **holds** | reversed | **holds** | holds |
| panel collusion | reversed | reversed | reversed | reversed | **holds** | holds |
| DA time bomb (court void after the seal) | holds | holds | holds | **reversed** | holds | holds |
| Sybil-fed fresh node | holds | captured | captured | captured | **holds** | holds |
| trusts blue work | no | **yes** | no | no | at an even split ≥ n/3 only | as E |

C2 is unsafe. F is a regression. C1 alone keeps V6. K changes finality's meaning; not needed now.

## 3. C3 alone against rule E — on V6, both forms, and on what else they do

V6 has two forms (record §3): a *carrier* form (a branch that carries a pending licence of a claim both branches hold, before the
incumbent does) and a *clock* form (a branch whose clock runs one slot ahead crosses the `Final` height of a claim both branches hold
first). Both need a claim **both branches hold** whose economic contribution changes with time.

| | C3 alone | rule E |
|---|---|---|
| V6, carrier form | **closed** — the claim is portable, its licence counts on neither side | **closed** — same keys, and participation outranks them past `W_p` |
| V6, clock form | **closed** — the claim's `Final` is portable | **closed** |
| V6 by merged borrowing: the attacker forks earlier and merges the public attempts made between the fork and the payment, so their claims are "above the fork" on both branches | **closed only if "portable" means "accepted in a block in both tips' past"**; reopened if it means "accepted at or below the fork's blue score" | **closed only with the same definition** for the keys AND for participation — counted over each tip's exclusive past, the merged honest attempts cancel; counted "above the fork", they inflate the attacker's participation |
| panel collusion (a quorum-holding attacker self-licensing its own branch's claims) | **reversed** — those claims are exclusive to its branch, and count | holds — participation first |
| liveness: tied partitions | split (as today) | heals where bonds were active on both sides |
| liveness: a portable licence on the heavier side (today: heals) | **split** — the licence no longer counts, the tie keeps the incumbent: a regression | heals |
| liveness: devnet r1 | split | heals onto the bond majority |
| cost | a walk of both tips' exclusive pasts per non-extension comparison (bounded by the finality depth); the search unchanged | the same walk, plus participation (attempt headers, same walk) and C1's search over every tip and relay below the merge root |
| fingerprint | one fence; no block-validity or state-root change | one fence; no block-validity or state-root change |

**Recommendation (PROPOSED): rule E**, with both definitions taken over each tip's **exclusive past** (blocks in one tip's past and not
the other's) — not "above the fork's blue score". On V6 the two rules are equal once that definition is used; C3 alone leaves panel
collusion open and makes one more class of honest partitions permanent, while rule E closes V6, closes panel collusion, and heals the
partitions P0 is about. Since the fix ships in one full-activation release either way, rule E's larger cost is paid once, behind the same
fence. Choose C3 alone only if the release must carry the smallest possible fork-choice change; then accept that partitions stay as
today or worse, and that panel collusion stays open.

## 4. Decision (ACCEPTED: rule E; as implemented)

Under one dormant fence, `Params::palw_fork_choice_rule_e_v1` (a height, read at the INCUMBENT's DAA), past which:

1. **One order.** For two tips `a`, `b` with common selected-chain ancestor `F` (chain reachability): each side's **exclusive
   claims** are the claims its own chain accepted above `F` (accepted blue score above `F`'s) whose id the other tip's state does not
   hold — so a claim both tips hold, or one accepted at or below `F`, decides nothing (this equals "carrying block outside the other
   tip's past" except where the other tip's fold refused a merged attempt this tip accepted; it is the form a header-verified
   client can check). **Participation** is the number of distinct executor bonds of the side's exclusive attempt claims that are
   registered in `F`'s registry — the common past (a losing draw and an unbonded header make no claim; a bond registered after `F`
   never counts). The **economic keys** `(frontier, safe, live)` are the fold's own per-claim expressions, priced at each tip, summed
   over the exclusive claims (F-W's per-bond cap applied over that set; the frontier by the resolved-prefix rule restricted to it;
   past `palw_bond_budget_v1`, each `(bond, window)` at most its `F_max` — §10). Every side goes through one accumulator over
   per-claim records, the one a client reading the fork-choice leaf v2 calls; a node streams the records from its state, with no
   claim-count refusal (§10.2). Order:
   participation first **once the lower tip stands at least `W_p` = 20 DAA above `F`** (symmetric in the pair; for an incumbent
   facing a branch at least as long, its own history since the fork), then the economic keys.
2. **The gate.** A non-extension candidate replaces the incumbent on a strict win; a tie goes to GHOSTDAG's order where
   participation counts and each side reaches `max(1, ⌈n/3⌉)` of `F`'s `n` bonds, else to strict-win's shallow question, else
   the incumbent. Unweighable refuses. ADR-0065 D2 still applies after an allow. The absolute orders are not read.
3. **One search.** After its first acceptable candidate (GHOSTDAG's heaviest the gate admits), the sink search goes on: up to 256
   heap entries are ranked by **header-level participation** (registered bonds' attempt headers above the fork and outside the first
   candidate's past — headers and reachability only); entries with none, slot races of the first (both within 2 DAA of the fork) and
   entries in its past are passed over without a validation; the top 8 are UTXO-validated, gated, and each admitted one replaces
   the best on a win in the order of point 1 (the gate's tie rule). Where the first stays best the answer is byte-identical to the
   status quo's; where a lighter tip wins, the virtual merges only what is lighter than it.
4. **Relay below the merge-depth root.** A non-heartbeat block below the virtual's merge-depth root is validated rather than skipped
   (never announced), within a per-peer budget (16 at once, one per 15 s); its missing ancestors arrive as orphan roots. A heartbeat
   below the root is still skipped.
5. **IBD commits by the same order.** `validate_staging_palw_order` asks `palw_rule_e_ibd_commit_v1`: the claim-set difference of
   the local sink's state and the staged pruning point's state (a claim older than the other state's retirement horizon plus a
   600-DAA merge slack is neither side's), participation over the bonds both states register (no fork block is known there) and
   counted (the headers-proof path's entry conditions put the fork far deeper than `W_p`), ties keep the incumbent. Fail closed as
   before.
6. **The ordering rule.** `validate_palw_v2` refuses `palw_dns_retirement_v1` unless rule E is armed at or below it, and refuses
   rule E armed at all in this binary — last, after every other refusal.
7. **Finality is unchanged.** `finality_depth` stays `window_challenge / 2`; no freeze.

## 5. Consequences

* **Heals** every honest partition shorter than the seal (154–300 slots, 5.2–10.2 h on testnet-12) in which bonds attempted on both
  sides — on the side more bonds attempted on, `max(0, W_p − D)` DAA after the heal (MODEL: +22 at D = 3, +5 at D = 20, +1 past `W_p`).
  r1 converges on the majority's branch, and B's old datadir commits it.
* **Holds** against every adversary modelled: tied heartbeat branches, junk headers, both V6 forms, panel collusion with ground panels,
  the DA time bomb, Sybil-fed fresh nodes. Assumption, stated: **the attacker holds fewer bonds than the honest bonds active in the
  public branch's exclusive past over `W_p`**. An attacker that ties honest participation at a third of the bonds or more is decided by
  GHOSTDAG (blue work).
* **Residuals, named.** (a) A partition in a heartbeat-only period (no bond attempts on either side) stays split — no unforgeable input
  exists then (record §2.3); the node-local partition watchdog (LIVE-R1's N2) and a resync with `--checkpoint` are the remedy; bonded slot clocks
  would remove it (out of scope). (b) A partition longer than the seal stays split, as on every Kaspa network; K is the protocol answer if
  ever wanted. (c) The assumption is about *active* bonds.
* **Cost.** No block-validity or state-root change. Per non-extension comparison: the exclusive pasts of both tips (bounded by the
  finality depth), their attempt headers, and the PALW deltas over them. The search validates lighter branches it used to skip — bounded by
  point 1's header-level pre-filter. **Not measured** (GAP): the cost on a node fed many junk branches.
* **Fingerprint.** One fence: scheduling moves the params id and the schedule id; activation moves the consensus identity id. Below it
  the build is byte-identical.

## 6. Ordering rule, and V6's live window under the DNS veto

**Hard prerequisite: the V6 fix (C3 or rule E) is active at or below the height that arms `palw_dns_retirement_v1`.** Today the DNS BFT
veto (ADR-0128, `dns_bft_gate`, armed on testnet-12 from DAA 0) is the only layer that bounds V6; RFC-0012's retirement removes it. A
schedule that arms the retirement without this fix at or below the same height must be refused — by `validate_palw_v2`, the way F-W
already refuses to arm without strict-win (`PALW_REORG_SHALLOW_TIE_NEVER_LOWERS_SINK_DAA_V1`).

**V6's live window as a function of the DNS state.** Let `L` be the victim's anchor lag: its sink's DAA minus the DAA of its last
DNS-final anchor. The veto refuses every candidate that does not hold that anchor, and releases it once the anchor is stale on the
victim's own chain, `L > TTL` with `TTL = dns_veto_ttl_daa_score = 120` DAA (`confirmed_anchor_is_stale`). A reversal needs the fork at or
above the anchor (the anchor in both branches' history) and the payment above the fork, so a payment `d` DAA deep is reachable exactly
when `2 < d ≤ R`, with

| DNS state on the victim | reach `R` | at ~120 s a DAA |
|---|---|---|
| Active, anchor confirmed, `L ≤ 3` | none — the veto covers every depth past the shallow window | — |
| Active, anchor confirmed, `3 < L ≤ 120` | `R = L − 1` | up to 4 h at the TTL edge |
| anchor stale (`L > 120`: validators stalled), overlay not Active, no confirmed anchor, the gate abstaining, or the overlay retired | `R = F`, the finality depth in DAA (600 blue at 3.9–2.0 blue a slot: 154–300) | 5.2–10.2 h |

(Within `R`, the record's §3.1 practical bounds still apply: the carrier form closes once the public chain carries licences of its own
post-payment claims, ≈ 21–25 DAA while attempts flow; the clock form stays open until those claims turn `Final`, ≥ 141 DAA.) The live
`L` is not measured here (record §9); reading it off the fleet needs the user's approval.

## 7. What this ADR does not decide

`W_p` beyond testnet-12 (20 there; it must stay between the longest interval an active honest bond goes without an attempt and the
licence delay). K. The frontier-by-DAA follow-up. Bonded slot clocks. Stake-weighted participation (bonds count one each here).

## 8. Until the release (PROPOSED, node-local, no fence)

* LIVE-R1's N2 watchdog and the explained wedge warning — a wedged node says so and stops participating.
* The operator runbook for a sealed node: resync from an empty datadir **with `--checkpoint=<daa>:<hash>` of the network's chain**, so the
  fresh IBD cannot land on whichever side serves it first (V5).
* Internal operational guidance for the stale-incumbent comparison is in the record, §3 (not for publication).

## 9. Implementation and verification (2026-10-09)

Code (all behind the fence; below it the build is byte-identical):

* `consensus/core/src/palw_fork_choice_rule_e_v1.rs` — the fence's validation and the ordering rule, the side computation over a
  caller's exclusivity predicate, the order and the decision, the IBD commit over the claim-set difference, the relay's policy and
  its per-peer budget; unit tests.
* `consensus/src/pipeline/virtual_processor/palw_rule_e.rs` — the node's inputs (states, reachability exclusivity, the fork span,
  the slot-race test, header-level participation), the gate and the search's continuation. `dns_reorg_outcome` and
  `sink_search_algorithm` call them past the fence.
* `protocol/flows`: the IBD commit and the relay below the merge-depth root; `ConsensusApi::get_palw_rule_e_weighing_v1`.
* `consensus/core/src/palw_fork_choice_rule_e_leaf_v2.rs` — the fork-choice leaf v2 (agreed with lane L2FC2; the envelope and the
  header sites are theirs): window and registry trees, openings, provers and verifiers, and the client's pair. Note:
  `docs/design/palw/rule-e-leaf-v2-note.md`.
* Tests armed through the real pipeline: `hb_fork_choice_probe::finality_consistency::rule_e` (V1–V6 flipped, V7 by the relay
  policy's unit test, the merge-past attacker, a Sybil peer flood, Sybil bonds registered after the fork, the bounded search cost,
  three combined rejoin cases); the seven status-quo tests are unchanged and still pass unarmed. Results: the record, §10.

Residuals, named (in addition to §5's):

* **Merged work counts for neither side.** The GHOSTDAG-heavier side merges the lighter side's blocks while they are within its
  merge depth (≈ 16 slots of divergence on testnet-12's shapes), so those attempts become common and the merging side wins a short
  partition on its own exclusive attempts — the right chain (it holds both sides' work), but not "the bond majority" the model
  (which does not merge) reports for short partitions.
* **The continuation's bounds.** A lighter branch with no registered bond's attempt above the fork is never UTXO-validated, so a
  lighter branch whose only advantage would be exclusive free-prompt `Final`s is not weighed by the search (the gate still weighs
  it when it is the heavier). A flood can take the eight validations only with header-level participation at least the honest
  branch's — an attacker's own registered bonds (losing draws included, at the price of a signature each).
* **IBD ties keep the incumbent**, even at an even split (the relay decides those once the blocks arrive), and a network with no
  claims inside the retirement horizon cannot be told apart by the claim-set difference.

## 10. ADR-0176 and ADR-0177 applied to rule E (2026-10-10)

**ADR-0176 D3 / RFC-0014 §16.10 — Final weight is bounded per bond, and fork choice reads the same versioned allocation.**

* **Participation** counts a bond once, whatever number of claims it holds in an exclusive past, so speed buys none; a bond's
  attempt counts only once the fold accepted it, which past `palw_bond_budget_v1` means its claim budget `Q_max` was reserved.
  The header-level pre-filter of the search also counts bonds, not headers.
* **The economic keys** read each claim through `PalwRuleEBondBudgetV1` (the interface; lane BUDGET implements it, FINX does not).
  Where an allocation governs a claim, the record carries the allocated weights (Final: consumed at `Final`; live: the
  reservation's provisional weight) and the terms `(version, window, F_max)`; the side credits each `(bond, version, window)` at
  most `F_max` on the safe key and on the live total, whatever the engine reports. Where none governs it (below the fence; a claim
  accepted under an older ruleset, which completes under it — ADR-0176 §4) the record is priced as before. The selector
  `palw_rule_e_bond_budget_v1` is the one place every path (node, IBD, leaf v2) learns the allocation; today it returns "none".
* **Measured** (unit, `a_fast_forger_counts_once_and_its_budget_caps_its_credit`): one bond with 20,000 exclusive claims against an
  honest bond of the same budget that did its real work — participation 1 each, both credited exactly `F_max`, the pair ties; the
  same records without an allocation sum to 1,500× `F_max` (the pre-budget path ADR-0176 D3 forbids in the new ruleset).
* **Contract for BUDGET** (on the trait): `allocation` is a function of the state alone; `None` exactly where no allocation
  governs; `Some` gives the very weights the fold adds to the state's safe weight / bounded immature for that claim, so rule E's
  sum over a whole state equals the state's accumulators (BUDGET's acceptance test); it reads no availability input.

**A finding the budget work exposed (fixed here).** Rule E refused a comparison when a side's exclusive past held more than 16,384
claims. Under ADR-0160 S that is reachable by claim volume alone (at ρ = 1,000 a 13,000 MSK bond holds 2,000 outstanding claims and
issues 100 a DAA; the honest network's own volume at ρ ≥ 100 passes it inside the finality depth), and a refusal keeps the
incumbent — a veto bought with claim volume. A node now streams each side from its state (memory: bonds, budget groups, distinct
blue scores) and never refuses on volume; the bound stays only where records are materialized: a leaf v2 window (not built past it)
and a client's opening (the client STOPs). L2FC must decide the window's form at high ρ (the leaf-v2 note).

**ADR-0177 — nothing in fork choice depends on model availability.** Rule E's inputs are the fold's accepted claims (phase, bond,
attempt flag, weights), the bond registry, blue/DAA scores, headers and reachability. None is a model's distribution, a peer count,
an acquisition result or a lease. Two indirect readers, for the record: (a) a claim's phase is the fold's — an availability-based
void, if any dormant code still had one, would reach rule E through the phase; ADR-0177 removes such voids at the fold (lane DA16b),
and rule E adds none; (b) a claim's price reads class shares only under `palw_uncertified_weightless` (genesis-only: the devnet
and rc presets arm it at genesis, testnet-12 does not), and class shares derive from admitted claims (ADR-0132); a seat's readiness
(ADR-0135) proves the seat's own copy of the model it computes with — the producer's possession, not distribution to anyone else.
Whether ADR-0177 reaches seat readiness is a question for the Lead, not a fork-choice input. The participation key counts a voided attempt (its signature still lies above the fork), so no void — of any cause —
moves participation.

## 11. What blocks arming (2026-10-10)

* **Code:** BUDGET's engine behind the interface (`palw_bond_budget_v1`); L2FC's envelope and the leaf's form at high ρ; the relay
  below the merge-depth root has a unit test only — a real multi-node run (record §6) is the E2E.
* **Policy:** `W_p` beyond testnet-12; whether participation stays one-per-bond or becomes budget-weighted (a capital split into
  minimum bonds before the fork counts once per bond — the per-bond caps of ADR-0176 do not reach a count); the heights.
* **External:** review of the mechanism (internal) and the multi-node runs.
