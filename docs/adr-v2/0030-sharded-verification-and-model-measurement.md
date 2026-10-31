# ADR-v2-0030 — Shard検証・登録時計測・2M実装計画

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0003](../rfc-v2/0003-model-onboarding-and-artifacts.md) / [RFC-v2-0006](../rfc-v2/0006-panels-and-probabilistic-verification.md)

## 1. この文書の役割

登録時計測とshard検証の役割を整理し、長文脈の物理的限界を隠さない。

## 2. 統合した判断の読み方

### 2.1

manifestから導出できるprofile・artifact規模・必要shard数と、host依存の再生時間等の自己申告を分ける。[旧ADR-0099](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)

### 2.2

Panelのseatがshardを持つ検証設計を、classの同一性・裁定境界・登録admissionと対応させる。[旧ADR-0099](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)

### 2.3

2M計画はRAM・replay時間・期限が制約になるため、担保やρだけでは検証供給が増えないという前提を維持する。[旧ADR-0153](../adr/0153-2m-verification-sharding-plan.md)

## 3. 維持する境界・後続改定

ADR-0153はDraft planで、2M一般対応やmainnet有効化の承認ではない。過去のfixture／測定hostを全モデルに一般化しない。

## 4. 隣接する判断との分担

model登録は[ADR-v2-0015](0015-permissionless-model-registration.md)、contextの上限は[ADR-v2-0018](0018-long-context-measurement-and-admission.md)、capacity倍率は[ADR-v2-0036](0036-capacity-scaling-and-emission-budget.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) | L1–L371 | L15–L42 |
| [0153-2m-verification-sharding-plan.md](../adr/0153-2m-verification-sharding-plan.md) | L1–L78 | L5–L13 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard"></a>

<details>
<summary>0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md — 原文 L1–L371</summary>

[原文を開く](../adr/0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) · SHA-256: `721025729355d16b3ca85934245abed441c2a8bcb19bd5af1989f9739666deaa`

<!-- ADR_SOURCE_BEGIN:0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md -->
~~~~~~~~markdown
# ADR-0099 — The adder measures, the chain recomputes, and a seat holds a shard

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-10. **Decisions 1–4 and 6 IMPLEMENTED the same day, consensus-inert**
  (§9): types, pure functions, a generator and tests; no object, no acceptance rule, no fingerprint
  moves on any shipped preset. **Decision 5's fence is declared and refused at assembly** on this
  build, in the shape ADR-0096 Decision 8 used. Decisions 7–8 are stated, not built. Every
  MEASUREMENT this ADR needs is the ADDER's, taken by the tool this ADR ships, and is recorded as a
  unit the adder runs (§6), never as a number this document knows.
