# ADR-0168 — An execution block is a third class, and the chain reaches it through an anchor, never a parent

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **RFC8 integration precedence — 2026-10-08:** For the unified EXEC lane, [revised RFC-0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) governs the current direction: `EXEC_TX` and `EXEC_SLICE` share a chain-independent class with zero raw/PALW fork-choice weight, blue score and DAA contribution. §10.3's positive round `safe_weight` pool and `REAL_ROUND`/E-BLUE naming are earlier proposals, not requirements for this lane; root weight is not redistributed to EXEC. Parent/anchor hygiene and bounded lane acceptance remain design references. Earlier branch implementation/status records below do not claim these features are shipped in main or activated.

* Historical Status (superseded for the unified RFC8 EXEC lane by the amendment above): **PROPOSED 2026-10-03, REVISED TWICE (2026-10-03 14:00, user): §10 is the design — a Round coloring class generalising ADR-0105's `LaneColoring`, rounds first-class and transparent to consensus accounting, one weight budget a claim. §§1–9 keep the incident, the parent hygiene, the anchor, the closure, the window and the state rows; where §2 Decision 1 says "weight 0 class" §10 governs.** Target: the next fence after DAA 5,300; dormant on this branch.
  `palw_exec_class_v1`, **dormant on every shipped preset**. See §9 for what is built and what is not.
* Operator's request (2026-10-03): a third canonical class EXEC beside BLUE and RED; consensus parents separate from
  execution-lane heads; acceptance by an anchoring blue block's root inside a window; EXEC never advances the DAA;
  block class and lane health in RPC.
