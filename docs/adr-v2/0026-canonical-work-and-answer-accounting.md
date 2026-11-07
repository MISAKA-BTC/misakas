# ADR-v2-0026 — 正規仕事量・PWU・推論対価・回答の二重計上防止

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 9本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md) / [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)

## 1. この文書の役割

同じ計算をどの単位で数え、どのclaimで支払うかを集約する。

## 2. 統合した判断の読み方

### 2.1

実需要のfree-prompt work、canonical attempt、forward単位の仕事を、対応するclaim・job・登録profileへ結び付ける。[旧ADR-0073](../adr/0073-real-demand-work-bears-the-weight.md) [旧ADR-0117](../adr/0117-a-draw-is-one-forward.md) [旧ADR-0144](../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md)

### 2.2

PWUや経済computeをleaf数や自己申告量から切り離し、正規導出・kernel費用・実際の制約を参照する。異なるclassの価格比較とliveness不足を分ける。[旧ADR-0131](../adr/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) [旧ADR-0132](../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) [旧ADR-0145](../adr/0145-canonical-work-is-derived-and-admission-is-earned.md) [旧ADR-0148](../adr/0148-the-free-prompt-lane-prices-compute.md) [旧ADR-0149](../adr/0149-an-attempts-pwu-is-the-derivation.md)

### 2.3

decode済みの同じ回答を二度支払わない会計案を、受理したtransaction・根拠identity・費用の一回性まで含めて扱う。[旧ADR-0205](../adr/0205-a-decoded-answer-is-not-paid-twice.md)

## 3. 維持する境界・後続改定

ADR-0205は原文でProposed／Draftである。今回の統合を回答会計全体の本番採択や、実GPU時間を証明する仕組みの完成と解釈しない。

## 4. 隣接する判断との分担

claim lifecycleは[ADR-v2-0027](0027-claim-lifecycle-and-liability.md)、fixed emission budgetは[ADR-v2-0036](0036-capacity-scaling-and-emission-budget.md)、配分の経済係数は[ADR-v2-0019](0019-model-tiers-and-annual-catalog.md)・[ADR-v2-0020](0020-class-share-and-allocation-evolution.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0073-real-demand-work-bears-the-weight.md](../adr/0073-real-demand-work-bears-the-weight.md) | L1–L331 | L21–L26 |
| [0117-a-draw-is-one-forward.md](../adr/0117-a-draw-is-one-forward.md) | L1–L260 | L15–L32 |
| [0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md](../adr/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) | L1–L190 | L17–L29 |
| [0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md](../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) | L1–L371 | L17–L28 |
| [0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md](../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) | L1–L298 | L15–L17 |
| [0145-canonical-work-is-derived-and-admission-is-earned.md](../adr/0145-canonical-work-is-derived-and-admission-is-earned.md) | L1–L308 | L15–L19 |
| [0148-the-free-prompt-lane-prices-compute.md](../adr/0148-the-free-prompt-lane-prices-compute.md) | L1–L214 | L15–L19 |
| [0149-an-attempts-pwu-is-the-derivation.md](../adr/0149-an-attempts-pwu-is-the-derivation.md) | L1–L160 | L15–L16 |
| [0205-a-decoded-answer-is-not-paid-twice.md](../adr/0205-a-decoded-answer-is-not-paid-twice.md) | L1–L30 | L3–L5 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0073-real-demand-work-bears-the-weight"></a>

<details>
<summary>0073-real-demand-work-bears-the-weight.md — 原文 L1–L331</summary>

[原文を開く](../adr/0073-real-demand-work-bears-the-weight.md) · SHA-256: `27999d1cb5fc2cdb4b0ab9df8d28d8b5e830427f7fc9408c3195763a26d46816`

<!-- ADR_SOURCE_BEGIN:0073-real-demand-work-bears-the-weight.md -->
~~~~~~~~markdown
# ADR-0073: Real-demand work bears the weight

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


> **Status amendment (2026-09-18, ADR-0137):** past `Params::palw_work_target` — testnet-11's DAA 6,001 flag day —
> a class's *share* is a **result**, not a lottery input. The class target, the class DAA, the epoch budget, the
> admission-derived share and the seat price this ADR specifies are **not read** past the fence: a block draws
> against `CCU / W` (one network-wide work target) and the readers report the rolling share of finalized work.
> Below the fence the text stands as the rule it was; see ADR-0137 §3–§5 and §22.