* Builds on: [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  (the fit as a lookup; Decision 5's first row, which this ADR is; §1.3's stand-in, which this
  ADR turns into a manifest), [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (the coverage of a one-leaf lie is independent of the shard count; licensing fits one
  transaction up to eight shards; a seat that found a lie files nothing; the court a shard can
  open, named there and built here as far as a type), [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md)
  Decision 8 (a seat opens a court at the leaf, holding the refutation's inputs),
  [0082](0082-the-close-is-flat-in-the-context.md) Decisions 4 and 9 (the cache half of the
  ladder is court material, tile-addressed; the seat recomputes its state from the prompt it
  holds, and the drill enforces its window),
  [0075](0075-certification-is-a-consensus-object.md) (a class is seated by a certification the
  chain's own drill produces), [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) SA-1
  (a bond declares what it can judge), [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md)
  Decision 4 (a wider row is a new mint), [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
  (the fused site's terminal), [0096](0096-the-app-you-already-use-is-the-entrance-and-the-shape-of-the-answer-is-committed.md)
  Decisions 10 and 13 (the components manifest; a model request has a door),
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) / [0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  (a line, an owner, a seeded pair — what a registered model becomes).
* Amends: ADR-0097 Decision 4 keeps its rule — a stand-in is a verdict, never a row — and gains
  a form: a stand-in is now also a MANIFEST (Decision 1), read by the same generator that will
  read a converted model's, and the day a model is converted its manifest is what names the
  class. Supersedes nothing.

## 0. The sentence this ADR is

**Anyone adds a model: they hand the network a manifest, the network's own tool measures what
can be computed from it — the artifact's bytes, every wall of ADR-0097 at every context, the
state a seat holds, the shards a seat budget allows — writes it into a Measured Model Artifact
the adder signs and any node recomputes, refusing by the field that disagrees; what only a run can tell — the
replay rate — the adder reports and the chain's own drill verifies; and the model is carried by
seats that each hold a SHARD of it.** The K3 stand-in of ADR-0097 becomes a manifest here, and
the manifest says: at the card's 2.8 T parameters, 23 seats of 128 GiB hold it, each replaying
four of its 92 layers from a 3.5 GiB boundary opening or its own 0.8 GiB of state — and the
RULESET still refuses the row at every context the table prints, as it did in ADR-0097 past ten
positions, because sharding changes what a seat must hold and not what a ladder admits. That last sentence is why this ADR ends where ADR-0097
Decision 5 began: a K3-class network is a different mint, and this ADR gives it the tool that
prices the mint.

The operator's reframing (2026-09-10), adopted whole: *「モデル追加者が計測 → MISAKAが検証」* — the
measuring party is the adder, not this project; the tool measures; the chain verifies what it can
recompute and drills what it cannot; and the result is a basis for ANY architecture the tree can
build a graph for, never a K3-specific path.

## 1. What was measured

Read on `feat/adr-0098-seat-coverage` at `44758a0b` on 2026-09-10; every figure is what
`misaka-palw-base0 --bin palw-shard-plan` printed on that tree (recorded in
`docs/palw-shard-plan-2026-09-10.md`). The binary is the authority (ADR-0092 §5).

### 1.1 What the tree already gives a shard

* **A shard's input is another shard's committed output.** Every node's output row at every
  position is a step leaf (`palw_step`: `pre ‖ layer 0 ‖ … ‖ post`, one row per node per
  position). The layer input of a shard's first layer is the previous shard's last node's row,
  opened against the claim's step root like any other. Nothing new is committed anywhere.
* **A shard's leaves are one run per position.** The enumeration is position-major then by global
  node slot (`canonical_step_coordinates`), so a layer range is a slot range and a slot range is
  one contiguous run of leaves at every step. Measured (`palw_shard_leaf_run_v1`, Invariant 4):
  on the dense row cut four ways, the four runs tile every step of a job exactly — the first of
  each starting where the last ended, the sum ending at the next step's first leaf.
* **The court's terminal step is already shard-local.** A disputed leaf's refutation opens its own
  committed inputs and one node's weight rows (ADR-0092 §2); nothing about the whole model
  enters. What is NOT shard-local is the SEARCH: today's challenger opens over the whole step space
  and answers every rung from its own whole execution (ADR-0098 §1.3).
* **A bond already declares what it can judge** (`capable_classes`, ADR-0071 SA-1). A shard can be
  a capability without a new field (Decision 3).

### 1.2 The plan, and what a shard costs a seat

A plan for `k` shards is the contiguous partition of the layers that minimises the widest seat —
each layer weighed by its artifact bytes (the family formula, one byte a weight) plus the state
its kind holds at the class's context (an attention layer's i32 cache, a delta-rule layer's
constant state), with the embedding pinned to shard 0 and the logits to the last. Derived, never
chosen: two nodes given the class and `k` produce the same shards.

The formula lands beside the artifacts this tree ships — the dense A16 row at 1,776,943,104
bytes against the real artifact's 1,795,427,276 (`docs/model-requests.md`; no norms, no biases,
one byte a weight is a floor); the hybrid at 32.3 GiB against the 33 GiB the fleet maps — and
does NOT reconcile the K3 card: 896 experts of 3,072 on every one of 92 layers is 5,108 GiB, against
the card's 2,608. The card's layer split is not public; both brackets are tabled and the
reconciliation is the adder's first unit (U-01).

The Kimi K3 stand-in at the card's total, 131,072 positions:

| shards | widest seat (artifact + cache + state) | its layers | per seat per job: recompute / resume | licensing receipts / bytes / one tx? |
|---|---|---|---|---|
| 1 | 2,625.2 GiB (2,607.7 + 17.2 + 0.2) | 92 | 0 / 17.5 GiB | 3 / 14,385 / yes |
| 4 | 656.8 GiB | 23 | 3.5 GiB / 4.6 GiB | 12 / 57,333 / yes |
| 8 | 342.3 GiB | 12 | 3.5 GiB / 2.3 GiB | 24 / 114,597 / yes |
| 16 | 171.5 GiB | 6 | 3.5 GiB / 1.5 GiB | 48 / 229,125 / **no** |
| 32 | 85.8 GiB | 3 | 3.5 GiB / 0.8 GiB | 96 / 458,181 / **no** |
| 64 | 57.4 GiB | 2 | 3.5 GiB / 0.8 GiB | 192 / 916,293 / **no** |

The fewest shards a seat can hold one of: **23** for a 128 GiB seat (widest 114.6 GiB, four
layers each), 46 for 64 GiB, 6 for 512 GiB, and none for 24 GiB — at the card's total a layer is
28.3 GiB and the narrowest seat any plan of up to 92 shards makes is 29.6 GiB. Under the formula's 5,108 GiB the same seats need 46 / 92 / 11 shards. The dense row needs
one seat of 24 GiB at every context up to the ceiling; the hybrid needs one of 64 GiB, or two of 24.

**Two transfer forms, and neither is free.** A shard seat resumes an interval either by
*recompute* — the previous shard's committed rows for every earlier position, `positions × hidden
× 4` (3.5 GiB at 131,072 positions of a 7,168-wide residual; 28 GiB at 1M) — or by *resume* — the
checkpoint chunks of its own layers at the interval's start, at most its state at the class's
context (0.8 GiB for a 3-layer K3 shard). Both are committed material, so both are verifiable;
which is cheaper is the shard's shape — many attention layers and a narrow residual favour
recompute, few and a wide one favour resume — and the plan prints both. Shard 0 recomputes from
the prompt ids it holds and needs neither. Across a whole panel the recompute form is `(shards −
1) × positions × hidden × 4` per replaying seat: 77 GiB per job at 23 shards and 131,072 positions.
That is the cost a stratified panel pays for verification, and it is the number Decision 7's
measurement is about.

**The fit does not move.** At every context the stand-in's class is refused by the ladder (and
from 32,768 up by the court window, the state chunks and the PublicDa payload), exactly as
ADR-0097 §1.3 found, whatever the shard count. A plan is a statement about SEATS; the ruleset's walls are statements
about the CLASS, and only a mint moves them (ADR-0092 Decision 4).

### 1.3 The Measured Model Artifact, exercised

`palw-shard-plan --manifest docs/model-manifests/kimi-k3-stand-in.json --replay-ms 572 --measured-out …`
wrote a document whose deterministic half — the artifact's bytes, and at each of 512 / 32,768 /
131,072 / 1,048,576 positions the class id, the fit verdict with its refusing walls, the cache and
state bytes, and the fewest shards per seat budget — verified against the RC by recomputation,
field by field; a copy with one byte added to the artifact and the 512 row's verdict flipped was
refused naming `artifact_bytes`, `rows[512].fit_admitted` and `rows[512].refusing_walls`. Its
self-reported half — 572 ms a position, an example host — was carried, listed as self-reported,
and its derived figure (125,400 positions inside the RC's receipt window) recomputed from the
report. The manifests of the two shipped classes name the shipped classes' own ids (Invariant 6).

### 1.4 What the shard court needs, and what it can try

The accusation a shard seat would file is what the seat already builds and throws away:
`fp_capture_samples_clear` constructs, per sampled leaf, exactly a refutation (`refutation_for_free_prompt_index`,
`operand_openings_for`) and runs the court's terminal check on it. Bound to a named leaf and a
named shard it is a one-move court (`palw_shard_court_v1`): the chain runs the same check; a fault
convicts the executor; a refutation that proves none convicts the accuser; and a fused attention
leaf is answered `NeedsDissection`, because its terminal is ADR-0082's k-ary dissection and needs
ADR-0093's responder. Nothing is asked of the accused, so there is no responder and no clock for
every node but the fused site.

## 2. The requirement

> **R-add — anyone adds a model: a manifest is enough to name the class; every quantity a node
> can recompute from the manifest and the ruleset is recomputed and refused by the field that
> disagrees; every quantity only a run can tell is the adder's report, labelled, and verified by
> the chain's own drill; and a class too large for one seat is carried by seats that each hold a
> shard, verifying with nothing but their shard and committed material.**

## 3. Decisions

**Decision 1 — the manifest is the model definition, and the Measured Model Artifact is what the
network makes of it.** `PalwModelManifestV1` is the family (`dense-a16` or `hybrid-qwen36` — the
two graphs this tree builds; another architecture needs a converter and kernels first, ADR-0075's
route, and is refused at the manifest) and the geometry's public numbers, with an optional
`total_parameters` a card states. `manifest.profile(n_ctx)` is the family's graph-v5 row over the
artifact's epsilon — the same projection the registered rows are built through — so a manifest of
a shipped class names the shipped class id (Invariant 6). `palw_measure_model_v1` turns a manifest
into `PalwMeasuredModelV1`: the ruleset it was evaluated on by name and fingerprint; the
deterministic half (`artifact_bytes` and its basis; per context the class id or `None` past the
ceiling, the ADR-0097 verdict with its walls, the cache, state and boundary-row bytes, and the
fewest shards per seat budget); the self-reported half (the replay rate in ms a position and the
host it was measured on, with the figure ADR-0082 Decision 9 would derive from it and the sentence
that says what verifies it); and room for the adder's ML-DSA-87 signature over the id
(`palw_measured_model_id_v1`, borsh over everything but the signature, under
`misaka-palw/measured-model/id/v1`). The signing context `misaka-palw/measured-model/mldsa87/v1` is
not in the bundle's registry and never will be — no consensus rule verifies it; a registration is
verified by recomputation — so the generator leaves the signature empty and §6's `palw-class
measure` is what fills it with the bond key, as a statement to whoever reads the document.
`palw_verify_measured_model_v1` measures the manifest again on the same ruleset and compares every
deterministic field by name; a document is refused by the first field that disagrees, and the
self-reported fields are listed as such. The stand-in of ADR-0097 §1.3 is now a manifest
(`docs/model-manifests/kimi-k3-stand-in.json`), and the day a K3 is converted, its manifest — read
off the artifact — replaces it.

**Decision 2 — a shard is a contiguous layer range, and the plan is derived.**
`palw_shard_plan_v1(profile, artifact, k)` is the contiguous partition minimising the widest
seat, with the embedding on shard 0 and the logits on the last; `palw_shard_plan_for_seat_v1` is
the fewest shards whose widest seat fits a budget. A shard's leaves at a step are one run
(`palw_shard_leaf_run_v1`), so a shard's interval opening is today's interval opening cut to the
run. A shard seat resumes an interval by recompute (the previous shard's committed rows) or by
resume (its own layers' checkpoint chunks), both committed material; the plan prices both and
chooses neither.

**Decision 3 — a shard is a capability, and the panel is stratified.** *(Corrected by ADR-0100
Decision 4: `capable_classes` holds registered class ids only — the fold refuses any other entry
— so a bond declares its shards with `BondShardsDeclared`, a refinement of a class it declared
there. The stratified draw below stands.)* A bond that holds shard `i`
of a `k`-shard plan of class `c` declares `palw_shard_capability_id_v1(c, k, i)` in its
`capable_classes` — the field it already carries, under the rule ADR-0071 SA-1 already applies. A
claim on a sharded class draws `seats_per_shard` seats per shard from the bonds that declared that
shard (`derive_shard_panel_v1`): the panel's own ticket with the shard mixed in, one seat per
operator per shard, a shard short of operators refusing the whole draw by name. A one-leaf lie is
caught with ADR-0098's number at every shard count (only its shard's seats can replay it), and the
licensing object carries `quorum × k` receipts.

**Decision 4 — licensing past eight shards is a design this ADR names and does not build.** At a
quorum of three the licensing object fits one standard transaction up to eight shards (ADR-0098
§1.2). Past it, either the object is split per shard and the claim licenses when every shard's
object has landed — a state-machine change — or the receipts are aggregated, which ML-DSA-87 does
not offer. Either is measured first; a network that shards a class past eight decides it at mint.

**Decision 5 — the court a shard can open, behind `Params::palw_shard_court`, refused at
assembly.** `PalwShardCourtAccusationV1` names a claim, its roots, the accuser's shard and the
leaf, and carries the refutation and the artifact openings; `palw_shard_court_verdict_v1` checks
the shape (the leaf inside the shard's slots, under the ladder; the accuser not the accused; the
refutation about the named leaf), proves the openings against the class root, and runs the court's
own `check_execution_step_refutation_capped_v1`: `ExecutorGuilty`, `FalseAccusation`, or
`NeedsDissection` for a fused site. The fence is `None` on every preset, Some-only in both
fingerprints, and `validate_palw_v2` refuses a scheduled height on this build because the
accusation is not a consensus object: no acceptance rule takes it, no fold slashes on its
verdict, its signing context is not in the bundle's registry, and no seat files one. The refusal
lifts when a build carries all four.

**Decision 6 — admission recomputes, and never believes a rate.** A registration of a manifest's
class is judged by `verify_class_admission_v5` on the recomputed profile, priced by ADR-0097's
walls, and seated by the certification drill (ADR-0075 Decision 7) — which measures the seat's replay on
the NETWORK's hosts. The Measured Model Artifact's deterministic half must agree with the
recomputation or the registration is refused by the field; its self-reported rate is read by the
plan (to size a seat) and by nobody else. An adder who overstated a rate registers a class no
seat certifies; an adder who understated one wastes seats. Neither can move a wall.

**Decision 7 — measured first, by the adder, with the tool.** The tool (`palw-shard-plan`, and
the SDK's `palw-class` / `palw-certify` around it) is the registration path's measurement step:

| step | tool | what it yields | verified by |
|---|---|---|---|
| read the manifest | `palw-shard-plan --manifest` | the family and the geometry | refused by name if the family has no graph |
| convert the weights | the family converter (`qwen25-convert`, `qwen36-convert`) | the artifact, its root, its byte count | the root is what registration carries; the byte count is the file's |
| bind the tokenizer | `palw-class bind-tokenizer` (ADR-0096 D10) | the tokenizer commitment | the worker refuses an unbound artifact |
| measure the state and the shards | this ADR's generator | the deterministic half | recomputed by every node |
| measure the replay rate | `palw-certify` on the adder's host | the self-reported half | the certification drill, on the network's hosts |
| the fit | ADR-0097's generator, inside the document | every wall, per context | recomputed |
| register | `--palw-register-class` (ADR-0075) | a `ClassRegistered` | `verify_class_admission_v5` |
| certify, line, pair | the drill; ADR-0088; ADR-0090 | `ClassLaneCertified`, a line, a market | consensus |
| distribute | the components manifest (ADR-0096 D10) | rows for the artifact and the shards | the manifest's sha256 |

Units the adder runs, per model, before any of Decisions 3–5 is armed for it (§6): U-01 the
reconciliation of the formula with the artifact's real bytes; U-02 the replay rate under
`palw-certify`; U-03 the transfer of one boundary opening and one resume opening between two
hosts, against `window_receipt`; U-04 a devnet drill of a sharded class with the stratified panel.
None is this project's to take for a model it does not hold.

**Decision 8 — the ruleset is still the ruleset.** A plan admits no class. The K3 manifest is
refused at every context by the ladder on both shipped presets; a K3-class network mints with
ADR-0097's table and ADR-0092's generator, and then shards under this ADR. Stating it here stops
the next reader from concluding that 23 seats of 128 GiB make a K3 runnable on testnet-11.

## 4. What this costs

* **Chain:** nothing on any shipped preset. One fence field, `None` everywhere, byte-identical
  fingerprints; two signing contexts named and not registered.
* **Node:** four consensus-core modules of pure functions and types; one generator; tests.
* **An adder:** one manifest, one generator run, the family converter and certify they already
  needed. **A shard seat:** its shard's artifact and state, and per job the smaller of two
  openings — both in the plan.

## 5. Invariants the tests hold

```
1  The artifact formula is within the measured artifacts of both shipped families and above the
   K3 card's total (the reconciliation is U-01, and the scaled bracket meets the card).
2  A plan partitions the layers contiguously, holds the embedding and the logits exactly once,
   splits every artifact byte, and its shards' slots tile the profile's slots.
3  The widest seat never grows with the shard count, and is within one layer of the perfect
   split.
4  A shard's leaves are one run per step, and the runs of all shards tile every step exactly
   (against the enumeration's own inverse).
5  The seat-budget search finds the fewest shards, and the stand-in at the card's total needs
   exactly 23 for a 128 GiB seat at 131,072 positions (pinned as the limitation it is).
6  A manifest of each shipped class names the shipped class id, and survives JSON.
7  An accusation's session id binds every field but the signature, and its shape rules refuse
   by name.
8  The shard court fence is dormant on every preset, visible the moment it is not, distinct
   from the constraint fence, and refused at assembly by name.
9  A Measured Model Artifact verifies against its own ruleset; every deterministic field is
   refused by name when tampered; the self-reported half is carried, listed, and its derived
   figure must match the report it came from.
10 The stratified draw seats one operator per shard, is a function of the shard, and refuses a
   short shard by name; a shard capability names the class, the plan and the shard.
```

1–9 are `consensus/core/tests/palw_adr0099_shard_plan.rs` and the fence's tests in
`config::params`; 10 is `palw_shard_panel_v1`'s; the partition and scaling helpers have their own.

## 6. Order of work

1. Decisions 1, 2, 3, 6 — **done** (§9).
2. Decision 5's fence and types — **done**; its object, acceptance rule, fold and signing-context
   registration — a build that carries them lifts the refusal.
3. The SDK's `palw-class` gains `measure` (this generator over a converted artifact, reading the
   geometry off the file rather than off a manifest) and signs the document with the bond key —
   the `misaka-model add` the operator described, assembled from the steps of Decision 7.
4. Decision 4 — a design with a measurement, its own ADR.
5. Decision 7's units — the adder's, per model.
6. The successor mint for a K3-class network — ADR-0097 Decision 5, priced by this ADR's
   generator.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0097 §1.3 (a stand-in's substitutions are named on the constant) | kept; the stand-in is now also a manifest, and a converted model's manifest replaces it |
| ADR-0097 Decision 5, first row (a seat that holds a shard, measured first) | the plan, the panel, the court's type and the measurement tool; the measurement itself is the adder's |
| ADR-0098 Decision 5 (a court opened at a named leaf; licensing past eight shards) | the court as a type and a verdict function behind a fence; licensing named again as Decision 4; ADR-0098 §1.2's "326 GiB a seat at eight shards" was an even split of the card's total and is 340 GiB under the plan, because a shard is whole layers |
| ADR-0077 Decision 8 ("as any bonded challenger may, holding the refutation's inputs") | the shard court is that sentence with the search removed |
| ADR-0082 Decision 9 (the seat recomputes its state; the drill enforces the window) | kept: recompute is one of the two forms; the drill verifies the rate |
| ADR-0071 SA-1 (`capable_classes`) | a shard is a capability under the same rule — corrected by ADR-0100 Decision 4: the shard list is its own object, refining a declared class |
| ADR-0075 (certification is a consensus object) | what verifies every self-reported number |
| ADR-0096 Decision 13 (a model request has a door) | the door now leads to a manifest and a generator run |

## 8. What is deliberately not decided

* **Any number of the successor mint** (ADR-0092 Decision 4; ADR-0097 Decision 5).
* **Which transfer form a shard seat uses** — priced, not chosen; U-03 measures.
* **Licensing past eight shards** (Decision 4).
* **The shard court's economics** — what an accuser stakes and what a false accusation costs is
  the existing rule (`da3_…`) until the object exists.
* **A third family.** A manifest of an architecture this tree has no graph for is refused; the
  graph is ADR-0075's route.

## 9. Number hygiene and implementation record

0099 is the next free number after ADR-0098 (whose README row says so). Claimed on
`feat/adr-0099-sharded-seat`, branched from `feat/adr-0098-seat-coverage` at `44758a0b`. The
operator's own draft named this "ADR-0098"; that number was taken the same morning by the
coverage measurement, which this ADR builds on. A concurrent claimant renumbers the later writer.
**The next free number is 0100.**

* **2026-09-10** — ADR written and implemented the same day:
  * `consensus/core/src/palw_shard_plan_v1.rs` — Decision 2: the artifact formula per family,
    the plan, the seat-budget search, the leaf runs, the two transfer forms.
  * `consensus/core/src/palw_shard_panel_v1.rs` — Decision 3: the shard capability id and the
    stratified draw.
  * `consensus/core/src/palw_shard_court_v1.rs` — Decision 5: the accusation, its session id, its
    shape rules, the one-move verdict over the court's own adjudicator.
  * `consensus/core/src/palw_measured_model_v1.rs` — Decision 1: the manifest, the document, its
    id, measure and verify.
  * `consensus/core/src/config/params.rs`, `fork_id_v1.rs` — Decision 5's fence, dormant and
    refused, with its tests.
  * `misaka/palw/base0/src/bin/palw-shard-plan.rs` — the generator, with `--manifest`,
    `--measured-out`, `--verify`, `--replay-ms`; `docs/palw-shard-plan-2026-09-10.md` its output
    as a dated record; `docs/model-manifests/kimi-k3-stand-in.json` the manifest example.
  * `consensus/core/tests/palw_adr0099_shard_plan.rs` — Invariants 1–9.

## Mission alignment amendment — 2026-10-07

shardを持つ担当seatの最適化は維持するが、同じ公開認証境界から非seatも局所化・裁定できなければならない。adderの測定やchainの再計算は、証拠の公開取得を免除しない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md -->

</details>

<a id="source-0153-2m-verification-sharding-plan"></a>

<details>
<summary>0153-2m-verification-sharding-plan.md — 原文 L1–L78</summary>

[原文を開く](../adr/0153-2m-verification-sharding-plan.md) · SHA-256: `ed68950c99c9d9a7d9c2d1913ea9a0f0c6fc2c717f57f2711ef33a0d496e36e7`

<!-- ADR_SOURCE_BEGIN:0153-2m-verification-sharding-plan.md -->
~~~~~~~~markdown
# ADR-0153 (draft) — 2M verification by shards: the implementation plan

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* Status: **Draft plan** (rcore/cap-s1, stage 7 of the user's capacity plan, 2026-09-27). **Decides
  nothing and arms nothing**: the 2M class stays capped — `c_2M = 1` held to `Final` (its row's static
  `max_inflight_claims`, which no ρ scales), on the C7 list (ADR-0152 D7, ADR-0160 D-13), uncredited:
  its escrow `E`, its seats' duty and lock and its `G` today's at every ρ (F1); only its weight
  reservation is `⌈w/ρ⌉` like every attempt's under F-W, the whole bond liable behind it (AG-2) — until
  the gates below pass and the user decides.
* Supersedes nothing. Related: ADR-0152 §4-quater (held attention, the 2M deadline redesign of
  2026-09-24), ADR-0160 (capacity; 2M is excluded from every ρ step's credit), ADR-0151 (liveness is
  structural), the kv-codec resource profile (2026-09-23).

## 1. Why 2M is capped, in physical numbers

Verification capacity is physical — replay time and memory on the verifier's hardware — and no
collateral or protocol rule buys it:

| quantity (2M context, 1.5B dense, A16-KV-i16) | measured / derived |
|---|---|
| K/V resident for one execution | 7.00 GiB (i32: 14.00 GiB) |
| one replay's need (artifact, K/V, attention and trace scratch, Fold capture, leg) | 12.42 GiB |
| prefill throughput on the reference host | ≈ 4 positions/s → 262,143 positions ≥ 18 h |
| one full replay | ≈ 9.7 days ≈ 6,984 DAA at 120 s/DAA — longer than the court window (3,000 DAA) |
| an S1 partial seat's replay hashes (dense capture) | ≈ 607 GiB at segment 0 — S1 is structurally infeasible for 2M |
| attempt step space | 27,002,845,160 leaves (dense capture 1.7 TB; held classes Fold) |
| uncovered loss per 2M claim at launch (C) | ≤ 63,599 MSK, `c_2M = 1` (≤ 9.35 M MSK / month) |

So one honest verifier cannot prove a 2M fraud inside today's deadlines, and ρ multiplies claims, not
verifiers. The cap is the only sound rule until verification itself is split.

## 2. The idea: a 2M execution verified as independent shards

A 2M execution is split at `K` checkpoints (every `P` positions, per class, from measurement) into
shards; shard `j` is replayable from checkpoint `j−1`'s state alone. A panel verifies a claim by
assigning shards to seats (each seat replays ≤ `P` positions — hours, not days), and a court localises
a dispute to one shard. Capacity then scales with seats × GPUs, not with one host's 9.7 days.

## 3. Work items (each a lane with its own fence; order is the dependency order)

1. **Measure (no fence).** On the reference GPU host and the fleet's: positions/s by context depth; the
   time to build, serialise and load a checkpoint (K/V at a boundary: ≈ 7 GiB i16 at depth 2M, scaling
   with depth); the time from detection to a complete court evidence object; per-shard replay at `P` ∈
   {16k, 32k, 65k}. Deliverable: the table the deadlines are derived from (the user's rule of
   2026-09-24: deadlines per class from measured cost + margin, never one global number).
2. **Checkpoint commitments (consensus).** The attempt commits a root over the shard boundary states
   (the Fold capture already derives the roots dense capture does; add the boundary state digests as
   leaves). A claim's material names them; a shard is replayed from its boundary.
3. **Checkpoint availability (DA).** A boundary state is 1–7 GiB: the producer must serve it on demand
   within the plain-response deadline (42 DAA), or default (an existing DA unit kind, sized). Retention
   until the claim's liability window closes.
4. **Shard assignment in the panel (consensus).** Each seat is drawn for a shard subset (the segment
   mask machinery, `PalwSegmentMaskV2`, generalised from positions to checkpoint shards), with
   `k ≥ 2` coverage per shard; the licence's coverage basis counts shards, not whole replays.
5. **Shard courts (consensus).** A dissection localises to one shard, whose re-execution fits the
   compute-bearing deadline; a forged boundary state is refuted by the previous shard's replay
   (chain of custody). The held-attention (A-held) procedure is the template.
6. **Three deadline kinds, per class, derived** (the user's split): plain response, compute-bearing
   verification (from item 1's shard time + margin), economic liability (never released before
   verification can conclude).
7. **Economic `Final` for 2M re-enabled** only when items 2–6 are live and an honest verifier has proven
   a planted 2M fraud inside the derived deadline on a drill (the user's condition).
8. **Only then**: `c_2M ≥ 2`, 2M credit and 2M in a capacity step — each its own flag day, never by ρ.

## 4. Gates before any 2M cap moves

* G-A: item 1's table published; shard replay ≤ the compute-bearing deadline with ≥ 2× margin.
* G-B: a drill proves a planted 2M fraud (one forged shard) and the forger's bond is taken, with the
  checkpoint served by DA.
* G-C: the verifier fleet's shard throughput covers the target 2M claim rate with k ≥ 2 per shard.
* G-D: the user's decision.

## 5. What stage 7 of the capacity plan ships instead

ρ 1000 as its own package for the floor only (ADR-0160 §9.1), with every capacity fence at or below it
and `k_aud = 2`; 2M stays at `c_2M = 1`, uncredited, off the capacity list's credit
(`palw_capacity_class_creditable_v1`: the floor only), and a test pins that no package moves it.
~~~~~~~~
<!-- ADR_SOURCE_END:0153-2m-verification-sharding-plan.md -->

</details>

