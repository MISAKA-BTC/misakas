# ADR-v2-0015 — Permissionlessモデル登録・class profile・不変性

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 5本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0003](../rfc-v2/0003-model-onboarding-and-artifacts.md)

## 1. この文書の役割

対応する実行familyのmodelを、固定identity・profile・資源条件に基づいて登録する判断をまとめる。

## 2. 統合した判断の読み方

### 2.1

execution class・model band・routeの適合性と、permissionless登録の受入条件を分離する。誰でも登録できることは任意architectureを無条件に実行できる意味ではない。[旧ADR-0034](../adr/0034-palw-execution-class-model-band-routing.md) [旧ADR-0056](../adr/0056-palw-permissionless-class-admission-and-share-economy.md)

### 2.2

model dataからprofileを導出し、shardの検証・裁定条件とregistry lifecycleを合わせて読む。[旧ADR-0100](../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) [旧ADR-0135](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md)

### 2.3

登録済みModel ID／artifact root／実行仕様を永久不変とし、異なる内容は独立した新モデルとして登録する。[旧ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)

## 3. 維持する境界・後続改定

旧profileのモデル資格審査と、後続ADR-0179が廃止したモデル単位の審査を混同しない。claim単位の検証と技術的登録条件は別途維持される。

## 4. 隣接する判断との分担

artifact所有とlineageは[ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md)、二階層と年次指定は[ADR-v2-0019](0019-model-tiers-and-annual-catalog.md)、計測は[ADR-v2-0030](0030-sharded-verification-and-model-measurement.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0034-palw-execution-class-model-band-routing.md](../adr/0034-palw-execution-class-model-band-routing.md) | L1–L644 | L15–L40 |
| [0056-palw-permissionless-class-admission-and-share-economy.md](../adr/0056-palw-permissionless-class-admission-and-share-economy.md) | L1–L287 | L15–L19 |
| [0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md](../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) | L1–L425 | L15–L39 |
| [0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) | L1–L438 | L15–L31 |
| [0175-registered-models-are-permanently-immutable.md](../adr/0175-registered-models-are-permanently-immutable.md) | L1–L103 | L12–L12 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0034-palw-execution-class-model-band-routing"></a>

<details>
<summary>0034-palw-execution-class-model-band-routing.md — 原文 L1–L644</summary>

[原文を開く](../adr/0034-palw-execution-class-model-band-routing.md) · SHA-256: `e6b5f79f095910be77ca01f0a7edb064f21d1260081bb7bc384454f4f54db78a`

<!-- ADR_SOURCE_BEGIN:0034-palw-execution-class-model-band-routing.md -->
~~~~~~~~markdown
# ADR-0034: PALW re-verification routing — four execution-class families, five model bands, one deciding binding

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Accepted (architecture; activates nothing). Amended 2026-08-26 by
[ADR-0053](0053-palw-one-execution-family.md) Decision 5: the `Metal` routing family joins `Cuda`
and `Rocm` as reserved.** Nothing routed to it once the Metal/GGUF verification family was
withdrawn, and a Metal binding today would be a deterministic-integer class on Apple GPUs — a thing
this tree has no kernel for and no vendor guarantees float reproducibility for. It returns the way
CUDA does, through the ADR-0027 conformance campaign. Everything else here stands.** Devnet / shadow / zero-credit envelope
unchanged. This ADR fixes how re-verification work is **routed** once PALW carries more than one
model and more than one backend: which coarse keys a receipt and a verifier advertise, how the
two sides are matched automatically without a human picking jobs off a list, and — the part that
keeps the rest honest — which key is allowed to *decide* anything. It was circulated in draft
under the number 0028 ("Execution Class / Model Band による PALW 自動再検証ルーティング設計");
that number was already taken by the challenge-sampling protocol, which this ADR builds on
rather than replaces. One element of the draft is **rejected, not adopted** — the
`FINALIZED_WITHOUT_REPLAY` crediting path — see §7, which explains why it cannot coexist with
premises this fork has already accepted and what replaces it.
Date: 2026-08-16
Relates to: ADR-0026 §3/§8 (exact-within-class, per-class signed bundles), ADR-0027 (P1–P3,
one-step adjudication, GPU admission falsifiability), ADR-0028 (credit gate, `select_replay_panel_v1`
assignment, DAA windows, `q` as funded replays, opening-call audits), ADR-0029 (carriage kinds
0x01–0x06, the version-trap rule, mass budgets), ADR-0030 (step space, `PalwShapeProfileV3`,
`PalwStepOpKindV1`), ADR-0031 (transcendental provenance; the Apple-libm admission boundary),
ADR-0032 (fee/bounty mechanics), ADR-0033 (where `credit(C)` is evaluated),
`consensus/core/src/palw_registry.rs` (`PalwClassRegistrationV1` — the object this ADR
generalizes), `consensus/core/src/palw_schedule.rs` (`select_replay_panel_v1`,
`credited_ceiling_tokens_v1`, `PALW_SCHEDULE_REPLAY_KAPPA`), `consensus/core/src/vlt.rs`
(`derive_runtime_class_id`, the class-tag literals), the v2 design §16 (capability declaration).

## Thesis

Routing identity is **three-keyed**, and the keys have strictly ordered authority:

```
ExecutionFamily   (4 values, fixed)     — routes work to hardware that could hold it
ModelBand         (5 values, fixed)     — sizes windows, bonds and capacity; coarse load class
ModelBinding      (exact, unbounded)    — the ONLY key a verdict may reference
```

A family and a band may decide *who gets asked*. Only the exact binding — model × family ×
class version × runtime manifest × quantization, resolving to a registered
`runtime_class_id` lineage — decides *what a replay means*. Two receipts in the same family
and band are not comparable; two receipts under the same binding are exactly comparable, to
the byte (ADR-0026 §3, unmoved). This is what lets model count grow without growing the class
count past four or the band count past five, and without ever weakening the exactness rule.

## Premises carried forward, plus one

P1 (no BFT), P2 (no challenge-randomness dependence), P3 (slash-terminal) from ADR-0027; the
ADR-0028 corollary (**randomness may schedule work; it may never make an unchecked job safe**);
class = conformance to the canonical reference (ADR-0027 §2); class membership is
manifest-hash exact, never a label (ADR-0026 §3). One new premise joins them:

> **A family is a routing index, not a determinism claim.** The measurements that forced the
> class mechanism also refute every coarser identity: Metal ≠ CPU on the same job
> (`ba3b9994…` ≠ `d04672dc…`); arm ≡ x86 only 7/8 seeds (a prefill batch-GEMM near-tie);
> EPYC ≠ Broadwell 4/8 on the registry Q8_0 artifact; fp16-lane attention accumulation and
> repack coverage split aarch64 from x86_64 *structurally* (ADR-0030 Facts 13–14). "CUDA" or
> "CPU" is where a job *could* run, never a set whose members agree. Any rule that compares
> two roots because their families match repeats the golden-gate label bug (`f9ab6ab`) at
> protocol scale.

## Decision

### 1. The three keys, and what each is allowed to touch

```rust
enum PalwExecutionFamilyV1 {          // new; frozen at four — no fifth family
    Metal = 1,
    Cuda  = 2,
    Rocm  = 3,
    Cpu   = 4,
}

enum PalwModelBandV1 {                // new; frozen at five — no B5
    B0 = 0, B1 = 1, B2 = 2, B3 = 3, B4 = 4,
}
```

| key | may influence | may never influence |
| --- | --- | --- |
| family + family version | candidate discovery, capability indexing, coverage accounting, UI grouping | any verdict, any comparison of roots, any slash |
| band | window sizing, bond floors, capacity/admission caps, audit-rate scaling, reward scaling | verifier selection **alone** (§6), any verdict |
| binding | everything: panel eligibility, replay comparison, refutation, adjudication | — |

The family is not new state so much as recognition of structure the class tags already carry:
`misaka-palw-lite-cpu/x86_64/v1` is *family segment / arch segment / version segment*, and
`derive_runtime_class_id` hashes the whole tag. This ADR makes the first and last segments
machine-readable (registry fields, §3) without touching how `runtime_class_id` is derived —
existing class ids do not move. New acceleration hardware must be shown to fit an existing
family before any proposal to add one; the draft's rule stands: **no fifth family, no B5.**

