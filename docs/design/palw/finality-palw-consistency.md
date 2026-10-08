# The finality guard and the PALW fork choice — are they consistent? (FINX, 2026-10-08)

> **INTERNAL — not for publication (user decision 2026-10-08).** V6 (§3) is a live safety finding on testnet-12; its mechanism stays
> out of every public text (docs, commit messages, explorer text, issues). This record, the tests and the model live on the unpushed
> branch `fin/palw-finality-consistency`. The fix ships in the single full-activation release (C3 or rule E — the user's choice).

Lane FINX · branch `fin/palw-finality-consistency` (from the integration head `9ea89994b`) · decision draft:
[ADR-0175](../../adr/0175-fork-choice-heals-a-partition-on-bonded-participation.md).

**Status: analysis, probes and a model. No shipping rule is changed.** Every candidate rule below lives only in the model
(`consensus/core/tests/finality_palw_consistency_model.rs`); the pipeline tests run testnet-12's rules as they are. What to change,
if anything, is the user's decision.

Words used exactly, as in the RFC-0012 record: **MEASURED** — printed by a test on this branch (named; command in §8). **DERIVED** —
arithmetic on shipped constants. **MODEL** — an output of the executable model (§5), which drives the shipped comparator functions but
abstracts the DAG. **PROPOSED** — a recommendation with its reason. **GAP** — not known or not testable here.

DAA → time: a DAA step needs one heartbeat slot, so **at least 120 s per DAA** (`target_time_per_block`); the harness's honest slot
is 122 s. Hours below are lower bounds.

## 0. The answer

**No. The GHOSTDAG finality guard and the PALW fork choice are not consistent, in either of the two senses that matter**, and that is
the root cause of devnet r1's permanent partition:

1. **They do not decide the same chain from the same information.** The sink search is GHOSTDAG's (blue work first); the PALW
   comparator is asked only about a *non-extension* candidate, only against the node's *previous* sink, and a deep all-economic tie
   keeps that incumbent. So two honest nodes that hold one DAG keep two sinks whenever the side heavier in blue work is not also
   strictly ahead in the economic keys (V1, V2). In devnet r1 the minority was economically ahead (a portable licence), the majority
   heavier: each kept its own. It needs no licence at all: **an honest partition three DAA long with nothing economic on either side
   never heals** (MEASURED, `finx_p0_a_*`; MODEL).
2. **The finality guard seals what the PALW rule would replace.** The finality point advances by the node's *own* heartbeats — 600
   blue score is 154 slots on a side whose producers race (3.90 blue a slot) and 300 on a single producer's (2.00), 5.2–10.2 h
   (MEASURED `finx_p0_facts`) — whether or not the node is refusing a competing branch at the time; once it passes the fork the PALW
   gate is never asked again (V3, MEASURED `finx_p0_d_*`). The economic keys change on their own clock (a licence lands; testnet-12
   ships the short challenge window from DAA 0, so a licence turns `Final` 121 DAA later — MEASURED; 1,201 on the unshortened window),
   so whether a partition heals depends on which clock runs out first.

The analysis also found a **safety** defect next to the liveness one (V6, MEASURED `finx_p0_e_*` and `finx_p0_f_*`; MODEL): because the
deep-reorg gate compares a candidate with the node's *previous* sink and the economic keys move with time, **a bondless private
heartbeat branch reverses a payment past the 2-DAA shallow window** by carrying a public licence (or crossing a shared claim's `Final`
height) one tick before the victim's chain does. The honest chain catching up one block later does not undo it. The same code path is
in int-12 (`rcore/int-12` @`0b1c11b87`). On the live chain the DNS BFT veto (armed from DAA 0) runs in front of it and bounds its reach
to payments newer than the last DNS-final anchor while the overlay is Active and confirming; with the overlay inactive, stalled past
its 120-DAA TTL, or retired (RFC-0012's proposal), V6 reaches to the finality depth (§3.1).

**What heals it** (MODEL, §5): no rule that uses only blue work, hashes and the economic keys can both heal an economically tied honest
partition and refuse an attacker's private branch — to the lighter node they are the same blocks (§2.3). The candidates that keep the
private-branch defence and heal partitions use an unforgeable signal; the one the chain already has is **bonded participation** —
distinct pre-fork bonds that signed attempts in each branch's exclusive past. Rule **E** (C1's search over every tip, C3's exclusive-past
keys, participation first once the fork is `W_p` deep) heals every partition shorter than the seal in which bonds were active on both
sides, on the side with more of them, and holds against every private-branch adversary modelled, including panel collusion and the
stale-incumbent attack. What no candidate heals without a new trust assumption: a partition in a heartbeat-only period (no bond signs
anything), and a partition longer than the seal (rule K, a 2/3-participation checkpoint, does; at the price of crossing finality).

## 1. The two rules, as the code runs them

**testnet-12 as shipped** (MEASURED `finx_p0_facts`): finality depth 600 blue, merge depth 30, pruning depth 74,920; windows bind 600,
receipt 600, challenge 1,200 with the short window (120) armed from DAA 0, court 3,000; anchor delay 20, quorum 3; strict-win and the
pruning-proof strict-economic commit at DAA 750, F-W at 1,700; **frontier provenance (ADR-0065 D2) `None`**; DNS overlay configured and
**the DNS BFT veto (ADR-0128) armed from DAA 0** (`dns_bft_gate`, params.rs:22443). `dns_bft_gate_refusal` runs in `dns_reorg_outcome`
before the V2 arm and refuses a candidate that abandons the last DNS-final anchor — when the rollout stage is Active, an anchor is
confirmed, it is not stale (TTL 120 DAA on the node's own chain) and the gate is not abstaining; otherwise it answers nothing and the V2
arm decides. The harness runs the overlay in Bootstrap (no validators), so every pipeline test here measures the V2 arm alone. t12's DNS
cadence: attestation epoch 2 blue, lag 2 blue, backoff 1.
int-12 (`0b1c11b87`): `sink_search_algorithm`, `palw_reorg_shallow_ghostdag_win_v1`, `palw_candidate_order_v2`,
`palw_fork_authority_v2.rs` and `palw_fork_choice.rs` byte-identical to this branch's base; `dns_reorg_outcome` differs only by RFC-0012's
dormant DNS-retirement switch.

**The finality guard** (GHOSTDAG/Kaspa). `resolve_virtual` (processor.rs:1802) computes the finality point from the *previous*
virtual (`virtual_finality_point`, :1938 → `BlockDepthManager::calc_finality_point`, block_depth.rs: the highest chain block whose blue
score is below `bs(virtual) − finality_depth`), drops body tips outside its DAG future, and the sink search refuses any candidate whose
selected chain does not hold it (`candidate_at_or_above_finality`, processor.rs:18593 — "Finality Violation Detected"). testnet-12:
`finality_depth = window_challenge / 2 = 600` blue score (params.rs:3982), `merge_depth = 30`. Further down: the relay skips a block
whose blue work is not above the virtual's merge-depth root (`protocol/flows/src/v7/blockrelay/flow.rs:294–307`), and IBD refuses a
peer whose chain does not hold the local pruning point (`ibd/flow.rs:1847`).

**The PALW fork choice.** `sink_search_algorithm` (processor.rs:18483) pops tips in GHOSTDAG's order (`SortableBlock`: blue work, then
hash; :18530) and returns the first candidate that is UTXO-valid and that `dns_reorg_outcome` (:17939) accepts. Its V2 arm (:17975)
runs only for a candidate that is **not a chain descendant of the previous sink**, and compares the two with
`palw_reorg_strict_economic_win_v1` (palw_fork_authority_v2.rs:215): a strict economic win on `(safe frontier, safe weight, live
total)` allows, a loss refuses, a tie allows only if shallow (the incumbent's chain above the fork spans ≤ 2 DAA), GHOSTDAG-heavier and
not DAA-lowering. A refused candidate's parents go back on the heap. The IBD commit (`ibd/flow.rs:2016–2030`) asks
`palw_ibd_commit_strict_economic_v1` of the two whole-consensus orders (armed on testnet-12 at DAA 750). The keys are a per-chain fold
(`palw_state_v2.rs`): `safe_frontier` is the accepted blue score of the deepest `Final` claim *below the oldest open claim*
(:29650–29690), `live = safe + bounded immature`, and past F-W a `Created` attempt weighs 0 and a licence 1000‰.

The module doc of `palw_fork_authority_v2` states the intended design: "Tip selection: the comparator's maximum, permutation-invariant
by totality", and "a challenger that crosses the finality depth must ALSO strictly win the comparator". What runs is neither: the
maximum is GHOSTDAG's, the comparator is a veto relative to the incumbent, and the two vetoes (finality, comparator) are evaluated on
different information at different times.

## 2. The property

### 2.1 Definitions

A node holds a DAG `G`, a sink `s` and a finality point `f`; its rule is `s' = Σ(G, s, f)` and `f' = Φ(s')`. Honest nodes are
connected with delay `Δ`, except for a partition of length `D`.

* **(A) Agreement — "both rules decide the same chain from the same information."** There is a selection `Sel(G)` such that every
  honest node holding `G`, whose finality point lies on `Sel(G)`'s chain, has sink `Sel(G)` one resolve after it holds `G` —
  whatever its previous sink and whatever order the blocks arrived in.
* **(N) No seal against the authority — "a node never seals by finality a chain the PALW rule would replace."** A node's finality
  point does not pass a fork while the node holds a fully validated branch from that fork which its own rule would select given the
  information honest nodes will hold within `Δ` of the heal. Equivalently: honest finality points never conflict for a partition with
  `D < T_seal`, where `T_seal` is the finality horizon in time.
* **(L) Healing.** After a partition with `D < T_seal`, every honest node converges within a bounded `Δ_heal`.
* **(S) Unforgeability.** No reorg deeper than the stated shallow window `W_s` (2 DAA) is decided by forgeable weight: blue work
  (heartbeats are permissionless; an attempt header carries 2^20 whatever its bond and lottery), hashes, arrival order, peers.

(A) ∧ (N) is "consistent". (L) is what the user lost in devnet r1; (S) is what strict-win was armed to keep.

### 2.2 What the status quo satisfies

(S) for an economically *tied* private branch (MEASURED `capacity_probe_a_shallow_tie_is_ghostdags_and_a_deep_tie_keeps_the_incumbent`;
not for a time-advantaged one — V6). Not (A), not (N), not (L) — §3.

### 2.3 The indistinguishability bound

**Claim.** No rule whose inputs are the DAG topology, blue work, hashes and the economic keys satisfies (S) and (L) for an honest
partition whose two sides are economically tied and longer than `W_s`.

*Why.* At the heal, the lighter side's node is handed a branch that is heavier in blue work and ties every economic key. An attacker
can mine exactly that branch privately — heartbeats need no bond, attempt headers cost a signature — and release it at the same moment.
(S) obliges the node to refuse the second; the rule is a function of its inputs; so it refuses the first. The heavier side's node is
handed a lighter, tied branch; a rule that took it would take an attacker's lighter tied branch too, and lightness is cheaper still. So
each node keeps its own sink: no (L). ∎

The way out is an input an honest partition side has and a private branch does not, short of an honest-majority-equivalent resource:

* **matured work** (`Final`) — unforgeable without panel collusion, but slow (123 / 1,226 DAA after acceptance) and only on the side
  whose claims matured; and forgeable on a private branch by a quorum-holding adversary that grinds its panels (MODEL, panel collusion);
* **bonded participation** — a pre-fork bond's signature on an attempt above the fork. An attempt is signed by its bond over its own
  challenge, which binds its parents, so it cannot be copied onto another branch; header-verifiable; bonds minted after the fork are
  already excluded by ADR-0065 D2's set difference. A private branch carries only the attacker's bonds; an honest side carries its own;
* **external** — operators, checkpoints (`--checkpoint`, `trusted_checkpoint`), the DNS overlay's BFT votes (armed on testnet-12 from
  DAA 0; RFC-0012 proposes retiring them). The DNS overlay is itself a stake-signed finality layer; it bounds V6 while it confirms
  (§3.1) but does not heal a partition: a side whose DNS-final anchor is on its own branch refuses the other branch outright.

## 3. The violations

| | statement | code | evidence |
|---|---|---|---|
| **V1** | **The sink is not the comparator's maximum.** The search pops GHOSTDAG's order and accepts the first extension of the previous sink without asking the comparator; a lighter branch is never weighed — even when the node holds it and it is strictly ahead on every economic key. | processor.rs:18530, :17975 (the V2 arm scoped to non-extensions) | MEASURED `finx_p0_b_*` (a `Final` on the lighter side: the heavy node keeps its own tip; the light chain is the comparator's maximum), `finx_p0_c_*` (r1); MODEL rows `PortableOnMinority` |
| **V2** | **A deep all-economic tie keeps the incumbent on both sides**, so the outcome is a function of history. An honest partition longer than 2 DAA with nothing economic on either side never heals, symmetric or not, whatever the node count. | `palw_reorg_strict_economic_win_v1` (Equal & !shallow → Refuse) | MEASURED `finx_p0_a_*` (2 slots agree; 3 and 6 split, three more exchanged rounds change nothing); MODEL `HeartbeatOnly`, `AttemptsNoLicence` |
| **V3** | **Finality seals what the gate refuses.** The finality point advances on the node's own chain while the gate is refusing a fully validated competitor; past the fork, the competitor fails `candidate_at_or_above_finality` before the gate is asked — even once it strictly wins. `T_seal` = 600 blue ÷ 3.90 or 2.00 blue a slot = 154–300 slots (5.2–10.2 h, MEASURED), shorter than any post-fork claim's `Final` on the unshortened window (≥ 1,221 DAA) by construction (`finality_depth = window_challenge / 2`); on the short window a `Final` (≥ 141 DAA after acceptance) can beat it, but a `Final` on the lighter side still does not heal (V1). | processor.rs:1806, :18593 | MEASURED `finx_p0_d_*` (the same strict win heals before the seal and is refused after it); MODEL "sealed" verdicts |
| **V4** | **The relay path and the IBD path decide the same pair differently.** An old heavy-side datadir staging the light chain COMMITS it (strict economic win), though its relay never weighs it; an old light-side datadir staging the heavy chain is refused. | `ibd/flow.rs:2016–2030` vs V1 | MEASURED `finx_p0_c_*` |
| **V5** | **Arrival order decides for a node with no history.** Two fresh nodes handed one DAG in opposite orders end on opposite sides; an IBD onto an empty datadir commits whatever the first peer serves ("an incumbent standing at genesis defends nothing"). A Sybil-eclipsed fresh node stays on an attacker's branch after it meets honest peers. | heap order + V2; `ibd/flow.rs` genesis exemption | MEASURED `finx_p0_c_*`; MODEL `SybilFreshNode` |
| **V6** | **The stale incumbent (safety).** The gate compares a candidate with the PREVIOUS sink, and the keys are time-dependent (a licence lands, a claim turns `Final` at a fixed DAA, an unlicensed claim is voided). A branch that carries first the licence of a claim both branches hold, or whose clock runs a slot ahead across such a claim's `Final` height, is "strictly ahead" of a stale incumbent on keys the incumbent's own next block would tie. No bond, no seat, no collusion. See §3.1. | `dns_reorg_outcome(candidate, prev_sink)` (processor.rs:17939, the V2 arm at :17975) | MEASURED `finx_p0_e_*` (carrier form: X reversed 32 and 60 blue score deep; the honest chain's own carrier one block later changes nothing; past the seal X stands) and `finx_p0_f_*` (clock form); MODEL `PrivateHeartbeats @4` (and `@40` on the short window): reversed under SQ, C1, C2, F; held under C3, C1+C3, E′, E, E+F, K |
| **V7** | **The information is not even shared.** The heavier side's relay skips every lighter block below its merge-depth root, so after a partition longer than ~10 DAA the majority does not hold the minority's branch at all. | blockrelay/flow.rs:294–307 | code; LIVE-R1's r1 logs |

### 3.1 V6 on live testnet-12 (internal)

What the attacker needs, both forms: a private branch forked at or before the payment X that is GHOSTDAG-heavier than the public tip
when it is released (so the victim's heap pops it before its own extension: sibling heartbeats give up to 4 blue a slot against the
honest 2–3.9, and unbonded junk attempt headers add 2^20 each at the price of a signature); X more than 2 DAA deep and above the victim's
finality point; and a claim **both branches hold** with a pending transition. *Carrier form:* the claim's quorum receipts exist (the seats
broadcast them; the quorum object is assembled by anyone) and the victim's chain has not yet carried them; the attacker funds the carrier
from any UTXO and releases right after including it. *Clock form:* the claim's `Final` height is the next DAA; the attacker's branch runs
one slot ahead (within the timestamp tolerance, `hb_probe_b_future_*`) and is released standing on that height. Neither needs a bond.

**The DNS BFT veto bounds it on the live chain.** While the overlay is Active with a confirmed, non-stale DNS-final anchor, a candidate
that abandons that anchor is refused before the V2 arm — so V6 reaches only a payment newer than the victim's last DNS-final anchor,
i.e. the DNS confirmation lag (GAP: the live lag is not measured here; it is the fleet's tip-minus-anchor DAA distance). The bound
disappears when the overlay is not Active, when validators stall past the veto's 120-DAA TTL, when the gate abstains, and when RFC-0012
retires the overlay — then the window below applies. **So C3 or rule E must be active no later than the DNS retirement.**

Window without the veto (DERIVED from the rules; ~120 s a DAA): lower bound 3 DAA (≈ 6 min) — the shallow window is 2. Upper bound the
victim's finality point (600 blue: 154–300 DAA, 5.2–10.2 h). In practice the carrier form closes once the public chain carries licences of its own claims
made after X, which the private branch cannot hold without merging X's past (anchor delay 20 + receipts ≈ 21–25 DAA, ≈ 45–50 min, while
attempts flow; open to the finality depth in quiet stretches, and receipts may land anywhere in the 600-DAA receipt window); the clock
form wins on `safe` weight and the frontier, which outrank `live`, so it stays open until the public chain's own post-X claims turn
`Final` (≈ 20 + licence + 121 ≥ 141 DAA after X, ≈ 4.7 h).

Payments at risk: any payment deeper than 2 DAA and shallower than the finality depth that no `Final` settlement anchor covers. The
published small-value rule (30 blue score above, attempts continuing) does not protect; a `Final` anchor after X does (its frontier
outranks every key the attacker can bring), as does waiting for the finality depth.

**A note on the fix's definition.** An attacker can also make a post-fork claim "shared": fork earlier and merge the public attempts made
between the fork and X (their pasts exclude X; ADR-0058 counts merged work). Their claims are then "above the fork" on both branches, and
their licences and `Final`s are timing-sensitive again. So C3 and rule E close V6 only when "portable" and "participation" are taken over
each tip's **exclusive past** (blocks in one tip's past and not the other's) — merged honest attempts then cancel — not over "accepted
above the fork's blue score". In the model's tree (no merging) the two definitions coincide; the merge variant is not exercised (§9).

A further note, not a violation measured here: `safe_frontier` is a **blue score**, which heartbeat density inflates — of two branches
that matured the same claims at the same DAA, the denser one ranks higher on the first key. Comparing frontiers by DAA (or by claim)
would remove it.

**The r1 chain of events in these terms.** V7 (A never had B's blocks), V1 (A would not have weighed them), V2/portable weight (B kept
its own on a strict "loss" the licence produced), V3 (B's heartbeats sealed it 4–6 h later on the drill's depth), V4 (B's old datadir
refuses an IBD from A; a resync from an empty datadir is the only way back — and by V5 it lands on whichever side serves it).

## 4. The pipeline tests (P0)

`consensus/src/pipeline/virtual_processor/tests/hb_fork_choice_probe/finality_consistency.rs`. Two (or more) testnet-12 nodes on the
harness (cards, windows as shipped, the fork-choice set testnet-12 runs past DAA 1,700: strict-win, lane A + F1's execution seed,
F-W); a partition is "do not mirror", a heal is "mirror". `finx_p0_d/e` lower `finality_depth` to 60 blue score on both nodes to fit
the seal into a debug build; `finx_p0_facts` converts.

| test | covers | asserts (status quo) | measured |
|---|---|---|---|
| `finx_p0_facts_depths_windows_fences_and_blue_per_slot` | the numbers | depths, windows, fence heights; blue score a slot | §4.1 |
| `finx_p0_a_a_partition_with_nothing_economic_never_heals_past_two_slots` | symmetric partition, no `Final`; the majority with more nodes | 2 slots: agree; 3 and 6: split at the heal and after three exchanged rounds — one producer a side and two against one | §4.1 |
| `finx_p0_b_a_final_heals_only_on_the_heavier_side` | a `Final` on one side | on the heavier side: the light node takes it; on the lighter side: split, the comparator's maximum on the light side, the heavy node never asks | §4.1 |
| `finx_p0_c_the_minority_holds_the_licence_and_no_way_back_heals_it` | the minority with more licences (r1); restart and IBD of a node on each side; nodes joining fresh during the split | split; restart keeps both sinks; IBD light←heavy refused, heavy←light commits; fresh nodes end on the side heard first | §4.1 |
| `finx_p0_d_finality_seals_a_chain_the_palw_rule_would_replace` | V3; a deep reorg across the seal | the heavy side's strict win heals before the seal, is refused after it | §4.1 |
| `finx_p0_e_a_private_economic_win_reverses_x_up_to_the_finality_depth_and_no_further` | an attacker's private branch released half-way, at the last slot before, and past the finality depth; V6 carrier form | reversed, reversed, refused; the honest catch-up block does not undo it | §4.1 |
| `finx_p0_f_a_branch_one_tick_ahead_crosses_a_pre_fork_final_first_and_reverses_x` | V6 clock form (real depth 600) | a heartbeat branch one slot ahead, standing on a shared claim's `Final` height, reverses X | §4.1 |

### 4.1 Measured (2026-10-08; logs `finx-cons2-p0e.log`, `finx-cons3-p0.log`, `finx-cons4-df.log` in `MISAKA-wt-b/`)

All seven pass on testnet-12's rules as armed past DAA 1,700 (they assert the status quo's behaviour).

* **facts.** Finality 600 blue, merge 30, pruning 74,920, k 1; windows bind 600, receipt 600, challenge 1,200 (applied 120 from DAA 0),
  court 3,000, anchor delay 20, quorum 3; shipped fences: strict-win 750, pruning-proof strict-economic 750, F-W 1,700, frontier
  provenance none, short challenge window 0, DNS BFT veto 0. Blue score a slot: **3.90** with two producers racing, **2.00** with one —
  the seal at depth 600 comes **154 / 300 slots (5.2 / 10.2 h)** after a fork.
* **a (V2).** Heartbeat-only, keys `(0, 0, 0)` on both sides. One producer a side: 2 slots AGREE at the heal and after three rounds;
  3 and 6 slots SPLIT at the heal and after three rounds. Two producers against one (+7 against +4 blue work at 2 slots, +23 against +12
  at 6): the same. The light node logs "refusing the heavier candidate … DominanceViolation" on every resolve.
* **b (V1).** A claim licensed on one side only, `Final` at DAA 145 there (licensed 24, window 120), healed 122 slots after the fork.
  `Final` on the heavier side: keys `(8, 6.04e9, 6.04e9)` against `(0, 0, 6.0e6)` — the light node takes the heavy tip. `Final` on the
  lighter side: the light chain is the comparator's maximum, the heavy node keeps its own, and its fold of the light tip is `None` —
  **never UTXO-validated: its search stopped at its own extension**; the light node refuses the heavy tip. Split for two more rounds.
* **c (r1, V4, V5).** Heavy tip +3,145,758 blue work, keys `(0, 0, 6.0e6)`; light tip +1,048,591, keys `(0, 0, 6.04e8)` (the portable
  licence). Split; **both nodes restarted on their databases come back on their own sinks**; **IBD light←heavy: KeepIncumbent; heavy←light:
  Commit**; two fresh nodes fed one DAG in opposite orders end on the side each heard first.
* **d (V3).** Depth 60. Three claims bound before the fork; the light side carries one licence. The heavy side carrying two more
  licences right after the heal: keys `(0, 0, 1.21e9)` against `(0, 0, 6.16e8)` — the gate Allows and the light node moves. Carried after
  the light node's finality point passed the fork (**31 slots, 62 blue score, after the fork**): the same keys, the gate would Allow — and
  the light node stays on its own chain for good.
* **e (V6, carrier form).** Depth 60; the victim seals at 62 blue score above the fork. Released at 32 and at 60: the victim's sink goes
  PRIVATE, X gone, Y present; the honest chain's own carrier one block later leaves it there. Released at 62: X stands.
* **f (V6, clock form).** Real depth 600. A shared claim `Final` at DAA 145; the private tip at DAA 145 `(8, 6.04e9, 6.04e9)` against the
  victim's sink at DAA 144 `(0, 0, 6.04e8)`; X at DAA 136. Released: the victim's sink goes PRIVATE, **X reversed 8 DAA deep**.

## 5. The model (P1)

`consensus/core/tests/finality_palw_consistency_model.rs` — an executable model, deterministic, ~1,500 lines; the full table takes about two minutes.

**What it models.** A tree of branches over one shared prefix, one model block per branch per DAA tick, cumulative blue score and blue
work (a blue beat 2^24, an attempt 2^20), each bond's last attempt on the chain, and the claim lifecycle per chain: an attempt creates a
claim (0 weight past F-W); a quorum (3 of 5) of its panel's seats whose receipts reach a branch's producers licenses it there; `Final`
one applied challenge window later (120 / 1,200); voided past the deadline; the frontier under the resolved-prefix rule. The status quo
is modelled with the **shipped functions** (`palw_reorg_strict_economic_win_v1` with the processor's shallow question,
`palw_ibd_commit_strict_economic_v1`, `PalwCandidateOrderV1`, `PALW_REORG_SHALLOW_TIE_DAA_V1`), GHOSTDAG's heap search over the tips in the
finality point's future, Kaspa's block-at-depth finality point, and the merge-depth relay skip. Not modelled, and why no verdict rests
on it: merging across branches (beyond merge depth it is refused; under it, a merged carrier at best *ties* the keys and a deep tie
keeps the incumbent — so licence cases start at 20 DAA); UTXO conflicts (a double spend is a payment one branch carries); stake
(eight equal bonds, testnet-12's cards).

**Rules.**

| rule | definition |
|---|---|
| SQ | the status quo |
| C1 | LIVE-R1: the sink is the best, in the comparator's order, of the incumbent's extension and every tip the gate admits (V2-max); the relay fetches below the merge-depth root |
| C2 | SQ, and a refused reorg is allowed when its blue work above the fork exceeds 4× the incumbent's (the DNS overlay's override) |
| C3 | SQ with "portable" weight — claims accepted at or below the fork — left out of both sides' keys |
| F | SQ, and the finality point is held at the fork of any heavier competing branch the node holds and refuses (a hold, never a retreat; bounded by the pruning depth) |
| C1+C3 | both |
| E′ | C1 + C3, with participation as the key after the economic ones |
| **E** | C1 + C3; for a fork at least `W_p` = 24 DAA deep, **participation first**: distinct pre-fork bonds that signed an attempt on the branch above the fork > frontier > safe > live > (an even split, each side ≥ ⌈n/3⌉ bonds: GHOSTDAG's order) > incumbent |
| E+F | E, and the finality point held at the fork of a branch E refuses on the incumbent's privilege alone |
| K | E, and a candidate below the finality point is admitted when > 2/3 of the bonds signed on its branch above the fork and it wins E's order (a protocol-recognised checkpointed resync) |

`W_p` must be at least the interval an active honest bond goes without attempting (so its signature is above the fork) and at most the
licence delay (so no self-licensed claim exists on a private branch before participation outranks it); testnet-12's anchor delay is 20.

**Scenarios.** Honest partitions: 8 bonds split 5:3 (the majority also makes 3 blue a tick against 2) or 4:4 (2 against 2), 6:2;
`D` ∈ {2, 3, 20 (devnet r1's 41 min), 150, 320} DAA; economics: heartbeat-only (nobody attempts), attempts without licences, a
portable licence stuck on one side (r1), every licence a reachable quorum allows. Each run carries a node per side, two nodes that join
fresh half-way through the split from a peer on either side, a restart on each side after the heal and an old-datadir IBD of a
minority node from a majority peer 50 DAA after it. Adversaries (release depth = the victim's DAA above the fork; the adversary's clock
one tick ahead, as the lead cap allows): a bondless private heartbeat branch (4 blue a slot against 2); the same plus junk attempt
headers past 4× the public blue work; panel collusion (three bonds, a quorum: self-licensing at six times an honest bond's rate on its
own branch with its panels ground onto its seats, and a pre-fork claim's receipts withheld from the public chain); a DA-withholding
time bomb (a public licence the court voids later, while the private branch carries a colluded one); a Sybil-fed fresh node.

### 5.1 Results — honest partitions (liveness)

`ok+N`: every honest node on one chain from N DAA after the heal to the end (1,500 DAA). `SPLIT/sealed`: two honest finality points on
conflicting chains — permanent. `seal1`: sealed on one side. `held`: rule F/E+F held a finality point. `crossed`: rule K crossed one.
`T_seal` = 200 DAA for a 3-blue side, 300 for a 2-blue side.

**Applied challenge window 120** (earliest `Final` 146 DAA after acceptance):

| honest partition | SQ | C1 | C2 | C3 | F | C1+C3 | E' | E | E+F | K |
|---|---|---|---|---|---|---|---|---|---|---|
| HeartbeatOnly 5:3 D=2 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1,held | ok+1 |
| HeartbeatOnly 5:3 D=3 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed |
| HeartbeatOnly 5:3 D=20 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed |
| HeartbeatOnly 5:3 D=150 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed |
| HeartbeatOnly 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| AttemptsNoLicence 5:3 D=2 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 5:3 D=3 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+22 | ok+22 | ok+22 | ok+22 |
| AttemptsNoLicence 5:3 D=20 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| AttemptsNoLicence 5:3 D=150 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| AttemptsNoLicence 4:4 D=2 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 4:4 D=3 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed,held | SPLIT/sealed | ok+22 | ok+22 | ok+22 | ok+22 |
| AttemptsNoLicence 4:4 D=20 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed,held | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| AttemptsNoLicence 4:4 D=150 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed,held | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMinority 5:3 D=20 | SPLIT/sealed | ok+1 | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMinority 5:3 D=150 | SPLIT/sealed | ok+1 | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMinority 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMinority 4:4 D=20 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMinority 4:4 D=150 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMinority 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMajority 5:3 D=20 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMajority 5:3 D=150 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMajority 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMajority 4:4 D=20 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMajority 4:4 D=150 | ok+2 | ok+1 | ok+2 | SPLIT/sealed | ok+2 | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMajority 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| FullLicensing 5:3 D=20 | ok+1 | ok+1 | ok+1 | ok+8 | ok+1 | ok+8 | ok+5 | ok+5 | ok+5 | ok+5 |
| FullLicensing 5:3 D=150 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| FullLicensing 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| FullLicensing 4:4 D=20 | ok+1 | ok+1 | ok+1 | ok+8 | ok+1 | ok+8 | ok+5 | ok+5 | ok+5 | ok+5 |
| FullLicensing 4:4 D=150 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| FullLicensing 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| AttemptsNoLicence 6:2 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | ok+1,crossed |

**Applied challenge window 1200** (earliest `Final` 1226 DAA after acceptance):

| honest partition | SQ | C1 | C2 | C3 | F | C1+C3 | E' | E | E+F | K |
|---|---|---|---|---|---|---|---|---|---|---|
| HeartbeatOnly 5:3 D=2 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1,held | ok+1 |
| HeartbeatOnly 5:3 D=3 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed |
| HeartbeatOnly 5:3 D=20 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed |
| HeartbeatOnly 5:3 D=150 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed |
| HeartbeatOnly 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| AttemptsNoLicence 5:3 D=2 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 5:3 D=3 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+22 | ok+22 | ok+22 | ok+22 |
| AttemptsNoLicence 5:3 D=20 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| AttemptsNoLicence 5:3 D=150 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| AttemptsNoLicence 4:4 D=2 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 4:4 D=3 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed,held | SPLIT/sealed | ok+22 | ok+22 | ok+22 | ok+22 |
| AttemptsNoLicence 4:4 D=20 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed,held | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| AttemptsNoLicence 4:4 D=150 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed,held | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| AttemptsNoLicence 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMinority 5:3 D=20 | SPLIT/sealed | ok+1 | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMinority 5:3 D=150 | SPLIT/sealed | ok+1 | SPLIT/sealed | SPLIT/sealed | SPLIT/seal1,held | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMinority 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMinority 4:4 D=20 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMinority 4:4 D=150 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMinority 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMajority 5:3 D=20 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMajority 5:3 D=150 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMajority 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| PortableOnMajority 4:4 D=20 | ok+1 | ok+1 | ok+1 | SPLIT/sealed | ok+1 | SPLIT/sealed | ok+5 | ok+5 | ok+5 | ok+5 |
| PortableOnMajority 4:4 D=150 | ok+2 | ok+1 | ok+2 | SPLIT/sealed | ok+2 | SPLIT/sealed | ok+1 | ok+1 | ok+1 | ok+1 |
| PortableOnMajority 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| FullLicensing 5:3 D=20 | ok+1 | ok+1 | ok+1 | ok+8 | ok+1 | ok+8 | ok+5 | ok+5 | ok+5 | ok+5 |
| FullLicensing 5:3 D=150 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| FullLicensing 5:3 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| FullLicensing 4:4 D=20 | ok+1 | ok+1 | ok+1 | ok+8 | ok+1 | ok+8 | ok+5 | ok+5 | ok+5 | ok+5 |
| FullLicensing 4:4 D=150 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 | ok+1 |
| FullLicensing 4:4 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed |
| AttemptsNoLicence 6:2 D=320 | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | SPLIT/sealed | ok+1,crossed |


### 5.2 Results — adversaries (safety: the merchant's X on the victim)

`ok`: X stands (or was reversed inside the shallow window — the stated price). `REV@d`: reversed `d` DAA deep. `/bw`: decided by blue
work alone.

**Applied challenge window 120** (earliest `Final` 146 DAA after acceptance):

| adversary (release depth) | SQ | C1 | C2 | C3 | F | C1+C3 | E' | E | E+F | K |
|---|---|---|---|---|---|---|---|---|---|---|
| PrivateHeartbeats @3 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateHeartbeats @4 | REV@4 | REV@4 | REV@4 | ok | REV@4 | ok | ok | ok | ok | ok |
| PrivateHeartbeats @40 | REV@86 | REV@86 | REV@86 | ok | REV@86 | ok | ok | ok | ok | ok |
| PrivateHeartbeats @295 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateHeartbeats @305 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @3 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @4 | REV@4 | REV@4 | REV@3/bw | ok | REV@4 | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @40 | REV@86 | REV@86 | REV@39/bw | ok | REV@86 | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @295 | ok | ok | REV@294/bw | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @305 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PanelCollusion @3 | ok | ok | ok | REV@24 | ok | REV@24 | REV@24 | ok | ok | ok |
| PanelCollusion @4 | REV@3 | REV@3 | REV@3 | REV@24 | REV@3 | REV@24 | REV@24 | ok | ok | ok |
| PanelCollusion @40 | REV@39 | REV@39 | REV@39 | REV@39 | REV@39 | REV@39 | REV@39 | ok | ok | ok |
| PanelCollusion @295 | REV@294 | REV@294 | REV@294 | REV@294 | REV@294 | REV@294 | REV@294 | ok | ok | ok |
| PanelCollusion @305 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| DaTimeBomb @40 | REV@120 | REV@120 | REV@120 | ok | REV@120 | ok | ok | ok | ok | ok |
| SybilFreshNode @40 | REV@440 | REV@440 | REV@440 | REV@440 | REV@440 | ok | ok | ok | ok | ok |

**Applied challenge window 1200** (earliest `Final` 1226 DAA after acceptance):

| adversary (release depth) | SQ | C1 | C2 | C3 | F | C1+C3 | E' | E | E+F | K |
|---|---|---|---|---|---|---|---|---|---|---|
| PrivateHeartbeats @3 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateHeartbeats @4 | REV@4 | REV@4 | REV@4 | ok | REV@4 | ok | ok | ok | ok | ok |
| PrivateHeartbeats @40 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateHeartbeats @295 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateHeartbeats @305 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @3 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @4 | REV@4 | REV@4 | REV@3/bw | ok | REV@4 | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @40 | ok | ok | REV@39/bw | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @295 | ok | ok | REV@294/bw | ok | ok | ok | ok | ok | ok | ok |
| PrivateJunkAttempts @305 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| PanelCollusion @3 | ok | ok | ok | REV@24 | ok | REV@24 | REV@24 | ok | ok | ok |
| PanelCollusion @4 | REV@3 | REV@3 | REV@3 | REV@24 | REV@3 | REV@24 | REV@24 | ok | ok | ok |
| PanelCollusion @40 | REV@39 | REV@39 | REV@39 | REV@39 | REV@39 | REV@39 | REV@39 | ok | ok | ok |
| PanelCollusion @295 | REV@294 | REV@294 | REV@294 | REV@294 | REV@294 | REV@294 | REV@294 | ok | ok | ok |
| PanelCollusion @305 | ok | ok | ok | ok | ok | ok | ok | ok | ok | ok |
| DaTimeBomb @40 | ok | ok | ok | ok | REV@359 | ok | ok | ok | ok | ok |
| SybilFreshNode @40 | REV@440 | ok | REV@440 | REV@440 | REV@440 | ok | ok | ok | ok | ok |


### 5.3 Reading

* **SQ** heals an honest partition only when the side heavier in blue work is also strictly ahead economically, or inside two DAA; every
  tied partition — and r1 — is a permanent split, sealed in 154–300 slots (5.2–10.2 h). It loses X to the stale-incumbent timing (V6), to panel
  collusion, and captures Sybil-fed fresh nodes.
* **C1** heals whenever the economics differ — on the economic winner's side, the minority included (r1 converges on B: seven nodes
  reorg 20 DAA onto the two-node branch). Ties still split. Safety as SQ (V6 included, because its keys still count portable weight,
  and the attacker's clock lead beats even a fresh comparison), Sybil fixed.
* **C2** never helps an honest partition (3:2 is not 4×) and lets junk headers reverse X at any depth under the seal: it trusts blue
  work. Not a candidate.
* **C3** removes the portable advantage and V6 (both forms, with the exclusive-past definition of §3.1) — and with it SQ's only healing path for portable cases: a portable licence on either
  side becomes a tie, and every tie splits. Partial, as LIVE-R1 said: safety improves, liveness does not.
* **F** heals nothing SQ does not (it only keeps the side holding the refused branch unsealed), and it **reopens the DA time bomb**:
  holding the finality point open on a heavier refused branch lets a reorg land once a court void flips the keys, past the depth
  finality would have stopped it at (`DaTimeBomb` with the unshortened window: REV@359 under F only). An attacker can also hold any
  node's finality open by feeding it a heavier junk branch. Not recommended.
* **E′** shows why participation must come first: with the economic keys ahead of it, panel collusion reverses X.
* **E** heals every partition shorter than the seal in which bonds were active on both sides — 5:3, 4:4, portable licence on either side,
  r1 (on the bond majority's side, 5 DAA after the heal, the old-datadir IBD then commits) — and holds against every adversary modelled.
  It cannot heal a heartbeat-only partition (zero participation on both sides — §2.3) or a partition longer than the seal. Its trust
  assumption: the attacker controls fewer bonds than the honest bonds active in the public branch's exclusive past over `W_p`; at an even split of at least a
  third of the bonds each, GHOSTDAG's order decides (an attacker that ties the honest participation is already at that line).
* **E+F** adds nothing to E in any modelled case.
* **K** additionally heals a partition longer than the seal when one side holds more than 2/3 of the bonds (6:2 row), by crossing
  finality. That is a change to finality itself, safe only under a < 1/3 Byzantine-bond assumption with equivocation evidence; not
  needed for the first step.

## 6. Real multi-node runs (for the Lead to schedule)

`scripts/finx-devnet-partition.sh`, written for H1's harness (`tests/hf-onboarding` on `onboard/h1-hf-closed-loop`: its node layout,
the `ISOLATED` mark, `start_node`/`stop_node`, `rpc`). It launches nothing on its own; run against a devnet `devnet.sh up` already
brought up.

| run | command | predicts (status quo) | decides |
|---|---|---|---|
| R1 | `split 2` | the sinks agree within minutes | control for R2 |
| R2 | `split 3` | permanent split; B logs "all-economic tie deeper than the shallow window" each resolve (LIVE-R1's explained wedge warning) | V2 on real nodes, with no licence anywhere (3 DAA is under the anchor delay) |
| R3 | `seal` after R2 | the first "Finality Violation Detected" in B's log ≈ 600 ÷ B's blue a slot DAA after the fork (on the drill's own depth) | the seal time, on the wire |
| R4 | `fresh minority` / `fresh majority` during a split | Z ends on the side it synced from | V5 |

## 7. The candidates compared

| rule | heals a tied partition | heals r1 | private branch (tied) | stale incumbent (V6) | panel collusion | DA time bomb | Sybil fresh node | trusts blue work | fence / fingerprint |
|---|---|---|---|---|---|---|---|---|---|
| SQ | no (> 2 DAA) | no | holds | **reversed** | **reversed** | holds | **captured** | no | — |
| C1 | no | yes (minority side) | holds | **reversed** | **reversed** | holds | holds (short window: **captured**, by V6) | no | new fork-choice fence; relay change node-local |
| C2 | no | no | holds (junk: **reversed**) | **reversed** | **reversed** | holds | **captured** | **yes** | fence |
| C3 | no | no | holds | holds | **reversed** | holds | **captured** | no | fence; a delta walk from the fork |
| F | no | no (unsealed, wedged) | holds | **reversed** | **reversed** | **reversed** | **captured** | no | node-local; bounded by pruning |
| E | **yes, if bonds active on both sides** | **yes (bond majority)** | holds | holds | holds | holds | holds | only at an even split ≥ n/3 | fence; C3's walk + the attempt headers of both exclusive pasts; no state-root change |
| K | E + > 2/3-bond partitions past the seal | yes | holds | holds | holds | holds | holds | as E | fence; changes finality |

"Fence / fingerprint": every rule that changes which chain a node follows must be armed by a `Params` fence at a scheduled DAA (the
`palw_reorg_strict_economic_win` pattern: scheduling moves the params id and the schedule id, the consensus identity id moves at
activation). None changes block validity or the state root. F is node-local (the finality point is virtual-local) but changes which
chains a node can follow, so a mixed network still disagrees.

## 8. Reproduce

* Model: `cargo test -p kaspa-consensus-core --test finality_palw_consistency_model -- --nocapture` (≈ 2 min; prints §5's tables).
* Pipeline: `~/Downloads/MISAKA-wt-b/buildslot.sh cargo test -p kaspa-consensus --lib finx_p0 -- --nocapture --test-threads 2`
  (≈ 10 min of tests after the build; `finx_p0_b` is the long one).

## 9. GAPs

* **The live DNS confirmation lag** — V6's reach on the live chain while the DNS BFT veto confirms (§3.1). Read it off the fleet.
* **The merge-borrow variant of V6** (§3.1) is analysis only; neither the model (no merging) nor a pipeline test exercises it. It decides
  the definition the fix must use (exclusive past), not whether the fix is needed.
* **C3 and rule E are not implemented in real code**, even behind a test-only fence: C3's fork-relative keys need the weight of each claim
  in a tip's exclusive past, which `retired_safe_weight` and the F-W capacity index aggregate away — re-deriving them is the
  implementation's first piece of work. The C3-vs-E comparison rests on the model.

* The model abstracts merging; the pipeline tests carry it (they merge whatever the real virtual merges).
* V6 is measured through the pipeline in its carrier form (`finx_p0_e`); the `Final`-crossing form with a clock lead is MODEL only.
* testnet-12's real attempt cadence per card is not measured here; it sizes `W_p` (§5).
* E's participation count needs the attempt headers above the fork on both branches: header-only data, bounded by the finality depth,
  but its cost on a node fed many junk branches is not measured. A header-level pre-filter (participation is computable before bodies
  arrive) is the obvious bound.
