# ADR-v2-0018 — 長文脈profile・公開計測・context有効化

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 6本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0003](../rfc-v2/0003-model-onboarding-and-artifacts.md) / [RFC-v2-0005](../rfc-v2/0005-bounded-resources-and-long-context.md)

## 1. この文書の役割

長文脈modelを「動く」と呼ぶ前に、admission・裁定・時間・搬送のどの上限に当たるかを示す。

## 2. 統合した判断の読み方

### 2.1

reachableなstepを裁定できること、model-fit判定を同じadmission述語から導くこと、公開vectorで再現できることを対応づける。[旧ADR-0070](../adr/0070-the-model-tiers-step-spaces-are-adjudicable.md) [旧ADR-0097](../adr/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) [旧ADR-0110](../adr/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)

### 2.2

step ladderをgenesis／rulesetで固定する判断は裁定空間の話であり、tokenの発行ladderではない。後続のheld-context方式によるcarrier budget側への変更も追う。[旧ADR-0092](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) [旧ADR-0103](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)

### 2.3

held regimeの有効化高さ・prompt form・classごとのladderをidentityとchain stateに結び付ける。大きなcontextの宣言だけでadmissionを通さない。[旧ADR-0118](../adr/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) [旧ADR-0119](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)

## 3. 維持する境界・後続改定

旧ADR-0092とADR-0103のladder基準は適用方式が異なる。原文のfixture試験と本物の大規模モデル・公開networkの検証を区別する。

## 4. 隣接する判断との分担

通常メモリ最適化は[ADR-v2-0017](0017-model-streaming-and-memory-bounds.md)、2M shard計画は[ADR-v2-0030](0030-sharded-verification-and-model-measurement.md)、長文脈courtの新案は[ADR-v2-0041](0041-long-context-court-decomposition.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0070-the-model-tiers-step-spaces-are-adjudicable.md](../adr/0070-the-model-tiers-step-spaces-are-adjudicable.md) | L1–L206 | L15–L21 |
| [0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) | L1–L296 | L15–L25 |
| [0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md](../adr/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) | L1–L462 | L15–L35 |
| [0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md](../adr/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) | L1–L395 | L15–L28 |
| [0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md](../adr/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) | L1–L285 | L15–L35 |
| [0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) | L1–L244 | L15–L30 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0070-the-model-tiers-step-spaces-are-adjudicable"></a>

<details>
<summary>0070-the-model-tiers-step-spaces-are-adjudicable.md — 原文 L1–L206</summary>

[原文を開く](../adr/0070-the-model-tiers-step-spaces-are-adjudicable.md) · SHA-256: `4437a88ff1b19adb32c59467d2e04055b76b48216c141aec93fc27275d8fe668`

<!-- ADR_SOURCE_BEGIN:0070-the-model-tiers-step-spaces-are-adjudicable.md -->
~~~~~~~~markdown
# ADR-0070: The model tiers' step spaces are adjudicable — end to end, and proven by sweeping them

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: ACCEPTED (implementation on `palw-step-space-e2e`; arming is a deployment decision, §7)
Date: 2026-09-01 (numbered 0070 on merge: ADR-0069 was taken by the
launch audit's `e2e-adjudicability-is-the-price-of-weight`, which the consensus
implementation cites throughout — this one moved because only one citation had to.)
Relates: ADR-0030 (the step space), ADR-0038 A4 (coverage), ADR-0049 (canonical IR, Decisions B/C/F/G),
ADR-0052 (the hybrid's integer arithmetic), ADR-0053 (one execution family), ADR-0067 (classes are
chain data), ADR-0068 (the LLM-primary economy)

> **Follow-on (index reconciliation, 2026-09-02).** The arming §7 leaves to deployment happened:
> the corrected classes — the A16 graph-v2 row and the QWEN36 graph-v3 row — are the ones
> testnet-11's genesis registers from Relaunch 5 on, both certified end to end and weight-bearing
> under [ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md). Since
> [ADR-0075](0075-certification-is-a-consensus-object.md) the certificates these step spaces make
> possible ride the chain as `FamilyCertified` objects rather than build pins, and
> [ADR-0076](0076-the-attempt-lanes-seed-is-the-retargets-equilibrium.md) seeds each tier's
> attempt-lane target from its own `pwu_per_inference`. Map: [`README.md`](README.md).

## 1. The problem, stated the way the chain would have met it

ADR-0068 hands the two model tiers ≈98 % of blocks and issuance. On the day that table armed,
neither tier's claims could be prosecuted:

* **Neither model backend emitted a step space.** `execute` committed a bespoke composite
  (`keyed(context, shape_id, trace_root, output_root)`) in the `execution_root` slot — the qwen36
  module said it in so many words: *"here there is no step leg yet"*. `check_execution_root_binding`
  compares a close against `binding.committed_execution_root`, so every close on a model-tier claim
  died at `ExecutionRootMismatch` before any arithmetic was read. Both backends answered
  `supports_court() == false`; a dispute could not leave round 0 whichever party was honest.
* **The retained material discarded the evidence.** Logits rows and ids only — no binding, no
  tiles, no state chunks. A producer could not have answered a rung about its own execution.
* **The court's front door refused the tiled scheme.** `check_job_context_shape` admitted only the
  float event-tree scheme id, so every model-tier binding was `SchemeMismatch` before its first leaf.
* **A dozen arm/registration mismatches**, each structurally sufficient to make honest material
  `Unadjudicable` (the I10 freeze arm — a frozen class as the reward for being audited). §4 lists
  them; every one was found by the sweep, not by reading.

The registered v1 descriptors cannot close these gaps: the v1 A16 profile declares a one-byte state
map over an `i32` cache and omits a narrowing its engine performs; the v1 hybrid table carries a
phantom node no execution fills. Their ids are frozen chain facts. The corrected classes
(`qwen25_a16_profile_v2`, the qwen36 `graph-v3` row) are where court capability lands — which is
what ADR-0067 built them for.

## 2. Decision — the acceptance test IS the property

**A model tier is adjudicable when every leaf of a captured attempt adjudicates.** The landed
acceptance tests state it as code:

* `every_a16_leaf_adjudicates_and_a_tampered_one_convicts`: one captured A16 attempt; for EVERY
  leaf of its step space the backend's own prover assembles the refutation, the backend's own
  inventory answers the operands through real Merkle openings against its root, and the court
  finds no fault; then one tampered lane convicts at representative kernels — a decode call (the
  tiled pin) and a checkpoint-anchored attention step (the v2-map geometry) included.
* `every_qwen36_leaf_adjudicates_and_a_tampered_one_convicts` — the hybrid twin, over the RC's
  canonical job shape: ~1,900 leaves, every one `NoFaultFound`, five tampered leaves convicting
  (embed, a GDN recurrence head, a routed-expert tile, a decode call, the last leaf); plus
  `every_qwen3moe_leaf_adjudicates_and_a_tampered_routed_tile_convicts` for the all-attention
  flavor. Between them every arm of both layer kinds is exercised — recurrence, convolution,
  router, routed experts, combine, shared expert, gated attention.
* `a_real_bisection_converges_on_the_tampered_leaf_and_the_close_convicts`: the composition
  nothing had — a real ladder driven by the two parties' own materials converges on exactly the
  injected-fault leaf, the close passes the cost gate, proves operands against the class root,
  and convicts; the honest direction clears through the identical machinery.

Sampling was rejected deliberately: every defect in §4 was invisible at tile 0, leaf 0, or the
first position, which is where a sampled test looks.

## 3. The commitments a model-tier claim now carries

`execute` for a court-capable class runs the captured walk (the A16 engine's traced/planned
forward; the hybrid's planned interpreter — one committed row per declared node), tiles every row
at the node's own `tile_len` into the canonical leaf space, and commits:

* `execution_root = binding.committed_execution_root` — the step binding's own composition
  (logits leg ‖ activation leg ‖ checkpoint leg ‖ step leg), exactly what the court pins.
* `trace_root` = the TILED logits root over the selecting rows (one per generated token).
* material = the family codec (`binding, tiles, logits rows, ids, checkpoint chunks`) — the one
  encoding the floor already used, so a seat rebuilds the legs and reproduces the roots.

Checkpoints: the A16 class captures under its registered four-byte map at the family interval.
The hybrid registers NO map (its recurrence is genesis-anchored by declaration); its canonical
cadence is the empty leg — `interval = n_ctx`, above every legal job's decode-call count, so
`decode_calls / interval` is zero and the sentinel map is never asked to chunk anything.

Capability follows the class, not the binary: the v1 A16 profile keeps the legacy composite and
says `supports_court() == false`; the hybrid's court capability rides exactly the constructor that
proves servability (`from_registered_profile` — the plan is the proof, ADR-0067).

## 4. The consensus changes, and why they are one version

`PALW_V2_TRACE_FORMAT_VERSION` moved 2 → 3. Every item below changes what a court concludes about
an object a v2 court also concludes about — two verdicts, one object, which is a fork — so they
ship as one version and two builds refuse each other at the handshake instead:

1. `PalwDecodeTokenPinV1::TiledV1` — a tiled class pins its generated ids through the rows-tree
   root (one selecting row at a model vocabulary is hundreds of KiB against the ~80 KiB close).
2. `verify_kv_anchor` derives the anchor geometry from the class's REGISTERED `state_chunk_map_id`
   (four-byte map included) and widens at the map's own element width, instead of the one-byte map
   unconditionally — which read a v2 class's chunks at quarter width and froze honest classes.
3. The matmul arms' parameter triples ride the `{name}.a16` suffix (the rule the conv triples,
   grouped exps and rope clamp already follow): the bare name is the codes', both are requested at
   byte offset zero, and `find_operand_v1` resolves `(name, layer, offset)` before length — one of
   the two requests was structurally unservable by any canonical inventory.
4. The registration-shape alignments: scores/values read the ONE registered triple and tile it (a
   per-`(head, position)` request has no tensor to land on — the count is the job's); the softmax
   reads its registered single byte; `Fixed` vs `KvScaled` out-width decides per-lane vs
   singleton-tiled for the stream narrowings; the seven position-0 sink seams resolve `.sink0` by
   the family's FIXED list (an artifact-dependent rule would let a challenger steer which table
   the court reads by withholding an opening).
5. `run_program` reports the slice offset for the lane- and head-sliced families (it reported 0,
   mis-slicing every tile but the first); `src_tile` resolves the `LayerIn` sentinel the way the
   canonical leaf set does.
6. `check_job_context_shape` admits the tiled scheme pin.
7. The routed-expert resolution (§5).

t11's fingerprint moved d7510c7a… → 923fe103… (devnet → 65eaa6e7…). The genesis is untouched;
deployment is whole-fleet-together (a Relaunch train, or one upgrade window).

## 5. The routed experts — the hybrid's one genuinely new court arm

A `.routed` matmul node computes the k CHOSEN experts' projections, and which experts those are is
decided by the execution itself — the ids exist only in the `RouterTopk` node's committed row.
The court read `node.weight_name` verbatim, a name no artifact tensor matches: every routed step
was `Unadjudicable` by construction.

Derive, never declare: the canonical input set for a `.routed` node APPENDS the table's unique
router-topk row at the same `(call, position)` — committed material, opened like any other input —
and the arm walks the challenged tile block by block, reading each block's expert id off that row
and resolving the codes/exps/triples under the PER-EXPERT tensors the artifact already stores
(`blk.N.ffn_expert.{e}_…`) at expert-local offsets. The expert identity is never the accuser's to
choose: it comes off the claim's own committed row, proven against the claim's own step root.

Building the arm convicted five more court-side descriptions of this graph, each of which would
have refuted an honest producer at a real dispute: the router arm read `k` from a stored triple
the engine never reads and emitted an interleaved row (the engine commits ids then weights, and
`k` is the declared width halved); the combine passed the whole routing row where the kernel takes
its weight lanes, and assumed disjoint covering runs; `MulWide` needed the four name-dispatched
resolutions the engine performs (plain, routed-fused, shared-fused, scalar broadcast); the decay
read a declared name no store holds instead of its two calibration rows plus the bias; the
convolution's decode window was one position short, and its requant name carried a `.weight` the
store does not. The `.sink0` position-0 convention is now scoped to classes that declare the dense
family's own projection kernel — it was firing on the hybrid's like-named seams.

## 6. Registration obligations — what a court-capable class pins

* **`artifact_root` is an OPENABLE inventory root.** ADR-0049 Decision G said it about the floor;
  it now binds the tiers: a close's operand openings verify against the registered root, and
  nothing can be opened against a flat digest. `a16_inventory_v1` / `qwen36_inventory_v1` are the
  canonical layouts (each normalising its artifact's parameter store to the shapes the arms
  request — singleton expansion included), and `qwen25_a16_registration_v2` states the obligation
  on its signature. The SDK's dense lineage resolves the court-capable row by the row's own root
  derivation — one mapping, `CanonicalClassV1::artifact_root`, decides on both sides.
* **The corrected A16 class's head names the engine's own view** (`output.weight`,
  `QWEN25_A16_HEAD_TENSOR_V2`): the tied spelling put the gather's rows and the head matmul's
  tiles under one inventory name at colliding offsets, making one view structurally unservable.
  Tying stays a fact about bytes. This moves the corrected class's id — which was never
  chain-registered, and a class is its graph.
* The corrected classes pass the FULL admission gate at their real geometries
  (`verify_class_admission_v2`: shape, both coverage gates, the ladder bound, all three
  court-cost ceilings, the PWU recount) — asserted by tests, not claimed by comments.

## 7. What this ADR does NOT decide

* **When the fleet moves to fingerprint 923fe103…** — a deployment train (whole-fleet, since the
  ruleset id moves), the operator's and ADR-0068's to schedule.
* **When the corrected classes register on chain and when producers switch to them.** The v1
  classes remain registered, remain liveness-admissible, and remain unprosecutable — ADR-0039's
  words already cover them ("admissible for liveness and must not carry weight"); moving the
  weight-bearing share onto the corrected rows is the operator half of this ADR's promise.
* The hybrid's checkpoint-anchored recurrence (a registered state chunk map for the GDN state) —
  deliberately deferred exactly as the profile's own comment defers it; `n_ctx 8` is the
  genesis-replay budget until then, and raising the context is gated on that map.
* **Two findings the sweep left standing rather than papering over.** (a) The hybrid's compiled
  `forward_token_probed` walk records ~17 named sites per layer against the 46–48 nodes the v2/v3
  tables declare, so it cannot fill the declared step space: BOTH authorities capture by compiling
  the plan from the profile they hold (bit-identical to the compiled engine by the standing
  differentials), and a backend whose id names no ledger row stays legacy and court-incapable.
  (b) A per-token `embed_lift.a16` store — which the real converter may write for the 33 GiB
  artifact — is not servable per lane; the inventory refuses it rather than guessing, and the
  Requantize arm would need the gather's per-token resolution before that leaf adjudicates on real
  weights. Neither blocks a fixture-verified class; both block claiming the real artifact is
  covered, which is why they are written here.
* The hybrid's checkpoint-anchored recurrence, as above; and the `MulElem` cost arm's 9-byte
  triple pricing (A16 triples are 17), an under-pricing the already-admitted dense class shares.

## Mission alignment amendment — 2026-10-07

既存step-space sweepは、そのfixtureのkernel裁定能力を示す。canonical profile、公開transportとfresh verifierによるlocalization/convictionを含まない限り、全profileで外部訴追が完了したという意味へ拡張しない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0070-the-model-tiers-step-spaces-are-adjudicable.md -->

</details>

<a id="source-0092-the-ladder-is-minted-once-and-the-clock-is-what-binds"></a>

<details>
<summary>0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md — 原文 L1–L296</summary>

[原文を開く](../adr/0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) · SHA-256: `60d497474f1417a18b098ae13be5b76095cc71b755018da323ccc2d5961bc46f`