A **family version** (the draft's `class_version`) names a coordinated runtime generation
inside a family — the `/v1` tag segment plus a `PalwExecutionFamilyManifestV1` enumerating
which exact runtime manifests that generation admits (worker binary hash lineage, kernel
bundle, deterministic flags, golden-set root, activation/retirement epochs — the per-class
signed-bundle discipline of ADR-0026 §8, indexed). At most **two versions per family** are
active at once (`current`, `previous`); retirement follows the profile rule — profiles are
never overwritten (ADR-0027 §5).

**The CPU family is not the adjudicator.** The CPU family is for miners/re-executors that run
LLMs on CPUs under a pinned deterministic build (today's `CPU_BUILD_PROFILE`:
`no-blas/no-openmp/single-variant/…` — general BLAS is already excluded by construction, per
the draft's requirement). Full-node one-step adjudication runs in canonical reference
arithmetic (`palw_reference.rs` ruleset v2 + the Berkeley-SoftFloat second implementation in
`misaka-palw-reference2`) — a separate thing from any family, needing no family membership
(ADR-0027 §2).

### 2. Draft vocabulary → this fork's identifiers

The draft named objects the tree does not; this table is normative for implementation so the
ADR and the code cannot drift apart. "new" = to be landed, consensus-inert first, like
everything before it.

| draft name | here |
| --- | --- |
| `execution_class` | `PalwExecutionFamilyV1` (new) — "family", to avoid colliding with `runtime_class_id`, which stays the exact-class identity |
| `class_version` | family version: the tag's `/vN` segment + `PalwExecutionFamilyManifestV1` (new) |
| `ExecutionClassManifestV1` | `PalwExecutionFamilyManifestV1` (new); per-class facts stay in `RuntimeManifestV2` and the registration row |
| `model_id` / `model_root` | `model_profile_id` (already in every v2 envelope/context); `ModelDefinitionV1` (new) gives it a registered preimage — today it is an opaque input |
| `model_binding_id` | `binding_id` = `registration_id()` of a registration row (§3): **a binding IS a registration row**, extended with model/family/band fields |
| `model_band` | `PalwModelBandV1` (new), binding-scoped |
| `PalwReceiptV3` | the Stage-1 successor of carriage kind 0x01 (`PalwCommitmentCarriageV1`) carrying the routing keys — a **new kind/id** per ADR-0029's version-trap rule, never a retrofit |
| `VerifierCapabilityV1` | `PalwVerifierCapabilityV1` (new): the binding-aware successor of the v2 design §16 capability declaration (node-side handle today: `PalwComputeCapability`) |
| Receipt Verifier (role) | not a party: stateless carriage validation (`validate_palw_carriage_v1`) + the registry equality checks of §5 |
| Bonded Re-executor | assigned panel member (`select_replay_panel_v1` lineage) + ADR-0028 §4 attester |
| Challenger | ADR-0027 §1 refuter (re-execute → name the first divergence) |
| `ReplayAttestationV1` | `PalwExecutionAttestationV1` (landed, `palw_slash.rs`) + its carriage (kind 0x02) |
| adjudication primitives (`LOAD_FRAGMENT`, `MUL_INT`, …) | the landed step taxonomy: `PalwStepOpKindV1` (17 kinds, `EmbedLookup … CpyF32F16`) resolved to kernel programs by `kernel_semantics_id` (`resolve_kernel`), executed by `check_execution_step_refutation_v1` |
| `palw_reference.rs` "one primitive" | one **step tile** under ruleset v2 — already the landed unit |

### 3. The registry: definitions, bindings, and the row that already exists

Model identity and execution identity register separately and join in a binding:

```rust
struct ModelDefinitionV1 {            // new
    model_profile_id: Hash64,         // the id every envelope already carries, now with a preimage
    gguf_sha256: [u8; 32],            // artifact identity (the qwen35_pins discipline, generalized)
    gguf_size: u64,
    tokenizer_id: Hash64,             // tokenizer_id_v2_for_gguf lineage — pinned outside the runtime
    architecture_id: Hash64,
    total_parameter_count: u64,
    active_parameter_count: u64,      // MoE honesty: totals alone misclassify (draft §7.1)
    publisher_signature: Vec<u8>,
}
```

A **binding** is `PalwClassRegistrationV1` (landed, consensus-inert, `palw_registry.rs`)
extended with the routing keys — the registration row already co-locates everything a binding
means (class id, manifest hash, model id, shape profile, commitment form, adjudication depth,
replay-cost measurement, credited ceiling, windows). Added fields:

```
class_tag: String                            // the SAME string runtime_class_id hashes —
                                             //   validate() recomputes the id from it and
                                             //   machine-reads family/version out of it
                                             //   (routing_keys_for_class_tag_v1, fail-closed
                                             //   on unrecognized tag shapes), so neither key
                                             //   below is ever self-declared
execution_family: PalwExecutionFamilyV1      // checked against the tag, never believed
family_version: u16                          // checked against the tag, never believed
model_band: PalwModelBandV1                  // derived, §4 — validate() rejects a declared band
                                             //   that does not equal the derivation (the
                                             //   CeilingNotDerived pattern, applied to bands)
quantization_id: Hash64
model_artifact_bytes: u64                    // must equal the signed ModelDefinitionV1.gguf_size
                                             //   at activation (binding_matches_definition_v1) —
                                             //   the one band dimension the artifact digest
                                             //   mechanically pins is not left self-declared
peak_memory_bytes: u64
max_proof_material_bytes: u64
```

The wall-clock replay deadline is an **accessor** (`replay_deadline_secs(block_time_ms)`),
not a stored field: a stored copy is uninterpretable without also storing the block time it
assumed, and a network re-parameterizing its block rate (this branch's own 1 s-block work)
would invalidate every drafted registration. The registration's `version` field moves to
**layout generation 2** with this extension, so any stray generation-1 bytes fail
`UnsupportedVersion` instead of misdecoding at the insertion point.

`binding_id = registration_id()` over the extended preimage. The same model under Metal, CUDA,
ROCm and CPU is four bindings; a model with no CPU binding registered simply has none — the
family count does not move (draft §6.2, adopted verbatim). Re-banding an existing binding is
prohibited: a re-classification is a **new binding id with its own activation epoch**
(draft §21.9 — the same rule the registry already enforces for every identity field: ids move,
edits do not exist).

Today's tree is row 1: `Qwen3.5-2B-Q4_K_M` (`aaf42c8b…`) × Cpu family ×
`misaka-palw-lite-cpu/x86_64/v1` — and row 2, its aarch64-dotprod sibling; the Metal tags are
candidate rows pending their own registration measurements. Nothing about multi-model routing
may break while the registry holds exactly these.

### 4. Model band: derived from the binding, and capped by the pruning horizon

A band is a property of a **binding**, never of a model name (the draft's CUDA-Q4 vs CPU-Q8
example is exactly right), and never self-declared (§21.1). The v1 derivation, adopted from
the draft with its inputs grounded in landed measurement types:

```
S_artifact = model_artifact_bytes / 4 GiB
S_memory   = peak_memory_bytes / 8 GiB
S_work     = max_replay_work_units / BASE_REPLAY_WORK_UNITS
S_proof    = max_proof_material_bytes / 64 MiB
resource_score = max(S_artifact, S_memory, S_work, S_proof)

band: ≤1 → B0   ≤2 → B1   ≤4 → B2   ≤8 → B3   ≤16 → B4   >16 → not registrable in v1
```

`max_replay_work_units` is derived from the binding's own registered replay-cost measurement
at its derived ceiling (`replay_work_ms_v1`) — measurement-derived, never miner-declared; the
active-parameter-based derivation is a future refinement, and no doc may claim it is wired
before it is. `BASE_REPLAY_WORK_UNITS` is a **frozen snapshot** of row 1's measured
full-replay cost at v1-definition time (679 975 ms), so B0 means "costs about what the
reference binding cost when the derivation was defined." A later fleet re-bench does NOT move
the constant — every registered band re-derives against it, so an edit would re-band every
binding at once (re-banding-by-constant); a base change is a new derivation version, never an
edit. Every coefficient above is a **placeholder until devnet measurement fixes it** —
shipping a placeholder as measured is the §15-class violation it always is. One consequence
is stated rather than hidden: the work dimension is window-relative (a class registering
tighter windows both works less and credits less), which the economic simulation gate must
price — the artifact/memory/proof dimensions are the window-independent floor.

What a band does, concretely:

* **Windows scale with the band.** `w_replay ≥ κ · p99_cold_replay(binding, ceiling)` with
  `κ = PALW_SCHEDULE_REPLAY_KAPPA = 3` — a B4 replay deadline is long because its p99 is long
  (draft §22.2, now with the landed inequality enforcing it: `validate()` already rejects
  `ReplayDoesNotFit`).
* **And therefore the pruning horizon caps the band.** ADR-0028 §3's chain of inequalities
  (`W_challenge ≥ W_replay + L_bisect · W_round + margin`, `finality < W_challenge`,
  `W_challenge + slack < pruning horizon` — 30 h / 38 h on the two parameter sets) does not
  bend for a big model. A binding whose measured p99 pushes its window set past the horizon
  **cannot be registered as creditable** at any declared ceiling — the existing
  `credited_ceiling_tokens_v1` / `ReplayDoesNotFit` machinery generalizes per band. Roughly:
  B0–B1 fit today's horizons trivially (fleet p99 37–91 s at D=512); B3–B4 fit **only if**
  the checkpoint interval registered with the binding makes rung responses interval-priced
  rather than full-replay-priced, and possibly not at all without a longer horizon or
  pruning-surviving carriage. That is a parameter decision taken explicitly at registration —
  never a silent stretch, exactly ADR-0028's rule. A binding that cannot credit may still run
  SHADOW (uncredited) for measurement.
* **Bond floors and admission caps index by band** (§8; the parameter table), including the physical cap
  `R_jobs · q ≤ Σ capacity(p99)` from ADR-0028 §4e, now computed per binding.
* **Initial family restriction:** `CPU max active band = B1` (row 1 is B0); every other
  family starts at B0. The cap is a **registered fact** — a `max_active_band` field on the
  family-version manifest (§1), seeded from `initial_family_max_active_band_v1` — so raising
  it is publishing a new manifest record once the independent-re-executor count and measured
  deadlines the draft demands exist, never a code edit two observers could disagree across
  mid-rollout; among a generation's records the most restrictive published cap wins.

The draft's illustrative table (B0 ≈ 1–3B … B4 ≈ 70B-class / large MoE) is kept as
explanation, not as normative input.

### 5. Receipts carry the keys; the registry, not the miner, gives them meaning

The Stage-1 commitment body (kind 0x01's successor, new id) grows exactly three routing
fields — `binding_id`, `execution_family`, `model_band` — beside what it already carries
(envelope, committed form, committed root, legs binding, identity, signature). Acceptance
checks, in the landed vocabulary (the draft's §8.2, made mechanical):

```
stateless:  validate_palw_carriage_v1 lineage — decode, caps, signature (ML-DSA-87 carriage
            context), committed-root recomputation for composite forms (recomputed, never
            trusted — ADR-0029's rule)
registry:   binding_id is ACTIVE (not LOW_COVERAGE-frozen states of §10, not retired)
            envelope.model_profile_id      == binding.model_profile_id
            envelope.runtime_manifest_hash == binding.runtime_manifest_hash
            envelope.runtime_class_id      == binding.runtime_class_id
            envelope.shape_profile_id      == binding.shape_profile.shape_profile_id()
            envelope.trace_scheme_id / cu_ruleset_id == the registered ones
            carried execution_family        == binding.execution_family
            carried model_band              == binding.model_band      ← band forgery = invalid
            exact_decode_tokens ≤ binding.credited_ceiling_tokens (crediting path)
                                 ≤ PALW_V2 format ceilings (always)
dedup:      first-accepted-wins on committed_root (ADR-0029 §2)
```

A receipt whose declared band or family disagrees with the registry row is rejected as
malformed — the miner's claim is checked against `registry[binding_id]`, never believed
(draft §8.2's rule, unchanged). No re-execution happens at acceptance; this layer is cheap by
construction and runs on every node.

### 6. Verifier capability: two layers, and an agent that registers itself

Capability is **two claims with different verification costs**, and both are required
(draft §9.1 — band-only assignment ships jobs to hosts that do not hold the model):

* **Hardware capability** — family, family version, `max_model_band`, memory, concurrency,
  bond account, availability TTL. Cheap to declare, cheap to index.
* **Ready bindings** — the set of `binding_id`s this verifier can replay *now*: artifact held
  (exact digest), runtime boots, **golden set passed** (the landed `v2-selftest` discipline —
  a ready claim without a golden pass is not a claim), replay benchmark measured (the landed
  `v2-replay-bench` path). Committed as `ready_binding_root` (Merkle); duty eligibility and
  any claim carry `binding_id + proof` against it.

```rust
struct PalwVerifierCapabilityV1 {     // new; successor of the v2 design §16 declaration
    verifier_id: Hash64,
    execution_family: PalwExecutionFamilyV1,
    family_version: u16,
    max_model_band: PalwModelBandV1,
    ready_binding_root: Hash64,
    max_concurrency: u16,
    available_slots: u16,
    max_accepted_replay_secs: u32,
    minimum_reward: u64,
    replay_bond_outpoint: TransactionOutpoint,   // the bond-UTXO discipline, ADR-0016 lineage
    available_bond: u64,
    availability_expiry_daa: u64,
    capability_nonce: u64,
    signature: Vec<u8>,
}
```

The **`misaka-palw-reexecutor` agent** (the operational successor of `misaka-palw-agent`
Phase A plus `palw-shadow attest`) automates the draft's §9.3 sequence: detect backend →
resolve family/version → scan local artifacts against active bindings → verify memory fit →
run goldens → run the replay benchmark → emit the ready set → derive `max_model_band` →
submit the capability → heartbeat. No human enumerates models. Operator policy stays local
(allow/deny model globs, auto-download budget, max band, concurrency, minimum profit,
power/schedule caps — the draft's §9.4 TOML shape is adopted as the agent's config surface).
A capability is a signed, TTL'd, nonce-monotonic statement; an expired capability is simply
not eligible — silence is never an offense at this layer.

### 7. What routing may decide — and the draft state that is rejected

**Adopted:** every accepted receipt on the crediting path gets an **assigned, funded panel**,
and assignment is the deterministic ticket lottery of ADR-0028 §2 with its eligibility
predicate extended by the two new layers:

```
eligible(v, C) ⟺ v.execution_family == binding(C).execution_family
              ∧ v.family_version    == binding(C).family_version
              ∧ v.max_model_band    ≥  binding(C).model_band          // routing precondition,
              ∧ ready(v, binding_id(C))   // Merkle proof against ready_binding_root — the
                                          //   part band can never substitute for
              ∧ v.available_slots > 0
              ∧ v.available_bond ≥ required_replay_bond(band)
              ∧ v.availability_expiry_daa > now
              ∧ reputation(v) ≥ floor
              ∧ ¬same_control_domain(v, executor(C))    // self-verification excluded, operator
                                                        //   aggregation counted once (v0.1)
              ∧ ¬frozen(v)

panel(C) = q lowest tickets over eligible(·, C)         // domain-keyed, executor-excluded,
                                                        //   anchor(C) at daa(C)+Δ_bind — the
                                                        //   select_replay_panel_v1 lineage
```

Three panel rules the twins did not need, because their candidates come from one
consensus-registered validator set while routed candidates are **self-published capability
records**: (1) the ticket additionally binds `binding_id` and the escalation round — a
commitment root reused across two bindings, or a re-draw at the same anchor, must not produce
correlated draws; (2) a verifier id appearing more than once in the candidate set is dropped
entirely (the caller failed nonce supersession; choosing between two conflicting records
would make the panel input-order-dependent — fail-closed); (3) **one seat per control
domain** among distinct verifiers, lowest ticket holding the seat — the §10 counts-once rule
applied to seats, or one operator's sibling identities would collapse the funded redundancy
of `q` to a single machine.

Band alone never selects (`Receipt = B4, Verifier max = B4 → assign` remains the named
anti-pattern); the full conjunction is the assignment rule (draft §12.2, adopted).
`q ≥ 2` on the crediting path (ADR-0028 §4 — the draft's `required_independent_replays = 1`
default is corrected to the accepted value; one replay is a liveness single-point-of-failure).

**Escalation replaces monopoly-claiming.** The draft's Round 0/1/2 ladder maps onto the
assignment model rather than a race: Round 0 *is* the panel (a funded duty, not an exclusive
right); if `w_replay` passes without the required on-time attestations, the panel is
**re-drawn wider** at the escalation anchor (original no-shows keep their objective offense),
and a further lapse raises the reward (`ReplayBountyEscalated`). Bonds are an eligibility
floor with a capped influence on selection weight — stake never buys the panel (draft §12.4,
adopted). The refutation window stays permissionless throughout: anyone who replays and
diverges may refute regardless of panel membership (ADR-0028 §4d — rivalry, not altruism).

**Rejected: `FINALIZED_WITHOUT_REPLAY`.** The draft finalizes receipts that lose a
band-thresholded lottery (`selection_hash < replay_threshold[model_band]`) without any
replay. Under P2 that is precisely the dependence this fork refused twice already: the moment
the draw is predictable or grindable, *the unchecked set is the attacker's choice* — and the
ADR-0028 corollary is explicit that no rule of the form "not drawn, therefore creditable
unchecked" may exist. It does not return here. `credit(C)` stays ADR-0033's predicate,
verbatim: window closed ∧ ≥ 1 assigned root-equal attestation ∧ no accepted refutation; zero
attestations ⇒ credit 0, never "shrink the panel and mint." The verification tax is the
`(1 + q·ρ_v)` issuance split, visible in emission — not a cost the lottery quietly deletes.

**What survives of the draft's randomness and risk scoring** (both decide nothing):

* the PRF-selected **opening-call audit** stream (ADR-0028 §5, landed seam) — answerability
  and DA sampling, priced per ADR-0032; its *rate* may scale with band and risk;
* **risk-based escalation of `q` and audit rate** — new miner, new binding, new family
  version, anomalous timing, mismatch history, coverage thinness raise the *redundancy* and
  *audit frequency* purchased for a job (the v0.1 §9.2 dynamic inputs, unchanged meaning).
  Risk raises how much checking is funded; it never selects who is guilty and never waives a
  replay on the crediting path.

### 8. Bonds: two escalating locks, on the rails that already exist

The draft's two-bond structure is adopted and lands on ADR-0032's mechanics — no new covenant
machinery:

* **Replay bond** — the panel-duty lock: claim-hoarding, timeout and false-completion
  pricing. Sized `max(band_min_replay_bond[band], reward × multiplier, externality)`.
  No-show slashes per the amended floor `min(100 · ρ_v · base, bond)` (the B15 finding —
  uncollectible floors are lies).
* **Challenge bond** — the refutation lock, strictly larger; it prices false-slash attempts
  and adjudication DoS. **No vote can confiscate it** — under P1 the only thing that may take
  a challenge bond is losing the deterministic one-step check
  (`check_execution_step_refutation_v1` → `NoFaultFound`), and the only thing that may take a
  miner's bond is losing it (`ComputationMismatch`). `Unadjudicable` (outside the kernel
  catalog / opaque decoder) slashes **nobody** — the landed three-way verdict is load-bearing
  here.

Settlement flows are ADR-0032's: match ⇒ bond back + `ρ_v · base` fee + reputation; timeout ⇒
partial/total replay-bond forfeit; correct challenge ⇒ bond back + bounty
(`min(49 % · slashed, B_cap)` under ADR-0032's 2026-10-10 amendment to the refutation carrier's `(tx_id, 0)` slot, remainder burns,
`slash_id`-idempotent); false challenge ⇒ challenge bond split burn / treasury / injured-miner
compensation. DA failure is the miner's offense, never the assignee's
(`DATA_WITHHOLDING` — draft §21.10, already the accepted rule).

### 9. Adjudication is band-independent, bounded, and per-binding in depth

The full node holds no model, no GPU, no family membership (draft §2.8–2.9 — already the
architecture). The hard bounds that make "one disputed primitive" true at any model size are
the **landed constants**, restated as the invariant the draft asked for:

```
one step  = one output tile, recomputed from openings under ruleset v2
            (palw_reference.rs + the SoftFloat-3e second implementation, 12.5 M-case differential)
opening   ≤ 22 siblings (PALW_STEP_LEG_MAX_OPENING_SIBLINGS); tile ≤ 256 KiB
            (PALW_STEP_LEG_MAX_TILE_BYTES); state chunk ≤ 1 MiB
carriage  ≤ 16 openings per call; evidence chunks ≤ 340 000 B × ≤ 4; every measured
            worst-case step opening fits one 480 000-mass standard tx (ADR-0030 §3)
bisection ≤ 48 rounds over a space ≤ 2^40 (PALW_BISECT_MAX_ROUNDS / _MAX_SPACE)
reference ops per check ≈ 10⁵–10⁷ (dot ≤ 2^20, GEMM dim ≤ 4096)
```

Growing the model to 70B / 200B / 500B grows the *number* of steps (and the band), never the
size of one adjudication — these constants do not scale with the band, by rule. The draft's
latency targets are adopted as engineering targets for the one-step check: p50 < 1 s,
p95 < 5 s, p99 < 20 s.

The draft's primitive list (`LOAD_FRAGMENT`, `MUL_INT`, …) is superseded by the landed step
taxonomy: `PalwStepOpKindV1`'s 17 kinds, resolved per node to `kernel_semantics_id` programs
(the transcription catalog: L2Norm, RmsNormFused, Swiglu, sigmoid/softplus over glibc-2.39
programs, GdnCore in both dot structures — growing by transcription + differential
validation, never by edit).

**Adjudication depth is a binding fact, surfaced by routing.** `PalwAdjudicationDepthV1`
already encodes it: `ArithmeticCatalogued` bindings expose the full refutation ladder;
`StructuralOnly` bindings (today: every Apple-libm-dependent class — ADR-0031's honest
admission boundary) expose structural faults, contradiction certificates and DA offenses, but
not arithmetic conviction. Capability listings, coverage accounting and the explorer MUST
display depth per binding; a Stage-2 (slash-bearing) binding requires `ArithmeticCatalogued`
plus the registry's existing `stage2_eligible` gates. CUDA and ROCm are **reserved families
with no code paths today** (the tree's only CUDA reference is the literal `cuda-off`);
admitting the first binding in either is the ADR-0027 falsifiable campaign — deterministic
kernels, pinned reductions, reference conformance — and some hardware is expected to fail it.

### 10. Coverage: a binding nobody can replay does not get to exist quietly

Binding activation requires (draft §16.1, with the accepted thresholds):

```
golden set passed on ≥ the minimum independent re-executors
artifact retrievable under the DA rules            independent ready re-executors:
family version active IN THE MANIFEST SET              devnet ≥ 3, mainnet ≥ 5
binding's runtime manifest ∈ the generation's      (same control domain counts once)
  admitted lineage                                 adjudication depth recorded
declared band ≤ the generation's registered cap    signed ModelDefinition joins the row
windows/ceiling validate() against the network       (model_profile_id + artifact size)
family not reserved (Cuda/Rocm never activate in v1 — their first binding is the
  ADR-0027 conformance campaign, a different act than activation)
```

The generation facts are derived from the manifest records inside
`binding_may_activate_v1` — not accepted as caller booleans — so two observers of one
registry state cannot disagree about what activated.

Live coverage walks the ladder — each transition an on-chain-observable fact, none a
governance vote:

```
ACTIVE → LOW_COVERAGE (ready count < threshold):  replay reward up, audit rate up, warning
       → THROTTLED:   new-receipt admission rate-limited; high-value jobs refused
       → FROZEN:      no new receipts; existing disputes run to completion
DEPRECATED → RETIRED: planned retirement; receipts against retired bindings rejected
CONTRADICTION_FREEZE: ClassContradictionCertificateV1 — two same-class signed differing
                      roots ⇒ freeze, bonds frozen not released, no slash (ADR-0027 §5);
                      registry-side this is to_zero_credit(): the ceiling IS the switch
```

Two orderings are normative: **starvation outranks deprecation** (a deprecated binding still
accepts receipts, so zero ready re-executors — or a zero threshold, which is no threshold —
freezes it regardless of the planned retirement), and the `epochs_below_min` streak is
counted by ONE published rule (`next_epochs_below_min_v1`: reset at-or-above threshold,
saturating increment below), because a privately invented counter would let two observers
classify the same chain history into different coverage states.

"検証者が存在しないモデルを登録したまま、祈りで運用することは認めない" — the draft's sentence is
the rule: a binding without its minimum independent ready re-executors never activates, and
one that loses them stops accepting work *before* the panel machinery would start assigning
duties nobody can serve (an empty eligible set already means credit 0 — never shrink-and-mint).

### 11. Same-binding decides; cross-anything diagnoses

Unchanged, restated once against the new keys: slashing-grade comparison exists **only inside
one binding** (same `binding_id`, hence same exact class). Cross-binding, cross-family,
cross-band comparisons — including CUDA-vs-Metal on the same model — are the diagnostic tier:
drift alarms, golden re-runs, contradiction investigation, freeze inputs. They never slash,
never mint, never gate a block (ADR-0026 §3's carve-out; ADR-0027 made it structural). A
tolerant comparator anywhere in the binding path remains a new-scheme-version event.

### 12. `llm.misakascan.com` is a window, never an oracle

The explorer/market UI shows what the chain and the capability set already say — receipts
with their three keys and depth, candidates with rewards/bonds/deadlines/eligible-count,
verifier dashboards, coverage states. It schedules nothing and adjudicates nothing; every
listed action is reachable agent-side from chain RPC alone, so the site's death changes
liveness not at all (draft §19, adopted; ADR-0029's watcher/reporter is the data path).
Keys and model execution live in the local agent, never the browser.

## Receipt lifecycle (the draft's §17, corrected)

```
SUBMITTED → stateless+registry checks → ACCEPTED (panel derivable at daa(C)+Δ_bind)
  ATTESTED (≥1 on-time assigned root-equal)  → W_challenge close → CREDITED
  no on-time attestation (escalations spent) → W_challenge close → NOT_CREDITED (credit 0)
  MISMATCH found by anyone → CHALLENGE_OPEN → direct refutation
                                            → (DA-degraded: BISECTION, ≤48 rungs)
                           → ONE_STEP_ADJUDICATION
                                → ComputationMismatch  → MINER_FAULT   → miner penalty
                                → NoFaultFound         → CHALLENGER_FAULT → challenge bond slash
                                → Unadjudicable        → nobody slashed; catalog gap recorded
  refutation accepted after close: convicts, does not un-credit (ADR-0033 §3's tail metric)
```

`FINALIZED_WITHOUT_REPLAY` does not appear; there is no path from ACCEPTED to credit that
bypasses an attested replay.

## Stage mapping (the draft's §24 onto the accepted ladder)

| draft stage | lands as | note |
| --- | --- | --- |
| 0 Shadow Capability | ladder Stage 0 (ADR-0028 §6) + capability/matching shadow | family detect, band derivation, ready sets, capability tx, matching *logs* — joins the existing drill (`palw-shadow` submit/attest/watch/report); no value moves |
| 1 Non-slashing Replay Market | **rejected as a network stage** | consensus rewards without slash break §4b (attestation = liability): rubber-stamping would be strictly dominant. Market mechanics soak in the drill namespace (drill `network_id`, operator-side accounting) instead |
| 2 Bonded Replay | ladder Stage 1 → 2 preconditions | replay bonds, timeout offenses, reputation — objective offenses first (Stage 1), credit only at Stage 2 |
| 3 Challenge Game | ladder Stage 2 | refutation slashes WorkBond; requires `ArithmeticCatalogued` + `stage2_eligible` + the §4e leverage remedy |
| 4 Production Slashing | ladder Stage 3 | full economics, coverage enforcement, wider exposure; second reference implementation gate |

## Initial parameters (every number a placeholder; simulation- or registration-gated)

```
families = 4 (frozen)          bands = 5 (frozen)         active versions per family ≤ 2
CPU max active band = B1       q = 2 (crediting path)     κ = 3 (landed)
independent ready re-executors: devnet 3 / mainnet 5
capability TTL = 30 min        heartbeat = 5 min
band thresholds 1/2/4/8/16     BASE_REPLAY_WORK_UNITS = row-1 measured replay
replay/challenge bond floors, F_call, B_cap, multipliers: economic-simulation gate (B15
  discipline; the ADR-0028 §4e leverage remedy MUST be encoded before any Stage-2 credit)
```

## Required tests (the draft's §25, in this vocabulary — plus the routing red team)

The landed adversarial suite (`palw_adversarial.rs`, 10 attacks) gains routing attacks; each
line below is a test obligation, most against pure functions:

```
B4 receipt never panels a max-B3 verifier          family mismatch never panels (Cuda vs Metal)
family-version mismatch refused                    band match without ready proof refused
forged ready-binding Merkle proof refused          executor/self/same-domain excluded
expired capability ineligible                      bond-short capability ineligible
declared band ≠ registry band → receipt invalid    retired/frozen binding → receipt invalid
DA-unavailable ⇒ assignee unpunished, miner's offense
cross-family mismatch never slashes (diagnostic only)
false challenge slashes only after NoFaultFound    Unadjudicable slashes nobody
adjudication needs no model artifact               coverage drop walks ACTIVE→…→FROZEN
band re-derivation from registry fields is exact (no declared-band trust anywhere)
no path credits a receipt with zero attestations (FINALIZED_WITHOUT_REPLAY is untypable)
```

## What this ADR deliberately does not decide

* **Numbers.** Band coefficients, thresholds per band for bonds/rewards/audit rates, fee and
  bounty magnitudes — the economic-simulation gate owns them all.
* **CUDA/ROCm admission.** Reserved families stay empty until a backend passes the ADR-0027
  conformance campaign; per-generation class granularity inside those families is a
  measurement question, not a naming one.
* **RPC surface.** No PALW RPC exists today (the drill consumes generic block/acceptance
  RPC); the draft's `palw_*` method list and event names are the Stage-1 design input, to be
  fixed alongside the Stage-1 carriage release — events as store-derived notifications, not
  new consensus facts.
* **DA-layer artifact distribution** (auto-download of model artifacts to re-executors) —
  agent policy today; a protocol concern only if coverage economics prove it must be.
* **Mainnet.** Separate ADR after Stage 3, as always.

## Consequences

* **New objects to land, consensus-inert first:** `PalwExecutionFamilyV1` / `PalwModelBandV1`
  / `ModelDefinitionV1` / `PalwExecutionFamilyManifestV1` (carrying `max_active_band`) / the
  registration-row extension (`class_tag`-verified keys, band derivation, layout generation 2)
  in `validate()` / `PalwVerifierCapabilityV1` (+ carriage kind for capabilities at Stage 1)
  / the eligibility extension beside `select_replay_panel_v1` (new domain key — the
  assignment twin discipline — with the seat-dedup rules of §7) / the coverage state walk
  and its counting rule / the store set-rules (`binding_rows_coherent_v1`: no two rows share
  a `runtime_class_id`, or a receipt key-validates under one row and credits under the
  other's windows; `binding_matches_definition_v1`) / the `misaka-palw-reexecutor` agent
  surface. Each with goldens and domain-uniqueness tests against every existing PALW family
  and the VLT sortition key, like everything before it.
* **ADR-0028 §2 is amended** by §7: panel eligibility becomes binding-aware (ready proof
  required), and the escalation re-draw is added for panel lapse. Its §4 `q ≥ 2`, funding
  split, no-show pricing and permissionless window are unchanged.
* **ADR-0029's Stage-1 commitment kind** grows the three routing fields as a new body/id
  (version-trap rule); the capability object gets its own kind. Mass estimates must be
  re-run for both.
* **`PalwClassRegistrationV1` becomes the binding row** — the reader ADR-0033 promised gains
  a second consumer (routing) before its first (the credit gate) activates; both read the
  same rows, so the registry stays the single place where "what may this class do" lives.
* **The routing layer inherits the registry's honesty guarantees**: bands derived not
  declared, ceilings derived not declared, depth recorded not implied — the three lies
  `validate()` already refuses, now load-bearing for matchmaking too.
* The draft (Japanese, circulated as "ADR-0028 v0.1") is superseded by this document; its
  section numbers are preserved in the cross-references above where the mapping is not
  obvious.

## Mission alignment amendment — 2026-10-07

capability/model-band routingとoperator assignmentは実行供給の仕組みとして維持するが、そのroutingが外部bondの証拠アクセス・客観的訴追を排除してはならない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0034-palw-execution-class-model-band-routing.md -->

</details>

<a id="source-0056-palw-permissionless-class-admission-and-share-economy"></a>

<details>
<summary>0056-palw-permissionless-class-admission-and-share-economy.md — 原文 L1–L287</summary>

[原文を開く](../adr/0056-palw-permissionless-class-admission-and-share-economy.md) · SHA-256: `944c47dde0335bb41e2474aa9f89aba080b048483d41f87893fe8a8b627b52cd`

<!-- ADR_SOURCE_BEGIN:0056-palw-permissionless-class-admission-and-share-economy.md -->
~~~~~~~~markdown
# ADR-0056: Permissionless class admission, and the share economy that survives it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** Accepted and **implemented** (2026-08-27). Decisions 1, 2 and 7 were already the
shipped design; Decisions 3 and 5 landed with the state version 8 → 9 this ADR predicted, and
Decision 6 is a policy the gate keeps by not deduplicating. **Decision 4 is withdrawn** in favour
of ADR-0054 Decision 1, which answers the same question with a proportional step instead of a
streak — see that decision for the measurement that settled it.

**Renumbered** from 0054 at the merge: ADR-0054 and ADR-0055 were taken by two branches whose
documents were written earlier the same day.
**Date:** 2026-08-27
**Depends on:** ADR-0038 (PALW is the consensus work), ADR-0039 (the floor; no weight without a
complete catalog), ADR-0045 (the class economy: one share table, conservation, donation),
ADR-0049 Decision H (the post-genesis admission carriage), ADR-0053 (one execution family).

---

> **Amended (index reconciliation, 2026-09-02).** Three statements below are stale. (1) The
> constants table's `min_base_class_share_permille = 300‰` is [ADR-0068](0068-the-llm-primary-economy-and-the-floors-minimum.md)'s
> 20‰ since Relaunch 5. (2) Item 7's "the mid-epoch budget defect is also closed" is contradicted by
> [ADR-0053](0053-palw-one-execution-family.md) (the fix was reverted on `main`) and by the shipped
> transition: a class activating mid-epoch has no attempt-lane budget until the next boundary
> (`ensure_epoch_budgets`), a known gap kept for state-root compatibility. (3) Decision 5 still says
> "share is earned per Decision 4"; Decision 4 is withdrawn and the share walk is
> [ADR-0054](0054-palw-share-follows-production.md) Decision 1. Registration itself is gated further
> by [ADR-0069](0069-e2e-adjudicability-is-the-price-of-weight.md) (an uncertified family registers
> weightless) and [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) (a bond
> is earned). Map: [`README.md`](README.md).

## Context

testnet-11 registers three classes at genesis. The question this ADR answers is the next one:
**who may add the fourth, by what rule, and what stops a thousand of them?**

Two failure shapes bound the design space:

* **A permissioned registry** ("the foundation approves models") reintroduces the authority PALW
  exists to remove. The chain's whole verification story is that a claim is checked by
  *recomputation*, not by *reputation*; a model list curated by identity would be the one
  component whose correctness is somebody's word.
* **A free registry** is an attack surface. Every registration takes one permille of cadence from
  every incumbent (ADR-0045 Decision 3's donation), and share once granted currently has **no
  path back**: no decay, no reclamation, no growth either. A flooder with one bond could register
  hundreds of dead classes, dilute every working class toward the 1‰ grant floor, and the table
  would stay that way forever. Nothing about the flood is even dishonest — each registration
  passes every technical check.

The resolution is the repository's standing pattern: **make the gate arithmetic and the economics
self-limiting**, so that "who may register?" has the same answer as "who may produce a block?" —
anyone, at a price denominated in the same collateral as everything else, under rules every
validator recomputes independently.

### What already holds (measured, not planned)

The technical half of permissionless admission is **already the shipped design**:

1. **The gate is deterministic and total.** `verify_class_admission_v2`
   (`consensus/core/src/palw_class_admission_v2.rs`) checks, from the carried material alone:
   the profile validates; `class_id == shape_profile_id` (identity is *derived*, never chosen);
   every reachable kernel is in the court catalog (A4); the logits commitment scheme is one this
   build adjudicates; the worst-case step space fits the ladder; the three court-cost ceilings
   hold under `derive_court_cost_v1` — the same function the genesis mint uses (one derivation,
   two doors); the canonical job fits the registered context in the enumeration's own form; and
   the declared `pwu_per_inference` equals the **counted** step-leaf count, so a registration
   cannot multiply its own weight by self-report.
2. **The object carries its own evidence.** ADR-0049 Decision H: a post-genesis `ClassRegistered`
   rides with `PalwClassAdmissionCarriageV2` — the full shape profile and the canonical job —
   because a running chain has no catalog to look the entrant up in. Genesis registrations are
   checked by `verify_palw_genesis_v2` against the catalog the ruleset id commits to instead.
3. **The registrant is a bond, and signs.** The acceptance layer
   (`virtual_processor/processor.rs`, the `ClassRegistered` arm) requires an **Active** bond and
   an ML-DSA-87 signature over `palw_class_registration_message_v2(network, class_id, share,
   activation_daa, bond)`. Not a permission system: the smallest answer to "who", denominated in
   the collateral every other authority on this chain is denominated in.
4. **The entrant's share is pinned.** A post-genesis entrant joins at exactly
   `min_grantable_share_permille()` (1‰) — a registrant that could name its own permille would be
   donating itself a slice of every incumbent's cadence.
5. **Donation conserves and floors.** `granted_share_table_v2`: incumbents are scaled by largest
   remainder (total order on ties, so two nodes cannot disagree about a residue permille),
   Σ = 1000‰ is a construction, and a donation that would push any incumbent below the 1‰ grant
   floor is refused (`DonationBreaksGrantFloor`).
6. **Activation is a soak, and a clock.** `activation_daa` registers the class *weightless*
   (`PalwClassStatusV2::Registered`): in the registry, in the catalog, fully adjudicable, holding
   no share, its attempts refused — until the named DAA score, when the transition grants the
   pending share. The flip is a clock, not an object: nobody submits it, so there is nothing to
   forge.
7. **Idle share does not strangle producers.** `derive_epoch_budgets_v2` uses the *census*
   denominator (ADR-0045 Decision 2): a class's budget is its share over the **competing** set of
   the closed epoch, so a table full of idle entrants slows nobody who is actually producing.
   (This closed defect H1; the mid-epoch re-derivation defect is also closed — budgets re-derive
   whenever the share table grows under them.)
8. **Freezing is evidence, not opinion.** A class leaves by `ClassFrozen` only with a structural
   contradiction certificate, and the floor may never be frozen (ADR-0039 W6′).

What does **not** exist today — and what this ADR decides — is the economics around that gate:
registration has no cost beyond a transaction fee, share has no path up, no path down, and no
reclamation. Decisions 3–6 add exactly those, and nothing else.

---

## Decision 1 — The constitution: admission is arithmetic, and only arithmetic

**Implemented.** A class joins this network if and only if a `ClassRegistered` object passes the
deterministic gate above. There is no allowlist, no vote, no identity check, and none may be
added to this path. "The MISAKA foundation approved it" is not a condition; "every validator
independently recomputed the same admission facts" is the only one.

Two corollaries, stated so they cannot be un-stated quietly:

* **No BFT vote on models.** Validators do not vote on a registration; they *validate* it, the
  way they validate any transaction. Agreement is a consequence of determinism, not of a quorum.
* **Governance, if it ever exists, is a separate layer with a smaller job.** A community that
  wants to *refuse* a technically admissible model may only do so the way it changes any other
  consensus rule: a ruleset change, visible in the fingerprint, adopted by upgrade. This ADR
  reserves no hook for it inside the gate, deliberately — a hook is a permission system with
  extra steps.

## Decision 2 — The kernel boundary: what needs a binary, and what does not

**Implemented.** A model whose graph reaches only catalogued kernels, commits a supported logits
scheme, and fits the ladder and the cost ceilings **registers without any software change** —
weights, profile, carriage, signature, done. A model that needs a new kernel, a new commitment
scheme, or a deeper ladder **cannot** be admitted by any transaction: the gate refuses it by
name, because a court that cannot re-execute a step must never accept work committed under it.

The upgrade path for that case is the one this repository has already walked three times (the
Qwen3.6 descriptors, the A16 rope/mul-elem pair, the tile floor): grow the court catalog in the
validator binary, which **moves the ruleset id**, which is a coordinated network upgrade visible
at the handshake — and *then* the model registers through the unchanged gate. Deployment and
consensus are different events; conflating them ("everyone updated, so the model is adopted") is
refused by construction, because the registration object still has to ride a block and pass the
gate afterwards.

## Decision 3 — Registration exposure: entry is priced in bonded collateral

**Implemented.** The transition's `ClassRegistered` arm reserves; `ClassFrozen` and reclamation
release; `verify_admission_v2`'s ceiling reads the sum of both ledgers.

A registration reserves **`REGISTRATION_EXPOSURE_SOMPI`** against the registrant's bond for as
long as the class it registered is `Registered` or `Active`, released when the class is
reclaimed (Decision 5) or frozen. The reservation rides the same exposure accounting that prices
a claim's bind window, and the same ceiling (`max_exposure_ratio_permille`) bounds it — so a bond
can hold at most `⌊collateral · ratio / REGISTRATION_EXPOSURE⌋` live registrations *minus* what
its claims are using, and flooding the registry competes with producing blocks for the same
capital.

The constant is a bundle parameter (inside the ruleset id), sized like the claim exposure is
sized: high enough that a thousand dead classes cost a thousand bonds' worth of idle collateral,
low enough that one honest model costs one operator nothing they don't already hold. It is a
**reservation, not a burn**: honest registrants get it back at retirement, so the price of entry
is the *time value* of collateral, and the price of spam is that time value multiplied by a
number the spammer chose.

*Why not a burn?* A burn punishes the honest entrant exactly as much as the flooder and creates a
money sink whose size is a policy knob. A reservation punishes only *holding many dead classes
simultaneously*, which is precisely the attack, and it composes with Decision 5: reclamation
frees the spammer's collateral only by also freeing the share they squatted.

## Decision 4 — Withdrawn: the share walk is ADR-0054's, not this one's

連続 epoch の streak で一 permille ずつ増やす旧 share 調整は採用しない。minimum-share class の budget では必要な変化を観測できず、実測で target が動かなかったため。[ADR-0054](0054-palw-share-follows-production.md) の比例調整を採用する。Decision 3 の登録 exposure と Decision 5 の reclamation は維持する。

## Decision 5 — Reclamation: dead classes give the network back

**Implemented** as `apply_class_reclamation`, in transition slot 2d — at the same boundary and off
the same closed epoch's census as ADR-0054's growth step, and immediately after it, because a class
the growth rule just decayed and a class that has been silent for `reclaim_epochs` are different
facts and only the second takes a class out of the table. The status flip is written exactly as
`activate_due_classes` writes activation — a clock, not an object.

A class that produced **zero** blocks for `RECLAIM_EPOCHS` consecutive epochs transitions
`Active → Dormant`:

* its share returns to the incumbents by the inverse of the registration donation (largest
  remainder, same total order);
* its registrant's `REGISTRATION_EXPOSURE` is released (Decision 3);
* it **stays in the registry and the catalog** — Dormant is not Frozen: its past claims remain
  adjudicable, disputes against them still run, and its artifact root still names it;
* admission refuses its attempts, exactly as it refuses a `Registered`-not-yet-active class.

A Dormant class returns by a fresh `ClassRegistered` for the same `class_id` — the one case where
`DuplicateClass` does not refuse — carrying a fresh signature, a fresh exposure reservation, a
fresh activation soak, and entering again at the 1‰ floor. Nothing about its history transfers:
share is earned per Decision 4, from the bottom, both times.

Together with Decision 3 this closes the flood: a spammer's collateral stays locked exactly as
long as the squatted permille stays out of the table, and both revert on the same clock. The
attack's steady state is *paying rent on nothing*.

## Decision 6 — Duplicates are priced, not policed

**Implemented by absence, and now decided.** The same
`artifact_root` under a different profile is a **different class** — legitimately: a geometry
upgrade (a wider context after a court-format improvement, a different tile budget) is exactly a
new profile over the same weights, and this repository has already done it twice. The gate does
not deduplicate by weights, and must not: it cannot distinguish an upgrade from a copy, and
trying would make the artifact root a lock instead of a name.

What bounds copy-spam is Decisions 3–5: every copy reserves its own exposure, enters at 1‰,
must *produce* to keep it, and is reclaimed when it does not. A hundred copies of a working model
are a hundred rents; the table converges on the copies that someone actually runs — which is the
only definition of "the real one" a permissionless network can afford.

## Decision 7 — What the chain does not judge

**Implemented, by absence — kept deliberately.** The gate checks *adjudicability*, never
*quality*. A model that passes every check and generates noise is admissible; a model that would
win benchmarks but reaches one uncatalogued kernel is not. Text quality is the operators' market
(they choose what to run and what to pay for); the chain's only promise is that whatever runs
can be re-executed, disputed, and slashed. Confusing the two would put a benchmark inside
consensus, and a benchmark is an oracle.

---

## The attack table

| Attack | What stops it | Decision |
|---|---|---|
| Register hundreds of dead classes to dilute incumbents | Each reserves bonded exposure while alive; zero production reclaims share and only then frees the collateral — steady state is rent on nothing | 3 + 5 |
| Name your own share at entry | Acceptance pins entrants to the 1‰ grant floor | (shipped) |
| Grow share by fake demand | The only growth signal is filled budget, which costs full production price to fake; one bounded step per epoch | ADR-0054 D1 |
| Squat share without producing | Decay to the floor, then reclamation to Dormant, by clock | ADR-0054 D1 + 5 |
| Register someone else's class id | `class_id == shape_profile_id`, derived not declared | (shipped) |
| Overstate `pwu_per_inference` to inflate weight | Declared value must equal the counted step-leaf count | (shipped) |
| Register a class the court cannot try | Coverage, scheme, ladder and cost gates refuse by name | (shipped) |
| Sneak a new kernel in by transaction | Impossible by construction: the catalog is in the binary, growing it moves the ruleset id | 2 |
| Freeze a competitor | `ClassFrozen` needs a structural contradiction certificate; the floor is unfreezable | (shipped) |
| Dilute the floor below liveness | `min_base_class_share_permille` bounds every donation path | 3 + ADR-0054 D2 |
| Flood-register to slow incumbents' *budgets* | Census denominator: idle share is excluded from competing budgets | (shipped, H1) |
| Re-register a reclaimed class to reset its slate | Allowed — and it re-enters at 1‰ with fresh exposure and a fresh soak; there is no slate to reset | 5 |

## Consequences

* Adding a fourth model to testnet-11 is, today, a `ClassRegistered` transaction from any active
  bond — the gate is live end to end (`the_a16_dense_class_passes_the_admission_gate` and the
  acceptance arm are its tests). What ships with Decisions 3–5 is the guarantee that the
  *thousandth* registration is as harmless as the fourth.
* **The constants this network chose, and why each is derived rather than picked:**

  | Parameter | RC value | Where it comes from |
  |---|---|---|
  | `REGISTRATION_EXPOSURE_SOMPI` | 40,000 | A minimum bond's ceiling is `400,000 × 500‰ = 200,000`, so a smallest-possible bond holds **five** live registrations and no claims. A hundred dead classes needs twenty minimum bonds, idle. |
  | `RECLAIM_EPOCHS` | 12 | Reclamation takes the WHOLE share, so it is deliberately far slower than ADR-0054's step. One block in twelve epochs is not what this rule is about. |
  | `min_base_class_share_permille` | 300‰ | Leaves 700‰ for a busy registry while keeping the class every node can run at nearly a third of the cadence. One field, checked by growth, decay and grants alike — see Decision 4. |

* Two bundle parameters and state version 8 → 9 entered the ruleset id together; landing them was
  a re-mint, like every ruleset move on this line. The shipped testnet-11 value is re-derived over
  the whole merged ruleset rather than carried from any one branch, so no number from this ADR's
  drafting survives as a pin.
* **What the implementation added that the ADR text did not anticipate:** a class with NO budget
  for the closed epoch is not measured at all — it could not have filled a ceiling nobody gave it
  (the mid-epoch activation case), and counting that as a decay would punish a class for the
  boundary's own arithmetic.
* The share table becomes a slow, legible instrument: one bounded step per class per epoch
  boundary, every move derivable by every observer from on-chain state alone.
* Nothing in this ADR adds an authority. The list of things that can move a permille after it
  lands: a signed registration (in), the epoch clock (up, down, back). Every one is validated by
  recomputation.

## Mission alignment amendment — 2026-10-07

permissionless registrationとreward-bearing admissionを区別する。誰でも登録できることを、公開に訴追できないclaimがweightを得てよい根拠にしない。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

permissionlessなclass登録と有用計算の資格に、class非依存のproducer bond/DAA予算を追加する。大きなモデル、追加class、PWUやtoken数の申告で採掘総上限を増やさない。
既存class share/発行予算・検証admissionは別の制限として保持し、その不足をbond枠の加算で補わない。登録/分割と同額資本の総採掘機会を混同しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0056-palw-permissionless-class-admission-and-share-economy.md -->

</details>

<a id="source-0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard"></a>

<details>
<summary>0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md — 原文 L1–L425</summary>

[原文を開く](../adr/0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md) · SHA-256: `4bcc6c87a8576ad37219c66efa0c19edb92a2ef55c04e09dd208a3f31f277af3`

<!-- ADR_SOURCE_BEGIN:0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md -->
~~~~~~~~markdown
# ADR-0100 — A model is data: the one-move court, the held measurement and the licence per shard, and the boundary "permissionless" means

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-10. **Decisions 1, 2, 3, 4 and 6 IMPLEMENTED the same day,
  consensus-inert on every shipped preset** (§9): the one-move court is a consensus object behind
  the fence ADR-0099 declared, `None` everywhere, and the fence arms only over a genesis that
  commits to the court's signing context; `palw-class measure` and `verify` exist and were run on
  the real A16 artifact; licensing per shard is a consensus object behind its own fence
  (`Params::palw_shard_licensing`), with no state-version move (Decision 4, amended the same day).
  **Decisions 5, 7 and 8 are stated, not built**, each with the reason. Every number below is the generator's or
  the tool's, recorded in `docs/palw-shard-plan-2026-09-10.md` and §1.3.
* Builds on: [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
  (Decision 5's fence and "the four"; §6 steps 2–4; Decision 4 named and not built),
  [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (a seat that found a lie files nothing else — now it files the one thing; licensing fits one
  transaction up to eight shards), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  (classes are chain data, kernels are the build; Decision 5's fenced interpreter and SA-4's
  "an unpaid panel is the cheapest thing on the chain to buy"; Decision 6's tiers and
  "registration and possession are different acts"), [0062](0062-data-availability-court.md)
  SA-1 (who may accuse, and at what price), [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)
  / [0095](0095-a-position-is-a-membership-not-an-income.md) (a position is bought from the curve
  and sold back to it, transfers nothing and pays nobody), [0056](0056-palw-permissionless-class-admission-and-share-economy.md)
  (no allowlist, no vote, no identity at admission), [0075](0075-certification-is-a-consensus-object.md)
  (anyone certifies; the court grades), [0049](0049-palw-adjudication-contract.md)
  Decision G (the inventory is the layout an `artifact_root` commits to).
* Amends: ADR-0099 Decision 5 (built: the refusal "not a consensus object" is replaced by the
  ruleset-move condition), ADR-0099 §6 step 3 (`palw-class measure`, built), ADR-0099 Decision 4
  (licensing per shard: designed here, pure half built). Supersedes nothing.

## 0. The sentence this ADR is

**A model is data.** The two reviews this ADR answers (2026-09-10) said the same thing from two
sides: the chain's registration, certification, admission, draw, court and market are already
permissionless — nobody's permission is involved anywhere a validator can recompute the answer —
and what is NOT permissionless is that a new model FAMILY is still Rust: a profile builder, a
converter, an engine, a lineage, and a merge. This ADR does not remove that boundary; it states
it precisely, keeps every mechanism the reviews found sound, and builds three things ADR-0099 left
as words: the court a shard seat can open (a consensus object now), the measurement a holder of
an artifact takes (a tool now, run on the real A16 artifact and reproducing its genesis class id
and registered root), and the licence a panel of shard seats produces (a per-shard object and
quorum now, a fold next).

The definition this repository commits to:

> **Any user may introduce any model, without a maintainer's approval or a node software
> upgrade, provided the model can be deterministically lowered into the PALW model IR and kernel
> semantics the build already carries, and its artifact, class profile, work and execution can be
> independently verified.** The MODEL is permissionless; the EXECUTION SEMANTICS are a
> deterministic protocol; a new primitive is a protocol upgrade, never a registration.

And the settled boundary the operator restated with it: **a Position is a membership and never
money.** It is bought from the curve and sold back to it, it is not transferable between holders,
and it is never a means of payment or settlement — ADR-0087's rule, kept as it is and named here
as a standing invariant of this boundary (Decision 8).

## 1. What was measured

### 1.1 The one-move court is a consensus object

The four things `Params::palw_shard_court`'s refusal named (ADR-0099 §3 Decision 5) exist:

| the four | where |
|---|---|
| the object | `PalwConsensusObjectV2::ShardCourtAccused { accusation }` — appended last (the discriminant is positional; a rebase onto `main`'s enum fix keeps it last) |
| the acceptance rule | the virtual processor's arm: the FENCE, the accuser's bond key over the session id, the shape, the ruleset's close ceiling, the claim's executor and roots, the class, and the verdict DERIVED at the court's ladder — a fused site refused, an unadjudicable refutation refused |
| the fold | `palw_state_v2`'s arm: the fence again (the ladder rides in `PalwTransitionExtrasV1::shard_court_ladder`), the claim live, the accuser Active and above the floor and not the producer, one court at a time on a claim, the verdict derived again and applied — `CourtFraud` voids the claim and slashes `claim.reserved`; a refutation that proves none charges the accuser `min(reserved, floor)` |
| the signing context | `misaka-palw/shard-court/accuse/mldsa87/v1`, in `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3` — V2 in V2's order, then this one — derived from the family's own domain list by the registry's test |
| who files one | the seat, from its own capture check: `fp_capture_samples_clear` now returns the refutation and the openings it built, and a seat that found a leaf that does not recompute files exactly that, signed under its bond key, once per claim, when the fence is in force; and `misaka palw shard-accuse` signs an accusation built elsewhere |

What the fence still refuses is ARMING over a bundle whose committed context set is not V3: a
network that wants the court states V3 at genesis through `palw_signature_contexts_v2`, and no
shipped preset does (§5 Invariant 8). No state record changed, so `PALW_STATE_V2_VERSION` did
not move.

### 1.2 The plan reads the inventory, and a shard's rows are the inventory's

ADR-0099's plan weighed layers by a formula. The inventory (ADR-0049 Decision G) is the layout
the class's root commits to, one row per tensor slice with its layer, so a HELD artifact measures
itself: every row lands by its layer, a graph-level row by the node tables that name it (the
embedding's on the first shard, the logits' on the last, a tied head's on both ends, a tensor a
layer node names without a placeholder on every shard, a `<tensor>.<derivation>` row — the A16
quantisation parameters — with its tensor), and a row nothing names is refused by name. The
plan's per-seat bytes and the rows' own bytes close a round trip (§5 Invariant 3), and a shard
is named by the class's `artifact_root` and a set of leaf indices under it — no new identity.

### 1.3 The held measurement, on the real artifact

`palw-class measure --network testnet-11 qwen25-1.5b-a16.palwart` (this machine, 2026-09-10; the
shipped A16 artifact, 1,795,427,276 bytes — the size `docs/model-requests.md` records):

| what | value |
|---|---|
| the geometry read off the file | 28 layers, the dense A16 lineage, the artifact's own `eps_q` carried into the manifest |
| the class at 512 | `4277d84f7d91…` — **the shipped `Qwen/Qwen2.5-1.5B/graph-v5@512` class, genesis-registered on testnet-11** |
| the inventory root | `1a7457f100d9…` — **the root the graph-v2, -v3 and -v5 rows register** |
| bytes measured from the inventory | 1,826,522,928 (the rows the commitment covers, a tied head's both views among them) |
| the file on disk | 1,795,427,276 |
| the family formula (ADR-0099) | 1,776,943,104 — a floor, as it said |
| fit | admitted at 512; refused at 32,768 and 131,072 by the ladder, the court window, the state chunks and the PublicDa payload; no profile at 1M |
| a self-reported 34 ms a position on this Mac | 2,117,400 positions inside the RC's receipt window, listed as self-reported |
| signed under a throwaway ML-DSA-87 seed | `verify --artifact` recomputes every field and the signature verifies; one byte added to the artifact's bytes → `artifact_bytes` MISMATCH and the signature does not verify, exit 1 |
| `verify` WITHOUT the artifact | the eight artifact-derived fields (bytes, basis, root, file size, four contexts' plans) are named as needing the artifact; everything else recomputed; exit 0 |
| the hybrid-container `qwen35-2b.palwq36` on this Mac (the Qwen3.5-2B rehearsal artifact) | **refused by name**: the inventory is built under the graph-v5 profile its manifest projects, and this artifact's embedding-lift table (`embed_lift.a16: 248320 triples serve neither one lane nor 2048`) is not a layout that inventory accepts — a measurement that cannot place a row refuses rather than estimates. **Answered by [ADR-0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)**: graph-v6 reads the lift per token, and the artifact measures (§1.4 there) |

### 1.4 Licensing per shard, priced

One part per shard at the shipped quorum is **14,393 bytes** (three receipts of 4,772, the claim,
two shard fields and the tags); it fits one standard transaction at every shard count, where the
whole-object form stops at eight (114,597 bytes; 128,913 at nine, ADR-0098 §1.2). A K3-class
claim at 23 shards is 23 carriers of 14,393 bytes — 331 KB of receipts across as many blocks as
it takes, against a block that cannot carry them together — which is why the progress a claim
keeps is a bitmap and the fold licenses on the last part (Decision 4).

## 2. The requirement

> **R-data — a model is data and its execution is protocol.** Anyone adds a model the build can
> lower; nobody adds a kernel by registering one. A seat holding a shard can convict; a holder of
> an artifact can measure it in a form any other holder recomputes and any node can check the
> recomputable half of; a panel of shard seats can license a claim in parts that each fit a
> carrier; and a Position stays a membership — never transferred, never a payment.

## 3. Decisions

**Decision 1 — the one-move court is built, and arming it is the ruleset move (ADR-0099 Decision
5, finished).** `ShardCourtAccused` rides signed and bounded by the close ceiling; the acceptance
arm and the fold both derive the verdict at the court's ladder (the fold's ladder rides in the
block's extras, so a node cannot admit what its fold refuses); the accuser is any Active bond at
or above the floor that is not the claim's own — priced, not privileged (ADR-0062 SA-1); a false
accusation costs `min(claim.reserved, floor)`; a fused site is refused, not tried (ADR-0093 is
still what a fused terminal needs). The shard fields left the object: the chain holds no plan,
and a leaf either recomputes or it does not, whoever names it; the seat's own rule — name only a
leaf your shard replayed — is a function beside the object. The signing context is in the V3
set, and `validate_palw_v2` refuses `palw_shard_court` over any bundle that does not commit to V3.

**Decision 2 — the inventory measures the artifact, and a shard's rows are the inventory's.**
`palw_artifact_bytes_from_inventory_v1` replaces the family formula for a held artifact (basis
"the artifact inventory's rows, byte for byte"); `palw_shard_inventory_rows_v1` is the shard
manifest — per shard, the inventory indices a seat holds and their bytes — derived, and equal to
the plan's own figure. `PalwArtifactBytesV1` gains `shared` (every shard's) and `ends` (both
ends') for the two graph-level classes an inventory can carry and a formula never does.

**Decision 3 — `palw-class measure` and `verify`.** The SDK's binary reads the geometry off the
artifact (the `eps_q` it carries now travels in the manifest, so the class named is the class the
artifact is), builds the inventory under the manifest's own profile, measures it, evaluates the
walls and the plans on the named network, writes the Measured Model Artifact, and signs its id
under the bond key when given one (`misaka-palw/measured-model/mldsa87/v1` — the convention's
name; NOT in any committed set, because no consensus rule verifies it: a registration is verified
by recomputation, ADR-0099 Decision 6). `verify` recomputes a document field by field; without
the artifact it names the artifact's fields as needing one rather than comparing them with a
formula (`PalwMeasuredCheckV1::NeedsTheArtifact`); it checks the signature and exits 1 on a
mismatch or a signature that does not verify.

**Decision 4 — licensing per shard, built, and it needs no flag day** (amended the same day; the
first text said the fold was a state-version flag day, and the precedent below says otherwise).
The pure half: `PalwShardReceiptPartV1 { claim, shard_count, shard_index, receipts }` is one
shard's licensing; `palw_shard_quorum_v1` is that shard's quorum over verified verdicts (its own
seats, once each, `Incapable` for neither side, a majority quorum so the two outcomes are
disjoint); `PalwShardLicensingProgressV1` is the bitmap a claim keeps. The consensus half:

* **Who declares the count: the class's registrant, once** (`ClassShardPlanDeclared`, signed
  under `misaka-palw/shard-plan/mldsa87/v1`). Not the line's owner, as this Decision first said:
  the panel is the class's, and a class carries many lines. A genesis class has no registrant and
  is never sharded; a plan is immutable, like the class's graph.
* **Who holds a shard: a bond, as a refinement of a class it declared** (`BondShardsDeclared`,
  signed under `misaka-palw/bond-shards/mldsa87/v1`; an empty list withdraws). **This corrects
  ADR-0099 Decision 3**, which said a shard capability id rides `capable_classes`: the fold
  refuses every entry of that set that is not a registered class id (`MissingClass`), so a derived
  id could never have been declared — and keeping the class entry is what keeps SA-1's bound,
  SA-2's exposure and SA-3's production proof on a shard seat.
* **The draw.** For a claim whose class has a plan — the fence read at the claim's ANCHOR, as
  ADR-0065 D1 and ADR-0071 SA-3 are, through one processor helper the binding and its validation
  share — the chain draws `derive_stratified_panel_v2`: the flat draw's eligibility
  (`palw_panel_eligible_bonds_v2`, now one spelling for both draws), each bond carrying its shards
  of the class, `derive_shard_panel_v1` per shard, stored flat and shard-major. A shard nobody
  holds refuses the draw by name and the claim voids at `BindTimeout`; a sharded class never falls
  back to a flat panel, which would ask shard seats to judge a whole model.
* **The licence.** A claim licenses by parts exactly when its bound panel has the stratified shape
  — `shard_count × seat_count` seats, told from a flat panel by its length alone, so a claim bound
  before the declaration keeps the whole licence it was drawn for (`palw_claim_licenses_by_parts_v1`).
  `ShardReceiptLicensed` carries one shard's receipts. At acceptance they meet the whole licence's
  own checks over that shard's seats (`receipt_quorum_over_seats_v2`, one spelling for both
  doors). In the fold: that shard's dissenting seats are charged as the whole licence charges
  them; the claim licenses in the block that lands its last shard (`license_claim`, one spelling
  for both licences); a shard whose quorum says Unavailable voids the claim where ADR-0065 D4 has
  not retired that door. `ReceiptLicensed` and `ProducerDefaulted` are refused for a claim that
  licenses by parts, and the collector assembles the next ready part.
* **No flag day.** The three new collections — plans, bonds' shards, per-claim progress — enter
  the state root in their own guarded block and the state carriage behind tail `0x99` only once
  written, which is the ADR-0087 / ADR-0088 / ADR-0095 precedent, and nothing can be written below
  `Params::palw_shard_licensing`. `PALW_STATE_V2_VERSION` stays 20, the golden vectors stay, and a
  dormant network roots byte-identically; the claim record gains no field (the progress lives in
  its own collection, dropped by `write_claim` on every way out of `PanelBound`). Arming is an
  ordinary scheduled activation, which `validate_palw_v2` refuses over a bundle that does not
  commit to the V3 contexts (the two new ones are in it) and without the one-move court armed at
  or below the same height — a panel of shard seats must be able to convict with what a shard
  seat holds (ADR-0098 Decision 5).

**Decision 5 — the economics come before the arming.** The stratified panel multiplies ADR-0067
SA-4's problem — a registrant funding a quorum of judges for the price of a few bonds — by the
shard count, and the one-move court is the remedy, not the cure: any bonded party can convict a
lie a bought panel licensed, but a bought panel still licences it first. `chain_classes` stays
sealed until seats are paid (SA-4) and ADR-0067 Decision 5's fuzz gate has run; a sharded class
becomes weight-bearing under the same two conditions and no earlier. Stated, so the order is not
reversed.

**Decision 6 — the boundary "permissionless" means, fixed.** Kept as the reviews found them:
admission without allowlist, vote or identity (ADR-0056); certification by anyone (ADR-0075);
the attempt drawn by the chain (ADR-0074); the court and the DA court open to any bonded party at
a price (ADR-0027, ADR-0062); a line founded by anyone, its versions its developer's, its roles
its owner's — ownership of a line is not permission over the chain (ADR-0088); a Position seeded,
bought and sold by anyone (ADR-0087, ADR-0094); the reward buying the pair with nobody's hand on
it (ADR-0091); a node syncing and verifying without a bond, production needing one (ADR-0061);
private prompts served to the claim's readers and to nobody else (the private-prompts design).
What is added: the definition in §0, and — for the membership half of the boundary — ADR-0101:
**the line controls the PRODUCT, providers control the SERVING, the chain proves the MEMBERSHIP**,
so a benefit a line declares (ADR-0095) is served by any provider a client can check, not only
by the line's origin.

**Decision 7 — what "a model is data" still needs, in order (the first review's list, adopted).**
1. A model descriptor and IR: the manifest of ADR-0099 Decision 1 is the descriptor's first form;
   the IR is the graph the profile already is, with a tensor-mapping spec beside it so a converter
   is data too. 2. A source adapter that fetches an immutable revision and parses DATA ONLY —
   never runs a repository's code; the worker never touches the repository. 3. A generic tensor
   mapping and artifact builder. 4. **The positive control: the two shipped families through the
   generic pipeline, reproducing their class ids, artifact roots and execution facts** — the day
   that passes, "new family = new Rust" ends for every architecture the kernel vocabulary covers.
5. `resolve_chain_registered` promoted to the ordinary path (after Decision 5). 6. An out-of-band
   content-addressed artifact store and resolver (local, repository, mirror, peer), fetching only
   what an executor chose — registration still carries kilobytes and no URL (ADR-0067 Decision
   6). 7. The shard manifest over the inventory (Decision 2) feeding a sharded execution backend
   behind `PalwExecutionBackendV1`, with placement an executor-side act and no scheduler on the
   chain (ADR-0074). 8. Seat advertisements as host facts, never chain truth (work stays in leaves
   and pwu). 9. A provider-side service descriptor — line, benefits, endpoint, model roots,
   expiry, provider key, signed — carried by a directory the chain does not hold. 10. `misaka
   model add` from a repository revision to a registered, certified, seeded line, over the
   primitives that exist.

**Decision 8 — a Position is a membership and never money.** Settled by the operator on
2026-09-10 and decided, with its three pins, as ADR-0101 Decision 5: bought from the curve, sold
back to it, transferable to nobody, never a means of payment or settlement — and nothing "a model
is data" adds (a provider, a directory, a sharded seat) receives, escrows or is paid in one.

## 4. What this costs

* **Chain:** nothing on any shipped preset — the object is refused wholesale while the fence is
  `None`, the fence is `None` everywhere, and the fingerprints are byte-identical. A network that
  arms it states V3 at genesis (a different `signature_contexts_root`, therefore a different
  ruleset id — the move ADR-0099 named).
* **Node:** one acceptance arm, one fold arm, one field on the extras, one module of pure
  functions for the licence; the seat keeps what it already computed.
* **An adder:** `palw-class measure` over the converted artifact, on the network's name, with the
  bond key — one command, one document.
* **A seat holding a shard:** its shard's inventory rows (Decision 2), and one carrier when it
  finds a lie.

## 5. Invariants the tests hold

```
1  The registry's committed set is derived from the families, V2 is the frozen nine's prefix,
   V3 is V2's prefix plus the one-move court's context, three sets have three roots, and the
   frozen root has not moved.
2  An inventory measures its artifact: every row lands by its layer or by the node that names
   it (a derived row with its tensor), a row nothing names is refused by name, and the total is
   the rows' own.
3  A shard's rows are the inventory's, every row on exactly the shards that hold it, and a
   shard's measured bytes are the plan's figure for it; a tied head lands on both ends and a
   shared tensor on every shard, counted once by a seat holding the whole.
4  An accusation's session id binds the network domain and every field but the signature; the
   shape rules refuse by name; the filer's rule names another shard's leaf.
5  A part fits one standard transaction at every shard count; the whole form does not at nine.
6  A shard's quorum counts only its own seats, once each, Incapable for neither side, and
   refuses a non-majority quorum.
7  The progress completes on the last shard, ignores a repeat, and names a foreign part.
8  The shard court fence is dormant on every preset, refused over the frozen set and over V2 by
   name, and assembles over a genesis that states V3.
9  A manifest of a shipped class names the shipped class (ADR-0099), and the held measurement
   of the real A16 artifact names the genesis class and its registered root (§1.3, run by hand
   — the artifact is not in the tree).
10 A document measured from an inventory verifies with the artifact, is refused by the tampered
   field with it, and without it names the artifact's fields as needing one.
11 A sharded claim licenses on its last part, holds a progress row only in between, and each
   part reverts and re-applies exactly.
12 The whole-object licence is refused for a claim drawn per shard and kept for one drawn flat;
   a part is refused for a flat claim.
13 A part is refused by name: dormant, another plan, a shard out of range, a seat of another
   shard, short of its quorum, already licensed; a shard whose quorum says Unavailable voids the
   claim, and past ADR-0065 D4 cannot.
14 A plan is declared once, by a class with a registrant, with a count in range; a bond's shards
   refine a class it declared, against the plan's count, and an empty list withdraws; a redraw
   drops the progress.
15 The stratified draw seats each shard from the bonds that hold it, one operator a shard; the
   binding validator recomputes it only when told the claim is sharded; a short shard refuses the
   draw by name; a part is validated over its own shard's seats and the whole door is shut.
16 The sharding collections enter the root and the carriage only once written; the licensing
   fence is dormant everywhere and arms only over V3 with the court no later; the new delta
   entries (36–38) and objects (39–41) keep their Borsh tags.
```

1 is `palw_mode_v2`'s tests; 2–3 `palw_shard_plan_v1`'s; 4 and 10
`consensus/core/tests/palw_adr0099_shard_plan.rs`; 5–7 `palw_shard_licensing_v1`'s; 8
`config::params`'s `the_shard_court_arms_only_over_a_bundle_that_commits_to_its_signing_context`;
11–14 and the root half of 16 `palw_state_v2`'s; 15 `palw_panel_v2`'s; the fence half of 16
`config::params`'s `the_shard_licensing_fence_is_dormant_and_arms_only_over_v3_with_the_court`, the
tags the two discriminant pins.

## 6. Order of work

1. Decisions 1, 2, 3 and 4 — **done** (§9); Decision 4's consensus half without a flag day.
2. A devnet drill of the one-move court: a genesis stating V3, a seat that files, a fold that
   convicts — the U-04 of ADR-0099, now with an object to file. **Done 2026-09-10** (§9, "the
   drill"): `scripts/drills/misaka-palw-shard-court-devnet-drill.sh`, PASS on the second run.
3. A devnet drill of per-shard licensing: a class registered by a bond, a plan, bonds declaring
   shards, a stratified panel, parts landing — the seats holding the whole model until a sharded
   backend exists (Decision 7 step 7).
4. Decision 5's conditions: paid seats, the fuzz gate.
5. Decision 7, in its order; step 4 (the positive control) before any of 5–10.
6. The successor mint for a K3-class network (ADR-0097 Decision 5), priced by the tools.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0099 Decision 5 (the court declared, refused at assembly) | built; the refusal is now the ruleset-move condition (V3) |
| ADR-0099 Decision 4 (licensing past eight shards, named) | the part, the quorum and the progress built; the fold and the state version move stated as the next step |
| ADR-0099 §6 step 3 (`palw-class measure`) | built, with `verify`, run on the real artifact |
| ADR-0099 Decision 2 (the plan, derived from the formula) | the formula stays the estimate for a manifest; a held artifact measures itself from the inventory |
| ADR-0098 Decision 2 (a seat that found a lie files nothing else) | it files the one thing — the accusation — and nothing else |
| ADR-0087 / ADR-0095 (a Position is bought from the curve; a membership, not an income) | kept as they are; named as the standing invariant of Decision 8 |
| ADR-0067 SA-4 / Decision 5 (arming stays off; the fuzz gate) | kept; Decision 5 here says a sharded class inherits both conditions |

## 8. What is deliberately not decided

* **The provider directory's transport** (Decision 7 step 9): a signed descriptor, carried by
  something the chain does not hold.
* **A third family.** Still a converter and kernels first; Decision 7 step 4 is what changes that.
* **Any number of the successor mint.**

## 9. Number hygiene and implementation record

0100 is the next free number after ADR-0099 (whose README row says so). Claimed on
`feat/adr-0099-sharded-seat`, the same branch, on 2026-09-10; ADR-0101 follows it there. **The
next free number is 0102.** The branch's merge base with `main` (`7f4dded4`) predates `main`'s
Borsh enum fix (`891a1a14`, which moved `ModelLineBenefitsDeclared` to the end): the merge must
keep `ShardCourtAccused` LAST, after it — tag 38 on both trees, pinned by
`the_one_move_courts_borsh_tag_is_the_last_and_pinned` — and the branch's own copy of the pre-fix
order is `main`'s to overwrite.

* **2026-09-10** — written and implemented the same day:
  * `consensus/core/src/palw_shard_court_v1.rs` — Decision 1: the object as a consensus object's
    payload, the session id with the network domain, the ceiling, the charge, the verdict, the
    filer's rule.
  * `consensus/core/src/palw_state_v2.rs` — `ShardCourtAccused`, `shard_court_ladder` on the
    extras, seven refusals, the fold arm; `palw_lifecycle_objects_v2.rs` — the ride rule;
    `consensus/src/pipeline/virtual_processor/processor.rs` — the cached fence, the extras' ladder,
    the acceptance arm, the name table; `kaspad/src/palw_panel.rs` — the seat keeps and files
    its refutation; `kaspad/src/palw_producer.rs` — the fence reader; `misaka/apps/cli/src/palw_shard_court.rs`
    — `palw shard-accuse`.
  * `consensus/core/src/palw_mode_v2.rs` — `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`, its root, the
    bundle check, the derivation test; `palw_derived_v1.rs` — the sweep; `config/params.rs` —
    the fence's new condition and its test.
  * `consensus/core/src/palw_shard_plan_v1.rs` — Decision 2: `shared` and `ends`, the inventory
    measurement, the shard rows, the derived-row rule.
  * `consensus/core/src/palw_measured_model_v1.rs` — Decision 3: the held artifact in the
    inputs and the document, `rms_eps_q` in the manifest, `NeedsTheArtifact`; the context renamed
    to the convention; `misaka/palw/sdk/src/bin/palw-class.rs` — `measure` and `verify`.
  * `consensus/core/src/palw_shard_licensing_v1.rs` — Decision 4's pure half.
  * **Decision 4's consensus half (same day):** `palw_shard_licensing_v1.rs` — the plan record,
    the two message functions and contexts, the extras parameters, the shard slice, the by-parts
    predicate; `palw_state_v2.rs` — three collections (rooted and carried behind `0x99` only once
    written), three delta entries, `ClassShardPlanDeclared` / `BondShardsDeclared` /
    `ShardReceiptLicensed` (tags 39–41), `PalwTransitionExtrasV1::shard_licensing`, fourteen
    refusals, the three fold arms, `license_claim` and the `write_claim` hook; `palw_panel_v2.rs`
    — `palw_panel_eligible_bonds_v2`, `derive_stratified_panel_v2`,
    `validate_panel_bound_v2_with_shards`, `receipt_quorum_over_seats_v2`,
    `validate_shard_receipt_part_v1`; `palw_lifecycle_objects_v2.rs` — three ride rules;
    `palw_mode_v2.rs` — V3 gains the two contexts; `config/params.rs`, `fork_id_v1.rs` —
    `Params::palw_shard_licensing` at every site, its two assembly rules and its test;
    `processor.rs` — the cached fence, the one-place stratified decision, the extras, the three
    acceptance arms, the stratified binding and its validation, the part assembler;
    `misaka/apps/cli/src/palw_shard_licensing.rs` — `palw shard-plan` and `palw bond-shards`;
    `kaspad/src/palw_panel.rs` — the seat files only where the chain's refutation ladder can try
    the claim, and its source pin reads the new arm.
  * `docs/palw-shard-plan-2026-09-10.md` regenerated; this ADR; the README index.
* **2026-09-10, the drill (§6 step 2).** Three devnet nodes on this Mac, the floor class only,
  `--palw-shard-court-devnet=0` (the genesis states the COMPLETE_V3 set; the court armed from
  block one); node-0 produces canonical free-prompt claims whose capture is corrupted at step leaf
  0 (`--palw-drill-tamper-fp-leaf=0`, devnet/simnet only), the other two are its panel.
  * **Run 1 found a defect in the drill's own lie, not in the court.** The panel patched the
    attempt-lane liar's outcome into the honest free-prompt run, so the run's FACTS (the four leg
    roots the free-prompt execution root is recomputed from) still described the honest capture:
    `palw_fp_commitment_from_context_v3` refused the pair (`ContextDoesNotReproduceTheRoot`) and
    node-0 logged "the run does not assemble into a commitment" every interval — no claim, so no
    court. Fixed by a free-prompt verb of the drill fault
    (`PalwExecutionBackendV1::execute_free_prompt_with_injected_fault`; BASE-0 corrupts the
    capture BEFORE any fact is measured, `corrupt_capture_v1` shared with the attempt lane), the
    live failure pinned by `a_free_prompt_drill_fault_assembles_into_its_own_commitment_and_convicts`.
  * **Run 2, PASS.** 22:25:36 node-0 committed claim `d3766988f70d7a37…` (tx `8bd939f1…`), its
    material self-consistent; 22:29:19 both seats found leaf 0 of the served capture does not
    recompute and filed `ShardCourtAccused` (sessions `69e87283…`, `c797e2ef…`) — the claim was
    `panel_bound`; 22:30:21 block `16729a4e…` carried one accusation, every node folded it, and the
    three duplicates that followed were dropped with their blocks standing ("wrong phase"); the
    claim reads `voided court_fraud` on node-1 and node-2 over RPC; bond 0's collateral fell from
    1,110,106,160 to 1,110,067,640 sompi, −38,520 (`void_and_slash` debits the claim's `reserved`).
    No human step between the lie and the slash.

## Mission alignment amendment — 2026-10-07

shard court/catalogと層別Panelの存在は訴追の土台である。stratified assignmentやquorumを外部proofの受理条件にせず、bounded shard openingsから非seatがterminal filingできる試験を追加する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md -->

</details>

<a id="source-0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle"></a>

<details>
<summary>0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md — 原文 L1–L438</summary>

[原文を開く](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md) · SHA-256: `070ed0fc77a317c8943318e71c9b01d354bd084aaf5d10d87a4f86d8e311afc0`

<!-- ADR_SOURCE_BEGIN:0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md -->
~~~~~~~~markdown
# ADR-0135 — A model is data: the permissionless registry derives its profile, proves its panel, and walks its lifecycle

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **PROPOSED 2026-09-17; the rule set built in shadow the same day, and Protocol Upgrade A built behind a
  dormant fence the same evening** (`palw_model_registry_v1.rs`, `palw_state_v2.rs`; §7). No shipped preset arms
  it: no parameter or fingerprint moves until the operator schedules `palw_model_registry` at a height.
* Operator's direction, in the operator's words: "誰でもモデルを chain へ登録でき、検証時間・必要 Panel 数・claim 上限・
  報酬係数までプロトコルが自動計算し、条件を満たしたら自動 activate"; "モデルをコードではなくデータとして追加する";
  "実測値を consensus parameter にしない"; "Panel 側も permissionless — 自己申告だけではダメ"; "モデル登録から
  activation まで全部 state machine"; "share も最終的には手動設定をなくしたい"; "既存 Canonical ML VM で表現可能 →
  permissionless、新しい opcode が必要 → protocol upgrade".
* Supersedes, in ADR-0133: Decision 2's *measured* p99 as an input to the profile (measurements become telemetry),
  §9a's "profile freeze by the operator" and "tiny activation → ramp by the operator" (the lifecycle does both).
  Keeps everything else of ADR-0133 (three clocks, Little's law, class-local fail-closed, the artifact options).
* Builds on: [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md),
  [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic compute from the
  graph), [0049](0049-palw-adjudication-contract.md) (the canonical IR the VM boundary is drawn on), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  and [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (the on-chain class registration and carriage this extends),
  [0056](0056-palw-permissionless-class-admission-and-share-economy.md) (class admission is permissionless already; this ADR removes the share it kept), [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) (who may judge a class).

## 0. The sentence this ADR is

**A model is registered as data — a graph root, an artifact root and its bytes, a canonical job, a quantization,
a runtime version, and a bond — and from that data every node derives the same verification window, prefetch,
inflight cap, required ready seats, registration bond and claim cadence; seats count only when they prove the
artifact, their participation, their collateral and a recent verification; the class walks REGISTERED →
PREFETCHING → PROBATION → ACTIVE_LIMITED → ACTIVE on chain-visible facts and falls to HELD alone; and nothing —
not a window, not a rate, not a share — is ever stated by a registrant, measured by an operator or committed to
a source tree.**

> **2026-10-06 terminology and future-profile amendment:** “canonical ML VM” / “VM boundary”
> below denote the then-existing layer-template/kernel evaluator, not a BVM/GVM/Universal VM
> implementation plan. [ADR0172](0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
> selects active-kernel declarative plans, with coordinated kernel updates for missing relations.
> [ADR0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md) replaces ordinary full replay
> for new large-model profiles with small encoded constraint checks and exact court on dispute.
> Historical work/seat/lifecycle formulas remain the legacy baseline; derive and test new resource
> schedules separately. The quoted operator direction and original decisions below are historical.
> This amendment changes no active registry or existing class identity.

## 1. What today's path still needs a human for, and what stays a human's

Today (ADR-0133 §9a): shadow measurement → the operator freezes a profile → a fence in `main` → a share set by
hand → a release. Each step is a fork or a decision per model, and the p99 the operator would freeze is the
measuring host's (an M4 200 s, an H100 50 s, a slow disk 500 s). This ADR removes every one of them **for a model
the canonical ML VM already expresses** (matmul, attention, the mixture router and experts, the gated-delta
recurrence, norms, rotations, the LM head — ADR-0049's IR at the class's `runtime_version`). What stays a
protocol upgrade: a model that needs an instruction the VM lacks. That is not a model registration but a VM
instruction-set change, and `palw_manifest_verdict_v1` says so (`UnsupportedOp`) rather than letting a node
guess at an op's meaning.

## 2. Decisions

**Decision 1 — the manifest.** `PalwModelManifestV1 { graph_ir_root, artifact_root, artifact_bytes,
canonical_prefill_tokens, canonical_decode_tokens, quantization_format, runtime_version }` with a registration
bond. It is today's on-chain class registration (ADR-0067's carriage: the profile IR and the canonical job)
plus the artifact's bytes — and nothing a human decides: no window, no reward, no share. A manifest is judged
before it is a class: `Valid`, `UnsupportedOp` (the VM boundary), `EmptyArtifact`, `EmptyJob`, `ZeroWork`.

**Decision 2 — the work, derived.** From the graph, every node computes `PalwModelWorkV1 { verification_ccu
(ADR-0131's economic compute of the job a seat replays), economic_ccu_per_claim (the same over the class's
expected draws, ADR-0132), artifact_bytes, working_set_bytes, ops_supported }`. Dense MACs, attention over the
kv length, the recurrence, the active experts, the logits and the elementwise work are counted by the one
versioned cost table; two nodes with one graph get one number.

**Decision 3 — the profile, derived, never measured.** Against `PalwRegistryGlobalsV1` — one set of reference
constants for every class, changed only by a fence:

* `verification_window_spans = ⌈safety × verification_ccu / reference_work_per_span⌉ + receipt_allowance_spans`
* `artifact_prefetch_spans = ⌈io_safety × artifact_bytes / reference_bytes_per_span⌉`
* `required_ready_seats = max(seat_count + spare_seats, ⌈seat_count × verification_ccu / (utilization ×
  reference_work_per_span)⌉)` — the panel plus the spare, or more where one claim a span already needs more
  replay time than that many seats offer at the target utilization
* `max_inflight_claims = ⌊utilization × required_ready_seats × window × reference_work_per_span / (seat_count ×
  verification_ccu)⌋` — Little's law
* `registration_bond = bond_per_span × verification_window_spans` — the burden a class puts on panels is what
  its registrant bonds
* `admission_claims_per_span = min(budget_ccu_per_span / economic_ccu_per_claim, max_inflight / window)` —
  Decision 5

The fleet's reference (ADR-0133): 4 G MAC-eq/s (2.4 T a span), 1 GB/s (600 GB a span), ×2 and ×2, one span of
receipt allowance, five seats plus two spare, 70 %. On it the dense tier and the hybrid derive two spans (one of
verification, one of allowance), one span of prefetch, seven ready seats; a Kimi-class of 1 T MAC-eq and 300 GiB
derives two spans, two of prefetch, seven seats; a 10 T MAC-eq graph derives ten spans and more than seven seats
— all from the same code, none from a decision. **Measured p95/p99 are telemetry**: op 185 keeps printing them
so the fleet can see whether the derived window is being met; they enter no rule.

**Decision 4 — readiness is evidence.** `PalwReadinessEvidenceV1 { artifact_root_matches, all_chunks_held,
participation_ok, free_collateral_sompi, last_probe_ok_span }`; a seat is ready for a class only with the root
matched, every chunk held (a possession proof over the manifest's chunks), its node participating (synced, not
held), free collateral for `readiness_collateral_multiple` seat exposures, and a canonical probe verified within
`readiness_probe_max_age_spans`. A declaration alone counts for nothing (the failure ADR-0132 found: every
genesis bond "declared" the hybrid and one could run it). A seat that says `Incapable` is simply not ready and
is not dealt the class until its evidence says otherwise.

**Decision 5 — no share.** A class's claim cadence is `budget_ccu_per_span / economic_ccu_per_claim`, capped by
what its window can hold: a heavy model claims rarely and is paid much a claim, a light one often and little, and
the network's compute budget a span is what is constant. Under the single lottery (ADR-0132 S, ADR-0133 §9a Fence
2) each class's share of draws is its admission over every class's (`palw_class_shares_from_admission_v1`); a
new class takes its slice from all in proportion, a class with zero admission holds none. The reward stays
ADR-0132's `f(EconomicAttempted, GlobalRate)` — no model's name enters it — and a per-claim escrow cap is not
adjusted per model: cadence is what moves.

**Decision 6 — the lifecycle.**

```text
REGISTERED ──manifest valid──▶ PREFETCHING ──ready seats ≥ required, collateral ok──▶ PROBATION
PROBATION ──probation_claims probes passed, none failed──▶ ACTIVE_LIMITED (a tenth of admission)
ACTIVE_LIMITED ──stable_epochs spans at target, nothing held──▶ ACTIVE (full admission)
PROBATION / ACTIVE_LIMITED / ACTIVE ──panel not drawable (ready < seat_count) or utilization ≥ 1──▶ HELD
HELD ──ready seats ≥ required, utilization < 1──▶ PROBATION (never straight back to ACTIVE)
```

Every arrow is `palw_lifecycle_step_v1` over `PalwLifecycleObservationV1` (ready seats, probes passed and
failed, utilization, collateral, span stability) — chain-visible facts at a span boundary. `HELD` holds the
class's own new claims and nothing else: another class on its own facts stays where it is, the execution lane
schedules the classes with `Final`s, the anchor cadence is untouched (pinned).

**Decision 7 — the boundary.** Registration is permissionless for a graph the canonical VM expresses at the
manifest's `runtime_version`; a graph that needs a new op is refused as `UnsupportedOp` and its op is a protocol
upgrade. The registry therefore never has to understand an operation it was not built to verify.

## 3. What Kimi's registration looks like

A third party submits the manifest and the bond. Every node derives `economic_ccu`, `verification_ccu`,
`artifact 287 GiB`, `prefetch 2`, `verification 2`, `max inflight`, `required ready 7` (or more), the bond. Seven
operators fetch the artifact, run the probe, and their evidence makes them ready. Ten probe claims finalize
with none failing: limited activation at a tenth of the derived admission. Three stable spans: `ACTIVE`. Two
operators leave: `HELD`; the dense tier and the lane continue; the operators return: `PROBATION` again. Nobody
updates a repository.

## 4. What this changes in ADR-0132 and ADR-0133

* ADR-0133 Decision 2 (`warm_p99`/`cold_p99` as inputs): **superseded** — the shadow module keeps them as this
  node's telemetry, the registry's profile ignores them.
* ADR-0133 §9a Fence 1: **becomes Protocol Upgrade A**, the generic framework — manifest, derived profile,
  readiness evidence, lifecycle, class-local capacity gate — armed once; no "Kimi activation" step exists after it.
* ADR-0132 Decision 6 (a new model earns only after a shadow period): **becomes** the lifecycle's `PROBATION` and
  `ACTIVE_LIMITED`, automatic.
* ADR-0124 Decision 6's unit and ADR-0131's rate: **the rate is global** (ADR-0132 C); no class sets a unit.

## 5. Security amendments

* **SA-1 — the registrant cannot shrink its window**: the profile reads the graph, not the manifest's opinion
  of it; under-reporting is impossible because nothing is reported.
* **SA-2 — the registrant cannot flood**: the registration bond grows with the window it imposes, admission is
  budgeted, and a class that cannot draw a panel is `HELD` at no cost to any other.
* **SA-3 — a seat cannot fake readiness cheaply**: the possession proof is over the artifact root the manifest
  names, the probe is a canonical verification the chain can check, and the collateral is real.
* **SA-4 — a class cannot fork the network by existing**: a manifest the VM cannot express is refused at
  registration; nothing about a registered class changes a rule any node runs.
* **SA-5 — globals are one fence, not one per model**: the reference constants are consensus parameters a
  fence moves for every class at once; a measured p99 never moves one.

## 6. The road

```text
now      ADR-0132 / 0133 / 0135 in shadow: the CLI prints each class's derived profile beside its telemetry
   A     Protocol Upgrade A — Permissionless Model Registry V1: manifest, derived profile, readiness evidence,
         lifecycle, class-local capacity gate (ADR-0132 F1/F2 folded in); the dense tier and the hybrid walk it live
   B     the single lottery, if adopted (ADR-0132 S): class shares from admission
   C     EconomicAttempted + a global rate + budgeted admission (ADR-0132 Fence 3), no per-model cap
         — built 2026-09-17 behind the dormant `palw_economic_payout` fence (ADR-0132 §7); armed after A's drill
after    Kimi, Llama, a new mixture, a 100B model: a manifest and a bond, no fork
```

## 7. What is built — Protocol Upgrade A, behind a dormant fence (2026-09-17)

**Armed for testnet-11 (prepared 2026-09-17 night, on `wip/arm-6001-registry-payout`, merged only on the
operator's go after the devnet drill):** `palw_model_registry` at the 6,001 flag day, beside ADR-0132 Upgrade
C (`palw_economic_payout`, ADR-0132 §7) and ADR-0137's work target (`palw_work_target`, which stops
Decision 5's shares being read at the same height they would first be written — ADR-0137 §3.5 is why the
two may not be a height apart). The fingerprint moves from `135b6ee0…` to `32c2e8e3… (`8af89f85…` since 2026-09-18, when the single lottery and the short challenge window joined the same day)`; the fork id's
height set does not (6,001 was already scheduled), so the two builds are told apart by the fingerprint alone.
Rows open at the first span boundary past 6,001 for every class the node can describe (the amendment
below), the seats' proofs count after one readiness age (30 spans × 5 DAA = 150 DAA, ~30 h), and the classes
walk the lifecycle from there.

**Amended 2026-09-17 (evening), found by the devnet drill:** the fold's registry input named "the genesis
classes' work, from the registrations the bundle carries" — but no shipped bundle registration carries an
admission carriage (a genesis class's profile is the catalog's, of which the bundle holds only a root), so no
genesis class had a work, no row opened for the floor or for testnet-11's three classes, and a class node-1
registered before the fence had no row either (its carriage was consumed by the registration block). The
node now describes a class from what it can prove it computes: a genesis class through the canonical class
table the binary compiles (`canonical_classes_v1`, the same derivation the registration message uses), a
registered class through the carriage the chain carried — before or after the fence — kept in the class
carriage store and adopted by a syncing node before the block that needs it. A class the node cannot
describe stays a legacy row. The fold itself is unchanged: it opens rows from the works it is handed.

**Amended the same evening, the second drill finding:** a possession proof names the span it was made for
and the fold took it only in that span or the next; on the devnet's two-DAA spans the carrier that brings it
waited longer than that in mempools and for a block (every proof refused as "names span 11 at span 21").
A proof now lands within `max(8 spans, 40 DAA)` of the span it names (`palw_readiness_landing_spans_v1`:
eight spans on testnet-11's five-DAA spans, twenty on the devnet's two — phase 2 found eight spans still short
of a chain of receipts on a bursty devnet; a future span is still refused), and the row it writes is dated at the named span's first DAA: a late proof is exactly as
fresh as when it was made, a replayed one renews nothing, and the thirty-span readiness age bounds the rest; on testnet-11 (five-DAA spans, ~5 DAA an
hour) the allowance is about eight hours of carrier latency.

**Devnet drill, phase 1 — PASS (2026-09-17 14:45Z, VPS 95.111.236.186, seven mapped-artifact nodes, `LANE=0,2,2`,
registry at DAA 20, node-1 registering `Qwen/Qwen2.5-1.5B/graph-v5@512`):** the fence crossed and the rows
opened at the first boundary (floor `Active`, the registered class `Prefetching`), the class registered and
its row opened, possession proofs landed, the grace passed with the floor `Active` and the chain producing
(tip 82), node-2 restarted and re-read the same rows. The class-local verdict at the first governed boundary
was `Prefetching` with two ready seats of seven required — the third drill finding: **a seat's proofs starve
behind its own receipt traffic.** Every lifecycle carrier of a node chains on one rolling fee outpoint with
eight unconfirmed at most, receipts for the floor's claims filled the chain for fifteen minutes at a time,
and four of seven seats never got a proof out (three submitted once). The node now lets a proof that found
no slot take the next one and holds its receipts until it lands (`readiness_waiting`), and asks for the
proof again at once instead of after the thirty-second read throttle. Memory on the host: 1.75 GiB of
file-backed pages once, 50–560 MiB anonymous per node, `MemAvailable` 9.8–10.8 GiB of 12 throughout.

**Phase 2, first attempt — the fourth finding:** with proofs given precedence and the landing allowance in
DAA, six of seven seats proved within a span of the restart and the seventh never could: the devnet mints
six genesis bonds (`PALW_DEVNET_GENESIS_BONDS`) with six fee floats, so node-6 held a key the chain never
bonded and no float to carry anything, while the registry's requirement is `seat_count + spare_seats = 7`
ready seats for its five-seat panels. The devnet now mints eight bonds and eight floats (its genesis
commitment and fingerprint move; a rehearsal network of public seeds, not a regenesis of anything
deployed), the drills run eight nodes, and phase 1 is repeated on the eight-bond genesis before phase 2.

**Phase 1 on the eight-bond genesis — PASS (2026-09-17 17:11Z, eight nodes):** the same six steps; the class
`Prefetching` with two ready seats at the verdict, because — the fifth finding — the per-node carrier chain
(eight in flight) took fifteen to twenty-five minutes to clear on an eight-node devnet: a child of an
unconfirmed parent is dropped in relay, so only the origin's own blocks (one in eight) mine the tail, and
every proof queued there (six of eight seats never got one out; `waits for a carrier slot` thirteen times
each). `MAX_INFLIGHT_CARRIERS` is now one: a carrier that spends a confirmed output relays everywhere and is
mined by anyone's next block, and a seat's throughput becomes one carrier a block, which testnet-11's
cadence never approaches. Memory: 1.75 GiB of file-backed pages once, `MemAvailable` 9.7–10.4 GiB of 12
with eight nodes.

**Phase 2 on the eight-bond genesis, second attempt — the sixth finding:** with one carrier in flight every
seat proved within a span (eight fresh proofs), the class entered PROBATION with an admission share of
980 ‰ — and its producer held on "this class's epoch budget is already spent": the epoch budget (ADR-0039
D5) is derived from the shares at the epoch's first block and read until the next, so a share the
registry raises mid-epoch admitted nothing until the epoch turned. The boundary that moves a share now
re-derives the current epoch's budgets (the produced counters stay); the fold test asserts Kimi's budget
follows its admission share in the same epoch.

**The seventh finding, behind the sixth:** with the budget following the admission share, the class in
PROBATION still drew at a ticket of 3.6 × 10⁻³ — hours per claim on the devnet's forwards — and nothing
moved it. The ticket was the floor's, exactly: op 180's terms seed a registration with the floor's current
target, a price converged to the floor's microsecond draws, and a model whose draw is a whole forward sits
hundreds of times too hard at it. The registry then holds the class weightless while it is REGISTERED or
PREFETCHING and grants it 980 ‰ at admission, and no rule re-prices it: the retarget skips a class that
produced nothing, and the idle convergence (ADR-0071) moves an idle class only toward the incumbent's
price, which it already had. (The earlier reading — the class DAA cutting the target after a first
epoch's over-production — was not the cause: the fresh-genesis run with the budget fix showed the same
ticket, and the floor producer printed the same number.) ADR-0076 already answers this at the activation
edge, for a weightless class seated at its activation DAA: *a class being seated is a class being
priced*, from the share the table just wrote and the class's own counted work. The registry's admission
is the same event, so at the first governed boundary a non-floor class holds a nonzero share in an
admitted state its target becomes `attempt_target_seed_v1(share, pwu)` and the row records
`priced_share_permille` — once; the class DAA owns the target from there, and every later share move is
measured by the retarget against the history the class then has. The floor is never priced here (its
target is the class DAA's, and it has history by definition). Op 186 prints the priced share. The
boundary that seats a class reads the cap utilization at the price it walked in with; the next reads
the seat's. The Upgrade C fixtures (ADR-0132 §7) now walk at the seated price — `MAX / 14 913` for the
toy at 900 ‰ × 160 pwu, 14 913 forwards a claim, the panel share at its floor — and their numbers were
re-derived. The drill is repeated from a fresh genesis with every fix in place.

**The fence.** `Params::palw_model_registry: Option<ForkActivation>` — `None` on every shipped preset
(the t11 fingerprint `135b6ee0…` does not move); hashed `Some`-only; the fork-id gate names it when
armed; `validate_palw_v2` refuses it without `palw_panel_economy` and `palw_execution_lane` at or
below its height (it reads seat exposure and steps at the lane's span boundaries). Arming it is a
height in one constant; the pins are `adr0135_the_registry_fence_is_dormant_everywhere_and_arms_by_height`.

**On chain, past the fence** (`palw_state_v2.rs`, ADR-0135's rows in their own guarded root
sub-block and carriage tail `0xAA`, delta entries 51/52):

* `model_lifecycles: class → PalwModelLifecycleRowV1 { state, work, profile, since_span, probes, the
  last boundary's reading, admission }` and `seat_readiness: (bond, class) → PalwSeatReadinessRowV1
  { proved_daa, proved_span, leaf_index }`.
* **Decision 1–3 — the profile from the graph.** `ClassRegistered` with its carriage opens the row:
  `palw_model_work_from_carriage_v1(profile, canonical)` reads the verification compute and the
  draw compute off the graph with ADR-0131's cost table (the artifact bytes are an estimate from the
  dense weights until a manifest carries them — V2); the class starts `PREFETCHING`, or `REGISTERED`
  where the graph derives no work (the VM boundary). The classes registered before the fence get
  their work from the bundle's genesis registrations (`PalwModelRegistryFoldV1::genesis_works`, the
  same on every node because the bundle is fingerprinted); a class registered before the fence
  without a carriage keeps no row and is never gated. The profile is re-derived at every boundary
  from the stored work and the class's live target (`palw_lifecycle_profile_v1`).
* **Decision 4 — readiness is a possession proof.** `SeatReadinessProved { bond, class_id, span,
  opening, signature }`: one leaf of the registered artifact root, at an index inside the window the
  (class, bond, span) challenge names (`palw_readiness_challenge_seed_v1`, eight consecutive leaves),
  the span current or just closed, signed by the bond's key (checked at acceptance like a
  capability declaration; below the fence refused). A seat is ready for a class while its proof is
  younger than 30 spans, it is active and above the floor, and its free collateral covers three
  times the network's floor (`palw_model_registry_ready_seats_v1`). The probe half is the class's
  own claims: a `Final` passes, a court fraud or a withholding fails (`note_model_probe`).
* **Decision 6 — the lifecycle at the boundary.** `step_model_registry` at every span boundary
  (before the lane's rotation): rows open, every row is observed (ready seats, claims in flight,
  the span's probes, utilization = inflight × seats / (ready × window)) and stepped by
  `palw_lifecycle_step_v1`; the base class is `ACTIVE` and never gated.
* **The class-local gate** (ADR-0132 F1/F2). `apply_attempt` refuses a claim of a class whose row
  does not admit (`ClassNotAdmitting`) or is at its inflight cap (`ClassInflightCapped`); a bind
  window that closes with fewer ready seats than a panel voids as `NoCapablePanel` (void reason 4)
  rather than `BindTimeout`; the draw and the fold's panel validation judge by evidence
  (`palw_bond_may_judge_class_v4`, `PalwPanelDrawPolicyV1::readiness`): under the registry a seat
  needs a fresh proof, not a declaration, and the base class stays open to every bond.
* **Decision 5 — shares from admission.** At every boundary the shares of the rowed classes are
  written from `admission_claims_per_span × admission_permille(state)` over the keys the table
  already holds — the base class holding what is left and never below its floor, a class
  registered before the fence without a row keeping the share it has. `PROBATION` admits at a twentieth (a class must be able to produce the claims that probe it —
at zero it could never leave probation), `ACTIVE_LIMITED` at a tenth, `ACTIVE` in full; a class that
admits anything holds at least the grant floor of one permille, so a target exists for it.

**Read.** Op 186 `getPalwModelRegistry` (`misaka palw registry`): the fence, the globals, every
class's row and its reading now, every proof and whether it is fresh.

**Tests** (`palw_state_v2::tests::adr0135`, `palw_model_registry_v1::tests`, the fence pin): the
boundary opens rows from the genesis work and the base class is active (and the delta reverts, the
carriage round-trips, the root moves only with rows, the fold without the fence is byte-identical);
readiness is a possession proof — a wrong leaf, a forged leaf, a stale span and the dormant fence
are refused, six ready seats leave a class prefetching and the seventh admits it to probation, a
thin bond does not count; a held class takes no claims while the base class keeps producing; the
inflight cap refuses the claim after the cap; `NoCapablePanel` names the void and the class recovers
through probation, and an operator outage (every proof aged out) holds it alone; the discriminant
pins (void reason 4, the object as the enum's last variant, delta entries 51 and 52).

**The seat's side, built the same night.** The panel service submits `SeatReadinessProved` on its
own (`readiness_duties`): every thirty seconds it reads op 186; for each non-base class the
registry is in force for it proves when the chain holds none of this bond's proofs for the class
or the one it holds is past half the readiness age (`palw_readiness_duty_due_v1` — a restart
re-reads the chain and never re-sends a fresh proof), never twice in a span; the leaf is the first
under the opening cap inside the challenge's window, opened from the held artifact
(`PalwExecutionBackendV1::artifact_row_opening`; every family roots its artifact in one
streaming pass and opens one row by a second — the inventory is never materialised: eight nodes
that materialised the A16 inventory to root it rebooted a 24 GiB host on 2026-09-17), rooted locally against the class's registered root
before it is signed by the bond's key. **Fail-closed**: a class this node holds no artifact for, or
holds under a different root, gets no proof and is named once in the log; a node in IBD or not near
the tip proves nothing; a bond that is inactive, below the floor or without the readiness multiple of
free collateral (op 186's `bonds`) proves nothing and says why once. A reorg that drops a proof's
block leaves no row, and the next span's duty re-proves against the current challenge — an old
span's proof is never resent (the fold refuses it). The producer reads the
same op before a draw and holds (a log line, no rule) for a class the registry holds or has at its
inflight cap. `--palw-model-registry-devnet=<daa>` arms the registry on a private devnet (with the
panel economy it reads, where the devnet has none). The registry's seat count is the network's
panel size (the bundle's panel params: five on testnet-11, so seven ready seats; the global
constant is the fallback), never a number of its own. The drill is
`scripts/drills/misaka-palw-model-registry-devnet-drill.sh`: eight fixture nodes, the lane at 2-DAA spans,
the registry armed at DAA 20, an artifact every node holds (`CLASS_ARTIFACT=`) so the seats prove
for its genesis class; it waits for the fence, the rows, the proofs, the grace's end with the base
class ACTIVE and the chain producing, and a restarted node's rows — the record is §7's last entry.

**The activation grace.** Proofs are refused below the fence, so at the fence no seat is ready and
every live class would be HELD at the first boundary. The fold therefore opens rows and takes
proofs from the fence but steps no row, moves no share, judges no draw by evidence and names no
`NoCapablePanel` until the fence is one readiness age old (`PalwModelRegistryFoldV1::grace_until_daa`,
30 spans); the classes registered before the fence open in the state their history earns (ACTIVE
with a `Final`, PREFETCHING without) and are governed from the grace's end. A test walks it.

**What the review asked, answered.**

* *Migration at activation*: nothing is migrated — the rows are derived at the first boundary from
  the chain (the base class ACTIVE; a class with a `Final` ACTIVE; a class with work and no `Final`
  PREFETCHING; a class the fence found without a carriage, none — never gated); the work comes
  from the bundle's genesis registrations, the same on every node.
* *Determinism*: the profile, the observation and the step read the state, the block context and
  `PalwTransitionExtrasV1` built from `Params` — no clock, no local store, no RPC; the fold test
  folds one block twice and compares roots, and the delta reverts to the parent.
* *Expiry*: a proof counts for 30 spans of DAA and no longer; a bond that leaves `Active`, drops
  below the floor or loses its collateral headroom stops counting at the next reading whatever its
  proof says; a class's artifact is its registration — a new root is a new class and new rows.
* *Inflight*: never a counter. Claims in flight are counted from the claims at every reading
  (accepted, not terminal), so a `Final`, a void, a timeout or a reorg needs no decrement.
* *NoCapablePanel*: named once per claim when its bind window closes without a panel; a HELD class
  takes no new claim, so nothing loops; with zero eligible seats the claim voids and the class holds.
* *Cadence*: shares move on the first block of a span (the lane's `opens_span`); a class enters the
  lottery when its share is above zero — ACTIVE_LIMITED or ACTIVE — and its share key must already
  exist (the registration grants it).
* *Old nodes*: a build without the fence cannot decode `SeatReadinessProved`, and past the fence
  computes no rows and no reasons — but it never gets there: the fork-id gate names the height, so
  it is refused as a peer from the fence; below the fence the new build refuses the object, so no
  block carries one.
* *Observability*: op 186 prints each class's state with its reason (`held: ready 3 < 5 for a
  panel …`, `probing 4/10 …`, `base class …`), `since_span`, the derived profile, ready seats and
  claims in flight now against the cap, utilization, admission, `no_capable_panel_voids`, the
  counts of classes by state, the bonds with headroom for a seat, and each proof's freshness with
  the reason it does not count (`stale`, `bond inactive`, `below floor`, `collateral short`) — so
  a HELD by the rule and a stop by a fault read differently.

**`artifact_bytes` is not consensus-critical, and V2 lets the registrant state it (built
2026-09-18).** V1 estimates it from the graph's dense weights, and only the prefetch allowance reads
it; admission, the bond, the window and the inflight cap read the compute. **Manifest V2** is the
object `PalwConsensusObjectV2::ClassManifestV2 { class_id, artifact_bytes, registrant_bond,
signature }` (tag 48, appended last): the class's registrant bond signs
`palw_class_manifest_message_v2` over the count, the acceptance layer checks the signature as it
does a readiness proof's, and the transition puts the count on the class's registry row
(`work.artifact_bytes`, and the working set where it equalled the estimate), re-deriving the profile
so `artifact_prefetch_spans` reads the file rather than the graph. Refused while the registry is
dormant, for a class without a row, from any bond but the registrant's, and for zero bytes; a
genesis class has no registrant and keeps the catalog's estimate; a re-measured file is a new
commitment. It is consensus-critical — the row it writes is rooted and its profile moves the
lifecycle — and it is in the 6,001 bundle by construction: accepted only past `palw_model_registry`, the
flag day. The CLI's `model-class` extension route builds and carries it beside the registration
when the extension manifest names `artifact.bytes` or a readable `artifact.path` (the file's length,
the meaning `artifact.bytes` already had), and withholds it below the fence. Not built: the node's
own `--palw-register-class` route does not yet follow its registration with a manifest (the operator
carries `<name>.class-manifest.borsh`).

**Audit corrections (2026-09-18, `docs/palw-audit-2026-09-18-6001.md`).** The pre-arming audit
refused the bundle as it stood and four of its seven blockers were this ADR's: the registry fold's
work map read the node's own carriage store, so a class without a lifecycle row could be rowed on one
node and not on another (a rooted divergence — the map is now the chain and the build alone); the
panel-room check went live with the work target while a readiness proof is refused below the same
height, so every class refused claims on the flag day (it now waits for `grace_until_daa`); the
ready-seat counter used the network collateral floor while the panel draw demands ten times it; and
the node's replay memory need was the largest artifact it held rather than the class it was proving.
The possession proof's width is unchanged and remains this ADR's V1 limitation — the audit's H-6.

**Not built, stated.** The possession proof's width (one leaf a proof, V1); a held class's existing
claims run to their end untouched. The PALW state sync path (`PalwStateSyncV2`, unused by the live
node) carries no lane and no registry.

## 8. Number hygiene

0135 was free when written; the next free number is 0136.

## Mission alignment amendment — 2026-10-07

registry lifecycle/admission jury/readinessの公開化は維持する。承認されたKernel内でも公開訴追不能なprofileを新しいmineable/weight-bearing状態へ昇格させない。public registrationとpublic prosecutionを別々に完成させる。

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md -->

</details>

<a id="source-0175-registered-models-are-permanently-immutable"></a>

<details>
<summary>0175-registered-models-are-permanently-immutable.md — 原文 L1–L103</summary>

[原文を開く](../adr/0175-registered-models-are-permanently-immutable.md) · SHA-256: `22ae8670d982d6325a536da7c8e9d4beb048071bcde1506462511eece963bb77`

<!-- ADR_SOURCE_BEGIN:0175-registered-models-are-permanently-immutable.md -->
~~~~~~~~markdown
# ADR-0175 — 新規モデル登録は自由、登録モデルは永久不変

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** Accepted design, 2026-10-09. `pre`に独立fenceとfold/CLIを実装済み（休眠）。2026-10-10の[検証記録](../evidence/adr/0175-immutable-model-registration-2026-10-10.md)を参照。既存ネットワークの有効化高さは設定しない。

## 決定

MISAKAでは誰でも新しいモデルを継続して登録できる。登録が成立したモデルの重み、計算グラフ、tokenizer、実行仕様、canonical artifact root、kernelとverification planのbinding、登録IDは永久に不変である。登録者、開発者、Panel、miner、運営者、ガバナンス、自己改善プロトコルにも既存登録を更新・上書き・置換する権限を与えない。

fine-tuning、蒸留、LoRA、新しい重み、異なるtokenizer・実行仕様・kernel/planの成果物は、新しい独立モデルとして登録する。新規登録には自身の登録ID、固定root、検証責任、報酬資格、PositionとAMMがある。派生元への参照は来歴であり、親を書き換える権限ではない。登録の自由は既存の客観的admission、資源上限、手数料・担保条件を免除しない。配布や報酬資格の成立を登録成立と混同しない。

モデルAの改善からBが生まれても、AのPosition、AMMのreserve、価格、holder残高をBへ付け替えてはならない。Bに投資・利用する場合はB自身の市場と登録を明示的に選ぶ。Aは永久にAのままである。

| 状態・操作 | 新規則 |
| --- | --- |
| 重み、graph、tokenizer、artifact root、実行/kernel/verification binding、登録ID | 永久不変 |
| 同一登録へのV2/V3 publish、preview、promotion、withdraw、head rollback/自動切替 | 拒否 |
| 改善版の独立登録、来歴の記録、評価・学習・dataset処理 | 許可。親の内容・市場を変更しない |
| 担保、slash、報酬資格の停止・回復、利用量、運用担当者 | 別の合意規則で更新可能 |
| Seeder、配布先、同じ内容の追加包装 | off-chainの任意運用。合意上のavailability lease/取得資格を要求しない。任意の受理済みmanifest/infohash/root bindingは不変とし、別包装は別binding IDで固定rootとの対応を認証する。登録内容を上書きしない |
| EARLY_VERSION | 旧wire bitを保持し、新規則下の新しい宣言では拒否 |
| PRIVATE_BETA / サービス特典 | 当該固定モデルへのアクセス・運用サービス。改善版への権利を含めない |

## 計算classと登録ID

`class_id`は既存のexecution/admission classを識別する。legacy profileのclassはgraphに由来し、TIR/Genの既存class IDはprogram/pipeline、layout、tokenizer、artifact root等も含む。この既存hash式を変更して履歴のclassを再採番しない。graphのみの識別子はlegacy profile IDまたはTIR `graph_ir_root`として区別できる。

`ModelLineFounded`が新fence以降に作る登録ID（stateの`line_id`）は、次のdomain-separated hashで固定する。

```text
model_registration_id_v1 = H64(key "misaka-palw/model-registration/id/v1",
  class_id || canonical_artifact_root || borsh(founder) || le32(name.len) || name)
```

同じ計算graph、founder、nameでも異なるrootは別登録になる。実行・tokenizer・kernelの意味は変更不能なclass定義とcanonical artifactに拘束する。kernelを拡張する場合は新たなclass/登録を使い、稼働中のprimitive semanticsを置換しない。宣言だけの`runtime_hash`等を検証済み仕様とみなさない。

既存のfounding lineの`line_id == class_id`とV1のline IDは互換aliasとして保持する。既存class登録carriageも自身の固定rootを持つ独立のfounding登録として保持する。既存ID・class hash・Positionの再採番は行わない。同一class内の追加登録は既存のroot ownership indexで一意性を検査する。一つのartifactを別の計算classで使う既存規則は維持し、同一classの他登録からrootを奪えない。

## 有効化と履歴

新しい`Params::palw_model_immutable_v1: Option<ForkActivation>`を使う。ConsensusV2、model lines、artifact root ownershipがその高さまでに有効であることを必要とする。全shipped presetで`None`。既存のimprovement、typed artifact、kernel、transportの休眠fenceをこの変更で有効化しない。

新fenceより前のwire object、enum tag、署名message、state row、snapshot、履歴foldは保持する。`None`/`Some(never())`は既存fingerprintを変えない。将来の高さはparams/scheduleにcommitし、将来fenceのidentity正規化も他のfenceと同じ方式を使う。reorgのdelta undoで過去の状態を再現する。

有効化時に存在するモデルは、その時点で確定している登録・versionと履歴を保つ。過去のV2をV1へ戻したり、旧previewを別の登録へ勝手に移したりしない。その後のpublish/promote/withdraw、lineage rollbackは名前を指定して決定論的に拒否する。既存preview等の履歴上の資格・期限は旧定義のままで、変更権限を再開しない。認識できるが拒否されたlifecycle objectは通常のacceptance規則に従って適用せず、carrying blockは成立する。

休眠classの再登録による運用復帰は、既存root、PWU規則、slash単位、fused semantics、registrantが一致する場合だけ認め、元の登録DAAを維持する。内容が変わる再登録は拒否する。shard verification planは新classの登録block内に確定させ、古いclassへの後付けを拒否する。提供者の加入・脱退やleaseの更新とは分離する。

## 自己改善

学習・評価は継続できる。旧`Promoted`（outcome tag 0）、`NoChange`（tag 1）を維持し、`CandidateSelected`をtag 2として追加する。新fence下の評価勝者は既に独立admissionを受けたcandidate classを指し、親の`head`、version、Position、AMMを変更しない。候補が利用・報酬の対象になるには候補自身の登録と資格が必要である。評価勝利だけで資格を継承しない。

既存の評価料、bond返還、trainer/dataset grant、vestingを旧資金保存則に従って清算する。未決epochがfenceを跨いでも勝者選択と支払は完了し、headは動かさない。opt-out/re-opt-inでも既存headを別のclassへ切り替えない。既存のrollback用wire payloadと履歴は残すが、新規則下では処理しない。

## 固定同一性・配布不介入と報酬配分の境界 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

## 改定対象と検証

ADR0088の同一line更新、RFC0004/spec17の自動head置換を改定する。ADR0095の将来version特典を禁止し、ADR0087/0089/0090/0091/0162の市場を固定モデルに拘束する。ADR0143、RFC0011とRFC0014/Torrentは内容bindingと運用可用性を分離する。

検証対象は、fence前の旧更新とwire tag、fence後の拒否、同一graphの独立root登録、既存履歴・Position・reserveの保持、休眠復帰と不正な上書き、既存epochの候補選択・資金保存・rollback拒否、snapshotとdelta undo、shipped fingerprintとfence scheduleである。これらの試験はtransport availabilityや新verification方式の完成証明ではない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

## ユーザー方針 2026-10-15 第4回 — 年次フォークの承認リストは追加のみ

> **統治する前提:** [PRINCIPLES §9](../PRINCIPLES.md)(凍結前提)。規則は [ADR-0179](0179-model-tiers-genesis-annual-1-0-registered-0-3.md) が定める。

* 年 1 回程度のソフトフォークまたはハードフォークは、`ApprovedModels`(1.0 の階層)に「モデル ID + 固定 artifact root」を**追加するだけ**である。既存モデルを上書き・置換・再 root しない。
* 実装では、validation が同じモデル ID の再掲を拒否する。membership は、登録済み class 自身の artifact root と一致したときだけ成立する(`consensus/core/src/palw_model_tier_v1.rs`)。
* リストへの指定は政策的決定であり、公開性・独立検証可能性の技術的証明ではない。
* 後から登録したモデルは登録時点から 0.3 に固定し、審査で 1.0 にはならない。登録の自由と不変性(本 ADR)は変わらない。
~~~~~~~~
<!-- ADR_SOURCE_END:0175-registered-models-are-permanently-immutable.md -->

</details>