* Builds on: [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (the lane, its permits and its round blocks), [0139](0139-the-execution-lanes-gas-is-one-budget-a-round.md),
  [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md).
* Amends: ADR-0125 Decisions 1, 2 and 8 *past the fence only* (round blocks stop being parents of chain blocks).
  Supersedes nothing.

## 0. The sentence this ADR is

**A round block is canonical and weightless: it is never a parent, never red and never counted; a chain block
accepts the lane by committing, in its own coinbase, the heads it covers and the Merkle root over the blocks they
newly bring, and the blocks outside the two-span window are dropped instead of wedging anybody.**

## 1. The incident

From 2026-10-01 10:12 JST node b0's round producer made 0 round blocks: 10,106 submissions were refused with "block
is violating bounded merge depth" (`~/Downloads/MISAKA-wt-b/lanes/evidence/multi-block-gap-1003.md`). One round block
(`7c0bb084…`, DAA 2,817) had never been merged; `round_adapt_block_template` chose round **parents** from the DAG
with no merge-depth test, every later round block named it, and `check_bounded_merge_depth` refused each of them. A
tip nobody could merge stayed a tip forever, so the wedge was self-perpetuating and survived restarts. The cause is
structural: while round blocks are GHOSTDAG parents, the lane's liveness is hostage to the chain's mergeset rules.
(Lane P2's int-10.7 closes the symptom node-side. This ADR removes the coupling.)

## 2. Decisions

**Decision 1 — EXEC is a class, not a colour.** Past the fence an accepted execution-lane block is EXEC: canonical
(its transactions are accepted when the anchoring block's parent state grants its permit, its fees are paid), never
red, with **fork-choice weight 0 and DAA contribution 0**. It never carries blue work: one real-model claim must not
become 120 units of consensus weight. (A future *bounded* useful-work weight, capped per claim, is a separate fence and
out of scope; nothing here forecloses it.) The class is a function of the header (algo 10 with the lane open) and,
for "accepted", of the chain's anchored set; block queries report `blue | exec | red`.

**Decision 2 — consensus parents are consensus tips only.** Past the fence a block that is not a round block names no
round block as a direct parent (`BadRoundLaneParents`). The chain's GHOSTDAG, mergeset, merge-depth, k-cluster and
DAA windows therefore never see a round block, and a stale lane head cannot reach a template. A round block keeps its
ADR-0125 shape (at most one chain parent — its anchor — and any round parents), so a lane is still a chain of round
blocks hanging from the chain.

**Decision 3 — the anchor trailer.** A chain block that wants the lane accepted ends its coinbase payload (the end of
`extra_data`) with

```
… ‖ version u8 ‖ n_heads u8 ‖ heads[n]·64 ‖ count u32 ‖ root·64 ‖ body_len u16 ‖ "PXA1"
```

(1 ≤ n ≤ 8, heads strictly ascending). The coinbase is inside `hash_merkle_root`, which is in the pre-PoW header, so
the anchor is committed before the grind and nobody can re-wrap a solved block. **No header field moves**: stored
headers, the P2P and RPC header encodings and the block id of every existing block are unchanged — which is why this is
a trailer and not a header field (a new header field would have been a stored-format break that an in-place flag day
cannot carry; the 2026-10-03 analysis is in §7). Magic-terminated bytes are an anchor and a malformed one is refused;
anything else is the miner's.

**Decision 4 — what an anchor covers.** The *closure* of its heads through round-parent edges
(`palw_exec_closure_v1`), stopping at

* a block the parent state already anchored (`exec_anchored`): covered once, never walked past;
* a block **outside the window** — its anchor's span is not this block's span or the one before (the verdict's own
  two spans, the ones the fold keeps schedules and permit ledgers for): **dropped**, and the walk does not go past it;
* a block whose anchor is not on the anchoring block's selected chain: dropped.

A head outside the window therefore contributes nothing and invalidates nothing; the lane **resumes from its latest
anchored checkpoint** (a round block names the checkpoint, not the stale head). Everything else the walk reaches is
covered, in canonical `(round, permit index, hash)` order. `count` is the number covered and `root` is the Merkle root
over their leaves (`H(leaf ‖ hash ‖ round ‖ index)`, odd node promoted, count bound into the root, the empty set has a
root of its own). The anchoring block's validation recomputes the closure from its parent state and refuses a wrong
`count` or `root` (`BadExecutionRoot`, the block is disqualified from the chain like any commitment fault).

**Decision 5 — acceptance.** Where today a merging block's mergeset reds supply the round blocks that
`palw_round_verdicts_v1` judges, past the fence the *covered set* supplies them: appended after the mergeset in
canonical order, judged by the same verdict (permit in the parent state's schedule, bond active and key match, payout
match, permit unused), their transactions accepted in the same loop and their fees paid by the same coinbase rule.
The fold records the covered blocks (`exec_anchored`, state delta 111) and the chain's record of the lane
(`exec_lane`, delta 112). Only a block **on the selected chain** anchors: an anchor in a side branch is judged when
(if) that branch becomes the chain. Reorgs revert the deltas exactly.

**Decision 6 — the clock.** EXEC blocks never advance the DAA score and never carry the tick (ADR-0125 already kept
algo 10 outside the DAA set; this ADR keeps it so). Lane RS's rule — one real-model attempt per clock slot carries
the tick — is unchanged and independent.

**Decision 7 — bounds.** ≤ 8 heads per anchor; ≤ `max_per_mergeset` (the lane's own bound, 600 on testnet-12) covered
blocks per anchor, the walk stopping at the bound (`TooManyLeaves`); the state holds only blocks inside two spans
(`record_exec_anchor` drops older entries); a trailer adds ≤ 650 bytes to a coinbase. A hostile lane costs at most
`max_leaves + 1` visits plus one probe per boundary block it reaches.

**Decision 8 — dependencies.** An anchor names blocks the validating node must hold. At the body stage a block whose
anchor heads lack a body is `MissingParents(heads)` — retryable, never invalid — and the orphan and in-flight
dependency logic treat the heads as parents. The IBD enumeration lists a chain block's covered blocks beside its
mergeset (§5).

## 3. Threat analysis

* **Fork-weight amplification.** The reason for Decision 1. A covered block adds nothing to blue score, blue work or
  the DAA score, and is never in a mergeset, so no number of round blocks moves fork choice. The only weight a claim
  buys is what it bought before (its attempt block). *Residual:* none; a bounded useful-work weight is a new fence.
* **Anchoring withholding.** A producer that wins chain blocks can decline to anchor a rival's lane, or anchor only
  its own. Cost to the lane: a round block's transactions wait for the next anchoring block, and are lost to the
  lane after two spans. Mitigations: any chain block may anchor any lane head in the window, so an honest block
  anchors it a block later; round-block producers are paid only when covered, so withholding is a visible, attributable
  loss (`getPalwRoundLane` reports `rejected_since` and the reason); the user's transactions are also in the mempool
  and are included by ordinary blocks. *Residual:* a 100 % miner can censor the lane, as it can censor anything.
* **DAG bloat / tips.** Round blocks hang beside the chain and are not parents, so they remain DAG tips until pruned
  and a lane head that is abandoned stays a tip. They are excluded from virtual-parent selection and the sink (they
  already were), and `body_tips` is pruned by the pruning point as before. *Residual:* a hostile producer with a permit
  can leave one tip per permit per round; permits are scheduled and bounded (ADR-0125 SA-4).
* **Header size.** Zero: the anchor rides the coinbase. The coinbase grows ≤ 650 bytes per anchoring block; its mass
  counts against the block.
* **Anchor grinding / equivocation.** The anchor is pre-PoW (coinbase in the merkle root), so a producer cannot grind
  it after solving, and a third party cannot swap it. A permit holder who signs two round blocks for one permit is
  still ADR-0125 SA-2's evidence; the covered set holds at most one per `(round, index)` through the permit ledger.
* **Stale-head wedge.** A stale head is *dropped*, never an error: the closure stops at it. The producer builds on the
  lane's anchored checkpoint when its head is stale (`stale` in the health read). The incident of §1 cannot recur by
  this path.
* **Validation cost.** The closure is bounded by Decision 7 and reads headers the node already holds; no state is read
  per leaf beyond `exec_anchored` membership and the permit verdict ADR-0125 already runs.
* **Reorg determinism.** The closure is a function of the anchoring block's coinbase, headers and its selected
  parent's state; the fold writes through deltas that revert exactly (test).
* **Missing data on a syncing node.** See Decision 8 and §5; the failure mode is "wait and fetch", never "disqualify".

## 4. What does not change

Every DAG parameter; the attempt lane, receipt lane and heartbeat; ADR-0125's permits, schedules, equivocation
evidence and fees (the verdict is the same function over a different set); the header, its hash and its stored
shape; every network until its fence is armed. The fence's four places (`Option<ForkActivation>` field,
`for_each_fence`, Some-only writes in `consensus_params_id` and `consensus_schedule_id`, the `never()` collapse),
its reader and `validate_palw_exec_class_v1` (ConsensusV2; `palw_execution_lane` and
`palw_lane_accept_parents_first` open at or below it).

## 5. Pipeline map

| Stage | Change |
|---|---|
| Header, pre-GHOSTDAG | a non-round block past the fence names no round parent |
| Body, isolation | a round block's coinbase carries no trailer; a trailer is well-formed |
| Body, in context | an anchor's heads have bodies (`MissingParents` otherwise) |
| Virtual, chain candidate | closure from the coinbase + parent state; `count`/`root` verified; the covered set joins the verdict, the acceptance loop and the coinbase payouts; fold records it |
| Template | no round parents; the anchor trailer is computed from the lane's heads by the same closure |
| Round producer | extends its anchored checkpoint when its head is stale |
| IBD / orphans | a chain block's covered blocks are listed with it; heads count as dependencies |
| RPC | block class; `getPalwRoundLane` health |

## 6. Spec text

`docs/spec/palw/18-exec-class.md` carries the normative rules (trailer layout, closure, window, root, verdict
inputs, state rows).

## 7. Why not a header field, why not parents

* *A header field* (`execution_anchors`) is the cleanest wire shape and was the first design. It changes the stored
  header encoding (`HeaderWithBlockLevel` is bincode, field-ordered), the P2P and RPC header messages and the
  header-hash preimage, and testnet-12 upgrades in place; the previous header field (`palw_commitment`) arrived with
  a re-genesis. A coinbase trailer has the same commitment properties (pre-PoW, in the block id) with none of that.
* *Round blocks as red parents with an EXEC colour* fixes the weight and the merge-depth refusal but keeps the
  coupling: a lane head is still a candidate parent, still counted in the mergeset bound and still a tip the
  template must choose among.

## 8. Rollout

Dormant everywhere on this branch. Arming is a flag day: the fence height H, with `palw_execution_lane` and
`palw_lane_accept_parents_first` at or below it. Below H nothing changes, byte for byte; at H a template stops naming
round parents and starts anchoring. A node that has not upgraded refuses the first anchoring block (its coinbase is
valid to it, but the round blocks are never merged by it) and diverges in UTXO state at the first accepted lane
transaction, so this is a coordinated fence like every other.

## 9. Status at the time of writing

See the lane report; this section is updated with the final state of the branch.

## 10. The design (2026-10-03 14:00): a Round coloring class, transparent to consensus accounting (option a2)

The operator's design, and a correction of this ADR's first revision: a real-model round block (`REAL_ROUND`, algo 10)
is a **first-class DAG block** — a vertex, gossiped, canonical, shown E-BLUE — that is **transparent to consensus
accounting**. ADR-0105's incident was never "blue real-model blocks"; it was slow bonded attempts landing behind
heartbeats and being coloured RED by `k = 1`. ADR-0105 answered it by making colouring *lane-aware* (`LaneColoring`:
Classic / Weighted / Heartbeat), and ADR-0105 §11 (F1, 2026-09-25) bounded that transparency to candidates hanging from
the merging block's own selected chain. This ADR adds one more class to that mechanism and reuses its code.

### 10.1 The class and what "transparent" means

`LaneColoring::Round`, chosen from the candidate's own header (algo 10 with the lane open **and** this fence active at
the candidate's own DAA score — the key ADR-0105 uses, fixed before any block that merges the candidate is coloured, so
no block's colouring depends on the merging block's own GHOSTDAG output). A Round candidate in a mergeset is **coloured
blue when it is canonical (§10.2) and is otherwise red as any block** — but a blue Round, and the Round's contribution to
every number a *normal* block is judged by, is nil:

| quantity | Round's effect |
|---|---|
| normal blocks' k anticone budget | **none** — a Round is not counted in any non-Round candidate's blue anticone, and does not enlarge its recorded count (ADR-0105's "invisible" rule, and its "a heartbeat must not turn a bonded block red through a third block" rule, applied to the Round) |
| `blue_score` | **+0** — a blue Round is recorded in the mergeset's blues but not counted in the score |
| raw `blue_work` | **+0** — `palw_lane_blue_work_v1` gets an arm returning zero for Round |
| DAA score / windows | **+0** — rounds were already outside the DAA set (ADR-0125) and stay so |
| merge-depth / finality / pruning clocks | **none** — all are blue-score clocks, which a Round does not advance |
| PALW `safe_weight` | **+ its allocated share of `W_claim`** (§10.3), and only that |

So `k` is not changed, no clock is changed, and a Round can neither colour a normal block red nor make a normal block
deeper. What it *is* is a canonical block: its transactions are accepted, its fees are paid, it is blue in the
explorer's sense, and it is a DAG vertex that peers gossip and sync as a block (§10.5).

### 10.2 Canonical iff

A Round is blue in a merging block's colouring iff **all** of:

1. **valid claim** — the claim the permit's bond earned exists in the parent state and is `Final` (ADR-0125 Decision 3);
2. **hangs from the merging block's own selected chain** — ADR-0105 §11's `hangs_from_the_merging_chain`, reused as is
   (the Round's anchor is the merging block's selected parent or a selected-chain ancestor at most `merge_depth` blue score
   below it). A Round hanging from another branch is coloured `Classic`, i.e. red there: this is what closes the
   private-branch borrowing double spend for rounds exactly as F1 closed it for heartbeats;
3. **round index in the permitted range** — the permit's `(span, round, index)` is granted by the schedule (≤ 120 a
   `Final`, ADR-0125 / quanta);
4. **unique** — `(claim_id, round_index)` is not already in the ledger of the merging block's parent state;
5. **budget not exceeded** — the claim's round pool still has a share to give (§10.3);
6. **inside the window** — anchor span within the two spans the fold keeps, and inside the merge-depth floor ADR-0105 §5.1
   derives (a Round whose walk has to pass further back is red, and the stale-lane wedge of §1 is a red tip, not a refused
   parent);
7. **parent and anchor not stale** — via the shared merge-depth predicate of `rcore/int-10-p4` (`bc1c57de9`), the same
   function the template uses to choose parents, so a producer cannot build a Round the validator refuses for staleness.

Items 1, 3, 4 and 5 read the parent state; the colouring walk is a header-stage function with no state. The class is
therefore decided in two steps with one answer: the **structural** part (2, 6, 7: positions in the DAG) is coloured by
GHOSTDAG at the header stage and is what makes a Round "blue" in the DAG's own data; the **state** part (1, 3, 4, 5) is
decided by the merging block's parent state, exactly as ADR-0125 decides a permit today, and a Round that fails it is
merged blue-or-red as the structure says but is credited nothing, its transactions are not accepted, and its fees are not
paid (the ADR-0125 "red without its permit" outcome, unchanged). Blue-by-structure therefore never depends on state a
header-only node lacks, and acceptance never depends on colour alone.

### 10.3 The weight budget

Fork-choice weight is PALW state weight (`safe_weight`, the comparator's keys), so the budget lives in the fold:

* at `Final` a claim is worth `W_claim`, its certified useful work, as today;
* the attempt block keeps `W_attempt = W_claim − R`, and the rounds' pool is `R = ⌊W_claim · ρ / 1000⌋` (`ρ` a fence
  companion value; `ρ = 0` makes rounds weightless and trivially respects the budget);
* each canonical round adds `⌊R / N⌋` to `safe_weight` and records `(claim_id, round_index)`; `N` is the number of
  tickets the claim minted (≤ 120); shares never granted (an unspent ticket, the division's remainder) are never created:
  **`W_attempt + Σ round shares ≤ W_claim` for 120 rounds or one**. (A CCU-proportional split is the alternative; it is
  deferred because it needs each round's compute, which the lane does not record, and the equal split is already a bound.)
* ADR-0152 J-1's per-bond cap applies to the sum.

### 10.4 Why colouring-side transparency, and not (c)

ADR-0105 §4 weighed (c) — choose the selected parent by `blue_work + own work` — and rejected it: it moves every
ordering in consensus that treats blue work as the heavier chain (`find_selected_parent`, the sink search, the
`pick_virtual_parents` assertion, the pruning proof, IBD, the DNS reorg gate), it must keep the mergeset in blue-work
order for the colouring loop, and it cannot be fenced because the selected parent determines the mergeset that the fence's
DAA score is read from. Giving a Round weight by changing *whose work counts in blue work or in the selected parent*
would be (c) again, with a producer-controlled number in it. (a2) takes the other path ADR-0105 took for (d):

* the **colouring** changes (one more class in a function that already has three, keyed on the candidate's own DAA score,
  fenced, with the pruning proof's build and validate carrying it through `with_level` as before);
* the **ordering does not** — raw `blue_work` gains 0 from a Round, so every comparison of blue work in consensus reads
  what it read; `find_selected_parent` and the sink are untouched, and a Round is never a selected parent (it never was);
* **weight lives where fork choice already reads PALW work** (state, `safe_weight`), so it needs no header term the
  header stage could not verify, and it is bounded by a budget in the fold rather than by a clock.

Why that is safe against the three failure shapes the first revision named: (1) a Round does not enter any normal block's
anticone count (the Heartbeat/Weighted rule), so `k = 1` cannot colour an attempt red because of it; (2) blue score and
raw blue work are untouched, so merge depth, finality and pruning are as long as before; (3) a Round is never a selected
parent, so the lane cannot become the chain, and a stale lane is a red Round tip — a verdict about a block, not a
refusal of the next parent.

### 10.5 Lane hygiene and acceptance (what §§2–9 contribute)

* A chain block names no round block as a parent past the fence **only in the sense of §2 Decision 2 when the Round is not
  canonical for it** — under (a2) a canonical Round *is* a parent-reachable vertex, and §2's separation is retained for
  the one thing it is for: **a stale or foreign Round must never reach parent selection**, which is the shared predicate of
  §10.2(7). Which of the two carriers (a Round named as a parent by a chain block inside the merge-depth window, as ADR-0125
  does, or the anchor trailer of §2 Decision 3) the implementation keeps is decided at implementation by the stale-tip
  tests; the trailer is kept in this branch because it already solves dependency and IBD enumeration without a header
  field. *(open item, §10.7)*
* The closure, window, `exec_anchored` ledger and `exec_lane` record of §§2–5 are the **ledger of canonical Rounds**:
  `(claim_id, round_index)` uniqueness (§10.2(4)) is `exec_anchored`'s key space extended with the claim.
* **Clock.** Rounds never advance the DAA and never carry the tick; lane RS's rule (one real attempt or fallback per slot)
  is untouched.

### 10.6 A later fraud verdict

Fork choice is deterministic: **a verdict never changes the weight of a block already folded.** The Rounds stay in
history; the claim's useful-work credit and its reward (attempt and rounds alike) are voided and the producer is slashed
by the ordinary void path. The attacker's gain is the fork weight its claim carried while it stood: at most
`J1_cap × ⌈challenge window ÷ span⌉` claims in flight, each worth at most `W_claim` **in total across attempt and Rounds** —
so Rounds multiply it by 1, not 120. The figures are computed from the shipped params when the fence is built (§10.7).

### 10.7 RPC, status and open items

* **RPC.** `blockKind`: `REAL` for attempts, `REAL_ROUND` for Rounds; `blockClass` stays; the explorer groups
  `REAL_ROUND` under their claim, rendered E-BLUE (canonical).
* **Built (dormant) on `rcore/exec-class`:** parent/anchor hygiene (§2), the closure, window and DoS bounds, the state
  rows at deltas 111/112 and tail `0xF1`, the RPC health and `blockClass`, the drill. **Unbuilt:** `LaneColoring::Round` and
  its `palw_lane_blue_work_v1` arm, `hangs_from_the_merging_chain` reuse for Rounds, the budget fold and `ρ`, the
  `(claim_id, round_index)` ledger row, `blockKind`, and the property tests: weight of a claim with 120 Rounds == with 1 ==
  `W_claim`; reorg determinism; Rounds never outweigh an honest chain beyond the budget; Rounds never move the DAA, the
  blue score or raw blue work; a Round never changes a normal block's colour; the attacker-gain table.
* **Open:** the carrier choice of §10.5; whether ADR-0105 §11's `merge_depth` bound is the right anchor bound for a lane
  that produces a block a second (it was derived for a block every 12 s).
* **Go/no-go for DAA 5,300: no-go.** The design moved after the freeze's inputs; this arms at the next fence.
