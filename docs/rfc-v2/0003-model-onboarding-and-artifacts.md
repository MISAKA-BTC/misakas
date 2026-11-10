# RFC-v2-0003 — モデル登録・変換・Artifact

編集日: 2026-10-29 / 入力: `misakas-pre(5).zip`

[15本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

**原文収録元:** [旧0002](../rfc/0002-palw-tensor-ir.md)、[旧0011](../rfc/0011-permissionless-model-and-long-context-onboarding.md)、[旧0013](../rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md)。

**関連RFC:** [01 推論・生成Job・入出力](0001-inference-and-generative-jobs.md) / [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [04 モデル二階層・年次カタログ](0004-model-tiers-and-annual-catalog.md) / [05 資源上限・長文脈メモリ・計測](0005-bounded-resources-and-long-context.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [09 Remote Client・SDK・非保管型操作](0009-remote-client-and-sdk.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. 登録までの経路

旧0011のpermissionless onboardingと旧0013のreproducible layout／resource-bounded toolsを統合する。旧0002 Part IIのうちpreflight・converter・runtime pack・段階的登録・対応範囲の条項もここへ移す。

標準経路は、入力artifactの検査、architecture／resource preflight、streaming変換、TIR／layout／pack生成、identity固定、登録、対象profileの検証条件確認である。登録できたことと報酬付きで採掘できることは区別する。

## 2. 不変なモデルIdentity

モデルIDとartifact root、TIR、tokenizer、layout等のbindingを維持する。既存IDの中身を後から入れ替えない。改善版は独立した不変モデルであり、元のID／rootを上書きする運用へ戻さない。

登録時の構造・形式・資源検査は残る。公開取得可能性は成功 Final の必要条件とする。詳細と実装前の未決事項は [公開モデル可用性 RFC](public-model-availability.md) を参照する。保持者数による昇格制度は導入しない。モデル階層は04、廃止した審査の台帳は15を参照する。

## 3. 再現可能な変換と資源境界

同じ入力から同じ正規artifactを得るため、converter version、quantisation data、layout、sidecar、pack manifest、digestを固定する。全量を同時に保持しないstreaming手順と、RAM／ディスク／一時ファイルの上限を明示する。

対応するkernelや検証関係が不足しているモデルを、「登録形式を読めた」だけで採掘可能と表示しない。対応率を示す場合は母集団、architecture、task、context、除外理由、失敗地点を残す。

## 4. 段階を飛ばさない

候補の解析、変換成功、登録受理、運用状態、claimの検証、報酬／consensus creditの資格は別の状態である。特定の段階名・wireは元のversionに従う。未対応の演算や2M contextを、今回の文書統合によって対応済みにしない。

## 5. 受入確認

header-only preflight、変換の再現性、壊れたartifactの拒否、identity不一致、既存ID上書きの拒否、低メモリ環境、登録からclaim検証までの結合を確認する。裁判可能性の判定と反証の完遂条件は14から参照し、登録側で緩い別基準を作らない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の15本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0002 | 732–1244 | [Part II — Model onboarding: a standard path from a Hugging Face directory to a registered, repr](#source-v1-0002-L00732) |
| 旧0002 | 1418–2078 | [II.6 Feature scope (R4)](#source-v1-0002-L01418) |
| 旧0011 | 1–746 | [RFC-0011: Permissionless model onboarding with probabilistic constraint verification and court ](#source-v1-0011-L00001) |
| 旧0013 | 1–394 | [RFC-0013: Reproducible layouts and resource-bounded model onboarding tools](#source-v1-0013-L00001) |

---

<a id="source-v1-0002-L00732"></a>

### 旧RFC-0002 / 原文732–1244行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIR文書内のonboarding手順をモデル登録RFCへ / onboardingの概要をモデル登録RFCへ。

### Part II — Model onboarding: a standard path from a Hugging Face directory to a registered, reproducible and honestly staged class

*Added 2026-10-01 (lane F, branch `tir/onboard`). Part I defines the IR, the court and the fused-kernel rule; Part II
defines everything between a model someone downloaded and a class that earns. **Part II adds no primitive and changes
no consensus rule.** §II.7.5 records the consensus rules *decided* on 2026-10-01 under a dormant fence; they are specified there and
not yet built. Everything else in Part II is tooling, data formats and node-side reporting, outside consensus (P7).*

#### Part II in one page

Part I makes a model *expressible as data*. It does not make *getting a model there* a standard path, and today each
model is refused for a different reason, late: after hundreds of GB were downloaded and converted. Part II specifies eight
things, one per requirement of §II.0.1.

1. **A header-only preflight (§II.2).** Reading only the Hugging Face config files plus the safetensors headers, or the
   GGUF header, it reports the architecture and its features, every tensor's storage type, the vision/audio/MTP parts the
   class would not compute, the artifact size, the testnet-12 conditions the chain would judge (admission sizing, close
   sizes, court cost, DA ladder, the court window, the canonical job), and the memory a seat needs, and then names, per
   stage (**convert**, **register**, **mine**), what is missing. It answers before a single weight is fetched.
2. **The converter as a function with a streaming loader (§II.3) and versioned runtime packs (§II.4).** The converter holds
   one block of a tensor at a time. A *runtime pack* is one manifest pinning the source, the frontend, the quantisation
   descriptors, the profile, the converter and its math, the executors, the logit convention, the result and the
   conformance vectors, so that anyone can rebuild the same artifact root and check it on three independent executors.
   Adding a model never inherits another model's conditions: every condition is in the pack and in its digest.
3. **Quantisation as data (§II.5).** A checkpoint's quantisation is a descriptor file (`misaka.palw.quant-format.v1`)
   read by one interpreter. An unknown type is refused by name, with the safe paths that exist; a new one is a file with
   test vectors, not a release.
4. **Feature scope (§II.6).** A class is one function. What the model has beyond it (a vision tower, an audio tower, a
   projector, MTP heads) is named with its evidence in the pack, the listing and the preflight; the class either computes
   a part entirely or says it does not.
5. **Staged enablement (§II.7).** Registration is existence; mining is a separate, gated stage. The chain's lifecycle
   already separates them; §II.7 states what it does and does not guarantee against the requirement, gives the registry a
   structured `blocking` field (stage, code, what is missing, how much), and records the consensus rules, decided but not yet built,
   that close what it lacks.
6. **Hub-wide end-to-end coverage (§II.10).** The 50–100 architecture corpus measures the lowerer, not how many Hugging Face
   repositories can register, obtain independent seats, produce a valid claim and reach `Final`. A pinned Hub census and a
   stratified, real-checkpoint cohort measure every gate separately. The largest measured failure categories determine the
   next generic frontend, format, modality, admission or verification work; a declaration of broad coverage needs the
   end-to-end evidence, including panel capacity.
7. **Future-model permissionlessness (§II.11).** A new family whose computation fits the armed TIR primitive set and
   canonical job profiles can supply its own compiler or declarative frontend pack and register canonical program and
   artifact bytes without a `main` release or model-name allowlist. Missing semantics require a reusable versioned
   kernel extension, checker/court/resource evidence and coordinated activation before registration.
8. **The local-LLM kernel-coverage objective (§II.12).** Aim for at least 90 % of a pinned, publicly runnable local-LLM cohort
   to reach registration, independent seats and `Final` through active tensor/verification kernels. Publish the
   residual extension/resource/source gaps; do not presume that a VM covers the remainder.

#### 概要(Part II、日本語)

- **目的。** 「このモデルは登録できるか、何が足りないか」を、数百 GB を落とす **前** に答える。今は一つ一つのモデルが別の理由で、遅く拒否される。
- **preflight(§II.2)。** HF の config と safetensors の header(GGUF は header だけ)しか読まない。architecture と feature(Level A/B/C)、
  tensor ごとの量子化型、vision/audio/mmproj/MTP、artifact の推定サイズ、t12 の条件(admission の大きさ、close の大きさ ≤ 搬送上限、
  court cost ≤ 2^26、DA の ladder、court window、canonical job)、seat の必要メモリを出し、**convert / register / mine** のどの段階で何が欠けるかを名指しする。
- **runtime pack(§II.3–II.4)。** 変換は streaming(常駐メモリは最大 chunk のみ)。pack は source・frontend・量子化 descriptor・profile・converter(数学は
  `libm-v1`)・executor・conformance を 1 つの manifest に固定し、誰でも同じ artifact root を再構築して、独立 3 実装で確認できる。他モデルの条件は黙って継承されない。
- **量子化はデータ(§II.5)。** GGUF/GPTQ/AWQ/FP8/compressed-tensors/MXFP4/bitsandbytes は descriptor ファイル。未知の型は **名指しで拒否** し、安全な経路を示す。新型は release でなくファイル+テストベクタ。
- **feature scope(§II.6)。** text-only はすべての面に出る。vision / audio / projector は「未対応の feature」であり、部分対応にはしない。
- **段階的な有効化(§II.7)。** 登録は存在、mining は別の段階。現行 lifecycle の穴(ready seat は bond 数・probation の claim は報酬が出る・実行系の認証 gate が無い・seat のメモリ不足は無言)を洗い出し、
  node 側だけで直せる部分(registry の構造化 `blocking`)は実装し、consensus に触れる部分は dormant fence(`palw_class_seating`、全 class 種別に 1 つの着席規則)として 2026-10-01 に決定済み(未実装)。
- **完成基準(§II.8)。** 50〜100 の代表 architecture、既存 feature で ≥ 90 % 表現可能、到達可能な TIR op への court coverage 100 %。
- **HF 全体への到達(§II.10)。** 上の 90 % は代表 architecture の下ろしやすさであり、HF のモデル登録率ではない。公開モデルの
  repo と revision を固定して、取得可能性、変換、実サイズの admission、独立 seat、claim の `Final` を別々に測る。失敗を
  format・feature・modality・court・座席/処理能力に分類し、件数の大きい汎用的な欠落から閉じる。実走なしに「大半が登録・Final」と言わない。
- **将来のモデル(§II.11)。** 既存 TIR primitive と canonical job で意味を記述できる新型モデルは、第三者の frontend pack または
  直接生成した TIR / plan を使って、`main` のモデル別更新なしに登録できる経路を必須とする。未対応演算・checker・courtは
  汎用Kernelの合意更新が必要で、VMでは迂回しない。有効化と実サイズの実走までは対応済みに数えない。
- **ローカル LLM のKernel coverage目標(§II.12)。** 公開・取得可能な固定cohortで、active tensor/verification Kernel経路による
  ≥ 90 % の登録・独立 seat・`Final` を目標にする。残りはKernel拡張・format・resource等の欠落を公開し、未実装を成功と数えない。

#### II.0 Requirements, acceptance and the three examples

##### II.0.1 The requirements

The user's, in the user's order. R1–R5 are the onboarding path; R6 adds the 2026-10-03 Hub-majority outcome;
R7 makes future-family onboarding independent of a `main` release where the armed semantics suffice;
R8 sets the local-LLM active-kernel coverage target, replacing the historical 90/10 VM split.
The acceptance criteria are §II.0.2.

- **R1. A header-only preflight.** Read only the Hugging Face config files plus the safetensors headers, or the GGUF
  header (metadata KV and tensor infos, never the data section). Report: supported architecture and features; per-tensor
  quant types; vision/audio components; estimated artifact size; the testnet-12 court and canonical-job conditions
  (admission v10 sizing; close sizes ≤ the carried cap; court cost ≤ 2^26; DA step leaves against the ladder, 2^22 below
  `palw_tir_fence2` and the class's own ladder up to 2^30 past it; the court window); and the seat resources needed.
- **R2. Versioned runtime packs.** One manifest identifies the model config, weight format, profile, converter version,
  executor and conformance vectors. Adding a model never silently inherits another model's conditions, and registration
  is reproducible.
- **R3. An extensible quant registry.** Unknown types are refused explicitly, never misread. New types can be added as
  decoder modules with tests. Refusals present the safe paths that exist.
- **R4. Feature scope shown.** A text-only artifact says so in the manifest, the listing and the preflight. A model that
  needs extra files (a vision projector, an `mmproj`) is shown as an unsupported feature, never as partial support.
- **R5. Registration separated from mining enablement.** Mining claims and rewards stay off until the class is
  adjudicable, the executor is certified (conformance), and enough ready seats from distinct operators have possession
  proofs. A failure says which stage (convert / register / mine) lacked what, in both the CLI and the chain registry.
- **R6. Most Hub models can complete that path, measured as repositories rather than architecture examples.** The
  system must name each blocking gate, close the largest generic gaps without a model-specific consensus path, and prove
  that a representative set of real checkpoints can register, be independently seated and reach `Final` (§II.10).
- **R7. Permissionless extension within an armed semantic envelope.** An unknown model family that can be expressed
  with the armed TIR primitives and job profiles must be registerable through a third-party frontend or direct canonical
  TIR submission, with no model-name allowlist, built-in feature-registry entry or `main` code change (§II.11).
- **R8. Local-LLM kernel coverage.** Target at least 90 % of a dated, reproducible cohort of publicly runnable local
  LLM checkpoints through active tensor/verification kernels. The residual requires explicit extension/resource
  work, not RFC05's withdrawn VM fallback; publish real-size registration-to-Final evidence (§II.12).

##### II.0.2 Acceptance

- **A1.** A model with a custom quantisation gets its verdict, and the features it lacks, before its full weights are
  fetched.
- **A2.** The artifact root, the converter version and the profile are reproducible and pass an independent conformance
  check.
- **A3.** The testnet-12 preflight (shape depth and above) shows the court window and the canonical-job conditions.
- **A4.** Mining starts only after the ready seats of distinct operators and their possession proofs.
- **A5.** Failures are visible per stage, in the CLI and in the registry.
- **A6.** A pinned, auditable Hub census and real-checkpoint cohort meet the quantitative end-to-end claim rule of
  §II.10.5. The architecture-corpus percentage alone cannot satisfy A6.
- **A7.** An independently authored, previously unknown family and weight layout pass the no-`main`-change
  registration-to-`Final` exercise in §II.11.4; unsupported semantics refuse by name and never execute as an unknown op.
- **A8.** Meet §II.12's ≥90 % active-kernel threshold and confidence requirement, with a complete residual table.
  Residual failures stay in the denominator; A8 does not establish 100 % coverage or support for an inactive
  extension. A proposed kernel route is not a measured result.

##### II.0.3 The three examples

| Example | What it needs | Where Part II answers it |
| --- | --- | --- |
| **`qwen4_exp`** — QSA (sparse block attention), a hashed n-gram per-layer embedding, a gated residual; 125 B parameters with 6 B active plus 51 B of n-gram tables; about 360 GB | feature lowerers that exist as data/generic code, not a Qwen4 parser; a streamed conversion; a verdict from headers | §II.1 (Levels), §II.3, §II.2: the architecture is a *combination of features* (lane G); the preflight reads a 360 GB checkpoint's headers (kilobytes) and says Level A/B/C, the artifact size, and the admission sizing |
| **`Mitsuba-ComfyUI-27B-GGUF`** — a Qwen3.8-27B-family GGUF with custom types `PQ2_0` / `PTQ1_0` and an `mmproj` | a quant registry that refuses a custom type by name and takes a descriptor file; feature scope for the `mmproj` | §II.5 (the refusal names the type id and the tensors, lists what is described, and gives the safe paths); §II.6 (`mmproj` is a modality the class does not take; the class is the text decoder). The two custom types stay refused until their layout is specified; a descriptor file can add them with no code change |
| **A Qwen3.5-9B refused for its court window** — "the court needs 5,102 DAA, the t12 limit is 3,000" | the court window as a *named, early* condition | §II.2.3(7) (the preflight computes the window and the limit at the asked height) and §II.2.6 (the plug point for the class-specific window that release int-10 carries) |

##### II.0.4 Where each requirement lives

| Requirement | Part II | State on `tir/onboard` |
| --- | --- | --- |
| R1 preflight | §II.2 | specified here; the command is task 7. Its inputs exist: `TensorSource` headers (§II.3), `model::analyze`/`ArchitectureReport` (lane G), `model::scope_of`, `check_ir_config_v1`, the descriptor registry |
| R2 packs | §II.3, §II.4 | implemented: `palw-class pack build \| verify \| show` (`468ae4404`); `libm-v1` (`537ca553f`); streaming converter (`b0d799769`, `0876a1744`, `ef1f8ef40`); content-addressed chunks (`e6fbb3ba2`); streamed inventory root (`e502567ba`) |
| R3 quant registry | §II.5 | implemented: 29 ggml types (`957d8e26d`), GPTQ, AWQ, FP8 block, three compressed-tensors formats (`1db3c430f`), MXFP4 from safetensors (`096e88de5`), bitsandbytes nf4/fp4/int8 (`f74d1a333`); known-undescribed list (`17febcd9d`) |
| R4 feature scope | §II.6 | implemented (`d7c0d776a`); shown by the report, the pack and `misaka model inspect` |
| R5 staged enablement | §II.7 | analysed here; the registry's `blocking` field and its CLI display are node-only and specified in §II.7.4; the consensus rules of §II.7.5 are decided (2026-10-01) and not yet built |
| R6 Hub-majority outcome | §II.10 | new acceptance program, **not an implementation claim**; requires a live Hub census, real checkpoints and an adequately seated end-to-end drill |
| R7 future-family extension | §II.11 | proposed direct-TIR and third-party frontend contract; existing data adapters and quant descriptors cover only their current languages, so this is **not yet a proven end-to-end capability** |
| R8 local-LLM kernel coverage | §II.12 and [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md) | unmeasured target; real-size checker/court/panel resources need evidence; no VM route |
| A1–A5 | §II.8 | A2 is met by the pack; A1, A3, A4, A5 complete with the preflight (task 7), `blocking` and the decision on §II.7.5 |
| A6 | §II.10.5 | open until the measured `register` and `Final` gates pass; neither the 92 % A/B corpus reading nor the earlier Hub estimates close it |
| A7 | §II.11.4 | open until third-party direct-TIR and frontend-pack paths pass with unchanged node/consensus binaries and independent seats |
| A8 | §II.12 | open until the dated cohort, active-kernel end-to-end funnel, confidence requirement and residual table meet the target |

#### II.1 Principles

These come from lane G's frontend (`misaka-palw-tir-lower`, branch `tir/generic`;
`docs/design/palw/tir/model-adapter-v1.md` §§1–6, `docs/design/palw/tir/hf-coverage.md`; commits `c52b8b3a7` ModelSpec V1
and the feature registry, `f2a364b74` Gemma-4 as data with no per-family Rust in the default build, `1a4964205` the Qwen4-Exp
fixtures through the data adapter). Part II depends on them and restates them because every part of the onboarding path
must obey them.

- **P1. `model_type` is informational.** Nothing is selected by it. Two models whose `ModelSpec`s are equal compute the same
  function whatever they are called, and the court never sees the name.
- **P2. A finite, versioned feature vocabulary.** A model is a *combination* of features (`FeatureId`s of `model::REGISTRY`:
  attention variants, MoE routing, linear recurrences, state-space blocks, per-layer inputs, KV sharing, hyper-connection
  streams, hashed n-gram embeddings, …). A feature is lowered once, by a generic lowerer, to the existing primitives.
  Adding a feature adds a lowerer and tests, never a primitive and never a per-model code path.
- **P3. Levels A, B and C, and adapters are data.**
  - **A**: the standard keys and tensor names suffice; the reader's own template reads the configuration.
  - **B**: an adapter file (`misaka.palw.model-adapter.v1`, a JSON document with a bounded, pure expression language)
    maps the class's keys onto features; no protocol change. The adapter is pinned by the BLAKE2b-512 hash of its
    effective canonical JSON, and the built-in pack by a pack hash, so a runtime pack names exactly which reader ran.
  - **C**: a capability is missing: a feature that cannot be lowered yet, or a protocol gap, **named, together with the
    smallest *general* primitive that would close it** — never a model-specific one. A Level C verdict is a statement about
    the vocabulary, not about the model.
- **P4. Static shapes, nothing ignored.** Every dimension is constant except the history length `H` (§3.1). A configuration
  key that no rule reads is refused (`NOT_LOWERABLE`), never ignored: a key that can change the math is understood or the
  model is refused.
- **P5. Selection is in the IR, not in the model's code.** `TopK` breaks ties to the lowest index and returns its set in
  index order (PALW-TIR-11, ADR-0052 B); routing variants (grouped, shared + routed, bias terms) are data over `TopK`,
  `Gather` and `Select`.
- **P6. State is declared.** Every recurrence and every cache is a state declaration of §4.1 (`Fixed` with its range, or
  `Hist` with its window), derived by the lowerer from the spec's mixers. Nothing about state is implicit in a kernel.
- **P7. The court sees only TIR nodes.** The court knows primitives, committed values, params and the primitive semantics. It
  never sees a model name, a feature, an adapter, a quantisation descriptor or a pack. Therefore **everything in Part II
  lies outside consensus** (§2): a class's identity is `H(graph_ir_root, layout, artifact_root, tokenizer)` (§4.1), and
  how the artifact was produced, or by whom, confers no protocol authority. The one exception is the lifecycle of §II.7,
  which is registry behaviour and is analysed, not changed, here.
- **P8. The legacy converter is frozen.** `qwen36-convert` serves the three architectures it already serves and points every
  other model at `palw-class check-architecture` and the TIR path (commit `0e5f56ac7`). New architectures are never
  added to it.
- **P9. A limit of the tool is not a verdict on the model.** A refusal states what the *tool* lacks (a feature, a
  descriptor, a primitive, a fence) and what exists instead. The preflight reports an unknown as unknown and a block as a
  block; it never turns "this build cannot" into "this model cannot" (ADR-0108's rule for verification depths).
- **P10. Reproducible by construction.** A conversion depends on nothing but the pack's inputs: no clock, no thread count,
  no platform libm (§II.3.3). Two builds from one pack give one artifact root.

#### II.2 The header-only preflight (R1)

`palw-class preflight <input>` and `misaka model preflight <input>` answer, for a model nobody has downloaded yet, whether it can be
registered and what is missing. Both print the same report, human or `--json`.

##### II.2.1 Inputs, and what is read

The command dispatches on the kind of its input and **says which mode ran in the first line of its output**.

| Input | Mode | Read | Never read |
| --- | --- | --- | --- |
| a directory with `config.json` (a Hugging Face snapshot, possibly with the weight files absent or sparse) | model | `config.json`; the presence and size of the tokenizer files (the tokenizer files are read whole: they are megabytes) and of `generation_config.json`, `preprocessor_config.json` and any `*mmproj*` file beside the checkpoint; `model.safetensors.index.json`; **the header of each safetensors shard** (8-byte length and the JSON header) | the data region of any shard |
| a `config.json` file | model | the same, beside it; shard headers if `--headers <dir>` supplies them (`*.safetensors` files or their header prefixes) | |
| a `.gguf` file | model | the magic, version, tensor count, the metadata key/values and the tensor infos (name, dimensions, type, offset) | the data section: the file may be truncated after the header, and the tool only uses the file size |
| a model repository id (cargo feature `remote`, off by default) | model | the same ranges by HTTP range requests, one request per header | everything else; no test uses the network |
| a `.palwtir` artifact | artifact admission | as today (`palw-class preflight` / `misaka model preflight <artifact>`) | |

A download that follows a preflight is therefore a *decision*, and the report says how many bytes it would fetch and how many
of them the class needs: a tensor the feature scope leaves out (§II.6) is listed with its size and need not be downloaded.

##### II.2.2 Depths and the first line

The verification depths of ADR-0108 (Structural, Vectors, Full) apply, renamed for a model:

| Depth | Computes | Needs |
| --- | --- | --- |
| `headers` | architecture and features, scope, per-tensor storage types and their descriptors, the tensor and shape checks against the program's parameters, the artifact-size estimate; the convert stage's verdict | the files of §II.2.1 |
| `shape` (the default when a network is named) | adds the **shape-only lowering** (no weights: the program depends on the configuration, and the parameters' shapes come from the headers), the admission sizing, the chain's conditions at a height, the seat's resource needs and the lifecycle forecast; the register and mine verdicts | a network (`--network`, default `testnet-12`) and a height (`--height <DAA>`; default the node's tip when `--node` is given, else the first height at which every fence the network schedules is in force, and the choice is printed) |
| `full` | the artifact is built (or supplied) and verified: the existing artifact admission and `pack verify` | the weights and a pack |

The first line is of the form

```
preflight: model · Hugging Face directory · depth shape (requested shape) · testnet-12 at DAA 7,150 · 61 KiB of 17.6 GB read
```

or `preflight: artifact admission · .palwtir` for an artifact. When a depth stops early the report says `depth_reached`,
`depth_requested` and `stopped_at` (for example `depth_reached: headers, stopped_at: "network not given"`), and every verdict that
needs a deeper depth is `unknown`, never `ok`.

##### II.2.3 What the shape depth computes

Each item names the function that answers it, so the preflight is a composition of existing, tested parts and invents no rule.

1. **The model.** `hf_schema::read_model_with` reads the configuration (and, when headers are given, the tensor names) into a
   `ModelSpec`; `model::analyze` gives the `ArchitectureReport`: the Level (A, B or C), the adapter (built-in, user file, none)
   and its hash, the features used with their lowering status, the assumed defaults and the refusals by named feature.
2. **The scope.** `model::scope_of` over the configuration, the tensor index and the files beside it (§II.6): what the class
   computes and what it leaves out, with evidence and the bytes the left-out tensors hold.
3. **The storage.** A histogram of the checkpoint's tensors by storage type (safetensors dtype; GGUF type id and name), each
   mapped to a descriptor of the registry (§II.5): described and supported; **no descriptor** (the id, the name the metadata
   gives it, the tensors that need it); known and not yet described (§II.5.6); or refused by the descriptor's own rule (for
   example LLM.int8 with an outlier threshold). Parameters a descriptor reads from `quantization_config` are resolved and
   its shape checks run on the header-only role tensors (plus the few small role tensors a descriptor reads for its own
   parameters, at most 4 KiB each).
4. **The tensors.** Every parameter of the lowered program is bound to a checkpoint tensor (or a served virtual tensor, §II.5.3)
   and its shape equals the header's; a missing tensor, a shape mismatch and a checkpoint tensor nothing reads (outside the
   scope's left-out parts) are reported before any download.
5. **The artifact.** The estimated size of the `PALWTIR1` container: the parameters by the quantisation policy (the integer
   width each weight lowers to, per-row scales, tables), the program, the tokenizer; and the *download* size, the bytes of the
   tensors the class needs.
6. **Admission.** `tir_admit_v1` (the function consensus admission calls) over the shape-only program: node counts against the
   caps, ranges, costs, court cones; the verdict is one of Part I §8's (`ADMISSIBLE`, `ADMISSIBLE_GENERIC`, `EXCEEDS(limit, value,
   cap)`, `NEEDS_PRIMITIVE(name)`, `NOT_LOWERABLE(reason)`).
7. **The chain's conditions at the height**, each as *needed against limit*:
   - **close sizes**: the worst carried close of every terminal tile (PALW-TIR-38, `palw_tir_close_size_v1`) against the carried cap;
   - **court cost** (`derive_court_cost_v1`): terminal MACs, operands and close chunks against 2^26 elements / terms and the
     per-tile ceilings of §5.4;
   - **the DA ladder**: the class's step leaves against the ladder in force — 2^22 below `palw_tir_fence2`, the class's own
     ladder, up to 2^30, past it;
   - **the court window**: the DAA the court needs to adjudicate the class (the dissection ladder's rungs, their deadlines and
     the close) against the window the network gives it (§II.2.6);
   - **the canonical job**: the declared prefill and decode token counts, `max_context` and the history bound against the
     network's canonical-job bounds and the class's profile;
   - **the fences**: every fence the class needs (`palw_tir_v1`, `palw_tir_fence2`, …) against what the network has armed at the
     height (`FENCE_NOT_ARMED(name)`), and the registration's price (the 1 BILI burn and the carrier fee).
8. **The seat.** The memory one seat needs to replay the class — the artifact (mapped) plus the replay working set, which
   grows with the context (the K/V history and the scratch of the widest cone) — against the memory share a seat declares
   (`--palw-host-memory-share`), and the time to page the artifact in. A seat whose share is below the replay never becomes
   ready and, today, says so nowhere (§II.7.3 F4); the preflight is where it is said first.
9. **The forecast.** The path the lifecycle (§II.7.1) would take on the network: an admission jury at the next audit
   (`palw_admission_audit_period_daa`), `required_ready_seats` ready seats, `probation_claims` probe claims, `stable_epochs`
   stable spans, how many independent operators the network has against the seating floor (§II.7.5), and the earliest time to the
   first paid claim. It is informational: the chain decides.

##### II.2.4 Verdicts, stages and codes

The verdict is one table, one row per stage, in the order the user meets them:

| Stage | The question | Blocked by |
| --- | --- | --- |
| **convert** | Can an artifact be built from this source, reproducibly, by anyone? | architecture and features (Level C), config keys, quantisation, tensors, tokenizer, completeness of the source |
| **register** | Would the chain admit this class at this height? | admission sizing, the court's conditions, the court window, the canonical job, the fences |
| **mine** | If it is registered, will it be admitted to claims and paid? | seat resources, ready seats, the lifecycle path, conformance (full depth) |

Each stage is `ok`, `blocked` or `unknown` (depth too shallow). A blocker is
`{ stage, code, what, evidence, have, need, safe_paths }`; `have` and `need` are present when the condition is a count or a
size. Codes are stable once published; the initial set (`SCREAMING_SNAKE`, the registry's own `PalwModelRegistrationCodeV1`
style, and the registration codes it already has are reused where they mean the same):

| Stage | Codes |
| --- | --- |
| convert | `ARCH_NEEDS_FEATURE(feature)`, `ARCH_NEEDS_PRIMITIVE(name)`, `ARCH_REFUSED(feature, why)`, `CONFIG_KEY_UNREAD(key)`, `CONFIG_INVALID`, `REMOTE_CODE(module)`, `QUANT_NO_DESCRIPTOR(scheme, id, name, tensors)`, `QUANT_KNOWN_UNDESCRIBED(method)`, `QUANT_REFUSED(descriptor, why)`, `TENSOR_MISSING(name)`, `TENSOR_SHAPE(name, want, got)`, `TOKENIZER_MISSING`, `SOURCE_INCOMPLETE(file)` |
| register | `ADMISSION_EXCEEDS(limit, value, cap)`, `CLOSE_SIZE_OVER_CAP`, `COURT_COST_OVER_CEILING`, `DA_LADDER_EXCEEDED`, `COURT_WINDOW_EXCEEDED`, `CANONICAL_JOB_OUT_OF_BOUNDS`, `FENCE_NOT_ARMED(name)`, `ARTIFACT_ROOT_KNOWN` (full depth) |
| mine | `SEAT_MEMORY_SHORT`, `READY_SEATS_INSUFFICIENT` (a class already on the chain), `PACK_NOT_VERIFIED` (full depth) |

Not every note is a blocker. The feature scope (§II.6) is reported as a note, not a block: a vision-language model is registrable
as its text decoder, and the report says so (`text stage only on testnet-12; vision needs RFC-0003's generative class`).

##### II.2.5 Safe paths

A refusal says what exists instead, taken from data rather than prose: for `QUANT_NO_DESCRIPTOR`, the model's original
safetensors, or re-quantising to a described type (listed), or supplying a descriptor file with `--quant-format` (§II.5.5); for
`ARCH_NEEDS_FEATURE`, the feature's name and the general primitive that would close it (lane G's `FeatureInfo`) and whether a
data adapter can supply a missing key mapping; for `COURT_WINDOW_EXCEEDED`, a smaller `max_context`, the class-specific window
where the build has it, and the numbers each would give; for `SEAT_MEMORY_SHORT`, the context at which the class fits the seat.

##### II.2.6 Plug points

- **The court window.** The derivation of a class's court window arrives with release int-10 (lane R, `rcore/int-10`). Until a
  build has it, the preflight prints the *global* rule (`window_court` against the dissection depth the class needs) and says
  `court window: global rule; this build has no class-specific window`. The window is a provider (`needed`, `limit`,
  `source`) so that merging int-10 replaces one function.
- **The network.** Parameters (`Params`), fences and registration terms come from the named network preset or, with `--node`, from
  the node (`getPalwRegistrationTerms`), so a live chain's terms — which differ from genesis after a retarget — are the ones
  judged.
- **The seat share.** The default tiers are the testnet-12 fleet's: a memory share of 3.5 GiB (the 5.104 seats, MemoryMax 9 GiB) and 8 GiB (the ibm/.113 seats, MemoryMax 16–20 GiB); `--seat-share [name=]GiB` (repeatable) replaces them.

##### II.2.7 The JSON form

`--json` prints `misaka.palw.preflight.v1`: the mode, depths, the network and height; `source` (files, sizes, SHA-256 of the
config and each header); `model` (architectures, Level, adapter id and hash, features); `scope`; `storage` (histogram, descriptors
by name and digest); `artifact` (estimates); `chain` (each condition with `needed`, `limit`, `source`); `seat`; `forecast`;
`verdict` (the stage table); and `registries` (the adapter-pack hash, the feature-registry digest, the quant-registry digest and
the tool's version), so a verdict is attributable to the build that gave it. It contains no timestamp and no path: the same
inputs give the same bytes.

##### II.2.8 What a preflight does not claim

It does not claim fidelity (that needs the weights: §II.4's reference fit), it does not claim the artifact root (that needs the
data), and it does not claim that the chain *will* admit the class: it computes the chain's own functions at a height, and the
chain decides when the object is carried. It claims only what the headers and the rules say, and says which.

##### II.2.9 As built (2026-10-01), and what this slice leaves out

`misaka_palw_sdk::preflight` is the library; `palw-class preflight <model>` and `misaka model preflight <model>` are two faces
of it (the same report, `--json` the same bytes). Where the build differs from, or decides within, the text above:

- **Dispatch.** A directory with `config.json`, a `config.json` file, a `.gguf` file (or a directory holding only a
  `model.gguf`) is a model; a `.palwtir` file is an artifact and keeps its old meaning in both commands (`palw-class preflight`
  runs the artifact admission; `misaka model preflight` asks the live chain). The first line says which ran.
- **Exit code.** 0 when no stage is blocked and the convert stage is `ok`, 2 when a blocker exists, 1 for an input the command
  cannot read.
- **Network and height.** `--network` defaults to `testnet-12` for a model; `--depth headers` needs none. With no `--height` the
  height is the last DAA `Params::fence_schedule_v1` lists (every fence the network schedules is in force there) and the report
  prints how it was chosen. `palw_tir_v1` is read from the network's real schedule: below its activation, or on a network that
  has not armed it, the register stage carries `FENCE_NOT_ARMED(palw_tir_v1)` and **every other condition is judged as if it
  were armed** (at the fence's own height, or DAA 1), so a model learns what else stands in its way before the flag day.
- **The declared context is searched.** A class is declared at a context; `declare-layout` defaults to the widest the program
  and the network admit, which no one can register (past 32,783 positions the canonical prompt exceeds J5b's 4,096 inline ids).
  The preflight, given no `--max-context`, declares the **widest context at which admission v10 admits the class** (the court
  window and the inline bound cap the search, then halving and bisection on the gate itself) and says so, with the refusal the
  program's own widest context meets. `--max-context N` judges exactly N.
- **Every wall at once.** The gate refuses at the first wall it meets; the preflight also computes the court window, the canonical
  job and the sizing on their own, and any that is over its limit is a blocker too, so fixing one wall is not mistaken for
  fixing the model (ADR-0097's rule). Each condition is `needed / limit / unit / ok`.
- **Shape-only lowering.** The program is the lowering of the configuration (`prepare_spec`, which reads no weights); the
  parameters' shapes are checked against the headers by the frontend's own binding (`check_weights`; `check_names` when only the
  index is present). A described format that derives a shape from a small role tensor (at most 4 KiB) whose data is not on disk
  is reported as `not checkable` with the tensor to fetch, never as a wrong shape.
- **The artifact estimate** is `palw_tir_work_shape_v1(program).param_bytes()` — the registry's own figure for the class's
  parameters — plus the program and the tokenizer files; the download figure is the tensors the class reads, minus what the
  scope leaves out. The inventory's leaf count is an estimate (it only sets the depth of a close's paths).
- **The seat.** `needed = artifact (mapped) + state at the declared context + one position's peak live bytes + the widest tile a
  close opens`; the report gives the need against each seat tier (the fleet's two, or `--seat-share`) and says which tiers hold the class and which never become ready for it; `SEAT_MEMORY_SHORT` is raised only when no tier holds it, with the context at which each fits; a class only some tiers hold is a note.
- **The forecast** is `palw_derive_profile_v1` over `palw_tir_model_work_v2`: the registry's own derivation of the window,
  prefetch, ready seats, in-flight claims and registration bond. It does not know how many independent operators the network has.
- **Tokenizer.** `TOKENIZER_MISSING` is raised for a directory (no `tokenizer.json`, `tokenizer.model` or `vocab.json`) and for a
  GGUF without `tokenizer.ggml.tokens`; a lone `config.json` says nothing about it.

**Not in this slice**, each with where it plugs in: `--node` (the live chain's registration terms and tip, and with them
`READY_SEATS_INSUFFICIENT` for a class already on the chain); a repository id by HTTP ranges (the `remote` feature of the
lowerer's `RemoteCheckpoint` is the reader; no network is used by any test); the `full` depth, which reports what it needs
(`palw-tir-fidelity`, `pack build`, `pack verify`) and `PACK_NOT_VERIFIED` / `ARTIFACT_ROOT_KNOWN`; and the class-specific court
window, which replaces one function (`window_at` in `preflight::chain`) when release int-10 merges.

#### II.3 The streaming loader and the converter (R2)

A conversion that must hold a checkpoint in memory cannot be run on a 360 GB model, so the converter is built around one rule:
**the resident memory of a conversion is a block of a tensor, never a tensor and never the checkpoint.**

##### II.3.1 `TensorSource`

The loader's face is a trait (`weights::TensorSource`). Beyond reading a whole tensor it serves the three things streaming needs:
`metadata(name)` (a header lookup, no data), `read_slice(name, range)` (a byte range of the tensor's stored data) and
`load_rows(name, rows)` (a row range decoded to `f32`); `serves_row_ranges()` says whether the last reads only what it is asked
for. The sources are:

| Source | Serves ranges by |
| --- | --- |
| sharded safetensors (`Checkpoint`: `model.safetensors.index.json` and the shards) | `pread` of the shard file at the header's offsets |
| GGUF | the header's data offsets; block-quantised tensors decode through their descriptor (§II.5) |
| a *described* source (`DescribedSource`) | a wrapper that hides tensors a descriptor stores packed and serves the float tensors it declares in their place (§II.5.3), decoding any element range without materialising the rest |
| remote (`CurlFetcher`, cargo feature `remote`, off by default) | HTTP range requests; the library never opens a connection by itself and no test does; the feature is tested against a loopback server |

##### II.3.2 Streaming materialisation and the converter

- **By blocks of rows.** `materialise_stream` produces each parameter of the program a block of rows at a time, evaluating a
  source row-wise where its form allows (a slice, a transpose, a stack of experts); projections whose input has outlier channels
  split their rows by the calibration statistics in the same pass. What a fill cannot evaluate row-wise (a few small tensors) is
  read whole, and the loader counts them.
- **Content-addressed chunks.** The container (`PALWTIR1`) is assembled from canonical 4 MiB chunks in a chunk store, streamed:
  the same bytes are the same chunk. A composite class (a parent and an unmerged LoRA adapter, RFC-0004) therefore *shares its
  parent's chunks*, and an updated adapter rewrites only its own.
- **The inventory root by leaf.** The inventory root is computed leaf by leaf over a container or a chunk store (`derive_streamed`),
  equal to the consensus root; a 74 MiB container is rooted with 4 MiB resident.
- **Calibration** produces the activation statistics (`misaka.palw.calib-stats.v1`) the quantisation policy needs. They are a
  bit-exact file (a decimal does not round-trip through every JSON reader), a pinned input of the pack (§II.4), and re-measurable
  from the calibration sequences the pack carries.
- **`convert_model(ConvertRequest) -> ConvertOutcome`** is the converter as a function; `palw-tir-convert` and `palw-class pack
  build` are thin wrappers. The container's meta records what read the model — the frontend, the scope, the descriptors, the math
  — and enters the file digest, never the roots.

##### II.3.3 Reproducibility

A conversion is a function of the pack's inputs and of nothing else:

- **Math.** Every transcendental the lowerer evaluates (activation and RoPE tables, scales, query temperatures, SSM decay) and
  the log2 of its scale arithmetic come from `detmath`: the pinned pure-Rust `libm` 0.2.8 (`math: "libm-v1"`, the default of a
  new pack), so a seat on any platform rebuilds the same root. A class registered before it records `math: "std"` and the
  platform it was built on (os/arch), and stays reproducible there. Measured on the 68 fixtures on macOS arm64 (`std` against
  `libm-v1`, one set of statistics): **0 of 14,688,768** activation-table entries and **0 of 4,141,056** RoPE-table entries
  differ; 2,146 of 6,577,152 entries of every other tensor differ, all in the query-temperature table of Llama-4, Llama-4-VLM
  and Ministral-3, by at most 8 codes (one binary32 ulp of `ln_1p` at Q24). The platforms' `f64` libms agree on every table
  here; their binary32 ones do not, and another platform's `f64` is untested, which is why the math is pinned.
- **Threads.** The conversion uses rayon; `tests/determinism.rs` rebuilds nine families (RoPE variants, hybrid, SSM, MoE,
  LongRoPE, YaRN) under pools of 1 and 7 threads, and the statistics and container bytes are equal.
- **Equality with the whole-tensor path.** The streamed artifact is byte-identical to `materialise` on all 68 Hugging Face
  fixtures, and `tests/golden_lowering.rs` pins the program and artifact digests of 92 checkpoints and 63 real configurations: a
  change in the lowering that moves a root is noticed by that test, and `LOWERING_VERSION_V1` is bumped when it is meant.
- **Memory.** `tests/streaming_budget.rs` converts a synthetic sharded Llama-shaped checkpoint (default 24 layers, about 270 MB of
  BF16 in many shards; `TIR_BUDGET_LAYERS=100` is a 2 GB-disk run) under a counting allocator: the peak above the process's own
  memory stays inside a budget (default 32 MiB) that is below the embedding table alone as `f32`, and no tensor deferred to the
  streaming path was loaded whole.

The known limits of this section (the float reference of calibration is not row-streamed; conformance on a very large model
loads the artifact whole on each executor) are in §II.9.

#### II.4 Runtime packs (R2)

A **runtime pack** (`misaka.palw.runtime-pack.v1`) is the one document that says what is needed to rebuild a class's artifact
from public source and to judge the result. It is a directory: `pack.json` and the sidecars it pins by hash (the calibration
statistics, the calibration tokens, the Hugging Face reference logits, a user-supplied adapter, user-supplied descriptors).

##### II.4.1 Identity and form

- **Identity.** `pack_digest` is BLAKE2b-256, keyed (`misaka.palw.runtime-pack.v1`), over the canonical JSON of `pack.json` (keys
  sorted, compact, integral floats as integers). The file may be pretty-printed; the digest does not depend on it. Nothing in the
  manifest depends on the machine, the time or the thread count.
- **Numbers.** Every number that is not an integer — the quantisation headroom, the logit scale, the tolerances, the measured
  fit — is stored as the 16 hexadecimal digits of its IEEE-754 binary64 bits, because a decimal does not round-trip through
  every JSON reader (serde_json's default parser is one ulp off on some values) and a digest must not depend on the reader.
- **Strictness.** Unknown fields are an error (`deny_unknown_fields`); a sidecar named by a path with `..` or an absolute path is
  refused; hash lengths and character sets are checked before anything is trusted.

##### II.4.2 The sections

| Section | Pins | Why it is there |
| --- | --- | --- |
| `model` | the source: format (`safetensors` / `gguf`), label and revision (informational), architectures, `model_type` (informational), the digest of the configuration the frontend read, and every file's size and SHA-256 | the source is public, so anyone can fetch it and the hashes say it is the same bytes |
| `frontend` | the adapter (`none` for Level A, `built-in` or `user-file`, by id and BLAKE2b-512 hash; the template for Level A), the built-in pack's hash, the digest of the `ModelSpec` the lowering started from, the Level, and the configuration keys the class defaults supplied | a different reader is a different pack |
| `features` | the digest of the feature vocabulary of the build, the features used (`{id, layers, detail}`), and the scope (§II.6) | what the class computes and what it leaves out |
| `quant` | the descriptors, by name and digest (built-in, or a file inside the pack) | the weights are read the way the descriptor says (§II.5) |
| `profile` | the quantisation policy (headrooms), the maximum window, the context, the calibration (statistics digest and file, where measured, optionally the sequences) | the numbers that decide the integer program |
| `converter` | name, crate version, the lowering version (`misaka.palw.lowering.v1`), the math (`libm-v1`, or `std` with its platform) | the tool and the arithmetic |
| `executor` | the program format, `prim_set_id`, and the implementations that must agree byte for byte | what "correct" means (§II.4.4) |
| `logits` | the convention (`legacy-greedy-only`, or `q24-natural-v1`: code / 2^24 are natural-log logits, the scale exactly 2^-24), the scale and the tolerance (slope, correlation, top-1, KL) | what the integer logits are in |
| `result` | the container's file digest and size, `graph_ir_root`, the inventory root, the leaf count, the tokenizer id, the program size | what the build produced |
| `conformance` | the vectors: a prompt, the greedily decoded tokens, the number of positions, and two digests — over every position's logits and over every commit point | what any executor must reproduce |
| `hf_reference` | the Hugging Face reference (its file and digest, what produced it, sizes) and the fit the integer program measured against it (slope, intercept, correlation, max-abs, RMSE, top-1, KL) | the fidelity claim (never a bit-identity claim: Part I §1.2(5)) |
| `declared` | per network: the layout digest, the class id the layout gives, `max_context`, checkpoint interval, `h_tile`, and the file digest of the container written with it | the class the artifact becomes on that network |
| `files` | the sidecars, by BLAKE2b-256 | |

Adding a model never inherits another model's conditions because **there is nowhere to inherit them from**: every condition that
shapes the artifact — adapter, descriptors, policy, statistics, math, executors — is a field of this model's pack and so of its
digest, and verifying a pack reads nothing from any other.

##### II.4.3 The commands

```
palw-class pack build  --model <hf dir | .gguf> --out <artifact.palwtir> --pack <dir> (--calib <tokens.json> | --stats-in <stats.json>) …
palw-class pack verify <pack dir> [--model <dir | .gguf>] [--artifact <file>] [--rebuild] [--no-ref2] [--no-exec] [--strict] [--json]
palw-class pack show   <pack dir> [--json]
```

`build` converts (streaming, `libm-v1` by default), writes the artifact and the pack, runs the conformance vectors on every
executor, holds the program to the reference within the tolerance, and, per `--declare <network>`, declares the class.
`verify` checks every claim it can from what is at hand and reports **PASS**, **FAIL** or **SKIPPED** (not checkable here, and
why) for each: the manifest and sidecars by hash; the source files by hash; the adapter and the descriptors by hash; the
frontend by reading the model again (the spec digest, the features, the scope); the artifact, rebuilt from the source with the
pack's profile and statistics (`--rebuild`) or the one supplied (`--artifact`), by file digest, inventory root, graph root and
tokenizer; the declared classes by class id; the conformance vectors on every executor; and the program against the Hugging Face
reference within the tolerance. A pack is **VERIFIED** only when nothing failed and nothing was skipped.

##### II.4.4 Independent conformance

The conformance vectors are the independent check of A2. Each is run on three implementations, which must give the same bytes at
every position's logits and at every commit point `(slot, block, layer, node, values)`:

1. the reference evaluator (`misaka-palw-tir`, Part I §6: the meaning of "correct");
2. the typed backend that nodes run (`misaka-palw-tir-exec`);
3. the independent second implementation (`misaka-palw-tir-ref2`), written from the specification alone, decoding the canonical
   bytes with its own codec.

The Hugging Face reference fit is a *unit check* as much as a fidelity measure: the least-squares slope of the reference logits on
the program's `code × scale` must lie in `[slope_min, slope_max]`, which fails a program whose logits are in other units than
natural-log logits (a slope far from 1).

##### II.4.5 What a pack is not

A pack is not a chain object and no consensus code reads it (P7). It is the evidence a person gives so that others can reproduce
a class without trusting them; the chain's evidence is the court. A registration's artifact root is the same value whether or not
a pack exists; the pack is what lets a seat, an auditor or a competing registrant rebuild that root and check that the executors
agree before spending a fee or a gigabyte on it.

---

<a id="source-v1-0002-L01418"></a>

### 旧RFC-0002 / 原文1418–2078行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: TIR文書内のonboarding手順をモデル登録RFCへ。

#### II.6 Feature scope (R4)

A registered class is **one function**: token ids in, next-token logits out (for an encoder, a pooled embedding). A model that also
takes images or audio, or proposes several tokens at once, has parts the class leaves out. The *scope* says which, with the
evidence and the reason, wherever the class is shown, so that a text-only class is never presented as the whole model — and never
as a partly multimodal one: **a class computes a part entirely or says that it does not.**

- **The object** (`misaka.palw.feature-scope.v1`): `task` (`text-generation` or `text-embedding`), `input`, `output`, `text_only`
  (the model has a modality the class does not take), and `excluded`: for each left-out part its `kind`, `what`, `why`, whether it
  is `modal` (an input the class cannot take, as against a head it can do without), the `evidence` that showed it
  (`config.<key>`, `tensors <prefix>*`, `file <name>`) and the tensors and bytes it holds.
- **The rules are data** (`scope/rules.v1.json`, `misaka.palw.scope-rules.v1`): configuration keys, tensor-name prefixes or
  substrings and file names beside the checkpoint that are evidence of a part. Nothing selects code by model name. The kinds are
  `vision` (the tower; `vision_config`, `image_token_id`, …; `*mmproj*.gguf`, `preprocessor_config.json`), `projector` (the module
  that maps vision or audio features into the decoder's space), `audio`, `mtp` (multi-token-prediction heads) and `draft`
  (a speculative-decoding draft network, Medusa/Eagle heads).
- **Where it is shown.** The architecture report; the container's provenance meta; the runtime pack's `features.scope`
  (§II.4.2); `misaka model inspect`; the listing; and the preflight (§II.2.3 item 2), with the bytes of the tensors the class does
  not need, which a download may skip.
- **What it says for a vision-language model.** *Text stage only on testnet-12; vision needs RFC-0003's generative class
  (`palw_gen_v1`).* The class is the text decoder (RFC-0003 §II.2.1 specifies the placement of image rows an LM reads); the tower
  and the projector are named, and an `mmproj` file is an unsupported feature, not a partial one. A user who needs the modality
  sees at once that this class cannot give it.
- **It is a note, not a block.** A vision-language model is registrable as its text decoder; the report says what it is and what
  it is not. It becomes a block only where a user requires a modality (`preflight --require vision`).

#### II.7 Staged enablement: convert, register, mine (R5)

The requirement is that **registering a model and enabling mining for it are different acts**, and that when either does not
happen the user is told which stage lacked what. §II.7.1 states the lifecycle the chain has today; §II.7.2 what it already
guarantees; §II.7.3 where it falls short of R5 (found by reading the code, with the functions named); §II.7.4 the node-only change
that makes every stage visible; §II.7.5 the consensus rules that were decided to close what is missing.

##### II.7.1 The lifecycle today

`PalwModelLifecycleV1` (`palw_model_registry_v1.rs`) is stepped once per span by the registry fold from chain-visible facts
(`step_model_registry`, `palw_lifecycle_step_v1`). On testnet-12 a panel has 5 seats, `required_ready_seats` is the panel plus
2 spare, 7 (`palw_seat_gate_possession`), and the audit period is 100 DAA.

| State | Admits claims | Leaves when | The stage that lacks something |
| --- | --- | --- | --- |
| `Registered` | no | the manifest verdict is `Valid` (the graph names only operations the VM defines and derives work) | **convert** — the artifact's graph is the problem |
| `Candidate` (a bought class, past `palw_admission_independence`) | no | an admission jury seats: `seat_count` operators drawn from the *network's* population, not the class's, excluding the registrant's bond and operator, and a majority of them hold the class ready (ADR-0147); one audit per period | **register** — the network has not admitted it |
| `Prefetching` (a genesis class starts here) | no | `ready_seats ≥ required_ready_seats` and the collateral is there | **mine** — possession |
| `Probation { probes_passed }` | 50 ‰ of the derived admission | `probation_claims` (10) probe claims reach `Final` with none failing (a failure resets it; past `palw_registry_resilience` only two distinct bonds' failures do) | **mine** — verified service |
| `ActiveLimited { stable_epochs }` | 100 ‰ | `stable_epochs` (3) stable spans and the class's cap utilization under the ceiling | **mine** — stability |
| `Active` | 1,000 ‰ | falls to `ActiveLimited` if the cap is exceeded, to `Held` if the panel cannot be drawn or the class overloads | — |
| `Held` | no | the seats are back and the class is not overloaded; it re-enters `Probation` | **mine** — seats or the receipt window |

A *ready seat* is a bond with a fresh readiness row: a possession proof of a challenge leaf of the artifact's inventory that rotates
every span (readiness V2; 24 spans on testnet-12), an active bond above the floor, and free collateral for the readiness multiple
(3 seat exposures). One predicate, `palw_seat_not_ready_reason_net_v1`, counts ready seats for the lifecycle, the jury's votes, the
panel room and the registry RPC.

##### II.7.2 What it already guarantees

- **Registration is existence.** `Registered`, `Candidate` and `Prefetching` admit no claim. The claim gate
  (`class_lifecycle_refusal`) refuses a claim to a class whose state does not admit claims, the model market follows the same
  gate, and the producer asks it before it spends an inference. A class nobody serves earns nothing and carries no weight.
- **Static court feasibility is checked at registration.** Admission v10 checks types, ranges, costs and court cones (Part I §5); existing weight rules also require a certified family covering the program's primitives (`NotEndToEndCertified`; `palw_uncertified_weightless`). These are the existing gate's scope, not proof that a fresh public bond can acquire evidence and prosecute every admitted profile. Future reward/weight admission additionally requires RFC14's public-verifier dispute-completeness gates under ADR173.
- **Independence, once.** A bought class passes a jury of operators the network drew (ADR-0147), at one audit per period, so a
  registrant is admitted alone only with the probability that its share of the operator lottery wins a majority of the jury.
- **Possession.** Ready seats hold fresh possession proofs with collateral behind them.
- **Volume.** A new class is admitted at 50 ‰ and then 100 ‰ of its derived admission before it is `Active`.

##### II.7.3 Where it falls short of R5

**F1. "Ready seats from distinct operators" is a count of bonds after `Candidate`.** The registry fold observes `ready_seats` as the
number of *bonds* with a ready row (`model_registry_ready_seats`), and `Prefetching → Probation` needs it to reach
`required_ready_seats`. Distinctness of operators is enforced in one place only: the admission jury. On testnet-12,
`palw_operator_id_unique` ("one operator identity, one bond", with a proof of possession of the operator key in the registration)
makes a bond and an operator identity one thing, so the bond count there *is* a count of distinct operator identities. Three
things remain. (i) On a network without that fence the lifecycle over-counts, while the panel room's own count
(`palw_panel_ready_eff_of_bonds_v1`) already groups by operator and caps weights: the two disagree about what a ready seat is. (ii)
The jury's independence is a snapshot at one audit: after `Candidate` nothing asks again whether the ready seats are anyone but the
registrant's own keys, so a registrant that holds seven bonds, each a distinct key, keeps its class in `Probation` alone once its
jury has once been seated and the outsiders' readiness has lapsed (24 spans). (iii) The chain cannot tell two keys of one party from
two parties: independence is economic (the collateral behind each key), never an identity. And the rule is now **two rules**: lane D's
RFC-0003 lane admits a tensor claim only once `seat_count` distinct operators, never the executor's, hold the class with a fresh readiness
V2 proof (`GenClassNotReady`, RFC-0003 §I.4.5, `rfc3/gen-claim`), while an attempt class counts bonds and a composite class follows the
attempt lane. How many *independent* operators should gate the first claims, and one rule for every class kind, is §II.7.5 Proposal A.

**F2. Probe claims are paid.** `Probation` admits claims at 50 ‰ of the derived admission and a probe *is* an ordinary attempt claim:
one that reaches `Final` is counted as a pass (`note_model_probe`) and is paid and weighed as any other claim of the class: the
lifecycle only scales how many claims are admitted (`admission_permille`). `admits_claims()` is the only gate and the registry has
no unpaid state (ADR-0145 §7: "probation bounds volume, not price"). So rewards begin the span the class enters `Probation`, as soon as `ready_seats ≥ required_ready_seats`, before one claim
of the class has been verified and before any evidence about a seat's *executor* beyond a possession proof.

**F3. There is no executor-certification gate.** What the chain checks about whether a class can be executed correctly: statically at
registration (above), and by the primitive-set certificate (`certify_tir_e2e_family_v1`: the court acquits an honest run and convicts
planted faults at every primitive the program uses). Readiness is *possession* plus collateral and says nothing about whether a
seat's executor reproduces the class. The first evidence is the probe claims, in band and paid (F2). A divergent executor is *safe*
(the court convicts the party that computed wrongly, Part I §6) but not *free*: an artifact that does not match its own program, or an
executor that mis-implements a primitive, voids claims, resets probation, holds the class and wastes the honest producers' work. The
pack's conformance vectors (§II.4.4) catch these before registration; nothing makes registration, or readiness, depend on them.

**F4. A seat's resource failure is silent.** A seat whose memory share is below the class's replay writes a line to its own log and
never becomes ready (the 8k class needs 3.37 GiB per full-seat replay: 1.68 of artifact and 1.67 of trace scratch). The class never
leaves `Prefetching`, and the registry says only `ready 0 < 7 required (seats prove possession to be counted)`.

**F5. The registry's reasons are prose.** `RpcPalwModelLifecycle.reason` is a sentence from `palw_lifecycle_reason_v2`: no stage, no
code, no structured count, and the `Candidate` sentence describes the jury rule loosely. `getPalwModelRegistrationStatus` has stable
codes for the carrier's pipeline (constructed, submitted, accepted, included, folded) and nothing after `folded`.

##### II.7.4 The node-only change: a structured `blocking` in the registry

`getPalwModelRegistry` gains, for each class, **what is blocking it and at which stage**. It reads only what the node already
serves, so it needs no consensus change, and its derivation is one pure function over a class's row and the registry's globals
(`palw_registry_blocking`, in `rpc/core`), used by the node to fill the response and by the CLI to show an older node's registry the
same way.

```
RpcPalwClassBlocking {
  class_id, stage: "convert" | "register" | "mine" | "",     // "" : nothing blocks
  code,                                                         // stable, SCREAMING_SNAKE
  what,                                                         // one sentence: what is missing
  have, need,                                                   // present where the condition is a count
  next,                                                         // what lifts it
}
```

| State (and condition) | Stage | Code | have / need |
| --- | --- | --- | --- |
| no row, or the base class | — | (none: legacy, never gated) | |
| `Registered`, `ops_supported` false | convert | `KERNEL_EXTENSION_REQUIRED` | |
| `Registered`, no artifact bytes or no derived work | convert | `NO_WORK` | |
| `Candidate` | register | `ADMISSION_JURY` | ready seats now / the jury quorum (`seat_count / 2 + 1`); the jury counts only its drawn operators |
| `Prefetching` | mine | `READY_SEATS` | `ready_seats_now` / `required_ready_seats` |
| `Probation { probes_passed }` | mine | `PROBATION` | `probes_passed` / `probation_claims` |
| `ActiveLimited`, spans below target | mine | `STABLE_SPANS` | stable spans / `stable_epochs` |
| `ActiveLimited`, spans reached | mine | `CAP_SATURATED` | |
| `Held`, ready seats below a panel | mine | `PANEL_NOT_DRAWABLE` | `ready_seats_now` / `seat_count` |
| `Held`, below the required seats | mine | `READY_SEATS` | `ready_seats_now` / `required_ready_seats` |
| `Held`, seats back, window does not fit | mine | `WINDOW_DOES_NOT_FIT` | |
| `Active` | — | (none) | |

`KERNEL_EXTENSION_REQUIRED` waits for the unsupported operation's reviewed kernel extension to be
implemented and activated; then the model must be lowered and registered as a new class (ADR-0172).

`what` carries the numbers a reader needs (`3 of 7 seats hold a fresh possession proof; 14 bonds on the network have the
collateral headroom to be a seat`; `4 of 10 probe claims have reached Final; 0 failed since entry`), and `next` says the act that
lifts the block (`run a seat for this class: a seat needs 3.37 GiB`, `the next admission audit is within 100 DAA`). The wire:
`GetPalwModelRegistryResponse` moves from version 1 to 2 and **appends** `blocking: Vec<RpcPalwClassBlocking>` after its last field;
an element of `classes` is not extended, because it is read as a vector element and a longer one would misalign an older reader,
while a trailing field of the response is not read by one. The CLI shows it in `misaka palw registry` (a line under each class
that has one) and in `misaka model status <class>` (`stage`, `blocked by`, `have/need`, `next`), and it is in `--json`.

Reserved for Proposal A (§II.7.5): `INDEPENDENT_OPERATORS` (`have / need` independent operators) and the class's licensable share,
served once the consensus read that carries the seating facts exists.

This closes F5 and the *reporting* half of F4 (the registry names `READY_SEATS` and the preflight, §II.2.3 item 8, names the
memory). It changes nothing the chain decides.

##### II.7.5 The consensus rules — historical decision and implementation status at 2026-10-01

**Current reading (2026-10-10):** the seating predicate and dormant fence now exist in
`palw_class_seating_v1.rs` and `palw_class_seating_fence_v1.rs`. The following “not built” statements
are the original decision record. Presence of those modules does not prove activation or A7's
independent registration-to-Final exercise; see the implementation ledger for verified evidence.

The coordinator decided on 2026-10-01: **Proposal A approved** (one seating rule for every class kind, under the new dormant fence
`palw_class_seating`; the independence floor a fence parameter, default 3; armed with the RFC-0003/0004 flag day, not with int-10);
**Proposal B: go**; **Proposal C: C1**; and **`misaka model add` requires a `pack verify` PASS by default**, with an explicit expert
override flag that prints a warning (the chain does not enforce it). The consensus rules below are built by the consensus lanes
(lane D owns the shared predicate; this lane specifies it here, writes the scenarios the drills must include, and builds the
node-side reporting and the seat duty). None of the consensus rules is built yet. *(Corrected 2026-10-12: Proposal A is built —
`palw_class_seating`, armed on testnet-12 at DAA 5,300 with `independent_floor = 3`; see the [register](../design/palw/integration/mainnet-spec-register-2026-10-12.md) N1/N2.)*

All of them are **fences**: dormant on every preset (`None`, hashed Some-only, a dormant network fingerprints as if the rule did not
exist), armed on testnet-12 through a flag-day entry like `PALW_T12_TIR_FENCE2_ENTRY`, with a drill that crosses the fence on the
shipping binary.

###### Proposal A — one seating rule for every class kind (approved)

A fence `palw_class_seating`. Past it there is **one predicate**, asked at every door through which a class's work is admitted.

> `class_seated(class, executor, span)`. Let `Ready(class, span)` be the set of **operators** — an operator counted once, however many
> bonds it holds — with a fresh readiness V2 possession proof for the class, by the registry's own five-clause predicate (an active
> bond above the floor, a fresh row, free collateral for the readiness multiple), and `Base(span)` the network's base-class
> population as the admission jury and the outsider seat draw it (active bonds above the floor that serve the liveness floor,
> matured, registered before the span's cut). The class is **seated** for a claim whose executor is `e` iff
>
> 1. **possession floor** — `|Ready \ {operator(e)}| ≥ seat_count`: a panel can be drawn without the executor;
> 2. **independence floor** — `|(Ready ∩ Base) \ {operator(registrant), operator(e)}| ≥ independent_floor`, where `independent_floor`
>    is a **parameter of the fence** (default `quorum(seat_count)` = `⌊seat_count / 2⌋ + 1` = **3** on testnet-12): operators that are
>    neither the claim's executor nor the class's registrant and that the network's own lottery could have drawn. A genesis class has no
>    registrant to exclude.

**The parameter.** `independent_floor` lives in the fence's ceilings (`PalwClassSeatingCeilingsV1`), hashed into the params
fingerprint with the fence (Some-only), so it can be raised by a later flag-day entry and never moves under a running chain.
`validate_palw_v2` refuses a floor of 0 or above `seat_count` (a floor no panel could meet). Three is the default because it is the
number of operators the network already accepts as evidence of independence when it admits a bought class (the jury's strict
majority), made a *continuing* condition; the reasoning is below.

**Where it is asked** (one function, so the doors cannot disagree):

| Door | Today | Under Proposal A |
| --- | --- | --- |
| claims of an IR or legacy attempt class, bought or genesis | the lifecycle state admits claims (`Prefetching → Probation` at `required_ready_seats` ready *bonds*); a claim no panel can be drawn for is voided *after* the bind window as `NoCapablePanel`, the executor's reservation held meanwhile | `class_seated` at claim admission: a refusal (`ClassNotSeated`, naming the floor that failed) before the reservation is taken |
| a tensor claim of an RFC-0003 generative class | lane D's `GenClassNotReady`: `seat_count` distinct operators, never the executor's, hold the class with a fresh readiness V2 proof (`rfc3/gen-claim`, `9a25e6d20` / `28f60cf24`); its lifecycle row stays `Registered`, so this is its only gate | the same function: D's rule *is* condition 1; condition 2 is added |
| claims of a composite class (a parent and an unmerged adapter) | as an attempt class, on the composite's own class id and artifact root | the same function, on the composite's readiness rows |
| the lifecycle | `Prefetching → Probation` and `Held → Probation` read the bond count against `required_ready_seats` | they read `|Ready| ≥ required_ready_seats` (`seat_count` + spare, **7** on testnet-12) **and** condition 2: the class enters `Probation`, and its probe claims begin to be paid, only when seven distinct operators, three of them independent, are ready |
| the registry, the CLI and the preflight | `blocking` (§II.7.4) names `READY_SEATS` with a bond count | `blocking` reports `have / need` for each condition (`INDEPENDENT_OPERATORS`), and the registry reports the class's licensable share; the preflight's forecast says how many independent operators the network has |

**The shared function — the API proposed to lane D.** One implementation in `consensus/core` (`palw_class_seating_v1.rs`), pure over the
state, with no side effect. Lane D's inline loop in `apply_gen_tensor_commitment_v1` becomes a call to it.

```rust
/// The fence's terms in force at a DAA (`Params::palw_class_seating_terms_at`): `None` below the fence.
pub struct PalwClassSeatingTermsV1 { pub independent_floor: u16 }          // default 3

/// What the predicate counted: every number a refusal, the lifecycle, the registry RPC and the preflight need.
pub struct PalwClassSeatingV1 {
    pub ready_operators: u32,        // |Ready \ {operator(executor)}|                       condition 1, have
    pub needed_operators: u32,       // seat_count                                           condition 1, need
    pub independent_operators: u32,  // |(Ready ∩ Base) \ {operator(registrant), operator(executor)}|   condition 2, have
    pub needed_independent: u32,     // terms.independent_floor                              condition 2, need
    pub base_operators: u32,         // |Base \ {operator(registrant)}|                      the licensable share's denominator
}
pub enum PalwClassNotSeatedV1 {
    Possession { ready: u32, needed: u32 },
    Independence { independent: u32, needed: u32 },
}
impl PalwClassSeatingV1 {
    pub fn verdict(&self) -> Result<(), PalwClassNotSeatedV1>;     // condition 1 first: possession, then independence
    pub fn licensable_share_permille(&self) -> u16;                // independent_operators × 1000 / base_operators; 0 for an empty base
}
pub fn palw_class_seating_v1(
    read: &PalwFoldReadV1<'_>,                 // the fold's reader at the block, of which `model_registry_seat_is_ready` is a method
    fold: &PalwModelRegistryFoldV1,            // the registry fold: `globals.seat_count`, the readiness horizon
    class_id: &Hash64,
    executor: Option<&PalwBondKeyV2>,          // the claim's executor bond; `None` for the lifecycle's observation and the registry read
    daa: u64,                                  // the block's DAA score
    terms: PalwClassSeatingTermsV1,
) -> PalwClassSeatingV1;
```

- The **population** `Base` is one function, extracted from `admission_jury_v1`'s filter (`Active`, `palw_bond_may_take_work_v2`,
  `palw_bond_may_judge_class_v2`, registered before the span's cut and matured, the registrant's bond and operator removed), so the
  jury, the per-claim outsider draw and the seating floor read **one** population and cannot drift apart.
- **The refusal** is one error, `PalwStateV2Error::ClassNotSeated { class, floor, have, need }`, with `GenClassNotReady` kept as the
  generative lane's name for the possession floor if lane D prefers (`Possession` maps to it one to one).
- **The lifecycle** reads it with `executor: None`: `PalwLifecycleObservationV1.ready_seats` becomes `ready_operators` and a new field
  `independence_floor_met` (true below the fence, so no existing transition changes); `Prefetching → Probation` and `Held →
  Probation` require `ready_enough && independence_floor_met`.
- **The registry read** (`PalwModelRegistryClassReadV1`) gains `seating: Option<PalwClassSeatingV1>` (`None` below the fence); the RPC
  and the node's `blocking` (§II.7.4) serve it, and `GetPalwModelRegistryResponse` gains the class's licensable share.
- It is `O(bonds)` per call over a `BTreeMap`, deterministic, and called once per claim at admission and once per class per span.

**Why these numbers.**

- *Possession floor = `seat_count`.* It is the least for which a panel exists without the executor: lane D's structural floor, kept.
- *Independence floor = `quorum(seat_count)`, 3 on testnet-12.* It is the number of operators the network already accepts as evidence
  of independence when it admits a bought class (the jury's strict majority), made a *continuing* condition instead of one audit's
  snapshot, and it is the least that stops the registrant and the executor, between them, from completing a first panel's quorum.
  Fewer (1) leaves one operator's outage between a class and `Incapable` voids; more (`seat_count`) makes every new class depend on
  the whole fleet of a network that has a few dozen operators. Independence beyond the quorum is carried by the lottery and by the
  collateral behind each key, not by a larger count — and the count is a parameter, so a larger network can raise it.
- *Operators, not bonds.* As the panel room's `ready_eff` already counts. On testnet-12, where `palw_operator_id_unique` makes an
  operator one bond, it changes no number; elsewhere it removes an over-count.
- *What it cannot do.* The chain cannot tell two keys of one party from two parties (§II.7.3 F1(iii)): a registrant that funds three
  more keys, each with the collateral of a panel-eligible bond (130,000 BILI on testnet-12), passes condition 2. The independence is
  bought and priced in collateral locked, which is the most that a rule reading only the chain can ask.

**The ADR-0147 outsider seat (lane D's caveat).** A bought class's claim has an outsider seat, drawn **per claim** from `Base`, which
**must hold the class to file `Valid`**: else it pleads `Incapable` and the claim voids at its receipt deadline (ADR-0147 §2.1). Condition
2 counts only operators of `Base` that hold the class, so the outsider pool is never empty; it does **not** make the outsider capable,
because the draw is over all of `Base`, not over its holders. The probability that a claim's outsider holds the class is
`|Ready ∩ Base \ {registrant}| / |Base \ {registrant}|`; three holders in a base of twenty license 15 % of the claims. That is
ADR-0147's price and it is kept on purpose: drawing the outsider from the *holders* would make its composition the registrant's choice,
which is exactly what ADR-0147 refuses. What Proposal A adds is **visibility**: the registry and the preflight report that share (the
class's *licensable share*), so a producer sees before its first claim what fraction of its claims can be licensed. No gate reads it.

**Arming.** With the RFC-0003/0004 flag day (the same consensus release as `palw_gen_v1` and the improvement protocol), **not** with
int-10. `palw_class_seating` is `None` on every preset until that flag day's entry sets its height and ceilings.

Cost: a class nobody else serves is admitted no claim, which is the independence the jury demanded once, kept; adoption is how a model
gets claims, and the registry says how far it is (`blocking`: `have 2 / need 3` independent operators). Risk: one function now guards
every door, so a defect in it is a defect everywhere; the predicate is pure over the state, is tested against lane D's gate and the
lifecycle's, and the fence is crossed by a drill on the shipping binary.

###### Scenarios the drills must include

The seating rule is exercised in the RFC-0003/0004 drills (lane C's D-M and lane D's step 8). Each scenario applies to every class
kind (an attempt class, a generative class, a composite class) unless it says otherwise, and states what must be observed. **Where
each runs** (int-11, S = 240 on the salted drill chain, seven seat nodes and one node-less bond): in the chain, SEAT-2 together with
the SEAT-5 lapse (one class, the executor among exactly five ready operators after one node stops serving it, one node down at a time),
SEAT-4 (six ready, a seventh starts serving), SEAT-7 (a genesis class admitting claims), SEAT-9 (the fence's edge, with a relay started
without the flag) and, where the class exists, SEAT-11; as fold-level tests, SEAT-3, SEAT-6, SEAT-8's statistics (the chain checks only
that the registry reports the licensable share) and SEAT-10; the generative variants of SEAT-1 to SEAT-5 in lane D's chain.

| # | Setup | Observed |
| --- | --- | --- |
| SEAT-1 | **Too few operators.** Fewer than `seat_count` distinct operators, the executor's excluded, hold the class ready (4 of 5) | the claim is refused, `ClassNotSeated { Possession, have 4, need 5 }` (a generative claim: `GenClassNotReady`); no reservation is taken and no `NoCapablePanel` void follows; the registry's `blocking` says `READY_SEATS` with `have / need`; the CLI shows stage `mine`. *For an attempt class the lifecycle holds a class below `seat_count` ready operators at the next span boundary, so the refusal is by seating within the span of the lapse and by the state (`Held`, `PANEL_NOT_DRAWABLE`) after it; the pure possession refusal is the generative lane's, whose lifecycle stays `Registered`* |
| SEAT-2 | **The executor does not count.** Exactly `seat_count` ready operators, the executor's among them | refused with `have = seat_count − 1`; the same class and claim with a different executor (not among the ready operators) is admitted |
| SEAT-3 | **Enough operators, too few independent.** `seat_count` ready operators of which only two are independent (the others: the registrant's, the executor's, or registered after the span's cut). *A fold-level test: where every operator is in `Base`, only the registrant and the executor are excluded, so five ready operators excluding the executor contain at least four independent ones and condition 2 cannot bind alone* | refused, `ClassNotSeated { Independence, have 2, need 3 }`; `blocking: INDEPENDENT_OPERATORS 2/3`; when a third independent operator proves readiness the next claim is admitted |
| SEAT-4 | **Enough.** At least `seat_count` ready operators, three independent. For an attempt class in `Prefetching`: with six ready operators it stays `Prefetching` (`READY_SEATS 6/7`); with a seventh (three independent) it enters `Probation` at the next span boundary; likewise `Held → Probation` | claims admitted; the lifecycle transitions as stated; the first probe claim is paid |
| SEAT-5 | **A lapse.** One independent operator's readiness expires (the readiness age) while the class is in `Probation` | the next claim is refused (`Independence`, 2/3), the state stays `Probation` until the existing rule holds it (`ready < seat_count`), and claims flow again when the operator re-proves; no probe is counted for a refused claim |
| SEAT-6 | **Operators, not bonds** (a devnet without `palw_operator_id_unique`). One operator with three ready bonds | counts once: five bonds from three operators are three ready operators, and the claim is refused |
| SEAT-7 | **A genesis class** (no registrant). The floor excludes only the executor | with the eight genesis operators ready: admitted; with four: refused |
| SEAT-8 | **The outsider.** A bought class held by 3 of a base of 20 | the registry reports a licensable share of 150 ‰ before the first claim; over at least 400 claims the licensed fraction is within tolerance of it, and the rest void as `Incapable`, as ADR-0147 says |
| SEAT-9 | **The fence's edge.** A claim accepted just below the fence height | judged by the old rules for its whole life (the claim's own `accepted_daa`, as ADR-0147 §2.4: the draw, the bind and every licensing arm read one answer); a claim accepted above it by the seating rule; below the height every rule is byte for byte the release's |
| SEAT-10 | **The parameter.** `independent_floor` raised from 3 to 4 at a later flag-day entry | a class with three independent operators is admitted below the new height and refused (`Independence`, 3/4) above it |
| SEAT-11 | **A composite class.** The parent held by seven operators, the composite (parent + adapter) by two | the composite is refused (readiness is per class id and artifact root); the parent's claims are unaffected |

###### Proposal B — executor conformance by the seat (go)

**Decided: go.** To be built by this lane after the preflight (§II.2), node-only, no fence. A seat, before it first proves readiness
for a class, runs the class's canonical self-test on its own executor against the reference evaluator — a few positions, or one tile
of each primitive kind for a large class (§II.9 L2: sampled conformance) — and posts a possession proof only if they agree. When it
does not, it writes why, and `getPalwNodeStatus` (what this node is doing) shows it. A dishonest seat can skip the self-test and sign a false assertion; readiness alone does not prevent harm or establish detection. Responsibility follows only when authenticated evidence objectively refutes the assertion within its signed scope. Future admission must therefore also pass independent public-bond evidence acquisition, localization and conviction under RFC14/ADR173. The registry cannot tell from a readiness declaration alone which seats actually tested.

###### Proposal C — what probation pays (C1 decided)

**Decided: C1.** Probation stays paid, and the gates move in front of it (A, and B at the seats). Probation then begins only with
seven ready *distinct* operators, three of them independent of the registrant and the executor, each of which ran the class's
conformance on its own executor, and it admits 50 ‰ of the derived volume until ten probe claims have been verified. The first paid
claim follows every condition of R5 that a chain can enforce. (C2, unpaid probation, was not chosen: it needs a probe-claim marker or
a state-dependent payout in the claim accounting, touches conservation properties of the escrow and the vesting, and asks producers to
run inferences unpaid for a class they may not trust.)

###### The registration policy of the CLI (decided)

`misaka model add` **requires a `palw-class pack verify` PASS by default**: it takes the pack with `--pack <dir>`, verifies it against
the artifact, and registers only on PASS. An explicit expert override, `--skip-pack-verify`, registers without one and prints a
warning that nothing the pack would have checked was checked. This is a CLI policy, not consensus: the chain does not enforce it, and a
registration made any other way is valid.

###### Open items

1. **The API with lane D** (above): to confirm or amend the signature, the error and the lifecycle observation's new field; one
   implementation for IR, generative and composite classes.
2. **The flag-day height** for `palw_class_seating` (the RFC-0003/0004 flag day).
3. **The drills** (lane C's D-M, lane D's step 8) to include SEAT-1 to SEAT-11.

#### II.8 Acceptance criteria and corpus completion

##### II.8.1 The acceptance criteria as checks

| Criterion | The check that closes it | State |
| --- | --- | --- |
| **A1** a custom quantisation gets a verdict before the weights are fetched | `palw-class preflight` over a synthetic GGUF header with custom type ids and an `mmproj`, in a file truncated after the header: the verdict names the type ids, the tensors and the unsupported feature, and the source never reads past the data offset (instrumented) | with the preflight (task 7). The refusal's content exists (§II.5.6) |
| **A2** root, converter version and profile reproducible; independent conformance | `pack build` then `pack verify --rebuild` rebuilds the artifact under pools of 1 and 7 threads to one root and runs the vectors on the reference evaluator, the typed backend and the independent implementation; the Hugging Face fit is within the tolerance | met on the fixtures (`tests/runtime_pack.rs`, `tests/determinism.rs`); cross-platform reproduction is §II.9 L7 |
| **A3** the testnet-12 preflight shows the court window and the canonical-job conditions | a Qwen3.5-9B-shaped configuration: the window needed and the limit, and the canonical job against the network's bounds, at a height | with the preflight (task 7) and the court-window plug point (§II.2.6) |
| **A4** mining starts only after ready seats of distinct operators with possession proofs | today: ready bonds with possession proofs (the lifecycle, §II.7.2); after Proposal A and C1: for every class kind, `seat_count` distinct ready operators besides the executor (five on testnet-12; "seven" was an earlier draft — corrected 2026-10-12), three of them independent of the registrant and the executor | partly met today; Proposal A (approved) closes the rest |
| **A5** failures are visible per stage in the CLI and the registry | the preflight's stage table (convert, register, mine); the registry's `blocking` and its CLI display (§II.7.4), one test per lifecycle state | with `blocking` (this lane) and the preflight (task 7) |
| **A6** a majority of Hub model repos can register and reach `Final` | the pinned Hub census, real-checkpoint cohort and end-to-end gate evidence meet every threshold in §II.10.5 | open; architecture-corpus percentages and earlier Hub estimates do not close it |
| **A7** a new family within armed semantics needs no `main` update | third-party direct TIR and a declarative frontend pack each complete §II.11.4 with the same node/consensus binaries; a novel primitive is refused | open; current adapter and quant-descriptor support do not prove this full route |
| **A8** local LLMs meet kernel coverage | pinned cohort shows ≥90 % real-size registration-to-Final success with the confidence requirement; all residual blockers remain counted | open; no measured denominator or active-kernel cohort drill |

##### II.8.2 Corpus completion

The preflight and the frontend are only as good as the architectures they have met, so Part II adds a *completion* criterion to
Part I's freeze criterion (§1.2), for the corpus-and-coverage lane. The corpus is complete when:

1. **Breadth.** It holds **50 to 100** representative architectures, counted by *feature set*, not by name (two models with equal
   `ModelSpec`s are one entry), each with a real published configuration and a tiny random-initialised fixture that matches
   `transformers` for its reference.
2. **Expressibility.** **At least 90 %** of the corpus lowers with the features and primitives that exist, as Level A or Level B —
   no new feature, no new primitive. The remainder are Level C, each naming its missing feature or its smallest *general*
   primitive.
3. **Court coverage.** For every TIR primitive *reachable* from the lowered corpus programs (appearing in some commit cone), the court
   has a conviction and an acquittal vector: the family certificate covers **100 %** of them.
4. **No model names in the path.** No branch on `model_type` or an architecture name outside adapters, fixtures and tests; the gate
   is a search over the crates.
5. **Reproducible verdicts.** Every corpus entry has a golden preflight (`--json`, no timestamp) and a golden program and artifact
   digest (`tests/golden_lowering.rs`); a change that moves one is deliberate and bumps the version it belongs to.

#### II.9 Known limits and open questions

**Limits of what exists.**

- **L1. Calibration's float reference is not row-streamed.** It runs one layer's weights resident at a time before the budgeted
  window of the conversion and is reported, not budgeted (`tests/streaming_budget.rs`). A model whose single layer does not fit a
  host's memory needs the float reference itself streamed.
- **L2. Conformance loads the artifact whole on each of three executors.** For a very large model, `pack build` and `pack verify`
  need a streamed or **sampled** conformance mode (the commit points of a few positions, or one tile of each primitive kind), which
  Proposal B also needs. Not built.
- **L3. The lowerer emits the `legacy-greedy-only` logit convention** until lane G's `LOGITS_Q24_V1` (lowering version 2, `af8d35a86`
  on `tir/generic`) reaches this line; a pack records either, and `pack verify` checks the unit by the slope of the fit.
- **L4. Quant vectors from re-implementations** (§II.5.7): only the ggml types and `MXFP4_HF` are checked against the maintainers'
  own code. A real GPTQ, AWQ, compressed-tensors and bitsandbytes checkpoint, with the library's own dequantised output, belongs in the
  corpus.
- **L5. An LLM.int8 checkpoint is read at any threshold** (§II.5.8), so the integer program omits the run-time outlier decomposition the
  library applies at a threshold above 0; the pack's reference fit is what measures the difference, and a pack that fails its tolerance is
  not VERIFIED. No real int8 checkpoint has been measured (the library is not installed offline).
- **L6. A 4-bit code costs a byte** (§II.5.8); a packed `i4` is a v2 candidate in `freeze-v1.md`.
- **L7. `libm-v1` is argued platform-independent (pure Rust, no `std` math) and measured on one platform** (§II.3.3); a Linux
  x86-64 rebuild of a pack built on macOS arm64 has not been run.
- **L8. Remote header fetching** (cargo feature `remote`) is tested against a loopback server only.
- **L9. Vision and audio towers are not computed** by any class (RFC-0003), and are named in the scope.

**Open questions.**

1. **Should a pack's digest be committed in the class record?** It would let a seat check that the artifact it holds was built by a
   verifiable pack. The container's meta already carries the provenance and the sidecar binds file digest and inventory root, so this
   is not proposed: a class record is consensus data, and a pack is evidence.
2. **Where do conformance vectors for a permissionless class come from?** The registrant's pack is one source; every seat can also
   regenerate the vectors from the program on the reference evaluator, which is Proposal B.
3. **The court window** (§II.2.6): whether the class-specific window of int-10 replaces the global rule everywhere the preflight
   reads it, and what the preflight reports for a class registered under the old rule.

#### II.10 Hugging Face majority: from a lowerable architecture to a class that can reach `Final`

*Added 2026-10-03 for R6/A6. This is an acceptance and implementation plan for Part II. It does not arm a fence, relax
`tir_admit_v1` or the court, or assert that the present build already meets the target. RFC-0003 supplies non-text job and
output profiles; RFC-0006 and RFC-0007 supply the smaller per-seat verification and cheaper receipt carriage needed to
serve large classes. RFC-0008 changes block work, not the model-onboarding denominator.*

##### II.10.1 The gap and the unit of measurement

The 92 % A/B reading of the 100-architecture corpus (§Implementation status, `hf-coverage.md` §23) means that those
fixtures' feature combinations read. It is **not** the share of Hub repositories whose real weights convert, whose
full advertised task is supported, whose real-size program passes admission, or whose claims can be licensed and
finalised. `hf-coverage.md` §19.2's approximate registrable counts are estimates over incomplete modality and format
coverage, not a live census or a `Final` measurement. An unsupported image, audio, encoder, adapter or quantised repo
cannot disappear from the denominator just because the decoder corpus passes.

The census unit is one public **model repository at one pinned commit SHA** (`repo_id@revision`) in a dated Hub snapshot.
One repo counts once in the headline, even if it publishes many formats. Its selected artifact path, every base-model
reference and their SHAs, task, intended inputs/outputs and weight digest are recorded. A derived LoRA or other adapter
counts only if its base resolves at a pinned revision and the complete composite can be checked. Also report unique
artifact roots and unique feature sets separately, so a popular architecture copied into thousands of repos cannot
hide missing families. Later revisions enter the next snapshot, never rewrite the old result.

Use three visible denominators, and publish the exclusions rather than silently shrinking them:

| Denominator | Meaning |
| --- | --- |
| `D_all` | Every public Hub model repo enumerated at the snapshot, including gated listings and repos with no usable weights. Private repos are not enumerable and are stated outside the claim. |
| `D_files` | The subset with publicly readable, complete weight files or a fully resolvable base-plus-adapter chain. An unknown architecture, format or modality stays **in** this denominator. |
| `D_rights` | Repos from `D_files` for which the registrant has established permission for the proposed download, redistribution/serving and on-chain use. A card's license field is evidence to review, not an automatic permission decision; unknown terms remain `RIGHTS_UNCONFIRMED`. |

Publish both repo-weighted and download-weighted rates, but the repo-weighted `D_all` rate is the headline. Downloads
are a secondary demand signal, never a substitute for breadth. Report each task (`pipeline_tag`), library, model-size
band, format, age and feature family, including an `unknown` stratum. A repo that advertises a multimodal or generative
task does not count as covered because its text decoder alone lowers: the **whole declared task** must have a canonical
job, inputs, output, court path and artifact. A smaller text-only class may be listed as a separately scoped class,
without crediting the original repo's full task.

##### II.10.2 A repeatable Hub census and real-checkpoint cohort

At a recorded UTC instant, enumerate model repos through the version-pinned Hub API. Save the response, repo id,
commit SHA, `pipeline_tag`, `library_name`, card license/gating fields, downloads, `siblings` and the recursive file
inventory with sizes and content identifiers. Resolve the repository and every referenced component at those SHAs.
Fetch only configs, model/pipeline indices, tokenizer metadata and safetensors or GGUF **headers** for the census's
first pass. File access failure, missing weights, an ambiguous base reference and unsupported files are results, not
repos dropped from the table. Respect Hub pagination and rate limits; the census is reproducible from its saved
manifest without asking the Hub to retain the same mutable `main` branch.

The Hub exposes `list_models`, revision-pinned `model_info` and `list_repo_tree`; model cards carry task and license
metadata. `snapshot_download` can fetch only named files at a revision. Standard Diffusers pipelines have a
`model_index.json`, and modular pipelines may have `modular_model_index.json`; treating every repository as a
`transformers` text `config.json` is therefore a measurable coverage loss. See the primary API descriptions:
[Hub API](https://huggingface.co/docs/huggingface_hub/package_reference/hf_api),
[model cards](https://huggingface.co/docs/hub/model-cards),
[revision-pinned downloads](https://huggingface.co/docs/huggingface_hub/en/guides/download), and
[Diffusers loading](https://huggingface.co/docs/diffusers/main/using-diffusers/loading).

Run the header-only preflight for **every** `D_all` entry that exposes readable metadata. For deeper gates, draw a
published, reproducible stratified sample of real checkpoints from each material task × format × size × feature
stratum, with a random tail sample as well as the most downloaded repos. Pin the sample seed, inclusion probability,
all repo SHAs and reasons for nonresponse. A finite corpus of 100 architecture fixtures remains a regression suite;
it is not this probability sample. Publish estimates with confidence intervals and raw counts, and show the failure
list so the numerator cannot improve by reclassifying a hard repo as `unknown` or `inaccessible`.

##### II.10.3 The gates whose failures must be counted

The preflight, pack and chain observations use one machine-readable row per repo and one stable `blocking` code per
failed gate. A downstream gate is `NOT_RUN_AFTER_<gate>` when an earlier one failed, never `PASS` by inference.

| Gate | PASS evidence | Examples of named blockers |
| --- | --- | --- |
| `source` | Pinned files, components, task and access/rights decision; complete artifact or resolvable adapter base | `GATED_ACCESS`, `MISSING_WEIGHTS`, `BASE_UNPINNED`, `RIGHTS_UNCONFIRMED` |
| `lower` | Full task lowers to a versioned `ModelSpec`/TIR program and every semantic key and weight is consumed | `FEATURE_C`, `CUSTOM_CODE_UNMODELLED`, `MODALITY_PROFILE_MISSING`, `PARTIAL_TASK_ONLY` |
| `pack` | Real checkpoint converts by a bounded-memory stream; format decode matches an independent implementation; reference and integer outputs meet the pinned fidelity rule; rebuilt roots agree | `QUANT_DESCRIPTOR_MISSING`, `QUANT_LIBRARY_UNVERIFIED`, `FIDELITY_FAIL`, `ARTIFACT_MISMATCH` |
| `admit` | The real-size graph, cost, court cone/window, DA close, capacity and artifact pass the actual `tir_admit_v1` and registry path at a named ruleset/height | `COURT_BUDGET`, `CLOSE_TOO_LARGE`, `CONTEXT_BOUND`, `CLASS_SEATING` |
| `seat` | Required distinct, independent operators prove possession and each seat's executor conformance for **this** class root; required shard/witness data are available | `READY_SEATS_SHORT`, `INDEPENDENCE_SHORT`, `SEAT_MEMORY`, `WITNESS_UNAVAILABLE` |
| `claim` / `Final` | A real job makes a valid PALW claim, obtains counted independent receipts, survives the court window and reaches `Final` with its expected reward/weight; the claim and all lifecycle objects remain in the accepted fold | `PANEL_BACKLOG`, `RECEIPT_TIMEOUT`, `OBJECT_DROPPED`, `COURT_FAULT` |

`registration-ready` means `source` through `admit` passed offline against the named ruleset; `registered` requires
the class's transaction to be accepted on the chain. `seat-ready` and `Final-proven` are later, different facts.
A produced block, a `PanelBound` object, a `Capped` class, or a successful tiny fixture is not a `Final` verdict.
Report stage counts as a funnel for `D_all`, `D_files`, `D_rights` and every stratum, with the exact stage at which a
repo stopped. Never use an unverified pack, an unavailable independent seat or an optimistic receipt as a pass.

##### II.10.4 Close the largest measured blockers without weakening verification

1. **Generic frontend and full-task profiles.** Rank `FEATURE_C` and `MODALITY_PROFILE_MISSING` by lost repos and
   downloads. Add a feature to the data adapter/generic lowerer once for all matching architectures (II.1 P1–P4),
   and require a full real-checkpoint fixture plus the one-move court battery. Route image, embedding, vision input,
   audio and video through RFC-0003's canonical jobs, stage edges and outputs. Keep a missing profile blocked; no
   text-decoder-only success can silently stand for a vision or audio model.
2. **Formats and custom code.** Add common safetensors, GGUF and other measured weight layouts through the descriptor
   registry (§II.5), checking real upstream quantiser output, not only synthetic bytes (§II.9 L4–L5). A repo's Python
   `trust_remote_code` or custom Diffusers pipeline never runs in a consensus node or seat. An offline, revision-pinned,
   isolated reference run may help author a declarative adapter and fixtures; independent implementations must then
   reproduce the canonical program, weights and output. Unknown behaviour remains a named refusal. Hub custom code
   is executable code, as [Diffusers documents](https://huggingface.co/docs/diffusers/using-diffusers/custom_pipeline_overview).
3. **Real-size admission.** Triage actual `COURT_BUDGET`, close, context and fidelity failures before changing a
   ceiling. For example, `hf-coverage.md` §19.1c records Qwen3.5's real-size `C_j` refusal and proposes bounding
   elementwise work in the same generic cost proof. Measure the new bound and court vectors; never waive a court
   ceiling, a range proof or an unknown quantisation to improve a coverage percentage.
4. **Seats and verification supply.** Proposal A's class seating and Proposal B's seat conformance (§II.7.5) remain
   binding. RFC-0006 can make a large class feasible per seat by verifying committed layer/position cells with only
   that shard's weights; RFC-0007 can lower verifier work where its algebraic checker is sound and batch verdict
   carriage. Their dormant fences need independent crossing drills before their gains count. A capped class under
   RFC-0007 is measured separately: it does not pass `Final` until full holder coverage and re-verification. At the
   target claim rate, measure receipt supply, `PanelBound` age, queue growth, licences, `Final` rate and dropped
   lifecycle objects. Increase qualified seat supply or bound intake when the queue grows; do not turn missing
   receipts into `Valid` or reduce independence/quorum to meet a target.
5. **Iterate from the census.** For each release, publish the top blocker buckets with counts, one generic change per
   bucket, its before/after cohort, and regressions in other strata. Re-run the census on new pinned snapshots.
   An improvement in A/B architecture coverage that does not move `registration-ready`, `seat-ready` and `Final`
   remains a frontend improvement, not completion of R6.

##### II.10.5 Exit criteria and the permitted coverage claim

The phrase **"most Hugging Face models can be brought to PALW"** is permitted only when a dated report shows all of
the following. The first target is literal majority; the engineering target is at least 80 % of `D_files` at
`registration-ready`, without treating `D_files` as the headline denominator.

1. The estimated repo-weighted `registration-ready / D_all` share has a **one-sided 95 % lower confidence bound above
   50 %**, with exclusions and nonresponses included as failures in `D_all`. Give the exact point estimate, bound,
   denominator and snapshot. Report all material task/format/size strata even if some remain below 50 %; never
   advertise an unrestricted modality claim from a text-only result.
2. From the same stratified real-checkpoint cohort, the estimated `Final`-capable share of `D_all` also has a lower
   bound above 50 %. Each sampled success needs a confirmed registration, independent seat evidence, a PALW claim
   and its on-chain `Final` under the shipping binary and the target ruleset. A sample without enough seats is a
   measured `seat` failure, not a waived trial. The report includes enough independent operators to exercise the
   seating rule; a four-node fence-crossing drill cannot close this item.
3. The panel runs at the intended sustained issuance rate for at least two receipt windows without an increasing
   `PanelBound` queue, an unaccounted dropped lifecycle object or a stalled `Final` rate. Every failure is retained
   in the cohort's gate table. Corpus coverage (§II.8.2), `Capped` entry, block production and isolated unit tests
   are supporting evidence, never replacements for this run.

If any condition fails, publish the measured narrower claim (for example, *decoder-only float checkpoints lower*)
and the blocker distribution. This keeps the goal visible without declaring Hub-majority registration or `Final`
from the 90–92 % architecture corpus or the earlier unmeasured Hub estimates.

#### II.11 Future families without a `main` release (R7/A7)

*Added 2026-10-03. This is the target contract for permissionless onboarding, not a claim that the current frontend,
registry or seats already pass it. “No `main` update” means no consensus/node binary upgrade and no privileged
model-family approval. Registrants and seats may install third-party **off-chain** tooling and fetch class artifacts;
the class must still satisfy the armed admission, seating, execution and court rules.*

##### II.11.1 Decide by the missing semantic, not by the model's name

| What a future model needs | Route | `main`/protocol change? |
| --- | --- | --- |
| New config keys, tensor names, layer schedule or a **static** combination of already armed TIR primitives | A third-party frontend pack (§II.11.2), or direct canonical TIR (§II.11.3) | No, if its graph and real-size class pass admission |
| New storage or quantisation layout expressible by `quant-format.v1`, or a custom offline importer that emits the canonical artifact | A descriptor with independent decode vectors (§II.5), or an independently reproducible converter | No; unknown layouts remain refused until described and checked |
| A new task made solely from already armed canonical job/input/output kinds and pipeline stages | A class-declared RFC-0003 pipeline and complete task scope | No; a new **consensus** job or output meaning needs a versioned RFC-0003 extension |
| A tensor operation not expressible by active primitives within checker/court limits | `KERNEL_EXTENSION_REQUIRED`: generic semantic/constraint/checker/court family, independent vectors, resource/soundness review and fence (§1.2, §9; ADR-0172) | Yes, coordinated kernel upgrade; no guest fallback |
| Bounded data-dependent control | Finite AOT/`Select` within active grammar/limits, or an explicitly defined bounded routing/state kernel extension | No for admitted composition; yes for missing relations; no BVM/GVM |
| Runtime-dependent agent/tool workflow or arbitrary program logic | Off-chain orchestration may submit supported jobs; arbitrary uploaded programs are outside the model-plan grammar | No consensus VM is planned; a new bounded task relation needs a separate kernel/task proposal |

Thus a frozen finite TIR can support **new combinations indefinitely**, but cannot promise every future computation.
There is deliberately no universal instruction substrate. Missing semantic/memory/checker/court relations remain
extension gaps until a reusable kernel upgrade becomes active. Arbitrary circuits, `CustomOp`, EVM precompiles
and frontend scripts cannot bypass that boundary. Bind semantics, whole-statement constraint coverage, bounded
resources and dispute localization; confidence is not registrant-selected. Importers and job/input/output profiles
remain separate problems. The old RFC05 decoder survey is not measured residual-model coverage (§II.12).

##### II.11.2 An open, content-addressed frontend-pack interface

The built-in `FeatureId` registry and `model-adapter.v1` are conveniences for the reference lowerer, **not** an
allowlist for consensus. The present adapter maps keys onto features that the installed lowerer already knows;
therefore that adapter alone cannot express every future feature without a tool update. Add a versioned,
user-supplied `tir-frontend-pack` format for the common no-release path. Its declarative language can construct a
bounded static TIR subgraph from existing primitives, dimensions, layer schedules and declared `Fixed`/`Hist` state;
map config keys and tensor roles; include quant descriptors; and declare the full task scope and source revisions.
It may compose named template fragments but has no runtime branch, IO, clock, remote fetch, native code or operation
definition. The compiler expands it under explicit size/fuel limits, type-checks every tensor and refuses an
unconsumed math-affecting config key, tensor, component or custom-code behavior. Its output is ordinary canonical
TIR plus the artifact and runtime pack, not a new class kind. A pack may be published anywhere and identified by
its content hash; no default repository, signer or maintainer approval is needed for admission. The reference CLI
may offer a curated index for discovery, but index membership cannot decide validity.

The runtime pack pins the frontend-pack hash, compiler/tool version, config and source SHAs, quant descriptors,
generated `ModelSpec`/TIR digest, artifact root, complete/partial task scope, conformance vectors and HF-reference
fidelity evidence (§II.4). A seat verifies **the submitted TIR and artifact** with the armed generic evaluator and
class conformance rules; it never has to execute a registrant's Python, `trust_remote_code` or frontend pack to
decide a claim. A third-party pack's own test vectors do not grant authority: malformed output fails the common
verifier, and a false HF-equivalence claim is distinguished from the on-chain integer-program verdict.

**Implementation increment (2026-10-10):** [tir-frontend-pack.v1](../design/palw/tir/tir-frontend-pack-v1.md)
now composes the frozen primitive/state grammar without ModelSpec dispatch, consumes config/tensor
bindings, and streams exact integer or explicitly rounded IEEE imports into the ordinary container.
`palw-tir-frontend` emits replayable receipts; SDK `pack build-frontend`/`verify-frontend` pins source
SHAs/compiler/executor revisions and uses common inventory and three-way conformance. Fixtures
include direct-byte equality and offline class admission. Bounded inline **virtual**, **tensors**
and **blocks** quant descriptors now feed the same compiler/SDK route, with explicit raw roles,
mandatory decode vectors, nested-config checks, bounded pages and before/after source hashes.
Tensor shape/JSON metadata is acquired after all binding-header checks, with per-plan and aggregate
limits, exact-number parsing and source-change pins. MXFP4 expert axes, all built-in saved layouts
and unknown third-party formats are exercised. Storage decode is explicitly binary32 before the
declared integer rounding. Native whole/split GGUF acquisition now exposes raw names/layouts to
the same generic CLI/SDK, including supplied unknown GGML IDs, strict native metadata and embedded
tokenizer identity. Standard split names and a public index allow independent part selection;
part numbers, shared declarations, primary metadata and global tensor uniqueness are checked.
Aggregate header/allocation/count budgets, checked extents and snapshot revalidation protect
this producer-side reader across every part and index. SDK artifact publication follows all receipt/inventory/conformance/source
checks. Independent companions now attach public reference logits under predeclared revision/task/
context/units/error criteria, and verify independently repeats a portable fit. The existing exact-class
binding and beacon commit/run/fresh-verify tools accept both formats through one artifact/layout/
static-admission/challenge/evidence path; independent imports use typed absent calibration.
Conformance binds the node's kernel program-root domain, separately from the container graph root,
and derives the same plan. Single-program selection uses the node prosecution/carrier/block
limits for v1/v2/segmented v4 without shortening requested context; v4 still requires separate
OPV admission/economics. No tool report supplies availability, activation or approved policy.
Reference-logit gates stay separate from source identity, routing/state fidelity and task quality.
These synthetic fixtures do not close §II.11.4: real-checkpoint source fidelity, real-node beacon and
independent-seat
claim/Final/redemption evidence remain open in the [implementation ledger](../design/palw/tir/rfc0002-implementation.md).

##### II.11.3 Direct-TIR escape hatch and exact boundary of permissionlessness

Any independent compiler may submit canonical `TirProgramV1` bytes (or an armed RFC-0003 pipeline), a compatible
artifact and the ordinary class registration. The registry must judge them by `prim_set_id`, program version,
canonical encoding, type/range/cost/court admission, artifact commitment and class lifecycle only. It must not
require a recognised `model_type`, built-in `FeatureId`, official pack, HF repository, preferred compiler or
model-family signature. This path is essential when the declarative frontend language is too narrow but the
**resulting TIR is already expressible**. It also keeps an extension-language revision outside consensus.

Direct TIR proves the submitted integer program can be adjudicated; it does **not** prove that it faithfully
imports the named HF checkpoint. Without independently reproducible source conversion and fidelity evidence,
show `SOURCE_EQUIVALENCE_UNVERIFIED` in onboarding reports and do not count the repo as an A6 full-task success.
Likewise, an active kernel execution path alone does not solve a missing canonical input, output or job profile;
the full advertised task stays blocked under §II.10.1 until that profile is armed. A class that exceeds static
ceilings remains `EXCEEDS`, however popular its model family is. New semantics must use a new `prim_set_id` or
program/job version and fence; previously registered classes retain their exact old meaning and court.

##### II.11.4 Acceptance before calling this permissionless

With **unchanged** consensus/node binaries, unchanged armed `prim_set_id` and no model-name allowlist:

1. An independent party names a previously unknown, static feature combination, publishes a frontend pack and
   a real checkpoint, then completes header preflight, streamed conversion, reproducible pack verification,
   registration, independent seat possession/conformance, a claim and on-chain `Final`. Another party rebuilds
   the same roots without the first party's executable code.
2. Another independent party emits the same canonical TIR bytes directly with a different compiler. Admission and
   court give the same verdict. A third party adds a quant descriptor and its independent vectors without a
   node release; bad vectors and ambiguous tensor bindings are refused.
3. Mutation cases cover an unknown op, hidden config key, missing tensor/component, false shape, unbounded
   expansion, unsupported job kind, bad artifact root and a deliberately wrong fused backend. Each fails at the
   named gate; an untrusted pack cannot bypass admission, seating or court. The old registered class still runs
   byte-identically after a newer frontend-pack version is published.

Record node/consensus binary digests, frontend/compiler and pack digests, class and artifact roots, seat operators,
claim id, `Final` reference and every refusal. Passing a small direct-TIR fixture establishes the protocol
interface; a **real checkpoint, full task and independent seats** establish the onboarding claim. Feed these
results into A6's Hub cohort, so the no-release extension mechanism demonstrably improves the real `Final` funnel.

Implement in this order: arm and exercise generic TIR admission/execution/court and the needed RFC-0003 job profiles;
prove direct third-party TIR registration; add the bounded declarative frontend-pack interpreter and open discovery;
run A7 and the Hub-wide A6 cohort. Measure the local-LLM residual (§II.12) before prioritizing or claiming a new
kernel extension. The public extension interface, Hub-wide majority and local-LLM coverage target are distinct milestones;
none may be inferred from another.

---

<a id="source-v1-0011-L00001"></a>

### 旧RFC-0011 / 原文1–746行

出典: [0011-permissionless-model-and-long-context-onboarding.md](../rfc/0011-permissionless-model-and-long-context-onboarding.md)。配置理由: 主題別統合。

### RFC-0011: Permissionless model onboarding with probabilistic constraint verification and court on dispute

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **適用時期の現行方針（2026-10-17 文書化）:** 本書が参照する画像・動画生成および RFC-0004 の改善に限り **mainnet 後へ延期・当面無効**。初期 mainnet の必須実装・提供・有効化に含めず、mainnet 開始による自動有効化もしない。共有のテキスト・固定登録・検証・会計は維持する。過去の Status・実装・試験・全面有効化計画はこの範囲の有効化承認ではない。[正式な範囲・無効化要求・再開条件](../governance/post-mainnet-scope.md)。今回 node コードは変更しておらず、runtime の無効化は未確認。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


#### 2026-10-09 改定 — 登録IDとモデル内容の永久不変binding

[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)を適用する。permissionlessな新規登録は継続し、weights、graph、tokenizer、実行仕様、canonical artifact root、kernel/verification planを登録IDに固定する。登録後の更新要求、同一登録へのV2/V3、overwrite、head置換を拒否する。改善版・異なる実行仕様は新規IDと独立した検証・報酬資格を持つ。計算graphの識別とmodel registration IDを混同しない。既存class/hash/wire IDは再採番せず、追加modelの登録IDにrootを含める。

新kernel・planの有効化は新登録のadmissionを可能にするもので、既存モデルのbindingを置換しない。実際のbeacon、challenge transcript、conformance証拠、利用量や報酬資格は運用・証拠状態であり、固定された承認bindingを変更しない。登録の自由と配布・rewardabilityの要件は分離する。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: Revised Draft, 2026-10-10後続改定。Sampling-first/court-on-disputeと永久不変identityを維持し、§17のmodel取得gateを撤回する。model-bond coinbase配分・条件付きG14を採用する設計で、実装/経済評価/activationは未完了。既存t12/fingerprintは変更しない。
* Source audit: repository commit `27670cd2b` (includes the DAA 5,300 release). Source-proven limits below describe this commit, not a fresh measurement of every deployed node. Prior 9B run results, current code facts, and proposed fixes are labelled separately; no successful 9B registration or 2M run is claimed.
* Scope: PALW-TIR model conversion, class admission, long-context claims, probabilistic constraint checks, data availability, court, and readiness. The primary coverage target is **at least 90% of all public Hugging Face model repositories at a pinned snapshot, registered as their complete advertised task and context**. Registration must be independent of the registrant's VPS size. Execution semantics and terminal adjudication remain exact; normal verification accepts a quantified, nonzero soundness error.
* Related: [RFC-0002](../rfc/0002-palw-tensor-ir.md), [RFC-0006](../rfc/0006-palw-layer-sharded-panels.md), [RFC-0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md), [ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md), [ADR-0103](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md), [ADR-0135](../adr/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md).

#### 0. Decision requested

**Use probabilistic constraint verification as the normal acceptance path: a miner executes once, commits its result and evidence, then verifiers use Freivalds/GKR-style checks under post-commit challenges. A passing claim may become Final after its challenge window without full execution or segment replay by the Panel. On disagreement, localize the disputed computation and use the existing deterministic terminal court where compatible.** A passing transcript is correctness at the declared error bound, not a proof with zero error. Raw trace spot checks alone do not meet this decision (§15).

Registration checks a bounded `VerificationPlanV1`: semantic bindings, coverage of constraint families, probabilistic soundness parameters, compositional resource limits and a bounded dispute route. It does not enumerate the entire future execution or prove every execution value correct in advance. Heavy conversion, calibration, witness construction and proof generation may be supplied by untrusted workers. A class can be listed before it is mineable; useful-work weight requires the armed verification, DA and capacity rules.

**Revision boundary.** This replaces §13.1's former requirement for a model-specific, exhaustive deterministic admission proof as the preferred route. Small existing classes may retain exact sizing/replay. The old 9B/2M failures and coverage audit remain historical evidence. Probabilistic execution acceptance does not permit probabilistic parser/memory safety: every permitted query and terminal dispute must still fit enforced bounds, established through reusable approved kernel-primitive rules and composition. An unknown dynamic access can use an authenticated-memory constraint and bounded opening; it cannot be omitted from verification. §15 defines the new route; where earlier release gates mention exact bounds they refer to resource safety and terminal semantics, not full normal-path replay.

**Kernel-only revision, 2026-10-06:** [ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) and [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md) withdraw the BVM/GVM fallback. Model extensibility uses declarative plans within active kernels or a coordinated, versioned kernel extension when semantics/checkers/courts are missing (§16). The probabilistic, encoded-constraint acceptance policy in §15 is unchanged. This is not a claim that all finite models fit today's kernels or that a SegWit-style extension is automatically a soft fork.

**The coverage claim this RFC is intended to enable is not “90% of decoder fixtures” or “90% of local LLMs”; it is `registered_full_task / D_all ≥ 0.90` for a dated, reproducible census of all public Hugging Face model repositories (§7).** `registered_full_task` requires an accepted on-chain class registration under the target rules, or independently reproduced equivalence to an already registered class (§11.2), not a passing config parser, a draft artifact, a catalogue entry, a short-context substitute or a model-market line. This proposal does not assert that today's implementation meets that threshold; it defines the evidence that must exist before anyone may say it does.

“Any model, any context if resources are supplied” is a *directional interoperability goal*, not an assertion that arbitrary future code or an infinite context can safely run under today's TIR. Every admitted class still needs deterministic TIR semantics (or a separately reviewed, versioned primitive set), a valid lowering, a finite resource envelope and an answerable challenge. Enough independently qualified execution/verification capacity is a further condition for mining, not a requirement to start a Panel before zero-share registration. Unsupported architecture, unbounded work, and unavailable evidence have distinct, machine-readable outcomes. Neither changing `max_context` to 512 nor bypassing a court/DA bound counts as success.

#### 1. Evidence: why the Huihui Qwen3.5-9B registration stopped

The attempted source was `huihui-ai/Huihui-Qwen3.5-9B-abliterated`, revision `05b9e7c9b978ba29bdb8f50a49c30e4b91183339`. A fidelity artifact was built and retained locally; no class-registration transaction was signed or broadcast, and no model line was created. The following are separate gates, not one generic “CPU shortage”:

| Stage | Observed or code-proven result | Meaning |
| --- | --- | --- |
| Source → fidelity artifact | The 9B source and 512-position calibration produced a PALW-TIR artifact. Full pack conformance was costly on CPU; an independent `ref2` run was skipped at this artifact size. | Source conversion is possible; pack verification time is **not** evidence that consensus rejected the class. |
| Artifact metadata | The streamed `palw-tir-convert`/pack path checks recurrent calibration length when `--calib` is used, but its written metadata omits `calibrated_context`; `palw-tir-fidelity` writes it. `declare-layout` refuses a recurrent artifact that cannot show context coverage. | A tooling/provenance mismatch, independent of hardware. See [`convert.rs`](../../misaka/palw/tir/lower/src/convert.rs) and [`tir_layout.rs`](../../misaka/palw/sdk/src/tir_layout.rs). |
| First 512-position layout | Default tile produced `COURT_COST_EXCEEDS_CEILING`: 16,842,752 tile MACs against 16,777,216 allowed. | A tile-shape constraint; smaller legal tiles get past this check. It is not an invitation to raise the court ceiling blindly. |
| Reduced-tile layouts | Contexts 512, 256, 128, 32 and 16 still returned `TIR_EXCEEDS_CEILING`: IR close-sizing work **67,108,865** against **67,108,864**. The t12 node's live preflight at DAA 5,381 also refused the tested 256 profile. | This is the decisive observed registration blocker. **67,108,865 is `cap + 1`, a refusal sentinel, not the measured exact work.** The range twin introduced by `palw_tir_fence2` is already available on t12 after DAA 3,600; it must be profiled rather than proposed as an unactivated fix. See [`palw_tir_admission_v1.rs`](../../consensus/core/src/palw_tir_admission_v1.rs) and [`palw_tir_close_size_v1.rs`](../../consensus/core/src/palw_tir_close_size_v1.rs). |
| Seat feasibility | The prior preflight reported a working-set shortage (`SEAT_MEMORY_SHORT`). Its default comparison uses fixed t12 tier estimates of 3.5 and 8 GiB, not an automatic measurement of live seats. | This is a **mine/readiness** blocker, not the registration refusal. Preserve exact needed bytes and tier provenance in the next report; adding genuinely capable seats or safe layer shards addresses it. An arbitrary `--seat-share` does not prove live capacity. See [`preflight/chain.rs`](../../misaka/palw/sdk/src/preflight/chain.rs). |

These measurements are one 9B artifact and particular layouts, not a proof that *all* Qwen3.5-9B variants or all contexts fail. The exact cap-crossing commit point, range-twin step count, terminal close bytes, and candidate layout's other gates remain to be measured. The former 5,102-DAA ladder-clock report is not established as the blocker for this artifact: t12's held clock and its manifest verifier must be checked against the same live ruleset before making that claim.

**Scope of this source matters.** The pinned upstream [config](https://huggingface.co/huihui-ai/Huihui-Qwen3.5-9B-abliterated/blob/05b9e7c9b978ba29bdb8f50a49c30e4b91183339/config.json) declares `Qwen3_5ForConditionalGeneration`, a vision configuration, and `text_config.max_position_embeddings = 262144`. An 8k text-only profile is a useful requested deployment, but is **not** full-task/full-context coverage of this repository for §7. A 2M extension also needs a versioned position/attention policy and fidelity evidence: raising a context integer does not give the upstream model a validated 2M capability. Track the requested 9B-8k case, full-source coverage, and a validated 2M profile as separate acceptance cases.

#### 2. Why 2M is more than a CPU problem

1. **Calibration cost.** For a recurrent program with `Fixed` state, [`check_calibration_length`](../../misaka/palw/tir/lower/src/fidelity.rs) requires at least one sequence as long as the declared context. A 2,097,152-position declaration therefore asks for 2,097,152 sequentially dependent positions under today's rule: 256 times the token count of 8,192, or 4,096 times 512. This is *not* a measured 256× elapsed-time prediction; attention, paging, batching and state size can change scaling. GDN/Mamba state writes cannot simply be distributed as independent positions. The short-calibration waiver exists but is not a general safety-preserving solution; its long-context fidelity must be demonstrated.
2. **Three independent format gates block 2M, before hardware is considered.** [`palw_tir_v1.rs`](../../consensus/core/src/palw_tir_v1.rs)'s t12 ceilings set `max_context = 2^18 = 262144`. [`program.rs`](../../misaka/palw/tir/core/src/program.rs) permits only the small `2^18` and held `2^21` history declarations, but [`verify_class_admission_v10`](../../consensus/core/src/palw_tir_admission_v1.rs) explicitly rejects the held declaration because its accusation/answer route is still legacy-binding-shaped. Independently, the canonical prefill is `floor(max_context / 8) - 1` and must fit J5b's 4,096 inline token IDs; this limits admitted layouts to 32,783 positions. At 2,097,152 positions the canonical prefill would be 262,143 IDs. Fixing **only** prompt attribution, **only** the network context ceiling, or **only** held admission leaves the others in place. A new complete IR context/court route and its armed ruleset are required.
3. **A full-context proof is not a full-context replay.** A root over held state can keep *carried bytes* small, but a producer must still do the work, seats must be able to inspect disputed segments, and the court must have bounded bytes, computation and time. The close-sizing `2^26` step cap is a validator DoS guard, not a parameter estimating the model's inference FLOPs. If the sizing algorithm traverses too much, moving the same traversal to every validator with a larger cap is not a solution. Bound-preserving algebraic/range proofs or succinct certificates need exact differential tests and a court path.
4. **Memory, DA and seats are separate.** Long histories and large weights can exceed a seat's published memory share even after admission is fast. The existing preflight estimates `artifact + state(context) + peak live + widest opened tile`. A class without capable seats can be registered/listed only if the protocol explicitly leaves it non-mineable; it must not claim ready status or earn for unverifiable work.

Therefore “CPU不足” should be reported as one of `LOCAL_COMPUTE_EXHAUSTED`, `CALIBRATION_LENGTH_UNPROVEN`, `ADMISSION_WORK_CAP`, `CANONICAL_PROMPT_UNATTRIBUTABLE`, `COURT_BYTES`, `COURT_WINDOW`, `DA_UNANSWERABLE`, or `SEAT_MEMORY_SHORT`, with stage, actual measured value (or a **lower bound** when censored), limit and active fence. A 2M benchmark has not been run in this investigation; exact runtime and RAM requirements must be measured before capacity claims.

#### 3. Proposed interface and trust separation

```text
source checkpoint + tokenizer + frontend/adapter version
       │
       ├── untrusted calibration worker → CalibrationEvidenceV1
       ├── untrusted streamed converter → content-addressed TIR artifact
       └── untrusted conformance worker → independently reproducible results
                                              │
                                              ▼
  bounded VerificationPlan admission → Registered / Dormant / Certified / Mineable
                                              │
  producer execution → committed trace / constraints / material
                                              │
  post-commit challenges → probabilistic checks → challenge window → Final
                                   │ disagreement / missing evidence
                                   └→ localization / DA route → exact terminal court
```

`CalibrationEvidenceV1` MUST bind the source root/revision, tokenizer and frontend/adapter spec digests, deterministic corpus/token-sequence root and length, claimed context, statistics root, quantization policy, math and converter versions. A signed worker statement can locate or attest to evidence but is **not** a substitute for reproducing it. Stats accepted through `--stats-in` MUST carry the same context-coverage record as `--calib`; neither path may silently omit it. Static registration may commit to unverified provenance while clearly withholding source-fidelity certification; the evidence must be verified before counting it as source-equivalent coverage or granting the corresponding certification. Do not impose full calibration replay on every registering client or validator in the name of checking metadata.

The protocol MUST distinguish:

* **Published artifact / listed line:** hash-identified, discoverable, no claim that its model is ready or useful work is being rewarded.
* **Registered TIR class:** deterministic semantics and a validated verification plan for the declared envelope, with coverage, soundness, court and DA budgets; initially zero useful-work weight where current rules allow that. No execution has been certified by registration alone.
* **Certified and mineable:** independent conformance, material availability, sufficient capable bonded seats, deadlines and end-to-end challenge drills passed. The existing `ClassLaneCertified`/activation semantics govern rewards; this RFC does not grant weight to a merely listed class.

Tool eligibility is separate from consensus admission. Today's CLI requires artifact and conformance PASS in its runtime-pack gate **before submission**; a size-based independent-conformance SKIPPED can therefore stop local registration even when live class preflight passes. Consensus itself does not enforce that pack gate. [RFC13](../rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) specifies exact layout handoff, target-ruleset provenance and large-artifact independent checking, using the separate Llama 3B attempt as evidence.

If the current transaction format already requires full admissibility for `ClassRegisteredTirV1`, a lighter *artifact publication* must be a separate, explicitly non-mineable object or off-chain content address—not an unsafe weakening of `ClassRegisteredTirV1`. A model market/position is also a separate economic object: a line appearing in `#/lines` does not itself prove that the class is mineable.

#### 4. Admission and long-context changes

##### A. Make today's 9B failure explainable and fixable without raising limits

Instrument the close sizer to return `cap_exceeded_at` (commit point, counted work, exact count only when cheaply obtainable, lower bound otherwise), twin, largest read-set category, and close-byte estimate. Profile the 9B `16, 128, 512, 8192` layouts under the **actual** t12 post-fence parameters. Compare the element and range twins' bounds and work; if both exceed the cap, identify which loop scales with program size rather than context. For the legacy route, improve exact sizing inside its budget; for the new route, use §13.1's compositional verification plan and bounded terminal templates. The new route need not first solve exhaustive legacy enumeration, but must establish its own resource bounds and accepted class transition under an armed fence. Differential-test terminal bounds against uncapped small cases and mutation cases. Do not raise `2^26`, tile MACs, close bytes or court windows until adversarial validator CPU and carrier tests justify a separately fenced rule.

##### B. Open a safe path above 32,783 positions

Add a versioned, Merkle-attributed/tiled prompt-ID commitment to **IR canonical jobs**, with domain-separated root, token count, tile index and paths bound into job/claim identity. Define a bounded opening and challenge for every segment; include reorg/DA behavior and a worst-case prompt-ID proof size. This must replace—not merely bypass—the J5b inline-attribution assumption and have a distinct consensus fence and fingerprint. A 2M class may use a bounded canonical work segment plus committed predecessor state, but only after proving that claim splitting cannot duplicate work credit, alter ticket odds, or launder unverified history. RFC-0008's work-slice blocks and RFC-0006's layer/position cells are related designs, not already-shipped solutions. Admission must still account for the longest permitted service context and worst-case dispute, not just a short benchmark prompt.

This phase MUST also replace the `2^18` network context cap and the explicit IR held-history rejection with a fully specified versioned route. It must handle the separate per-position MAC/state/live-memory caps, job leaf ladder and DA reach in §12. A segmented execution retains the full declared history through authenticated boundary state; setting every segment's semantic context to 512 is a different model, not segmentation. Every source configuration change, including positional scaling for a 2M extension, is committed into the profile identity and tested independently.

##### C. Keep execution exact; make normal verification probabilistic

Permit independent workers to produce checkpointed, resumable calibration evidence and streamed weight chunks, keyed by all semantic inputs. Separate `convert`, `declare/preflight`, `conformance`, and `certify` commands; `pack build` may remain a convenience wrapper. Use existing `--stats-in`, `--chunk-store`, and `--keep-chunks` as implementation starting points, but fix provenance and resumability. CPU or integer GPU acceleration is an **operator choice**, never a new semantic rule: all outputs and commit points must match the reference TIR; a GPU backend's existence does not mean the pack calibration/conformance path already uses it. Recurrent scans require a validated chunk-summary/parallel-scan algorithm or honest sequential execution; do not promise linear core scaling.

Normal Panel verification follows §15: randomized algebraic checks replace replay of heavy operations; authenticated constraints cover the remaining operations, state and routing. Exhaustive reference replay is a development/conformance baseline and an optional small-class path, not a per-claim requirement for large classes. Applying the same machinery to calibration verifies the declared calibration computation; source-model accuracy still requires its separate fidelity evaluation.

For 2M, a full-sequence calibration requirement may remain for accuracy, but it must be possible for a separate, provisioned worker to compute it once and publish content-addressed evidence. Before making “ordinary VPS registration” a guarantee, specify how a cheap verifier detects forged coverage/statistics without replaying the entire calibration. Options include recomputable challenge windows, a bounded fraud-proof with committed intermediate states, or independently bonded attestations plus slashable availability, each with quantified soundness; a hash and signature alone prove identity, **not correctness**. Calibration fidelity is also not the same as consensus determinism: the chain may validate deterministic TIR semantics while a certification gate judges source-model fidelity.

##### D. Resource envelopes, not fixed fleet tiers or architecture names

Admission/preflight should expose separate finite budgets for `registration_validator_steps`, producer inference time/bytes, DA answer bytes/time, worst court bytes/work/window, and seat RAM/VRAM/storage. A registrant can publish a larger honest resource requirement; operators self-declare capacity and prove readiness through reproducible jobs. The chain must not assume the SDK's current 8 GiB tier is every future seat. Layer sharding or other verified partitioning may reduce per-seat footprint only with the coverage and fraud-detection proof in RFC-0006/0007.

An unknown HF architecture is first a **frontend/primitive coverage** question, not a CPU error. Allow permissionless, signed/source-pinned frontend adapters that lower to a frozen, validated TIR primitive set. New semantic primitives require review, a new version/fence, reference/court support and a test corpus; arbitrary Python or custom kernels do not become consensus meaning by being uploaded. The objective is that any finite, representable model can progress by supplying an adapter and sufficient resources, and that a genuinely unsupported model reports the missing primitive or semantic rule precisely.

#### 5. Implementation order and release gates

| Phase | Change | Passing evidence |
| --- | --- | --- |
| A — tool-only | Fix `calibrated_context` parity for `--calib`/`--stats-in`; structured refusal report; resumable calibration/conversion; split pack operations. | Same source+stats+policy yields identical artifact root on two machines; forged/short coverage is refused; 9B refusal names the exact gate without allocating unbounded CPU. No consensus fingerprint change. |
| B — admission efficiency | Bounded verification-plan admission for the new route; exact close-sizing improvement remains an optional legacy route. | Real 9B profiles under the explicitly named target ruleset: old refusal reproduced, new route passes **all** registration gates and its terminal bounds hold. A newly exposed blocker is not completion. Differential bound checks and hostile-input CPU ceiling. A consensus change has its own fence and depends on P before use. |
| C — 2M semantics | IR prompt attribution beyond 4,096 IDs, replacement of the `2^18` context cap and held-format refusal, bounded segment/state commitments, DA and court route. | 32,783/32,784 and 262,144/262,145 boundaries; 2M valid/invalid jobs; altered prompt tile or predecessor state convicted; no split-credit inflation; **all** §12 gates pass with a real artifact. New fence and fingerprint. |
| D — capacity/certification | Open worker evidence, capable-seat/shard certification and lifecycle. | Untrusted calibration worker can be replaced; a low-resource registrant can publish/register a *safe* class without doing model inference locally; class remains weightless until independent readiness; a provisioned model is actually mined and challenged; non-ready class cannot earn. |
| P — probabilistic acceptance (required for the new route) | §15 verification plans, commitment-bound Freivalds/GKR checks, complete constraint coverage, post-commit challenge schedule, soundness accounting and bounded court localization. Develop alongside B/C; D cannot enable this route before P passes. | Single-fault and adaptive-grinding tests; reviewed claim-wide error bound; valid claims finalize without routine segment replay; a failed check reaches an exact terminal dispute; missing evidence cannot finalize; model/SDK/IBD verdict parity. A separate dormant fence, no inherited activation at DAA 5,300. |
| E — Hub-wide coverage | Census and close the highest-volume *full-task* blockers across every Hub modality, format and framework (§§7–10). | A pinned all-Hub report proves the one-sided 95% lower bound for `registered_full_task / D_all` is **at least 0.90**, with actual on-chain registration evidence for sampled successes; every inaccessible, partial-task and untested repo remains in the denominator and out of the numerator. |

Release gate: reproduce the prior 9B `2^26 + 1` refusal on the old binary, show the proposed binary's proven sufficient bound **and accepted state transition through every registration gate**, compare node and SDK preflight at the same DAA/fence, and run registration/IBD/reorg/court/DA tests. A named next blocker is a useful development result, **never a release pass**. Then measure 8k and 2M calibration wall time, peak RSS/VRAM, energy and cost on named hardware, with abort/resume and missing-worker cases. No change is “fixed” merely because a UI line appears or a fast happy-path registration passes.

#### 6. Explicit non-goals and unresolved questions

This RFC does not register the Huihui model, open a market, start a Panel, claim that 2M or Kimi K3 was benchmarked, or assert an exact hardware requirement. It does not remove court windows, treat trust in a calibration signer as a proof, or promise support for physically unbounded contexts. The selected probabilistic acceptance policy deliberately admits a nonzero false-accept risk under explicit assumptions. Before activation, resolve §15's challenge construction, integer/field equivalence, whole-claim error budget, witness/prover costs and bounded escalation, alongside the existing 9B/2M barriers. Neither court availability nor a successful small-model benchmark resolves those questions by itself.

#### 7. The 90% claim: denominator, numerator, and feasibility ceiling

This section makes the user's *all-Hugging-Face* target explicit. It is intentionally stronger than [RFC-0002 §II.10](../rfc/0002-palw-tensor-ir.md)'s first **majority** milestone and distinct from [RFC-0002 §II.12](../rfc/0002-palw-tensor-ir.md)'s **90% of local text LLMs** target. The latter cannot satisfy this RFC by changing a denominator.

* `D_all`: every publicly discoverable HF **model repository** in a dated, complete, cursor-paginated Hub snapshot, counted once as `repo_id@commit_sha`. Include gated listings, missing or broken weight repos, unknown architecture/task, custom code, every modality and every framework. Private repos cannot be enumerated and must be reported outside the claim. Report exact count, crawl time, page count, API version, failures and duplicate-ID resolution. Do **not** silently exclude “unusable” repos.
* `D_files`: the subset with complete, readable checkpoint files or a resolvable base-plus-adapter chain at pinned revisions. `D_files` is a secondary diagnostic denominator, not the headline.
* `D_rights`: the subset of `D_files` with evidence of the rights needed for the proposed registration and artifact distribution. Unknown permission is `RIGHTS_UNCONFIRMED`, not an assumed pass. This RFC cannot create rights that a registrant does not have.
* `R_full`: repositories in `D_all` for which an independent party has reproduced source and declared task/context, built the full artifact, passed **real-size** consensus admission, and obtained either (a) an accepted on-chain class registration or (b) reproducible evidence that the exact same artifact/program/tokenizer/layout/task/context is already represented by an accepted registration (§11.2). The evidence must link the source revision to the existing class and its chain-state proof; a similar name, model card, shared weights alone or unverified equivalence assertion does not count. Reusing an identical registered class is an idempotent success, not a second registration or added reward weight. A narrow text-only extraction of a vision/audio model, a 512-context redefinition of an 8k/2M model, header preflight, a published-only artifact, and market creation are not `R_full`. A genuinely admitted zero-share class may count; mining readiness and `Final` are separately reported and do not automatically follow from registration.
* Primary target: `R_full / D_all ≥ 0.90`, **and**, when a probability sample is used, a predeclared one-sided 95% lower confidence bound for this same repo-weighted ratio ≥ 0.90. Publish point estimate, bound, exact raw successes/failures and stratum weights. Also publish `R_full / D_files`, `R_full / D_rights`, the actual on-chain registration count, download-weighted demand, unique artifact roots and feature-family coverage. None substitutes for the primary ratio.

Rights/access evidence is an off-chain feasibility and publication condition, **not a proposed consensus licensing oracle, architecture allowlist or HF-account permission gate**. The chain verifies commitments, signatures and computation rules; it does not decide whether an HF username owns intellectual-property rights. Do not introduce a central approver under the label of permissionless registration.

An immediate feasibility check is `D_rights / D_all ≥ 0.90`: `R_full ≤ D_rights`. If gated, missing, legally unavailable or unresolvable repositories alone exceed 10%, **no converter, extra CPU or RFC wording can make the all-public-listings target true**. Report the actual ceiling and seek access/rights or explicitly revise the objective with the user; do not quietly switch the headline to `D_files`. A repository with uncertain rights remains a failure until evidence exists. The snapshot must disclose whether HF listings, rather than distinct checkpoint weights, are the intended unit; popular mirrors do not prove family breadth, so artifact- and feature-weighted results remain mandatory diagnostics.

#### 8. Current evidence, and how much remains unproven

The existing [HF coverage analysis](../design/palw/tir/hf-coverage.md) reports **90/100 representative architecture entries labelled A/B** (§23). This is not uniformly a completed full-weight harness: the underlying report includes route-only entries and `chatglm3` with `fixture.available: false` while still labelled B. Its curated corpus is not a probability sample of HF repositories. The analysis also excludes non-text models from an earlier architecture denominator (§2). Its 2026-09-28 Hub-count-based calculations (§§18–20) estimate about **3.10 million** model repositories and roughly **505,000 / 16.3%** potentially registrable after proposed generic-class work. These are **estimates**, not a pinned, audited `R_full` census; they include model families and fences not proven at real size and therefore are not a current registration percentage. RFC-0002 §II.10 reaches the same conclusion. The real 9B refusal in §1 is a counterexample to treating “lowerable” as “registrable.”

On that historical *illustrative* 3.10-million denominator, 90% is about **2.79 million** repositories. Even taking the optimistic 505,000 estimate at face value leaves about **2.285 million additional full-task registrations** to justify. This arithmetic is not a forecast; it shows why improving only the 9B converter, 2M context, or GGUF import cannot substantiate an all-HF 90% claim. The status at the time of this draft is **UNPROVEN**. No live all-Hub census, rights ceiling, real-checkpoint registration sample, or one-sided lower bound is present in this worktree.

The appropriate empirical sources are the official [HF Hub API](https://huggingface.co/docs/huggingface_hub/en/package_reference/hf_api) (`list_models`, revision-pinned `model_info`, `list_repo_tree`), [model cards](https://huggingface.co/docs/hub/model-cards) and [Diffusers loading metadata](https://huggingface.co/docs/diffusers/main/using-diffusers/loading). A `transformers`-only `config.json` inventory necessarily misses other Hub libraries; these are not “not models” and remain in `D_all`.

##### 8.1 Recount of the checked-in evidence, not a projected success rate

The accompanying [audit JSON](../evidence/rfc/0011-existing-coverage-audit.json) pins SHA-256 hashes of the reports and their harnesses. The [read-only audit script](../evidence/rfc/0011-audit-coverage.mjs) recomputes these results from repository files; it does not execute or register models, enumerate HF, or replace missing evidence with a prediction.

| Evidence scope | Observed saved result | What it establishes |
| --- | --- | --- |
| Curated architecture report | A/B 90/100 | Expressibility labels under this harness; not 90 accepted registrations. |
| Additional architecture report, misleading if read as a Hub census | A/B 54/91; C 37/91 | More causal-LM-family fixtures, not a repo-weighted probability sample; unsupported tail still substantial in this fixture set. |
| Pinned shape preflight, `convert` | ok 87, blocked 13 | Conversion-stage verdict on light specs. |
| Same preflight, `register` | ok **74**, blocked **4**, unknown **22** | Static stage verdict, not a signed stateful registration. |
| Same rows, `convert` AND `register` | **73/100** | Stage counts are not a sequential funnel: `minicpm` has blocked conversion but ok registration-stage status. |
| A/B AND both preceding preflight stages | **72/100** | Even this intersection is only fixture evidence, not HF registration coverage. |

The preflight [harness](../../misaka/palw/sdk/tests/corpus_preflight.rs) explicitly writes **header-only safetensors** (tensor data absent), defaults to `Depth::Shape` and `max_context = 128`, and compares verdicts against golden JSON. It does not send registration transactions. The architecture [harness](../../misaka/palw/tir/lower/tests/corpus_v2.rs) principally uses tiny random-initialized models. A `heavy_fixture: true` report field therefore does not by itself mean a production-sized upstream checkpoint was tested. These limitations prevent any table entry above from being used as `R_full/D_all`.

The four pinned registration blockers are concrete regression targets: `deepseek_v32` operand-count admission; `deepseek_v4` dissection root-claim bytes/court cost; `gemma3n_text` program-byte limits; `internlm2` tile MACs. The 22 unknowns include encoder–decoder, vision, diffusion and audio entries. Classify and actually exercise those routes instead of promoting `unknown` to supported. The 9B fix alone does not address these independent gates.

Reproduce the audit without modifying source or connecting to a node:

```sh
node docs/rfc/evidence/0011-audit-coverage.mjs --check
node docs/rfc/evidence/0011-audit-coverage.mjs --self-test
```

If report bytes change, `--check` must fail until the evidence and explanation are deliberately regenerated/reviewed. A passing audit checks provenance and arithmetic only; its verdict remains `UNPROVEN` for HF registration coverage. Missing tests, unrun later stages and a permissive label never become evidence through this audit.

#### 9. Required coverage work, selected by measured lost repositories

The following are **candidate workstreams**, not unsupported claims about their present share. The census must quantify each bucket first and then attach before/after evidence to a reusable implementation, so the combined *remaining* blocker count is at most 10% of `D_all` with uncertainty included.

| Full-task blocker family | Required generic route | Passing evidence; no shortcut |
| --- | --- | --- |
| Text generation, MoE, recurrent/hybrid, 8k–2M context | RFC-0002 TIR / active versioned kernel relations; new families require the §16 upgrade route, not a VM fallback. Real-size close sizing, prompt attribution, calibration and court fixes in §§2–4. | Full configured context and source-equivalent task pass actual chain admission; 9B and 2M examples are adversarial fixtures, not representative counts. |
| Text encoders, classifiers, rerankers, seq2seq and embeddings | Task-specific canonical input/output and pooling/heads, tokenizer and labels, bidirectional/encoder–decoder execution; no decoder-only inference from a familiar backbone. | Each advertised output and preprocessing path reproduced by independent vectors and the court; real checkpoint registered. |
| Image classification, detection, segmentation, generation/editing, vision-language | Full image preprocessing, backbone/projector or diffusion/flow pipeline, output profile and input commitments through RFC-0003 or an equally exact route. | Complete vision task, not the text subnetwork, passes exact job, artifact, DA and court tests. |
| Speech, audio, video and other temporal or multimodal tasks | Canonical media encoding/preprocessing, stage pipeline, bounded long-sequence/reduction dissection, exact output forms. | End-to-end intended modality and resolution/duration; missing RFC-0003 profile is an explicit blocker. |
| GGUF, GPTQ/AWQ, bitsandbytes, MLX and other quantized or framework-specific formats | Declarative, versioned importers and per-format tensor/scale/rounding semantics; offline sandbox for untrusted code. | Real upstream files and independent decoder cross-check; no implicit dequantize-and-requantize that changes the claimed model silently. |
| LoRA/PEFT, merged variants, custom-code architectures and composite repositories | Resolve every pinned base, adapter, tokenizer and custom component; lower into an active kernel's bounded tensor/state and constraint grammar. | Whole dependency graph and source equivalence, not a generic `model_type` guess; custom source is conversion input, never an uploaded verifier or guest executed by consensus. |
| Gated, missing, ambiguous or legally unusable sources | Access and rights evidence, archive/revision pinning, or upstream repair. | If unresolved, fail `source` and remain in `D_all`; software cannot manufacture absent weights or permission. |

An architecture adapter is sufficient **only** when the complete operation is expressible under armed semantics. Otherwise a missing general primitive, task profile, format semantics, DA route or court path requires a versioned kernel upgrade and an actual fence (§16). RFC05's former GVM residual is withdrawn: unsupported operations remain `KERNEL_EXTENSION_REQUIRED` until the complete new kernel route is implemented and active. Keep model-file size, operator hardware and court envelope visible separately. More capable machines can satisfy finite execution budgets; they cannot validate an unbounded or unrepresentable class by declaration.

#### 10. Reproducible census, experiment, and release decision

1. **Freeze the frame.** Save the complete cursor-paginated Hub listing, crawl start/end UTC, immutable repo SHA resolution, task/library/gating/license metadata, file tree identifiers/sizes, and API/client version. A multi-page live crawl is an observation interval, not an atomic historical Hub snapshot; document and reconcile added/deleted/renamed repos, retain unresolved IDs as unknown failures, then freeze the resulting frame. Hash and publish the manifest. The crawl must reach the end cursor; interruptions resume from a recorded cursor without treating uncrawled pages as supported. File errors and `unknown` categories stay in `D_all`.
2. **Predeclare the experiment.** Stratify by advertised `pipeline_tag`/task, library, format, parameter/bytes band, context band, age and rare/unknown features. Record a deterministic random seed, inclusion probabilities, sample size powered so its **one-sided 95% lower bound could exceed 0.90**, and a holdout snapshot not used to select features. Include both a probability sample and a separate stress cohort of top-download, new-family, custom-code, largest-size and 2M-context repos. Convenience samples and the 100-architecture corpus never estimate `D_all` coverage.
3. **Record the full funnel.** For every sampled repo, retain source/rights → full-task frontend → real-weight pack and fidelity → consensus admission → accepted registration → qualified seats → claim/`Final`. Mark `NOT_RUN_AFTER_<gate>` when blocked; it is not a pass. Preserve original task/context and exact source revision. Each failure has stage, code, needed/cap values, active fence, retry provenance and independent reproduction. A censored sizing result states “≥ cap+1,” not an exact work count. Existing RFC-0002 §II.10.3 gate names are the baseline.
4. **Prove each sampled registration.** Where the repository can legally be fetched, use real checkpoint bytes and the actual network rules/binary; produce an accepted class-registration transaction or verified equivalence to an existing registration (§11.2), with block/DAA/class ID, artifact root and node/SDK matching verdict. Verify the resulting class record, not merely transaction inclusion: the processor can drop a lifecycle object while its carrier block stands. If cost requires an isolated testnet, run the same consensus rules with a separately named test chain, then cross-check a subset on the intended public testnet. Offline preflight alone is `registration-ready`, **not** `R_full`. If a sampled model cannot be fully exercised, it is a failure/unknown outside the numerator, not projected success.
5. **Calculate without optimism.** Estimate repo-weighted `R_full/D_all` using the predeclared stratified design and a valid one-sided lower bound (finite-population/stratum weighting disclosed). Include inaccessible/rights-unknown and nonresponding samples as failures; report best-case eligibility ceiling, per-stratum estimates, a family-level holdout and the exact unsupported-repo list. Guard against a large copied architecture masking whole missing tasks. Re-run on a newer snapshot to detect model-distribution drift.
6. **Iterate by lost count.** Prioritize generic workstreams by observed lost **repositories**, not only downloads or a striking example. For each change, publish blocker count before/after, regressions, full-task fixtures, source equality, on-chain sample and the new lower bound. If the all-Hub bound remains below 0.90, RFC11 remains open regardless of decoder-only or local-LLM success.

Acceptance is conjunctive: the crawl is complete and pinned; rights/access ceiling permits 90%; the estimate's one-sided lower bound is ≥90% for **on-chain full-task registration over `D_all`**; no sampled success rests on a partial task, short-context substitution or unarmed rule; the release binary has bounded validator work, exact court and DA tests for every newly armed semantic route; and mining/`Final` rates are reported separately, never implied by registration. If any item lacks evidence, the allowed statement is the measured narrower result and its blocker distribution—not “RFC11 implementation registers 90% of HF models.”

#### 11. No hidden registration blocker: exhaustive gate-closure contract

The 9B sequence (`metadata` → `tile MAC` → `close-sizing` → possible later gate) demonstrates why removing the **first** error is not removing the barrier. A successful implementation must expose and close the *whole* path, including its transaction acceptance, for every claimed success. The current authoritative surfaces include [`palw_tir_admission_v1.rs`](../../consensus/core/src/palw_tir_admission_v1.rs), [`PalwClassAdmissionError::code`](../../consensus/core/src/palw_class_admission_v2.rs), the SDK's [`tir_registration_preflight_v1`](../../misaka/palw/sdk/src/tir_registration.rs), [`preflight`](../../misaka/palw/sdk/src/preflight/mod.rs) and the processor's `ClassRegisteredTirV1` acceptance arm. `model add --manifest` also has a verifier path. A frontend PASS that bypasses any one of these is insufficient.

##### 11.1 Inventory every gate, not only the first refusal

| Gate family | Current/likely blocker | Closure obligation and proof |
| --- | --- | --- |
| Source, scope, rights | Missing/gated weights, unresolved base/adapters, `CONFIG_INVALID`, partial advertised task, unsafe custom code or `RIGHTS_UNCONFIRMED` | Immutable complete source graph; owner/access evidence; full-task input/output and external-code isolation. A missing or unauthorized source is an **external** failure, not “fixed” by implementation. |
| Frontend and material | `ARCH_REFUSED`, `ARCH_NEEDS_PRIMITIVE`, unknown quant type, unconsumed tensor, tokenizer mismatch, `calibrated_context`/`--stats-in` mismatch, fidelity or reproducibility failure | Versioned adapter over active kernel semantics, or an explicitly required kernel extension; all weight/layout/rounding keys consumed, independent reference comparison, same root on independent workers and full declared context. |
| Protocol availability | `FENCE_NOT_ARMED`, wrong `prim_set_id`, absent task/job/output profile, legacy-only prompt attribution | Required rule actually armed at the target DAA with a pinned fingerprint; direct-TIR remains open, new primitives use a reviewed fence, and >4,096 prompt IDs have a complete IR proof route. |
| Static admission | `TIR_EXCEEDS_CEILING`, tile MAC/elementwise/transcendental budget, close-sizing `2^26`, range/bounds, canonical job, PWU, ladder, credit, context and history bounds | A candidate layout or mathematically equivalent bound passes the *same* chain gate under worst-case real-size inputs; no unmeasured ceiling waiver. All later checks are run after each fix. |
| Court and DA | `TIR_NEEDS_DISSECTION`, `TIR_DISSECTION_REFUSED`, close bytes, unavailable history/tile, court window, unanswerable DA challenge | Every reachable commit point has an exact one-move or bounded dissection route, a feasible signed DA response and deterministic worst-case bytes/work/time; independent false-claim court drill. |
| Registry and transport | SDK duplicate-root filter, class ID mismatch, source/class ambiguity, registrant's active bond and signature, exposure plus burn, zero-share rule, activation horizon, per-block registration cap, carrier assembly, transaction inclusion, reorg/pruning | Deterministic IDs and signed sponsorship path, idempotent reuse of exact classes, stateful preflight and retryable capacity results; end-to-end **class-state** acceptance and recovery after reorg. No genesis/fleet-only permission. |
| Pre-submission tooling | `PACK_NOT_VERIFIED`, artifact mismatch, required conformance SKIPPED, incomplete declared layout | CLI/tool blockers, not chain refusals; exact identity/conformance must pass before the normal CLI submission route. RFC13 defines remediation without overriding verification. |
| Post-registration only | `SEAT_MEMORY_SHORT`, insufficient independent seats, Panel backlog, absent `Final`, market not opened | Report separately as `mine`/`serve`/`market`. These do not retroactively make an accepted class unregistered, but no UI may label it mineable or sell benefits as ready without the corresponding proof. |

This table is **not** a manually maintained claim that no other gate exists. CI must enumerate every `PalwClassAdmissionError` code, every stable SDK preflight blocker and every processor/manifest rejection reachable for a class registration. Add an exhaustiveness test so a new refusal cannot ship without a stage, structured `code`, `needed/cap/fence`, remediation category, adversarial test and coverage-census bucket. Record all blockers per candidate by independent staged checks even when production validation stops at the first refusal; `NOT_RUN_AFTER_<gate>` remains explicit where subsequent checks require unavailable data. Compare the manifest verifier, SDK, mempool and accepted-chain result at the same ruleset/DAA, including genesis/reorg replay. Any disagreement is a **tool/protocol bug**, not a model failure to ignore.

For the 9B 8k target, a release demonstration MUST use the original pinned source and the full 8k artifact, show fixed provenance, the sufficient close-sizing bound, tile/court/DA/window results, a successfully accepted `ClassRegisteredTirV1` (or proven equivalence to the exact existing class) and matching node/SDK verdicts. Then show the seat shortage as a separate readiness state. For 2M, use a real declared 2M model with 2M calibration evidence, >4,096-ID prompt attribution, worst-case history dissection, finite resource/court bounds and an accepted registration. A 512-context substitute, synthetic tiny net or header-only test cannot close either case. If the next refusal appears, the case remains **open** until it too is resolved and retested through acceptance. The separate full-HF task/context requirement still applies before either case contributes to `R_full`.

##### 11.2 Exact duplicates must be idempotent, without inventing a new mandatory registry

The SDK's [`tir_registration_candidate_v1`](../../misaka/palw/sdk/src/tir_registration.rs) removes both already registered class IDs and **already registered artifact roots**. This client-side test is broader than the fold's `claim_artifact_root`: its ownership key is `(class_id, artifact_root)`, not a globally unique artifact root. Do not infer that consensus forbids every distinct class sharing weights, or introduce a new mandatory on-chain source-binding transaction solely to work around this SDK filter.

Change the SDK result to `ALREADY_REGISTERED` **only after** checking the complete class identity against accepted chain state. The coverage evidence binds `repo_id@sha`, source-component hashes, frontend/quantization versions, program, tokenizer, layout, full task/context and artifact root to that class. Independent re-conversion supplies the equivalence evidence; a signature or URL alone does not. Exact mirrors then need no new fee, transaction or reward weight. Preserve and report lifecycle status: reuse cannot rehabilitate a `Frozen` class or skip the fresh signed/reserved transition for Dormant reactivation. Different context, tokenizer, task or program is not an alias: run the normal class gate and stateful ownership checks instead of rejecting it merely for sharing a weight root. Test both equal-class reuse and different-class/same-root registration. Preserve existing ownership, work-credit and payout rules; this route confers no right to impersonate an HF publisher or claim its rewards.

The current post-genesis path also requires an **active registrant bond and a signed object** in the virtual processor. Provide a permissionless remote-builder/relayer workflow using those existing signed objects so a registrant need not run inference or a Panel locally. A sponsor may pay only under explicit, signed terms distinguishing the bond signer, publisher attribution and model-line economic owner; being a transport relayer gives no signing or ownership authority. If preserving a distinct publisher's on-chain ownership requires new authority fields, specify and fence that change rather than silently assigning ownership to the sponsor. Exposure, the registration burn and transaction fees remain deliberate anti-spam/economic conditions, not CPU errors. The dry-run must price them against current reserved and slashable balances, not merely test that the bond exists.

##### 11.3 A proof obligation for “more hardware solves it”

For every candidate blocked by local compute, attach a *witnessed resource plan*: pinned source/artifact bytes, streaming peak RAM, disk/net transfer, calibration and conversion steps/time on named CPU/GPU, producer latency, seat RAM/VRAM/storage, number and independence of seats, DA fetch bandwidth, worst court work/bytes/window and node admission steps. Provide an actually executed reference run and a second independent verifier where required. A user-supplied `--seat-share` or a declared fast GPU is **not** proof of capacity. The work may be delegated or sponsored, but must finish within the protocol's real deadlines. If any protocol bound still refuses after more resources are supplied, label `PROTOCOL_LIMIT` and design a safe fenced change; never report `ADD_CPU` as its remedy. If no finite reproducible plan is known, the case is unproven, not “guaranteed with enough hardware.”

##### 11.4 Zero-surprise acceptance test and permitted language

The release candidate's coverage report MUST have a machine-checkable row for every sampled repository containing source/task/context SHA bindings, all tested gate codes, independent artifact build roots, chosen rule/fence/DAA, accepted class-state proof and transaction (or verified reuse) evidence, and a separate `mine_ready`/`market_open` result. A random independent auditor reruns the entire acceptance path, not just the preflight. Mutation tests include: one missing weight, forged calibration length, wrong quant rounding, too-wide tile, close-sizing sentinel, 4,097-ID and 2M prompts, false Merkle tile, late court move, unavailable DA shard, same-root/different-context selection, unbonded or wrong signer, depleted exposure, stale fence view and a one-block reorg. A valid case reaches acceptance; each invalid case fails at the named gate without node CPU/memory exhaustion. No unexplained `ADMISSION_REFUSED` bucket remains among claimed successes.

**Only after §§7–14 pass may documentation say “the implemented release can register at least nine of ten public HF model repositories in the named snapshot.”** If based on sampling, this is an estimate at the stated confidence, not a measured success for every repo. Quote the snapshot, numerator/denominator, lower bound, ruleset, full-task definition and residual failures. “All models, every context, no barriers” is not a defensible protocol guarantee: inaccessible weights, missing permission, non-finite work and an absent sound court cannot be made valid by a software patch. The implementable guarantee is **no undocumented or arbitrarily hard-coded registration blocker for a source-complete, authorized, finite, deterministically representable model inside an armed, provably adjudicable envelope**; outside that envelope the precise missing capability or external constraint is reported and counted against the 90% target.

For registrations under the new probabilistic route, §15's verification-plan and soundness requirements are additional release conditions for the coverage claim above. A registration percentage cannot substitute for computational soundness.

#### 12. Concrete remaining limits: replace aggregate cliffs, retain bounded verification

The following code facts were checked at the audit commit. They are independent checks, not predictions that the 9B artifact exceeds each one. **More CPU on a worker changes none of these consensus comparisons.** The implementation must resolve every relevant row for each target profile; a single new context flag does not suffice.

| Current gate and source | Current bound / behavior | Required design change or retained condition |
| --- | --- | --- |
| TIR shape/encoding, [`program.rs`](../../misaka/palw/tir/core/src/program.rs) | 256 KiB encoded program; 16 blocks; 512 nodes/block; 1,024 layers; 4,096 parameter declarations; 64 state declarations; 8 inputs/node. These are **declarations**, not a 4,096-weight parameter limit. | Reuse structural templates where semantically exact. For models beyond this grammar, versioned kernel graph composition with typed interfaces and bounded summaries; missing grammar/semantics requires an activated extension, not a VM. Do not raise parser/allocation caps without metering or assume every architecture fits this grammar. |
| Network TIR ceilings, [`PALW_T12_TIR_CEILINGS_V1`](../../consensus/core/src/palw_tir_v1.rs) | 88,000 program bytes; 65,536 unrolled nodes; 262,144 positions; `2^37` MACs/position; 32 GiB state; 4 GiB peak live; 65,536 cone-work units. | Separate **total producer resources** from **maximum validator/terminal work**. A versioned resource envelope may allow larger total work/state when bounded segments, authenticated paging, complete DA and court verification cover it. Those aggregate caps remain effective until that route is armed; available RAM alone is insufficient. |
| IR held/context and canonical input, [`verify_class_admission_v10`](../../consensus/core/src/palw_tir_admission_v1.rs) | Explicit refusal of `HISTORY_BOUND_V1_HELD`; canonical prefill ≤4,096 IDs, thus context ≤32,783 independently of the larger network cap. | Versioned IR held bindings plus attributable prompt tiles, with predecessor-state and input challenges. Implement all three context changes together, not by waiving one check. |
| Per-tile court and recurrence | MAC, elementwise/transcendental work, operands and `(checkpoint_interval − 1) × state-replay work` are bounded independently. Smaller output tiles do not necessarily shrink a whole input/reduction or state replay. | Derive a legal tile/checkpoint plan. If none exists, split the reduction/state transition into committed, independently adjudicable steps preserving rounding and order. Supply a witness for **every** commit point, not just the largest matmul. |
| Close bytes and close sizing | Both the preliminary `program + frame + operands` test and the final carried close/root-claim bounds must pass; the final sizer stops at `2^26` units. Range sizing is already selected after fence2. | Profile actual repeated ranges/requests. Use proof-checked reusable summaries (§13), and reconcile preliminary vs actual-carriage accounting under a fence if it changes acceptance. A small estimate, faster worker or hash of an unchecked bound is not a proof. |
| Job step ladder, PWU and DA | Deepest permitted job must fit the network ladder; canonical counted PWU must agree. [`PALW_TIR_DA_SEAT_REACH_LEAVES_V1`](../../consensus/core/src/palw_tir_court_v1.rs) is `2^30` leaves from three 10-level descents plus a terminal session. | Segment large execution with authenticated global coordinates/state and disjoint credit. Price segment-directory proofs and all DA descents; never reset a leaf counter to evade the reach bound. Every allowed job, including noncanonical jobs on other lanes, needs an explicit reach proof. |
| Court deadline and material | Every root claim, dissection round, terminal close and assembly must fit its carrier and time budget. | Preserve a bounded terminal court; compose segment proofs and derive total worst-case deadline/escrow retention. Benchmark an adversarial last-leaf dispute and missing material, not only an honest receipt. A model may not select its own arbitrarily long timeout. |
| Stateful registration, [`apply_class_registration_v1`](../../consensus/core/src/palw_state_v2.rs) | Duplicate non-Dormant class; network slash price and target; nonzero PWU; activation no more than 4,000 DAA ahead; affordable reserved exposure plus registration burn and existing slashable locks; share and ownership checks. | Shared stateful dry-run with precise retry/remediation, including Dormant reactivation and accepted class lookup. These are security/economic conditions to satisfy, not checks to delete. The SDK's broad duplicate-root filter must not create an extra protocol rule (§11.2). |
| Inclusion capacity and replay | At most **one TIR registration per block**; the processor can drop further objects while keeping the block. Signed object acceptance, class state, IBD and pruning must all agree. | Return `QUEUED_CAPACITY`, `ACCEPTED` or a named refusal; retry idempotently after state refresh. Increase throughput only with measured, bounded verification cost and a versioned admission-work budget. Do not treat a transaction hash or elapsed DAA as registration success. |

The inventory must also traverse nested decoder/layout errors, transaction mass/size, object chunks and rent, public-source requirements where armed, signature verification, state-root persistence and RPC visibility. Matching a list of public error strings alone cannot prove completeness: CI must associate the actual acceptance call graph and each early return with a gate ID, and run boundary/negative cases at each reachable gate. Proposed diagnostic codes in this RFC are not claims that all those strings already exist.

#### 13. Implementable route to removing the cliffs

##### 13.1 Verification-plan admission; probabilistic execution acceptance

The preferred route is `VerificationPlanV1`, composed from implemented, versioned kernel-primitive verifier templates. Registration checks the plan and resource envelope; later claims instantiate its randomized checks. The plan is bounded typed data, not executable verifier code or a universal circuit/ISA fallback. Existing exact range sizing remains available for compatible classes. This is a proposed protocol, not a certificate system already shipped.

1. Bind the network/ruleset, active kernel descriptor/version, source/program/tokenizer/artifact roots, full task/context, arithmetic and memory semantics, constraint compiler, verifier suite, segmentation and plan version. Bind the challenge schedule, field/modulus choices, repetition policy, maximum proof/witness sizes, resource limits and security target. Neither a registrant nor a seat may lower these below the armed network policy.
2. Use reusable templates for matrix relations, nonlinear/lookup constraints, routing and authenticated state/memory. Verify typed interfaces and composition over a compact graph or metered modules. Do not unroll every token, expert execution and possible dynamic index at registration. Dynamic access must have an authenticated read/write and bounds rule; routing must prove which expert is selected. The plan covers all constraint families and input/output bindings even though checking their instances is probabilistic.
3. Enforce deterministic limits on parser/allocation work, query openings, localization and terminal computation. Approved kernel-primitive courts and composed primitive bounds establish adjudicability without a separate symbolic proof for each reachable model commit point; this is not an arbitrary ISA/microcode court. If a primitive cannot be checked or localized within the current route, return the missing verifier/court capability. A sample cannot establish the worst-case safety of an unchecked operation.
4. Charge each module/rule, arithmetic width and evidence byte before allocation; cache under the full semantic digest. Large plans use metered composition with authenticated accumulated state, expiry and replay/reorg rules. Registration completes only after composition and stateful gates pass. Cold/warm cache and IBD must give the same result.
5. At claim time verify the committed constraints with the suite in §15. Accept at its reviewed error bound after positive evidence and the challenge window; full trace or full segment replay is not the normal-path completion condition. Preserve deterministic reference executors for conformance and exact terminal disputes.

The real 9B/2M acceptance cases and census remain required. A reusable verifier removes the need to analyze a giant execution exhaustively, but it does not automatically supply unsupported operations, field encodings, authenticated openings or a sound composition theorem.

##### 13.2 Segments preserve the model; they do not shorten its context

A segment identity binds `class/program`, job and input roots, full context policy, global layer/position interval, entry-state root, exit-state root, material root and predecessor identity. Tiling, module splitting and parallel scheduling must preserve integer rounding, overflow rules and ordered recurrence; even an associative real-valued expression is not automatically associative under the committed arithmetic. The boundary-state protocol must establish continuity from a verified initial state and permit a challenge to any dependency across segments.

Admission checks a covering partition: no missing or overlapping credited execution cells, no dependency cycle outside an explicitly bounded recurrence, and an exact ownership rule for shared boundary work. PWU/reward calculations charge original work once; splitting a claim cannot multiply ticket opportunities, subsidy or counted inference. A segment directory must itself have bounded inclusion/DA proofs and an adversarially tested dispute route. Total producer RAM/storage/time can grow with the model; per-validator verification, each terminal close, and the aggregate open-session exposure stay bounded by armed rules. This requires coordinated work with RFC-0006/0008, not merely accepting longer arrays.

##### 13.3 Cheap registration is not cheap correctness certification

Remove local inference from the **registrant client** requirement: clients may submit a prebuilt artifact commitment and proof-checked static plan made by independent workers. Full weights remain available through verified material providers. Registration is distinct from source fidelity and readiness, with those states explicit in both RPC and UI. `--stats-in` provenance can establish which computation is claimed, but proving the statistics/calibration correct still needs reproduction or a sound challenge/proof protocol before they count as verified.

For quantized models, exact agreement between independent TIR executors is mandatory; bitwise equality to the upstream floating-point runtime is **not** generally possible. Pin the transformation and predeclare numerical/task fidelity thresholds, corpus, maximum context and modality coverage. Report measured fidelity separately. An explicit, validated quantization may count under the census policy; silently dropping vision, shortening context, changing quant semantics or skipping the independent reference does not. Extending this particular 262k-configured source to 2M must be labelled an extension and independently validated, not counted as upstream 2M support.

##### 13.4 Deterministic eligibility, not a promise of immediate inclusion

Add a shared stateful dry-run which invokes both admission and the same fold guards used by acceptance, on an immutable parent-state snapshot. Report `ELIGIBLE_AT(parent_hash, ruleset, object_hash)` with exact required exposure/fee, activation range and remaining block budget. Before signing/sending, refresh changes in bond state, target, fence and ownership. An earlier result is not a guarantee across a state change or reorg.

The proof obligation is: **for identical parent state, ruleset and object bytes, with valid carriage and available per-block budget, the dry-run's accepted transition equals the node's accepted transition and class-state root contribution**. Differential tests exercise this property against production acceptance, not a mock reimplementation. Network liveness and a fair inclusion opportunity are separate assumptions; under congestion or censorship no RFC can promise that every eligible transaction will be included at a particular wall time. Block-cap deferral must be observable and retryable, not presented as model incompatibility.

#### 14. Required evidence bundle and completion checklist

Publish a versioned `registration-evidence/v1` bundle for each accepted case. These are required output fields, not fabricated run results:

* `source`: repo/revision, complete file/component hashes, advertised task/context and any declared transformation/extension; model-card/config evidence.
* `build`: converter and adapter commit, immutable calibration inputs/statistics, source-fidelity results, artifact/program/tokenizer/layout roots, independent rebuild/reference result and hardware/resource measurements.
* `rules`: chain genesis, code commit, consensus/schedule fingerprints, parent hash/state root, DAA, armed fences and all derived limits.
* `gates[]`: stable gate ID and source symbol; `PASS / FAIL / NOT_RUN / DEFERRED`; exact value or censored lower bound, units, cap, proof/witness digest, test result, dependency and remediation. `PASS` cannot be manufactured from an unchecked declaration.
* `acceptance`: exact object/transaction hash, accepted block, resulting class-state proof, or existing-class proof plus reproduced equivalence. Include object-level disposition, RPC observation, confirmation/reorg policy and cold-node IBD replay.
* `post_registration`: independent fields for capable seats, certification, first mined claim/Final, model-line listing and market availability. Missing Panel operation does not invalidate **registration**, and registration does not imply any of these later states.
* `verification`: verification-plan/suite version, constraint-coverage map, committed evidence and challenge/transcript digests, per-claim error derivation and assumptions, Panel rule, challenge-window result, and measured prover/verifier/DA/localization costs. Distinguish `probabilistic` from legacy `replay`; never label a sampling receipt as full recomputation.

Release checks are conjunctive:

| Case | Required result |
| --- | --- |
| Previous 9B failure | Old pinned build reproduces the named refusal; candidate passes every gate using the requested 8k semantics; accepted class state and RPC agree. No context reduction or skipped late gate. |
| Long context | Validated real 2M artifact, all three context barriers removed, worst-case court/DA/resource bounds and accepted state proof; upstream full-task/context coverage evaluated separately. |
| Broad coverage | Complete pinned sampling frame; declared full-task policy; real-source probability sample, missing/untested entries counted as failures; ≥90% lower bound and separate unsupported buckets. A statistical confidence bound is not a deterministic guarantee for every repository. |
| Anti-regression | Boundary tests at cap−1/cap/cap+1, malicious proof and material tests, manifest/SDK/fold parity, no-reference-skip acceptance, independent audit and cold IBD/reorg equivalence. |
| Operations | Offloaded workers replaceable; abort/resume reproduces roots; inclusion-cap retries and depleted-bond errors observable; no false “registered” based on transaction presence alone. |
| Probabilistic route | §15's whole-claim soundness review, single-fault/adaptive adversarial cases, constrained Final and bounded dispute/DA handling; no routine segment replay. Kimi K3 is a separate real-model scaling gate, not implied by the 9B pass. |

Until the bundle exists, the conclusion is **“known barriers identified; proposed closure path; registration/coverage unproven.”** RFC approval is not evidence of implementation correctness. Successful real registrations establish individual cases; a representative experiment establishes a scoped coverage estimate. Neither establishes support for all future architectures or unlimited contexts.

#### 15. Sampling-first / court-on-dispute (2026-10-06 decision)

##### 15.1 What is accepted probabilistically

Normal verification evaluates randomized **constraints on committed computation**, rather than re-executing whole sampled segments. The miner still performs the intended execution and constructs its evidence. All constraints affecting outputs, state and rewarded work must enter a sound check or a sound aggregation; this is coverage of the verification statement, not deterministic replay of all operations. Small cheap constraints may be checked exactly. The accepted mathematical statement is that a false committed execution passes with probability at most the suite's stated bound, under its stated assumptions. It is not a Bayesian claim that a particular passing execution has that posterior probability of being wrong.

For raw uniform sampling without replacement, with `N` segments, `b` bad segments and `s` queries, the miss probability is `C(N-b,s) / C(N,s)`. For a single bad segment it is `1-s/N`; at `N=100,000,000`, `s=100`, detection is only `0.0001%`. Checking a selected segment perfectly, or using Freivalds only inside it, does not repair that selection loss. The exact court only handles a detected/filed dispute; it cannot retroactively make an undetected lie detectable.

Accordingly, raw 32–128 segment spot checks may be diagnostic audits but are not sufficient for this route's Final or useful-work entitlement. Selection of a small number of encoded queries is permitted when the complete constraint-to-proof reduction has a reviewed error bound, including a single output-changing fault. Erasure-coding a false raw trace does not itself prove its execution correct; FRI proximity must be composed with computation constraints and boundary conditions. A public, non-ZK interactive proof/IOP is allowed. Calling it sampling does not remove its witness/prover cost.

##### 15.2 Constraint map and checker choices

[RFC-0007 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#part-v--constraint-verification-is-the-normal-panel-path-2026-10-06) supplies the Panel integration contract: batched token/cell verification, an evidence-bound `PalwConstraintReceiptV1`, explicit whole-claim/segment/audit scopes, coverage-aware tally and a MatMul-first delivery plan. The proposed receipt is distinct from the already existing `PalwSeatReceiptV3`; protocol versions and old claims must not be reinterpreted. That contract implements the direction here without lowering this section's coverage or §15.4's error target.

| Relation in the committed computation | Normal check | Exactness and evidence obligations |
| --- | --- | --- |
| Dense, attention and executed-expert matrix products | Freivalds projection or GKR/sum-check reduction | Bind both inputs, output, dimensions, weights and arithmetic before challenges. Authenticate projected evaluations against those same commitments. |
| Multiple layers/modules and many products | Random aggregation and a reviewed GKR/sum-check composition | Include every constraint and its wiring; derive degree/round/batch error, not a fixed `1/p` for an arbitrary circuit. Final input evaluations must bind the actual input and weight roots. |
| Quantization, rounding, saturation, carries, activation, normalization and comparisons | Bounded exact checks or range/lookup/bit constraints within the proof suite | Prove committed integer semantics, including ties and overflow. Replacing a nonlinear operation by an approximate polynomial defines a different execution unless explicitly part of the model identity. |
| MoE routing/TopK, dynamic indexing, embeddings and memory/history | Routing, index bounds, authenticated read/write and permutation/lookup consistency checks | Prove the chosen expert/address and read-after-write order. A correct multiplication using the wrong expert is invalid. Include all executed branches and prove their selection; unused branches need not be executed. |
| Segment boundaries, recurrence, inputs and outputs | Boundary equality and authenticated state-transition constraints | Bind the initial state, predecessor chain, full context, final output and disjoint work coordinates. Correct isolated segments with fabricated entry states cannot pass as a correct job. |
| Data availability | Committed material, bounded requests and a separate availability policy | Availability is not execution validity. Evidence must remain retrievable through disputes and retention; unavailable evidence cannot be a positive receipt. |

For `X ∈ F^(m×k)`, `W ∈ F^(k×n)` and claimed `Y`, Freivalds checks `X(Wr) = Yr`. For fixed false `Y`, a uniform independent `r ∈ F_p^n` passes with probability at most `1/p`; `t` independent repetitions give at most `p^-t`. A binary challenge vector instead gives the classical bound `2^-t`. These statements require a nonzero error **in that field**, fixed before the challenge. Integer differences that are multiples of the modulus require proven ranges, limb/carry constraints or sufficient independent moduli; mere modular equality is insufficient. See [Slalom, Lemma 2.1](https://arxiv.org/html/1806.03287v2).

Direct Freivalds costs `O(mk + kn + mn)` field work per repetition, including reading the matrices, versus `O(mkn)` classical multiplication. For single-token GEMV this need not save asymptotic work. Preprocessing/sketches can help fixed weights, but their construction, storage, authenticated relation to the artifact and refresh must be counted. Public `r` known before `Y` is fixed is unsafe. RFC-0007's private reusable sketches have different secrecy/reuse assumptions; do not expose them as a public reusable challenge or inherit their amortization for fresh public vectors.

GKR is the preferred candidate for aggregating compatible large constraint graphs; its cost depends on circuit depth, input access, wiring regularity and commitment openings. It is not an automatic constant-time verifier for arbitrary TIR. The [GKR theorem](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/) and [SafetyNets](https://arxiv.org/abs/1706.10268) motivate this route but do not establish performance for MISAKA's nonlinear, recurrent, multimodal graphs. A Merkle root alone cannot authenticate an arbitrary linear/polynomial evaluation cheaply: specify and price the opening protocol, a verified streaming pass, or a reviewed commitment scheme. Preserve MISAKA's post-quantum security requirements when selecting a scheme; pairing-based commitments cannot silently become a new assumption.

##### 15.3 Bind first, challenge later, preserve replay determinism

The proposed `VerificationPlanV1` and `VerificationEvidenceV1` are new schemas, not names of existing wire objects. They bind `chain_genesis`, `ruleset`, class/program/artifact roots, job/input and initial/final state roots, trace/constraint/material commitments, full semantic context, segment directory, suite version and security parameters. A claim cannot substitute a different output, expert, witness or plan after learning a challenge.

1. Carry the claim and required initial commitments, reserve its exposure, and bind the immutable `challenge_policy_id` of [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol).
2. Use only that protocol's qualifying future PALW work, canonical collection/lock, seed derivation and field/query sampler. RFC11 defines no independent seed formula. Source validity must be independent of the consuming claim; a header-valid attempt, heartbeat/BASE-0/EXEC hash or committee signature is not replacement entropy.
3. Each GKR/sum-check round fixes the prover's message **before that round's challenge**, under the policy-pinned staged-beacon or transcript-bound Fiat–Shamir mode in RFC07 VI.6. No all-future-challenges seed, prover-selected mode switch or unbounded transcript retry is permitted; the transform's own security review remains necessary.
4. Make roots/openings/transcripts available before their deadlines. Seats verify assigned relations or the aggregate proof and file positive signed receipts bound to evidence and suite. The baseline retains a versioned, explicitly specified Panel quorum: node replay checks signatures, assignment, roots, timing and lifecycle; it does not secretly replay the LLM. A receipt attests that a seat ran the suite, not that the node checked all its arithmetic. If compact public proof verification is later moved into consensus, meter it separately and define the new acceptance rule.
5. Canonical bytes and fixed transcript challenges yield identical verification/replay decisions. Nodes never use private runtime randomness to decide consensus. Reorg/IBD/pruning restore the same roots, seed derivation and outcome; a new anchor invalidates stale receipts as prescribed by the fence. Bound and account for retries rather than letting producers resample until success.

RFC07 Part VI is the normative construction contract; its source-unpredictability, bias, bootstrap, codec, parameters, transcript transform and replay gates remain open. Commitment opening and Panel tally also need complete implementation evidence. This draft does not prove a deployed beacon secure. §17 extends the same protocol to model onboarding; it does not replace per-claim checks by registration conformance.

##### 15.4 Whole-claim error budget and adversary

The plan must provide a reviewed conditional computational-check error `ε_check` for a **false whole claim**, not just one selected product. Include constraint selection, aggregation, polynomial degree, rounds, repetitions, field representation, boundary/memory checks and binding/opening errors. A conservative composition can sum component bounds; no independence is needed for a union bound. A missing constraint has no small error bound. For illustration, at most `2^20` separate matrix relations, each checked three times over a field of size at least `2^61` with fresh independent vectors, give the loose union bound `2^20 × 2^-183 = 2^-163` for that matrix-check component alone. This is neither a complete suite nor its real cost.

**Proposed cryptographic design target:** `ε_check ≤ 2^-128` per committed claim under honest execution of the verifier and the specified challenge/binding assumptions. This is a target to demonstrate and parameterize, not a measured property, a shipped constant, or “128 segments.” If unmet, the suite remains experimental and cannot silently lower the network threshold. The manifest declares the bound; the verifier derives/checks it from the approved suite and dimensions, rather than trusting the declaration.

Separately quantify probability of a compromised accepting Panel, challenge bias/withholding, selective DA and censorship. Quorum signatures do not multiply soundness by the number of seats, especially with shared public challenges; colluding seats can sign without checking. For a stated horizon with `Q` adversarial attempts/reorg retries and bad-assumption event bound `ε_env`, a conservative model is `P(any false acceptance) ≤ min(1, Q·ε_check + ε_env)`. Count off-chain Fiat–Shamir grinding in the applicable proof analysis as well. Do not describe the network as having 128-bit security solely from the algebraic target. Publish assumptions, maximum/estimated exposure horizon, economic incentives and sensitivity to correlated operators.

Full proof security needs analysis, not just successful random fault tests. PoSP-style economic incentives can discourage skipped work but are a separate rational-adversary argument. They do not supply a worst-case single-fault detection theorem.

##### 15.5 Final and exact dispute handling

Proposed state flow (wire names and migrations to be specified before implementation):

```text
Committed → ChallengeBound → Checking → ProbabilisticPass → WindowClosed → Final
                                 └─ mismatch / filed challenge → Disputed → exact verdict
                                 └─ missing evidence / deadline → DA or timeout outcome
```

`ProbabilisticPass` requires the complete suite's positive evidence and the specified receipt rule. Silence, missing rounds, absent required constraints or unavailable witnesses never count as pass. Final requires the armed challenge window to close, required DA/retention obligations, and no unresolved accepted dispute. Reward escrow and useful-work maturation follow this lifecycle. Any REAL claim's pre-Final fork-choice weight and aggregate adversarial exposure must remain bounded and drilled under ADR-0069. Revised RFC-0008's EXEC slices contribute zero fork-choice weight before and after Final; their pending reward/collateral/DA/court exposure still needs bounds, with one aggregate root Final and settlement. Sampling receipts must not immediately release an entire session's budget or duplicate slice credit. A single network-versioned policy applies to all claims using this route, not a model-selected confidence level.

A failing random check suspends acceptance and identifies an evidence/relation disagreement. It does **not** by itself convict a miner: the served witness may differ from the committed claim, or a verifier may be faulty. Localize via authenticated matrix tiling or proof/dissection into named TIR/kernel primitive transitions with bounded inputs, weights and entry state. Then the existing compatible terminal court recomputes the exact step; a new primitive requires a versioned court extension, not a generic VM fallback. A GKR mismatch does not automatically identify a false terminal leaf: the plan must supply and test that localization protocol, its total bytes, work and deadline. No emergency end-to-end replay of a multi-terabyte model is an acceptable hidden dependency.

Use the existing distinction between missing material, missed duties and proven false computation. Apply slashing only under its defined evidence/obligation rule; verifier unavailability is not producer arithmetic fraud. Rate-limit bonded disputes and bound simultaneous court exposure so an attacker cannot force unbounded fallback work. Exact fraud proof, dismissal and availability/timeout results must each have deterministic transitions. The protocol accepts the residual chance of undetected invalidity after Final; court does not make it zero, and this proposal does not invent a post-final rollback rule.

##### 15.6 Kimi K3 and large-model feasibility gate

The official [Kimi K3 model card](https://huggingface.co/moonshotai/Kimi-K3) describes a 2.8T-parameter multimodal model, KDA/Attention Residuals and a million-token context (consulted 2026-10-06). This makes it a concrete stress target for the selected architecture. Pin its exact source revision, tokenizer, quantization, full modality and context before testing. A text-only or short-context extraction is a separately named partial result. The card is evidence of the target, not evidence of MISAKA compatibility or a successful benchmark.

**Engineering decision:** for MISAKA's intended large-model path, normal verification must use bounded probabilistic/algebraic constraint checks instead of requiring every seat to repeat full inference or every queried segment in full. This is the chosen way to seek affordable Kimi K3-class verification. No theorem establishes that sampling is the only possible architecture, or that parameter count alone makes every replay scheme infeasible. Existing RFC-0007 §II.9 measurements also show that algebra can lose to replay at low bandwidth or on prefill-heavy work; they are not Kimi K3 measurements.

The benchmark report must include producer execution **and** witness/proof generation, cold and amortized weight preprocessing, verifier field work, total proof/opening bytes, network transfer, RAM/VRAM/storage, time to positive receipts and Final, worst localized dispute cost and attack-induced court load. Compare against the same deterministic replay baseline on named hardware for prefill, decode, recurrent state, routing and multimodal paths. Report separate registration, verification-feasibility and mining outcomes. No automatic “all models” claim follows from adding Freivalds/GKR.

##### 15.7 Activation tests and implementation boundaries

* **Arithmetic and binding:** independent reference vectors; single incorrect output scalar, modulus-alias error, rounding/carry fault, wrong expert/TopK tie, memory permutation, forged entry state, uncommitted projection, omitted constraint and duplicate work. All relations must be covered; toy-field trials check predicted distributions, while the production error target needs a reviewed proof.
* **Adaptive adversary:** producer fixes outputs after seeing a vector; reused/public sketches; prover learns future sum-check challenges; multiple commitments, abort/retry, beacon withholding/bias, selective availability, correlated seats and reorg reuse. Test rejection or explicitly bound remaining risk.
* **Court and resource bounds:** isolate a late single fault within the largest permitted model/context; prove exact terminal equivalence without whole-model replay; measure total localization bandwidth/work/time and concurrent disputes. Withheld data exercises the DA path rather than an invented arithmetic conviction.
* **Lifecycle and deployment:** old claims finish under old rules; plan/suite enters identities, signatures and fingerprints; independent node/SDK/manifest results and cold IBD agree; Final cannot bypass the window or an active dispute. Introduce a separately named dormant fence (proposed `palw_probabilistic_constraints_v1`), no activation height in this RFC. Compare in shadow mode before allowing reward-bearing acceptance.
* **Acceptance evidence:** retain §§7–14's coverage audit; add actual 9B-8k, validated 2M and Kimi K3 scaling reports for the new route. A conformance replay may be expensive once; normal verification must demonstrate the claimed reduction with all costs included. Unbuilt verifier families and unrun model cases remain open.

##### 15.8 Primary sources and what they establish

* [Slalom (2018/2019)](https://arxiv.org/abs/1806.03287): Freivalds-based linear-layer verification, including preprocessing and TEE assumptions. Its throughput results are not MISAKA/Kimi results; MISAKA does not inherit trusted hardware merely by citing it.
* [GKR (2008)](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/) and [SafetyNets (2017)](https://arxiv.org/abs/1706.10268): interactive verification of appropriate circuits. Input access, depth and exact operation encoding remain costs to resolve here.
* [FRI, TR17-134](https://eccc.weizmann.ac.il/report/2017/134/): low-degree proximity checks, a possible ingredient in a computation proof; not standalone evidence that arbitrary trace values follow a model.
* [Celestia DA documentation](https://docs.celestia.org/learn/celestia-101/data-availability/): erasure-coded availability sampling. Available data may still encode an invalid computation.
* [PoSP/spML](https://arxiv.org/html/2405.00295v3): probabilistic challenges, recomputation and economic arbitration assumptions. Its selected validators recompute the function; it does not prove this RFC's sub-execution checker sound.
* [opML (2024)](https://arxiv.org/abs/2401.17555): interactive ML fraud-proof precedent. MISAKA still needs its own binding and bounded localization implementation; a dispute system cannot detect a lie no participant challenges.

##### 15.9 Selected research basis: economic challenge, MatMul-first checks and bounded disputes

**Explicit exclusions:** no TEE/enclave/SGX trust assumption for computation validity; no model
execution/fraud-proof VM; no spML-style BFT orchestrator committee, trusted operator PKI or committee
beacon as a protocol authority. These concepts are not alternative implementations or future
dependencies of this route. Use public consensus identity/bond rules, independently specified
post-commit randomness, approved kernel checkers and the existing bounded PALW court. Existing
native EVM and required historical validation are outside this model-VM prohibition.

**2026-10-06 clarification requested by the operator:** “small probabilistic checks” means this
combination of established ideas, not inventing a raw few-segment trust rule. Review comparison
baselines in the following order before changing the acceptance policy; engineering starts with
the matrix checker. These are component precedents, not a theorem for the combined MISAKA protocol.

| Priority / primary comparison | What to use | What does not transfer automatically |
| --- | --- | --- |
| 1. [PoSP/spML v3, 2025](https://arxiv.org/html/2405.00295v3), §§2–3 | Economic challenge probability, verifier selection after commitment, bonded rewards/slashing, arbitration and failure handling | Selected validators recompute `f(x)`; the Nash-equilibrium result has rationality, cost, collusion and accurate-arbitration assumptions. spML also assumes BFT orchestrators/beacon and trusted chain/PKI. This is neither sublinear algebraic-check soundness nor authority to add DNS validators. |
| 2. [opML v2](https://arxiv.org/html/2401.17555v2), §§3, 5–6 | Interactive localization of a disagreement, execution/proving separation and bounded terminal arbitration as a comparison target | Its FPVM is not adopted. MISAKA localizes to an approved tensor/state kernel transition, not an ISA step. A participant must actually discover/file a fault. |
| 3. [Freivalds / Slalom](https://arxiv.org/html/1806.03287v2), Lemma 2.1 / §3 | First implement and measure committed matrix-product checks, batching and fixed-weight preprocessing | No inherited TEE, secret-reuse assumptions, field/quantization choices or reported speedups. Public-challenge openings and MISAKA integer semantics need their own construction. |
| 4. [Celestia DAS](https://docs.celestia.org/learn/celestia-101/data-availability/) | Coded data/commitments make substantial withholding detectable by a few availability queries | DA error amplification concerns missing data, not an incorrect scalar or transition. Merely erasure-coding a wrong trace cannot certify inference. |
| 5. [GKR](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/), [SafetyNets](https://arxiv.org/abs/1706.10268), [FRI](https://eccc.weizmann.ac.il/report/2017/134/) | Reviewed constraint aggregation / interactive proof / proximity components when their complete relation binding and measured cost justify them | Circuit/proof machinery does not authorize a model VM, arbitrary uploaded circuits or omitted nonlinear/state relations. Proximity alone does not prove execution. |

###### Implementation sequence and the measurement that decides it

1. Profile a source-pinned deterministic model: record fractions of execution work **and time**
   for GEMM/GEMV, attention, active-expert products, routing, nonlinear/range and authenticated
   state. Do not assume matrix multiplication dominates every model/device/task just from its brand
   or parameter count. Kimi K3 remains §15.6's measured full-task stress target, not proven support.
2. Implement the §15.2 Freivalds relation as a versioned checker kernel. Fix every matrix and
   material commitment before challenges; authenticate projections. Compare exact multiplication
   with fresh public-vector checks and a separately specified preprocessing variant on identical
   inputs/weights. Measure prefill batched GEMM, single-token GEMV and executed MoE experts separately.
   Include producer witness cost, cold/warm setup, retained sketches, openings, bytes, RAM and
   end-to-end receipt latency. A speedup in multiplication alone is not an end-to-end win.
3. Cover the remaining routing, quantization, nonlinear, memory and boundary relations using
   approved exact small checks or reviewed algebraic/lookup suites. Add GKR-style aggregation only
   with a bound for the **whole committed relation**, including sparse faults and all input openings.
   Add a computation-encoding/FRI route only with a demonstrated constraint-to-encoding reduction.
4. Localize failed checks to compatible bounded exact court with tested worst-case work/bytes/time.
   PoSP-style challenge/reward and timeout analysis must price this new checker/localization cost,
   not copy a full-function replay cost. Differentiate provable fraud, DA failure and verifier faults.
5. Repeat adversarial tests and independent soundness/economic review, then shadow the complete
   registration/Panel/Final path. No checker or Kernel extension is activated by this comparison.

###### Economic deterrence is not the cryptographic error bound

Preserve positive mandatory checks before Final. Optional PoSP-inspired extra audits can measure
cheating incentives and verifier shirking, but cannot replace those checks by “unchallenged = Final”.
If the **only** check is dispatched with probability `p`, then under an ideal honest checker with
conditional miss bound `ε`, a deliberately false claim's miss probability is
`(1-p) + p·ε`; a small dispatch probability cannot yield §15.4's whole-check target. This is distinct
from sampling a random vector on **every** claim, whose algebraic relation has its own conditional
soundness bound. Panel compromise and randomness/DA failures remain separately budgeted.

Publish the actual probability of detection **and successful enforceable conviction**, available
collateral, saved compute/proof cost, maximum external/reorg gain, checker participation costs,
fees/rewards, collusion/Sybil assumptions, and dispute backlog. As a screening requirement, expected
enforceable loss must exceed the bounded gain from fraud; that inequality alone is not a Nash-
equilibrium proof for MISAKA. Include RFC08's REAL-root exposure, pending EXEC work reward/collateral/DA/court exposure,
and RFC12's settlement risk. EXEC slice fork-choice weight is zero under revised RFC08; that does not make its other exposure zero. PoSP's paper “validators” map to computation checkers/Panel
seats here, never a new DNS attestation/precommit/finality authority.

#### 16. Model extensibility without a VM (2026-10-06 revision)

##### 16.1 Fixed policy, changed extension mechanism

Under [ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md),
the preferred large-model path is:

`pinned model → active kernel + declarative VerificationPlan → committed encoded constraints →`
`small probabilistic checks → positive receipts / DA / closed window → Final`.

Disagreement still invokes bounded exact court; routine full execution/segment replay is not added.
“Encoding” means a sound relation-preserving construction over all relevant computation, boundaries
and memory, not just erasure-coded trace storage. The miner/prover still performs the large work.
Whole-claim sparse-error soundness, post-commit randomness and the conditional proposed `2^-128`
target in §15 stay in force. Static bounds/identity checks remain deterministic.

The removed path is `unsupported model → BVM/GVM/Universal VM`. It is replaced by
`unsupported relation → precise missing kernel capability → reviewed versioned upgrade → retry`.
The full kernel descriptor, class binding and activation design is [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md).

##### 16.2 Registration outcomes, independently of hardware

| Finding | Outcome / next step |
| --- | --- |
| Whole task fits active kernel relations and all plan/admission bounds | `ELIGIBLE_AT(...)` after stateful admission; actual inclusion remains subject to §13.4 |
| Source/format frontend missing but target semantics exist | `FRONTEND_REQUIRED`; off-chain importer/lowerer and reference tests, no semantic node upgrade |
| Missing semantic primitive, verifier relation, memory rule or terminal court | `KERNEL_EXTENSION_REQUIRED` with family id, offending relation and required/available bounds; no successful registration under unknown semantics |
| Matching descriptor exists in code but is not activated | `KERNEL_NOT_ACTIVE` plus schedule/proposal status; no automatic activation by registration |
| Finite declared producer/seat requirements exceed current operators | Distinct readiness/capacity status; no arbitrary 8 GiB fleet cap on a semantically admissible class |
| Mandatory node/DA/court bounds exceeded, or incomplete constraint coverage | Refuse precisely; more producer CPU does not remove node DoS or missing-proof obligations |
| Missing rights/source material, uncontrolled external input or unbounded computation | Preserve the external/semantic blocker; neither a new kernel nor more CPU manufactures validity |

These are proposed structured outcomes, not already implemented RPC codes. Listing an extension
request is not class admission, and class admission is neither mineability nor market opening.

##### 16.3 Extension contract

1. A kernel contains reusable computational families, not per-model-brand allowlists. Plans may
   combine supported families without node updates, but must bind all task/context/quantization and
   input/output semantics. Frontend output and generated constraint circuits are untrusted input.
2. The plan grammar names implemented operators/checkers with finite dimensions, fixed bounded
   state templates and metered composition. It never supplies executable plugins, a guest ISA or
   a custom verifier program. GKR circuit support must be constrained to the approved relation
   constructors and checked against the intended model statement.
3. Every newly permitted family needs a deterministic reference, an independent implementation,
   full-statement error composition, authenticated evidence, bounded localization/terminal court,
   parser/resource limits and adversarial tests. No arbitrary success for unknown kernel/op ids.
4. New-format class/claim ids bind the immutable descriptor and plan. Old class ids and already-bound
   claims keep old rules. New semantics require a new version, not reinterpretation of old hashes.
   Snapshot/IBD and cross-kernel composition follow the same rule as live verification.
5. A SegWit/Taproot-class extension is the design analogy, **not automatic soft-fork compatibility**.
   Use a coordinated fingerprinted schedule unless a concrete valid-history/state/fork-choice
   compatibility proof supports something narrower. Unupgraded nodes cannot count unknown work as
   verified, reward it or continue post-fence full validation by skipping its witness.

##### 16.4 Recalculate coverage without fictional VM successes

The existing audit and historical 9B/2M evidence are unchanged by this design choice. No model has
been newly registered by editing these RFCs. Keep the exact pinned all-HF denominator and sampling
method in §§7–10. Assign **one primary current blocker per repository**, with secondary tags:

* `supported_active_kernel`: complete-task on-chain registration evidence under the measured release;
* `frontend_gap`: representable but importer/lowerer missing or untested;
* `kernel_extension_gap`: missing semantics/checker/court or not yet active;
* `resource_or_lifecycle_gap`: semantically covered but required admission/DA/resource/state gate fails;
* `external_gap`: inaccessible/missing/unauthorized source or other external dependency;
* `untested`: no qualifying end-to-end result.

Only the first bucket supplies successes to the existing estimator; the others remain failures.
Models formerly described as “GVM residual” are reclassified by actual kernel/adapter evidence, never
credited because a future extension could theoretically represent them. Thus **there is no new
numeric coverage percentage to report until new trials run**. The reference's 90–95% and 95–99%
figures are not empirical inputs, lower bounds or adopted guarantees. Do not change `D_all` to
“kernel-expressible models” to make the target easier.

Prioritize extensions by measured full-task repository gain, verification/prover/DA costs and audit
effort; publish both before and after results at each activated kernel version. The release gate
remains the all-HF ≥90% one-sided lower bound with source/task/registration evidence, not theoretical
expressiveness, a successful conformance fixture or a model-name list.

##### 16.5 Additional completion tests

Test old/new kernel coexistence, absent/not-active/forged ids, cheap but unsound custom checkers,
omitted relation families, cross-kernel replay, activation-boundary claims and rollback/pruning.
Require equivalent node/SDK verdicts without VM dependencies and real 9B-8k/validated long-context
registration plus Final/court/DA results. Report actual proof preparation and small-check costs.
Any reference replay used during development must be labelled conformance work rather than a
hidden prerequisite on every ordinary claim. No arbitrary soft-fork, universal-model support or
performance claim follows from choosing the kernel-only implementation strategy.

#### 17. Three-stage model onboarding with post-commit conformance (2026-10-08)

This is a proposed semantic lifecycle for the new route, not a change to current Rust enums, legacy registration,
active classes or network rules. [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)
alone defines the policy, sources, seed/sampling and interactive transcript rules. The [Kernel design](../design/palw/models/versioned-kernels.md) binds the policy to the
Kernel; RFC02 binds it into new class/plan identity; RFC13 records commitment/evidence and resumable tooling.

```text
Candidate -> StaticAdmitted / RegisteredDormant
          -> ChallengePending -> ConformancePending -> ConformancePassed
          -> G14 + ModelRewardEligible + resources + actual chain eligibility
          -> ActiveRewardable
```

`Registered != Active != Rewardable`. A static registration can finish without waiting for future work; a CLI can
return a durable candidate/commitment id and resume status later. Transaction presence or a local preflight alone
cannot claim chain registration, conformance PASS or reward eligibility. Missing beacon keeps the candidate dormant.

**2026-10-10後続改定 — identity / acquisition non-interference / model-bond allocation:**
[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を適用し、旧Torrent/Seeder/PoR/監査/lease/TRDC/FPR gateを撤回する。
登録済みModel ID/root、weights/tokenizer/config/specと受理済み任意descriptor bindingは不変とする。
改善版は別Model IDとして登録し、line/head/Position/AMMを付け替えない。
モデル取得・配布はoff-chainの任意運用であり、非公開/全peer停止だけで報酬・weightを止めない。

新`ModelRewardEligible`はimmutable identity、semantic/constraint/court planとactive rulesetを扱う（RFC14 §16.4）。
G14/計算検証は正しいmodelを得たverifierの条件付き能力とし、claim固有証拠の開示/保持/court scopeは維持する。
weight-file/range・全量列挙・反復要求をmodel取得の強制にしない。必要operandの認証と許可unitを確定する。
正しいbytesのroot一致はminerの実計算を証明せず、全保有者拒否時の取得保証は撤回する。

重複しないmodel別miner拘束元本に連動してcoinbase予算を配分し、network発行予算と個別bond上限を同時に維持する。
claim数/rho/複数modelで元本を増幅せず、資本預託だけでは支払わない。
公開の圧倒的利益は`f`・市場/競合・自己増資/独立資本・閉鎖時の検出率を比較する未立証目標である。
同額`S_m`なら自己/外部資本は同じ配分。未決の関数/会計・経済評価・実装/activationを旧HF censusやconformanceで代替しない。

##### 17.1 Stage A — Static Admission

Resolve supported active Kernel semantics, all typed relation/constraint coverage, exact-court/localization coverage,
authenticated public-material structure, deterministic class/plan identity and worst-case parse/verification/DA/court
resources. G14 structurally possible is required here; completed public-outside prosecution evidence is required
again at Stage C. Unknown op, uncovered rounding/routing/state relation or missing court returns the precise extension
gap. Beacon sampling cannot authorize it, and a Kernel proposal is not an active descriptor.

Fix artifact, program, tokenizer/input schema, layout, verification-plan, constraint and conformance commitments,
implementation-set root, scope, challenge policy and conditional error/resource profile. New-format class identity
is determined now, before actual beacon evidence, using the versioned RFC02 binding. The canonical accepted
commitment object and its position start the future window; a caller's wall-clock timestamp is insufficient.

##### 17.2 Stage B — Beacon Conformance

Apply `MODEL_CONFORMANCE` under the precommitted RFC07 policy. Only independently validated future work from
pre-existing eligible source classes qualifies; candidate X and work relying on X cannot test X. Collect/lock/replay
the source facts exactly as RFC07 specifies. Absence of qualifying sources is `BEACON_UNAVAILABLE`, not success,
and never enables heartbeat/BASE-0/EXEC/hash/committee fallback.

Run the approved independent authenticated checks: tensor/weight ranges, state positions, routing/TopK/boundaries,
pipeline stages, conformance vectors and Freivalds coefficients where the selected relation supports them. Include
all required static/adversarial deterministic tests. Pin reference/independent/optimized implementations before
randomness; retain exact results, openings, scope and transcript. No shared implementation disguised as an
independent oracle and no operator-picked post-beacon fixture list.

PASS is **probabilistic conformance for a declared scope and reviewed fault model**. It does not prove omitted
semantics or replace per-claim whole-statement verification. Derive/check the declared epsilon under the approved
suite rather than trusting manifest assertions. Raw s-of-N tile sampling misses a single bad tile with probability
`1-s/N`; Freivalds inside sampled tiles does not remove that selection term. A missing coverage/soundness argument
is UNKNOWN/ineligible for this route, not an unqualified conformance PASS. Publish measured artifact access,
transfer, verifier/prover work and resource use; a 100GB artifact becoming a few GB of checks is a feasibility target,
not demonstrated by this document. Opening authenticates bytes, not the computation relation by itself.

Changes to artifact/layout/plan/constraints/scope/implementation invalidate evidence and require a new commitment
before a new future window, subject to counted retries. If an accepted model's immutable identity is affected,
register a new independent model; never edit the registered record in place. A failed check initiates authenticated localization and the
applicable exact/DA outcome; it does not itself convict or slash. Preserve failed and aborted records.

##### 17.3 Stage C — Active Eligibility

ConformancePassed additionally needs complete semantic/constraint/court coverage, RFC14/G14 fresh-public-bond
prosecution conditional on acquired registered model data, claim-specific evidence/court/retention,
funded bond/liability and network/model/bond budgets, bounded resource profile, active
Kernel/challenge policy, challenge lifecycle and the actual chain carrier/admission path. Local conformance, a Panel
vote, register transaction or successful market opening cannot replace any of those conditions. Source fidelity,
market listing, operator readiness and active reward eligibility remain separately labelled.

```text
ActiveRewardable = SemanticComplete AND ConstraintComplete AND ChallengeComplete
                  AND CourtComplete AND PublicMaterialComplete AND G14Complete
                  AND ModelRewardEligible(model, epoch)
                  AND ResourceBounded AND ConformancePassed AND ActiveChainEligibility
```

Future randomness provides ChallengeComplete only when RFC07's source/security/implementation gates pass. It
does not supply the other predicates. A conformance pass does not activate a Kernel extension, remove Panel rules
or certify a future claim. Whole-claim Final, one-use work/reward and all present exposure caps still apply.
Reorg rolls back dependent challenge/conformance/activation state under RFC07's branch-relative lock policy.
`ActiveRewardable` is ruleset/verification eligibility, not a model-availability lease or a mutation of registered identity.
Require ADR177 §3 / RFC14 §16.8's non-interference/allocation matrix, §16.10's Rule E/fork-choice safety and §16.11's open-versus-closed ECON/MEAS
before activating the new gate. The registration/HF coverage numerator still measures accepted complete-task
registration; report active rewardable coverage separately and do not replace historical counts.

##### 17.4 Kernel extension and release evidence

Kernel authors first satisfy semantics, constraint coverage, court, public-material and resource completeness.
Then freeze implementation revisions and differential-vector scope, commit `KERNEL_CONFORMANCE` under RFC07,
and compare reference, independent implementation and optimized backend on canonical selected cases. Tests can
reduce fixture-selection bias; they cannot prove the statement includes every required relation or a Kernel sound.
Unknown semantics await ADR0172's coordinated extension review, shadow validation and explicit activation.

Test dormant asynchronous registration, missing/biased/nonqualifying sources, candidate self-source, changed roots,
policy/suite substitution, honest/invalid conformance, adaptive GKR messages, retries, no-private-state outsider
reconstruction/localization/conviction, reorg/restart/IBD/pruning and existing liveness under useful-work scarcity.
All new protocol gates remain open. Preserve existing 9B/2M/HF evidence and coverage denominators; these added
requirements do not convert untested models or dormant proposals into registered/active successes.

#### 18. Coverage target restated — 2026-10-08 (user decision)

The 2026-10-08 census (`docs/design/palw/tir/hf-census-report-2026-10-08.md`) measured 3,117,871 public repositories:

- shape-ready (full task, declared context, shape depth): **16.65%** of `D_all` (one-sided 95% lower bound 15.74%);
- software-closable failures: 34.84%;
- external failures under the current classification: 48.51% (missing weights 18.93%, gated 1.71%, adapter base missing 7.58%, no model task or config 20.28%).

So even with every software failure closed, `D_all` tops out at 51.49% under today's snapshot and buckets. The 90% bar of §0/§7/§10 over `D_all` is unreachable *by construction*. That says nothing about how much of the actual model population MISAKA can serve. It is a property of what the denominator counts.

**Primary target (replaces `registered_full_task / D_all ≥ 0.90` as the acceptance bar):** at least 90%, as a one-sided 95% lower bound, of the public Hugging Face repositories that form a **complete model** are registered on chain as their complete advertised task and advertised context, without model-specific consensus changes.

`D_complete` is the set of public repositories at a pinned snapshot whose weights and the information needed to build the model can be obtained. That means weights present or resolvable, a base resolvable for adapters, a task and config that are declared or deterministically derivable, and access permitted.

**Denominator rules (anti-gaming):**

- **Architectures and tasks MISAKA does not support stay in `D_complete`** and count as failures. This applies to any architecture or task (Qwen, Llama, GLM, DeepSeek, vision, audio, classification, …). Excluding a repository because MISAKA cannot handle it is forbidden; otherwise 90% could be reached by choosing easy models.
- **External failures are not permanent.** A repository moves into `D_complete` when its gap is resolved by a generic, auditable rule. Examples:
  - weights it references in another repository, resolved at a pinned revision;
  - a LoRA/adapter whose base is identified and pinned;
  - a task fixed safely from its weights and metadata;
  - a gated model whose access and redistribution rights are actually held.

  Each reclassification rule is published with the census, and its effect is reported separately.

**Always publish three numbers,** with their lower bounds, never only the flattering one:

1. coverage over **all public repositories** (`D_all`);
2. **full-task, full-advertised-context on-chain registration** over `D_complete`;
3. the share that reached **mining and a funded `Final` reward**.

**Evidence standard.** Frontend or shape-ready coverage never counts as registration. A sampled success is a real checkpoint → `.palwart` → G14 verification (public prosecution complete for its route) → devnet/chain registration. Alternatively, it is independently reproduced equivalence to an already registered class (§11.2). Text-only, short-context, synthetic-beacon, unit-test and dormant registrations are reported in their own columns. This section supersedes the acceptance bar of §0, §7, §10 and §14's "Broad coverage" row. `D_all` stays the first published number.

#### Mission alignment amendment — 2026-10-07

§13、§15、§16のVerificationPlan/kernel admission、9B-8k/long-context/全taskのcoverageに、fresh non-seat public-bond prosecutionの実測を追加する。kernel-only certificateや正直なFinalだけを成功と数えない。非公開weights/input/state、FOLD prefixやfused preimageが必要なprofileは、公開取得または承認済み公開証明経路が完結するまで新しい報酬対象にしない。HF coverage、ε_check、source fidelityとこのgateは別の指標である。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

全承認reward class/profileにclass非依存のbond/DAA採掘予算を追加し、巨大モデルや仕事量の申告で上限を増やさない。確率的な全constraint検査、G14、公的取得・裁定・資源gateとANDでadmitする。
一claimの権利/予約を小さくする評価でも、公開検証供給、最大同時責任、徴収可能net損失と計算費用を測る。多数の小claimを大きな未決済creditの迂回にしない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

#### 改定 — 2026-10-12（BeaconConformanceとbootstrap exact sourceの型分離：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §6。

§17の通常のBeaconConformanceと、限定したbootstrap exact source経路（RFC-0007 2026-10-12改定の `BootstrapExact`、仮称）を型で分離する。bootstrap経路でsource候補を得たことは乱数としての資格を意味せず、entropy資格は別に審査する。単体/統合試験用のsynthetic randomnessはproduction不可として型・設定を分離する。

---

<a id="source-v1-0013-L00001"></a>

### 旧RFC-0013 / 原文1–394行

出典: [0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md](../rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md)。配置理由: 主題別統合。

### RFC-0013: Reproducible layouts and resource-bounded model onboarding tools

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **適用時期の現行方針（2026-10-17 文書化）:** 本書が参照する画像・動画生成および RFC-0004 の改善に限り **mainnet 後へ延期・当面無効**。初期 mainnet の必須実装・提供・有効化に含めず、mainnet 開始による自動有効化もしない。共有のテキスト・固定登録・検証・会計は維持する。過去の Status・実装・試験・全面有効化計画はこの範囲の有効化承認ではない。[正式な範囲・無効化要求・再開条件](../governance/post-mainnet-scope.md)。今回 node コードは変更しておらず、runtime の無効化は未確認。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: Revised Draft, 2026-10-10. Existing tooling remediation and proposed post-commit conformance records plus §10 independent Torrent identity/availability/calculation evidence; **no runtime implementation, consensus activation or ceiling increase from this revision**. Earlier test results retain their original scope.
* Source baseline: `808baa6b9adcb029e64fffe51fcd84027f82e7c0`; the working-tree fixes accompanying this RFC are not yet a released binary or a main deployment.
* Validation record: [0013-onboarding-tool-validation.json](../evidence/rfc/0013-onboarding-tool-validation.json). Real-run stage status is explicit; a test result does not certify registration or source fidelity.
* Related: [RFC04](../rfc/0004-palw-model-improvement.md), [Kernel design](../design/palw/models/versioned-kernels.md), [RFC11](../rfc/0011-permissionless-model-and-long-context-onboarding.md), [RFC02](../rfc/0002-palw-tensor-ir.md), [ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md).

#### 0. 日本語での結論と範囲

今回のLlama登録作業は「CPU不足でチェーンが拒否した」という一つの問題ではない。
layoutによるcourtのサイズ超過、異なるDAAの審査、packの記録不足、独立検査のメモリ方式、
校正データの形式不一致、未完了の登録・認定を分ける必要がある。
**最終の512-context候補はlive preflightで受理可能と返ったが、登録transactionはまだ送信していない。**

RFC11にあるclose-sizing・長context・資源認定の方針は維持する。
[versioned Kernel 設計](../design/palw/models/versioned-kernels.md)の拡張方針、RFC04の評価・昇格方針も変更しない。
追加するのは、**同じartifact・layout・rulesetを各ツールで受け渡し、巨大artifactでも検査を省略せず、
失敗した段階から再開できる具体的なtoolchain契約**である。
これを実装しても全モデル・任意contextの無条件登録を保証しない。必要なKernelや資源・権限・証拠が
欠ける場合は、それを正確に報告する。制限の迂回、context短縮を「元モデル対応」と呼ぶこと、
`--skip-pack-verify`による成功扱いは解決策にしない。

#### 1. Evidence and its limits

##### 1.1 Pinned inputs

The Llama attempt used `mradermacher/Llama-3.2-3B-Instruct-uncensored-GGUF`, revision
`186911b5658f85bfe68684b83646ba2e89169a0c`, `Llama-3.2-3B-Instruct-uncensored.Q4_K_M.gguf`.
Source bytes: `2241004096`; SHA-256:
`1a01383341be9bf04a904e168089401c3f0533b3af4089d3536d8b36e497ccd6`.
Conversion produced a 28-layer Llama TIR program and an approximately 4.73-GiB integer artifact.
Quantized source size and expanded integer artifact/working-set size are different measurements.

The following live results are retained observations from the preceding onboarding run, **not new
benchmarks executed by writing this RFC**. Every refused live candidate was rejected by preflight,
not a submitted on-chain registration. Table labels describe the requested tile sizes; layout
search can try narrower candidates. Use the actual recorded layout, not its filename, for identity.

##### 1.2 Attempt-by-attempt diagnosis

| Attempt | Evidence / result | Actual reason and disposition |
| --- | --- | --- |
| Header-only GGUF architecture/preflight path | `ARCH_REFUSED`: header-only view holds no tensor data | The diagnostic requested data the header view cannot provide. Full GGUF conversion subsequently succeeded. This is not evidence that Llama semantics are unsupported. Route shape-only queries through metadata, or explicitly request the weight-backed path; do not suggest a new Kernel from this message alone. |
| Fidelity, 32 positions, reference evaluator | Interrupted after a long CPU run; artifact/statistics had been written | Local evaluation cost and an interrupted operation, not chain rejection. Typed execution with one reference cross-check subsequently completed. Preserve outputs and distinguish interrupted from failed/verified. |
| Fidelity `--stats-out` reused by pack | Legacy decimal-float warning | Fidelity wrote a plain serde float map while runtime packs require `misaka.palw.calib-stats.v1` bit-exact statistics. Re-serializing rounded decimals cannot recover original bits; regenerate exact statistics or retain the original bit-exact inputs. |
| ctx512, ordinary tile64, history64, logits4096 | Live DAA5756: root claim `100196 > 100000` bytes | Dissection root-carrier limit, not inference FLOPs. Narrowing only the logits tile does not necessarily shrink the history/root claim. |
| ctx512, ordinary64, history64, logits2048 | Live DAA5756: same root claim `100196 > 100000` | Wrong dimension was adjusted for this first blocker. |
| ctx512, ordinary32, history32, logits4096 | Live DAA5756: terminal close `16447959 > 3200000` bytes | Root-claim obstacle was passed, but the large logit head opening hit the next carrier limit. Smaller history alone is insufficient. |
| ctx511, default requested tiles | Offline sizing refusal; no retained live result | Reducing context by one did not remove the observed offline sizing-budget refusal. No claim about its live eligibility is justified. |
| ctx512, ordinary64, history128, logits4096 | Live DAA5758: root claim `149348 > 100000` | Enlarging the history tile increased the root-carrier requirement. |
| ctx512, ordinary128, history32, logits4096 | Live DAA5759: terminal close `16445703 > 3200000` | Enlarging ordinary commit tiles did not fix the oversized logit opening. |
| ctx512, ordinary64, history32, logits512 | Live DAA5760: `ADMISSION_OK` | A legal combination exists for this specific artifact/profile. This is not a registered class or proof of full advertised context support. |
| ctx128, ordinary64, history32, logits512 | Live DAA5761: `ADMISSION_OK` | Separate smaller-context profile; do not count this as success for the requested larger envelope. |
| Fresh bit-exact pack, ctx512, default 4×(32+4) conformance | Interrupted during expensive conformance | The convenience command serialized conversion and checks; interruption does not mean a false model. Resume/check stage outputs rather than converting again. |
| Fresh pack with two vectors (prefill≤4, decode2; 8 actual positions) | Build completed; independent implementation omitted above 512MiB | `ref2` required a whole expanded parameter map. Build completion is not full verification; verifier marks conformance SKIPPED when requested ref2 did not run. |
| Fresh exact-statistics artifact, ctx512, ordinary64/history32/logits512 | Live DAA5767: `ADMISSION_OK`; class `b4a2ba78…67c97b` | This is a different calibrated artifact/class from the earlier pass. Its pack lacked a declared record tying the base artifact to this wrapper. The next obstacle is tool provenance/conformance, not the earlier live court refusal. |
| First invocation of the new `pack bind-class --network testnet-12` command | Printed usage before binding | The outer CLI removed `--network` before dispatching the pack subcommand. Dispatch now preserves pack arguments; the CLI regression test and the actual large-pack binding pass. This was a tool argument-routing failure, not a class or chain rejection. |
| Registration, certification, line/market and mining | Not run to completion; no registration signed/broadcast in this attempt | Active bond/signature, stateful acceptance, certification, availability and sufficient qualified seats are separate untested conditions. No Panel was started. No chain refusal or mining success may be invented for these stages. |

##### 1.3 The offline sizing result is not one unit over

[`palw_tir_admission_v1.rs`](../../consensus/core/src/palw_tir_admission_v1.rs),
`palw_tir_carried_closes_admit_form_v1`, translates `PALW_TIR_CLOSE_SIZING_OVER_CAP_V1` into
`value = work_cap.saturating_add(1)`. Thus `67108865 / 67108864` is a **censored lower-bound
indicator**, not a measured exact workload. It cannot justify increasing the cap by one or claim
that unlimited CPU would make the same protocol comparison pass.

There is also a specific ruleset mismatch:
[`TirOfflineGateV1::of`](../../misaka/palw/sdk/src/tir_layout.rs) selects the first TIR activation,
DAA2000 on this checkout. [`config/params.rs`](../../consensus/core/src/config/params.rs) arms
`palw_tir_fence2` at DAA3600. This selects the later range/demand rules used by admission; a live
DAA5767 query is therefore **not the same experiment** as default offline declaration at DAA2000.
This source fact explains why comparing those labels alone is invalid. Full parity still requires
the exact deployed build, network identity, schedule, class and block-state snapshot; no assertion
is made that every local/live difference is solely this height choice.

##### 1.4 What the old pack could not reproduce

[`DeclaredClass`](../../misaka/palw/sdk/src/runtime_pack/manifest.rs) recorded context,
checkpoint and `h_tile`, but not `commit_tiles`, `state_tiles` or the effective logits scheme.
[`declared_check`](../../misaka/palw/sdk/src/runtime_pack/verify.rs) ran a new default-tile search,
with automatic logits selection, rather than replaying the exact layout. A legal explicit
logits512 layout cannot be inferred from that incomplete record. This is a code-proven gap;
not every individual variant above was run through `pack verify`.

A base artifact and a declared wrapper have different file digests (and possibly program graph
roots due to the logits scheme), even when their inventory/tokenizer are identical. A pack must
pin both, not waive digest checking. The CLI [`pack_gate.rs`](../../misaka/apps/cli/src/pack_gate.rs)
requires **artifact and conformance PASS** before constructing a registration; conformance SKIPPED
is a pre-submission CLI blocker. Consensus does not currently require this runtime-pack policy.
Other optional SKIPPED checks are reported separately, never called full source fidelity.

#### 2. Coverage by existing RFCs — no duplicate protocol redesign

| Problem | Existing coverage | RFC13 addition |
| --- | --- | --- |
| Court tile/MAC, root and terminal close bytes, sizing work | RFC11 §§1,4A,12–13: bounded sizing, legal layouts, compositional Kernel plans | Operational evidence and exact layout handoff; keep protocol caps unchanged. |
| 9B recurrent calibration/context metadata and long calibration | RFC11 §§1–4,11–13 | Shared exact-statistics format and stage identity, not a new calibration safety waiver. |
| 2M context cap, IR held rejection, inline canonical prompt cap and DA reach | RFC11 §§2,4B,12–13 | None to consensus. The Llama run did not test 2M; enough CPU does not remove these format gates. |
| Missing operators/formats, model extension | [Kernel design §§K.1–K.7](../design/palw/models/versioned-kernels.md) and RFC11 §16; frontend for existing semantics, reviewed Kernel update otherwise | Header-only diagnostics cannot misclassify a supported weight-backed conversion as missing semantics. |
| Candidate/adapters, evaluation and promotion | RFC04, including unchanged class identity/composite bindings | Same exact pack contract for standalone classes; composite binding must name its parent before support is claimed. |
| Small probabilistic normal checks; exact dispute court | RFC11 §15, RFC07 Part V, ADR0171/0172 | Unchanged. Offline executor conformance is not normal per-claim full replay. |
| Pack exact layout, historical versus live gate, independent large-artifact checks | RFC02 runtime-pack requirements and RFC11 general parity/split-stage obligations, but no complete concrete contract for these failures | §§3–7 define it; these tool-only blockers cannot be solved merely by activating a probabilistic Kernel. |
| Bond/exposure, inclusion, reorg, qualified seats, mineability | RFC11 §§3,11–14 | Exact completion labels and evidence; no assertion that an unexecuted stage passed. |

RFC13 is a companion toolchain specification, not a replacement for RFC04/05/11. No Universal VM,
BVM/GVM, TEE trust or BFT operator authority is introduced. Versioned Kernel extensions remain
coordinated upgrades, not automatically soft forks.

#### 3. Exact layout is data, not a search result to guess later

New packs MUST pin the actual layout version, context, checkpoint, history tile, **every** commit
tile and state tile, effective logits scheme, layout digest, class ID and declared file digest.
The class still uses the existing canonical Borsh/hash domains; JSON is a tool-side record only.
Search options may be recorded for provenance but cannot replace these effective values.

Verification recomputes layout digest and class ID from the pinned program/tokenizer/inventory
and recorded layout. When supplied the declared file it additionally checks the file's actual
class/layout binding. It MUST NOT run a fresh optimizer, reset tiles to defaults, substitute a
current fence, rewrite a multi-GiB artifact just to inspect its layout, or reinterpret an old class.
Identity verification and resource admission are separately labelled results.

Old manifests without the full record remain readable, but exact-layout reconstruction from a
base artifact is SKIPPED with remediation, not silently passed. Attaching an existing declared
artifact creates a **new pack/digest**, preserving the old pack and artifact. Reject unrelated
inventory, tokenizer or computation graphs, malformed layouts and bad sidecars before publishing
the new pack. Composite artifacts need their parent/section-root bindings; unsupported composite
attachment is explicitly refused, not hashed as a standalone class.

New optional fields require upgraded pack readers: old `deny_unknown_fields` readers may reject
them. This is a tool-manifest interoperability change, not a consensus fingerprint/class rehash;
do not claim that every old binary can read a newly enriched pack.

#### 4. Target-ruleset admission and useful diagnostics

Provide distinct modes: historical `--at-daa N`, explicit pinned schedule offline, and live block
snapshot. Default historical output must prominently state its height and cannot conclude that a
model is impossible on today's chain. A future target is conditional, never currently accepted.
Do not fix this by silently choosing the largest scheduled future DAA.

Local and remote checks MUST report chain genesis/identity, binary/tool revision, schedule/fingerprint,
snapshot block/DAA, active fences, exact class/program/layout/artifact/tokenizer identities and scope
(static, stateful, certification). Run the common consensus gate under those same inputs. Refuse
unavailable/mismatching snapshots or report them as UNKNOWN; a cached eligibility result expires
when relevant state changes. For proof-equivalent sizing optimizations, preserve acceptance and all
cap outcomes through differential tests; actual acceptance changes require their own fenced release.

Each refusal needs stage, gate/rule, measured value and unit, cap, whether exact or censored,
commit point/read category when available, selected sizing twin, attempted effective layout,
and a concrete remediation. Search preserves the first and subsequent failures, including reasons
that prevented exploration; it cannot report a remembered widest refusal as an exact measurement of
every narrower candidate. It must bound its own attempts/time/allocations, not run unbounded searches.

#### 5. Large-artifact independent conformance

Replace the whole-model `ref2` parameter map with a lazy byte source. Ref2 must independently decode
canonical program/tensor bytes and retain its own primitive arithmetic/state/commit calculation.
Sharing authenticated container I/O does not authorize copying the first evaluator's outputs or
using typed-backend results as the independent oracle. Compare logits and **all recorded commit
points at every tested position**, including prefill/history/decode and recurrent state.

There MUST be no unconditional 512MiB artifact cutoff or PASS-on-skip for a required implementation.
Report which implementations actually ran and why a required one failed/interrupted. Ref2 failures
and corrupt/missing tensors propagate as failures, not reference-only fallback. Streamed and loaded
checks reproduce the same vectors/digests; negative tests must show that forged commits fail.

Lazy whole-tensor decoding is a first tool-only step, **not a universal memory solution**:
the largest expanded tensor, simultaneous operands, activations, commit snapshots, history and
mapping residency still consume memory. The report's peak-parameter metric is not process peak RSS.
For models with one tensor larger than worker capacity, implement authenticated row/range readers
and independently specified tiled primitives preserving exact accumulation, rounding, overflow and
index order. A tensor can exceed the model's aggregate quantized source size after `i128` expansion.
Do not increase a cutoff and claim bounded memory or promise that ordinary hardware handles any model.

Workers may be provisioned remotely; their reports are content-addressed, bound to semantic/tool
versions and independently reproducible. A worker signature or cache is not correctness authority.
Finite conformance vectors exercise implementations; they are not a proof of whole-model fidelity,
not a new probabilistic Final rule, and not a reason to reduce normal-path constraint coverage.

#### 6. Exact calibration interchange and resumable stages

Fidelity and pack conversion MUST emit/read the same bit-exact calibration schema. Keep legacy
decimal input explicitly labelled for diagnostics, but never promote it to exact historical input.
Bind corpus/token IDs/length, calibration context/rule, source/frontend/quant policy, math mode and
tool versions as required by RFC11; synthetic short tokens are not full-context semantic evaluation.
The fresh Llama pack's statistics differ from the earlier fidelity run. Earlier accuracy metrics
cannot be copied onto a recalibrated artifact; a missing upstream-reference comparison stays missing.

Expose independently restartable stages:

`resolve → calibrate → convert → declare → target preflight → bind pack → conformance → certify → submit → observe`.

That pipeline records the earlier local-tool stages. The proposed beacon route adds the independently resumable
stages of §9; local deterministic conformance is neither Beacon Conformance nor Active eligibility.

Persist each stage's input/output hashes and status using atomic completion records. No output is
complete merely because its path exists. Resume only when all pinned inputs still match; changed
tile/scheme/DAA invalidates affected checks without recalibrating unchanged weights. Checksums, roots,
conformance vectors and admission cannot be borrowed across changed inputs. An I/O failure after
writing sidecars leaves an incomplete output, never a valid completion record. Support cancellation,
disk/memory/work budgets and progress per vector/position; do not duplicate multi-GiB artifacts per
small metadata experiment when a verified reference/layout view suffices.

#### 7. Release tests and observable completion

1. Nondefault ordinary/history/logits/checkpoint layouts round-trip from base and declared artifacts;
   mutate every field, scheme, digest, tokenizer and weight binding and require failure. Old packs
   parse, report missing exact layout, and migrate without overwriting old files.
2. Loaded versus lazy ref2 produces identical bytes/errors over golden and generated programs;
   missing/corrupt sources fail. Exercise an artifact above 512MiB and then the real Llama artifact
   with requested independent implementation: no size-based SKIPPED. Report tested vectors, resource
   use and duration. Tiny fixtures alone do not establish real-size completion.
3. Match historical DAA2000 and post-fence3600/current snapshot SDK/node gate inputs; report expected
   differences, then prove parity under identical inputs. Preserve the cap+1 lower-bound label and
   tested cap−1/cap/cap+1 behavior. Do not relax court limits to satisfy a benchmark.
4. Exact calibration emit/read/materialization reproduces bytes; reject changed sidecars and forged
   context coverage. Interrupt after each stage and resume to the same roots; changed inputs force
   rechecking. Test memory, disk exhaustion and invalid output/cached-success handling.
5. Observe registration by **accepted class state**, not a preflight pass, tx hash, elapsed DAA or
   model-market entry. Record bond authorization/exposure, processor verdict, block/class/root,
   RPC listing and reorg behavior. Certify/mineable/first Final/market-open are independent outcomes.

For this operator task the Panel-start step is expressly out of scope. Missing qualified seats may
leave a safely registered class non-mineable. Do not fabricate ready status or claim that merely
creating a model position completes mining qualification. RFC11's all-HF/full-task/context coverage
denominator and evidence requirements remain unchanged.

#### 8. Implementation boundary of the accompanying patch

The real `5079273984`-byte Llama artifact completed streamed conformance: **2 vectors, 8 actual
positions**, identical logits and all commits on reference, typed-backend and independent ref2.
`/usr/bin/time -l` reported 1228.90 seconds and maximum RSS 12828917760 bytes (about 11.95GiB)
on the 24GiB Mac, with zero swaps. The seeded `prefill=4` option is a maximum prompt length,
not a claim that every vector has four prompt tokens. This evidence covers only these actual
positions, not full 512-context fidelity or all models. Manifest, sidecars, source, frontend,
artifact and exact declared identity also passed. **HF reference is SKIPPED** for this newly
calibrated pack; the report is `ok=true`, `verified=false`, not full source-fidelity certification.
The real run predates the progress/log-message-only additions; its binary hash is pinned in
the evidence record. Current logging is tested separately on the small regression fixture.

The accompanying tool changes add full layout records, direct identity verification, a non-overwriting
`pack bind-class` path, lazy independent parameter decoding, per-position streamed verification
progress on stderr, and exact fidelity statistics output. Progress is not a resumable completion record.
They do not implement all of §§4–7: explicit target-snapshot/offline parity, row/range ref2,
durable per-position resume, complete calibration provenance, registration/certification and broad
coverage remain distinct acceptance work. Test/real-run results belong in the evidence report;
neither this text nor a passing build proves those unfinished outcomes.

#### 9. Beacon-aware packs and asynchronous conformance evidence (2026-10-08)

[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the sole
source/seed/sampling/transcript protocol; [RFC11 §17](../rfc/0011-permissionless-model-and-long-context-onboarding.md#17-three-stage-model-onboarding-with-post-commit-conformance-2026-10-08)
owns Static Admission → Beacon Conformance → Active Eligibility. RFC13 persists reproducible evidence and status,
not an independent beacon or admission authority. These records are proposed tool-manifest schemas; wire ids,
consensus carriage and compatible pack versioning require their own reviewed release.

```text
ConformanceCommitmentV1 {
    version, chain_genesis, ruleset_id,
    subject_kind, class_candidate_id_or_kernel_candidate_id,
    kernel_descriptor_id, challenge_policy_id,
    artifact_root, program_root, tokenizer_or_input_schema_root,
    layout_root, verification_plan_root, constraint_root,
    implementation_set_root, test_scope_root, conformance_commitment,
    input_and_state_binding_root, resource_profile_id,
    commitment_object_id, canonical_commitment_position
}

BeaconConformanceEvidenceV1 {
    version, commitment_root, challenge_policy_id,
    challenge_anchor, qualifying_source_evidence_root, lock_evidence_root,
    challenge_seed, selected_vectors_root, selected_tensor_ranges_root,
    reference_result_root, independent_result_root, backend_result_root,
    authenticated_openings_root, per_round_transcript_root, transcript_root,
    checks_required, checks_run, checks_failed, missing_checks,
    scope_and_fault_model_id, derived_epsilon, soundness_assumptions_root,
    status, measured_work_bytes_ram_time, public_material_locator_root
}
```

Canonical identity/commitment bytes come from the appropriate versioned RFC02/05/07 binding, never ordinary JSON
serialization. JSON is a tool record. Cached seed/epsilon/status fields are claims to recompute, not trusted inputs.
`commitment_object_id` and `canonical_commitment_position` are observed submission provenance outside the precommit
statement digest; do not hash an object id into itself. The conformance commitment binds the fixed test statement.
Reference code, independent implementation and optimized backend revisions/math/layout policies are committed
before randomness; the implementation set cannot be changed to agree with a revealed challenge. For Kernel testing,
bind the proposed descriptor/relation/court scope without treating that descriptor as an already active model Kernel.

The evidence locators resolve immutable public authenticated source, Final/settlement provenance, statement,
query openings, output and complete GKR transcript bytes. Root-only records do not show checks ran. Missing, skipped,
scope-mismatched or stale data cannot be a PASS. Record conditional probabilistic conformance separately from exact
reference agreement, semantic admission, full-task source fidelity, G14 and actual chain Active/Rewardable state.
Derive/check epsilon for the committed scope and fault model; a guessed epsilon or successful finite test list is
not a whole-Kernel/whole-model security theorem.

##### 9.1 Resume, retries and changed inputs

```text
resolve / calibrate / convert / bind exact pack
-> StaticAdmitted / RegisteredDormant
-> submit and observe canonical ConformanceCommitment
-> WaitingRandomness / BEACON_UNAVAILABLE
-> ChallengeResolved (RFC07 lock predicate)
-> ConformanceChecked / ConformancePassed
-> G14 + availability + resource + chain eligibility
-> ActiveRewardable
```

Persist hashes, commitment/source references, policy, per-stage progress, retry/abort history and atomic completion
records. A CLI may exit while WaitingRandomness and resume from the canonical object; it need not keep the process
or giant artifact fully resident until enough work arrives. Reconstruct from public chain facts, not the old process's
memory. Local timeout is pending/cancelled, never entropy substitution or activation.

Artifact/program/tokenizer/layout/plan/constraints/scope/implementation/policy changes invalidate every dependent
challenge/check result and require a new pre-beacon commitment. Retain the old failed/aborted evidence rather than
overwriting it; RFC07 counts retries and distinguishes off-chain grinding assumptions. Unchanged upstream stages
may be reused only after verifying their pinned hashes. Reorg invalidates branch-dependent source/lock/conformance
and eligibility evidence; do not rerun an optimizer or retain a stale seed under an apparently unchanged class id.
The actual beacon/queries/transcript are activation evidence, not new class-identity inputs.

##### 9.2 Independent bounded checks and release cases

Retain mandatory exact/adversarial conformance and the independent implementation requirements of §5. A reviewed
beacon profile may add bounded authenticated randomized checks over a huge artifact; it cannot silently turn a
previously required full-scope test into an unchecked sample or hide selection error. Stream/mmap verified ranges,
charge hash/opening/preprocessing and transfer work, and measure actual checked bytes/RAM/prover/verifier costs.
No 100GB-to-MB/GB reduction or universal speedup is claimed without an equivalent-security real-artifact run.

Require policy/suite/seed/source/lock replay, reference/independent/backend agreement on selected cases, forged
openings, changed implementations/layouts, omitted vectors, raw-sampling scope labels, adaptive GKR transcripts,
candidate self-source, scarcity, cancellation/resume, bounded retries and reorg/IBD/pruning tests. A fresh public
verifier must rebuild the same challenge and follow a mismatch through exact court/DA rules without producer-private
state. Old manifests stay readable with explicit unsupported/missing-evidence labels; upgrading pack metadata
cannot silently activate a new challenge policy or rehash a legacy class. All §9 implementation evidence is pending.

#### 10. Immutable identity and model-bond allocation evidence — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
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

toolは配分snapshot、`C_b_effective_locked`/`C_{b,m}`/`S_m`、`f`/model予算、network/個別bond予約、
identity照合と実検査/経済評価の条件を別記録する。取得結果はoff-chain観測であり報酬資格の証明ではない。
以前の9B/2M/conformance/HF censusの測定scopeを変更せず、新gateのPASSへ流用しない。

#### Mission alignment amendment — 2026-10-07

historical/live gate分離、exact layout、独立streamed conformanceは維持する。registration/PASS/Finalのrelease reportに、その実際のprofileで外部public bondが証拠を公開取得しlocalizeしてconvictできるかを別項目で報告する。preloaded producer captureや既知fault indexを渡すfixtureは独立prosecutionの成功証拠にしない。toolのメモリ削減で裁定の最大bytes/workを隠さない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

onboarding/資源測定に、同額bond・同期間・同倍率の正直な計算対高速偽造と容量拡大時の総block/reward/Final credit保存を追加する。claim数の増加を検証供給・徴収可能資本の増加と数えない。
receipt/rider/期間境界・丸め・reorg・担保再登録まで測定し、小fixtureやschema検査のPASSを実node/経済保証の完成に流用しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