**Status:** PROPOSED (2026-09-02). Decision 2 is DECIDED; its activation is gated by Decisions 1 and 3.
**Builds on:** ADR-0044 (the free-prompt lane), ADR-0066 (heartbeat out of `bits`), ADR-0068 (the LLM-primary
economy), ADR-0069 (adjudicability is the price of weight), ADR-0070 (the model tiers' step spaces),
ADR-0071/0072 (the attempt lane's price and ticket).
**Amends:** ADR-0044 Decision 4's "the receipt lane is weightless" (after Decisions 1 and 3 it is not);
ADR-0069's certification scope (a family is certified for the lane it was drilled on).

> **Progress and amendments (index reconciliation, 2026-09-02).** Phase ① (a free-prompt claim
> is tried in the same court) and Phase ③ (one unit — step leaves — and the attempt drawn by the
> chain's beacon, [ADR-0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md)) are landed and
> ship from Relaunch 5d. Phase ② (the receipt lane bears weight) is DECIDED and still gated;
> Phase ④ (share / chain position for the lane) is open. Decision 6 is amended by
> [ADR-0075](0075-certification-is-a-consensus-object.md): a weightless entrant is seated by a
> `ClassLaneCertified` object, and the certified free-prompt set is chain state (genesis ∪
> chain), not the first entry in the build's `palw_rc_fp_certified_families_v1`. Map: [`README.md`](README.md).

> **Security amendment appended (2026-09-02)** — see the last section: preconditions on Phase ④ — the single-block beacon's withholding bias is bounded (`k ≥ 3` attempt blocks), 4b's supply metric counts executors not claims, and receipt weight ramps like attempts.

> **ADR-0144 alignment (2026-09-21).** Decision 2's sentence that an executor self-prompt is "exactly as good as" a canonical job is withdrawn as a PALW-reward claim. Phase ④ is no longer an operator activation of the 2026-09-05 record: it waits for ADR-0144 §6 items 2–3. See the end section.

## 1. The finding

MISAKA's purpose is a user running a local LLM on a prompt of their own. The chain has two lanes
that carry LLM work, and today the wrong one carries the chain:

* The **attempt lane** (algo 6) runs a *canonical* job nobody asked for — the job is a function of
  the block template and a nonce bucket (`palw_job_anchor_v1`). Every unit of chain weight
  (`safe_weight`, blue work through `bits`, the class share table, the epoch budgets) is earned
  here. It is priced in **step leaves**: `pwu_per_inference` is the class's
  `canonical_step_leaf_count` by rule (`verify_palw_genesis_v2` refuses anything else), and a block
  claims `expected_draws × leaves` by equality (`DerivedV1`).
* The **free-prompt lane** (a 0x4a commitment transaction, a seat panel, a beacon-drawn receipt
  block on algo 7) runs the *user's* job. It exists and it works end to end on the BASE-0 floor.
  And it is, today, a side channel:
  1. **Weightless as a block.** `algo_id_carries_no_chain_position(7)` is `true`; a receipt block
     buys neither level nor blue work, and the PoW arm returns `true` without a target because of
     that (`the_receipt_lanes_freedom_from_bits_rests_on_its_weightlessness`). A spent quantum adds
     `PWU_PER_QUANTUM = 10` to `safe_weight`, and that is all.
  2. **Priced in a foreign unit.** `cu = prompt × 1 + decode × 64`, `quanta = min(⌊cu/100⌋, 64)`,
     `pwu = quanta × 10`. Tokens, not leaves. A whole certified BASE-0 free-prompt job is worth
     70 pwu; one BASE-0 attempt is worth thousands. Nothing in the tree relates CU to a step leaf.
  3. **Verified by re-execution.** The FP-R6 seat arm runs `execute_free_prompt` on every pooled
     payload (up to 4, plus 2 on-disk) — a full inference per seat per payload, five seats per
     panel: up to 30 inferences to certify one. The attempt lane's seats hash (`verify_material`);
     they never execute.
  4. **Not adjudicable.** The state machine is ready — `CourtOpened` has no source filter,
     `CourtClosed { ExecutorGuilty }` slashes an FP claim's `reserved` through `void_and_slash`
     — and no party can assemble a close, for four concrete reasons (§4). The shipped challenger
     re-executes the *anchor* job for an FP claim and therefore opens a court against every honest
     one it sees.
  5. **Uncertified.** ADR-0069's certificate is minted from a drill over `job_for_anchor` +
     `execute`; `execute_free_prompt` appears in no drill. The lane the certificate is about is
     the lane nobody asked for.

The consequence is that 100 % of the chain's weight is synthetic work, and the work the network
exists to do is paid an ordinary coinbase for a block that weighs nothing. ADR-0068 named the LLM
the primary economy; this ADR names *real demand* the primary lane of that economy, and lays out
the order in which that can be made true without re-opening any of the holes the attempt lane has
already closed.

**The order is not negotiable.** Weight without adjudicability is ADR-0069's original sin
(97.8 % of weight once went to a class the code said could never be convicted). Adjudicability
without a shared price unit gives the free-prompt lane weight the attempt lane's retarget cannot
read. So: court first, price second, weight third, share last.

## 2. Decisions

**Decision 1 — Phase ①: a free-prompt claim is disputed in the same court as an attempt, and a
seat's duty is to check, not to re-run.**

1a. *One capture codec.* An FP run already computes the court-usable capture — the family
material tuple `(binding, tiles, logits_rows, generated, checkpoint_chunks)` — into
`outcome.material`, and drops it. It is persisted and broadcast under the same data-availability
obligation as an attempt's (`trace_manifest_root` / `trace_chunk_count` / `trace_retention_daa`
already sit on the FP commitment for exactly this). The job-description material `FPM1`
(`PalwFpMaterialV1 { job, prompt_token_ids }`) stays as what it is: the question, not the answer.

1b. *The FP claim's anchor is its job id.* For an attempt the panel derives the job from the
header (`palw_job_anchor_v1` over template, class, bond, nonce bucket) so the accused cannot set
the question. For an FP claim the question is set by the *user* and fixed on chain before any
verifier looks: `fp_job_id_v3(job)` is what the binding's `job_context.job_id` carries, and the
0x4a payload carries the prompt ids hash-bound to it. So `PalwClaimRootsV1.anchor` for an FP claim
is the job id, and `verify_material`'s `job_id == anchor` equality holds for the right capture
and fails for every other, exactly as on the attempt lane. `PalwCourtDutyV2` learns the lane
(`free_prompt`, as `PalwSeatDutyV2` already does) so the panel's court arm can choose.

1c. *The prompt reaches the refutation.* `PalwExecutionStepRefutationV1::prompt_token_ids` is
"irreducible" (its own doc) and the court refuses a non-empty list that does not hash to the
binding's `prompt_token_ids_hash`. Today every backend derives the list from the anchor: A16 and
Qwen3.6 fall back to empty (prefill `Embed` leaves are `Unadjudicable`), BASE-0 passes the wrong
list (every close is `InputSetNotCanonical`). The prover takes the ids as an input for an FP
claim — from `FPM1` or the accepted 0x4a payload, both hash-bound — and the attempt path is
untouched.

1d. *The challenger disputes the job that was claimed.* For an FP claim the challenger's own
execution is `execute_free_prompt(job, prompt)` from the hash-bound ids, never `job_for_anchor`.
A challenger that cannot execute the FP path (Qwen3.6 today) opens nothing.

1e. *Seats check; challengers replay.* FP-R6 becomes what the attempt lane's seat arm is:
`verify_material` over the served capture (hashing), plus **k sampled leaves** — the seat picks k
leaf indices with its own randomness, asks the prover for each (`refutation_for_index` over the
served capture, which resumes from the served checkpoint leg and replays at most one checkpoint
interval), and runs the court's own `check_execution_step_refutation_v1` locally. `NoFaultFound`
on every sample and a price that re-derives (1f) is `Valid`. A skipped inference is wrong at every
leaf and is caught by any sample; a single corrupted leaf is the court's question, and the court
is cheap (bisect to one leaf, one arithmetic refutation — ADR-0070). Full re-execution is what a
*challenger* chooses to spend to open a court, never a seat's obligation.

1f. *Certification covers the lane.* ADR-0069's certificate gains an FP drill: a family is
FP-certified only when an `execute_free_prompt` run's every leaf adjudicates and a tampered leaf
convicts under the shipped court. `palw_rc_certified_families_v1` records the lane a family is
certified for; Decision 2 reads the FP bit, and nothing else does.

**Decision 2 — Phase ②: real-demand work bears weight. DECIDED.**

The question was whether free-prompt work should carry chain weight at all. It should: it is the
work the network exists to do, and after Decision 1 it is *more* accountable than an attempt, not
less — its prompt is on chain, its capture is served, its leaves adjudicate in the same court, and
its ticket is drawn by a beacon that does not exist when the claim is fixed (nothing to grind,
cf. the ADR-0072 review).

自己生成 prompt を canonical job と同じ PALW-reward 対象とする旧主張は撤回した。誰も必要としない出力の計算は、ユーザーの有用なローカル推論という目的を満たさないため。[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P1–P3 に従う。

Activation also requires ADR-0144 §6 items 2–3; the following technical conditions alone are insufficient. A class's FP lane bears weight when (i) Decision 1 has landed and
the class is FP-certified (1f), and (ii) Decision 3 has landed so the weight is in the unit the
retarget and the share table already read. Until both hold, a spent quantum keeps adding exactly
what it adds today.

**Decision 3 — Phase ③: one unit, the step leaf, on both lanes.**

3a. An FP claim's `pwu` is the job's **step-leaf count** — a pure function of the class profile
and `(prompt_tokens, decode_tokens_executed)`, derived the way `canonical_step_leaf_count` is for
the canonical job — and it is checked by *equality* at commitment admission, the `DerivedV1`
discipline. The binding's leaf count is the same number, so a capture whose leaves do not match
the claim is refused structurally.

3b. A quantum is a leaf count, not a CU count. *As landed (ADR-0074 Decision 5):* not a
network-wide `QUANTUM_LEAVES` but a fraction of the class's own canonical job —
`max(1, canonical_leaves / 8)` — so every class's job is eight draws and `pwu` stays in leaves. `PalwFpCuWeightsV3`, `QUANTUM_CU`, `PWU_PER_QUANTUM` are withdrawn
from the bundle (ADR-0044 retains the withdrawal reason). The "no shape prices above the
pure-decode reference" invariant becomes a theorem instead of a calibration: leaves are the work.

3c. The lottery discipline, stated once for both lanes: **a draw is a paid execution, drawn from a
value the executor cannot vary after paying.** The receipt lane's beacon draw satisfies it by
construction. The attempt lane's ADR-0072 draw satisfies it only when every field inside the
priced bytes is an equality against chain state, a function of the execution the panel replays,
or the position — the review of ADR-0072 found `trace_retention_daa` (and the DA trio generally)
pinned by nothing, which made one inference worth 2^64 draws. That is ADR-0072's to close; this
ADR records the invariant as the bar both lanes are held to, and notes that a beacon-drawn
attempt lane would satisfy it structurally rather than field by field.

**Decision 4 — Phase ④: the weight and the share move to the lane that does the work.**

4a. *A receipt block has a chain position.* `algo_id_carries_no_chain_position(7)` flips to
`false` at activation. Its blue work is `calc_work(bits)` like every other block's, with
`bits = compact(receipt_target[class])` — the class's receipt-lane target, which the stateful
admission already compares the quantum ticket against — so the header's declared work is checked
against state at admission (the way an attempt's `pwu` is), and the work has the same statistical
meaning as the attempt lane's: expected paid executions per block. The PoW arm stops skipping the
target comparison; the test that pinned "freedom from `bits` rests on weightlessness" is replaced
by "chain position rests on the lottery being consensus-checked at admission". ADR-0066's
predicate is honoured: a lane has weight iff its blocks are priced.

4b. *The share follows supply.* `fp_attempt_share_permille` leaves 900 on a per-class schedule
toward a receipt-lane majority, bounded by what the lane actually produces: the silent-lane
renormalisation (`apply_class_retargets`) stays as the liveness floor, so a class with no FP
supply retargets its attempt lane alone, and the attempt lane becomes what ADR-0068's floor
doctrine already says it is — the fallback that keeps the DAA moving when nobody is asking.

4c. *Payment.* A receipt block's coinbase remains the executor's payment; the worker carve stays
attempt-only. What Decision 4 changes is what the block *weighs*, not what it pays.

**Decision 5 — Rollout.** Phase ① moves the fingerprint once, through the certificate root (1f);
its node-side parts (1a–1e) ship ahead of it and are inert on an uncertified lane. Phase ③ and
Phase ④ are each one fingerprint move; ② is a bit inside ③'s. No re-genesis at any step.

## 3. What this costs, stated before it is measured

* **Seat cost per FP claim** falls from ≤ 30 inferences to hashing plus `k` checkpoint-interval
  replays. With the RC checkpoint interval and `k = 4`, a Qwen-class claim verifies in well under
  one inference per seat; the floor in milliseconds. This is what makes a receipt-lane majority
  affordable at all — at 30 inferences per certified inference the lane can never carry more than
  1/30 of the network's compute as weight.
* **Detection.** A claim with no execution behind it fails every sampled leaf; a claim with one
  corrupted leaf survives sampling with probability `1 − k/n` and is the challenger's to catch. The
  executor gains nothing from a single corrupted leaf (the work was still done) and loses
  `reserved` if caught, so the sampling rate prices the *skip*, which is the only profitable lie.
* **Court cost** is ADR-0070's: `log₂(leaves)` rungs plus one arithmetic refutation, bounded by
  the 80 KiB close ceiling — unchanged by this ADR, now reachable from the FP lane.
* **Weight.** After ④a a receipt block weighs `1/receipt_target` expected executions, like an
  attempt weighs `1/(P_class × P_bits)`. The chain's weight becomes a function of paid executions
  regardless of who asked for them — which is the sentence ADR-0072 ended on, made true of the
  lane it excluded.

## 4. Phase ① work items (the four gaps, with their closures)

| # | Gap (as found 2026-09-02) | Closure |
|---|---|---|
| G1 | The prover derives the prompt from the anchor: A16/Qwen3.6 → empty list → prefill `Embed` leaves `Unadjudicable`; BASE-0 → foreign list → `InputSetNotCanonical` on every close | `refutation_for_index` takes the FP prompt ids as an input on the FP path (hash-checked against the binding); attempt path unchanged |
| G2 | The court capture (family tuple) is computed by `execute_free_prompt` into `outcome.material` and never persisted or broadcast; only `FPM1` travels | The FP worker retains and gossips the tuple under the claim id, beside `FPM1`; the pool/retention readers accept both by magic |
| G3 | `PalwCourtDutyV2` has no lane; the court arm derives an attempt anchor and `verify_material` refuses on `job_id != anchor` | `free_prompt` on the court duty; anchor := `fp_job_id_v3(job)` from `FPM1` for an FP claim |
| G4 | The challenger re-executes the anchor job for an FP claim → opens a court against every honest FP claim, staking `reserved` | Challenger executes the FP job from the hash-bound ids; opens nothing when the backend has no FP path |
| G5 | FP-R6 seats re-execute (≤ 30 inferences per claim) | `verify_material` + `k` sampled leaves through the court's own refutation check; re-execution remains as the challenger's spend |
| G6 | No drill certifies the FP path | `certify_e2e_family_v1` accepts an FP-drill evidence set; the derived set records the lane |

Known and not in scope here: the registered A16 row refuses `execute_free_prompt` until its graph
is reconciled (ADR-0049 Decision F gap), and Qwen3.6 has no FP path; both are prerequisites for
those classes' FP lanes to be *certified*, not for the court to exist. Two stale comments to
retire when the files are touched: `palw_state_v2.rs` (the "FP worker captures no legs" note above
the `UnadjudicableCommitment` refusal) and `palw_fp_devnet_v3.rs` ("court 2400"; the value is 3000).

## 5. Invariants the tests must hold

* An FP claim built by the shipped worker on the floor class is convicted by the shipped court
  when one leaf of its capture is tampered, and every leaf of an honest one adjudicates
  `NoFaultFound` — the FP twin of ADR-0069's drill.
* A seat's FP verdict never calls `execute_free_prompt`.
* A challenger never opens a court against a claim whose capture reproduces from the claim's own
  job.
* `pwu` on an FP claim equals the capture's leaf count, and admission refuses one unit off in
  either direction (Decision 3).
* A receipt block's `bits` equals `compact(receipt_target[class])` at admission, and its blue work
  is `calc_work(bits)` (Decision 4) — with the PoW arm comparing the digest to that target.
* A class with no FP supply retargets exactly as it does today (Decision 4b, the liveness floor).

## 6. Supersession

| Decision | Status after this ADR |
|---|---|
| ADR-0044 Decision 4 — receipts are weightless, `algo_id_carries_no_chain_position(7)` | stands through Phases ①–③; flips at Phase ④ activation (Decision 4a) |
| ADR-0044 Decision 7 — CU weights price the lane | withdrawn at Phase ③ (Decision 3b); ADR-0044 retains the reason |
| ADR-0066 — a lane has weight iff its blocks are priced | honoured: receipt blocks become priced before they weigh |
| ADR-0068 — the LLM-primary economy, the floor's minimum | refined: real demand is the primary lane; the attempt lane is the floor doctrine's fallback |
| ADR-0069 — a certified family may bear weight | extended: certification names the lane; the FP bit gates Decision 2 |
| ADR-0072 §3 "does not make real-demand work the primary lane" | this is the ADR it deferred to |

## Security amendment (2026-09-02) — preconditions on Phase ④ (Decision 4) and on 4b's supply metric

**SA-1 — The beacon's single-block bias is bounded before receipt blocks gain chain position.**
Today the beacon is the first attempt-class chain block at or after the slot
(`derive_beacon_fact_v3`); re-rolling costs one inference, but *withholding* costs only that
block's subsidy: a producer whose block would be the beacon can drop it when the draw is
unfavourable to its own pending claims. The quantized ticket (ADR-0044 Decision 5) bounds the gain
per bit; Decision 4 multiplies the stake on that bit by giving receipt blocks position and share.
Precondition: the beacon becomes the fold of the first `k ≥ 3` attempt blocks at or after the slot,
so a producer with attempt share `p` controls the draw with probability `pᵏ` instead of `p`; the
bound is stated here with `p` read from the live share table and pinned by a test on the fork-choice
simulator. Receipt blocks remain non-beacons (F15).

> **SA-1 as landed (2026-09-02).** `derive_beacon_fact_v3` / `derive_beacon_fact_to_genesis_v3` take
> a `fold_k`, and past the fence the fact's `beacon_block` is `fp_beacon_fold_v3` of the first `k`
> attempt blocks at or after the slot (ascending chain order, domain-separated, count-prefixed) with
> `beacon_daa` the **k-th** — the height at which the draw becomes determined, so the use window
> cannot open against a fold that does not exist yet. `prev_attempt_daa` is unchanged, so "the fold
> begins at the slot" stays checkable by a non-walker. Fewer than `k` attempt blocks above the slot
> is `NoBeaconYet { needed, found }`, never a shorter fold. `k = 1` is the pre-SA-1 rule and the
> IDENTITY rather than a one-element digest, which is what keeps the fence-off bytes.
> **Fenced** as `Params::palw_beacon_fold: Option<PalwBeaconFoldV1 { activation, k }>` — top level,
> `None` on every shipped preset, activation-only in `for_each_fence`, both halves in
> `consensus_params_id` and `consensus_schedule_id`, resolved at the DRAW'S OWN SLOT so producer and
> validator cannot pick different widths for one claim. `validate_palw_v2` refuses an armed `k < 3`.
> `k` belongs in `PalwFreePromptParamsV3` (inside `palw_ruleset_id_v2`, where the V2 handshake would
> compare it) at the re-mint that arms the fence; until then it carries ADR-0065 D1's
> coordinated-change hazard, recorded on `PalwBeaconFoldV1`.

**SA-2 — 4b's supply metric counts executors, not claims.** "Bounded by what the lane produces"
must not be movable by one operator flooding `Canonical` self-claims — honest work, but self-dealt.
Count distinct executor bonds with a certified claim in the span, and cap one bond's contribution
at `1/seat_count` of the metric; otherwise the share walks toward the lane its largest operator
dominates.

**SA-3 — A receipt block's chain position is priced by its class's receipt target under the class
retarget's `max_factor` (4 per epoch), and receipt weight ramps exactly as attempts do**
(Provisional → ReceiptLicensed → Final); a receipt block whose claim voids is weightless
retroactively only inside the ramp window and never after `Final` (ADR-0039 3e).

## Record (2026-09-05)

Phases ①–③ are code; Phase ④ (Decision 4 — receipt blocks gain position and share) is an
ACTIVATION gated by this ADR's security amendment (the beacon's `k ≥ 3` fold, the supply metric's
`1/seat_count` floor). No code item is open; the activation is the operator's, on a measurement.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) is the constitution this
ADR's purpose already named. The 2026-09-05 record above stands as history. What it may no longer
schedule:

* **Decision 2, the self-dealing paragraph.** "An executor who submits prompts to itself … is
  exactly as good as" a canonical job is withdrawn as a PALW-reward claim. A self-prompt nobody
  asked for is Attempt by another name. ADR-0144 P2/P3 and §6 item 5 require that synthetic work
  shrink, not that it be treated as interchangeable with the user's own inference. Shape-crafting
  remains the hole Decision 3 closed; that half is untouched.
* **Phase ④ as an operator activation.** Decision 4 (receipt chain position, share walking toward
  the receipt lane, payment unchanged) is a widening of the economy. ADR-0144 §6 item 0 forbids
  that until items 2 and 3 land (representation-neutral accounting, earned admission). The 2026-09-02
  security preconditions (SA-1…SA-3) still bind if and when Phase ④ is built; they are no longer
  sufficient by themselves. ADR-0077 Decision 15's "one retarget span of measured receipt supply"
  is the same schedule, equally withheld.
* **What stays.** Phases ① and ③, and Decision 2's *question* (real-demand work should bear
  weight), remain the load-bearing path. The destination is still that the inference the user ran
  is the inference the chain rewards. Phase ④ is how that destination gets chain position — after
  the accounting can serve it, not before.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0073-real-demand-work-bears-the-weight.md -->

</details>

<a id="source-0117-a-draw-is-one-forward"></a>

<details>
<summary>0117-a-draw-is-one-forward.md — 原文 L1–L260</summary>

[原文を開く](../adr/0117-a-draw-is-one-forward.md) · SHA-256: `77973c5070da5c094291583c58b887549a2706e457f95c12008102b5ae1d8c45`

<!-- ADR_SOURCE_BEGIN:0117-a-draw-is-one-forward.md -->
~~~~~~~~markdown
# ADR-0117 — A draw is one forward

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED and IMPLEMENTED 2026-09-11 on `feat/adr-0103-held-context`, at the operator's
  instruction. The same day ADR-0112 §8 had recorded the one-forward ticket as not taken; the
  operator reversed that and asked for the change ("1 回の forward で抽選する … 実装を行う方針に変更する").
  Decision 2 (the engine) is consensus-neutral and in force wherever this build runs. Decision 1
  (the job) is behind `Params::palw_prefill_draw`, `None` on every shipped preset, so no fingerprint
  moves until a network arms it.
* **Scheduled on testnet-11 at DAA 4,000** (operator, 2026-09-11), on the pre-mainnet audit's flag day
  (`PALW_RC_AUDIT_FENCE_DAA`), so one rollout carries both; t11's fingerprint moves `09efd285…` →
  `4300409b…`. The shared height has one consequence the fork-id gate cannot see: a build carrying
  the audit fence without this one advertises the same fork id and parts silently at 4,000 (the
  constant's doc). Held context (ADR-0103) was asked for the same height and given its own flag day
  instead — its prerequisites on testnet-11 (signature contexts, Merkle prompt ids, the shard court)
  are not armable yet.
* Builds on: [0072](0072-the-ticket-is-the-execution.md) (the ticket is a function of the
  execution), [0084](0084-the-ids-ride-the-capture-stays-home.md) (Decision 7: the seat's verdict by
  execution), [0112](0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md)
  (the weights are read within a budget, and a draw's cost is what it reads).
* Reverses: ADR-0112 §8's "Decided 2026-09-11: not taken."

## 0. The sentence this ADR is

**A draw is decided by one pass over its prompt. The prompt is the one the anchor has always
derived, at the class's canonical prefill length, and the engine computes all of it one layer at a
time, so every weight is read once per draw. Past `Params::palw_prefill_draw` the draw's job has no
decode call: its one generated token is chosen from the last prefill position's logits, and the
ticket — a function of an attempt whose trace root commits that row and that token — is decided by
that pass. The class, its id, its certification, its price and the free-prompt lane are unchanged.
A seat holding a producer's material checks the whole job the block asked for, not only its id.**

## 1. What was known, and what was found writing it

* **The job.** Qwen3.6's canonical job is `(7, 2)`: seven prompt positions and two generated tokens.
  The first token comes from the last prefill position, so the committed path runs 7 + 1 = 8 forward
  passes (the step space's `P + D − 1`; only the old path with no compiled plan ran 9, and ADR-0112 §1
  and §8 said nine). Each forward was one position through every layer, so a draw read the weights
  eight times.
* **The order is not the protocol.** A layer's walk reads and writes only that layer's cache, and a
  position's walk through layer `l` reads only its own row out of layer `l − 1` and the positions
  before it in layer `l`. So the prompt's positions can be computed a layer at a time with every
  committed row bit-identical, and the weights of a layer — its tensors and the union of the experts
  its positions route to — read once for the whole prompt. That is the operator's "1 回の forward",
  and it needs no rule.
* **The decode call is a rule.** A generated token depends on the one before it, so the second token
  is a second pass over the weights whatever the engine does. Removing it changes the job.
* **The job is not in the class id.** `class_id = shape_profile_id()` hashes the graph profile alone
  (the virtual processor's own comment: "The canonical job is NOT covered by the class id"), so
  changing the draw's job needs no re-mint. ADR-0112 §8 said otherwise; it was wrong.
* **The job is enforced by seats, not at acceptance.** Nothing in block acceptance checks that the
  execution was the job the anchor names. Seats check it: a seat with no material replays the job and
  compares roots (ADR-0084 Decision 7); a seat holding the material verified it against the claim's
  roots and the anchor — and **only the job's id**. A producer could therefore answer the right
  question with a smaller job, skipping its decode calls or running a prompt of another length under
  the same id, and every seat that held its material vouched for it; only the replaying seats
  refused. That gap was found writing Decision 3 and is closed by it.
* **One token is not a prompt.** A one-token prompt maps the anchor to one of the vocabulary's
  248,320 ids, and the model's output for one token at position 0 is a fixed function of that id.
  Every draw would be a lookup into a table computed once, and the lottery would price nothing. The
  prompt stays at the class's canonical prefill: seven ids for Qwen3.6, 248,320^7 prompts.

## 2. The requirement

A draw costs one pass over the class's weights. The ticket stays a function of an execution a court
can adjudicate, position by position. Producers and seats agree on the job at every height, and a
seat that holds a material cannot vouch for a job its block did not ask for.

## 3. Decisions

**Decision 1 — past the fence, the attempt's job is the canonical job without its decode calls.**
`palw_attempt_v2::palw_attempt_job_v1(canonical, prefill_draw)` sets `exact_decode_tokens = 1` when
`Params::palw_prefill_draw_active_at` holds at the attempt's own block's DAA score, and changes
nothing else: the anchor, the prompt, the prefill and every identity field are the canonical job's.
The producer that runs the job (`produce_one`) and every seat path that replays or judges it (the
panel's `attempt_job_for_claim`: the challenger's re-execution, the court challenger's, and the
verdict by execution) call that one function at that one height. It is a fence because seats are
what enforce the job: a seat on the old rule replays the two-token job, finds a root it did not
compute, and voids an honest claim, so the network must switch at one height. A bare fence, `None`
on every preset, Some-only in the fingerprint, the schedule id and the fork id. Arming it is a flag
day like any other.

What does not move:

* **the class** — its id, its canonical job, its certification drill and its registration. A
  prefill-only job is a prefix of the canonical job's step space, which every court already
  adjudicates, and the drill that certified the class covered it;
* **the free-prompt lane** — an answer is decode tokens by definition, and it never calls the
  function;
* **the ticket's derivation** — `class_ticket_v3` over `execution_commitment_v3`. It is still the
  execution: the trace root commits the last prefill position's logits row and the token chosen from
  it, and the execution root commits every node's output at every prompt position.

**Decision 2 — the prefill is one pass over the weights.** `Qwen36Engine::forward_prefill_planned`
computes every prompt position through a layer before the next layer is read. The post table runs at
the last position only, the one whose rows the step space commits. The capture loop takes it whenever
the class wants no checkpoint inside the prefill (the shipped hybrid never does). A class whose
cadence wants one needs the cache as it stands after each position, which a layer-major pass never
holds, so it keeps the stepped pass. The rows, the last logits and the cache left behind are the
stepped pass's bits, so this is in force everywhere, fence or not: before the fence it takes Qwen3.6's
draw from eight passes over the weights to two (the prefill, then the decode call), and past it to
one. Under ADR-0112's budget the same order admits a layer's routed union while that layer runs. With
seven positions against forty layers, that union — at most 56 experts — sits inside even the floor's
320, so each expert a draw needs is read once.

**Decision 3 — a held material answers the whole job its block asked for.** `PalwClaimRootsV1` carries
`attempt_draw: Option<bool>`. It is `Some(prefill_draw)` for an attempt claim, read at the claim's
block's height, and `None` for a free-prompt claim, whose anchor is its job's own id, or for a caller
with no block. With `Some`, every family's `verify_material` (the floor, the dense tier, the hybrid)
requires the material's job context to equal `palw_attempt_job_v1(job_for_anchor(anchor),
prefill_draw)`, field for field, and says `Mismatch` otherwise. Before the fence that is the canonical
job, and the skipped-decode job the id check let through is refused. Past it, the canonical job is
the one refused. The legacy composite paths recompute under the same job.

**Decision 4 — the price stays the canonical job's.** `pwu_per_inference` is the class's canonical
step-leaf count, and item 6 of admission still derives an attempt's pwu from it. A prefill-only draw
executes the canonical job's prefill, so past the fence one pwu buys a fixed fraction of the leaves
it bought before — per class, the prefill share of the canonical job. Making the price the draw's own
leaf count needs each class's profile at admission. The genesis classes registered with no carriage
have only the catalog's canonical count on chain, and adding a per-class draw count to the ruleset
bundle would move every V2 network's fingerprint while the fence is dormant. Every pwu of one class
past the fence is the same fraction, and a faster engine already moves pwu per unit of compute the
same way (Decision 2 does, with no fence). So the price is unchanged, and the per-class fractions are
pinned by a test (§5 I-6) so they are a stated number rather than a surprise:

| shipped class | canonical job | leaves a one-forward draw executes | of the price |
|---|---|---|---|
| the floor | (8, 4) | 5,560 of 7,708 | 72.1 % |
| PALW-QWEN36 graph-v3 | (7, 2) | 2,326,264 of 2,685,360 | 86.6 % |
| the dense graph-v5@512 | (63, 2) | 6,508,520 of 6,630,544 | 98.1 % |

Today one pwu buys one leaf on every class. Past the fence it buys 0.72 of a leaf on the floor,
0.87 on the hybrid and 0.98 on the dense row. Measured against the dense row, the floor's weight per
leaf executed therefore rises by about 36 % and the hybrid's by about 13 %. That is the relative
weight this decision leaves standing. Re-pricing belongs to the next re-mint, which can carry the draw's count in the catalog.

## 4. What it costs, and what it buys

| Qwen3.6, per draw | before | Decision 2 only | Decisions 1 and 2 |
|---|---|---|---|
| passes over the weights | 8 | 2 | **1** |
| the always-set read (1.86 GiB), on a host with no residency | up to 8× | 2× | **1×** |
| routed experts read, under ADR-0112's budget | each token's, less the LRU's hits | the prompt's union once, plus the decode token's | **the prompt's union, once** |
| compute | 8 forwards | 8 forwards, fewer post tables | **7 positions, one post table** |

The post table is the unembedding, a 248,320 × 2,048 projection. The stepped prefill computed it at
every prompt position and discarded all but the last, and the one-pass prefill computes it once.

What it does not buy: on a host that cannot spare the floor (ADR-0112 §10.5), weights are still read
through the page cache's faults at 11 MB/s. The draw reads about 1.86 GiB of always-set plus the
prompt's experts once, instead of eight times, and it is still minutes. On such a host the residency
budget is the fix, and §8 of ADR-0112 names the node's own working set as what stands in its way.

## 5. Invariants the tests hold

1. **I-1, the one-pass prefill is the stepped one.** On the v2 and fused v5 graphs, from a fresh cache
   and from a prefix, every committed row, the last logits and the cache left behind are the same
   bits (`the_one_pass_prefill_is_the_position_by_position_one`).
2. **I-2, under a budget it is the owned store's rows and reads no more.** At the floor, the one-pass
   rows equal the owned store's, and its misses and bytes read are at most the stepped pass's
   (`the_one_pass_prefill_under_a_budget_is_the_owned_rows_and_reads_no_more`).
3. **I-3, the one-forward job is the canonical job without its decode calls.** It is idempotent, a
   different execution, and identical before the fence
   (`the_one_forward_job_is_the_canonical_job_without_its_decode_calls`).
4. **I-4, the fence is dormant everywhere**, named when armed, exact at its height, and absent
   outside `ConsensusV2` (`the_prefill_draw_fence_is_dormant_and_named_when_armed`).
5. **I-5, a held material answers the whole job.** Before the fence the canonical material matches
   and a skipped-decode material, or a longer prompt under the same id, is `Mismatch`; past it, the
   other way round; with no block the id check stands
   (`a_held_material_answers_the_whole_job_its_block_asked_for`; on the hybrid,
   `the_one_forward_draw_is_the_canonical_prompt_in_one_pass`, where the one-forward run's single
   token is the canonical run's first).
6. **I-6, the price's fraction is pinned per shipped class**
   (`the_one_forward_draw_is_priced_as_the_canonical_job_and_the_fraction_is_pinned`).

## 6. Supersession

| what | by |
|---|---|
| ADR-0112 §8: "Decided 2026-09-11: not taken" (the single-forward ticket) | Decisions 1 and 2 |
| ADR-0112 §8's claim that the job is in the class id, and §1's "nine forward passes" | §1 above: eight, and not in the id |
| every family's `verify_material` checking only the material's job id | Decision 3 |

## 7. What is deliberately not decided

* **The arming height.** A flag day, announced like DAA 3,500's, is the operator's call.
  testnet-11's schedule is unchanged by this ADR.
* **Re-pricing.** Decision 4's fraction stands until a re-mint can carry the draw's own count.
* ~~**The dense tier's one-pass prefill.**~~ **Built 2026-09-12** (§9.1): the dense engine walks
  the registered plan a layer at a time over runs of positions, and the capture loop and the
  interval replays take it.

## 8. Number hygiene

0117, beside 0116 (the same instruction's other half). **The next free number is 0118.**

## 9. Implementation record (2026-09-11)

| Decision | where | what pins it |
|---|---|---|
| **1** the job | `Params::palw_prefill_draw` (field, fence accessors, schedule id, fingerprint, fork-id probe); `palw_attempt_v2::palw_attempt_job_v1`; `palw_producer::produce_one`; `palw_panel::attempt_job_for_claim` and its three callers | I-3, I-4 |
| **2** one pass | `Qwen36Engine::forward_prefill_planned`; `qwen36_execute_streaming_v1` | I-1, I-2 |
| **3** the whole job | `PalwClaimRootsV1::attempt_draw`; `verify_material` in `backend.rs`, `qwen25_a16_backend.rs`, `qwen36_backend.rs`; `palw_panel::attempt_draw_for_claim` | I-5 |
| **4** the price | — (unchanged) | I-6 |

The suites on `feat/adr-0103-held-context` after the change: consensus core 2,123 passed (library
and integration), `misaka-palw-base0` 432, the SDK 25, `kaspad`'s library 87; `misaka-cli` and
`misaka-palw-job-replay` compile against the new `PalwClaimRootsV1`.

### 9.1 The dense tier's one pass (2026-09-12, at the operator's instruction)

`A16Engine::forward_prefill_planned` walks the registered plan a layer at a time over a run of
positions. Each projection runs once over the run — `kernels::a16_matmul_requant_batch`, the
weight row read once for every position, which the kernel tests hold bit-identical to the
single-row projection — with the sink position alone on its own parameters (ADR-0050). The fused
attention site reads one concatenation of the layer's history and hands each position its prefix,
where the stepped walk copied the whole history for every position. Every other node is the
stepped walk's own evaluation (`eval_node`, extracted from `walk_table` so the two walks are one
computation), on the pool one position a task. A cache write appends the run's rows in position
order and a read sees the series that ends at its own row; a plan whose layer reads the cache before
writing it keeps the stepped walk (`one_pass_prefill_supported`).

It runs where a prefill runs: the dense capture loop — the producer's draw (Decision 1's job) and
its free-prompt capture, in runs of `A16_PREFILL_RUN_POSITIONS` = 64, a checkpoint after a prefill
position taken after its run from the rows it reads by index — and, through
`Base0FpReplayForwardV1`, the interval replays a seat and an executor run (a held interval of
prompt positions, a shipped class's interval 0). The legacy v1 class, which commits every prompt
position's logits in its composite root, keeps the stepped walk.

| | stepped | runs of 32 | runs of 64 |
|---|---|---|---|
| 256 positions | 8.64 s (33.8 ms a position) | 3.94 s (15.4 ms), 2.20× | — |
| 508 positions (the 512 row's canonical prefill) | 18.30 s (36.0 ms) | 8.28 s (16.3 ms), 2.24× | 7.47 s (14.7 ms), 2.45× |

The real 1.5B artifact, testnet-11's graph-v5 row, one 12-core M-series host, the prompt the
env-gated `one_pass_prefill_on_the_real_dense_row` probe builds; every committed row, the cache and
the last position's logits the same bits in every run. Pinned on the fixture by
`the_one_pass_prefill_is_the_position_by_position_one` (the v2, v5 and v7 plans, the fast and
catalog engines, from the sink and from a stepped prefix, runs of every width), and end to end by
the context vectors, whose documents did not move. The 32,768-position vector's stages, the same
host, before and after (with ADR-0121's streamed replays beside it): produce 32.6 → 7.7 s, seat
91.6 → 34.7 s, court 180.7 → 44.2 s, availability 11.5 → 2.1 s — 318.6 s to 91.4 s, the document
`0b8b191d…` both times.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0117-a-draw-is-one-forward.md -->

</details>

<a id="source-0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves"></a>

<details>
<summary>0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md — 原文 L1–L190</summary>

[原文を開く](../adr/0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) · SHA-256: `6fcde9b4e87254ef3e430f0262bbe127f238fcbf668374d227f9e8d25d78e1be`

<!-- ADR_SOURCE_BEGIN:0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md -->
~~~~~~~~markdown
# ADR-0131 — A claim is paid for the compute it cost, in economic compute, not leaves

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **PROPOSED 2026-09-17; Decisions 1 and 2 IMPLEMENTED the same day, in shadow** on
  `feat/palw-exec-lane-and-validator-retirement` (§7). No consensus rule, parameter or fingerprint
  moves: everything here is node-local measurement until Decision 3 gets a height.
* **ADR-0144 alignment (2026-09-21).** Decisions 3–6 do not arm a scalar CCU until ADR-0146. See the end section.
* Operator's direction, in the operator's words: "次に見るべきは claim数ではなく `BILI / canonical compute`";
  "PWU = fork choice / consensus work、CCU = model間の経済価格 を分離"; "M4で何秒だったかを直接consensus値に
  しない — 時間は校正用データに留めます"; "DAA補正を二重に掛けないこと"; "Producer CCUとPanel CCUを分離";
  "45% domain capは報酬計算から切り離す"; "新モデル追加時に、shadow期間を必須にする"; "ここからモデル間の歪みを
  なくす方針で進める".
* Builds on: [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  Decision 6 (the work price this replaces), [0039](0039-palw-only-block-production.md) and
  [0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) (per-class targets), [0117](0117-a-draw-is-one-forward.md) (a draw is one
  forward), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (the shadow-first discipline).

## 0. The sentence this ADR is

**A claim's pay is the network's rate times the economic compute the claim cost — the expected draw forwards
at its class's target plus its canonical job, each priced by a versioned, deterministic cost of the kernels it
runs — while fork choice keeps its leaves; and no rate, class or unit changes pay until the node has measured,
in the open, how many BILI each class earns per unit of compute it finalized and per unit it attempted.**

## 1. What testnet-11 pays today (read from its genesis objects and live node, 2026-09-17)

| class | canonical job | `pwu_per_inference` (leaves) | initial target | price at 6,001 |
|---|---|---|---|---|
| `PALW-BASE-0/rc` (floor) | 8 + 4 tokens, n_ctx 12 | 7,708 | MAX / 12,665 | unpriced (100 %) |
| `Qwen3.6-35B-A3B/graph-v3` | 7 + 2 tokens, n_ctx 8 | 2,685,360 | 0.6115 × MAX | 29.8 % |
| `Qwen/Qwen2.5-1.5B/graph-v5@512` | 63 + 2 tokens, n_ctx 512 | 6,630,544 | MAX (every forward wins) | 73.7 % |
| `Qwen/Qwen3.8-27B/graph-v3` (DAA 1,165, 1 ‰) | 7 + 2 tokens | 9,000,776 | — | 100 % (the unit) |

* **The basis is a static leaf count, not the claim's pwu.** `work_priced_escrow` prices by
  `palw_exposure_pwu_v1(class, claim.pwu)`, which for a `DerivedV1` class is `pwu_per_inference`. `claim.pwu`
  (the class target's expected attempts times the per-inference leaves) is fork-choice work and is not read.
  So there is no double count today; the expected-attempts factor is simply missing: a class drawn at
  0.61 × MAX pays ~1.64 forwards a claim and is paid as if it paid one.
* **Leaves are not compute, and the two model tiers are priced 1.86× apart per unit of the compute
  they ran.** A leaf is `elements / tile_len` of a node's output — the dense tier tiles at 128, the hybrid
  at 512 — so the same row commits four times the leaves on one class; a routed-expert row commits eight
  experts' matmuls as one row; and the canonical jobs differ nine-fold in tokens. Measured with Decision 2
  (§7): the dense tier runs 12,767 MAC-equivalents a leaf, the hybrid 7,846 (canonical against canonical,
  1.63×); and since the job an attempt runs is the prefill-only job (ADR-0117) while the price is the
  canonical job's leaves, the hybrid — which loses two of its nine tokens' decode calls to the rule where
  the dense tier loses two of sixty-five — is paid **86.4 % more per MAC-equivalent it ran** than the
  dense tier on today's basis. By the operator's line (5 % good, 10 % acceptable, 20 % a distortion) that
  is a distortion, and it is the basis's, not the mix's: it is the same at any claim ratio.
* **The unit is whichever weight-bearing model class is heaviest**, and a class bears weight once its share is
  above zero — a class registered at 1 ‰ set testnet-11's unit and lowered every other class's pay. Read
  in expected attempts the same rule is unusable: that class's target is 3,165 draws a claim, so an
  attempted-compute unit set by it would price both live tiers at 0 ‰ (§7).
* **Panels license nothing for the hybrid.** In the live window (§7) the hybrid's 500 sampled claims
  reached no licence; every one timed out and was redrawn. Its producers' `A_m` is therefore zero
  whatever the price says — the largest live distortion is panel liveness for the 33 GiB class, not a
  formula. The dense tier licensed 133 of 453 terminal claims (48 `Final`, 320 voided).
* **What is already separate:** the 45 % domain cap and the parity rules shape execution slots only (ADR-0125,
  ADR-0130); pay is per `Final` claim. Execution-block fees go to the permit holder's payout — a future source of
  cross-class income differences, left for later.

## 2. Decisions

**Decision 1 — measure first: per-class economics over RPC (node-local).** For each class and each span: attempts
accepted, claims `Final` and voided, final rate; Σ `claim.pwu`, Σ static leaves, Σ economic compute (Decision 2);
producer BILI, panel BILI and execution-fee BILI actually paid; average class target and expected attempts; and
the ratios `F_m = paid BILI / Final compute`, `G_m = paid BILI / attempted compute` (voided claims included) and
`Gap = max(F) / min(F) − 1`, reported for the leaf basis and the compute basis side by side. A recorder follows
the chain and keys every row by claim id, so a reorg rewrites a row rather than adding one.

**Decision 2 — `EconomicComputeV1`: kernel-weighted compute, deterministic and hardware-independent.** From a
class's shape profile and a job context: per position, the multiply–accumulate count of every node — matmuls
by input × output and a dtype weight, attention by heads × head dim × the true kv length, GatedDeltaNet by its
state update, MoE by the router and the active experts only, elementwise ops by element count, the LM head where
logits are computed — summed with one versioned cost table (`PALW_ECONOMIC_COST_TABLE_V1`). Wall-clock time on
reference hosts calibrates the table and is never a consensus input. Shadow only: nothing on the block path reads it.

**Decisions 3–6 — reward scalar (not adopted).** 単一の CCU/MAC-eq に報酬を切り替える計画は実装しない。class 間の実コストを同じ尺度で表せる根拠がなく、[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P4 の representation-neutral accounting を満たさないため。[ADR-0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) は係数表を採用していない。D1–D2 の shadow 計測は維持する。

## 7. Implementation record and measurements (2026-09-17)

**Built, in shadow.** `consensus/core/src/palw_economic_compute_v1.rs`: the cost table
(`PALW_ECONOMIC_COST_TABLE_V1`: a MAC-equivalent is one 8-bit-weight multiply–accumulate; attention over
the cache by heads × head dim × true kv length; the gated-delta recurrence at four MACs a state element;
norms 2, rotations 2, transcendentals 4, GLU 5, the convolution a tap a channel; wider weight dtypes at their
byte width), the per-node derivation from the profile's own node tables (a dense matmul is input width ×
output width; a routed-expert matmul is the concatenated row's active experts only, the down projection
block-diagonal over the router's `k`; the LM head where logits are computed), the closed form over exactly
the positions the leaf enumeration counts, the priced reward over `u128` measures, the three bases, the
unit, and the class census (`palw_class_census_v1`: registration numbers, target, expected attempts, claims
by phase, redraws, escrows). `getPalwClassEconomics` (op 185) hands a client the census with each class's
compute from its registered carriage or this build's ledger; `misaka palw economics` prices every class on
every basis with the same functions and prints `F_m`, `A_m`, the panel's rate per verification compute and
the gaps. Nothing on the block path reads any of it.

**The live classes, measured** (cost table v1; the job an attempt runs is the prefill-only job):

| class | canonical job | leaves | canonical MAC-eq | attempt job MAC-eq | MAC-eq / leaf | per token |
|---|---|---|---|---|---|---|
| `PALW-BASE-0/rc` | 8 + 4 | 7,708 | 30,504,896 | 21,657,728 | 3,957 | 2.5 M |
| `Qwen3.6-35B-A3B/graph-v3` | 7 + 2 | 2,685,360 | 21,070,759,296 | 18,055,200,736 | 7,846 | 2.34 G |
| `Qwen/Qwen2.5-1.5B/graph-v5@512` | 63 + 2 | 6,630,544 | 84,653,733,376 | 83,102,171,136 | 12,767 | 1.30 G |
| `Qwen/Qwen3.8-27B/graph-v3` | 7 + 2 | 9,000,776 | 198,712,432,640 | 172,919,123,392 | 22,077 | 22.1 G |

By kind, the hybrid's canonical job is 11.4 G dense projections, 8.1 G routed experts (eight of 256 at
2048 × 512, three matmuls, forty layers), 0.5 G recurrence, 1.0 G logits (2048 × 248,320 twice), 3 M
attention at kv ≤ 8; the dense tier's is 83.9 G dense (28 layers of 1536 × 8960 SwiGLU over 64 positions),
0.18 G attention, 0.47 G logits. The 27B is a one-expert mixture of 5120 × 17,408 over 64 layers.

**The draws a claim costs are not whole.** The fork-choice factor `palw_expected_attempts_v1` floors
`2¹²⁸ / (target + 1)` to whole inferences — right for `claim.pwu`, wrong for a price of attempts. At the live
targets (DAA 5,784: the dense tier at `2.276 × 10³⁸`, the hybrid at `3.396 × 10³⁸`) both classes read as one
expected attempt, while the dense tier draws **1.495** forwards a claim in expectation and the hybrid
**1.002** (`palw_expected_attempts_q32_v1`, Q32 fixed point, integer part the consensus factor at every
target; the 27B 3,165.96, the floor 26,403.64). The shadow's attempted basis reads the fraction
(`palw_attempted_compute_q32_per_claim_v1`); the census and op 185 carry both numbers.

**Per claim, at testnet-11's 6,001 escrow (3,200.85 MSK), the live targets and the live unit (the 27B),
producer 80 / panel pool 20; `F` per MAC-eq the `Final` claim's job cost, `A` per MAC-eq its draws cost in
expectation (MSK per 10⁹ MAC-eq):**

| basis | unit | Qwen3.6 price · MSK (producer / pool) | Qwen2.5 price · MSK (producer / pool) | `F₃₆` / `F₂₅` | gap `F` | `A₃₆` / `A₂₅` | gap `A` |
|---|---|---|---|---|---|---|---|
| leaves (in force) | 9,000,776 | 29.8 % · 954.96 (763.97 / 190.99) | 73.7 % · 2,357.95 (1,886.36 / 471.59) | 52.89 / 28.37 | **86.4 %** | 52.78 / 18.98 | **178 %** |
| economic job | 172.9 G | 10.4 % · 334.21 (267.37 / 66.84) | 48.1 % · 1,538.28 (1,230.62 / 307.66) | 18.51 / 18.51 | 0.0 % | 18.47 / 12.38 | **49.2 %** |
| economic attempted | 547 T | 0 % · 0.11 | 0 % · 0.73 | 0.01 / 0.01 | 49.2 % | 0.01 / 0.01 | 0.0 % |

The measured producer / pool split is 80 % / 20 % after pricing. The compared bases do not simultaneously equalize per-job and per-attempt cost. No reward scalar or calibration rate is selected from this shadow comparison; ADR-0144 P4 and ADR-0146 govern. The conservation result remains `producer + pool + burned == escrow`; the floor is paid whole on every measured basis.

**The live window** (1,311 distinct claims read from four bonds at DAA 5,774, before 6,001, when every
`Final` paid its whole 2,756.28 MSK escrow to the producer and no panel was paid): the dense tier's
`F` = 29,021 MSK per 10¹² MAC-eq and `A` = 1,748 (48 `Final` of 797 accepted; 320 voided at
`receipt_timeout`, 451 redrawn); the hybrid's `A` = 0 (500 accepted, none licensed, 282 redrawn); the floor's
window held no `Final`. The hybrid's gap is not a price: its panels do not license it.

**Calibration.** The operator's timings — the dense tier's A16 runtime at ~50 prefill / ~33 decode tokens a
second on an M4 Pro, the hybrid's canonical job at ~7.7 s on the reference host — put the dense tier at
~65 G MAC-eq/s and the hybrid at ~2.7 G/s: a twenty-fold gap this table does not predict (its per-token
cost ratio is 1.8). The hybrid's 33 GiB artifact is not resident on that host (ADR-0112's page faults), so
its timing is a host property, not the graph's; calibration needs a host where the artifact is resident,
and the routed experts' per-token weight traffic (~1 GB a token at batch one) is the term to measure there.

**Not built.** Decision 1's durable recorder of actual payments and a window beyond state retention. The former scalar reward fence, panel-compute pricing and calibration-rate implementation plans are not adopted: no justified scalar was found (ADR-0144/0146).

## 3. Deferred

An execution-fee common pool; panel pay by verification compute (after Decision 4's measurement); a DA/availability
reward.

## 4. Status

* Decisions 1 and 2: built in shadow (§7); the λ shadow of ADR-0130 Decision 8 is not yet read by op 185.
* Decisions 3–6: not adopted; shadow measurements alone do not authorize a reward scalar.

## 5. Number hygiene

0131 was free when written; the next free number is 0132.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P4: reward may change
only when verified work changes. Canonical work is a **vector**; a single scalar (MAC-eq, CCU, a
heaviest-class unit) has not been shown to track real cost. [ADR-0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md)
records the coefficient search; no coefficient table or scalar was adopted.

* **Kept, in shadow.** Decisions 1 and 2: node-local measurement, PWU stays fork-choice work, time
  is calibration data, producer compute and panel compute are named separately. Op 185 may keep
  printing the census.
* **Decisions 3–6 do not arm a scalar CCU.** The reward basis, the panel-pay measurement as a
  consensus input, the rate, and a new model's shadow-then-price path wait on ADR-0146. Arming
  them first would freeze the arithmetic 0144/0145 already disqualified. The 2026-09-17 "operator's,
  after the shadow has run" schedule is withdrawn as a sufficient condition.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md -->

</details>

<a id="source-0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price"></a>

<details>
<summary>0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md — 原文 L1–L371</summary>

[原文を開く](../adr/0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) · SHA-256: `292a036fc3beaf9ebe538bfb79181a8eb5a270fda3594c4947baef81978b5a0a`

<!-- ADR_SOURCE_BEGIN:0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md -->
~~~~~~~~markdown
# ADR-0132 — What a model is actually paid per forward it ran, and why the gap is liveness before it is price

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **PROPOSED 2026-09-17; the end-to-end shadow (§6) built the same day** on
  `feat/palw-exec-lane-and-validator-retirement`. No consensus rule, parameter or fingerprint moves.
* Operator's direction, in the operator's words: "価格式だけ公平ではなく … 実際に支払われた BILI まで含めた end-to-end
  の actual BILI / attempted Economic CCU をモデル間で可能な限り一致させること"; "Final 率差の原因が protocol 上の
  不公平なのか、単なる runtime / artifact availability 問題なのかを分離"; "compute cost と data movement /
  availability cost は分離"; "モデル名ごとの手書き固定倍率は禁止"; "5 年後にモデルが 10 種類になっても破綻しにくい案".
* Builds on: [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic
  compute, the three bases, op 185), [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the panel split and the work price), [0130](0130-bps1-is-hardened-before-it-is-widened.md) (shadow first),
  [0072](0072-the-attempt-is-a-function-of-the-execution.md) (one execution, two lotteries),
  [0071](0071-hash-coupling-and-the-remainder.md) SA-3 (`palw_capability_bound`), [0117](0117-a-draw-is-one-forward.md),
  [0098](0098-a-panels-coverage-of-one-claim-is-a-number.md), [0107](0107-share-growth-is-counted-at-final.md).

## 0. The sentence this ADR is

**On testnet-11 today a dense-tier claim is paid, in expectation, 0.60 BILI per 10⁹ MAC-equivalents its producer
ran, and a hybrid claim is paid nothing — not because the price is wrong but because no seat that can run the
hybrid ever sits on its panel; the price, once the hybrid licenses at all, pays it 2.8 × more per forward than the
dense tier on today's leaf basis and exactly the same on an attempted-compute basis; so the order of work is
liveness (a panel that can run what it judges), then the basis (attempted compute at a fixed rate), and never a
per-model multiplier.**

## 1. The path a forward takes to a payout (read without changing code, 2026-09-17, DAA 5,774–5,787)

1. **A draw is one forward and two lotteries** (ADR-0072/0117, `kaspad/src/palw_producer.rs::produce_one`). The
   class ticket `H(execution) ≤ class_target` admits with `p_class = (T + 1) / 2¹²⁸`; a winning execution then
   faces the network draw, the Layer-0 digest of the header against `bits`, with `p_net = (target₂₅₆ + 1) / 2²⁵⁶`.
   Neither lottery is searched: a lost bucket is a lost inference. **Only the class lottery is credited**:
   `claim.pwu = ⌊2¹²⁸/(T+1)⌋ × leaves` counts class draws; the forwards the network draw discards buy no weight
   and no pay.
2. **The hybrid producer's counter** (`getPalwNodeStatus` on the ibm `.t11` node, bond `:0`, the only
   `Qwen3.6-35B-A3B` producer): `979 draws this run, 202 produced, 777 won the class ticket and lost the network
   draw against bits; class ticket p = 9.980e-1 per draw`. So `1 / (p_class · p_net) = 4.85` forwards a block
   while the chain credits 1.002. `bits` at the tip is `0x207fffff` — the floor of the difficulty, `p_net = 0.5` —
   and averaged `p_net ≈ 0.21` over the run. `p_net` is the same for every class at a moment (one `bits`, one
   digest rule), so it does not move the cross-model ratio; it multiplies every class's attempted compute by 2–5.
3. **What one hybrid forward moves.** The same node logs `this draw read 9,4xx–9,9xx MiB from storage (no mapped
   class holds a residency: the page cache decides)` for every draw: the 23.9 GB Q4 GGUF is not resident on a
   23 GB host running two nodes (13 GB used). Compute per forward is 18.1 G MAC-eq (ADR-0131); data moved is
   ~9.7 GiB. **These are two costs, not one**, and the second is the host's, not the graph's.
4. **Acceptance → panel.** A claim binds its five seats at the first chain block past `accepted + 20`
   (`anchor_delay`), the receipt window is 600 DAA, a panel that does not license is redrawn once at ~+620 and
   the claim voids `receipt_timeout` at ~+1,242 (`PALW_RC_WINDOWS_V1`; the live medians below agree).
5. **The draw is blind to what a seat holds** past no fence: `palw_bond_may_judge_class_v3(…, require_production
   = palw_capability_bound_at(anchor))`, and `palw_capability_bound` is `None` on testnet-11, so a bond judges
   every class it *declared* — every genesis bond declares all three. The executor and its operator are excluded
   from their own panel (ADR-0071).
6. **What seats do.** On the ibm seat's last 400 k log lines: `717 × "interval seat — drew [4] of N interval(s),
   0 opening(s) held"` → `748 × "asked the executor for interval(s)"` → `9 × "holding a served opening"` →
   `504 × "no material held after 2–4 DAA — replaying the anchor's job"` → `497 × "licensed by replay … 6,508,520
   leaves replayed, 20.1 s mean (12–61 s)"` → `527 × "Valid"`, `65 × "Unavailable"`, `0 × "Incapable"`. The
   sampled-interval path (ADR-0098) never verified anything: **every licence on testnet-11 is a full replay by
   every live seat**, so a claim's verification compute is `live seats × the attempt's job`, not the four
   intervals the design prices.
7. **Final → BILI.** Below DAA 6,001 every `Final` names its producer the whole escrow — coinbase outputs of exactly
   `275,628,448,680` sompi (2,756.28 BILI) at DAA 5,758 (×2), 5,763, 5,765 and 5,766 on the selected chain — and
   no seat is paid. From 6,001: escrow 3,200.85 BILI × the class's leaf price (dense tier 73.7 %, hybrid 29.8 %,
   unit = the 1 ‰ 27B) → 80 % producer / 20 % panel pool (unused seats' shares → reserve; the priced-away
   remainder never minted). Claims whose attempt block was merged below the deep fence hold `escrow 0` (paid at
   acceptance, ADR-0058 B-1 before 6,000): 111 dense, 81 hybrid in the window.

**The live window** (1,464 distinct attempt-lane claims from the eight genesis bonds' executor and seat views,
DAA 5,774 → 5,785):

| | Qwen2.5-1.5B@512 (dense) | Qwen3.6-35B-A3B (hybrid) | floor |
|---|---|---|---|
| accepted in view (accepted DAA) | 919 (3,542–5,629) | 500 (4,703–5,774) | 45 |
| provisional / bound / licensed / Final / voided | 10 / 424 / 117 / **48** / 320 (`receipt_timeout`) | 20 / 480 / **0 / 0 / 0** | 0 / 18 / 27 / 0 / 0 |
| redrawn (second panel) | 493 | 282 | 19 |
| accepted → first bind / redraw / licence / Final / void (median DAA) | 20 / 641 / — / **2,017** / 1,242 | 20 / 641 / — / — / — | 20 / — / — / — / — |
| producers | premine `:2` + 4 external operators | premine `:0` only | premine |
| seats drawn (bond index : panels) | 0–7 + 4 external, uniform | `1–7` uniform, **never `:0`** | |
| licence rate (licensed + Final) / accepted | 18.0 % | **0 %** | 60 % |
| Final rate, terminal claims | 48 / 368 = **13.0 %** | — | — |
| paid so far (pre-6,001 rule) | 48 × 2,756.28 = **132,301 BILI**, producers only | **0** | 0 |

**Why the hybrid never licenses — three facts, none a price.** The only host holding its artifact (`:0`) is its
only producer and is excluded from its own panels; bond `:1` (same host, no hybrid artifact) is drawn on 344 of
482 hybrid panels while its node reports `Chain participation held: state=ibd-running. Not mining, not
attesting` (a dead seat, filing nothing); bonds `2–7` never file a hybrid receipt (their hosts are memory-bound,
ADR-0112). Three of five seats must replay a 24 GB artifact within 20 h; zero can. The claim then waits the
whole window twice — `Incapable` is never filed here, and where it is filed it counts toward nothing and shortens
nothing (ADR-0065 D4) — and voids after 41 h with the producer's reservation held throughout. **The dense tier's
own 13 % `Final` rate is the same disease at lower dose**: one seat (`:0`) replays every dense claim it is dealt
in 20 s; the rest of the fleet does not keep up, and 87 % of terminal dense claims time out.

## 2. The end-to-end numbers, per accepted claim (expectation; `p_net` at the run's 0.206)

| metric | dense | hybrid | note |
|---|---|---|---|
| `expected_attempts_q32` (class) | 1.495 | 1.002 | ADR-0131 §7 |
| network draws per class win, `1 / p_net` | 4.85 | 4.85 | one `bits` for all |
| forwards per accepted claim | 7.24 | 4.86 | `1 / (p_class · p_net)` |
| attempted economic CCU per accepted claim | 601.6 G | 87.8 G | × the draw job (83.1 G / 18.06 G) |
| data moved per forward | not measured (A16, resident) | ~9.7 GiB | host property |
| panel verification CCU per bound claim (today: full replay × live seats) | ≤ 5 × 83.1 G = 415 G | ≤ 5 × 18.06 G (never run) | design: 4 intervals a seat |
| P(Final) per terminal claim | 0.130 | 0 | |
| **producer actual BILI / attempted 10⁹ CCU, pre-6,001** | 0.130 × 2,756.28 / 601.6 = **0.598** | **0** | gap ∞ |
| producer actual, 6,001 leaf basis (× 0.737 × 0.8) | 0.130 × 1,886.36 / 601.6 = **0.408** | 0 (would be **1.134** at the dense tier's Final rate) | price gap 178 % |
| producer actual, 6,001 economic-attempted basis, dense-tier unit | 0.130 × 2,560.68 / 601.6 = 0.553 | (0.553 at equal Final rate) | price gap 0 % |
| panel actual BILI / verification CCU, pre-6,001 | 0 | 0 | seats unpaid below 6,001 |

**Decomposition of the realized gap** (`actual_A₃₆ / actual_A₂₅`):
`= [price per claim ratio 954.96 / 2,357.95 = 0.405] × [attempted CCU ratio 601.6 / 87.8 = 6.85] × [Final-rate
ratio 0 / 0.130 = 0]`. The first two terms multiply to 2.78 (the 178 % price gap in the hybrid's favour); the third
is zero. **Today 100 % of the realized distortion is the liveness term**; the price term is what remains once the
hybrid licenses, and it flips the sign: the hybrid would be over-paid per forward, not under-paid.

## 3. Hypotheses for why a gap remains, each with what would falsify it

* **H1 — the panel cannot run the hybrid** (topology): the executor exclusion plus one artifact host. Falsified
  if a hybrid claim licenses with the fleet unchanged. It has not, in 500 claims.
* **H2 — the draw does not price availability**: declared ≠ resident (§1.5). Falsified if bonds without the
  artifact stop being dealt hybrid panels; today they are dealt 344–352 each.
* **H3 — verification is 5 × a full replay**, not 4 intervals: the opening protocol is dead (748 asks, 9 openings,
  0 verified; the interval server refused 22,202 requests as `not-held`). Falsified if openings verify; none did.
* **H4 — the leaf basis prices tiles, not compute**: ADR-0131 (86 % / 178 %). Falsified by the CCU table's
  calibration; the residual is the cost table's (§7).
* **H5 — the fork-choice factor floors the draws**: 1.495 read as 1 (ADR-0131 §7).
* **H6 — the network draw wastes 2–5 forwards a block for everyone**: class-neutral, but it multiplies the absolute
  attempted compute and the hybrid's I/O (4.85 × 9.7 GiB a block). Falsified if `p_net` differed by class; it cannot.
* **H7 — data movement is read as compute**: a 2-minute hybrid draw is 9.7 GiB of page-ins on a 23 GB host, not
  18 G MAC-eq of arithmetic. Falsified on a host where the artifact is resident (ADR-0131 §7 asks for one).
* **H8 — the 600-DAA window turns every dead panel into 41 h of held collateral**, so a class with weak panels
  loses claims before any price applies (ADR-0130's next ADR: a shorter liability lifetime).
* **H9 — merged blocks below the deep fence were paid at acceptance** without licensing (`escrow 0`): a
  class-neutral leak that ended at 6,000.

## 4. Proposals, compared

Twelve attributes each. "Consensus" = changes a rule a block is judged by (needs a fence, never retroactive).

| | A. `EconomicAttempted` as the future basis | B. Snapshot the claim's economics at acceptance | C. Unit → fixed rate (`min(escrow, attempted_ccu × rate)`) | D. Fixed pool shared by CCU ratio | E. Producer CCU and panel verification CCU priced apart | F. Panel liveness (capability by evidence, `Incapable` fast path, availability cost) | G. Fewer full replays (openings that work; sampled verification; optimistic licensing) | H. Absorb Final-rate gaps in the price | S. One lottery (drop the network draw for attempts) |
|---|---|---|---|---|---|---|---|---|---|
| fixes | price ∝ forwards actually run (class draws × network draws × draw CCU) | pay independent of later retargets, registrations, unit moves | a 1 ‰ registration can no longer lower every class's pay; heavy models cap at escrow | emission neutral by construction | panel pay follows verification cost, not the producer's | the hybrid licensing at all; dead seats not dealt | 5 × replay → ~1–2 × sampled; seat capacity ×3–5 | nothing real | 79 % of forwards not thrown away |
| distortion addressed | H4, H5, H6 (absolute) | drift between accept and Final | unit manipulation (ADR-0131 §1) | none of the live ones | future (when verification ≠ job) | **H1, H2, H8** | **H3**, and H1 by capacity | — (it masks H1) | H6 |
| consensus-critical | yes (basis at a fence) | yes (claim record fields) | yes (the rate is a fenced parameter) | yes | yes (split rule) | F1/F2 yes; F3/F4 no | G1 no; G2 no (design exists); G3 yes | yes | yes, large |
| migration | new claims only; fence height | new claims only; state version bump | none beyond the fence | none | none | none (F1 reads existing facts) | none | — | header/template rules |
| auto-extends to new models | yes (graph-derived CCU) | yes | yes; a heavier-than-cap model is paid whole and signals a re-pin | yes but dilutes others | yes | yes | yes | no (per-class rates = multipliers by another name) | yes |
| attack surface | a class can inflate its own draws only by a tighter target it does not control | none new | the rate is a governance constant; a heavy registrant gains nothing from others | registrant floods CCU to dilute | seats over-report verification? (measured from the rule, not self-reported) | F1: a bond must show a receipt/production on the class — cheap, honest; F2: Incapable is unsigned-cost → must not be slashable, only informational | G3 shifts security to the court: a lie is caught by sampling w.p. ADR-0098's number, not 3-of-5 | encourages failing to license | removes the `bits` ceiling on block rate → class targets must hold the cadence alone |
| liveness impact | none | none | none | none | none | **large positive** | positive (seat capacity) | negative (rewards silence) | positive (fewer wasted forwards) |
| panel load | none | none | none | none | none | less (no hopeless duties) | **much less** | none | none |
| producer load | none | none | none | none | none | less waiting | none | none | **÷ 2–5** |
| BILI emission | unchanged (remainder never minted) | unchanged | unchanged | unchanged (by construction) | unchanged | unchanged | unchanged | unchanged | unchanged |
| implementation | small (shadow exists) | small–medium | small (one fence, one constant) | small | medium (meter first) | F1 small (reuse `palw_bond_produced_on_class` + a receipt fact); F2 small; F3 node-side medium | G1 investigation; G2 exists; G3 medium | trivial | large |
| rollback | fence height → `never` before it fires | same | same | same | same | same | node-side: config | — | hard |

**What this ADR recommends, in order.**

1. **F first, without a fence, this week**: put the hybrid artifact on three seat hosts that are not its producer
   (or make bonds `2–7`'s hosts able to page it — a 24 GB Q4 file needs a 32 GB host or NVMe page cache); fix the
   `.t11b` node stuck in IBD; find why openings are `not-held` (G1). None of this is consensus, and it is the whole
   of today's realized gap.
2. **Measure end to end in shadow (§6, built)**: actual BILI paid, attempted CCU with both lotteries, verification
   CCU, lifetimes, per class; the fleet's artifact I/O beside its compute. Then the gap has a number that is not
   infinity.
3. **One fence, when the shadow says so** (ADR-0131 Decision 3's height): **C + A + B + F1 + F2** together — a
   claim snapshots `(expected_attempts_q32, network_expected_attempts_q32)` at acceptance (B), is paid
   `min(escrow, draw_ccu × both × rate)` (A + C) with `rate` a fenced constant calibrated from the shadow so the
   heaviest *live* class at that height is paid its escrow whole, 80/20 unchanged (E deferred until verification
   CCU is measured); a bond is dealt a class it has produced on **or filed a Valid/Invalid receipt on** within
   `capability_evidence_daa` (F1, an extension of ADR-0071 SA-3's `palw_capability_bound` that verify-only
   operators can satisfy); a panel whose `Incapable` answers make quorum unreachable is redrawn at once, and a
   class with no drawable panel voids at bind as `NoCapablePanel` (F2) — a class-liveness fact ADR-0107 can read.
4. **Not H.** A per-class multiplier for a low `Final` rate pays for claims that never finalize — i.e. nothing —
   and, once the class finalizes, over-pays it. Liveness is fixed as liveness.

**Three horizons.** *Now*: 1–2 above (no consensus). *Shadow → fence*: 3, after weeks of shadow data on a fleet
where the hybrid licenses. *Ideal*: **S** (one lottery) with **G3** (optimistic licensing on sampled audits) and
ADR-0107 (share by `Final`) — §5.

## 5. How the protocol should be simpler (the author's own proposal)

Today a forward runs two lotteries, five seats each re-run it whole, and a claim waits up to 41 hours for a
panel that may hold nothing. Three cuts, each measured on this chain:

* **One lottery.** The class targets already fix every class's block rate (`Σ share = 1`, retargeted by census).
  The network draw against `bits` adds nothing to that — it only discards `1 − p_net` of winning executions
  (79 % on the hybrid's run, 50 % at the difficulty floor) and credits none of them. Let the class target carry the
  cadence: `T_c = MAX · share_c · (block interval / class forward interval)`, retargeted as now, and `bits` becomes
  a derived witness of the class targets. A block costs one forward; the 9.7 GiB the hybrid moves per draw is moved
  once a block, not five times. Consensus-critical and large (header rules, templates, the heartbeat lane's
  separate `bits`); the pay-off is measured: 777 of 979 forwards.
* **Optimistic licensing over sampled audits.** ADR-0098 already says the panel's protection against a one-leaf
  lie is 6.5 % at 300 tokens and five seats; the court is what convicts. So make the licence what it already is in
  effect — *nobody proved a fault in the window* — with each seat replaying only its drawn intervals (a fixed
  fraction of the job, ADR-0098's number), and a single full replay a claim assigned by the draw to ONE seat.
  Verification compute drops from `5 × job` to `≈ 1 × job + 4 × intervals`, a panel that holds no artifact can
  say `Incapable` and be replaced the same span, and a class licenses as long as one capable seat exists — the
  hybrid would have licensed on this fleet. What it costs: the licence no longer carries three signatures over the
  whole execution; the reservation and the court carry the security, as they do for the free-prompt lane.
* **Pay compute at a rate, not a share of a class.** `min(escrow, ccu × rate)`: no unit, no heaviest class, no
  per-model number anywhere; a new model registers, runs in shadow until its CCU and its licence rate are
  measured, and is then paid by the same constant as every other. Ten models in five years change nothing here.

## 6. What is built (shadow, node-local)

Nothing consensus reads; the fingerprint does not move.

* **The network draw, priced from `bits`** — `palw_network_expected_attempts_q32_v1(bits) = ⌊2²⁵⁶ · 2³² /
  (target₂₅₆ + 1)⌋` (`consensus/core/src/palw_economic_compute_v1.rs`): the second lottery a winning forward
  faces (2.0 at the difficulty floor). ADR-0131's attempted basis now reads `class draws × network draws × one
  draw's job`, and the census carries the tip's `bits`.
* **The end-to-end ledger** (`consensus/core/src/palw_economics_ledger_v1.rs`, pure): an observation of every
  attempt-lane claim in the state (phase marks, escrow, the duty row's seats and credits); a row that keeps its
  first sight (class draws, network draws of the accepted block, the draw's compute, leaves) and fills its
  lifecycle marks once; the payout **as the rule in force at the claim's `Final`** derives it — the escrow
  whole below the fences, past them the leaf price against the unit, 80/20, credited seats, reserve, and the
  priced-away escrow burned (`palw_ledger_payout_v1`, the fold's own functions); per-class totals with the
  three actual rates the operator named (`producer / attempted`, `panel / verification`, `total / attempted`),
  `Final` and licence rates, mean waits (bind, licence, `Final`, void), mean draws (class and network); and
  proposal C's `min(escrow, attempted × rate)`.
* **The node's recorder** (`kaspad/src/palw_economics.rs`): rows in the meta database
  (`PalwEconomicsLedgerV1`, kept past the chain's retention, a schema marker that drops rows of another
  layout), refreshed every 60 s on its own thread and before every op 185 answer; the network draws of a row
  from its accepted block's header; the payout rule from `Params` at the `Final`'s height.
* **The node's telemetry**: per class, what THIS node did — the producer's draws, class wins, blocks, forward
  milliseconds and the MiB the process read from storage during each draw (ADR-0112's number, now counted);
  the seat's whole-job replays with their milliseconds and leaves, receipts by verdict, openings held.
* **Op 185 `getPalwClassEconomics`**, extended (never shipped, so its layout may still change): the tip's
  `bits` and network draws; per class a `ledger` block (counts, sompi paid to producers and seats, reserve,
  burned, attempted / `Final` / verification compute, the three actual rates, licence and `Final` rates,
  waits, mean draws) and a `telemetry` block. **`misaka palw economics`** prints the end-to-end table, the
  actual gaps (an infinite gap names the class paid nothing), this node's own work, and each class's
  `Final`s priced under every basis at the class's ACTUAL `Final` rate — `current` vs `EconomicJob` vs
  `EconomicAttempted` vs `EconomicRate (C)` side by side; JSON schema `misaka.palw.economics.v2` with
  `attempted_ccu`, `final_ccu`, `verification_ccu`, `actual_*_msk`, `msk_per_attempted_ccu`,
  `msk_per_final_ccu`, `panel_msk_per_verification_ccu`, the counts and rates, `avg_expected_attempts_q32`,
  `avg_claim_lifetime_*`, `avg_bind_wait_daa`, `artifact_bytes_fetched_mib`.
* **Not built**: artifact cache hit rate and fetch latency as such (the MiB read from storage per draw is the
  measured proxy; a resident artifact reads ~0 MiB, a non-resident one its working set); the paid amounts of
  claims the recorder first saw after their `Final` are derived, not observed (the duty row is gone by then,
  so their credited seats read as none); ADR-0091's buyback slice; a recorder that survives a node that
  never ran it (the window starts when the node does).

## 7. Implementation record

* 2026-09-17, `feat/palw-exec-lane-and-validator-retirement`: the read (§1–2) and the proposals (§4–5);
  then §6 on the same day. Tests: `adr0132_*` in `palw_economic_compute_v1` (the network draw from `bits`:
  the floor's coin flip, monotone in the target, saturation, the ratio invariant across `bits`),
  `palw_economics_ledger_v1` (the payout by the rule at the `Final` with the emission identity; a row's
  first sight and marks; the actual rates as a property of the class across the 100/0, 0/100, 50/50, 90/10
  and 10/90 mixes; a `Final`-rate gap and a licence-rate gap read as pay, not price; targets and `bits`
  move attempted compute, not pay; a class no panel can run pays nothing and verifies nothing, and one
  operator down reads as the licence rate it costs; a heavier model moves the unit but not a rate),
  `kaspad::palw_economics` (the store round-trips, upserts and drops a foreign layout; the telemetry counts
  per class) and `misaka-cli::palw_economics` (the end-to-end reading is what was paid; the four-basis
  table). Verification on `feat/palw-exec-lane-and-validator-retirement` (2026-09-17): consensus-core 2,142 /
  consensus (evm) 305 / kaspad 98 / misaka-cli 176 / rpc-core 148 / rpc-service 1 / database 8 / integration
  `rpc_tests::sanity_test` 1 — all green; clippy `-D warnings` over the eleven crates clean;
  `shipped_presets_have_pinned_fingerprints` unmoved (`ab4e7b9c…`).

* 2026-09-17 (later the same day), **Protocol Upgrade C behind a dormant fence** — `Params::palw_economic_payout:
  Option<PalwEconomicPayoutV1 { activation, rate_sompi_per_giga, panel_share_alpha_permille,
  panel_share_min_permille, panel_share_max_permille, cap_utilization_max_permille }>`, `None` on every shipped
  preset (the testnet-11 fingerprint `135b6ee0…` does not move), hashed `Some`-only with its five numbers,
  refused without `palw_model_registry` and `palw_panel_economy` at or below its height, and by name in the
  fork-id arm (`palw_economic_payout`, the devnet's numbers). What it does: **C + A + B** together — a
  model-class claim accepted past the fence writes a `claim_economics` row (state tail `0xAB`, delta
  discriminant 53, rooted once any exists, byte-identical below): the class draws at its target (Q32), the
  network draws at the carrying block's `bits` (Q32, one where no header was at hand), one draw's job and one
  seat's verification compute from the registry's work for the class, the rate then, and the panel share
  `clamp(α·C_V / (C_P + α·C_V), S_min, S_max)` with `C_P` the attempted compute and `C_V = seats ×
  verification` (the operator's rule; `α`, the bounds and the ceiling are the fence's, the same for every
  class). Its `Final` is paid `min(escrow, attempted × rate)`, the panel that share (per seat fixed, the
  uncredited to the reserve), the rest never named; the row leaves with the `Final` or the void. The floor takes
  no row and folds byte-identically; a claim accepted below the fence is paid as before whatever the fence says
  at its `Final`. **The cap ceiling as a rule** (ADR-0133 Fence 3): each registry boundary records a class's
  cap utilization (`attempted × rate` against the escrow a claim of the boundary block holds) and a class over
  the ceiling is not stepped to `ACTIVE`; an `ACTIVE` one falls back to a tenth. **Not in this fence**: S (the
  single lottery, Upgrade B — its own ADR, ADR-0133 Fence 2) and H (a licence-rate correction, which is a
  per-class multiplier by another name and stays rejected); "budgeted admission" is Upgrade A's admission-derived
  shares, already in force under the registry. Surfaces: op 186 carries `capUtilizationPermille`; the shadow
  ledger prices a snapshotted `Final` by its snapshot (`PalwLedgerEconomicRuleV1`; the row keeps the chain's
  numbers over the recorder's reading); `--palw-economic-payout-devnet <daa>` arms it on a private devnet with
  `PALW_ECONOMIC_PAYOUT_DEVNET_V1`. Tests: `adr0132_*` — the module's own (price, network draw, share bounds,
  cap, borsh layout), the fold's four (snapshot and Final at the rate against the same chain below the fence;
  the snapshot fixes the price; the floor and a void; cap utilization recorded and a saturated class not
  activated), the registry's step, the ledger's mirror, the params' fence — 21 green on consensus-core.
  **Arming**: testnet-11 at 6,001 beside the registry, after Upgrade A's end-to-end drill passes, with `rate`
  calibrated from the shadow so the heaviest live class sits under the 80 % ceiling (a class paid its escrow
  whole reads as saturated and would not activate).

* **Armed for testnet-11 (prepared 2026-09-17 night on `wip/arm-6001-registry-payout`, merged 2026-09-18 on the
  operator's go, after the devnet drill with the three fences armed passed its first phase):** `palw_economic_payout` at the 6,001 flag day beside the registry, with
  `rate = 900,000,000 sompi per 10⁹ MAC-eq` (9 MSK per G MAC-eq), `α = 100 ‰`, the panel share between
  100 ‰ and 300 ‰, the cap ceiling 800 ‰. Calibration from the shadow's numbers at DAA ~5,780 (§1–2,
  ADR-0131): the dense A16 row's attempted compute is 1.495 class draws × 2.0 network draws (the difficulty
  floor's `bits`) × 83.1 G MAC-eq ≈ 248.5 G a claim — the heaviest live class — priced at 2,237 MSK of the
  3,200.85 MSK escrow (69.9 %, under the ceiling with room for a retarget to ~1.7 draws); the hybrid's
  36.2 G a claim at 326 MSK (10.2 %). At `α = 0.1` a five-seat full replay gives the panel 14 % (dense:
  `C_V = 5 × 83.1 G`) and 20 % (hybrid: `5 × 18.1 G`) — the operator's "light verification ~10 %, heavy
  20–30 %" band. What the price leaves (30 % of the dense escrow, 90 % of the hybrid's) is never minted.
  The fingerprint moved to `32c2e8e3…` with the registry, the payout and the work target, and to `8af89f85…` once S and the
  short challenge window (§7.6) joined the same day (2026-09-18); the fork id's height set does not move.

### 7.5 S — the single lottery, built dormant (2026-09-18, commit 7c4f350e)

`Params::palw_single_lottery: Option<ForkActivation>` — None on every preset, `--palw-single-lottery-devnet`
arms it by height on devnet, `validate` refuses it below `palw_work_target`, and the fork-id arms it by
name. Past the fence:

* **Layer 0.** A PALW attempt header (`POW_ALGO_ID_PALW_COMMITTED_V2`, `POW_ALGO_ID_PALW_EXEC_V3`) passes
  `check_pow_layer0_v2` unconditionally, as the receipt lane does, and `calc_block_level_check_pow_layer0_v2`
  gives it block level 0 (`consensus/pow`; pre-ghostdag validation resolves the fence from `header.daa_score`,
  the `check_pow_algo_id` pattern).
* **Difficulty.** An attempt row is unpriced in the window (`DifficultyBlock.attempt`, `attempt_rows_unpriced`
  through `calculate_difficulty_bits` / `retarget_bits` / `retarget_bits_from_rows`), so `bits` no longer chases
  a cadence the class tickets set against a target no block pays; `algo_id_is_priced_by_bits_v3` says the same
  to the readers.
* **The one lottery.** The class lottery's floor becomes `max(W₀, W)` (`palw_work_lottery_floor_v1`): `W₀` is the
  block's own escrow over the rate (stateless, ADR-0137 §22.1) and `W` is the chain's rooted work target —
  `PalwChainStateV2::work_target` (`PalwWorkTargetV2`, carriage tail 0xAD, delta 56, `work_target/v1` in the
  root), stepped at each epoch boundary from the closed epoch's model blocks against `epoch × fp_attempt_share‰`,
  clamped by `class_daa_max_factor`, never below the floor; a quiet epoch returns it to the floor. Below S the
  rooted W exists (once `palw_work_target` is active) but the ticket reads `W₀` alone — the reason the two
  fences are separate heights.
* **Untouched, deliberately.** Blue work, fork choice, the execution block finality boundary, BPS.

Pinned by `adr0132_the_single_lottery_fence_is_dormant_everywhere_arms_by_height_and_needs_the_work_target`
(params), `the_single_lottery_admits_the_attempt_digest_unconditionally_and_buys_no_level` (pow), the
difficulty window test (`no row prices the window past the single lottery`) and
`adr0137_past_the_fence_the_chain_holds_a_rooted_work_target_that_the_single_lottery_reads` (state).
Activation: **testnet-11 DAA 6,001**, the same day as the registry, the payout and the work target — the operator's
rule of 2026-09-18 that the PALW upgrade has one flag day and no part of it fires without the rest, pinned by
`t11_daa_6000_is_the_compatibility_boundary_and_6001_the_one_palw_upgrade_flag_day`; the devnet drill arms all
four at 20 (`SINGLE_LOTTERY_AT`).

### 7.6 The short challenge window, made a fence (2026-09-18)

`6fdf6ba7` (origin/main, 2026-09-14) shortened a licensed claim's challenge window from `window_challenge`
(1,200 DAA) to 120 DAA past a bare `daa_score >= 7_000` in `PalwStateParamsV2::window_challenge_at`, "kept
outside the serialized ruleset so the existing fingerprint remains unchanged"; the 2026-09-17 flag-day move
carried the constant to 6,000. A consensus rule at a height no fingerprint or fork id can see is the
silent-fork shape this repository refuses elsewhere, and the operator's rule of 2026-09-18 is that nothing
rides the 6,001 bundle implicitly. It is now `Params::palw_short_challenge_window`: `Some(6,001)` on
testnet-11 — explicit, beside the model economy it exists for, since at testnet-11's pace (1–5 DAA an hour)
1,200 DAA to a Final is weeks — and `None` elsewhere; hashed Some-only, in the schedule and the fork id like
every fence; mirrored into the V2 bundle's `PalwStateParamsV2::short_challenge_window_from_daa` by
`Params::set_palw_short_challenge_window` (borsh-skipped, so no dormant preset's ruleset id moves), and
`validate_palw_v2` refuses a bundle whose copy disagrees with the fence. The window past it is
`PALW_SHORT_CHALLENGE_WINDOW_DAA_V1 = 120`. Pinned by `the_short_challenge_window_is_a_fence_not_a_constant`
(state) and the flag-day table test (params); DAA 6,000 keeps the long window — it is the compatibility
boundary, not a rule change.

## 8. Number hygiene

0132 was free when written; the next free number is 0133.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md -->

</details>

<a id="source-0144-palw-pays-for-the-inference-you-were-going-to-run-anyway"></a>

<details>
<summary>0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md — 原文 L1–L298</summary>

[原文を開く](../adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) · SHA-256: `96e431b3529045651c09b58053c1499c6c0ee99a6f0a438e81749529587d398c`

<!-- ADR_SOURCE_BEGIN:0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md -->
~~~~~~~~markdown
# ADR-0144 — PALW pays for the inference you were going to run anyway

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** CONSTITUTION, 2026-09-19. This ADR fixes what PALW is FOR. It changes no code and arms
no fence. Its purpose is to make the next four redesigns answerable: a proposal is in scope if it
serves the sentence below, and out of scope if it does not.

> **A person uses a local LLM on their own machine, with prompts they chose for their own reasons,
> and the same inference that answered them is the inference the chain rewards.**

Remote inference markets, GPU rental and third-party inference serving are **non-goals**. A
localhost OpenAI-compatible endpoint, so existing tools can reach the user's own model, is in scope.
An internet-facing gateway that sells somebody else's GPU is not.

## 1. Why this has to be written down now

The mechanism for it already exists and already says so. `palw_freeprompt_v3.rs` opens with

> ADR-0044 (FP-01): the free-prompt job, its execution commitment, and the quantized one-shot receipt
> spend — **the data layer of "the user's own inference becomes the consensus work."**
> `job — what the USER chose to run: their tokens, under a registered class, from a bond`

and the weight is real: a `FreePrompt` claim reaching `Final` retires
`pwu/quanta × spent.len()` of safe weight, beside an `Attempt` claim's `claim.pwu`
(`palw_state_v2.rs:10223-10228`). This ADR adds no new mechanism. It promotes an intent that is
already in the tree to the principle the rest of the protocol is judged against.

**It is written now because the measurement says the economy is not there.** On testnet-11 between
2026-09-15 and 2026-09-19 the node log carries **nine** `FreePromptCommitted` lifecycle objects, the
last on 09-18 01:54, and **zero** `palw-fp` subsystem lines, against 1,839 `palw-panel` lines, 10,206
`palw-interval` lines and a recent block mix that is 100 % algo 6. Essentially all reward today comes
from protocol-generated anchored jobs. The useful path is built, proven and carries none of the
economy.

That is not an argument that the design is wrong. It is an argument that **whatever is cheapest to
farm is what the economy will do**, and right now the cheapest thing to farm is a job nobody asked
for. A protocol that meant to buy useful inference and is buying synthetic inference has become an
LLM-flavoured proof of work, and it will stay one until the useful lane is the profitable one.

## 2. The five principles

**P1 — Local first.** Rewardable inference is inference the user ran on their own machine for their
own use. Remote GPU marketplaces are a non-goal, and no part of the reward path may assume one.

**P2 — Free prompt.** The protocol does not choose the question. Code, prose, translation, RAG,
agents, images — whatever the person actually wanted. The protocol never judges whether a prompt was
*worth* asking.

**P3 — Same inference, one purpose.** The answer the user reads and the work the chain rewards are
the same execution. Not a second run, not a proof job beside it.

**P4 — Representation-neutral accounting.** Reward may change only when *verified work* changes.
It must NOT change with `tile_len`, serialization, how a commitment is split, a registrant-declared
multiplier, the shape of a profile, map iteration order, or any self-reported cost.

**P5 — Efficiency is rewarded; accounting tricks are not.** The same canonical work on a faster GPU,
a better runtime, a better quantisation or a better architecture must leave the miner *more* profit
— that is the technical progress the network wants. Making the same work merely *look* larger must
leave them with nothing.

And the rule that makes P2 survivable:

**P6 — Local inference is unlimited; reward eligibility is scarce and protocol-assigned.** A person
may run their model ten thousand times a day. Only the inferences a future beacon makes eligible
earn. Spam economics are controlled by the scarcity of eligibility, never by the chain forming an
opinion about a prompt.

**P7 — Model admission is permissionless; economic weight is earned through verified use.**

Any user may register a new compatible local model **without modifying `main`**, without adding the
model to a hard-coded consensus list, and without a model-specific protocol upgrade or fork
activation. Model identity, architecture and execution semantics are carried as protocol-defined
canonical data, so a model nobody has heard of enters through the same generic path as one that
shipped with the network.

Registration alone MUST NOT grant unrestricted reward, consensus weight or full eligibility. A newly
registered model starts at bounded or zero eligibility and earns capacity only from
protocol-verifiable evidence: independent validation, correct execution, sustained real local use,
completed claims, continued compliance with the accounting rules.

**The distinction that makes this safe, and the one most likely to be got wrong:**

```
the price of one unit of canonical work   →  NEVER changes with usage
how much reward-eligible work a model
may contribute                            →  grows with verified use
```

A model does not become worth more per inference because it is popular. It becomes able to put MORE
of its inference in front of the protocol. Pricing popularity would make the economy a contest to
appear used; budgeting it makes the economy a contest to actually be used.

Specifically:

* adding a model MUST NOT require changing the `main` source tree merely to name or recognise it;
* registrants MUST NOT choose their own reward multiplier, work value or admission strength;
* usage growth MUST be derived from verified protocol events, never from self-reported popularity
  or self-declared compute;
* repeated legitimate use MAY increase the reward-eligible work budget available to that model;
* increasing usage MUST NOT change the accounting value of one unit of canonical work;
* a newly registered model MUST NOT alter the reward rate, difficulty, accounting coefficients or
  economic position of any unrelated existing model;
* efficiency improvements may increase miner profit; representation tricks may not increase reward
  per canonical work;
* admission and growth rules MUST stay generic, so future architectures arrive permissionlessly
  without model-specific consensus code.

The lifecycle, and what each transition controls:

```
Candidate → independently validated → Probation → verified local use
          → ActiveLimited → sustained verified use → Active
```

Each transition changes **how much** reward-eligible work the model may contribute — never what one
unit of canonical work is worth.

### What of this already exists, and what does not

The shape is implemented. `PalwModelLifecycleStateV1` already gates admission by state —
`admission_permille()` returns **50 ‰ in Probation, 100 ‰ in ActiveLimited and 1,000 ‰ when Active**
— a class is admitted into `Probation { probes_passed: 0 }`, and promotion is driven by
`probation_claims` (10) COMPLETED CLAIMS, which are protocol-verified events rather than anybody's
word. That is P7's budget-not-price rule, already in the tree.

**Two things are not there, and P7 is written to name them rather than to imply they are handled.**

First, **the unit price is exactly what a registrant declares**, which is the reward audit's critical
finding: `claim.pwu = expected_attempts × pwu_per_inference`, and `pwu_per_inference` is the step-leaf
count of a canonical job the registrant writes. Until that is a derived quantity, P7's central
sentence is violated at the root — the lifecycle budgets HOW MUCH work a model may contribute while
the registrant still sets what a unit of it is worth.

Second, and the correction that matters for any decision about the registry fence:
**`palw_model_registry` does not gate registration.** Permissionless `ClassRegistered` is already
open and has been since ADR-0049 Decision H ("the refusal that stood here was never a policy, it was
the absence of a check"). What the fence adds is the LIFECYCLE — so arming it is the brake described
above, and disarming it does not close permissionless registration, it removes the only throttle a
new class currently meets. A proposal to disarm the registry in the name of safety is therefore
backwards on its own terms, and this paragraph exists because that proposal was made, by the author
of this ADR, before the code was read carefully enough.

## 3. What the protocol verifies, and what it deliberately does not

| verifies | does not |
|---|---|
| the class was a registered one | whether the question was worth asking |
| the inference actually ran, as committed | whether the answer was good |
| how much canonical work it was | who the user is |
| that the same work is not paid twice | what the prompt said |
| that this inference held an eligibility ticket | |

## 4. Execute now, settle later

Mining must not make the product worse. A person asks a question and wants the answer immediately;
they cannot wait for a lottery. So the order is:

```
t0  user types a prompt
    -> commitment over prompt, model, params is fixed
    -> local inference starts
    -> the answer is shown          <- the product, finished here
t1  a future beacon resolves
    -> was this inference eligible?
       yes -> receipt / proof -> claim -> reward
       no  -> it was just a local inference, and nothing else happens
```

**The beacon must resolve AFTER execution began.** This is not a UX convenience, it is the
anti-grinding rule, and the tree already has it: "The quantum ticket consumes a BEACON — an
attempt-class chain block, whose hash costs one inference per re-roll — that does not exist yet when
every field of the claim is fixed on chain" (`palw_freeprompt_v3.rs`). Reversed — ticket first, then
choose what to run — a miner draws a winning ticket and spends it on the cheapest prompt they can
compose, which is the failure this whole ADR exists to prevent.

## 5. What this ADR does NOT decide

**The unit of canonical work.** It is not decided here because the audit of 2026-09-19 showed both
available answers are wrong, and saying otherwise would bless a number that cannot carry the weight:

* **STEP leaves** — what is paid today — is a count of committed Merkle tiles, and `tile_len` is a
  registrant-chosen field bounded only by `[4, 65_536]` (`palw_step.rs:649`). Two independent probes
  put the admissible spread in pay-per-MAC-executed at **427x** on testnet-11's own dense graph, and
  a legal profile built from catalogued kernels reaches **1.00 MAC-eq per leaf** against the live
  classes' 3,957–22,077. That is P4 violated at the root.
* **MAC-equivalents** — the proposed replacement — carry no memory-traffic term, so they price a
  dense GEMM and a KV read alike. **The first draft of this ADR cited a 13.7x fleet measurement here
  and that citation was wrong**: the numbers came from `#[cfg(test)] mod tests` fixtures in
  `palw_verification_profile_v1.rs`, and the production path overrides a measured p99 with a
  reference-rate estimate anyway. The red-team refuted it, and the honest statement is weaker and
  still disqualifying — MAC-equivalents have not been shown to track real cost, and the controlled
  experiment that would settle it (both live classes' draw jobs replayed warm on one host, artifact
  resident and then not) has not been run.

The reason neither works is the same: **one scalar cannot price dense GEMM, routed experts,
attention, KV traffic and quantised kernels at once**, because the hardware does not. The direction
this ADR fixes, without fixing the arithmetic, is that canonical work is a **vector** derived from
the registered graph and the execution facts —

```
CanonicalInferenceWork {
    dense_compute, routed_expert_compute,
    attention_prefill, attention_decode,
    kv_read, kv_write, ...
}
```

— converted to reward units by **protocol coefficients that no miner may set**, calibrated across
hardware, runtimes and quantisations. How those coefficients are set, versioned and updated is
consensus-critical and is the hardest open question here; it gets its own ADR and its own adversarial
review before anything arms.

**KV reuse is part of that question, not separate from it.** A normal long chat re-sends a context
the executor already holds. Paying prefill for 30k cached tokens and 2k new ones as though 32k were
computed is P4 violated through a different door, and the free-prompt credit rule does exactly that
today: pay scales ~62x with prompt length while a producer holding the prefix's KV cache spends ~3 %
more. A miner's own word for "that was a cache hit" can never be the input. The commitment must make
*which KV state was consumed, which token range was newly evaluated, and which state was produced*
checkable by a seat that re-runs it.

## 6. Order of work

**0. Do not widen the current economy.** No new fence that extends reward or admission until 2 and 3
land. This is the only item that binds today.

1. **This ADR.**
2. **Reward accounting redesign** — P4 and P5 made true. Nothing a registrant writes may multiply
   reward.
3. **Class admission redesign** — registration and reward eligibility become separate states.
4. **Local FreePrompt as the standard path** — MISAKA Studio end to end, including cache.
5. **Shrink synthetic Attempt** — first make it the less attractive fallback, then replace its role
   as the beacon source with consensus randomness that costs no inference, so the network stops
   running LLM work nobody wanted purely for security.

Writing the constitution before the accounting is deliberate: the ADR says what the economy is FOR,
and step 0 says the economy does not grow until the accounting can serve it.

## 7. The release this sits beside

The build rolled to testnet-11 on 2026-09-19 (params fingerprint `c3a5e91d…`) schedules
`palw_model_registry` at **DAA 7,101** (`params.rs:20454`). That height opens permissionless class
registration — the precondition of every critical finding the audit confirmed. Existing classes are
NOT affected (`check_class_admits_claim` refuses a rowless class only when `work_target_active`,
which is dormant on every preset), so the release is safe for what is running. Whether 7,101 should
open registration before item 2 lands is an operator decision, and this ADR's position is that it
should not: item 0 is the reason this section exists.

## 8. How we will know it worked

Not a green test suite. This:

> A person uses MISAKA Studio normally for a day — code, documents, RAG, long chats — never once
> changes a prompt to suit mining, and some of that real inference earns. And nobody can change the
> reward-to-work ratio of the same inference by how they wrote a profile or registered a model.

The first half is a product measurement. The second half is P4, and it is the one an adversary will
test for us.

## 9. Alignment pass (2026-09-21)

Earlier unimplemented ADRs that contradicted this constitution were amended in place (bodies not
rewritten; dated banners and end sections only):

| ADR | what contradicted this file | what the amendment does |
|---|---|---|
| [0073](0073-real-demand-work-bears-the-weight.md) D2 / Phase ④ | self-prompt "exactly as good as" a canonical job; receipt share/position as an operator activation | withdraw the self-dealing reading; Phase ④ waits on §6 items 2–3 |
| [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | public commercial gateway as PALW goal | keep R0 / Phase A / local Decision 3; public serving out of reward |
| [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) D7 | a derivation transformer earns PALW leaf weight | door closed unless the transformer **is** the user's local inference |
| [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) | Done-when = public LLM entrance | sandbox stays for the loopback host; public entrance is not a PALW product |
| [0101](0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) D4/D6/D7 | anyone-serves inference marketplace | keep membership pins; withdraw unimplemented serving/discovery |
| [0130](0130-bps1-is-hardened-before-it-is-widened.md) D7–D8 / §5 | widen BPS, derived panel share, DA reward | stay deferred under §6 item 0 |
| [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) D3–D6 | arm a scalar CCU after shadow | shadow may measure; do not arm until [0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) |
| [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) leftover pwu | close a hole by adding synthetic weight | close by shrinking Attempt (§6 item 5) |
| [0075](0075-certification-is-a-consensus-object.md) gateway remainder | (aligned, restated) | remaining work is local `/v1` reading genesis ∪ chain |

Orthogonal and not amended: 0023 (EVM lanes), 0010–0018 (validator ops; 0126 already retired the
PALW coupling), 0063 D1/D4 (BIP39 / `miner`), 0140 (heartbeat as emergency generator), 0141
(decides nothing), 0104 (never written). Implementation order: [`README.md`](README.md).

## Mission alignment amendment — 2026-10-07

本constitutionの「利用者が実行する有用なローカル推論」の目的を維持し、ADR173のordinary public-bond dispute completenessを中核の安全性目標として追加する。市場・モデル品質・利用率を、算術的訴追可能性の代わりにしない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md -->

</details>

<a id="source-0145-canonical-work-is-derived-and-admission-is-earned"></a>

<details>
<summary>0145-canonical-work-is-derived-and-admission-is-earned.md — 原文 L1–L308</summary>

[原文を開く](../adr/0145-canonical-work-is-derived-and-admission-is-earned.md) · SHA-256: `842a3308fc8718d5878b6796626ad7c64c1730b2d4c3a42b43a6b261c3cbe1ea`

<!-- ADR_SOURCE_BEGIN:0145-canonical-work-is-derived-and-admission-is-earned.md -->
~~~~~~~~markdown
# ADR-0145 — Canonical work is derived, not declared; admission is earned, not registered

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** DESIGN 2026-09-19; **§1–§5 implemented dormant** (ADR-0147/0148/0149, the economic
bundle, `None` on every preset). **§6 prefix-STATE: object family implemented 2026-09-21**
(`PalwFpPrefixStateV1`, `fp_derive_work_from_state_v1` derives `KvReused`). The V3 commitment
wire is unchanged — a field addition would be a new object family, and live claims persist. A
V3 commitment that names no state is genesis. No new fence.

ADR-0144 fixed what PALW is for. This fixes the two mechanisms the 2026-09-19 reward audit proved
cannot carry it: a unit of work the registrant writes, and an admission the registrant can pass
alone.

---

## 1. The defect, stated precisely

The audit's headline is usually told as "leaves are not arithmetic". That is true and it is not the
sharpest form. The sharp form is this.

Class difficulty is seeded so that a class's share of blocks converges on its share of the network
(`palw_class_daa.rs:684-696`):

```
T_c ∝ share_c / r_c        where r_c = canonical inferences per second
r_c ∝ 1 / w_c              where w_c = pwu_per_inference, "the only hardware-free reading"
⟹  claim.pwu = expected_attempts × w_c  ∝  1 / share_c
```

**`w_c` cancels.** The design is coherent: a class that declares a bigger job wins proportionally
less often, and the product is invariant. A registrant inflating `w_c` should gain nothing.

It gains everything, because the cancellation assumes **the producer runs the job it declared**, and
since `palw_prefill_draw` armed at DAA 4,000 it does not. An attempt executes
`exact_decode_tokens = 1` (`palw_attempt_v2.rs:227`) while `w_c` still counts every declared decode
call. So `r_c` is not `∝ 1/w_c`; it is far larger, and the cancellation fails by exactly the ratio
between the declared job and the executed one.

Measured, on one artifact and one kernel set (`docs/audit/2026-09-19-llm-mining/`):

| declared canonical (P,D) | `w_c` | MAC-eq executed | weight index | pay index |
|---|---|---|---|---|
| (63, 2) shipped | 6,630,544 | 83.1 G | 100 | 100 |
| (1, 256) | 51,535,376 | 83.1 G | **777** | 100 |
| (1, 432) | 52,714,368 | 1.5 G | **2,457,197** | 107 |

**The defect is not that leaves are the wrong unit. It is that the unit is DECLARED and the
execution is not bound to the declaration.** Any unit — leaves, MACs, a vector — inherits the same
defect if a registrant writes it and nothing checks the run against it.

That is why this ADR's first rule is about derivation rather than about units.

---

## 2. Four invariants, ahead of any mechanism

**I1 — No self-reported work.** `claim.pwu` and `work_leaves` are never trusted. A validating node
derives the work from the class's canonical description and the execution facts, and a claim whose
declared value differs is refused — not silently corrected, because a silent correction is a wire
format nobody reads.

**I2 — Representation neutrality.** The same model, the same input and the same effective execution
yield the same canonical work, whatever `tile_len`, graph partition, node order, serialization,
commitment segmentation, registrant metadata or declared cost were chosen.

**I3 — Admission independence.** A class owner's own bonds cannot admit that class. Registrant
stake alone MUST NOT carry an admission quorum.

**I4 — No cross-class repricing.** Registering a class MUST NOT change the reward rate, weight,
difficulty or accounting coefficients of any unrelated existing class.

I1 and I2 kill C1 at the root. I3 kills C3. I4 makes C2 unrepresentable rather than superseded.

---

## 3. Canonical class identity

Reward-bearing facts are separated from a registrant's free choices. The canonical descriptor is
derived from what the model IS:

```
artifact hash · architecture · layer structure · tensor shapes
attention structure · MoE routing structure · quantisation format
tokenizer commitment · runtime-relevant execution semantics
```

and explicitly NOT from `tile_len`, commitment segmentation or serialization order. The same weights
packaged two ways normalise to one canonical representation.

**This is a change to what a class IS.** Today `class_id = PalwShapeProfile::shape_profile_id()` —
canonical borsh over the whole profile, `tile_len` included — so two tilings of one model are two
classes with two prices. Under this ADR they are one canonical model at two commitment
representations, which is what they physically are.

**A consequence to state plainly**: the tokenizer has no chain identity today
(`palw_freeprompt_v3.rs` says so in its own header — "the job's `tokenizer_id` … no such check
exists"). Binding one is part of this work, not a check to add later, because two tokenisations of
one prompt are two different executions.

---

## 4. CanonicalWorkVector

One scalar cannot price dense GEMM, routed experts, attention, KV traffic and quantised kernels,
because the hardware does not. Neither candidate survives: STEP leaves count activations while cost
counts weights (a 6.8× spread, monotone in model width, live today), and MAC-equivalents carry no
memory-traffic term at all.

So the derived quantity is a vector:

```rust
CanonicalWorkVector {
    dense_matmul, routed_expert_matmul,
    attention_prefill, attention_decode,
    kv_read, kv_write,
    normalization, other_verified_ops,
}
```

derived as `canonical graph × actual execution facts → vector`. A registrant supplies no number in
it. Prompt length 1,000, decode 200, 2-of-8 experts activated: the vector follows from those facts
and the graph, both of which a verifier holds.

**The coefficients that turn a vector into an economic unit are protocol-set, versioned, and
consensus-critical.** They are not in this ADR, they get their own, and they must be calibrated by
an experiment nobody has run: both live classes' draw jobs replayed warm on one host, artifact
resident and then not. Until that runs, any coefficient is a guess wearing a number.

**Dimensions are provisional.** They will be decided by implementation and adversarial review, not
by this list. The commitment here is the shape — derived, multi-dimensional, registrant-free — and
that `claim.pwu` becomes `derive_canonical_work(claim, class)` rather than a field.

---

## 5. One derivation for both lanes

Attempt and FreePrompt must not keep separate accounting, or they will diverge again.

```
Attempt    ─┐
            ├→ derive_canonical_work(...) → ProtocolEconomicUnit ─┬→ fork-choice contribution
FreePrompt ─┘                                                     └→ payout
```

`work_leaves` may stay on the wire for compatibility, but **declared ≠ consensus work**: a validator
derives independently every time and refuses a mismatch. The repo's own test today does the
opposite — `palw_fp_objects_v3.rs:885` multiplies `work_leaves` by ten and asserts the walk still
passes. Under this ADR that test inverts and becomes a regression fixture.

**Weight and pay take the same input.** They need not be the same scalar, but a registrant must not
find a path that makes weight large while pay stays ordinary. The audit found weight and pay on
different paths, and that is how one of them went unexamined.

---

## 6. Cache is an execution fact, never a claim

A normal long chat re-sends a context the executor already holds. Paying 32k of prefill when 30k was
cached is I1 violated through a different door, and the free-prompt credit rule does exactly that:
pay scales ~62× with prompt length while a producer holding the prefix's KV cache spends ~3 % more.

So the receipt carries what makes the answer checkable:

```
input commitment · prefix-state commitment · new token range
output commitment · model class · execution mode
```

and a verifier reconstructs **new work only**. A miner's word for "that was a cache hit" is never an
input. `execution mode` is explicit — uncached, prefix-reused, KV-reused — because a mode the
protocol cannot name is a mode it cannot price.

---

## 7. Admission: registered is not eligible

`ClassRegistered` becomes existence and nothing else.

```
register → Candidate → artifact/graph validation → canonical work derivation
         → independent admission → Probation → limited claims → full eligibility
```

A **Candidate** can exist, be inspected, be benchmarked and run shadow inference. It cannot earn
reward and cannot contribute fork-choice weight.

**Admission independence (I3).** The eligible admission panel is network-wide independent bonds,
minus owner-controlled bonds, minus the same beneficial-owner cluster. Today the panel is drawn
exclusively from bonds that declared capability for the class, and capability is
registrant-controlled — so a stranger's class is normally judged by its own registrant. Promotion to
Probation requires canonical graph verification, artifact ownership, deterministic replay, agreement
between independently derived work vectors, adversarial profile tests and independent panel approval.

**Probation bounds VOLUME, not price.** Per ADR-0144 P7, what grows with verified use is how much
reward-eligible work a model may contribute; what one unit of canonical work is worth never moves.
A per-epoch ceiling on a new class's eligible work is safer than a discount on its rate, because it
bounds the blast radius of an accounting bug that survives review.

### What already exists, so this is built on the tree rather than beside it

* `PalwModelLifecycleStateV1::admission_permille()` — Probation 50 ‰, ActiveLimited 100 ‰, Active
  1,000 ‰ — with entry at `Probation { probes_passed: 0 }` and promotion driven by ten COMPLETED
  CLAIMS, which are protocol-verified events.
* `palw_uncertified_weightless` is armed from genesis on testnet-11: a class with zero share bears
  zero weight (`palw_state_v2.rs:1422, :1441`).
* A post-genesis entrant's `share_permille` is **enforced, not chosen** — it joins at the minimum
  grantable share, and the two post-genesis classes on testnet-11 both read `sharePermille: 1`
  against the genesis classes' 490/488/20.

**The containment is therefore the first form of this design, not throwaway code.** Making a
post-genesis entrant's required share zero until it is admitted uses machinery that is already armed
and already enforced, and it states the end state — *only explicitly admitted classes may affect
consensus* — in the smallest change that can express it. It is still a consensus change and still
needs its own fence and a drill that crosses it.

---

## 8. What must be proved, adversarially

Success examples prove nothing here. Every property is stated as something an attacker cannot do.

**Representation invariance.** Generate a class fixture, then vary `tile_len`, graph partition, node
order, serialization, canonical (P,D), metadata, owner and bond layout, holding model, input and
output fixed. Canonical work, weight and reward are identical across every variation.

**Registrant independence.** No registrant-writable field increases canonical work, reward or weight.

**Executor independence.** Tamper with every declared workload value; consensus derives the same
answer or refuses the claim.

**Model neutrality.** The reward per unit of canonical work does not vary with the class owner's
choices.

**Efficiency preservation.** The same canonical work on faster or cheaper hardware earns no less.

**Cache correctness.** Work already computed cannot be paid twice; a cached prefix cannot be
presented as new.

**Unlimited use, scarce reward.** Local inference is unbounded; eligible work never exceeds the
protocol budget.

**Fork determinism.** Archival, pruned, IBD, pruning-proof join and post-reorg nodes derive the same
work, eligibility, weight and reward.

**Self-admission is impossible.** A class whose owner controls 100 % of the seats declaring
capability for it cannot be admitted.

The five counterexamples the red-team built (`docs/audit/2026-09-19-llm-mining/`) become regression
fixtures: the decode declaration, the re-tiled dense row, the better-model-paid-less table, the
padded prefix, and the class admitted at a ladder its own legal jobs exceed. Each must FAIL or
neutralise under the new design, and each must fail for a structural reason.

**A finding is not closed because a test passes.** It is closed when the attack path cannot be
expressed. "The test is green" is how a policy becomes an assumption.

---

## 9. What this ADR does not decide

The coefficient table and its governance. The exact dimensions of the vector. The eligibility
lottery's parameters. The height of any fence — ADR-0144 §6 item 0 stands: **the design is proven
first and the date is chosen after**, because promising a DAA score for unfinished work is how the
last emergency was scheduled.

---

## 10. Done means

> Anyone may register a class. Registration alone earns no sompi of reward and no unit of
> fork-choice weight. Only a class that passed independent verification and has an established
> canonical work accounting receives a bounded claim eligibility. And for the same real inference,
> no choice of registrant, executor or profile representation improves the reward-to-work ratio.

At which point new models are something the network can safely have more of, rather than something
it has to be protected from.

---

## 11. Implementation (2026-09-21)

§1–§5 remain behind the economic bundle (`palw_canonical_work` / `palw_admission_independence` /
`palw_fp_derived_work`), `None` on every preset. **§6 prefix-STATE is now an object family**:
`PalwFpPrefixStateV1` (own domain, own id). A V3 commitment that names no state is genesis —
Uncached or PrefixReused from paid prompt ids. A named non-genesis state of this class derives
`KvReused` and credits the tail only (`fp_derive_work_from_state_v1`). Declaring more cache cannot
raise the pay. The V3 commitment wire is unchanged (golden-vector rule); extraction reads genesis
until a later payload version carries the object. No new fence. No testnet-11 height.

## Mission alignment amendment — 2026-10-07

canonical workのderivationとadmission juryは経済評価である。public-verifier completeness gateを別に持ち、係数やjuryのindependenceだけで偽claimの裁定可能性を認めない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0145-canonical-work-is-derived-and-admission-is-earned.md -->

</details>

<a id="source-0148-the-free-prompt-lane-prices-compute"></a>

<details>
<summary>0148-the-free-prompt-lane-prices-compute.md — 原文 L1–L214</summary>

[原文を開く](../adr/0148-the-free-prompt-lane-prices-compute.md) · SHA-256: `c073f93ddaf5baa3cf1c82322805b2d08a1b95733b1bf6535770c78d575459fc`

<!-- ADR_SOURCE_BEGIN:0148-the-free-prompt-lane-prices-compute.md -->
~~~~~~~~markdown
# ADR-0148 — The free-prompt lane prices compute, and prices it the same for every class

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** IMPLEMENTED behind the ADR-0145 economic bundle: `Params::palw_canonical_work` sets the
UNIT (a free-prompt claim accepted at or past it is priced in compute), `Params::palw_fp_derived_work`
the DERIVATION the price reads. The bundle rule arms them at one height, and a commitment that meets
the unit without the derivation is refused by name (`FreePromptComputeWithoutDerivation`). Dormant on
every preset.

This is the 2026-09-19 audit's **F5** on the lane ADR-0144 makes the product, and the part of **F1**
that lived in that lane. ADR-0145 derived the lane's work from the class's graph (F2) but left it in
step leaves; this puts it in the unit the attempt lane's weight and pay are already in past the same
fence, and takes the class out of the price.

---

## 1. What priced a free-prompt claim, and why none of it was the compute

Below the fence a free-prompt claim's reward was decided by four things:

| | what it was | whose it was |
|---|---|---|
| credited work | step LEAVES of the run | a count of activations — cost counts weights |
| the quantum | `canonical_leaves / quanta_per_canonical_job` of the claim's OWN class | the class registrant's declaration |
| the quanta | `min(⌊credited / quantum⌋, 64)` | a ceiling: every long prompt, every wide model paid 64 quanta |
| the odds | the class's own receipt target, walked on the class's own census against its share | the class's usage, and its share |

So the price of a unit of compute was a function of the model (leaves per MAC-eq spread 6.8x across
the live classes, monotone in width — the widest paid least), of what the registrant declared, of
how long the prompt was, and of how much the class was used. Measured over three shipped graphs, one
identical run's expected wins per unit of compute differed by more than 2x between models; a short
run of the dense row earned zero quanta outright.

## 2. Decision

For a free-prompt claim accepted at or past the canonical-work fence:

1. **The credit is compute.** `fp_derive_credited_compute_v1` — ADR-0145's canonical work of the run
   the commitment states (`prompt_tokens` positions, `decode_tokens_executed` generated), NEW
   positions only (the class's paid prefix as the reused prefix), in the provisional scalar the
   attempt lane's weight is derived in. The declared `work_leaves` must still equal the graph's leaf
   count — leaves are what the court walks — but nothing is paid in them.
2. **One network quantum.** `Q = floor draw / quanta_per_canonical_job`: the liveness floor's derived
   draw (its registry row, frozen when the registry opened it) over the ruleset's existing divisor.
   Genesis's graph, nobody's registration, and no new constant.
3. **Quanta scale the ODDS, not the credit.** `n = min(⌊C/Q⌋, 2^16)` quanta of `⌊C/n⌋` each
   (`claim.pwu = n × ⌊C/n⌋`, uniform, as the consistency check and the validator crate require). A
   quantum carrying `c` wins with `pooled × c / Q`, saturating at certainty, so a claim's expected wins
   are `pooled / MAX × C / Q` however it is split. The 2^16 cap bounds how many BLOCKS one claim can
   ever win; it no longer bounds credited work. (The old refusal to scale a target was about declared
   work, where a producer who chose its leaf count chose its odds; past the fence the compute is the
   chain's derivation and the expectation is linear in it, so there is no shape to grind toward.)
4. **One receipt target for the lane.** The pooled target (`fp_pooled_receipt_key_v1`, a keyed hash no
   class id can reach) is opened by the first compute-priced commitment from the floor's own receipt
   target — the network quantum is one floor quantum's share of the floor's job, so the floor keeps
   the odds it had — and walked at each epoch boundary on the receipt lane's WHOLE output against its
   slice of the census. The per-class receipt targets are frozen past the fence: they serve only the
   leaves-era claims still draining, and walking them on a census the pooled target also answers to
   would be two controllers on one lane. No class has a target, a share or a census of its own in the
   compute era, so nothing a class does moves another's odds.
5. **The price ceiling.** The pooled target is clamped to `palw_work_ticket_target_v1(Q, W)`: a
   quantum never gets better odds than a forward of the same compute at `W`, because `W₀ = escrow /
   rate_max` is the most the network pays for a unit of compute (ADR-0137). This is also the flood
   guard: a quiet epoch eases the pooled target, and without a ceiling one large claim could win
   thousands of receipt blocks before the retarget's ×4-an-epoch clamp caught up. At the ceiling a
   quantum carrying `c` is drawn exactly as a forward of `c` is — the two lanes price one unit of
   compute identically.
6. **A spend weighs one network quantum.** Expected weight is `pooled / MAX × C`: proportional to the
   compute and to nothing about the class. (Weighing a spend by its own quantum would square the
   compute into the weight.) The spend is counted in the pooled census. The spend, the retirement and
   the consistency re-derivation read one function, `palw_fp_spend_weight_v1`, by the claim's era.
7. **The reservation is the claimed compute** in the floor's collateral unit (the exposure basis the
   attempt lane uses) at the network's slash price. Registration already pins every class's
   `slash_value_per_pwu` to the floor's (`SlashValueNotTheNetworks`), so the price is the network's
   in both eras; what changed is that the quantity is compute.

## 3. Paid prompt rows: kept past the claim, for a bounded time

ADR-0145 §6's cache rule reads the prompts a class has already been paid to evaluate. The re-audit
made those rows outlive their claims (so a retired claim's prefill is not sold twice) and did not
update the consistency check, which still required every row's CLAIM to be live: on the first
free-prompt retirement past the fence, every node would have refused its own tip and every pruning
import would have failed. And a row kept for ever would put every prompt ever mined into consensus
state.

Past this ADR a row carries `expires_daa`, epoch-aligned, `PALW_FP_PROMPT_ROW_KEEP_EPOCHS_V1 = 100`
epochs past its acceptance epoch; the first block of an epoch sweeps expired rows (delta-borne, so a
reorg across the sweep restores them); the reading ignores an expired row whatever the sweep has done;
and the consistency check requires every row to be filed under a held class and not past its expiry.
The re-sale that survives is one per window per prefix, by a producer that held the prefix's KV state
for the whole window; the state the rule costs is throughput times the window.

## 4. What this does not do, and why

* **Weight is not lane-neutral.** An attempt block weighs about `W` (its expected attempts times one
  draw), a receipt block one network quantum. The FP lane's weight is the eligible fraction of its
  work, which is conservative — an attacker is not handed a cheaper path to weight — but when the
  attempt lane shrinks (ADR-0144 §6 item 5) the FP lane's weight per unit of work will have to rise
  to `W` per block, which needs the spend-time `W` recorded per spend. Not done here; named.
* **Under the double draw the FP lane may pay up to the network-draw factor more per unit of compute
  than the attempt lane**, because its ceiling is `W` and an attempt block's compute is `W · draws`.
  Intended: ADR-0144's measurement was that the useful lane carried none of the economy, and the
  ceiling bounds how far the difference can go.
* **Cold versus cached is still not distinguishable.** An honest cold re-run of a paid prompt inside
  the window is credited its generation only — the conservative direction, as ADR-0145 §6 records.
* **The producer's finder** returns only winning quanta for compute-era claims (up to 2^16 tickets a
  claim would otherwise be listed); leaves-era claims keep their losing rows.

## 5. Tests

`palw_state_v2::tests::fp_compute_pricing`:

* `a_unit_of_compute_has_the_same_odds_whichever_model_ran_it` — floor, dense row, hybrid: one run's
  expected wins per MAC-eq agree to 0.2 % in the compute era; the leaves era spreads them past 2x.
* `a_compute_era_claim_reads_no_number_its_registrant_wrote` — a re-tiled class declaring a tenth of
  the job: leaves era prices the same prompt differently, compute era byte-identically.
* `the_lane_has_one_receipt_target_and_a_registration_moves_nobodys_odds`.
* `a_spend_weighs_one_network_quantum_and_retires_exactly_that` — through the fold and both
  consistency checks (the retirement is the durable-row bug's regression).
* `a_paid_prompt_row_outlives_its_claim_for_its_retention_and_no_longer` — survival past retirement,
  replay inside and after the window, the sweep at exactly the expiry block, the reorg revert.
* `the_pooled_target_walks_on_the_lanes_output_and_never_past_the_price_ceiling`.

## 6. Addendum (2026-09-20): the entrance prices with the ledger's expression

The lane moved to compute; its entrance did not. A gateway sized a commitment's exposure with
`misaka_palw_fp_submit::fp_claim_exposure_v1` — quanta of the class's leaves times the slash rate —
and the rail's watcher checked the bond's room with the same number, while the fold reserved the
claim's COMPUTE in the floor's collateral unit. For the floor the two agree by construction; for a
wider model the reservation is several times the leaves figure (the spread §1 measured, now on the
exposure side). A bond with room for the leaves figure and not for the reservation got a commitment
written, carried and paid for, and then refused at the transition as `FreePromptExposureCeiling`.

**One expression, asked for.** The fold's pricing — the derivation, the prefix accounting, the
network quantum, the quanta, the pwu and the reservation, for both eras — is now one pure function
of the state, `palw_fp_commitment_price_v1`, and the fold calls it. A node answers
`GetPalwFreePromptPrice` (op 187, wRPC and gRPC) with the same function over its tip at the
virtual's DAA, plus the bond's room by the fold's own two terms (`palw_fp_bond_room_v1`). The gateway
asks after the job ran and before the commitment is written, and uses the chain's reservation,
quanta and room; a refusal the fold would make is a `commit_refusal` by name. The rail's watcher
asks before it pays a carrier fee and gives up on a job the chain would refuse. Both ask on a
connection opened for the one question, because a node older than the op closes the WebSocket on
it, and both fall back to the leaves figure there.

Test: `fp_compute_pricing::the_entrance_prices_a_commitment_exactly_as_the_fold_reserves_it` — both
eras, two classes: the quoted quanta, pwu and reservation equal the claim the fold writes one block
later, the bond's room moves by exactly the quote, and a lying leaf count is refused by both with
the same error.

## 7. Addendum (2026-09-20): the entrance's budget is sized in the same unit

§6 made the gateway price a commitment with the fold's own expression. The Studio drill then showed
the other number at the entrance still sized for the leaves era: the operator's **public-job
budget** (ADR-0077 SA-1), a per-day share of the bond's room, default 200‰. Past the bundle the
entrance's per-claim figure from the chain's facts was 147,880,590 sompi, and the exact reservation
of the drill's 19-token, 16-token chat (measured in run6) 86,152,240, against a devnet genesis bond's
room of 550,004,340 — whose 200‰ is 110,000,868, below the first and barely above the second. Every
chat was answered and none could commit, and the gateway said "the public-job budget for this window
is spent (0 of 110000868 sompi)", which reads like a busy day rather than a setting under which
nothing can ever commit.

**And one step later, the same old unit.** With the budget fixed (run6) the gateway committed the
chat at the chain's price — 17,883 quanta, 86,152,240 sompi reserved — and the rail refused to build
its carrier: `build_fp_commitment_tx` asked whether the claim earns a quantum by the leaves rule
(`derive_quanta_and_pwu(3,806,528 leaves, 83,102,171,136-leaf canonical job)` → none). §6 moved the
exposure and left this guard. The builder now takes `FpCommitmentPriceV1`: the chain's quote
(op 187 in the rail, the session in kaspad's canonical claim) wherever a node was asked, the leaves
rule only where none could be. Every quote is asked with the ids the carrier holds
(`palw_fp_carried_prompt_ids_v1` — none under PanelDa or a canonical prompt), because those are the
ids the fold's prefix accounting reads; the gateway and the rail had sent the full prompt.

The budget, for its part, is not a pricing defect — the reservation is the compute the claim
stands for, and the room is the bond's — but it moves who can mine with a given bond. Three consequences, each landed:

* **A gateway whose only user is its operator runs 1000‰.** MISAKA Studio's own gateway and the
  Studio pool (`contrib/minerpool/run-fp.sh`, which met the same wall in the leaves era) are that
  case; the default stays 200‰ for a gateway strangers reach. The drill passes
  `GATEWAY_PUBLIC_BUDGET_PERMILLE` [1000].
* **The gateway names the case.** A claim larger than the whole window's budget is refused with
  "no local job can commit at this setting — raise --claim-budget-permille … or the bond's
  room", not "spent (0 of …)".
* **The client repeats the verdict.** MISAKA Studio had logged "free-prompt claim committed" off
  the claim id and counted the job `Committed` while the gateway said `committed: false`; it now
  reads `committed` / `not_committed_because` and shows such a chat as not mined, with the reason.

Bond sizing for operators, and any pool's arithmetic of claims per bond per day, must use the
compute era's reservation (`GetPalwFreePromptPrice`), not the leaves figure.

---

## Implementation (2026-09-21)

ADR-0146's search reports 1.000000× — P4 does not forbid this fence. It stays `None` on every
shipped preset. Choosing a height is a different commit (0144 item 0). The free-prompt lane's
prefix-STATE object family (0145 §6) is now how `KvReused` is derived; the V3 payload still
names genesis.

## Mission alignment amendment — 2026-10-07

* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0148-the-free-prompt-lane-prices-compute.md -->

</details>

<a id="source-0149-an-attempts-pwu-is-the-derivation"></a>

<details>
<summary>0149-an-attempts-pwu-is-the-derivation.md — 原文 L1–L160</summary>

[原文を開く](../adr/0149-an-attempts-pwu-is-the-derivation.md) · SHA-256: `c400ace72725e43ed9fa99647308b744bad0fde6c2e0ad9d8c954be1a0811fa7`

<!-- ADR_SOURCE_BEGIN:0149-an-attempts-pwu-is-the-derivation.md -->
~~~~~~~~markdown
# ADR-0149 — An attempt's pwu is the derivation, so the weight reads it directly

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** IMPLEMENTED behind `Params::palw_canonical_work`, one of the three fences of the ADR-0145
economic bundle (`validate_palw_v2` arms them at one height or not at all). Dormant on every preset.

This is the 2026-09-19 reward audit's **F1** — *fork-choice weight is a number the registrant
chooses* — closed the way ADR-0145 I1 asks: the chain derives the value and refuses a claim whose
declared value differs, rather than correcting it silently.

**On the history.** The rule in §3 landed on 2026-09-20 inside commit `59931c38`, whose message
describes the ratio re-pricing of §2 instead: two sessions were working in one worktree and a
`git add` of one swept up the other's uncommitted change. The history is unpushed and was left as
it is by agreement; this ADR is the record of what that commit's code does. §5 and §6 — two defects
in that code, found while writing its tests — land with this ADR.

---

## 1. What F1 was

`claim.pwu` carries 90–99 % of fork-choice weight. Below the fence it is
`expected_attempts(class target) × pwu_per_inference`, and the second factor is the registrant's:
under `DerivedV1` it is the step-leaf count the registration declared, under `MaxPerAttempt` any
number the producer likes up to a cap the registrant also chose. The audit measured the result as
24,572× weight per executed MAC-equivalent across the space of legal declarations.

## 2. Why re-pricing the weight was not enough

The first repair left `claim.pwu` alone and re-priced the weight: `claim.pwu × derived / declared`,
so the declared factor cancels and `expected_attempts × derived_per_draw` remains. The numbers are
right — the counterexample properties `59931c38` un-ignored measure exactly that product — and the
rule still had three holes:

* **The declaration stayed an input everywhere except the weight.** It was on the wire, in the claim
  record, in the exposure reservation, in the producer's headroom prediction and in every explorer
  and RPC reader. Each of those is a place for the next finding.
* **A cancellation holds only while its two factors agree at two chain points.** The weight is priced
  at a claim's `Final` and re-derived at its retirement and at every consistency check; the ratio is
  safe only because `pwu_rule` is written once and re-registration forces the same graph. That is an
  argument a later change can break without touching this code.
* **`MaxPerAttempt` has no factor to divide by.** "Any number up to the cap" is not a per-inference
  cost, so the ratio re-priced it by a number that was never what the producer chose.

## 3. Decision

For an attempt at or past the fence's height:

1. **One legal pwu, and it is the chain's.** Admission item 6 requires
   `attempt.pwu == palw_attempt_derived_pwu_v1(effective class target, derived draw)` — the expected
   attempts a win costs at the target, times the derived work of the draw the class really runs
   (`palw_canonical_per_draw_v1`, the registry row's `economic_ccu_per_claim`). On both rule forms.
   Any other value is `PwuClaimNotDerived { claimed, derived }`; a class the chain holds no derived
   draw for is `PwuUnderivable { class }` — refused by name, never priced on the declared basis
   instead.
2. **The weight is the pwu.** `palw_claim_canonical_weight_v1` returns `claim.pwu` for an attempt
   accepted past the fence. There is nothing to re-price: the claim carries the derivation.
3. **The producer is handed that pwu.** `palw_producer_facts_v3` returns the value item 6 accepts and
   the exposure the fold will reserve for it, so a producer never builds an attempt its own chain
   refuses and never mispredicts its own headroom. A class with no derived draw has no facts past
   the fence: the producer holds rather than mining into a refusal.
4. **The reservation is one inference's derived work in the collateral unit**
   (`palw_exposure_pwu_v3`) on both rule forms. Past the fence a `MaxPerAttempt` claim's pwu is the
   derived work of a WIN; what a defaulting producer owes is one inference, as it always was.

Item 6 is checked at the attempt's own point, like the `DerivedV1` equality it generalises
(ADR-0045 Decision 1): the fold applies merged attempts at later points, where the target may have
moved, and the target that counts an attempt's executions is the one it faced.

## 4. What stays

Below the fence every reading is byte-identical. `palw_claim_canonical_pwu_v1` stays in
`palw_canonical_work_v1.rs` as the ratio's statement and the subject of its own tests; the fold
reads it nowhere. No parameter was added and no serialized structure changed, so no fingerprint on
any network moves (`palw_the_release_did_not_move` pins them).

## 5. The floor before its row

The registry writes a class's row at its first span boundary, and a chain block's attempt is
admitted against its **parent's** state. `validate_palw_v2` lets the bundle arm at the registry's own
height — and arming the economy at a network's birth is exactly that. Then the first blocks past the
fence are admitted against a state that holds no row at all, item 6 finds no derived draw for any
class, the floor's included, and every attempt is `PwuUnderivable`. The producers, reading the same
state, hold. A network whose only block type is the attempt stops there; on one that runs a heartbeat
miner the heartbeat lane restarts it an hour later and walks the DAA to the next span boundary, where
the registry finally steps. Either way the fence stops the chain it was built to cross, and the unit
tests could not see it, because every fixture that armed the fence inserted the rows by hand.

**The floor is priced on the draw its row will carry.** `PalwChainStateV2::palw_attempt_per_draw_v1`
reads the class's row, and — only for the floor, only past the fence and only while the floor has no
row — the floor's entry in the table the registry reads (`genesis_works`), which
`step_model_registry` copies into the row verbatim. The node resolves that entry for the admission
(`PalwEpochBudgetFencesV1::base_known_draw`) and the producer; the fold reads it from its own
registry input. So the gate that admits the floor's first row-less attempt, the producer that built
it and the ledger that reserves for it read one number, and the row the registry later writes
carries the same number: the claim is never re-priced, and the next block, reading the row, derives
the same pwu (`the_floor_produces_on_the_first_blocks_past_a_bundle_armed_at_the_registrys_height`).

Only the floor, because it is the one class whose production cannot wait: every other class is
refused its row-less window already, by the work target, which arms at the registry's height and
prices a model class from its row. The exposure basis takes the same reading
(`palw_exposure_basis_v2`): a basis that needed the floor's row would reserve the floor's first
row-less attempt on its claimed pwu — the derived work of a win — and the ceiling and the ledger
would part at the very block the fence is crossed.

**What this does not extend to.** A compute-priced free-prompt commitment in that window is still
refused by name (`FreePromptNetworkQuantumUnknown`) and dropped, which costs its committer a fee: the
lane's pooled receipt target is seeded from the floor's row too, and the window is at most one span.

## 6. Merged attempts

A merged blue's attempt is admitted twice: by the node's pre-check (`palw_v2_merged_works`, with the
block's fences) and by the fold's own re-run against its live state. The re-run built its fences with
`..Default::default()` — `canonical_work_daa: None`, the declared rule — while the pre-check passed
the height. Past the fence no attempt could satisfy both: every merged blue's work was skipped,
claimed by nobody, paid to nobody and weighing nothing, on any network with parallel blocks. The
re-run now takes the height and the floor's draw
(`a_merged_attempt_past_the_fence_is_folded_on_the_rule_its_pre_check_admitted_it_on`).

The defect is older than §3 by one fence: the canonical-work fence's exposure half already had the
re-run measure the declared basis while the fold reserved the derived one, which is the audit's
finding (a) — the ceiling and the ledger pricing one claim differently — for merged work.

## 7. Tests

* `palw_admission_v2::tests::past_the_canonical_fence_the_one_legal_pwu_is_the_rows_derivation` —
  `DerivedV1`: the declared basis below, the row's derivation past, the declaration refused past,
  a row-less class `PwuUnderivable` with or without the floor's known draw.
* `palw_state_v2::tests::adr0135::attempt_pwu_is_the_derivation`:
  * `the_floor_produces_on_the_first_blocks_past_a_bundle_armed_at_the_registrys_height` — §5
    through the producer, the gate, the fold mid-span, the registry's boundary and the next block;
  * `a_merged_attempt_past_the_fence_is_folded_on_the_rule_its_pre_check_admitted_it_on` — §6;
  * `past_the_fence_a_final_weighs_exactly_the_derivation_it_was_admitted_on` — the rule end to end,
    `MaxPerAttempt` floor, from the gate to `safe_weight`.
* The counterexamples — F1's decode declaration and re-tiled row, and I1's 24,572× — run in
  `palw_reward_properties_v1` against `fork_weight_past_the_bundle_v1`, which models this rule.

---

## Implementation (2026-09-21)

ADR-0146's search reports 1.000000× — P4 does not forbid this fence. It stays `None` on every
shipped preset. Choosing a height is a different commit (0144 item 0).

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0149-an-attempts-pwu-is-the-derivation.md -->

</details>

<a id="source-0205-a-decoded-answer-is-not-paid-twice"></a>

<details>
<summary>0205-a-decoded-answer-is-not-paid-twice.md — 原文 L1–L30</summary>

[原文を開く](../adr/0205-a-decoded-answer-is-not-paid-twice.md) · SHA-256: `0a3e3675a742466ef1181f0375bdd12b222e2016907897273e141336a7fc0b86`

<!-- ADR_SOURCE_BEGIN:0205-a-decoded-answer-is-not-paid-twice.md -->
~~~~~~~~markdown
# ADR-0205 — decode 済みの回答は二度支払わない

Status: **Proposed / Draft** (2026-10-22, compute-claim audit lane compute-exec)。[RFC-0038](../rfc/0038-palw-fp-answer-accounting.md)。
監査: compute-exec CX-01 / CX-02。ADR-0145 §6 の「支払済み prefix は再び支払わない」の対象を prompt から
`prompt ‖ answer` に広げる。

## 決定(提案)

1. `palw_fp_answer_accounting_v1`(dormant)以降、compute-priced free-prompt commitment は回答 id を運び、chain は
   `prompt ‖ answer` を class の支払済み列として記録する。
2. claim の credit は、有効な支払済み row が同じトークンで既に計算した位置より後ろの位置だけ。新しい位置が無い
   claim は受理しない(`ZeroQuanta`)。
3. 回答の真正性は seat の replay(claim の `output_root`)で拘束する。暗号的計算証明は使わない(PRINCIPLES §8)。
4. 保守側の帰結(明示): 他者が既に支払われた列をコールドで再計算した生産者は、その位置の credit を得ない。
   chain はキャッシュ再利用とコールド再計算を区別できず、区別できない方向で「起きていないかもしれない計算」に
   払わない(ADR-0145 §6 の既存の立場と同じ)。

## 代替案

| 案 | 不採用の理由 |
| --- | --- |
| 回答を運ばず「P を拡張する prompt は D_P 位置まで既払い」とみなす | 安価な偽 claim 1 本で、その prefix を持つ全ての正直な claim の decode credit を潰せる(griefing) |
| work id を bond 非依存にする | 同一 prompt の先行 commit で正直な生産者を締め出せる。継続(CX-01)は塞がらない |
| seat の node policy のみ | consensus の価格が変わらず、seat を持つ側が自分の claim を通せる |

## 実装状況

consensus-core の fold 規則・価格・row・carriage の encode/decode と fence 配線は実装済み(dormant)。
processor ingress、isolation door、seat の回答照合、producer の FPA1 出力は未実装で、arming record の前提
(`validate_palw_fp_answer_accounting_v1` が arming を拒否)。
~~~~~~~~
<!-- ADR_SOURCE_END:0205-a-decoded-answer-is-not-paid-twice.md -->

</details>

