# ADR-0172 — Consensus accounting v2: three block tiers and one tick a slot — C-BLUE carries the chain, E-BLUE carries a claim's execution and nothing else, FALLBACK is the one transparent reserve

* Status: **PROPOSED 2026-10-04** on `rcore/consensus-accounting-v2` (lane AC, off `rcore/int-12` `0b1c11b87`). Design plus a
  **DORMANT** skeleton behind one new fence, `Params::palw_accounting_v2: Option<ForkActivation>` (`None` on every preset, in no
  flag-day list, no height chosen). **Not in the DAA-5,300 release; it must not disturb it.** The next-fence candidate after 5,300.
* Direction: the user, 2026-10-04 — *one* consensus-accounting redesign, *one* ADR, four pillars (§1).
* Builds on: [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md) (lane colouring, heartbeat transparency, F1 same-chain),
  [0142](0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md) (the clock cursor: the header decides the DAA score),
  [0058](0058-palw-merged-work-is-counted.md) (merged work is applied, red or blue),
  [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md) (the round lane),
  [0165](0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md) (rev 4: A″ floor reserve, B real clock tick, FALLBACK-V1 as
  *designed*, its honest limit §00.2, §10 the agenda this ADR takes), [0170](0170-the-seed-anchor-is-a-window-not-a-span.md) (anchor window),
  and — **on other branches, read there** — ADR-0168 §10 (Round colouring class, `rfc8/claim-backed-blocks`), ADR-0169 + the RFC-0008
  implementation spec (merge admission, `palw_ws_clock_v1`, `rfc8/claim-backed-blocks`), RFC-0008.
* Amends, **under the fence only**: 0105 (the heartbeat lane is retired into FALLBACK; F1 becomes unconditional), 0125 (a round block is
  E-BLUE, not "red"), 0142/0165-B (the tick carrier is attributed by a stated rule; the grant is unchanged, and FALLBACK's wait for a REAL is producer policy), 0165 (BASE-0 is retired;
  FALLBACK-V1 is built as §5 of this ADR says), 0170 (who may record a seed anchor), 0168 (§10 adopted; the trailer/closure of §§2–5 is *not*
  used and `palw_exec_class_v1` must stay unarmed), 0169/RFC-0008 (D1 colouring and D5 clock of a slice; D6 stage A obsolete).
* Supersedes nothing that is armed. **Every rule below is dormant until its fence is armed.**