<!-- ADR_SOURCE_BEGIN:0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md -->
~~~~~~~~markdown
# ADR-0092 — The ladder is minted once, and the wall clock is what binds

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: ACCEPTED 2026-09-06 (design first, at the operator's word; §8 records the implementation)
* Builds on: [0049](0049-palw-adjudication-contract.md) Decision C (what one round may cost),
  [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (adjudicability is the price of
  weight), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decision 12
  (the `2^32` context ladder), [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (the
  verified unit is short), [0082](0082-the-close-is-flat-in-the-context.md) Decisions 1, 3, 6 and 9
  (the k-ary court, the close ceiling, the seat's window),
  [0084](0084-the-ids-ride-the-capture-stays-home.md) U-08 (the walkers take the ruleset's ladder).
* Amends: nothing yet — §4 proposes amendments to 0082 Decision 3 only if Decision 3 of this ADR
  is taken.
* Supersedes nothing.

## 0. The sentence this ADR is

**The ladder is a one-time consensus commitment; the Court window, not model size, is the binding
resource constraint.**

A model's width costs the court rounds, not leaves, and rounds are already logarithmic — so what
stops a bigger model is not the tree. It is that every round costs wall clock inside a window fixed
at mint, and `max_step_leaf_count` is inside the ruleset id and therefore **cannot be raised on a
running chain**. This ADR decides what a mainnet freezes at mint so that 7B and 1T are the same
protocol, and names where a future width problem is paid for.

## 1. What the operator asked, in the operator's words

> 大きい dense LLM も DA Court で扱えるようにするなら、「Court の上限を単純に引き上げる」のではなく、
> 巨大な推論を階層化して DA するのが一番きれいです。
> …
> モデルが大きくなっても、1回の dispute で Court が調べる量は一定 にする。
> …
> 「モデルサイズが大きくなるほど DA が破綻する」という現在の構造を、「モデルサイズが大きくなっても
> DA の対象が階層的に増えるだけ」に変えられます。

The goal is right and this ADR adopts it. What follows first is the part the operator could not
see from the audit report: **most of that structure is already shipped**, and saying so precisely is
what makes the remaining decision small enough to take.

## 2. What already exists, checked against the tree rather than against the ADRs

Read at `4fcce4b0`. Every claim here is a file and a line, because an ADR that restates a decision
already taken is the most expensive kind of document this project can write.

| the operator's step | where it already is |
|---|---|
| hierarchical chunk/subchunk tree | `palw_context_ladder.rs` (4,018 lines), `palw_bisect.rs` (1,993) |
| dissect down to one operation | `palw_attn_court_v1.rs` (2,882); the moves `CourtAttnRootClaimed` / `CourtAttnDissected` are consensus objects (`palw_state_v2.rs:3003, 3032`) |
| split the answer in time | ADR-0080's interval lane; `fp_interval.rs`, ADR-0077 Decision 8 |
| separate model from inference | ADR-0067, ADR-0078: `class_id` / `artifact_root` / `shape_profile_id` are bound separately from a job's roots |
| **court work ≈ constant in model size** | **already true**: `rounds = ceil(log2(max_step_leaf_count)) + terminal` (`palw_mode_v2.rs:270`) |
| **refuse at registration a class the court cannot try** | **already enforced**: `verify_class_admission_v5` demands *all three* of the close ceiling, the ladder depth and the court window, and names which one refused (`CourtCostExceedsCeiling` / `DeeperThanTheLadder` / `CourtWindowTooShort`) |

So the proposal's (1), (2), (3), (5) and (6) are not new work. They are the shipped design, arrived
at independently, which is a good sign about the design and a bad sign about the documentation:
nothing in `docs/adr/` states the invariant in one place, which is why it could be proposed as
missing.

Two things in the audit are genuinely open, and neither is a shape problem:

* **The responder does not exist.** The court can try a fused-attention dissection; no shipped
  binary constructs one. `palw_state_v2.rs:7805-7812` records that on a fused site an `Arithmetic`
  whole-row close is refused by `check_close_cost_v2` and an `AttnDissection` close is refused
  `NoDissection` — so every terminal leaf of a fused class is undefendable. That is audit finding
  C-2, it is a feature of the size of the close prover, and it is **not** what this ADR decides.
* **The walkers do not take the ruleset's ladder.** `PALW_STEP_MAX_LEAVES = 1 << 22` is the
  executor's constant; the RC ruleset's `max_step_leaf_count` is `1 << 26` and ADR-0077 D12's
  ladder is `1 << 32`. Audit H-4 / ADR-0084 U-08. It is an arming (`Params::palw_court_ladder`),
  not a design change.

## 3. The wall, and where it actually is

`PalwCourtParamsV2::worst_case_duration_with_history_daa` (`palw_mode_v2.rs:602-609`):

```
moves    = (bisection_rounds + history_dissection_rounds) * 2 + terminal_rounds + root_claim
duration = moves * turn_deadline_daa
```

Three consequences, in order of how much they bind:

1. **Leaves are cheap.** `bisection_rounds` is `ceil(log_arity(max_step_leaf_count))`, so at
   arity 2 **one doubling of the provisioned space costs one round** — that is the rule, and it is
   the only statement in this section that is not a generated number. The tree's own sizing table
   and `provisioning_the_whole_step_space_costs_four_rounds` already pin the RC's step from its
   floor to the whole step space in those terms. A model an order of magnitude wider than today's
   therefore costs rounds in the tens, not in the millions, and §5's generator is what says how
   many. Nothing about the leaf count is a wall.
2. **Rounds are not cheap, because each one is a `turn_deadline_daa` of wall clock, doubled for
   the two parties.** The duration must fit `window_court`. The RC already moved
   `court_turn_deadline` 60 → 42 for exactly this reason, and at ADR-0077 D12's `2^32` the worst
   case sits a few tens of DAA below the window. **`2^32` is not a leaf-count choice. It is the
   wall-clock ceiling at the RC's turn deadline and court window.**
3. **`max_step_leaf_count` is inside the ruleset id.** `palw_ruleset_id_v2` hashes the bundle, so
   raising the ladder on a running chain is a flag day, and
   `palw_class_admission_v2.rs:56-62` states the consequence in its own words: *"Unlike every other
   obstacle to adding a class later, this one cannot be repaired later: by the time the second
   class exists, the number is already inside the network's identity."*

That is the decision this ADR exists to take. Not "how do we make the court hierarchical" — it is —
but **what number a mainnet freezes at mint, given that it can never raise it, and what it does
when a model arrives that does not fit.**

## 4. Decisions

**Decision 1 — a mainnet mints its ladder at the top of the wall-clock budget, not at the width of
the models it has.** The card chooses `max_step_leaf_count` as the largest power of two whose
`worst_case_duration_with_history_daa` still fits `window_court` with a stated margin, and records
the margin. Rationale: rounds are logarithmic, so provisioning the whole reachable space costs a
handful of rounds; and the number cannot be raised later, so under-provisioning is the only
irreversible mistake available at mint.

**Decision 2 — the class-admission gate keeps all three refusals, and the ADR names them as one
invariant.** A class is adjudicable on this ruleset when, and only when, it satisfies the close
ceiling, the ladder depth and the court window simultaneously. This is already the code
(`verify_class_admission_v5`); Decision 2 makes it a decision so a future amendment cannot relax
one of the three without amending an ADR.

**Decision 3 — the arity is the protocol-level knob that trades dissection depth against per-round
Court cost, and its value is DERIVED, not chosen.** `palw_kary_rounds_v1(space, arity)` is
`ceil(log_arity(space))`, so raising the arity from 2 to `k` divides the *history* round count by
`log2(k)` and raises what one round carries. **Fewer rounds is not the same as less cost**, and
this ADR does not assume it is.

It does not have to. `palw_court_params_at_v2` already derives the arity, per block, whenever
`palw_kary_court` is armed: `palw_court_arity_v1` searches upward from the minimum and returns the
first arity that satisfies **both** halves — the clock (`moves × deadline + reserve < window_court`,
the same inequality admission applies) and the wire (one round must fit a framed carrier at the
widest registered lane count) — and returns `None`, a refusal rather than a fallback to 2, when
none does. Its own doc puts it plainly: *no preset writes a `k` and no `k` can be chosen.*

Two consequences worth stating as decisions rather than leaving to be rediscovered:

* **The arity scales with what is registered.** The derivation reads
  `palw_attn_widest_registered_site_v2(bundle)`, so registering a wider model is what raises the
  arity. This is the property that makes 7B and 1T the same protocol, and it is already the code.
* **The leaf ladder is binary whatever `k` is** (`PALW_COURT_LEAF_LADDER_ARITY_V1`). `k` buys
  history rounds only. An earlier derivation priced a k-ary leaf ladder no session plays and
  reported 34 moves where the played dispute takes 60 — audit D H-2. Any future work that makes
  the ladder itself k-ary moves that constant, and one place decides.

*This amends ADR-0082 Decision 3 only if the derivation's inputs are later changed.*

**Decision 4 — a model too wide for the minted ladder is a new class on a new ruleset, not a raised
ceiling.** The honest shape, and the one the code already forces. A network that wants a wider model
than it minted for takes a flag day or a re-mint; it does not get to widen its ladder in place,
because the ladder is in its identity. Stating this stops the next author from proposing an
in-place raise and discovering the identity problem after the design is written.

**Decision 5 — the time-direction split is the free-prompt lane's, and the attempt lane does not
get one.** ADR-0080 already bounds the verified unit on the free-prompt lane by interval, where
splitting a long answer improves both what a seat can verify and what a user can be served.

The attempt lane is refused it **not because it cannot be done, but because it would change the
consensus semantics of an attempt**. ADR-0072's unit is one execution, one ticket, one draw;
segmenting an attempt into `segment 1 … segment n` makes the ticket a sequence, which changes what
is drawn, what is priced and what a claim is — a change to ADR-0072 and ADR-0074, not a Court
optimisation. This ADR declines it on those grounds and records the reason, so the next author
reaches the semantics question deliberately instead of discovering it after the design.

## 5. The arithmetic, to be produced and not transcribed

> **Numbers in this section are generated artifacts, not normative hand-maintained constants.**
> A reader who needs a value runs the generator; a reader who finds a value here that the generator
> does not produce has found a bug in this document, not a decision.

The table this ADR needs — provisioned leaves, arity, bisection rounds, worst-case duration, margin
against `window_court` — must come from `misaka/palw/base0/src/bin/base0-class-sizing.rs` extended
to print the duration and the margin, and be pinned by a test the way
`provisioning_the_whole_step_space_costs_four_rounds` pins the round count today. **No number in
this section is written by hand.** This project's own lesson is that a worked example with no test
behind it is a number that drifts, and ADR-0087 §4's price column drifted twice for exactly that
reason.

What the table must answer before a mainnet card is minted:

* the largest `max_step_leaf_count` that fits `window_court` at the card's `turn_deadline_daa`,
  at arity 2 and at whatever arity the card chooses;
* the worst-case leaf count of every class the card registers, against that number — the dense
  graph-v5 512 row's canonical job is 6,630,544 and its worst case 52,778,128, both already
  measured in `palw_qwen25_profile.rs:949`;
* the margin, stated as a percentage, so the next model's fit is a lookup rather than a rediscovery.

## 6. Invariants the tests must hold

1. `rounds(max_step_leaf_count, arity) * turn_deadline * 2 + terminal <= window_court` for every
   shipped preset — asserted, not commented.
2. Every registered class of every shipped preset satisfies all three admission bounds, with the
   margin printed on failure.
3. Raising `max_step_leaf_count` on a preset moves that preset's `consensus_params_id` — the
   irreversibility Decision 4 rests on, asserted rather than assumed.
4. The sizing table is generated, and the test that pins it fails if the generator's output moves.

## 7. What is deliberately not decided

* **The fused-attention responder.** Audit C-2. It is the missing half of a court that already
  exists, it is the size of the close prover, and it belongs to its own ADR or to ADR-0082's
  implementation record — not here.
* ~~**Whether the RC's arity moves.**~~ **Withdrawn — this was never open.** An earlier draft of
  this ADR left the arity's value to a later operator decision. It is not one:
  `palw_court_params_at_v2` derives it from the widest registered site every time the k-ary court is
  armed, and §8 records what it derives on this tree. Decision 3 now says so. The draft's error is
  left visible rather than deleted, because "the value stays open" is exactly the kind of claim that
  becomes a work item nobody can close.
* **Whether testnet-11's ladder is re-armed at a height.** ADR-0084 U-08 / audit H-4 is an arming
  of `Params::palw_court_ladder`, and its schedule is an operator decision.
* **Any number.** §5 says why.

## 8. Implementation record (2026-09-06)

Steps 1 and 2 of §9 are done, and §6's invariants are tests. What the generator then measured is
worth reading before Decisions 1 and 3 are taken, because it changes what they are choosing
between.

**The generator.** `misaka-palw-base0 --bin base0-class-sizing` gained an ADR-0092 section that
reads the shipped RC bundle — turn deadline, terminal rounds, arity, close chunks, close-assembly
reserve, `window_court` — and prints, for each (history positions × arity) pair, the widest
`max_step_leaf_count` whose worst-case prosecution still fits. The predicate is the shipped one,
`palw_attn_court_admits_row_v1`, so a ladder the table calls admissible is one class admission
admits. No figure is written into this document.

**What it found, and it is the reason Decision 3 matters more than it looked.** At zero history the
wall is a long way off; the ladder is bounded by the round count alone and the same ceiling is
reached at every arity. Once a dispute must also dissect a history, the history's rounds — not the
ladder's — are what spend the window, and at the shipped arity the admissible ladder falls steeply
with the context. The shipped ladder and the shipped arity are a legal pair, and at the dense row's
own context they are *exactly* a legal pair: the widest ladder that arity can prosecute at that
context is the one the RC froze, with no headroom above it. That is pinned by
`the_shipped_ladder_is_the_widest_the_shipped_arity_can_prosecute_at_its_own_context`, which fails
in both directions — if the ladder is raised without the arity or the window moving with it, and if
the pair silently gains room.

So Decision 3's knob is not a refinement. On this window, raising the arity is what makes a wider
ladder reachable *at a real context at all*, and the decision table is where the two numbers are
priced against one another. The value stays open, as §7 says: this measures the wall clock, and the
bytes one round carries are the close ceiling's half of the same question.

**The derived arity, measured.** The generator now also calls the production derivation. On this
tree the widest registered site is 512 history positions at 8 lanes, and
`palw_court_arity_v1` returns 2 — which is what the bundle stores, so the shipped court plays the
arity its own ruleset derives. `palw_attn_court_v1`'s own test pins that pair. The report prints
the derivation beside the table so a reader cannot mistake the table's columns for a menu: they are
what *would* be derived at wider registered sites, which is the same thing said forward.

**Step 2 was already done.** `mainnet_card_base_v1` arms both `palw_context_ladder` and
`palw_court_ladder`; the second was the audit's M-15, a regression the merge of the two development
lines had created, and it landed with the 2026-09-06 audit fixes.

**§6's invariants**, in `consensus/core/tests/palw_adr0092_court_budget.rs`, four tests, each with
its negative control run and observed red before being recorded here:

| invariant | test |
|---|---|
| 1 — every shipped preset prosecutes its own ladder inside its own window | `every_shipped_preset_prosecutes_its_own_ladder_inside_its_own_window` |
| the shipped pair is at the wall, not below it | `the_shipped_ladder_is_the_widest_the_shipped_arity_can_prosecute_at_its_own_context` |
| Decision 3's knob is measured, not assumed | `a_higher_arity_buys_a_deeper_ladder_at_the_same_window` |
| 3 — Decision 4's irreversibility | `raising_the_ladder_moves_the_rulesets_fingerprint` |

Invariant 2 of §6 — every registered class against all three admission bounds — is not added here:
`verify_class_admission_v5` already enforces it at registration and the genesis loader already runs
it, so a separate assertion would restate the production path rather than check it. Recorded as a
deliberate omission rather than an oversight.

## 9. Order of work

1. Extend the sizing binary to print duration and margin; pin its output. (No consensus change.)
2. Arm `Params::palw_court_ladder` where U-08 requires it — audit H-4, already fenced, and a
   prerequisite for any of this being measurable on a class wider than `2^22`.
3. Fill §5's table from the generator, then take Decisions 1 and 3's values for the card.
4. The responder, under its own ADR.

## 10. Number hygiene

0092 is free: 0091 is the highest in `docs/adr/` at `4fcce4b0`, and `git grep -n "ADR-0092"` finds
no citation in code or docs. Next free number after this one is 0093.

## Mission alignment amendment — 2026-10-07

ladder/cost上限の静的admissionは維持するが、その最悪時のladderを実際に外部bondが辿ってcloseできることも受入対象にする。単にleaf空間が上限内であることをprosecutionの完成としない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md -->

</details>

<a id="source-0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token"></a>

<details>
<summary>0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md — 原文 L1–L462</summary>

[原文を開く](../adr/0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) · SHA-256: `f1866cf6be7ff1af3048bcca1fb4b5b48635082c6c5ddac3e896304aae452655`

<!-- ADR_SOURCE_BEGIN:0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md -->
~~~~~~~~markdown
# ADR-0097 — A model's fit is a lookup, and the entrance says its limits before the first token

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-10; **IMPLEMENTED the same day, consensus-inert** (§9). No fence, no
  field of `Params`, no object, no fingerprint moves: Decision 1 is a report over predicates the
  chain already runs, Decision 2 is two JSON objects on the gateway, and Decisions 3–5 are what
  the report says about the question that prompted it.
* Builds on: [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) (Decisions 1–4:
  the ladder is minted once, the window binds, the arity is derived; §5: every number is a
  generated artifact), [0082](0082-the-close-is-flat-in-the-context.md) (Z4: the window gate;
  Decision 3: the arity; Decision 9: the seat's window is the drill's),
  [0081](0081-long-context-the-input-is-a-state-chain.md) (§1.1: the hybrid is the structurally
  better long-context candidate; §1.2: the ids are the linear term; Decision 3: the tiled root),
  [0080](0080-the-answer-is-long-the-verified-unit-is-short.md) (§1: the residue is the width),
  [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) (Decisions 12–14: the
  ladder, the rows, the canonical job; Decision 16: `PanelDa`),
  [0096](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
  (Decision 1: the surface; Decision 5: a long thread is a chain of jobs; Decision 8: the token
  table as served material; Decision 10: the manifest), [0084](0084-the-ids-ride-the-capture-stays-home.md) /
  [0086](0086-the-opening-carries-the-fold-not-the-leaves.md) (what a seat is served),
  [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md) (the
  responder), [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md)
  (the host).
* Amends: nothing. Supersedes nothing.

## 0. The sentence this ADR is

**The chain can carry a model the size of Kimi K3 at ten positions of context, and nothing wider,
and the reason is nine walls with names — four of them numbers inside the ruleset id.** A 2M
context is refused for every model deeper than eight layers by the first of them, before any
court is consulted. This ADR does not move a wall. It makes the question "can this chain carry
model X at context C" a table generated from the chain's own predicates (Decision 1), makes the
entrance publish the limits a client needs before its first request (Decision 2), and names, wall
by wall, what a network that wants a wider model must mint with (Decision 5) — so the next person
to ask arrives at a lookup rather than at a conversation.

## 1. What was measured

The operator's question (2026-09-10):

> 現在のMISAKA chainはkimi k3くらいのdenceモデルを受け入れる基盤として 2M以上のコンテキストは
> […60 項目のチェックリスト…] などモデルを実用的に使用する基盤ができているか またできていなく
> 不十分ならADRを記述して実装を行なって

Read on `feat/adr-0096-everyday-lane` at `70d56519` (misakas) on 2026-09-10. Every figure below is
what `misaka-palw-base0 --bin palw-model-fit --preset rc` printed on that tree, from the
predicates named beside it; the binary is the authority and this section is its record
(ADR-0092 §5). The pinned figures are pinned as **limitations** by
`consensus/core/tests/palw_adr0097_model_fit.rs`, so that the day one moves, a test says so.

### 1.1 The walls, and where each ceiling lives

A class meets these in this order, and `verify_class_admission_v5` refuses at the first one it
meets. Read at the point of judgement "every scheduled fence armed" (`palw_kary_court` armed,
`palw_context_ladder` dormant, prompt ids `Flat` on testnet-11).

| wall | the predicate | the ceiling lives in | RC value |
|---|---|---|---|
| geometry ceiling | `n_ctx × layer_count ≤ PALW_STEP_MAX_ENUMERATION` | `PalwShapeProfileV3::validate_geometry` — a code constant that gates every `ClassRegistered`, so moving it is a ruleset move | 16,777,216 |
| ladder | the whole context as prefill, in leaves, ≤ `max_step_leaf_count` | `PalwCourtParamsV2`, inside `palw_ruleset_id_v2` (ADR-0092 D4) | 2^26 |
| close bytes / chunks / terminal macs / operand count | the widest close ≤ the court's ceilings | `PalwCourtParamsV2`, the same id | 2,250,000 B / 27 carriers / 2^24 / 8 |
| court window | `moves × turn_deadline + reserve < window_court` at the arity the court plays | the lattice windows, the same id (ADR-0082 Z4) | 3,000 DAA, 42 DAA a turn, 2 terminal |
| state chunks | the attention cache at `n_ctx`, tiled at 16, ≤ `PALW_STEP_LEG_MAX_STATE_CHUNKS` | `palw_step_leg`, the checkpoint leg's cap | 65,536 |
| public-da payload | the prompt ids on a `PublicDa` commitment ≤ one standard transaction | `PALW_STANDARD_TX_BYTES`; under `PanelDa` the ids do not ride (ADR-0077 D16) | 120,000 B |

Two quantities are reported and are not walls: what one JOB may answer (`min(max_decode_tokens,
PALW_V2_MAX_TRACE_EVENTS)` = 1,024 on both presets — a bound on an answer, which ADR-0096 Decision
5 chains, never on a class), and what a SEAT must hold (§1.5 — a host fact no ruleset states).

**The close past the ladder is not a number.** The court-cost derivation's walk is capped at the
ladder (audit D H-5), so a row the ladder refuses has an *unpriced* close, not an admitted one.
The generator prints `unpriced` there and never `admitted`; the widest context the four close
walls admit can therefore never exceed the ladder's.

### 1.2 The rows the chain carries, and the widest context each wall admits them at

Both shipped presets register one row with a carried profile: the dense graph-v5 row at 512
(`4277d84f…`). On the RC it is admitted on every wall — and **at the wall** on the window: 65 moves
× 42 DAA + 216 reserve = 2,946 against 2,999, ADR-0092 §8's finding restated as a lookup. The
hybrid family's row at 8 is admitted on the RC (close 160,100 bytes = 2 carriers of 27) and
**refused on the devnet by the close alone** (its ceiling is one carrier, 83,333 bytes): the devnet
cannot carry the hybrid tier, and nothing in `docs/` said so until this table.

| candidate (RC) | geometry | ladder | close | window | chunks | public-da | **fit** |
|---|---|---|---|---|---|---|---|
| Qwen2.5-1.5B A16 graph-v5 (28 layers) | 599,186 | **651** | ≤ ladder | **512** | 18,720 | 30,000 | **512** |
| Qwen3.6-35B-A3B graph-v5 (40 layers) | 419,430 | **204** | ≤ ladder | 512 | 52,416 | 30,000 | **204** |
| Kimi K3 stand-in (92 layers, §1.3) | 182,361 | **10** | ≤ ladder | 512 | 22,784 | 30,000 | **10** |

Each cell is the largest `n_ctx` at which that wall alone admits the row; the fit is the minimum.
Three things the table settles:

* **The dense row ships at the width the clock allows.** The ladder would take it to 651; the
  window stops it at 512 and refuses 513 by name (`CourtWindow`, and nothing else). A wider
  dense row on this ruleset is not a ladder question; it is an arity question (ADR-0092 D3), and
  at 128 lanes arity 64 fits the carrier — the generator's own derivation says what the arity
  would be at each width.
* **The hybrid — the structurally better long-context family (ADR-0081 §1.1: three layers in
  four carry O(1) state) — is held at 204 by one number, the ladder.** At 512 it is refused by
  the ladder ALONE; the window admits it, the chunks admit it. `palw_qwen36_profile` still says
  the row is 8 "because the whole-close derivation says so" — that was the 80 KiB carrier; the RC
  carries 27 and the binding wall moved without the comment moving.
* **The devnet's window admits the dense row to 4,096** (turn deadline 4, window 300), so its fit
  is the ladder's 651. A drill that wants a wider row than testnet-11 can carry has a network
  that can carry it, up to the ladder.

### 1.3 Kimi K3, as a stand-in, on the shipped ruleset

The public card (README and `config.json`, read 2026-09-10): 2.8T total / 104B activated
parameters, 93 layers as 69 KDA (a gated linear attention) + 24 gated MLA, hidden 7,168, 96
attention heads, MLA cache 512 + 64 a position, `v_head_dim` 128, 896 experts of 3,072 with 16
routed and 2 shared a token, vocabulary 163,840, context 1,048,576. Written as the hybrid
family's geometry (`palw_model_fit_v1::stand_ins::KIMI_K3_AS_HYBRID_V1`: KDA as GatedDeltaNet at
56 heads of 128, MLA as grouped-query attention at 6 KV heads of 128, 92 layers at interval 4 —
every substitution named on the constant, each in the direction that refuses rather than admits),
the tree's own predicates price it:

| n_ctx | what refuses |
|---|---|
| 1,048,576 (the card's) | the **geometry ceiling**, before a profile exists: `96,468,992 > 16,777,216`; the family builder refuses with `validate_geometry`'s own sentence |
| 131,072 | four walls at once — ladder `850,866,290,744 > 2^26`, window `3,618 > 2,999`, chunks `376,832 > 65,536`, public-da `524,288 > 120,000` — and the close unpriced |
| 512 | the ladder alone: `3,323,778,104 > 67,108,864` (50× over) |
| **10** | **admitted on every wall** |

So the honest sentence is not "the chain cannot carry a K3-class model". It is: **this ruleset
carries one at ten positions**, which is a class nobody would use, and every wider width is
refused by a named number — the ladder at 11, then four walls together past 32,768, then the
geometry ceiling past 182,361. No single fix admits it; Decision 5 says what would.

### 1.4 The 2M question, for every depth

`PALW_STEP_MAX_ENUMERATION` is a product, so the answer depends on nothing but the depth:

| positions | fewest layers refused | so |
|---|---|---|
| 2^21 (2M) | **9** | refused for every model deeper than eight layers |
| 2^20 (1M) | **17** | refused for every model deeper than sixteen |
| 2^17 (128K) | 129 | admitted for every model shallower than 129 layers — by THIS wall |

For the three geometries this tree prices the widest context the ceiling admits is 599,186 (28
layers), 419,430 (40) and 182,361 (92; 180,400 at the card's 93). The ceiling is a constant in
`validate_geometry` with a stated reason — it bounds an enumeration a validating node performs on
every `ClassRegistered` — and it is not the binding wall for any of them (§1.2: the ladder is),
so raising it alone would admit nothing.

### 1.5 What a seat must hold

From the geometry and the state map's row widths (i32 cache: `kv_heads × head_dim × 4` a
position a layer; the recurrence's state is `k_dim × v_dim × 4` a head and constant in the
context). No verdict: no ruleset states what a host has, and the artifact's size is the
converter's number — the parameter count is a lower bound at one byte a weight.

| candidate | n_ctx | attention cache | recurrent state | artifact |
|---|---|---|---|---|
| Qwen2.5-1.5B A16 | 512 / 2M | 28 MiB / 112 GiB | 0 | the converter's |
| Qwen3.6-35B-A3B | 512 / 2M | 20 MiB / 80 GiB | 60 MiB | the converter's (33 GiB, measured on 5f) |
| Kimi K3 stand-in | 512 / 1M / 2M | 69 MiB / 138 GiB / 276 GiB | 241.5 MiB | ≥ 2.5 TiB |

A seat replays a job from the prompt ids against the artifact it holds (ADR-0082 Decision 9,
ADR-0084/0086: the opening carries the fold, the seat supplies the leaves). At 1M positions a K3
seat holds a 138 GiB cache beside a 2.5 TiB artifact, and its replay must finish inside
`window_receipt × rate` (ADR-0082 D9 — the certification drill's number, measured at about one
second a token on the a16 host). That is not a wall in the tree; it is the reason Decision 5's
first item is the seat and not the court.

### 1.6 What the entrance told a client before

`GET /v1/models` carried `n_ctx` and nothing else about limits (`surface.rs`, `models_body`). A
request past the window was a 400 whose entire content was the worker's sentence — `prompt 51 +
decode ceiling 476 exceeds max_context_tokens 512` — which the Studio parses by string
(`backend/gateway.rs`, `ceiling_from_refusal`) to compute the retry it should have been able to
compute before sending. A `max_tokens` past `--max-decode-cap` was **clamped and nobody was told**
(`handle_chat`, `decode_limit`), which is the one silent downgrade ADR-0096 Decision 1's doctrine
left standing. Nothing said which features a fence had decided: a client learned that
`response_format` was advisory from the answer, and that `temperature` was refused from a 400.

### 1.7 The checklist

The operator's message carries a sixty-item list of what "a practical foundation for a
Kimi/Claude-class model" needs. Appendix A reads every item against this tree, with the file that
holds it, the ADR that decided it, or the doctrine that refuses it. The short form: the lane
already has the Tier-1 items an OpenAI-shaped client needs (the surface, streaming, tools as text,
formats, `usage`, conversations, the record, the model list — ADR-0096), refuses by doctrine the
ones that would make the seat's replay a different computation (sampling knobs, prompt/KV caching
across jobs, silent context truncation), and was missing exactly the two things this ADR adds:
the limits, published, and the fit, computed.

## 2. The requirement

> **R-fit — for any (model, context, ruleset), the answer to "does it fit" is a table generated
> from the chain's own predicates, with every wall named and its number, and the entrance a person
> uses publishes the limits their request must respect before they send it.** Nothing consensus
> moves to satisfy it. A wall that must move is a mint-time decision (ADR-0092 Decision 4) and is
> named here as one, never taken here.

## 3. Decisions

**Decision 1 — the fit is a lookup.** `consensus/core/src/palw_model_fit_v1.rs`:
`palw_model_fit_v1(profile, bundle, court, prompt_ids_form)` returns every wall of §1.1 as a row
`{wall, need, have, unit, verdict, note}` plus the answer-per-job and the seat footprint. Each
row is computed by the predicate admission runs — `worst_case_step_leaf_count_capped_v1`,
`derive_court_cost_shaped_v1` under the shape `palw_class_ladder_rules_for_court_v1` builds (the
genesis-anchored form for a class with no map), `palw_attn_court_admits_row_v1`,
`tiled_kv_state_geometry_v3` — never by a second spelling. `court` and `prompt_ids_form` are the
caller's reading of the fences (`palw_admission_shape_at_v1`, the acceptance path's own), for
ADR-0082 Decision 5's reason. A verdict is `Admitted`, `Refused` or `Unpriced`; the last is kept
apart because "too wide to price" and "priced and too wide" send a reader to different files.
`palw_geometry_ceiling_fit_v1(n_ctx, layers)` answers the first wall with no profile at all,
because it is the wall a person asking about 2M meets. `misaka-palw-base0 --bin palw-model-fit`
prints the table for a preset (§1's sections, in that order); it is the generator, and no ADR
writes a number it does not print.

**Decision 2 — the entrance says its limits before the first token.** `GET /v1/models` carries
`misaka.limits` and `GET /health` carries the same object (`surface::limits_body`, schema
`misaka.palw.limits.v1`): the context window and the rule on it (prompt and answer together), the
most an answer may be (`min(--max-decode-cap, n_ctx − 1)`), the default, the prompt's byte
ceiling, the tokenizer id and vocabulary (so a client that counts fetches the table ADR-0096
Decision 8 serves), `jobs_per_request: 1`, and — per feature the surface accepts — the word the
chain's fences decide: `advisory` / `committed` for formats, `greedy_only` / `requested` for the
sampler, `public_da` / `panel_da_available` and `flat` / `merkle` for the ids. The two refusals a
request meets before the chain — the window and the prompt bytes — are the gateway's own error
body (`{"error": {"message", "type"}}`, the sentence unchanged: every client reads
`error.message`) with `error.code` added — `context_length_exceeded`, which is OpenAI's own code
for the same refusal, so a stock SDK's `err.code` already reads it — and the numbers under
`misaka.refusal` (`room_for_answer` among them), so a client branches on a code and computes its
retry from numbers. And the clamp §1.6 found is reported: every answer carries
`misaka.decode = {requested_max_tokens, applied_limit, cap, clamped}`. Nothing is refused that was
served before, and nothing is downgraded that the response does not now say.

**Decision 3 — the geometry ceiling is a wall with a name, and this ADR does not move it.**
`PALW_STEP_MAX_ENUMERATION` refuses a 2M context for every model deeper than eight layers and a
1M context past sixteen (§1.4). It is a code constant, but it gates what a `ClassRegistered` may
carry, so a build that changed it would admit registrations another build refuses: it is a
ruleset move with the same shape as the ladder's, and ADR-0092 Decision 4 governs it — a network
that wants a wider product mints with one. It is also not the binding wall for any geometry this
tree prices (§1.2), so raising it alone admits nothing; a proposal to raise it must come with the
generator's table showing which wall binds after.

**Decision 4 — a stand-in is a verdict, never a row.** `palw_model_fit_v1::stand_ins` holds
geometries this tree prices that no chain registers, each substitution named on the constant and
chosen in the direction that refuses. No registration may cite a stand-in, no id derived from one
is a class id, and a stand-in that is ever converted becomes a MEASURED geometry in its family's
module and leaves this one. The K3 stand-in is the first; a second is added the same way, with
its card's reading dated.

**Decision 5 — what a network that wants a K3-class model at a long context must mint with, wall
by wall.** Named, not decided: each item is a mint-time number or a design with a measurement
in front of it, and this ADR's job is that the next author finds the list rather than one item.

| wall (§1.1) | what admits it | the ADR that owns it | what must be measured first |
|---|---|---|---|
| the seat (§1.5) | **a seat that holds a shard, not the model** — replay by expert or layer shard with the panel's coverage stated as a number | the successor of ADR-0080/0081 that ADR-0081 §3 waits for; ADR-0081 Decision 8's coverage number, never taken | the detection probability of one forged shard against the panel size (ADR-0081 U-03), and the replay clock against `window_receipt` (ADR-0082 D9) |
| geometry ceiling | a ceiling minted for the product the network wants | this ADR's Decision 3; ADR-0092 D4 | nothing — arithmetic; the generator prints the widest context per depth |
| ladder | `max_step_leaf_count` minted at the top of the wall-clock budget | ADR-0092 D1 | the generator's decision table (ADR-0092 §5) at the row's history |
| court window | an arity the carrier admits at the row's lanes; at 128 lanes 64 fits | ADR-0092 D3 (derived, never chosen) | `palw_court_arity_v1` at the registered set — the generator prints it per row |
| close | the responder that ADR-0093 specifies and no binary produces | ADR-0093 | ADR-0093 §7's order, unchanged |
| state chunks | a chunk cap sized with the ladder — the two are one budget, since both count the same cache | ADR-0082 D4's map; a re-mint moves `PALW_STEP_LEG_MAX_STATE_CHUNKS` with the ladder | the leg's serialization at the cap, measured |
| public-da payload | `PanelDa` for the ids (ADR-0077 D16, armed on the card only) or the tiled root (ADR-0081 D3, armed on the card from genesis) — testnet-11 has neither | ADR-0077 D16, ADR-0081 D3 | nothing new; both are built and fenced |
| the answer | ADR-0096 Decision 5's chain of jobs, as today; a longer job is `max_decode_tokens`, a ruleset field | ADR-0096 D5 | nothing |

What the table says as one sentence: **a K3-class network is a different mint, and the thing
that makes it possible is not a deeper court but a seat that does not hold the model** — the
court already dissects to one operation (ADR-0092 §2), and every other wall is a number the
generator can size. That successor is measured before it is designed (ADR-0081's own lesson), and
it is not this ADR.

**Decision 6 — what "practical use" means on this lane, and where each item lives.** Appendix A
is the decision: an item is *held* (with its file and ADR), *the app's* (MISAKA-Studio, under
ADR-0096), *refused by doctrine* (with the ADR that refuses it and why), or *absent and named*
(with what would carry it). Two items on the list are refused in a way worth stating here because
they look like features and are not: **prompt caching / KV handoff across requests** — the seat
replays a job from its prompt ids, a cached state is court material and not a transport (ADR-0096
Decision 5's last sentence), and a cache the seat cannot reach is a token the seat will not
reproduce; and **silent context compression** — the entrance trims and SAYS so
(`misaka.context.dropped_turns`), summarizes as a job of its own, and never fabricates a turn.

## 4. What this costs

* **Chain:** nothing. No object, no field, no fence, no id moves.
* **Node:** nothing at runtime; one module of pure functions and one test file. The generator
  runs three bisections per wall per candidate — about thirty seconds in a debug build for three
  candidates and two presets.
* **Gateway:** two JSON objects assembled on request; the refusal parse is two `split_once`s.
* **Wire:** `GET /v1/models` and `GET /health` grow by one object; the chat response by one small
  one. No field a client read before changes.

## 5. Invariants the tests must hold

```
1  Every row a shipped preset's genesis registers with a carried profile is ADMITTED on every
   wall of that preset (the positive control; read off genesis_objects, never off a list).
2  On the RC the dense row's fit is 512 and the wall at 513 is the court window alone; the
   ladder admits it to exactly 651, and past the ladder the close is UNPRICED (four walls), never
   admitted. The hybrid row at 512 is refused by the ladder ALONE, and the ladder admits it to 204.
3  A 2M context is refused by the geometry ceiling past eight layers and a 1M context past
   sixteen; the family builders refuse to build past it in validate_geometry's own sentence.
4  The K3 stand-in fits the RC at ten positions; at 512 the ladder alone refuses it; at 131,072
   four walls refuse it by name with the close unpriced; its seat footprint is the geometry's
   arithmetic.
5  A wall reads its ceiling: the same stand-in against the same ruleset with the ladder raised
   to 2^40 flips the ladder row to admitted, prices the close, and costs the window exactly
   2 × 14 × turn_deadline more (fourteen ladder rounds, both parties).
6  Every `have` is the bundle's own number read back, on every preset.
7  The limits object says the window, the answer ceiling (never the whole window, never past the
   operator's cap), the tokenizer, and — per feature — the word each fence decides; arming each
   fence moves exactly its own word.
8  A context refusal and a prompt-bytes refusal are the gateway's own error body plus
   error.code and misaka.refusal — remove those two and error_body(message) is what is left; every
   other message is error_body(message) byte for byte; the code is OpenAI's own where one exists.
```

1–6 are `consensus/core/tests/palw_adr0097_model_fit.rs` and the module's own tests; 7–8 are
`misaka/palw/gateway/src/surface.rs`'s. All were run red before green where a negative control
exists (5, 8) and observed against the generator's own output (1–4).

## 6. Order of work

1. Decision 1 — the module, the generator, the tests. **Done** (§9).
2. Decision 2 — the limits, the refusal, the decode report, the doc. **Done** (§9).
3. The Studio reads `misaka.limits` instead of `/health`'s bare `n_ctx` and branches on the
   refusal's code instead of the sentence — MISAKA-Studio. **Done** (§9): `feat/adr-0097-model-fit`
   in that repository, branched from its `feat/adr-0096-everyday-lane`.
4. `palw_qwen36_profile`'s "8 positions because the whole-close derivation says so" comment and
   `palw_qwen25_profile`'s "admits at most 574" are corrected to point at the generator; the
   numbers are the test's. **Done** (§9): each carries a dated note, the old sentence left in
   place beside it.
5. The generator's RC table is run on every re-mint and beside every class row a card registers
   (ADR-0092 §5's table gains its columns), so a class's fit is a lookup at mint too.
6. The successor: Decision 5's first row, measured first. Not on this branch.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0092 D1–D4, §5 | kept whole; the generator this ADR adds is the "table the next model's fit is a lookup in" §5 asked for |
| ADR-0092 §8's "at the wall" finding | restated as Invariant 2 from the fit rather than from the widest-ladder search; both stand |
| ADR-0081 §1.1 (the hybrid is the better long-context candidate) | confirmed and bounded: what holds it at 204 is the ladder alone (§1.2) |
| ADR-0081 Decision 8's coverage number | still not taken; Decision 5 names it as the first measurement of the successor |
| ADR-0077 Decision 13 (rows at 512 → 2,048 → 8,192) | 2,048 is refused today by the ladder AND the window (§1.2's sweep); the rungs are mint-time widths, as ADR-0092 D4 already says |
| ADR-0096 Decision 1 (refuse by name, never downgrade silently) | extended to the `max_tokens` clamp, which is now reported (Decision 2) |
| ADR-0096 Decision 5 (a long thread is a chain of jobs) | kept; §1.1 states the per-job answer bound beside it |
| `palw_qwen36_profile`'s and `palw_qwen25_profile`'s width comments | stale in the direction §1.2 measures; §6 item 4 |

## 8. What is deliberately not decided

* **Any wall's new value.** Each is a mint-time number (ADR-0092 D4), and the generator prices
  the candidates; the card's author reads the table.
* **The sharded seat** (Decision 5's first row). Measured first, designed after, under its own
  number.
* **Whether the geometry ceiling should be a ruleset field** rather than a constant. It behaves
  as one (Decision 3); making it a field is a fingerprint move for a value nobody has asked to
  change on a running chain.
* **A `/v1/tokenize` route.** The gateway has no tokenizer (`wire.rs`, by design: the worker
  renders); the tokenizer id and the served table (ADR-0096 D8) are what a client counts with.
* **Kimi K3's actual KDA head count and MLA-as-cache pricing.** The stand-in's substitutions are
  named and one-directional; a measured geometry replaces it the day the model is converted.

## 9. Number hygiene and implementation record

0097 is the next free number in the index at `70d56519` (ADR-0096's README row says so; 0094 and
0095 are resident on `main`), and this file claims it on `feat/adr-0097-model-fit` — written and
measured at `70d56519`, then rebased the same day onto `421cc92e` (`feat/adr-0096-everyday-lane`'s
merge of `origin/main` at `7f4dded4`), where the generator's output was re-run and is byte-identical
to §9's record. A concurrent claimant renumbers the later writer. **The next free number is 0098.**

* **2026-09-10** — ADR written against `70d56519` and rebased onto `421cc92e` (numbers unchanged);
  implemented on the same branch the same day:
  `consensus/core/src/palw_model_fit_v1.rs` (Decision 1: the walls, the report, the geometry
  ceiling alone, the stand-ins module with `KIMI_K3_AS_HYBRID_V1`),
  `misaka/palw/base0/src/bin/palw-model-fit.rs` (the generator: genesis rows, the sweep, the
  widest context per wall, the 2M table, the seat), `consensus/core/tests/palw_adr0097_model_fit.rs`
  (Invariants 1–6; 7 tests, 2 in the module), `misaka/palw/gateway/src/surface.rs` +
  `main.rs` (Decision 2: `limits_body`, `refusal_body`, `context_refusal`,
  `prompt_bytes_refusal`, `misaka.decode`; two new tests and the model-list test extended, the
  corpus unchanged at `5cdf9288…`), `docs/palw-freeprompt-gateway.md` (the limits and the refusal), and
  `docs/history/palw-model-fit-testnet11-2026-09-10.md` (the generator's RC output, as a dated record and
  not as a source). **Corrected the same day, by the Studio half (§6 item 3):** the first
  `refusal_body` served `{"error": "<sentence>"}` — a string where the gateway's own `error_body`,
  every OpenAI SDK and the Studio's `Lane::send` read an object — and its test compared the
  function with itself, so it passed. It now builds on the one spelling (`surface::error_body`,
  which `main.rs` delegates to), puts the code in `error.code`, and the test asserts that removing
  what the refusal adds leaves `error_body(message)`. **The Studio half** (MISAKA-Studio
  `feat/adr-0097-model-fit`, `74106e0`): `/health` read by one pure function whose limits win
  over the bare `n_ctx` and the `chain.*` flag; no job asked for more than `max_output_tokens`
  (per job — the request's own ask still drives the continue legs); one reader for the error
  body (`LaneRefusal`) used by the chat lane, its SSE path and the mining queue, deciding by
  `error.code` and computing from `misaka.refusal`, with the sentence parser kept only for a
  refusal that carries no code; proven end to end against an in-process gateway whose refusal
  sentence names no numbers. The mining queue had stringified the whole error object and
  recorded refusals as JSON. Measured on the way, and recorded in §1 because none of it was in
  `docs/`:
  the dense row's ladder width is 651 (not the 574 the profile comment says), the hybrid row is
  held at 204 by the ladder alone, the devnet cannot carry the hybrid row at all (one carrier),
  and the K3 stand-in fits at ten positions.

## Appendix A — the sixty-item checklist against this tree

The operator's list, by tier, with where each item is. *held* = in this tree, with its place;
*app* = MISAKA-Studio under ADR-0096; *doctrine* = refused on purpose, with the ADR; *named* =
absent, and what would carry it.

**Tier 1 — what an OpenAI-shaped client needs**

| item | where |
|---|---|
| 1 context length | held — the class's `n_ctx`, published now as `misaka.limits.context_window` (D2); its walls §1.1 |
| 2 input limit | held — `prompt + decode ≤ n_ctx` (`fp_worker.rs`); `max_prompt_bytes`; refusal with a code (D2) |
| 3 output limit | held — `max_output_tokens` (D2); `finish_reason: length`; ADR-0096 D5's continue legs (app) |
| 4 context management | app — trim → summary job → continue, each a claim, all reported (`misaka.context`, `misaka.jobs[]`, ADR-0096 D5) |
| 5 repository understanding, 6 code RAG, 7 AST, 8 git, 9 issues/PRs, 10 documents | not this lane's: an agent's tools over a repository are an application above the entrance; nothing on chain knows what a repository is (ADR-0096 D2's last sentence, for tools) |
| 11 long-term memory, 12 working memory | app — conversations as the runtime's files with export/import (ADR-0096 D12); the record (`records.rs`) |
| 13 tool calling | held — tools as the model's own Hermes-style text, `tool_calls[]` parsed after the run, `tool_choice` advisory (ADR-0096 D2); the round-trip is the app's |
| 14 agent loop | app — one job per leg, `misaka.jobs[]` (ADR-0096 D2/D5) |
| 15 sandbox, 16 permissions, 17 human approval | held for the HOST — ADR-0079 (the worker's confinement, the reachable-secret check, the public-bind acknowledgement); an agent's own permission levels are the app's |
| 18 streaming | held — SSE, the committed ids re-checked against what streamed (ADR-0077 D2 / W5) |
| 19 partial response / resume | app — `finish_reason: length` → continue legs (ADR-0096 D5); a job never resumes: one inference, one claim (R0) |
| 20 context compression | app — the summary job (ADR-0096 D5); **doctrine**: never silent, never fabricated |
| 21 prompt caching | **doctrine** — a cached state is court material, not a transport; the seat replays from the ids (ADR-0096 D5, ADR-0082 D4). What IS cheap: the artifact is resident (ADR-0077 D1) |
| 22–23 model switching / router | app — the Studio's backends (llama.cpp, MLX, the gateway); a gateway serves ONE class (`/v1/models`, ADR-0096 D1) and a router is a list of gateways |
| 24 modalities | **doctrine** — non-text parts refused by name (ADR-0096 D1); a class is a text graph |
| 25 error/log context, 26 runtime context, 27 chain context | held for the CHAIN — `/health`'s four names and `misaka.chain` (ADR-0077 D3); misakascan; `misaka palw facts`. As agent tools: the app's |
| 28 security context, 29 consensus safety | held — the court, the fences, `consensus-changes-by-activation`; a model does not change consensus by asking |
| 30–33 tests, fuzzing, property checks, diff review | held for THIS tree (the KATs, the profile fuzzers, the drills); as agent tools: the app's |
| 34 multi-agent | app |
| 35–36 context priority / budget | app — ADR-0096 D5's order (system, newest, then oldest-first drops); the budget is `n_ctx` and D2 publishes it |
| 37 token counting | held in part — `usage`, `misaka.decode`, `prompt_tokens` after the run; before it, the tokenizer id (D2) and the served table (ADR-0096 D8); no `/v1/tokenize` (§8) |
| 38 capability registry | held — `misaka.limits` (D2) on `/v1/models`; the class table and the components manifest (ADR-0096 D10) |
| 39 OpenAI/Anthropic-compatible API | held — the OpenAI surface (ADR-0096 D1); no Anthropic shape (named, not built) |
| 40 provider separation, 41 AI gateway | held — the gateway IS the adapter (`misaka-palw-gateway`); the chain never sees a provider |
| 42 audit log, 43 reproducibility | held — the record (`records.rs`, app), the artifact JSON beside every job (`fp-v3-gateway-artifact.v1`), and the claim itself: model, prompt ids, ids, roots, ruleset id — a commitment IS a reproducibility record |
| 44–46 cost, latency, fallback | app — the backends; on chain the price is leaves (ADR-0074 D5) and the latency is the row (§1.5) |
| 47 local/private model | held — `--answer-never-commit` with a bond-less identity (ADR-0096 D10); `PanelDa` for the ids where armed (ADR-0077 D16) |
| 48 secret redaction | held in part — SA-7 (the worker's stderr withheld), the reachable-secret boot check (ADR-0079); prompt content is the person's, and `PublicDa` says so beside the box (ADR-0096 D7's last bullet) |
| 49–51 prompt-injection, tool-result injection, isolation | app — the entrance renders a tool's reply as a `user`-role text turn (ADR-0096 D2), which is the isolation the model's own template offers and no more; nothing on chain executes a tool |
| 52–56 chunking, retrieval, reranking, relevance, large files | app / not this lane's |
| 57–58 structured tool output, typed tools | held — `tools` as JSON under RFC 8785 (ADR-0096 D2), the JSON-Schema subset (D3/D7) |
| 59 agent state machine | app |
| 60 the target architecture | the entrance (ADR-0096) over the lane (ADR-0077) over the court; the "gateway" of the diagram is this repository's gateway and the "runtime" is the Studio |

**What the list asks for that the lane refuses on purpose**, in one place: a sampler the seat does
not replay (ADR-0082 D11), a cache across jobs (ADR-0096 D5), a silently shortened context
(ADR-0096 D1), a modality the graph does not compute (ADR-0096 D1). **What it asks for that was
missing and is now here**: the limits (D2) and the fit (D1). **What it asks for that is named and
not built**: an Anthropic-shaped surface, a `/v1/tokenize` route, and the sharded seat that a
K3-class network would stand on (D5).

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md -->

</details>

<a id="source-0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation"></a>

<details>
<summary>0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md — 原文 L1–L395</summary>

[原文を開く](../adr/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md) · SHA-256: `875e343ab173ac164f1d3a90310f4781d97e240cf26f8222bf0ddd17f98e2be9`

<!-- ADR_SOURCE_BEGIN:0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md -->
~~~~~~~~markdown
# ADR-0110 — A context limit is activated from reproducible public vectors, not the maintainer's workstation

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, written from the operator's decision
  of the same day (ADR-0103 §10.5 answered by ADR-0111; this is the other half of that answer).
  **Decisions 1–5 IMPLEMENTED the same day (§9)**; the 512, 4,096 and 32,768-position vectors pass
  every stage and are pinned. Consensus-inert: no object, acceptance rule, fence, parameter or
  fingerprint moves. A fleet takes it by an ordinary rebuild.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (§10.5: no 2M claim ran; the "done when" was met at devnet widths and at 2M only in the
  generators' tables), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
  last prosecution gap before arming), [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md) and [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) §5 (a
  figure is a generated artifact, and a figure an ADR prints that the generator does not is a bug in
  the document), and ADR-0108 on `feat/adr-0108-extension-envelope` (Decision 4: a receipt is
  evidence of reproduction and never a vote; Decision 6: a ruleset change is described and costed,
  and activated only by a release).
* Supersedes nothing. Amends nothing in consensus.

## 0. The sentence this ADR is

**A context limit is armed only by a release whose evidence anyone can reproduce. The evidence is
a fixed vector — a name, a seed and a geometry, from which the class, the weights and the job are
all derived with no free parameter — and one verifier, `misaka palw verify-context`, which runs the
network's own pipeline over it: produce, commit, seat, court, availability, fit. It prints one
canonical document. Its consensus facts must match byte for byte on every honest machine; its host
facts are reported and never compared. A signed receipt of that document says who reproduced it.
It is evidence, not a vote: no code path reads it, and no number of receipts arms anything.**

## 1. What was missing

ADR-0103 made every term the chain carries per claim constant or logarithmic in the context, and
ADR-0111 closed the last prosecution gap. The regime's evidence at 2M is still the generator's
arithmetic. No 2M claim ran, because the dense row's cache at 2M is 112 GiB and the maintainer's Mac
seats nothing that size (ADR-0103 §10.5). The operator's decision states the consequence both ways.
The code can ship to testnet unarmed. The context is not armed on a result only one workstation
could see.

Arming a width needs five things, and none of them existed:

1. **A job anyone can regenerate bit for bit.** Every fixture that exercises the held rows lives in
   a `#[cfg(test)]` helper, and the drills pick their prompts from a live chain's anchors.
2. **One command that runs the pipeline, not a model of it.** The generator (`palw-model-fit`)
   prices walls from predicates; the drills run a devnet. Neither replays a named job at a named
   width end to end.
3. **A document that separates what must agree from what may differ.** Two honest machines agree on
   every root and disagree on every timing, and a report that mixes them cannot be compared.
4. **A signature that says who ran it without making it a vote** — the thing the operator ruled
   out: a user's "PASS" signature is never consensus truth, and no "three validator attestations"
   activate anything.
5. **CI that runs the small widths on every change**, so that "it worked at 2M once" is not the only
   statement anyone can make.

## 2. Decisions

**Decision 1 — a vector is a name, a seed and a geometry; everything else is derived.**
`PalwContextVectorV1` names the family's held row (the dense A16 graph-v7 row; a hybrid row when its
held drills exist), its geometry (layers, widths, heads, vocabulary, `n_ctx`, tile length), the job's
prefill and decode counts, the prompt-ids form (the Merkle form the held mint mandates) and a seed.
From those, and nothing else:

* the class is the family's profile projected at the geometry, and its id is `H(profile)`;
* the weights are the family's deterministic derivation from the seed (the floor's and the test
  fixtures' own derivation — no file is read);
* the job's anchor is `BLAKE2b-512(key = "misaka-palw/context-vector/anchor/v1", seed)`, and the
  prompt is the family's own canonical prompt for that anchor (`base0_rc_job_v1`'s keyed stream,
  each id reduced modulo the vocabulary). It is committed under the Merkle form and carried by a
  free-prompt job, the executor's own entrance, whose every other field is fixed or derived from
  the seed. No generator is new: a vector reuses the derivations a node already runs.

`vector_id = BLAKE2b-512(key = "misaka-palw/context-vector/v1", canonical bytes of the vector)`. The
shipped vectors are named by width (`0110-dense-v7-512`, `…-4k`, `…-32k`, `…-128k`, `…-2m`), and
each name's seed is `BLAKE2b-512(key = "misaka-palw/context-vector/seed/v1", name)`, so nobody
chooses a seed — a vector cannot be re-rolled until it passes. The geometry is the thinnest the held
row admits, not a real model's. What a vector proves is the protocol at the width: the tree depths,
the interval geometry, the court's openings at a deep ladder, the availability units at a large
chunk count. What a real model costs at that width is the generator's table, and Decision 7 says so.

**Decision 2 — the verifier is the network's pipeline, stage by stage, through the seam a node
holds.** `verify-context` builds the family backend exactly as a node's registered-row path does,
and runs these stages in order. A stage that fails still lets the later stages report by name:

| stage | what runs | pass means |
|---|---|---|
| `produce` | the job, executed and retained as the executor retains it (the fold) | a capture, and the answer's ids |
| `commit` | the binding and the claim's roots, derived from the capture | the roots the chain would record, each printed |
| `seat` | every interval opened and verified along the class's route (recompute, or resume from served state — ADR-0103 Decision 2) | every interval `Valid` |
| `court` | the executor's evidence (ADR-0111 Decision 1) at the vector's sampled leaves — the first prefill leaf, an interval's first leaf, the last decode leaf — judged by `palw_one_move_verdict_v1`; then a capture tampered at one of them, verified by a seat and judged the same way | honest leaves `FalseAccusation`; the tampered interval a fault, and its leaf `ExecutorGuilty` |
| `availability` | each held unit (a prompt tile, a state chunk, a step range, a leaf's evidence) answered from the retention and checked by `palw_held_da_check_disclosure_v1` | every answer accepted |
| `fit` | `palw_model_fit_v2` under the held regime at the vector's width, on the named ruleset | every chain wall constant or logarithmic, and admitted |

**Decision 3 — one canonical document, and its id covers only what must agree.**
`PalwContextVerificationV1` is RFC 8785 (JCS) JSON, canonicalised by the one canonicaliser this tree
already has (`misaka_palw_derive::canon_json`). It has five sections:

* `vector` — the vector, and `vector_id`;
* `ruleset` — the network, its `consensus_params_id`, and the fences the fit read;
* `consensus` — every fact the chain would check or a seat would compare: the class id, the artifact
  root, the execution, trace, output, step and checkpoint roots, the leaf and checkpoint counts, the
  interval count and width, each sampled leaf's evidence size and verdict, each availability answer's
  size, the fit's orders and needs;
* `verdicts` — each stage `pass`, `fail (reason)` or `skipped (reason)`. There is no bare overall
  PASS: the document says which stages ran;
* `host` — wall time per stage, peak resident memory, the CPU and operating system, the thread
  count, the build's version and commit. It is reported and never compared.

`document_id = BLAKE2b-512(key = "misaka-palw/context-verification/v1", canonical bytes of the
document without host)`. Two machines that reproduce a vector print the same `document_id`, or one
of them has found a bug, and the two documents are the preimage of the bug report.

**Decision 4 — a receipt is evidence, not a vote.** This restates ADR-0108 Decision 4's rule for
this kind. `--sign-with <key>` writes a `PalwContextReceiptV1`: the `document_id`, the canonical
host section, and an ML-DSA-87 signature by the verifier's key over the receipt's canonical bytes,
under the context `misaka-palw/context-receipt/v1`. That context is a constant of the verifier and is
not added to the bundle's `signature_contexts_root`: the chain verifies nothing signed under it, so
its consensus set has no reason to know it. A receipt cannot be replayed as an attestation, a bond
message or anything else ML-DSA signs in this system, and nothing signed for the chain verifies as a
receipt. **No code path in `kaspad`, `kaspa-consensus`, `kaspa-consensus-core` or the SDK reads a
receipt, and no count of receipts changes any admission, activation, fence, seat, price or share**
(§5 invariant 6). A receipt proves one thing: a named key ran this build over this vector and
reported this `document_id`.

**Decision 5 — CI runs the small widths on every change; the 2M vector is published, not assumed.**
The 512-position vector runs in the default test suite, and the 4,096 and 32,768-position vectors run
in the release-mode vector job. Each document's `document_id` is pinned, so a change that moves a
root, a count, a size or a verdict at any of those widths fails by name. The 131,072-position and 2M
vectors are external runs. The command is published, and so is the first reproduction's document,
with the host and the time it took. Its id is then pinned beside the others. It is pinned because a
reproduction exists, not because enough of them agreed. The measured cost of each width is §9's, and
the 2M vector's is not small (§9.3).

**Decision 6 — arming a wider context is a release, and the release cites its evidence.** A ruleset
move that arms `palw_held_context`, or admits a class whose context was unreachable before, is
ADR-0108's tier C: a coordinated release with a fork-id notice, never a manifest and never a count.
Its ADR names, before any height is scheduled:

1. the vectors at the widths the move admits, each with its `vector_id` and the `document_id` the
   release build prints — green in CI up to 32,768 positions, and published above that;
2. ADR-0111's two drills green on the release build;
3. every open prosecution gap at the widths the move admits closed, or named as accepted (the last
   one found, ADR-0085 Decision 3's close from served intervals against a lying interval, was
   closed the day it was found — ADR-0111 §8.5);
4. the fingerprint, identity and schedule the release prints, and which of them move.

**Decision 7 — what a vector does not prove, said in the document.** A vector proves the protocol at
a width with the thinnest model the row admits. It does not prove:

* the real model's resources at that width — the cache, the fetch, the replay time a seat needs.
  Those are `palw-model-fit --preset held`'s table (ADR-0103 §10.2), and the document names it rather
  than restating it;
* anything about a network of many seats, its latency or its bandwidth;
* that a class's weights are what anyone said they were — that is the artifact root's job, not this
  one.

## 3. The document (abridged)

```jsonc
{
  "document": "misaka-palw/context-verification/v1",
  "vector": { "vector": "misaka-palw/context-vector/v1", "name": "0110-dense-v7-4k",
              "family": "a16-graph-v7", "geometry": { "layers": 2, "hidden": 8, "ffn": 8, "heads": 2,
              "kv_heads": 2, "head_dim": 4, "vocab": 64, "n_ctx": 4096, "tile_len": 4 },
              "job": { "prefill": 4092, "decode": 4, "prompt_ids_form": "merkle-v1" },
              "seed": "<128 hex>" },
  "vector_id": "<128 hex>",
  "ruleset": { "network": "devnet", "preset": "held", "consensus_params_id": "<64 hex>" },
  "consensus": { "class_id": "…", "artifact_root": "…", "execution_root": "…", "trace_root": "…",
                 "output_root": "…", "step_merkle_root": "…", "checkpoint_merkle_root": "…",
                 "step_leaf_count": 0, "checkpoint_count": 0, "interval_count": 0, "interval_width": 0,
                 "court": [ { "leaf": 0, "evidence_bytes": 0, "honest": "false-accusation",
                              "tampered": "executor-guilty" } ],
                 "availability": [ { "unit": "state-chunk", "answer_bytes": 0, "accepted": true } ],
                 "fit": [ { "wall": "…", "order": "logarithmic", "need": 0, "limit": 0 } ] },
  "verdicts": { "produce": "pass", "commit": "pass", "seat": "pass", "court": "pass",
                "availability": "pass", "fit": "pass" },
  "document_id": "<128 hex>",
  "host": { "stages_ms": { "produce": 0 }, "peak_rss_bytes": 0, "cpu": "…", "os": "…",
            "threads": 0, "build": "…" }
}
```

## 4. The CLI

```text
misaka palw verify-context --vector <name> | --vector-file <file>
                           [--ruleset devnet-held] [--stages produce,commit,seat,court,availability,fit]
                           [--out <document.json>] [--sign-with <key-file>]
misaka palw verify-context --list                 the shipped vectors, their ids and what each costs to run
misaka palw verify-receipt <receipt.json> [--document <document.json>]
```

`--vector-file` runs a vector that is not shipped, so a stranger can pose a width, and its document
says `shipped: false`. `verify-receipt` checks the signature, recomputes the `document_id` from the
document if one is given, and refuses a receipt whose context is not this ADR's.

## 5. Invariants the tests must hold

```text
1  The generator has no free parameter: a vector's prompt, anchor, weights and class are pure
   functions of its name, seed and geometry, and two calls agree byte for byte.
2  At 512, 4,096 and 32,768 positions every stage passes, and the consensus section equals its pin.
3  The document_id excludes the host section: changing any host field leaves it unchanged, and
   changing any consensus field moves it.
4  A tampered vector is caught at its leaf: the seat's interval is a fault, and the evidence is
   ExecutorGuilty.
5  A receipt round-trips: sign, canonicalise, verify. A changed byte fails; a signature under
   another context fails; an unsigned document verifies as unsigned and says so.
6  No consensus, node or SDK crate reads a receipt or a document (checked by a pin over their
   sources, the way ADR-0108 I-9 is).
```

## 6. Order of work

1. The vector, the generator and the stages, in `misaka-palw-base0` (the seam's families live there).
2. The document, its canonical form and its id; the receipt; the CLI verbs in `misaka-cli`.
3. The pinned small vectors in the suite; the 128K vector's cost measured and placed.
4. The 2M vector's command published; its first document published and pinned.

## 7. What is deliberately not decided

* **The hybrid row's vectors**: the same stages once its held drills exist.
* **Where documents are published.** A document is content-addressed by its `document_id`, so where
  it lives does not matter (ADR-0108 Decision 9's reasoning).
* **The height of any activation**, which is the release's.

## 8. Number hygiene

The operator's draft called this ADR 0105. 0104–0109 were resident elsewhere when it was written
(ADR-0111 §7): 0108 and 0109 on `origin/main`, the extension envelope and the bridge liveness. No 0110
was resident on any branch on 2026-09-11 when it was first committed, and it keeps the number,
because its vectors are named by it (`0110-dense-v7-…`) and every seed and pin is derived from those
names. The leaf-demand ADR it builds on, written before it as 0109, is ADR-0111.

## 9. Implementation record (2026-09-11)

Built on `feat/adr-0103-held-context` (`528d1bbb` and its successors), and merged into the unarmed
testnet-11 integration branch `integ/t11-adr0103-unarmed`. Nothing consensus-side moved.

### 9.1 Where each Decision lives

| Decision | where | what pins it |
|---|---|---|
| **1** the vector | `misaka_palw_base0::context_vector` — `PalwContextVectorV1`, the shipped five (`palw_context_vectors_v1`), the seed `palw_context_vector_seed_v1(name)`, the anchor and the job fields keyed off the seed, the class from `qwen25_a16_profile_v7` at `palw_context_vector_geometry_v1`, the weights from `Base0ArtifactV1::derive_deterministic` | `a_vector_is_a_pure_function_of_its_name_seed_and_geometry` |
| **2** the stages | `palw_verify_context_vector_v1` over the registered-row `Qwen25A16Backend`, judged under `PalwContextRulesetV1::devnet_held_v1` (the `--palw-held-context-devnet` mint) | the pinned vectors below |
| **3** the document | `PalwContextFindingsV1::{agreed_json_v1, document_id, document_json_v1}`; `palw_canonical_json_v1`, checked against `misaka_palw_derive::canon_json` on every write | `the_canonical_form_sorts_and_escapes`; `the_document_id_excludes_the_host_and_moves_with_any_consensus_fact` |
| **4** the receipt | `misaka palw verify-context --sign-with` and `misaka palw verify-receipt` (`misaka/apps/cli/src/palw_verify_context.rs`), ML-DSA-87 under `misaka-palw/context-receipt/v1` | `a_receipt_round_trips_and_refuses_a_changed_byte_or_another_context`; `no_consensus_node_or_sdk_crate_reads_a_receipt` |
| **5** the pins | `the_512_vector_passes_every_stage_and_is_pinned` (default suite); `the_4k_…` and `the_32k_…` behind `--ignored` (`cargo test --release -p misaka-palw-base0 --lib -- --ignored context_vector`) | themselves; the 512 id is the same from a debug and a release build, and on the feature and the integration branch |

### 9.2 What the three widths measured

The dense graph-v7 row at the thinnest geometry, on one M-series host that was sharing its CPU
(load average 20–28) — so the host columns are an upper bound, and they are the columns nobody
compares:

| | 512 | 4,096 | 32,768 | order |
|---|---|---|---|---|
| step leaves | 51,180 | 409,580 | 3,276,780 | linear (held by the executor) |
| capture retained | 81 KB | 610 KB | 4.84 MB | linear (held by the executor) |
| a leaf's evidence (the one-move object) | 6,931 B | 7,315 B | 7,699 B | **logarithmic** — 128 B a doubling |
| a prompt tile answer | 457 B | 649 B | 841 B | **logarithmic** |
| a state chunk answer | 999 B | 1,191 B | 1,383 B | **logarithmic** |
| the widest step range answer | 66,129 B | 66,321 B | 66,513 B | **logarithmic** |
| an interval opening (the widest of four) | 6,077 B | 7,741 B | 19,133 B | linear in the interval's blocks, inside the lane's cap by the width rule |
| produce | 0.57 s | 1.9 s | 42.9 s | host |
| seat (four intervals, recompute route) | 1.8 s | 5.3 s | 217 s | host |
| court (three honest leaves and the drill) | 3.6 s | 9.0 s | 254 s | host |
| availability (five answers) | 0.4 s | 1.3 s | 87 s | host |
| peak resident | 47 MB | 343 MB | 1.64 GB | host |

Every stage passed at every width. **Everything the chain would carry per claim grew by a constant
number of bytes a doubling**, which is ADR-0103's R-held measured end to end rather than read off
a generator. Everything that grew with the context is held by the executor or the seat.

### 9.3 What the widths found

* **The seat's recompute is super-linear in the context on this engine.** From 4,096 to 32,768
  positions (8×) the seat stage took 41× longer and availability 67×. The recompute replays the
  prefix, and the attention over it is quadratic, even at the thinnest geometry. ADR-0103 Decision
  2's route rule prices a recompute linearly (`n_ctx × replay_ms_per_position`), so at wide contexts
  it is optimistic about when `Recompute` fits the window. A release that arms a wide context must
  price the seat with a measured curve, or route it to `Resume`. This is recorded against ADR-0103
  Decision 2; nothing here changes it.
* **The 2M vector is not an evening's run.** Extrapolated from 32,768 at the measured exponents
  (n^1.5 to n^2), the thin row at 2M costs 12–28 days of one process on this host. "External" means
  a large machine for days, or engine work first: a prefill that parallelises across positions, and a
  seat that resumes rather than recomputes. The 131,072-position vector extrapolates to about three
  hours and is the next external run.

### 9.4 What remains

* ~~The release-mode vector job in CI.~~ **Built 2026-09-11.** The `Context vectors (release)` job
  in `.github/workflows/ci.yaml` runs the `context-vectors` gate of `scripts/ci-gates.sh`
  (`cargo test --release -p misaka-palw-base0 --lib -- --ignored context_vector`). The gate names
  both vectors in its evidence and pins the count at two, so a third vector added under that
  filter turns it red instead of lengthening CI. `workflow-parity` holds the two spellings equal.
* ~~ADR-0103 Decision 2's route rule against the measured curve.~~ **Amended 2026-09-11** (ADR-0103
  §10.6): a replay now pays for its history. On this row the 4,096 and 32,768-position vectors take
  the Resume route. They were re-pinned for that alone: `9c5a772c…` and `848d9352…`, and on a less
  loaded host the 32,768 run took 398 s. The 512-position pin did not move.
* The first external documents for 128K and 2M, and their pins. **The 2M vector cannot run on
  this tree**: ADR-0103 §10.7 found that the A16 tier's attention ops refuse a history longer
  than 2^18. The widest A16 vector anything here can produce is 262,144 positions.

### 9.5 How the 2M vector runs (2026-09-11)

The operator asked how the 2M vector should be run. Answering meant running the widths below it
and reading where the time went. Four findings, in the order they bind:

1. **The wall.** No A16-family class executes past 262,143 positions (ADR-0103 §10.7). The 2M
   vector is therefore refused before produce, by name (`palw_context_vector_blocked_v1`), and
   `--list` says so. `0110-dense-v7-256k` (262,144 positions) was added as the widest vector the
   tier can run. Running 2M needs a ruleset decision first: a separate history bound for the
   attention ops, with its exactness argued (it holds to `2^26` rows) and its Q24 precision
   answered (about three bits a probability at `2^21`). A faster machine does not help.
2. **The pool's own overhead was the system time.** Sampled on the 32,768-position vector, 46% of
   thread time was `swtch_pri` and 34% `__psynch_cvwait`: rayon's workers yielding and waiting.
   Two things kept waking them. The fused attention site tiled one registered triple over
   `heads × history` entries twice per call and materialised four history-long rows. The kernels
   also went to the pool on a channel count alone, even for the thin row's 512-MAC unembedding.
   `kernels::a16_attn_fused_uniform_fast` reads each triple once and reuses its scratch, and
   `parallel_worth_it` / `FUSED_CHUNK_WORK` send only real work to the pool. Both are
   bit-identical by construction and pinned against the catalog. The 4,096-position vector went
   from 18.9 s (83 s of system time) to 10.9 s (0.14 s); the 32,768 one from 398 s to 274 s
   (1,312 s of system time to 240 s), on one host at like load.
3. **One claim's questions each walked the job from row zero.** The seat's interval starts, the
   executor's opening anchors, a leaf's evidence and the held units all ask for positions of ONE
   job. `fp_recompute::base0_fp_a16_seat_state_v1` keeps the dense tier's walk between them: an
   attention cache only appends, so a later question continues it and an earlier one reads its
   first rows. `a_resumed_walk_is_the_walk_from_zero` holds it equal to the walk from zero in every
   order.
4. **The court's tampered half re-executes the job dense**, about 700 bytes a leaf. That is 9 GB
   at 128K. At 256K (26.2 M leaves, past `PALW_CONTEXT_TAMPER_MAX_LEAVES_V1`) the stage says so
   and is skipped by name. The honest half still runs at every width. A tamper that re-folds
   instead of re-executing is the next engine change the widest vectors need, and it is not
   built.

**The runs, on this host** (a 12-core M-series, 24 GB, shared with a build), after the three
changes:

| | 4,096 | 32,768 | 131,072 |
|---|---|---|---|
| produce | 1.0 s | 33 s | 312 s |
| seat (four intervals, the Resume route) | 2.8 s | 89 s | 819 s |
| court (three honest leaves and the drill) | 5.9 s | 139 s | 1,402 s |
| availability (five answers) | 0.4 s | 12 s | 140 s |
| whole run | 10.9 s | 274 s | 2,682 s (44.7 min) |
| peak resident | 373 MB | 2.35 GB | 7.5 GB |
| a leaf's evidence | 7,315 B | 7,699 B | 8,019 B |
| `document_id` | `9c5a772c…` | `848d9352…` | `7bc3f88b…` |

The 131,072-position vector's first reproduction is this run. Its id is pinned in
`misaka/palw/base0/tests/context_vector_external.rs` — its own test target, so the release-mode
job's filter (which pins its count at two) cannot see it — and it runs by hand:
`cargo test --release -p misaka-palw-base0 --test context_vector_external -- --ignored`. Every
chain-side term still reads a constant number of bytes a doubling: the evidence grew 320 bytes
over the two doublings from 32,768, the answers likewise, and the roots, counts and verdicts are
the table's. The 262,144-position vector is the next external run; the 2M one waits on §10.7 of
ADR-0103.

### 9.6 Re-pinned for ADR-0119 (2026-09-12)

ADR-0119 walks a held class at the regime's `2^40` ladder instead of the held devnet's `2^26`, and
every vector runs a held class. So each document's fit reports 40 levels on its ladder row, and
its widest close, priced at that depth, grows. The 512 vector's close grows by 5,376 bytes, and
the three wider ones' by 896 (one path, fourteen levels deeper). Diffed field by field against
the documents the previous pins were taken from, nothing else moved: every root, count, route,
answer and verdict is the previous run's. With the class ladder held at the network's, the same
tree prints the previous 512 id. All four ids were reproduced by `misaka palw verify-context` in
release on the §9.5 host. The 128K run took 47.6 minutes beside a workspace build, peaking at
6.2 GB.

| | 512 | 4,096 | 32,768 | 131,072 |
|---|---|---|---|---|
| widest close (bytes) | 15,231 → 20,607 | 38,448 → 39,344 | 38,640 → 39,536 | 38,768 → 39,664 |
| `document_id` | `02818c10…` (was `d2364615…`) | `a54a5ee6…` (was `9c5a772c…`) | `0b8b191d…` (was `848d9352…`) | `d6464524…` (was `7bc3f88b…`) |

## Mission alignment amendment — 2026-10-07

public vectorsによるcontext activationの原則を強め、activation evidenceにfresh non-seat prosecutionを含める。maintainer workstationやliar-internal ServedViewを外部取得の代用にしない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md -->

</details>

<a id="source-0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form"></a>

<details>
<summary>0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md — 原文 L1–L285</summary>

[原文を開く](../adr/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md) · SHA-256: `c418227ea951b3fde01490e16879e86ca3aeaa02c1468f18300a11b96f527925`

<!-- ADR_SOURCE_BEGIN:0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md -->
~~~~~~~~markdown
# ADR-0118 — The held regime arrives at a height, and a held class carries its own prompt form

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0103-held-context`, at the operator's
  instruction ("held方式の実装完了をgoalとする"). The operator decided the same day that the
  regime takes a flag day of its own on testnet-11, at a height they will choose, and not the
  audit's 4,000 — and, asked again on 2026-09-12, chose **DAA 7,000**, the deep-audit fence's
  height, so one release carries both; **moved to DAA 6,000 on 2026-09-17** (the operator: the wait was
  too long; the tip was 5,798, and the deployed 7,000 release never reaches its height — it is refused
  from 6,000). testnet-11 therefore arms the regime at 6,000
  (`PALW_RC_HELD_FENCE_DAA = Some(6_000)`; its fingerprint on this branch alone was `80524c3b…`,
  merged with the 4,000 release that carries `palw_prefill_draw` it is `c2e5ffbe…`, and the release
  that also carries the deep-audit fence is re-pinned when it is assembled). Every other preset leaves the regime dormant.
* Builds on: [0081](0081-long-context-the-input-is-a-state-chain.md) (Decision 3: the prompt-ids
  form), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the held regime), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md) (the
  held DA court's leaf demand), [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
  (the history bound is the class's — the same move this ADR makes for the prompt form).
* Amends: ADR-0103's assembly rules ("V4 and the tiled prompt ids at the fence") to "from genesis";
  ADR-0081 Decision 3's "the form is the network's" to "the form is the class's, and the network's
  for every class but a held one".
* Amended by: [0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
  (the same day), which lifts §4's limitation at the chain — a held class is walked at the regime's
  `2^40` ladder — and gives §4's PanelDa jobs their executor path.

## 0. The sentence this ADR is

**A network minted before the held regime takes it at a height: the fence commits the V4 signing
contexts into the identity itself, a held class commits its prompt ids as the tiled Merkle root
while every other class keeps the network's genesis form, a prompt tile is demanded only of a
claim whose ids are a tile tree, and the carriers stay the chain's — so on a network minted flat
a held class commits under `PanelDa`.**

## 1. What was found

ADR-0103's regime was written as a mint. `validate_palw_v2` refused `palw_held_context` unless the
bundle stated the COMPLETE_V4 context set and the network's prompt ids were Merkle (trace format
4), and both of those are genesis-only by construction: the context root is inside the ruleset id,
and `palw_prompt_ids_merkle` is refused at any height but 0 because "no reader of a
`prompt_token_ids_hash` holds the job's anchor height" (audit M-8). testnet-11 was minted flat,
with the V1 set, and it takes consensus changes by activation and never by a re-genesis. So on the
network the regime was meant for next, it could not be armed at all. Probed on this branch's base:
`validate_palw_v2` for testnet-11 with the regime, the one-move court and the pins at one height
fails three ways — the shard court's V3 contexts, the regime's V4 contexts, and the flat ids.

Mapping every reader of the three facts (the held-regime survey of 2026-09-12) found:

* **No runtime verification reads the bundle's context root.** The acceptance layer verifies each
  object under a hardcoded context and the fold binds `signature: _`; the root is read only by
  `validate_palw_v2`, the bundle's own validation and the identity. testnet-11 already verifies,
  at runtime, ten COMPLETE_V2 contexts its frozen nine-context root does not name.
* **The prompt form's readers either hold the class's profile or hold no class at all.** The
  admission gate, the backends, the fp worker, the court and the held DA court hold the profile
  (the court's through a binding authenticated against the claim's execution root). The
  transaction's stateless check and the extraction walk hold neither a class nor, for the first,
  a height.
* **A held class's ids never ride** (ADR-0103 Decision 4), and a held class exists on a network
  only past the fence, where no bisection is opened — so the one reader that grades a bisection's
  close against the network's form never meets a held class.
* **The held DA court's `PromptIdsTile` demand had no form check.** On a network minted flat, a
  demand of a tile of a flat digest can never be answered, and the sweep slashes the producer for
  withholding at the disclosure deadline — an honest producer, by its silence, at the hands of any
  bond holding the claim's binding.
* **The validator sign gate spelled the flat digest alone**, so the rail refused to sign every job
  committed under the Merkle form, on any network.

## 2. The requirement

testnet-11 can arm the held regime at a height by a one-line change the operator makes. At that
height every existing class computes, commits and is judged exactly as before; a held class
registered past it commits ids its seats, courts and tools all read in one form; nobody can be
slashed for failing to open a unit their commitment does not contain; and no node can read a
different identity, form or context from another for the same class at the same point.

## 3. Decisions

**Decision 1 — past genesis, the fence commits the V4 contexts itself.** `validate_palw_v2` asks
for the COMPLETE_V4 bundle root only where the regime is armed from genesis — where the bundle is
being minted, and `palw_held_context_mint_v1` states it. Armed at a height over a bundle minted
before the regime, the objects the regime verifies under V4's contexts (the checkpoint court and
the held DA court's accusations and disclosures) did not exist before the fence, so no signature's
meaning changes at it; and where the bundle names an older set, `consensus_params_id` writes the
V4 root beside the fence's height, so two builds that spell a V4 context differently cannot share
an identity. M-8's rule, kept by the fence rather than by the bundle. A bundle that states V4 —
every mint that takes the regime from genesis — commits the set through the ruleset id already and
is not written twice, so no held network minted before this ADR changes its identity (the ADR-0110
vectors' documents, which name their ruleset's id, are unmoved).

**Decision 2 — the one-move court takes the fence's contexts at the same height.** The shard
court's "the bundle covers V3" check is waived where the regime is armed past genesis at or below
the court's own height: V4 covers V3, and Decision 1 committed V4 there. The regime requires the
one-move court at or below its height, so on a network that armed neither, the two land on one
height.

**Decision 3 — the prompt-ids form is the class's.** `palw_prompt_ids_form_of_class_v1(network,
profile)` is the tiled Merkle root for a profile under the held map and the network's genesis form
for every other. It is never a function of a height — the reason the network's form is
genesis-only still holds — but of the class, whose map is inside its id. Every reader that decides
the form reads it through this function: the admission gate (the held arm, the fit, and the ladder
rules' cost shape), the three backends and the fp worker (which store their class's form when
handed the network's), the panel (`class_prompt_ids_form`, off the chain's registration row or
this build's tables, remembered per class because the answer is a function of the id), the held DA
court, and the node's per-class producer facts (`class_prompt_ids_merkle`, wire version 8 and gRPC
field 30) that a gateway re-binds a worker's result under. On a network minted Merkle every class's
form is Merkle, so nothing there moves.

**Decision 4 — a prompt tile is demanded only of a claim whose ids are a tile tree.** The held DA
court refuses a `PromptIdsTile` accusation unless the claim's class commits Merkle ids, by name
("this claim's prompt ids are committed flat, and a flat digest has no tiles"), at the point the
binding says which class it is. The other units — a state chunk, a run of step leaves, one leaf —
read no prompt id and are unchanged.

**Decision 5 — the carriers are the chain's.** The transaction's stateless check and the
extraction walk hold no class, so they read the network's genesis form as before, and the one
builder that mirrors them for a class it chose — the panel's canonical claim — passes the network's
form (a canonical payload carries no ids, so the form reads nothing there either way). On a network
minted flat a PublicDa carrier therefore cannot hold a held class's Merkle-committed ids: it is
refused at the door, identically on every node. A held class commits under `PanelDa`, which the
regime already requires for its widest job and whose carrier holds no ids and meets no form. The
gateway refuses a PublicDa request for such a class before the inference, by name, and says to
serve it with `--privacy panel-da`. The rail derives the form from the worker's result, as it did;
the sign gate now binds a result's ids under either form (the two digests are domain-separated, so
no list binds one job under both), and which form the class commits in is the chain's and the
seats' to hold the claim to.

**Decision 6 — testnet-11's flag day is one line, and it is the operator's: DAA 6,000.**
`PALW_RC_HELD_FENCE_DAA: Option<u64> = Some(7_000)`, and `palw_arm_held_regime_at_v1` arms the three
fences the regime takes together on testnet-11: `palw_held_context`, the one-move court and the
retention pins (the k-ary court, `PanelDa` and the DA court are already armed there, from genesis
and at 1,900). Every fence at one height ships in one build — `fork_id_v1` digests the fired
heights, not the fence set — so the height is one no other schedule entry uses, or one whose other
fences ship in the same release: not 3,500, and not 4,000, which releases the audit and
`palw_prefill_draw` without the regime. 6,000 is shared with the deep-audit fence
(`palw_audit_2026_09_11_deep`), so no build may carry one of the two without the other.

**Decision 7 — a chain-registered class is served at the ruleset's ladder.** Every tabled lineage
builds its backend with `with_step_ladder_cap(court.max_step_leaf_count())`; the SDK's chain arm
(`resolve_chain_registered`) did not, and left both families at the executor's `2^22` default. A
held class reaches a network like testnet-11 only by registration, and a dense row's job passes
`2^22` leaves after a few dozen positions, so its own producer and seats refused it, and its
retained blocks were addressed at a level the seats did not derive. The chain arm now takes the
ruleset's ladder, as the tables do. Node policy; no rule moves.

## 4. What arming buys on testnet-11, measured

**Less than its name.** The held map lifts the context ceiling; it does not lift the step ladder
or the advertised prompt cap, and both are testnet-11's genesis's: the ladder is `2^26` because a
bisection opened before any fence must still be playable inside the court window (ADR-0103,
`a_ladder_past_the_clock_needs_the_regime_from_genesis`), and the free-prompt cap is 512. With the
regime armed at 6,000, the admission gate reads the dense lineage's held row (graph-v7) as:

| context | at testnet-11's ladder (`2^26`) | at the regime's ladder (`2^48`, not mintable on a live chain) |
|---|---|---|
| 512 | admitted | admitted |
| 1,024 | refused: the whole context as prefill is past `2^26`, and the close's order cannot be read | admitted |
| 32,768 and 2^21 | refused before the gate: the canonical job alone is 67,161,216 leaves | admitted |

So on testnet-11 the regime delivers its courts and its objects, and a held class up to about 512
positions that produces attempts and canonical claims — not the 2^21 ADR-0116 widened the history
for. A held class's USER prompts have no executor path there yet either: the chain refuses a
PublicDa carrier of its Merkle-committed ids (Decision 5), and the fp worker executes PublicDa jobs
only (`precheck_request_v1`), so the gateway has nothing to hand a PanelDa job to. This corrects the report of
2026-09-11, which said the 2^21 bound "changes nothing until the regime is armed" as if arming
were the only step. What would make 2^21 real there is the same move this ADR makes for the prompt
form, made for the ladder and the prompt cap: a held class exists only past the fence, where no
bisection is opened, so the bisection's clock — the only reason the ladder is frozen — never
binds a held class. That is §7's first question, not a decision taken here.

## 5. Invariants the tests hold

1. **I-1, testnet-11 takes the regime at its 6,000 flag day** — a height no released build schedules,
   shared only with the audit's deep fence, which ships in the same release: the preset is the
   helper at 6,000 beside that fence; it validates with the network's ids still flat, gains its
   schedule entry and moves the identity from the build before it; armed from genesis over the same
   bundle it is refused (`testnet_11_takes_the_held_regime_at_its_7000_flag_day`), and armed at a
   scheduled height it is invisible to the fork-id gate
   (`a_held_fence_at_a_scheduled_height_is_invisible_to_the_fork_id_gate`).
   The fork-id gate, the schedule and the fingerprint pins name 6,000 (`fork_id_v1`'s measured
   schedules, `shipped_presets_have_pinned_fingerprints`).
2. **I-2, the form is the class's**: the held row is priced Merkle and admitted under a court
   reading the flat form, and the shipped row keeps the flat form byte for byte
   (`on_a_network_minted_flat_a_held_class_is_priced_at_its_own_merkle_form_and_admitted`); a
   backend handed the flat form keeps its class's and derives its jobs under it
   (`a_backend_keeps_its_classs_prompt_ids_form_whatever_the_networks`).
3. **I-3, a tile is demanded only of a tile tree**
   (`a_prompt_tile_is_accusable_only_where_the_claims_class_commits_a_tile_tree`).
4. **I-4, the fence writes the V4 root beside its height** in `consensus_params_id`
   (`the_held_fence_writes_the_v4_contexts_beside_its_height`), and the genesis rules are
   unchanged from genesis (`the_held_context_fence_is_dormant_and_arms_only_over_v4_with_its_courts_and_tiled_ids`).
5. **I-5, the carriers**: a held class on a flat network is refused PublicDa by name and served
   under PanelDa (`a_held_class_on_a_flat_network_commits_under_panel_da_and_is_refused_public_da_by_name`);
   the class bit crosses both wires and an older node reads as its network's form
   (`the_producer_facts_survive_the_borsh_round_trip`, the gRPC round trip); the sign gate signs a
   Merkle job and binds only its own ids (`a_merkle_committed_job_is_signable_and_binds_only_its_own_ids`).
6. **I-6, the limitation, pinned** (§4's table,
   `what_the_regime_admits_on_testnet_11_is_bounded_by_its_frozen_ladder_and_prompt_cap`): the day
   a held class's ladder moves, this test says so.

## 6. Supersession

| what | by |
|---|---|
| `validate_palw_v2`: the regime requires the COMPLETE_V4 root and Merkle ids at its fence | from genesis only (Decisions 1, 3) |
| the shard court's V3 check under a late held fence | waived at or below the fence (Decision 2) |
| ADR-0081 D3 / the payload decoders' doc: "the commitment's form is the network's" | the class's, the network's for every class but a held one (Decision 3) |
| ADR-0103's suite: a held class under a flat court is refused `LinearInTheContext` on the close | admitted, priced at its own Merkle ids (Decision 3) |
| `palw_fp_sign_gate`: the flat digest alone | either form (Decision 5) |
| `GetPalwProducerFactsResponse` version 7 | version 8, `class_prompt_ids_merkle` (Decision 3) |

## 7. What is deliberately not decided

* **A held class's ladder and prompt cap on a live network** (§4). The per-class argument holds
  for both, but the ladder is read by some ninety sites — the court's refutation walkers, the held
  DA court, the stateless work-leaves cap the extraction walk applies with no class in hand, the
  producer's and the seats' caps — and the cap by the stateless ruleset check. The extraction walk
  would need either a height-wide widening past the fence (every class's work is still bounded by
  its own registration, but that is a claim to prove, not assume) or a held bit in the class state.
  It is the next ADR if the operator wants 2^21 on testnet-11 rather than on a network minted with
  the regime.
* **The other walls between a held class and 2^21**, found by the same survey (2026-09-12). Each is
  a bound a network minted with the regime meets as well, because ADR-0110's vectors run the
  stages in process at a tiny geometry and never reached them:
  - the transaction door's structural work-leaves cap, `PALW_FP_STRUCTURAL_WORK_LEAVES_CAP = 2^32`
    — with no class, no state and no height, and failing it invalidates the block — against about
    `2^38` leaves for the dense 1.5B row at 2^21 positions;
  - the worker's result frame (256 KiB), which carries the prompt ids whole: about 60,000 ids;
  - the fp worker's PublicDa-only precheck, above;
  - the held dissection opens its bisection ladder at the ladder's size, and `PalwBisectLadderV1`
    refuses a space past `2^40`, so at the held mint's `2^48` a fused held leaf is not prosecutable;
  - the fused dissection's bottom and root claim open rows under the structural
    `PALW_STEP_MAX_LEAVES` (`2^22`), not the ruleset's ladder — a bound testnet-11's own fused rows
    meet today, since the court's ladder there is `2^26` (reported to the deep-audit session, whose
    court findings ride 6,000; not decided here).
* **The dense-court fences on testnet-11.** `palw_fused_dissectable` and `palw_attn_anchored_root`
  (the audit's AC-D8) stay dormant there, so a held class with fused attention is dissected under
  the same rules testnet-11's shipped fused row is. The regime does not require them, and arming
  them is the mainnet card's correction, not this ADR's.
* **A node without the registration row.** The panel and the node's facts read the class's form
  off the chain's registration row (ADR-0067's index), with this build's tables as the panel's
  second source; a pruned-synced node adopts the row it lacks (ADR-0067 Decision 6) before it can
  serve the class at all.

## 8. Number hygiene

Written as 0118 on `feat/adr-0103-held-context` on 2026-09-12, after `origin/main` (`d080154e`)
and every local branch were listed: none holds 0118 or later. **The next free number is 0119.**

## 9. Implementation record (2026-09-12)

| Decision | where | what pins it |
|---|---|---|
| **1** the contexts | `Params::validate_palw_v2` (the held block), `Params::consensus_params_id` | I-1, I-4 |
| **2** the one-move court | `Params::validate_palw_v2` (the shard court's contexts) | I-1 |
| **3** the form | `palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1`; `palw_class_admission_v2` (held arm), `palw_model_fit_v1::palw_model_fit_v2`, `palw_context_ladder::palw_class_ladder_rules_for_court_v1`; `with_prompt_ids_form` in `backend.rs`, `qwen25_a16_backend.rs`, `qwen36_backend.rs`; `FpWorkerRuntime::new`; `PalwPanelService::{class_prompt_ids_form, payload_prompt_ids_form}` and `palw_fp_class_id_peek_v1`; `rpc/service` `get_palw_producer_facts_call`; `ChainFacts::prompt_ids_form` | I-2, I-5 |
| **4** the tile | `palw_held_da_v1::palw_held_da_check_accusation_v1`; `PalwTransitionExtrasV1::prompt_ids_merkle` for the fold | I-3 |
| **5** the carriers | `privacy_mode_for_request`, `ChainFacts::public_ids_cannot_ride`; `palw_fp_sign_gate::signable_claim_id` | I-5 |
| **6** the flag day | `PALW_RC_HELD_FENCE_DAA`, `palw_arm_held_regime_at_v1`, `palw_rc_base_params` | I-1, I-6 |
| **7** the chain arm's ladder | `PalwClassSdk::resolve_chain_registered` | `the_chain_arm_applies_the_rulesets_ladder_like_every_lineage` |

The suites, on the branch's head: the whole workspace under nextest, 4,665 tests, all passing —
with the AC-SLOT fixture generated first (`AC_SLOT_FIXTURE`, which CI does not set yet; two tests of
the base read it) and with the base's own repairs in the two commits before this one (a lost
`#[allow]`, two literals and a one-element loop clippy refuses, fmt drift in five files, and the extension's
missing arm for `palw_share_growth_final`). clippy (`--tests --benches --examples -D warnings`),
fmt, the PQ guard, doctests and the docs are green. The ADR-0103 held suite is 11 tests and this
ADR's own 5.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md -->

</details>

<a id="source-0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are"></a>

<details>
<summary>0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md — 原文 L1–L244</summary>

[原文を開く](../adr/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md) · SHA-256: `ec1c14f59b90270c105575b5c64f119c2647d53dcc4b619474911c3154a723c0`

<!-- ADR_SOURCE_BEGIN:0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md -->
~~~~~~~~markdown
# ADR-0119 — A held class is walked at the regime's ladder, and the chain records which classes those are

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED and, for its consensus half, IMPLEMENTED 2026-09-12 on
  `feat/adr-0103-held-context`, at the operator's instruction ("ADR-0119 に進める"), after ADR-0118
  §4 measured that testnet-11's frozen ladder admits a held dense class to about 512 positions.
  Rides testnet-11's held flag day, DAA 6,000 (ADR-0118 Decision 6). **Inert wherever the held
  regime is not in force:** the new state collection is empty until a held class registers, which
  nothing below the fence can do; the transaction door keeps its validity below the fence; every
  class that is not held is walked, priced and bounded exactly as before.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the regime, and Decision 1's "a ladder that costs no rounds is minted at the top of the carrier's
  budget"), [0118](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md)
  (the regime at a height; the class owns its prompt form — the move this ADR makes for the ladder),
  [0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
  (the history bound is the class's).
* Amends: ADR-0103 Decision 1's `2^48` example ladder (a held network's ladder is at most `2^40`);
  ADR-0082's "the opening cap is the structural one" for a fused site's rows (under the held
  regime it is the claim's ladder).

## 0. The sentence this ADR is

**A class under the held regime is walked, priced, bounded and prosecuted at the regime's ladder,
`2^40` leaves, on every network, and the chain records which classes those are when they register;
every other class keeps the network's ladder; and under the regime a fused site's rows are opened
at the claim's ladder, not at the structural `2^22`.**

## 1. What was found

ADR-0118 made the regime armable at a height and measured what that buys on testnet-11: the held
dense row is admitted at 512 positions and refused one doubling on, because the network's step
ladder — `2^26`, frozen in the genesis bundle — bounds the whole context as prefill. The ladder is
frozen at what a BISECTION of the whole step space can play inside the court window, since a
bisection opened before any fence must stay playable; a bundle that took a deeper one is refused
(`window_court does not fit the worst-case honest prosecution`). But a held class exists only past
the held fence, and past it no bisection opens. The clock that froze the ladder never binds a held
class.

Mapping every reader of the ladder (the survey of 2026-09-12, some ninety sites) found the ones
that decide:

* **Two consensus readers hold no class.** The free-prompt extraction walk bounds a commitment's
  `work_leaves` at the network's ladder, from the accepted transactions and the parent state; the
  class state kept no record of which classes are held, only `fused_attention`. The transaction
  door — isolation, which invalidates a block on failure — caps `work_leaves` at the structural
  `2^32` with no class, no state and no height; a dense job at `2^21` positions is `≈2^37.6`.
* **The held dissection could not open past `2^40`.** It opens its session on a bisection ladder
  whose space is the ladder, and a bisection ladder refuses a space past `PALW_BISECT_MAX_SPACE`
  (`2^40`). ADR-0103's reference mint at `2^48` had no prosecutable fused leaf.
* **A fused site's rows opened under the structural `2^22`**, not the ruleset's ladder
  (`palw_attn_dispute_site_v2`, deliberately: the fold holds no bundle). Past the court-ladder
  fence a class's claims are admitted to `2^26`; testnet-11's own graph-v5 512 row has a canonical
  job of 6,630,544 leaves, and every job of it past about 36 prompt tokens is above `2^22`
  (measured: 32 tokens and 4 decode 3,681,344 leaves; 40 and 4, 4,505,408). Its fused leaves could
  not be dissected at all — on testnet-11 today, not only under the regime. (Reported to the
  deep-audit session, whose C-01 binds the canonical count at 6,000 and so exposes exactly this.)
* **testnet-11's 512-token prompt cap is not enforced at runtime** (`palw_fp_ruleset_caps` is dormant
  there), so it was never the binding wall there; the ladder was.
* **On the node side**, the free-prompt worker refused every PanelDa job ("a mode the panel cannot
  replay must not execute", written four days before private prompts landed) — so a gateway started
  `--privacy panel-da` could file nothing, and a held class on a network minted flat, which commits
  under PanelDa only (ADR-0118 Decision 5), had no executor path.

## 2. The requirement

On testnet-11 past its held fence, a held class is admitted, produced, seated and prosecuted at
the contexts ADR-0116 widened the history for, and no other class's bounds move; below the fence
every node — upgraded or not — accepts exactly the transactions and blocks it accepted before;
and no two nodes can bound one claim at two ladders. This ADR meets the chain's half — what is
admitted, bounded and tried — and the node's half up to the network's ladder; the node's held route
past it is §7's.

## 3. Decisions

**Decision 1 — the held ladder is `2^40`.** `PALW_HELD_STEP_LADDER_V1 = PALW_BISECT_MAX_SPACE`: the
deepest ladder whose dissection session opens, 40 levels and 2,560 bytes of path in a close. The
dense 1.5B row at `2^21` positions is `≈2^37.6` leaves and Qwen3.6-35B's density `≈2^39.3`; a class
past `2^40` is refused by the gate's own ladder wall. `palw_class_step_ladder_v1(network, profile)`
is at least that for a held profile and the network's for every other. A held network's court ladder
is at most `2^40` (`validate_palw_v2`, and `palw_held_context_mint_v1` refuses a deeper one), so for
any real network a held class's ladder is exactly the regime's.

**Decision 2 — the chain records which classes are held.** `PalwChainStateV2::class_step_ladders`,
a guarded collection (carriage tail `0xA5`, delta variant 41, rooted only when non-empty): written
when a class registers with a held profile — the fold holds the profile then and never again — with
the regime's ladder. A class id is a hash of its profile, so a row is a function of the id and is
never rewritten; it reverts with the registration and travels with a pruned sync's carriage. Every
consensus reader of a claim's ladder asks `class_step_ladder_v1(class_id, network_ladder)`.

**Decision 3 — admission prices and counts a held class at its ladder.** The gate's ladder wall and
recount, the ladder rules' cost shape (the one door every gate, fit and deadline reads its rules
through), the fit, and the registration builder's canonical count all read the class's ladder.

**Decision 4 — the courts read the claim's ladder.** The one-move court (acceptance now finds the
claim before bounding the accusation's shape), the checkpoint court, the held DA court's answer,
and the held dissection's session space all read `class_step_ladder_v1` of the claim's class, in the
acceptance layer and in the fold alike. A fused site's rows open under `palw_attn_opening_cap_v1`:
the structural `2^22` before the held regime, as always; under it, the claim's ladder — a held
class's `2^40`, every other class's the network's step ladder. The acceptance layer asks it at the
block's DAA (`adjudicate_court_close_v3`), the fold through its extras. On testnet-11 this makes the
shipped dense row's fused leaves dissectable from 6,000 — a consensus change for that row's claims
past the fence, and the one the deep-audit session's C-01 needs beside it. On the node, a party
derives the site at its backend's ladder (`base0_attn_site_evidence_v1`; it derived at `2^22` and
refused its own first opening past it), and the panel files a root claim — the one move filed
without asking the chain first — only when the court will open it
(`attn_root_claim_is_openable_v1`: always under the regime, to `2^22` before it). The court's cap is
an upper bound (`step_opening_root_capped_v1` bounds the leaf count and the path), so evidence built
for one tree verifies under any cap that holds it. And an executor's range opening is bounded by its
tree's own depth, not the default leg's 44 siblings, which refused an honest opening of a claim past
`2^22` leaves that the chain walks under the ladder that admitted it.

**Decision 5 — the extraction walk bounds a commitment by its class.**
`palw_fp_objects_from_accepted_txs_by_class_v1` reads the parent state by the commitment's own class
id: a held class's commitment is bounded by its recorded ladder (and, where the ruleset caps are
armed, by the regime's prompt cap `PALW_FP_HELD_MAX_PROMPT_TOKENS_V1`), every other class's exactly
as before. A commitment in the block its class registers in meets the network's bounds — its class
is not yet in the parent state — and a producer resubmits it one block later.

**Decision 6 — the transaction door is the ADR-0087 D6 pair.** Isolation holds no height, so it asks
the height-free question: its work-leaves cap is the regime's `2^40` on a ruleset that declares the
regime at all, the structural `2^32` otherwise (`palw_fp_isolation_work_leaves_cap_v1`). The
header-context door (`check_palw_fp_work_leaves_in_context`, blocks at their own DAA and the mempool
at the virtual's) refuses a commitment past `2^32` below the fence, by name
(`PalwFpWorkLeavesBeforeHeldActivation`). So a build that schedules the regime and one that does not
carry it agree on every transaction before the activation — the refusal a stale node makes at the
door, an upgraded node makes one door later — and nothing is committed on the permissive answer.

**Decision 7 — the private free-prompt path runs.** The worker's precheck admits both modes the chain
carries, PublicDa and PanelDa. The rest of the path already handled PanelDa: the commitment builder
carries no ids under it, and the drawn seats read them from the staged material the submitter took
from the worker's result.

## 4. What it costs

* **A consensus change at 6,000 for the shipped dense row:** its fused leaves' rows open at `2^26`
  instead of `2^22`. Before the fence, byte-identical.
* **A state collection** that roots and encodes nothing until the first held registration.
* **A header-context check** on every free-prompt commitment on a ruleset that declares the regime:
  one decode of a payload isolation already decoded.
* **What it does not buy: a node that produces or tries a held job past the network's ladder.**
  Every backend is still built at the network's ladder, so a held class's producer refuses a job
  past `2^26` leaves on testnet-11 (about 650 positions of the 1.5B dense row) and a seat signs a
  claim past it `Unavailable`. The chain admits to `2^40`; the gap between the two is claims no
  honest producer makes, and until the node side lands (§7) no honest seat can verify such a claim
  (each signs `Unavailable`, and a quorum of those voids it as `ProducerDefaulted`) and no honest
  challenger can prosecute one — only a quorum of dishonest drawn seats could certify it.

## 5. Invariants the tests hold

1. **I-1, the ladder is the class's**: a held class records `2^40` at registration and a floor class
   nothing; the row is rooted, carried, imported and reverted
   (`a_held_class_records_its_ladder_at_registration_and_nothing_else_does`).
2. **I-2, admission**: on testnet-11 past 6,000 the held dense row is admitted at 512, 1,024, 32,768
   and `2^21`, not before the fence, and a graph-v5 row at 1,024 keeps the network's `2^26`
   (`a_held_class_on_testnet_11_is_admitted_to_2_21_at_its_own_ladder`); a held mint or network past
   `2^40` is refused; ADR-0103's 2M suite runs at `2^40`.
3. **I-3, the courts**: every held court arm of the fold and of the acceptance layer reads the
   claim's class ladder, and every close is judged knowing the regime
   (`every_held_court_arm_of_the_fold_reads_the_claims_class_ladder`,
   `every_held_court_acceptance_arm_reads_the_claims_class_ladder`); the opening cap is `2^22` before
   the regime and the claim's ladder under it (I-1's test); a party derives the site at its backend's
   ladder and files a root claim exactly when the court opens it
   (`a_party_derives_the_fused_site_at_its_backends_ladder`,
   `a_root_claim_is_filed_exactly_when_the_court_opens_its_rows`); a `2^26`-leaf tree serves a
   49-sibling opening the chain accepts (`a_tree_deeper_than_the_default_leg_serves_its_honest_openings`).
4. **I-4, the walk**: a held class's commitment past the network's ladder is extracted and every other
   class's is skipped as before (`a_held_class_commitment_is_bounded_by_its_own_ladder_and_every_other_by_the_networks`).
5. **I-5, the door**: isolation admits `2^40` only where the regime is declared
   (`the_isolation_door_admits_the_regimes_ladder_only_where_the_regime_is_declared`); a scheduled
   regime refuses a held job's leaves below its fence and admits them from it, and a commitment inside
   `2^32` is untouched (`a_scheduled_held_regime_admits_a_held_jobs_leaves_only_from_its_own_fence`).
6. **I-6, the private path**: a PanelDa job runs and returns its ids for the staged material
   (`a_panel_da_job_runs_and_returns_its_ids_for_the_staged_material`).

## 6. Supersession

| what | by |
|---|---|
| ADR-0103 Decision 1: "a `2^48` ladder is 3,072 bytes of path" | Decision 1: at most `2^40`, 2,560 bytes |
| ADR-0118 §4's pinned limitation (a held class to about 512 on testnet-11) | Decisions 2–6, at the chain; §7 for the node |
| `palw_attn_dispute_site_v2`'s structural opening cap | Decision 4 under the regime; unchanged before it |
| `fp_worker::precheck_request_v1`: PublicDa only | Decision 7 |

## 7. What is deliberately not decided yet

* **The node's held route past the network's ladder** — mapped on 2026-09-12 (every read of a
  backend's ladder, every job-sized allocation, the producer's and the seat's retention), and a design
  of its own (ADR-0121), because what it found is not a number to raise:
  * **The backend's ladder is two things.** It prices and refuses a job, and it is the only guard in
    front of the whole-capture paths that allocate a vector of `step_leaf_count` hashes from a decoded
    or gossiped count (`leaves_by_position`, the dense re-executions, `verify_material`'s dense arm).
    At `2^40` a hostile blob could ask for `2^46` bytes. The node needs the class's ladder for pricing,
    refusal and Merkle walks, and a separate materialization cap — the network's — at every job-sized
    site, with held claims kept off the whole-capture arms.
  * **The held route does not stream yet.** The producer's fold does (it holds one open block and
    the retained nodes), but every interval replay — the executor's openings, the seat's V4 verify,
    the held DA court's answers, name-the-leaf — allocates a leaf vector the size of the whole job and
    keeps every tile of the replayed window: tens of gigabytes at 4,096 positions of the 1.5B dense
    row, by the code's own sizes. The replays must fold digests on the fly and serve openings from the
    retained digests and the two boundary blocks, which the capture's own documentation already says
    suffice.
  * **The retain level must be pinned, not derived.** Producer and seat each derive it from their
    ladder (`r = max(⌈log2 cap⌉ − 20, 12)`: 12 at `2^26`, 20 at `2^40`); a seat that derives 12 against
    a producer at 20 addresses the wrong block and names no leaf, and an `r = 20` block is 64 MiB, past
    the 4 MiB interval lane. The held route keeps `r = 12`, the seat reads it off the opening, and the
    block-leaves request indexes within the interval (a global 16-bit block index reaches leaf `2^28`).
  * **Two walls are not the ladder's.** A resume opening carries the whole state over the same 4 MiB
    lane, and a class of `n_ctx` `2^21` always takes the Resume route; and a fused site's dissection
    evidence is built from dense rows and the whole K/V history. Each needs its own transport or a
    windowed builder. (A `2^21`-position job of the 1.5B row also needs a K/V cache of about 120 GB —
    a host's question, not the protocol's.)
  * **The worker frame and the gateway's prompt limit** (256 KiB, about 60,000 ids; 64 KiB of text)
    bind only past the network's ladder, so they move with the rest of this.
* **The same bottom cap on a network without the regime.** The mainnet card arms the court ladder
  from genesis and not the regime, so its fused rows' bottoms stay at `2^22`; that is the card's
  decision, and the deep audit's.

## 8. Number hygiene

Written as 0119 on `feat/adr-0103-held-context` on 2026-09-12; `origin/main` and every local branch
hold nothing past 0118. The state tail `0xA5` and delta variant 41 were agreed with the deep-audit
session, whose fixes add none. 0120 was claimed the same day by the model-seed session (the seed lock at
`palw_model_seed_v2`), so this ADR's node half — §7 — is **0121**, and **the next free number is 0122.**

## Mission alignment amendment — 2026-10-07

held classとして記録されたこと、regime ladderをkernelが裁定できること、外部bondが公開materialから訴追できることを三つの別条件として確認する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md -->

</details>