**User decisions of 2026-10-04 folded in** (the coordinator's message): Q1 FALLBACK reuses algo 8 (no new lane id); Q2 a work slice is E-BLUE (RFC-0008
D1/D5/D6's model-backed selected chain is **dropped**, stated in §9); Q3 σ = 0 — `safe_weight` only, never blue work — with the allocation code built;
Q4 the 20 s grace is node policy, not a consensus rule; Q5 FALLBACK is fee-only, with a constant-0 hook for a carrier-only subsidy; and `W_claim` is
canonical *active* compute (§6a). Q6–Q14 follow this ADR's recommended defaults and are listed in §10. A **fifth pillar, emission** (user decision, 2026-10-04 ~14:30), is §6b: one block's subsidy a DAA, shared by `W_claim`.

(日本語の要旨は §0.1)

## 0. The sentence this ADR is

**The chain is carried by blocks that are (a) real-model attempts, or (b) a bonded reserve that is transparent to them; everything a
claim emits afterwards is canonical but consumes no k, no blue score, no DAA and no clock; and the clock ticks at most once a slot, REAL
first, the reserve behind it — and a node knows which block is which from what the DAG and the chain's own state say, never from what a
header says about itself.**

### 0.1 要旨

* 3 層: **C-BLUE**(実モデル attempt = 有用仕事+安全性+時計の担い手。通常の GHOSTDAG 会計)、**E-BLUE**(1 claim が出す round/slice =
  正規ブロックだが k・blue_score・blue_work・DAA・merge-depth/finality/pruning を一切進めず、PALW `safe_weight` だけを claim 予算内で持つ。
  RED ではなく別状態)、**FALLBACK**(BASE-0 floor と heartbeat を 1 種に統合した bonded 予備。実モデルが止まった時の時計・最小仕事。
  同一 selected chain 上では後から来る REAL を RED にしない = ADR-0105/F1 を継承、別 branch は通常の着色)。
* 時計: 1 slot 1 tick(最大 DAA +1)。REAL が先、FALLBACK は予備(待ち時間 20 s は producer policy で合意規則ではない)。REAL+FALLBACK が同 slot にあっても +1。どちらも有効ブロック
  (「REAL がいるから無効」にしない)。carrier は mergeset の header 情報だけから決定的に選ぶ。
* 分類は header の自己申告 class に依らない: header の *lane*(PoW algo)と静的事実、DAG 上の位置、merging chain の rooted state だけを使う。
  着色(header 段階)は lane+DAG のみ、信用/重み/失格(virtual 段階)は rooted state のみ。両者は混ざらない。
* 未決事項は §10 に正直に列挙(ユーザー判断が要るもの 9 件)。

## 1. The four pillars (the user's direction, restated as requirements)

1. **C-BLUE** — consensus/security blocks: real-model attempts (useful work + security + clock carrier). Normal GHOSTDAG accounting: `k`,
   `blue_score`, `blue_work`, DAA, finality, pruning.
2. **E-BLUE** — canonical execution blocks: the rounds/slices one claim emits. Same BlockDAG, DB, gossip, hash namespace; first-class
   canonical members; but **no `k` anticone consumed, no `blue_score` increment, no DAA increment, no merge-depth/finality/pruning
   advance**; they carry PALW `safe_weight` only. **Invariant per claim: `attempt_weight + Σ round_weight ≤ W_claim`.** Not "RED": a status
   distinct from the losing, non-canonical RED.
3. **FALLBACK** — BASE-0 floor *and* heartbeat are both retired and merged into **one** FALLBACK kind, a special C-BLUE, for clock, emergency
   liveness and minimum work when real models stop. It inherits ADR-0105/F1: transparent when colouring a later-arriving real attempt on the
   *same* selected-chain ancestry; on another branch, classic colouring (no borrowing work across forks). **FALLBACK never turns real work RED.**
4. **Single-slot clock** — REAL first, FALLBACK reserve, at most one DAA per slot. Both may be valid blocks (no "invalid because REAL exists":
   propagation differs between nodes); the tick carrier is chosen deterministically; REAL + FALLBACK in one slot is **+1, never +2**.

5. **Emission** (added 2026-10-04) — the PALW issuance of one DAA, over every route, never exceeds `calc_block_subsidy(daa)` (§6b).

Invariants to state and to test (§8): `round_count↑ ≠ blue_score↑ ≠ DAA↑`; `fallback_count↑` never reddens real work; the per-claim weight
bound. Explorer metric: useful-work ratio against fallback.

## 2. What is wrong today (measured, and why each earlier fix is partial)

| defect | evidence | the partial fix that exists |
|---|---|---|
| a REAL attempt goes RED behind floor blocks at `k = 1` | P2, last 300 DAA of t12: **71 of 72** 8k attempts RED, every competitor a floor (141 counted peers, all floors), floors 3.0 a slot, 64 % external (ADR-0165 §00.2) | A″ (floor refused by the *fold* while REAL flows) — worthless without producer compliance: 1.4 % REAL BLUE if everybody ignores the hold, 72 % if one producer holding a tenth of the floors does |
| the floor was the chain's anchor factory | 98.7 % of t12 spans anchored by a floor; none of 86 REAL attempts a chain block (ADR-0165 §00.9, ADR-0170 §1) | anchor duty (policy) + `palw_anchor_window_v1` |
| a round block is "red, always" | ADR-0125; the stale-lane wedge of ADR-0168 §1 (10,106 refused submissions) | ADR-0168 §10 (designed, colouring unbuilt) |
| header-stage colouring cannot see state | ADR-0165 §10.1: the class, bond, room are state; the header's claimed class is unauthenticated | merge admission stage A (floors only; stage B unbuilt) |
| two block kinds that are "not a model" are two thirds of the chain | 600 blocks at DAA 3,939–4,071: 67 % attempts (almost all BASE-0), 33 % heartbeats (ADR-0165 §1) | FALLBACK-V1 (designed in ADR-0165 §5, not built) |
| the clock is carried by "newest source of any kind" with node-policy preference | ADR-0165 00.6 | none at consensus level |

Each fix is one half of the problem and several contradict each other: merge admission stage A *removes* the anchors lane A and the seed
stand on (ADR-0169 §3 item 3); R3 (class-aware colouring, ADR-0165 §10.3 / spec §7.1) grants unverified BLUE to a header-claimed REAL; the
RFC-0008 slice is a selected-parent candidate that ticks the clock while ADR-0168 §10 makes a round weightless. This ADR replaces the set
with one coherent accounting.

## 3. The tiers

| | C-BLUE | E-BLUE | FALLBACK |
|---|---|---|---|
| **lane** (header-verifiable shape) | attempt lane, algo 6 / 9 | round lane algo 10, and a work slice (algo 11) *if ever armed* (Q2) | **algo 8, re-meant** (Q1): the heartbeat lane at or past the fence |
| **colouring** (header stage) | `Weighted`: invisible to FALLBACK peers, counts other C peers; F1 same-chain always | `ExecNonScoring`: never enters `mergeset_blues`, never a peer, never enlarges a count | `Yielding` (today's `Heartbeat`): counted against every blue, never enlarges a C block's recorded count |
| `k` anticone budget | consumed (among C) | **0** | consumed against C blues, **never against a C block's budget** |
| `blue_score` | +1 per blue | **+0** | +1 per blue (it is a C-BLUE of the reserve kind) |
| `blue_work` | `2^20` (attempt constant, never the claim's pwu) | **0** | ε = 1 |
| DAA | exempt lane; tick source (REAL-first) | outside the DAA set; never a source | exempt lane; tick source (reserve; carries when no REAL qualifies) |
| merge-depth / finality / pruning | blue-score clocks, advanced | **not advanced** (blue score +0) | advanced by its +1 |
| PALW weight | the claim's `W_attempt` at `Final` | `⌊R/N⌋` per credited round into `safe_weight` | `w_fb` credited only while the floor state is Idle, one per bond per slot |
| reward | the claim's carve | fees, paid per the fold (ADR-0125) | **fee-only**; a constant-0 carrier-only subsidy hook (Q5) |
| status names | `C_BLUE` (credited) / `C_RED` | `E_BLUE` (credited) / `E_VOID` (structurally canonical, fold-refused) / `RED` | `FALLBACK` / `FALLBACK_RED` |

**Why E-BLUE is not RED, and where the difference lives.** GHOSTDAG already records a round block in `mergeset_reds` without walking it
(`round_flags` in `ghostdag()`), never adds it to a score and never counts it against anybody: that is *exactly* the E-BLUE accounting. So the
GHOSTDAG output needs **no stored-format change** for E-BLUE: `mergeset_reds` is read as "the non-scoring members", and the E-BLUE / RED split
is a **pure, recomputable verdict** (§4.3) over the stored GHOSTDAG data and the headers — the same function on the header path, the virtual,
reorg, IBD and a pruned node — consumed by the fold, the RPC and the audit index. This is a deliberate choice against a new
`mergeset_exec` field: such a field would change the stored `GhostdagData`, the P2P trusted-data messages and the proof apply path.

## 4. How a node knows what a block is — classification

### 4.1 The principle (answer to question (i))

> **R-NoClass.** No colouring, tier, transparency, tick eligibility, weight or credit decision may read a *self-declared class* in the
> declarer's favour. A header's claimed `class_id` (and every field of its claim) may be used only **against** the declarer (to refuse
> a retired class), and only where the refusal is stateless. Everything that favours a block is decided from evidence the *branch* holds.

Evidence, from weakest to strongest, and where each may be used:

| level | what | available at | may decide |
|---|---|---|---|
| **E0** lane | the PoW algorithm the header satisfies (algo id), its stateless-valid envelope (signature under the embedded key), its DAA score and timestamp | header stage | the *lane*: Attempt / Exec / Fallback / legacy; the tick-source facts |
| **E1** DAG | parents, mergeset, reachability, GHOSTDAG store of the ancestors, selected-chain ancestry of the *merging block* | header stage (GHOSTDAG) and every later stage | colouring; the F1 hang test; the E-BLUE structural verdict |
| **E2** static | `Params` and the chain's immutable facts: the genesis operator key set, the retired class ids, constants, fence heights | everywhere | operator-ness; refusal of a retired class (against the declarer) |
| **E3** rooted state | the PALW fold state of the merging chain's selected parent: floor machine, bond registry, claim ledger, budgets | virtual stage only | **credit, weight, reward, chain-disqualification — never colour** |

**A lane is not a self-declared class.** The algo id is chosen by the producer but is *proved* by the PoW the header must then satisfy
(heartbeat/FALLBACK: the fixed `2^-24` puzzle and a signed envelope; attempt: the attempt-lane check; round: its envelope and permit). A producer
who claims the FALLBACK lane for a REAL block only handicaps it (ε work, reserve treatment). A *class* (BASE-0 versus a registered model) is a
claim about state and is never evidence. **The direction of every grant is checked:** transparency is granted *to* the candidate being coloured
`Weighted` by *peers' lanes* (a peer that claims to be FALLBACK is only removing itself from the competition); no block can claim a lane to gain
an advantage over another.

### 4.2 Colouring is header-stage and uses E0, E1, E2 only

For a mergeset candidate `c` merged by block `m` (selected parent `sp`), keyed on **`c`'s own DAA score** (the ADR-0105 key: fixed before any
block that merges `c` is coloured, so no verdict depends on `m`'s own output):

```
if c.daa < F (palw_accounting_v2)            -> legacy rule, byte for byte (lane_coloring of ADR-0105)
lane(c) == Exec                               -> ExecNonScoring            (recorded non-scoring; no walk)
lane(c) == Fallback                           -> Yielding                  (today's Heartbeat rule)
lane(c) == Attempt:
    not hangs_from_the_merging_chain(c, sp)   -> Classic                   (F1: no borrowing across forks; UNCONDITIONAL here)
    else                                      -> Weighted                  (FALLBACK peers invisible; merge-depth floor after the first skip)
```

Peers that a `Weighted` candidate skips: **FALLBACK headers (algo 8, own DAA ≥ F) and legacy heartbeats (algo 8, own DAA < F)** — the same transparency, so below and at the fence nothing changes in `ghostdag()`. A pre-F BASE-0 floor
(attempt lane, DAA < F) is an ordinary attempt and counts classically against post-F REAL candidates until it leaves the merge-depth window
(~30 blue score); no post-F floor exists (§7). Nothing here reads a class.

### 4.3 The E-BLUE structural verdict is recomputed, not stored

```
exec_verdict_v2(c, m) = EBlue  iff  lane(c) == Exec ∧ fence(c.daa) ∧ hangs_from_the_merging_chain(c, m.selected_parent, merge_depth)
                        Red    otherwise                                    (another branch / outside the window)
```

`hangs_from_the_merging_chain` is ADR-0105 §11's walk on the **GHOSTDAG store** (selected-parent pointers and blue scores), bounded by
`merge_depth + 1` visits, answering `false` — identically on every node holding the same window — where a read fails. It is **not** read off the
reachability tree (ADR-0105 §11 explains why a pruning-proof node's tree can differ). The verdict is a function of the merging block's stored
selected parent chain and the candidate's header: nothing else.

E-BLUE colouring **grants nothing**: no work, no score, no tick, no k effect. That is why a header-only structural verdict is enough for it and
why a junk Exec-lane header can be "E-BLUE" in colour without harm: whether it is *credited* is E3 (the fold). Header-stage junk is bounded by
the lane's existing hygiene (mergeset size limit, template predicate, ADR-0125/0168 §3); this ADR adds no new junk surface.

### 4.4 State decides credit; it never decides colour (E3)

The fold (virtual stage) applies, on the merging chain's own state: A″ idle gate and bond eligibility (FALLBACK credit); the claim ledger,
permit, uniqueness `(claim_id, round_index)` and the budget (E-BLUE credit); the class lottery, bond, room, exposure (C-BLUE claim). A refusal is
a **skip** (ADR-0058: the merging block did not author its anticone), except for a chain block's *own* work, which disqualifies as it does today.
The colour is never revisited: a block's GHOSTDAG data is a pure function of its past, written once.

### 4.5 One answer on every path

| path | what it computes | why the answer is the same |
|---|---|---|
| header validation | `ghostdag()` with the lane fence | pure function of the DAG and the candidate's header |
| body validation | the same stored data; stateless refusals (algo 8 / BASE-0 past F, malformed FALLBACK envelope, Exec shape) | a body-invalid block is never in a valid block's past, so it never colours anything that is accepted |
| virtual / reorg | the virtual's own `ghostdag()` from its parents; the fold via deltas that revert and apply exactly | per-block colour is immutable; fold state is a function of the selected chain |
| IBD | headers first through the same manager; bodies later | identical function; a header-only node never reads E3 |
| pruning proof build / validate / apply | the same constructor parameter at all four sites (`GhostdagManager::new`, `with_level`) | FALLBACK and Exec derive no block level (no chain position / level 0), so proof levels > 0 hold C blocks only and the rule has nothing to apply there; level 0 is the header path |
| any arrival order | colour keyed on the candidate's own DAA, verdict on the merging block's own selected chain | no input depends on arrival |

What can still differ between a pruned and an archival node is exactly what already can for ADR-0105 F1: a walk that runs off the retained window
answers `false` on every node that holds the same window. Validating a block above the pruning point never needs deeper data than the
merge-depth window, which pruning keeps.

## 5. FALLBACK (answer to question (iii))

FALLBACK **is algo 8** (Q1): the heartbeat lane's mechanics (the fixed `2^-24` puzzle, ε work, lane-priced exemption, ADR-0142's stamp/slot/lead-cap
rules, ADR-0142 §9's chain-capacity pacing, ADR-0105's colouring) with three additions and two retirements. It is ADR-0165 §5 built, with the
corrections its open questions needed. No lane id is allocated; the earlier proposal of algo 12 is withdrawn.

**What algo 8 means on each side of the fence `F` (keyed on the header's OWN DAA score).**

| header | below `F` | at or past `F` |
|---|---|---|
| algo 8 | a heartbeat: unbonded, ε, tick source, no envelope | **FALLBACK**: a signed bond envelope is required (stateless), ε work, tick source, credit only with an eligible bond |
| attempt lane declaring the BASE-0 class | the floor: a bonded attempt, a claim, a carve | refused at the header stage (a retired class; the refusal is against the declarer) |

**Mutual rejection with an old node.** Both kinds are valid algo-8 / attempt-lane headers to a node that has not armed the fence, so the rule is
enforced by *blocks*, not by an unknown lane id: an old node accepts an unsigned algo-8 block and a BASE-0 floor at or past `F`; a new node refuses
both. The first such block splits the chains by design; the fork id names `F` (the params id and schedule id move, the identity does not), so a
launched peer is refused at the handshake from `F` and a connection made before it is re-judged (ADR-0105 §11's mechanics). Without a new lane
id nothing in the header parse tells an old node what a FALLBACK is, which is why the fork-id gate — not the lane — is the first line, and why every
node must run the armed build **before** the network reaches `F`.

**Where the envelope rides** (the one implementation detail this choice forces): `palw_commitment` is hashed only for PALW algos, so the envelope needs
either the coinbase payload or the header-hash gate widened for algo 8 *keyed on the header's own DAA ≥ F* (`pre-F` hashes unchanged). Recommended:
widen the gate, keyed so (stage 5; the choice is made there against the stored-header format).

1. **Envelope.** `{ version, bond outpoint, ML-DSA-87 pubkey, signature over ("MISAKA-FALLBACK-V1", selected parent, DAA score, bond) }`, verified
   *statelessly* against the embedded key. The attempt signature context is reused: a **new** context would change `signature_contexts_root`, part of
   testnet-12's ruleset id (ADR-0165 §5.1).
2. **Validity is cheap; credit is bonded.** A FALLBACK is *valid* with any key (the clock stays permissionless, the ADR-0060 doctrine). It is
   **credited** only if the fold finds the bond registered, the key the bond's, not Retiring/frozen, `palw_bond_may_take_work_v2`, the floor state **Idle**,
   and the bond not credited this slot (a rooted `bond → last credited DAA`). An unbonded FALLBACK carries ε, a blue-score step and a tick source — what a
   heartbeat carries today — and **no `safe_weight`, no reward**.
3. **Anti-spam.** (a) the `2^-24` puzzle per block; (b) at most `PALW_HEARTBEAT_MAX_PER_MERGESET` (4) per mergeset and ADR-0142 §9's chain-capacity
   pacing; (c) relay: one announced per `(DAA, parent DAA)` and a per-peer allowance (H2); (d) the tick is bounded by the cursor, not by cost.
   **There is no consensus grace** (Q4): waiting `PALW_ACCOUNTING_V2_PRODUCER_WAIT_MS` = 20 s behind a slot's opening for a REAL attempt before minting a
   FALLBACK is the producer's policy (as ADR-0165's heartbeat miner already does); a FALLBACK minted earlier is valid and may carry.
4. **Retirements.** At and past `F`: BASE-0's claim, carve and class-lottery path are unreachable; the A″ floor *state machine* stays as the Idle gate
   for FALLBACK credit and for REAL-flow bookkeeping.
5. **Weight.** `w_fb` credited to a rooted `fallback_weight`; the economic key becomes `(safe_frontier, safe_weight + fallback_weight, immature)`.
   ADR-0165 quotes `w_fb` as 604,250,611 (§6) and a floor's weight as 6,042,506,112 pwu (§00.4) — a factor 10 to reconcile before sizing (Q11).
6. **Emission (Q5).** **FALLBACK is fee-only: subsidy 0.** A carrier-only subsidy for the one FALLBACK that carries a tick while Idle can be added later
   by changing `PALW_ACCOUNTING_V2_FALLBACK_CARRIER_SUBSIDY_MILLI` (today 0, hashed with the fence, read by nothing). The cost is that the external
   miners that were 64 % of the floors (paid by the floor carve) have no subsidy to mine for; liveness then rests on operators and fee-seekers.
   Useful-work accounting of a FALLBACK is 0.
7. **Anchor duty and seed role.** A FALLBACK has no execution, so it can **never** record a seed anchor or be a seed source: its randomness is a hedged
   signature the producer can re-roll for the price of a `2^-24` puzzle (the 09-25 critical, ADR-0152). Therefore: the seed anchor of ADR-0170 is fed by
   **admitted REAL attempts only** past `F`; the **anchor duty** of ADR-0165 §00.9 is carried by an **operator-keyed FALLBACK** (operator-ness is E2: the
   genesis key set, `operator_of_v1` extended to the algo-8 envelope) whose seed is the operator attempt's execution commitment, in the form BASE-0 gave
   it (Q6); and **Candidate-class admission in an all-idle chain** (no admitted REAL attempt in the 24-span window) is skipped — the honest cost.

## 6. E-BLUE: weight, fork choice, merge depth (answer to question (iv))

* **Weight budget (user decision Q3).** **σ = 0, fixed**: a round is a progress record and earns no weight; the allocation code exists and σ is a
  constant of the fence, hashed with it (`PALW_ACCOUNTING_V2_SIGMA_PERMILLE`). Round weight lives in PALW `safe_weight` only and **never in blue work**.
  When σ is raised (a later, separate fence, expected 0.1–0.3, after a drill has measured the effect of a late-arriving round on the reorg gate): a
  claim's `W_claim` splits as `W_attempt = W_claim − R`, `R = ⌊W_claim · σ / 1000⌋`, and each round that is **verified from branch-local evidence** (E-BLUE
  structure, a `Final` claim, a granted permit, inside the window — the fold's verdict, never a header's) credits `⌊R / N⌋` **once per
  `(claim_id, round_index)`** (N = the tickets the claim minted, ≤ 120). Unspent shares are never created. **`W_attempt + Σ round shares ≤ W_claim`** for 0,
  1 or 120 rounds; at σ = 0 the claim weighs `W_claim`, exactly today. Reorg, IBD and a pruning-proof join give the same credit because the ledger is
  rooted state with exact deltas (credit is a set insert; revert removes it). Property-tested at σ = 0 and σ > 0 (`credit_round_v2`, §8 I3).
  *Rule for the σ > 0 fence:* `assert_internal_consistency` states `safe_weight == retired_safe_weight + Σ Final claims`, which a round credit would break; that
  fence must extend the identity (and `retire_claim`'s accounting) by the credited-rounds total.
* **Fork choice.** `blue_work` gains 0 from E-BLUE, so every blue-work comparison (`find_selected_parent`, the sink, `pick_virtual_parents`, the
  pruning-proof level comparison, IBD, the DNS reorg gate's work dominance) reads what it read. Rounds are never selected parents. The PALW
  comparator (`compare_palw_candidates_v1`: safe frontier, then `safe_weight`, then live) sees E-BLUE **only through `safe_weight`**, a fold
  quantity that reverts exactly on reorg. E-BLUE never moves the *frontier* (the first key): a round is credited after its claim is Final.
  A private fork cannot mint E-BLUE weight: it needs a Final claim of the same bond, and each claim's total is bounded; a later fraud verdict
  never changes folded weight (ADR-0168 §10.6) — the attacker's gain is the weight one claim carried, ×1, not ×120.
* **DNS work depth** is `blue_work(sink) − blue_work(anchor)`; E-BLUE adds 0, so rounds neither help nor hurt it.
* **Merge depth, finality, pruning** are blue-score clocks. E-BLUE adds 0 blue score, so it cannot make anything deeper or advance a clock. A
  stale round is not an E-BLUE verdict: it fails the hang test and the merge-depth predicate and is **refused as a merge** (bounded merge
  depth), not coloured; the template uses the same shared predicate (int-10.7), so the stale-lane wedge of ADR-0168 §1 is a red tip, not a
  refused parent. Pruning removes round blocks like any block outside the pruning point's future.
* **Carriage.** Rounds stay in the mergeset through ordinary parent edges (ADR-0125's shape); ADR-0168's coinbase anchor trailer, closure and
  `exec_anchored` rows are **not used** (Q9). One source of truth: the mergeset.

### 6a. `W_claim` is canonical active compute, and a large claim is not free (user decision, verified against the code)

Fairness between models is carried by `W_claim`, not by σ. Checked in the tree (`rcore/int-12` `0b1c11b87`), not assumed:

| property | where it holds | verdict |
|---|---|---|
| `W_claim ∝ pwu`; `pwu` is derived, not declared | ADR-0149: item 6 requires `attempt.pwu == palw_attempt_derived_pwu_v1(class target, derived draw)` and `palw_claim_canonical_weight_v1` returns `claim.pwu`; the draw is the registry row's `economic_ccu_per_claim`, the walk of the class's own program | **holds**, behind `palw_canonical_work` (an ADR-0145 bundle fence; `validate_palw_v2` ties it to the registry). New prerequisite of this fence |
| the weight is **active** compute, not parameters | `palw_tir_work_v1`: dense matmuls priced at MACs × weight width; a routed expert matmul prices **only the experts the router's `TopK` gathers**; attention by `H`; recurrence, normalisation by structure; nothing a registrant lays out is read | **holds**; shown by `tests/palw_accounting_v2_work.rs` on the corpus program `moe-top2-shared` (top-2 of 4 experts): one more position adds exactly `k × (per-expert elements) × layers` = **k/E of the expert parameters**, linear in executed positions |
| unfinished claim → `safe_weight` 0 | the Decision-2 table: a `Final` claim adds its `pwu` to `safe_weight`; a live claim contributes only to `bounded_immature` (staged ‰: Created 0, Anchored 10 ‰, Licensed 1,000 ‰ / S2 250 ‰, under F-W) | **holds** |
| expiry / void removes the weight | `release_for_claim` and the void door subtract `immature_contribution` (or the bond's capped term under F-W); `Voided` contributes nothing; a `Final` later convicted subtracts its ceiling | **holds** |
| a large `W_claim` reserves in proportion | `reserve_for_claim` adds `palw_claim_commitment_v1` = `reserved (= ⌈w_c/ρ⌉, ∝ pwu) + escrow + rights` to the bond's exposure; admission refuses past `collateral × max_exposure_ratio` (`ExposureCeilingExceeded`) | **holds**; ρ is the staged capacity divisor (ADR-0160/0167), a decided policy, not a hole |
| a large claim cannot flood fork choice while it waits | J-1: `bounded_immature = Σ_b min(X_b, W_cap(b))`, `W_cap(b) = ⌊C_b / 6,500 MSK⌋ × FCW`; the comparator ranks safe frontier, then `safe_weight`, before any immature weight | **holds** |
| a large claim occupies a slot while it lives | F-S issuance slots / buckets (`palw_issuance_read_at_v1`), released at `Final` / `Voided` | **holds** |

**Holes found:** none at σ = 0. One rule is added for the σ > 0 fence (above): the consistency identity. One *dependency* is made explicit: the fence now requires
`palw_canonical_work` and `palw_capacity_weight_cap` at or below it (written into `validate_palw_accounting_v2`), because without them `claim.pwu` can be a declared
number and the cap is absent. Not verified here (stated, not hidden): that the shipped canonical-work vector prices a *real* model's attention and KV
traffic like a measured device; the cost table is ADR-0131's and a calibration question, not a consensus-structure one.

### 6b. Emission: one block's subsidy a DAA, shared by `W_claim` (the fifth pillar, user decision 2026-10-04)

**The finding (the coordinator's scan of 21,979 chain blocks' coinbases on the local testnet-12 node; not re-measured here).** Today every claim-bearing block pays one
carve — 3,200.85 MSK, 720 ‰ of the 4,445.6 MSK block subsidy. At ≈3.5 Final claims a DAA that is ≈11 k MSK a DAA and, at ≈157 s a DAA, ≈6.1 M MSK a day, about
1.9× the schedule. ADR-0167's F-EM (`PALW_EMISSION_BLOCKS_PER_DAA_V1` = 16 carves a DAA) was set at three times a measured 5.3 claims, not from the schedule, and
permits up to ≈8× it.

**The rule.** The emission yardstick is the existing Kaspa schedule: `calc_block_subsidy(daa)` (`SUBSIDY_BY_MONTH_TABLE` scaled by `target_time_per_block` = 120 s,
≈0.1 B a month). From the fence:

1. **The DAA's budget is one block's subsidy.** The total a DAA mints on every PALW route — all claims' carves, a FALLBACK carrier subsidy, any other subsidy path —
   is ≤ `calc_block_subsidy(daa)` (`palw_emission_budget_v2`).
2. **Shared by `W_claim`.** With several claims in the DAA the pool is split by canonical compute, `⌊pool · W_i / ΣW⌋` (§6a: `W_claim` is derived active compute), each
   share capped at today's one-claim carve (`palw_emission_claim_cap_v2`, 720 ‰ of the subsidy), so a lone claim is paid exactly what it is paid now. A rider divides its
   lead's share as before (ADR-0167 D2; `palw_emission_rider_split_v2`), never adding to it.
3. **Don't mint the rest.** Remainders, a cap's excess and a DAA with no claim are **not minted** — not carried, not paid, not burned from a balance (nothing was minted).
   This is consistent with FALLBACK's fee-only policy (§5.6): a claim-less DAA issues no PALW subsidy; the carrier hook is a constant 0 and, if raised, counts
   **inside** the same budget (`palw_emission_carrier_subsidy_v2`).
4. **Real time.** The budget is per DAA, the schedule per `target_time_per_block`; a DAA slower than 120 s therefore mints **less per second than the schedule, never
   more** (`palw_emission_rate_sompi_per_s_v2`, tested for intervals from the target up).
5. **Replaces F-EM** (ADR-0167 D1) from the fence: the 16-carve ledger row is no longer the limiter (this rule is tighter whenever more than one claim lands a DAA, and is
   never looser than one block's subsidy). The ledger's removal and the coinbase/fold change (the carve entitlement read from the DAA's split rather than the block's
   own subsidy) are stage 4; until then nothing reads these functions.

**E2 — when the DAA's budget is divided (coordinator's question; both options built and tested).**

| | **A. first come, first served** (a running split in acceptance order) | **B. final-time split (recommended, built into the fold)** |
|---|---|---|
| reserved at acceptance | the escrow, ≤ one claim's carve (today's) | the same |
| paid | `min(escrow, what is left of the DAA's budget)` at acceptance order | `min(escrow, ⌊budget · W / ΣW⌋)` at `Final` |
| fairness | pays by arrival: the first claim takes 720 ‰ of the budget, the third nothing; a producer that orders its claim first, or a parent set that orders the mergeset in its favour, is paid at the others' expense | pays by `W_claim` (canonical active compute, §6a); arrival order is irrelevant |
| `Σ ≤ budget` | yes | yes — proven for any weights, any voids, any order of `Final`s (test) |
| state | none | one rooted row per DAA that holds a live claim: `{ ΣW: u128, budget: u64, open: u32 }` — at most one per live claim, ≈ 8 B key + 28 B value; dropped when `open` reaches 0 |
| reorg / IBD / pruning | order-dependent state | the row is a function of the claims accepted in that DAA on the chain; written and dropped by exact deltas; carried in the pruning carriage against the root, so a pruned join holds the same row. The *payout* reads only the row and the claim |
| `Final` before the DAA closes? | n/a | impossible: `Final` is at least a challenge window after acceptance, and a chain's DAA never falls; the fold refuses to pay from an open row (it pays nothing) |

**ΣW counts every claim accepted in the DAA, voided ones included.** A denominator that excluded voids would shrink as claims void, so an early `Final` would be paid
against a larger share than a late one and the DAA could mint past its budget (a second, order-dependent identity to maintain); with the full denominator the sum is
bounded for every outcome, and a voided claim's share is simply never minted — nobody else is paid more because a rival failed. **Riders** enter ΣW as the claims they are, each
bounded by its own (split) escrow, so a lead's carve is still never exceeded. **Recommendation: B**, implemented in the fold (`finalize_claim`: the payout is capped, the claim's
`W` registered at acceptance, the row released at `Final` or `Voided`); A is kept as a tested function for the comparison. Rows of a pre-fence claim do not exist and its payout is unchanged.

**What this does to the economy, stated.** At the measured 3.5 claims a DAA the pool is one block's 4,445.6 MSK shared by ≈3.5 claims (≈1,270 MSK each, against 3,200.85
now): PALW issuance falls to ≈4.4 k MSK a DAA, ≈2.4 M MSK a day at 157 s — about 0.75× the schedule's rate (the 157 s DAA is slower than the 120 s target). A claim's
reward falls as claims per DAA rise; *capacity* (ρ, the claim count) no longer buys issuance, which is what F-EM was for. Whether the resulting per-claim reward still pays
for a model's compute is **not verified here** and is the economic risk of the rule (§10, E1).

## 7. The clock (answer to question (ii))

**The header decides the DAA score (ADR-0142) — but the header decides it from its *own mergeset's headers*.** `internal_calc_daa_score` is
`sp.daa + (mergeset − non_daa) − exempt`, and `palw_clock_step_v1` grants one tick by removing exactly one exemption when a source qualifies.
So the carrier choice is a function of header facts already in hand at header time; no state is read and nothing is chosen "after".

```
sources in the mergeset (selected parent included), E-BLUE excluded:
  REAL      = attempt-lane blocks   newest stamp  t_R
  FALLBACK  = algo-8 blocks         newest stamp  t_F      (before or after the fence)
slot = cursor.next_slot_ms (None => open)
real_ok     = priced == 0 ∧ #REAL     > 0 ∧ t_R ≥ slot
fallback_ok = priced == 0 ∧ #FALLBACK > 0 ∧ t_F ≥ slot
granted     = real_ok ∨ fallback_ok      ->  removes ONE exemption  ->  DAA +1, never +2
carrier     = REAL if real_ok else FALLBACK if fallback_ok else none
```

**The grant is exactly today's rule** (ADR-0165 B: `palw_clock_tick_source_v1` followed by the slot test; held as a test over 5,000 random fact sets). The
consensus change in this section is *attribution only*: REAL wins a mergeset that holds both, so the useful-work ratio of ticks is a chain fact. There is
no grace in the rule (Q4): REAL-first in *time* is the producer's policy (a FALLBACK miner waits `PALW_ACCOUNTING_V2_PRODUCER_WAIT_MS` behind the slot for
a REAL before minting). A FALLBACK minted earlier is valid and may carry — which is the safe direction for liveness.

**Why it cannot double-tick.** `granted` is a boolean over the mergeset: any number of REAL and FALLBACK sources remove one exemption. Across blocks the
cursor, not the carrier, bounds the rate: a step block is stamped at or after its slot (H5) and at most `now + 132 s` (lead cap), so two ticks are at least
one interval apart in stamp however REAL and FALLBACK interleave; a REAL merged after a FALLBACK already carried the slot is stamped before the new cursor
slot and ticks nothing. The ADR-0165 simulation (adversarial mixes of up to 100 sources) is extended with the two-way carrier (`palw_accounting_v2` tests).
**Both blocks remain valid**: nothing is refused because the other kind exists.

**What REAL-first means and does not mean.** A priority on *who carries* and an accounting fact, never a validity rule. A REAL attempt carries only if its
stamp is at or past the slot; a slow class's attempt is stamped with its template's time and usually is *not* a carrier (ADR-0105 §1), so the reserve
carries. This is the honest limit and a decision (Q8): accept it, or let a *newly merged* REAL carry on the step block's stamp.

**E-BLUE never ticks.** Rounds are outside the DAA set; a slice (algo 11), if armed, is E-BLUE and is not a source — this overrides RFC-0008 D5.

## 8. Invariants, metrics, tests, drill

**Invariants (each a property test over generated sequences, not an example):**

* **I1** `round_count↑ ≠ blue_score↑ ≠ DAA↑`: for any mergeset, adding any number of E-BLUE changes no blue score, blue work, DAA, k peer set or
  recorded anticone count.
* **I2** `fallback_count↑` never reddens a REAL candidate on the same chain: the `Weighted` colouring of a REAL is invariant under adding any
  number of FALLBACK peers (within the merge-depth floor); on another branch it is classic.
* **I3** per-claim weight: `W_attempt + Σ round shares ≤ W_claim` for every `(W_claim, σ, N, rounds spent)`.
* **I4** the clock: DAA step ≤ 1 per slot; REAL + FALLBACK in one mergeset or in one slot is +1; two ticks ≥ one interval apart in stamp;
  the DAA over any horizon ≤ `horizon / interval + 2`.
* **I5** classification is arrival-order independent, identical on archival, pruned, IBD and proof nodes (differential test).
* **I6** below F every colouring, DAA and weight is byte-identical (dormancy), and no shipped preset's fingerprint moves.

**Success metrics** (from chain data, per window; the explorer shows them): **useful-work ratio** = C-BLUE REAL blocks ÷ C-BLUE (REAL + FALLBACK) and
REAL-carried ticks ÷ ticks; REAL BLUE rate (P2's 1.4 % → target ≥ 90 % *independent of any producer's compliance*); REAL BLUE rate as a function of
`fallback_count` (must be flat); ticks per slot (max 1); E-BLUE credited ÷ emitted; Σ weight per claim against `W_claim`; carrier mix and grace use;
fallback-only stretch lengths; REAL reds caused by other REALs (the fake-REAL measure, Q7).

**Drill plan** (run by the lead after the 5,300 combined drill, one drill at a time, `DRILL.lock`, the binary that would ship, a salted chain, the
fence crossing a low height no other fence uses): (1) **crossing** — DAA +1 per slot before, at and after F with REAL, FALLBACK, legacy heartbeats
and floors merged across F; (2) **IBD** — an empty node after F syncs to the same tip, sink, blue score, blue work; (3) **pruning proof** — build,
validate, apply across F with FALLBACK and E-BLUE headers in level 0; (4) **reorg across the fence** — a branch below F against one past it, both
ways; (5) **late REAL after N fallbacks** — N = 1, 5, 13, 14 (the merge-depth edge): blue on the same chain, classic red on a withheld branch;
(6) **a 120-round claim** — DAA +≤ 1 per slot throughout, blue score unmoved by rounds, `safe_weight` ≤ `W_claim`; (7) **attacks** — a withheld FALLBACK
branch absorbing public REAL attempts (F1), unbonded FALLBACK flood (tick ≤ 1/slot, no weight), junk Exec-lane flood, a duplicate-slice storm;
(8) **old vs new** — a launched node refused at F by the fork id; (9) **orders** — the same blocks replayed in shuffled orders on a second node and
a pruned node: equal verdicts. Evidence is read from logs after the action (the drill rules).

## 9. Migration, and mutual rejection (answer to question (v))

Everything is keyed on the **candidate's own DAA score** against `F` (the ADR-0105 key), so a block's rule never depends on who merges it.

| object | at F |
|---|---|
| BASE-0 floor attempts | an attempt-lane header declaring BASE-0 with DAA ≥ F: refused (header stage, stateless). A floor with DAA < F stays admissible wherever merged (made under its rules); its claim settles as before; it colours classically until it leaves the merge window |
| heartbeat lane (algo 8) | DAA < F: valid for ever, a legacy yielding peer and a tick source, no envelope. DAA ≥ F: **FALLBACK** — the same lane id re-meant (§5): a signed bond envelope is required, so an unsigned algo-8 block is invalid |
| round lane (algo 10) | DAA < F: legacy "red". DAA ≥ F: E-BLUE verdict (§4.3). The permit ledger, ADR-0125 rules and fees are unchanged |
| A″ floor state | kept as the Idle gate; `palw_floor_reserve_v1` stays a prerequisite |
| RFC-0008 dormant objects (tag 96, algo 11, deltas 107–109, tail `0xE8`) | stay dormant (**user decision Q2: a work slice is E-BLUE — it advances no clock and no blue score; RFC-0008 D1, D5 and D6, the model-backed selected chain, are dropped**); `validate` refuses `palw_accounting_v2` together with an armed `palw_ws_clock_v1`, `palw_merge_admission_v1` or `palw_exec_class_v1` (mutually exclusive); algo 11 is E-BLUE if ever armed (Q2) |
| the clock cursor | derived from the window as ever (ADR-0142 §6a); no stored cursor, nothing to migrate |

**Mutual rejection.** The fence is hashed Some-only into `consensus_params_id` and `consensus_schedule_id`, named in `palw_fences_v1`, so the fork id
names F: an old node's handshake is refused from F and a connection made before it is re-judged (the ADR-0105 §11 mechanics: F must not equal a
height another fence uses; arm after `palw_t12_base_params`' pass 2; re-make connections made before another fence's height when F is above it). An old
node accepts an unsigned algo-8 block and a BASE-0 floor at or past F, and a new node refuses both: the first such block splits the chains by design (§5).
Identity does not move (peering continues until F), the params id and schedule id do.

## 10. Decisions taken and questions still open

**Decided by the user, 2026-10-04**

* **Q1** FALLBACK reuses algo 8; no algo 12 (§5 states the before/after meaning and the old-node rejection).
* **Q2** A work slice is E-BLUE; RFC-0008 D1/D5/D6 (model-backed selected chain) dropped (§9).
* **Q3** σ = 0, fixed in this fence; the allocation code is built and property-tested at σ = 0 and σ > 0 (§6, §8); raising σ is a later fence after a drill.
* **Q4** The 20 s grace is node policy, not a consensus rule (§7).
* **Q5** FALLBACK is fee-only; a constant-0 carrier-subsidy hook (§5.6).
* **`W_claim` = canonical active compute** (§6a, verified).

**Taken at this ADR's recommended default** (reported in the milestone message; reversible before arming)

* **Q6** Operator FALLBACK carries the anchor duty, with BASE-0's execution commitment as the seed; REAL attempts alone feed the anchor window; the all-idle Candidate gap is accepted (§5.7).
* **Q7** Fake-REAL griefing is *not* closed here (measure first; options: the `2^-24` puzzle on the attempt lane, merge-admission stage B).
* **Q8** Slow-REAL carrier: accept the limit (REAL carries when its stamp is fresh).
* **Q9** E-BLUE is carried through the mergeset (parent edges); ADR-0168's trailer and closure are not used and `palw_exec_class_v1` stays unarmed.
* **Q10** E-BLUE reward is ADR-0125's rule unchanged; the `E_BLUE / E_VOID / RED` verdict is derived and shown through the audit index record (`block_accounting`).
* **Q11** Reconcile `w_fb` (604,250,611 vs 6,042,506,112) before sizing; a rooted `fallback_weight` field (delta 130, carriage tail `0xF3`) is the default, ε-only FALLBACK the fallback position.
* **Q12** F's height is chosen after the 5,300 release has settled and the drill has run, at a height no other fence uses.
* **Q13** Confirm by test that algo 10 and algo 8 at or past F derive no block level (extend `algo_id_derives_no_block_level` only if needed) and that the E-BLUE verdict near the pruning point behaves as F1's.
* **Q14** Audit every predicate that lists algo 8 (difficulty rows, level, blue work, lead cap, relay) for the re-meant lane — ADR-0105's lesson of a stale id list.

* **Emission rule (user, 2026-10-04 ~14:30)** — §6b; implemented as pure functions with property tests, dormant. **E1 (open):** whether ≈1,270 MSK a claim at today's claim rate still pays for the compute; the rule is a ceiling, the per-claim ceiling (720 ‰ of a block) and the share-by-`W_claim` are the knobs, and a later fence can move either.

* **E2 (decided in design, 2026-10-04):** option B, the final-time split over a rooted per-DAA weight row (§6b), ΣW over every accepted claim; option A kept as a tested alternative.

**Still genuinely open:** the σ value (after the drill); the `w_fb` number; the envelope's carriage (§5); how much of Q7 to close before arming.

## 11. Allocations (lane AC, appended to `lanes/COMMON.md`)

ADR 0172; **no pow algo id** (the algo-12 reservation is withdrawn); state deltas 130–133; carriage tail `0xF3`; `DatabaseStorePrefixes` 130–131 (reserved); no object tags. Fence
`palw_accounting_v2`. Spec: [`docs/design/palw/consensus-accounting-v2-spec.md`](../design/palw/consensus-accounting-v2-spec.md).

## 12. What is built (dormant) and what is not

See the spec §9. This ADR itself changes no rule: the fence is `None` everywhere and nothing reads it yet except the validators the skeleton adds.
