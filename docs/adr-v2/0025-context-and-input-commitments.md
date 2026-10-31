# ADR-v2-0025 — Context／入力state・commitment・取得単位

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0001](../rfc-v2/0001-inference-and-generative-jobs.md) / [RFC-v2-0008](../rfc-v2/0008-claim-dependencies-and-liability.md)

## 1. この文書の役割

入力・出力material、derived artifact、context stateのcommitmentと搬送範囲をまとめる。

## 2. 統合した判断の読み方

### 2.1

derived artifactは元のclaim出力・grammar・変換との関係をcommitし、artifact本体を常にchainへ運ぶ設計とは区別する。[旧ADR-0078](../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md)

### 2.2

answer envelopeではtoken IDsとjob contextをoutput rootへ結び付ける。IDsを受け取った事実だけで計算全体を再検証済みとはしない。[旧ADR-0084](../adr/0084-the-ids-ride-the-capture-stays-home.md)

### 2.3

held contextではroot・openingと局所証拠を使い、全履歴captureの搬送と区別する。checkpoint、step、artifactのどのrootを開くかを保持する。[旧ADR-0103](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)

## 3. 維持する境界・後続改定

旧「captureはローカル」の説明を、外部者へ必要なclaim証拠を出さなくてよいという免除にしない。後続の公開訴追とモデル非強制取得の境界を併せて読む。

## 4. 隣接する判断との分担

prefix生成元の責任は[ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md)、局所裁定は[ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md)、DA要求は[ADR-v2-0039](0039-data-availability-court-and-panel-da.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0078-what-was-made-from-it-is-committed-the-thing-never-rides.md](../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) | L1–L470 | L15–L28 |
| [0084-the-ids-ride-the-capture-stays-home.md](../adr/0084-the-ids-ride-the-capture-stays-home.md) | L1–L470 | L15–L31 |
| [0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) | L1–L807 | L15–L55 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0078-what-was-made-from-it-is-committed-the-thing-never-rides"></a>

<details>
<summary>0078-what-was-made-from-it-is-committed-the-thing-never-rides.md — 原文 L1–L470</summary>

[原文を開く](../adr/0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) · SHA-256: `36a5f3d6cf7f00be96cd0f38403e72f2c19b9bbead670ad638640e7ef25f4278`

<!-- ADR_SOURCE_BEGIN:0078-what-was-made-from-it-is-committed-the-thing-never-rides.md -->
~~~~~~~~markdown
# ADR-0078: What was made from it is committed; the thing itself never rides

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED (2026-09-02). Written to draw a line ADR-0077 needed drawn: implementing
ADR-0077 makes an inference at a usable width a certified, spendable claim. It does NOT make
"generate a 3D model with Qwen3.6, verified end to end" true. The mesh is not the inference. This
ADR is the layer above the receipt — the model's output turned into a thing a person keeps — and
its rule for the chain is the title. Nothing here bears weight, moves a preset, or changes how a
claim is certified; the consensus surface is one compact object per derivation, and it is
introduced in a ruleset move of its own.
**Builds on:** ADR-0077 (R0: one inference is one answer and one claim; R1: the artifact to the
user, the receipt to the chain), ADR-0044 (the receipt lane; `output_root` is the commitment to
the output ids, never the tokens), ADR-0074 Decision 1 (the `User` prompt mode), ADR-0075
(lifecycle objects; `ObjectChunk` for anything above one carrier), ADR-0053 (one execution family:
pure Rust in the tree, integer arithmetic, because float is unadjudicable), ADR-0026 (borrow the
architecture, refuse the tolerant proof model).
**Withdrawn:** independent PALW weight for a transformer's computation (§3 Decision 7). Derivation objects remain weightless under ADR-0144 P3.

> **Security amendment appended (2026-09-02)** — see the last section (SA-1…SA-6): model-written code runs on an ephemeral EVM state under a gas ceiling in a confined process; every transformer declares input/output bounds; uploaded inputs and the DSL DA election are bounded and authenticated; a `kind` with no published manifest is not storable; task graphs are never executed here.

> **ADR-0144 alignment (2026-09-21).** Decision 7's door — a derivation transformer registers as a class and earns PALW leaf weight — is withdrawn unless that transformer **is** the user's local inference (P3). The derivation object stays weightless. See the end section.

## 1. The line, and why it has to be drawn

ADR-0077's scope is exactly this chain of objects:

```text
Qwen3.6 (or A16, or any certified class)
  → an inference at the class's width (a prompt a person typed, an answer they keep)
  → checkpoint-anchored capture, one interval opened to each seat
  → claim → certified → receipt block
```

A person who asks for "ten NPCs for this game" gets JSON back, and that JSON's generation is what
the chain certified. Everything after it — the renderer that makes meshes of the JSON, the GLB the
game loads, a PNG, a STEP file, a compiled program, a MIDI file, a simulation's trace — is a
computation the model did not perform and the court cannot try. Two wrong answers are available
here and this ADR refuses both:

* **Carry the thing.** A GLB is megabytes; a block carries ~125 KB of transaction bytes and a
  lifecycle object at most eight 100,000-byte chunks (ADR-0075 Decision 14). The chain is not a
  file store, and a design that made it one would price every artifact against the mempool.
* **Pretend the inference certifies the thing.** A receipt says the model produced these output
  ids under this class. It says nothing about what a renderer did with them. Reading it as a
  certificate of the mesh is the same category error as reading a block hash as a certificate of
  the transaction's meaning.

The right answer is the one ADR-0077's R1 already states for the receipt and this ADR extends one
layer up: **commit what the model made, and what was made from it — keep neither.** The chain
holds a derivation: which claim, which transformer, which bytes in, which bytes out, by hash. The
bytes stay with the person who asked for them.

## 2. The shape

```text
     the model's output ids          (committed by the claim: output_root)
              │
              ▼  render + canonicalize  (a registered grammar: grammar_id)
     canonical DSL bytes               (dsl_hash)
              │
              ▼  a deterministic transformer  (content-named: transformer_id)
     the artifact                      (artifact_hash, artifact_bytes)
              │
      ┌───────┴────────┐
      ▼                ▼
 to the user       DerivedArtifactV1 { claim_id, grammar_id, transformer_id,
 (GLB, PNG, STEP,                     dsl_hash, artifact_hash, artifact_bytes, kind }
  code, map, MIDI,                    + the executor's signature → the chain
  a trace)
```

One inference, one claim (ADR-0077 R0). Zero or more derivations of that claim, each a compact
object. The DSL is not a second language the model must learn: it is the model's answer, in a form
a grammar can canonicalize — JSON for a scene, source text for a program, a note list for music.
What makes the layer sound is not the DSL; it is that the transformer is a **pure function** the
way a PALW class is: integer or exact arithmetic, no clock, no randomness, no network, one build
named by its content, byte-identical on two architectures before anything may name it.

## 3. Decisions

**Decision 1 — a derived artifact is a derivation, committed; the artifact never rides.** The
chain accepts `DerivedArtifactV1` and stores it beside the claim it names. It never accepts the DSL
bytes or the artifact bytes as consensus carriage, in any chunking, under any size. A future ADR
that wants the chain to hold a thing has to argue against this sentence.

**Decision 2 — the DSL is the claim's output, canonicalized by a registered grammar.** The
transformer's input is the rendering of the ids the claim committed (`output_root` is
`output_commitment_v2(job_context_hash, ids, rendered_hash)` — the ids, the job's context hash,
and the family's rendered-output hash, which on every shipped family is itself a keyed hash of
the ids; so a consumer holding the answer's ids, the job's context hash (a public value the
gateway returns beside the answer) and the family's name recomputes it — implementation note,
2026-09-02: the ADR's first draft read "over exactly those ids", and the code says three inputs),
passed through a grammar's canonicalizer — a pure function (whitespace, key order, number form,
nothing semantic) named by `grammar_id`. `dsl_hash = H(grammar_id ‖ canonical bytes)`. A parse
failure yields no derived object and nothing else: the inference still certifies and still mines,
because ADR-0077 R1 credits the computation, not what it happened to be good for. Nothing about
the prompt or the answer is changed to make it parse — F1 (ADR-0044) reaches this layer intact.

**Decision 3 — transformers are content-named pure functions, held to the family's discipline.**
A transformer is described by a manifest — `kind`, `grammar_id`, the build's source-tree hash, the
arithmetic discipline it declares (integer or exact rational; no `f32`/`f64` on any path that
reaches the output), and its output format's canonical writer — and `transformer_id = H(manifest)`.
It runs in the tree, pure Rust, no external runtime (the ADR-0053 rule, for the reason ADR-0053
gives: a float path is a path two honest hosts disagree on). Before a kind's transformer may be
named by any object, the drill runs it on the same DSL on two architectures and requires
byte-identical artifacts (the cross-device discipline the free-prompt lane already applies to
claims). A transformer that cannot pass that is not a transformer under this ADR; it is an
application, and applications are welcome to exist without a commitment.

**Decision 4 — the object, and what the chain checks.**

```rust
pub struct DerivedArtifactV1 {
    pub version: u16,
    pub network_domain: Hash64,
    pub claim_id: Hash64,          // the free-prompt claim whose output this derives from
    pub output_root: Hash64,       // MUST equal the claim's committed output_root (cross-check, not a second source)
    pub grammar_id: Hash64,
    pub transformer_id: Hash64,
    pub kind: u16,                 // the kind table, Decision 8
    pub dsl_hash: Hash64,
    pub artifact_hash: Hash64,
    pub artifact_bytes: u64,
    pub executor_pubkey: Vec<u8>,  // MUST equal the claim's executor bond key
}
// + signature; derived_id_v1 = H(canonical(object)) — total binding, every field in the preimage.
```

It rides a lifecycle transaction like ADR-0075's objects. At acceptance the transition checks:
the claim exists on this chain (in any phase from committed onward — a derivation of a claim
that later voids is a derivation of a voided claim, and says so when read); `output_root` equals
the claim's; the signer is the claim's executor; `(claim_id, transformer_id)` is unique;
`derived_per_claim ≤ PALW_DERIVED_MAX_PER_CLAIM` (4). It records the object beside the claim, in
the state root, and retires it with the claim (`CLAIM_RETIREMENT`). It credits **no weight, no
payment, no exposure**: the object is a statement, priced by its transaction fee, and a statement
does not need collateral to be useful — it needs to be checkable, which Decision 5 makes it.

**Decision 5 — verification belongs to the consumer, and the chain makes it possible.** Whoever
holds the answer (the user, or anyone the user hands it to) can check the whole derivation
without trusting the executor: recompute `output_root` from the ids and match the claim;
canonicalize under `grammar_id` and match `dsl_hash`; run the transformer named by
`transformer_id` and match `artifact_hash` and `artifact_bytes`. Every step is a pure function of
bytes the consumer has and ids the chain has. A false object is therefore publicly demonstrable
by anyone holding the DSL. Stated rather than hidden: in this ADR a demonstrated falsehood costs
the executor nothing on chain — no bond hangs on a derivation, because the chain cannot run an
arbitrary transformer and this ADR refuses to pretend it can. What a false derivation costs is
the one thing the object exists to carry: the executor's name on a provenance that anyone can
show is wrong. Decision 7's PALW-reward route was withdrawn by ADR-0144.

**Decision 6 — delivery, and what is under a data-availability obligation.** The gateway
(ADR-0077 Decision 3/4) returns, in one response: the answer (the DSL, as text), the artifact (as
bytes, inline for small artifacts and by a fetch handle above a size the gateway states), and the
signed `DerivedArtifactV1` it submitted or would submit. The artifact is never under a DA
obligation — the chain has no use for it. The DSL may be, at the user's election: an executor
that opts a claim's DSL into the obligation serves it on request like a capture opening
(ADR-0077 Decision 8), so that third parties can verify a derivation they did not commission;
the default is off, because the DSL is the answer to the user's prompt and ADR-0044 Decision 8's
sentence about silently publishing prompts applies to answers word for word.

**Decision 7 — transformer に PALW weight を与える案は不採用。**

mesh builder・rasterizer・simulator などを独立した PALW class にする案は、ユーザーがローカルで行う推論を検証する目的から外れるため。[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P3 に従い、derivation object は weightless とする。ユーザーの推論自体である computation は別途 class の要件で判断する。

**Decision 8 — the kind table, v1.** One row per kind a person asked for; each names its DSL, its
transformer's discipline, its artifact, and what "verify" means. Non-goals are in the table on
purpose, because a kind that is not there is not covered, and a reader should not have to infer
it.

| kind | the DSL (the model's answer) | transformer | artifact | determinism basis | not covered |
|---|---|---|---|---|---|
| `scene` | Canonical Scene DSL: objects, fixed-point transforms, materials by name, hierarchy | fixed-point mesh builder + canonical glTF writer (buffer order, accessor order and padding fixed) | `.glb` | integer geometry; no float in the path | texture synthesis; physics |
| `image` | vector/procedural DSL: paths, fills, layers, integer coordinates | integer rasterizer + canonical PNG writer (fixed filter, fixed zlib level, chunk order) | `.png` | fixed-point coverage; no AA randomness | diffusion / pixel models — those are model classes (ADR-0075's route), not transformers |
| `cad` | parametric solid DSL: sketch → extrude → revolve → boolean | exact-rational or fixed-point kernel; canonical STL / STEP writer | `.stl`, `.step` | exact arithmetic; booleans are the hard part and a kernel that cannot make them exact does not ship the boolean | NURBS surfaces at v1 |
| `code` | source files + a build manifest (targets, tests) | a pinned, hermetic toolchain: no network, fixed clock, fixed locale and env, build tree hash in the manifest | build outputs + a test log | hermetic reproducible build; the toolchain's own reproducibility is the gate — a toolchain whose output varies across runs is excluded by name | anything that fetches at build time |
| `map` | tile / graph DSL: cells, edges, integer attributes | integer map compiler | the map file | pure function | — |
| `music` | note-list DSL: pitch, onset and duration in ticks, channels, programs | Standard MIDI File writer | `.mid` | SMF bytes are canonical | waveform synthesis (float DSP) |
| `simulation` | scenario DSL: entities, integer rules, a seed, a step count | integer step simulator | a trace + a summary | an integer state machine; weightless derivation | anything with floating-point dynamics |

A kind is versioned by its grammar and its transformer, never by editing a row: a new grammar or a
new build is a new `grammar_id` / `transformer_id`, and the old derivations stay checkable against
the old ids forever, which is the same reason a PALW class is its graph.

**Decision 9 — the kind space is every (canonical representation, deterministic generator) pair;
the table is open, and the chain interprets none of it.** The v1 rows are the first seven of a
space whose shape is fixed by Decisions 2–5 and not by the rows: anything a model can answer in a
form a grammar canonicalizes, followed by anything a pure integer function can make of it, is a
kind. So the question this ADR settles for the network is not "what can Qwen3.6 make" but "how
many kinds of checkable computation can a prompt be the entrance to". The pipeline is §2's, every
time:

```text
natural language
   → the class (Qwen3.6, A16, any certified class)          one inference, one claim (ADR-0077 R0)
   → canonical representation                               grammar_id, dsl_hash
   → deterministic generator                                transformer_id
   → artifact                                               artifact_hash, artifact_bytes
   → DerivedArtifactV1, beside the claim                     the chain holds this and only this
```

Three consequences, each a rule:

* **Ids are assigned once and never reused.** A row's id is stable from the day it is written,
  whether or not its transformer exists yet. The chain checks `kind != 0` and interprets nothing
  else: what a kind means is the manifest behind `transformer_id`, and an object whose kind
  disagrees with its transformer's manifest is a false object under Decision 5 — demonstrable by
  anyone holding the manifest. Adding a row is therefore never a ruleset move.
* **◎ and ○ name which half is deterministic today.** A row is ◎ when the DSL is text a model
  already writes (JSON, source, a note list, a netlist) and the generator is integer or exact
  arithmetic. A row is ○ when the generator people actually want is itself a model (diffusion,
  speech, video) or float DSP: then the DSL half is a kind now — the storyboard, the voice
  script, the layer list — and the artifact half enters later, as a class through ADR-0075's route
  (§8) or as a fixed-point transformer through Decision 3's gate.
* **"The answer is the thing" is a kind.** For a document, a story, a curriculum, a spec, the
  transformer is the identity writer (`identity/v1`: the canonical DSL bytes ARE the artifact), so
  a chapter a person keeps carries the same provenance as a mesh.

The candidate table. One row per thing a person asks for; rows that are the same artifact under
another name are folded into their base row and listed under it, so that an id names an artifact
and not a marketing category.

| id | kind | asked for as | the DSL (the model's answer) | transformer | artifact | fit |
|---|---|---|---|---|---|---|
| 1 | `scene` | 3D: buildings, characters, props, scenes | Decision 8 | Decision 8 | `.glb` | ◎ |
| 2 | `image` | illustrations, UI mocks, textures, diagrams (as vectors); logos | Decision 8 | Decision 8 | `.png` | ◎ vector · ○ pixel models (a class) |
| 3 | `cad` | parts, enclosures, mechanisms | Decision 8 | Decision 8 | `.stl`, `.step` | ◎ |
| 4 | `code` | Rust, C/C++, Python, TS programs; Android/iOS/web apps (`app`) | Decision 8 | Decision 8; the app is its toolchain | outputs + test log | ◎ per named toolchain |
| 5 | `map` | 2D/3D maps, dungeons, worlds; GIS maps, routes, terrain, spatial data (`gis`) | Decision 8 | Decision 8 | the map file | ◎ |
| 6 | `music` | MIDI, scores, chord progressions, composition | Decision 8 | Decision 8 | `.mid` | ◎ |
| 7 | `simulation` | physics (integer), fluids (integer lattice), traffic, economy, game AI runs | Decision 8 | Decision 8 | trace + summary | ◎ |
| 8 | `text` | documents, articles, stories, scripts, summaries; lore, settings, characters, events (`world`); problems, materials, explanations, curricula (`education`); product specs (`product`) | canonical Markdown / plain text, optionally under a schema | `identity/v1` | the text | ◎ |
| 9 | `design` | API, DB schema, architecture, IaC | OpenAPI-, DDL- and HCL-shaped JSON | canonical schema writer + validator | schema files | ◎ |
| 10 | `game` | game rules, stages, NPCs, quests, world data packs | game-data JSON | validated game-data compiler | data pack | ◎ |
| 11 | `circuit` | schematics, PCB, Verilog/VHDL | netlist / HDL source | netlist canonicalizer; a deterministic integer HDL elaborator + simulator | netlist, sim trace | ◎ |
| 12 | `storyboard` | video: storyboard, cuts, motion, a video spec | shot-list DSL in ticks | canonical shot-list writer | the storyboard | ○ (the video is a class or a float render) |
| 13 | `voice` | speech, narration, voice settings | SSML-shaped DSL | canonical writer | the script | ○ (synthesis is float DSP or a class) |
| 14 | `animation` | rig, pose, motion, timeline | keyframe DSL: ticks, fixed-point transforms | integer keyframe compiler + canonical glTF animation writer | `.glb` | ◎ |
| 15 | `ui` | HTML, React, CSS, layouts; pages, components, SEO structure (`web`); UI systems and design tokens (`design`) | component-tree + token DSL | canonical HTML/CSS/token writer | site bundle | ◎ |
| 16 | `data` | SQL, analysis steps, statistical models, reports | SQL + an integer statistics plan over pinned tables | in-tree integer query engine | result set + report | ◎ integer · ○ float statistics |
| 17 | `science` | formulas, numerical methods, scientific computation | exact-rational / fixed-point program | exact evaluator | results | ◎ exact · ○ float |
| 18 | `robot` | motion plans, control code, task plans | waypoint / task DSL in fixed point | integer plan validator | the plan | ○ (execution on hardware is off-chain) |
| 19 | `agent` | agent workflows, tool plans, task graphs | task-graph DSL | DAG validator + canonical writer | the task graph | ◎ the plan · execution is Decision 10 |
| 20 | `database` | schema, migrations, queries, ETL | DDL/DML DSL | in-tree deterministic engine applies it to a pinned fixture | schema + fixture state | ◎ |
| 21 | `zk` | circuits, constraints, proof configurations | R1CS/PLONK-shaped constraint DSL | constraint compiler + integer satisfiability check on a witness | constraint system + check log | ◎ |
| 22 | `contract` | smart contracts, transactions, contract specs | EVM initcode + a test manifest | the in-tree EVM | runtime code + test log | ◎ |
| 23 | `math` | proofs, derivations, algorithms | proof-term DSL | in-tree proof checker (integer) | the checked proof | ◎ |
| 24 | `molecule` | molecular structures, reaction paths, experiment plans | SMILES / graph DSL | canonical graph writer + integer validity check | canonical structure | ○ (property prediction is float) |
| 25 | `manufacturing` | process plans, BOMs, routings; part structure | process DSL | integer scheduler / BOM compiler | plan + BOM | ◎ |
| 26 | `building` | BIM, floor plans, structural specs | fixed-point plan DSL | integer plan compiler | plan file (`.glb`, IFC-shaped) | ○ (structural analysis is float) |
| 27 | `procedural` | terrain, cities, worlds, environments | procedural DSL: seed + integer rules | integer generator | `.glb` / map | ◎ |

Priority among them is Decision 11's; admission is Decision 3's, one pair at a time.

**Decision 10 — four modes beyond generation ride the same object.** PALW is not limited to
"the model made something". Each mode below is a derivation with a different input shape, and
none needs a second object:

* **Transformation** (image → 3D, audio → MIDI, text → code, CAD A → CAD B, CSV → database). The
  source is part of the prompt and committed by the claim's prompt commitment. Where the source
  is bytes the model did not read as tokens, the DSL names them by hash and the transformer takes
  them as a second input: `dsl_hash` covers the naming, so the derivation is still a pure function
  of the DSL and of bytes the consumer must hold — X6's "answer ids" gains "and the named inputs",
  and nothing else moves.
* **Optimization** (faster code, a lighter CAD, a smaller circuit, a tighter stage, a shorter
  route). The answer is the candidate; the transformer builds it AND measures it with an integer
  cost model — instruction count on the in-tree VM, triangle count, gate count, path length in
  integer units — never a clock. The metric is in the artifact. "Better" is the difference of two
  derivations' metrics, and anyone can recompute both.
* **Verification** (code → tests, math → proof, circuit → simulation, CAD → constraint check,
  contract → invariant check). The transformer is a checker and the artifact is its verdict log.
  A checker is the purest transformer in the table — small output, and determinism is its whole
  point. A derived verdict is still not a court verdict (Decision 5): it convicts nobody; it is a
  statement anyone can re-run.
* **Planning** (natural language → task graph → agent execution → receipts). The artifact is the
  canonical task graph. Its execution is not a derivation — it touches clocks, networks and other
  inferences — and each inference it runs is its own claim (ADR-0077 R0). Linking a plan's claims
  into one object is the cross-claim item §8 leaves open, on purpose.

What generalizes is the name. The lineage's object is not *LLM output mining*; it is
**deterministic computational claim mining**: a canonical claim, a checkpointed receipt, a court
that verifies, a reward — and two entrances to it:

```text
                        PALW
                          │
             ┌────────────┴────────────┐
             │                         │
           LLM                    computation
     Qwen3.6, A16, …        generator / compiler / checker
             │                         │
     an inference is a         a transformer is a derivation      ← this ADR (weight: none)
     claim (ADR-0077)          no separate PALW weight             ← ADR-0144 P3
             │                         │
             └────────────┬────────────┘
                          ▼
                   canonical claim
                          ▼
                checkpoint / receipt
                          ▼
                 court verification
                          ▼
                       reward
```

Only the inference entrance bears PALW weight. A transformer produces a committed, checkable, weightless derivation; its former route to independent class weight was withdrawn by ADR-0144 P3.

**Decision 11 — the order in which domains open, and what §6 does with it.** The first areas,
in order: **① code → ② 3D / CAD → ③ game (map, game data, procedural) → ④ image / music → ⑤
simulation → ⑥ agent → ⑦ science / engineering computation.** §6's Q-03 still starts with
`scene` and `music`, because they are the cheapest proof that the machinery meets X3; after Q-04
the work follows this order — Q-05 is `code` and `cad`, Q-06 is `map`, `image` and `simulation` —
and the remaining rows enter in the order above, each as a grammar/transformer pair through
Decision 3's gate, with no ADR per kind. For ① the first toolchain NAMED is the one the tree
already holds at consensus grade: the in-tree EVM (`contract`, and `code` under `toolchain =
evm/v1`), X3-green on the day it ships. Pinned external toolchains (rustc → wasm32, solc, clang →
wasm32) are manifests — toolchain hash, arguments, environment whitelist, `SOURCE_DATE_EPOCH`, no
network — whose two-architecture drill runs on the fleet's Intel, AMD and Apple hosts, and none
is named by an object until its drill passes, exactly as Decision 3 says.

## 4. What this costs, stated before it is measured

* **Chain bytes.** One `DerivedArtifactV1` is a few hundred bytes; at most four per claim. No
  chunking is ever needed because nothing large is carried (Decision 1).
* **State.** One bounded table beside the claim table, retired on the claim's schedule; the state
  root grows by a bounded row per derivation.
* **Executor time.** The transformer's own run, after the inference. A scene build or a MIDI write
  is milliseconds; a hermetic build of a program is whatever the toolchain takes; a simulation is
  what it is, and the gateway streams the answer before the transformer starts, so a slow
  transformer never delays the text.
* **Consumer time.** Verification is one transformer run plus hashing, on bytes the consumer
  already holds.
* **Identity.** Introducing the object is a ruleset move (a testnet-11 relaunch); mainnet, PALW
  off, is untouched.

## 5. Invariants the tests must hold

```
X1   No consensus path accepts DSL bytes or artifact bytes as carriage, chunked or whole.
X2   A DerivedArtifactV1 is accepted only for a claim that exists on this chain, with the claim's
     own output_root and the claim's own executor key; (claim, transformer) is unique; at most
     PALW_DERIVED_MAX_PER_CLAIM per claim.
X3   A transformer named by any object is byte-identical on two architectures over the drill's
     DSL corpus, and declares no floating-point arithmetic on any path that reaches the output.
X4   A parse failure under a grammar produces no object and changes nothing about the claim.
X5   A derivation credits no weight, no payment and no exposure (ADR-0144 P3).
X6   From (answer ids, the job's context hash, the family, grammar_id, transformer_id) alone, a
     consumer recomputes output_root, dsl_hash and artifact_hash and reaches the object's values
     or a demonstrable mismatch (`palw-derive verify`).
X7   ADR-0077's R0 and R1 hold unchanged: one inference, one claim; the receipt's bytes and the
     seat's bytes are what they were.
X8   A kind id is assigned once and never reused; the chain checks kind != 0 and interprets no
     kind — a kind that disagrees with its transformer's manifest is demonstrable, never meaning.
X9   Extra inputs of a transformation or an optimization are named by hash inside the DSL, so
     dsl_hash still fixes the whole derivation; a metric is an integer cost model, never a clock.
```

## 6. Order of work

| unit | content | done when |
|---|---|---|
| Q-01 | `DerivedArtifactV1`, `derived_id_v1`, the kind table's ids | golden vectors; per-field mutation moves the id |
| Q-02 | the transition arm and the bounded table | X2 green; retirement with the claim |
| Q-03 | grammar + transformer for `scene` and `music` (the two with trivially canonical outputs) | X3 green on x86_64 and arm64 |
| Q-04 | the gateway's derivation step and one-response delivery | a browser request returns text, a GLB, and a signed object; X6 checked by the client |
| Q-05 | `code` (the hermetic runner, and `contract` / `evm/v1` as the first named toolchain) and `cad` (exact kernel, booleans last) — Decision 11's ① and ② | X3 green each; the kernel's boolean either exact or absent; an external toolchain is named only by its fleet drill |
| Q-06 | `map`, `image`, `simulation` transformers — ③, ④, ⑤ | X3 green each |
| Q-07 | the DA election for the DSL | served on request; off by default |

**Done when** a person asks a certified class for a scene, receives the JSON and the GLB, keeps
both, and anyone they hand the JSON to can recompute the claim's `output_root`, the `dsl_hash`
and the `artifact_hash` from the object on the chain — with the chain holding a few hundred bytes
and the GLB holding the megabytes.

## 7. Supersession

| what | disposition |
|---|---|
| ADR-0077 §2 "Where this ADR ends" | this ADR is the layer it points to |
| ADR-0077 R1 — the artifact goes to the user | honoured one layer up: the derived artifact goes to the user, its derivation to the chain |
| ADR-0075 Decision 14 — chunked carriage for objects above one carrier | not used: nothing here is above one carrier, by construction (Decision 1) |
| ADR-0053 — one execution family, pure Rust, integer | the transformer discipline (Decision 3) is the same rule applied to a non-model computation |
| ADR-0044 Decision 8 — prompts must not be silently published | applied to answers (Decision 6): DSL availability is the user's election, default off |

## 8. What is deliberately not decided

* **Slashing for a false derivation.** The standalone transformer-weight route is withdrawn (Decision 7); a court that cannot compute the transformation cannot objectively convict it. No additional transformer escrow is selected here.

* **Model classes for images and audio** — a diffusion model is a class, and its route is
  ADR-0075's, not this layer's.
* **Waveform synthesis, physics, texture generation** — floating-point by nature today; a
  fixed-point synthesizer or physics step would enter through Decision 3's gate like any other
  transformer.
* **Cross-claim derivations** (an artifact made from several answers) — an object with several
  `claim_id`s; not needed for any kind in the table, and left out until one needs it.
* **Which external toolchain is named first for `code`** — a drill result on the fleet, not a
  sentence here; until one passes, `code` ships its runner and `contract` ships the in-tree EVM.
* **The rows of Decision 9 beyond v1** — each is admitted by its own grammar/transformer pair
  through Decision 3, in Decision 11's order; their ids are fixed now so nothing renumbers later.

## 9. Number hygiene

This is ADR-0078; ADR-0077 is the last on this branch. A concurrent claimant renumbers the later
writer, per ADR-0036 Decision 5.

## Security amendment (2026-09-02) — hardening before the kinds are built

**SA-1 — Executing model-written code is the largest privilege in the lineage (ADR-0079 §1), and
the in-tree EVM is not exempt.** `contract` / `code` under `evm/v1` runs model-written initcode: on
an ephemeral, isolated state with a gas ceiling from the transformer manifest, in a separate process
under ADR-0079 Decision 5's confinement, never against the chain's EVM state and never inside the
node process. The gas ceiling and the state-fixture hash are part of `transformer_id`'s manifest.

**SA-2 — Every transformer declares input and output bounds, enforced before it runs.** A DSL is
attacker-shaped — it is the model's answer to a stranger's prompt — and a procedural or scene DSL
can encode a mesh that exhausts memory at build time, on the executor and on every consumer who
verifies. The manifest carries `max_dsl_bytes`, `max_artifact_bytes` and `max_steps` (or the kind's
own unit); exceeding one is "no object" (Decision 2's parse-failure arm), and X3's drill includes a
bound-exhausting corpus.

**SA-3 — Transformation inputs named by hash (Decision 10) are bytes a stranger uploads.** The
gateway bounds their size, holds them only for the job's life, and hands them to the transformer
through the same confinement; a hash the consumer cannot resolve is a demonstrable gap, not an
error the chain sees.

**SA-4 — The DSL DA election (Decision 6) is served like an opening**: to bonded requesters only,
bounded, rate-limited (ADR-0077 SA-2), so opting a DSL in cannot make the executor a public file
server.

**SA-5 — A derivation is refused for a `kind` whose transformer manifest is not published in the
tree at the object's `transformer_id`.** A consumer who cannot fetch the manifest cannot verify, and
an unverifiable statement should not be storable — Decision 5's promise depends on it.

**SA-6 — `agent` (kind 19) and Decision 10's planning mode produce artifacts only**; ADR-0079
Decision 8 governs. Nothing in this ADR's tooling executes a task graph, and the drill asserts it
(ADR-0079 S10).

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) P3: the answer the user
reads and the work the chain rewards are the same execution — not a second run, not a proof job
beside it.

* **Kept.** Decisions 1–6, 8–10: a derivation is committed; the artifact never rides; a derivation
  credits no weight, no payment and no exposure. That is already the honest weight of a computation
  the court cannot try, and it is the product layer this ADR exists to name.
* **Decision 7 withdrawn as a PALW-reward door.** A mesh builder, rasterizer, simulator or hermetic
  toolchain that runs *after* the user's inference is a second purpose. Registering it as a PALW
  class so that "its leaves are counted work exactly as an inference does" would pay for work the
  user did not ask the model for. The door re-opens only if the transformer **is** the user's local
  inference — the thing they ran, on their machine, for their own use — not a derivation of its
  output. Q-08 (which transformer becomes a family first) is therefore not an implementation item
  for PALW weight. The object layer may still land later as a weightless product; it does not earn.

## Mission alignment amendment — 2026-10-07

「thing itself never rides」はapplication artifactの保存方式であり、claimの検査・局所化・裁定に必要なinput/state/output openingまで私的なままにしてよいという意味ではない。対象計算の公開証拠要件と、不要なapplication dataの非公開を区別する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0078-what-was-made-from-it-is-committed-the-thing-never-rides.md -->

</details>

<a id="source-0084-the-ids-ride-the-capture-stays-home"></a>

<details>
<summary>0084-the-ids-ride-the-capture-stays-home.md — 原文 L1–L470</summary>

[原文を開く](../adr/0084-the-ids-ride-the-capture-stays-home.md) · SHA-256: `771eec722893b231fad947cabe04f84abb171ba8c4c17e5facf93a309a235342`

<!-- ADR_SOURCE_BEGIN:0084-the-ids-ride-the-capture-stays-home.md -->
~~~~~~~~markdown
# ADR-0084: The ids ride, the capture stays home — a model-class claim serves its answer, never its history

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


**Status:** PROPOSED (2026-09-04), implemented on `palw-adr0084-served-answer` (see §7 for what
landed and what did not). **Consensus-inert**: no fence, no state field, no object variant; the
fingerprint does not move. Ships by a rolling rebuild of the seats and of every node that answers
material pulls; an old seat and a new executor interoperate (an old seat still pulls the whole
material and still gets nothing over the cap, exactly as today).
**Builds on:** ADR-0042 Decision 7 (the transport half: broadcast + pull), ADR-0073 Phase ① 1a/1e
(`FPC1` "the answer beside the question"; a seat checks, it does not re-run), ADR-0077 Decision 8
and SA-2 (the seat opens `k` intervals; "the executor never ships the capture whole"), ADR-0078 X6
(`output_root` is recomputable from the answer's ids, the job's context hash and the family's
rendered-hash rule), ADR-0082 Decisions 7 and 9 (the capture is a fold; a seat recomputes the
cache from the prompt it holds and "the committed output ids"), ADR-0065 Decision 4 (an
`Unavailable` quorum abstains — a separate fence, not touched here).
**Amends:** ADR-0073 Phase ① 1a, on the served-payload side only — what a seat is served for a
claim whose capture does not fit the transport is the question and the answer's ids, not the
capture. ADR-0077 Decision 8's last sentence — the attempt lane's seats "keep hashing their
canonical-job captures until Decision 14 makes those captures large, at which point the same arm
serves both lanes": the captures are large now, and this is that point.

## 1. What was measured (testnet-11 Relaunch 5f, genesis `ad30b5cb…`, 2026-09-03/04)

Read from the 5f genesis card (§6k–§6m, §7b, step 9) and from the code on the 5f head `b52502a6`:

| quantity | value | where |
|---|---|---|
| material cap, one broadcast / one served pull | `PALW_MATERIAL_MAX_BYTES` = 16 MiB | `protocol/flows/src/palw_gossip.rs:45` |
| interval-opening cap, one served opening | `PALW_INTERVAL_OPENING_MAX_BYTES` = 4 MiB | same file, `:718` |
| openings a seat draws per claim | `PALW_FP_SEAT_INTERVAL_SAMPLES_V1` = 4 | `consensus/core/src/palw_fp_interval_v1.rs:28` |
| floor material (attempt lane) | 2.27 MB | card §6m |
| QWEN36 attempt material (dense tuple) | 253 MB | card §6l |
| QWEN25-A16 graph-v5 attempt material (dense tuple) | 748 MB per 512-context job | card §6k/§6l |
| graph-v5 free-prompt material (the fold, ADR-0082 D7) | ~700 MB — 696.5 MB of checkpoint chunks per 512-context job | ADR-0082 §10.3 |
| serve allowance | 48 MiB per peer per minute, 256 MiB node-wide | `palw_gossip.rs` `SERVE_BUDGET_*` |

**What happened on the attempt lane.** `palw_producer.rs` broadcasts the whole material and only
then submits the block; `broadcast_palw_material` has no size check on the push path; the
receiver's `admit_material` refuses `> cap` only after the bytes have arrived (gRPC decode cap 1
GiB); the serve path refuses `> cap` before sending ("never serve what the transport would
refuse"). On seat2's links 748 MB × five peers did not finish inside the 120 s flow window: the
peers' ping and relay flows timed out, the routers were torn down, and the block — announced once,
queued behind the material — was gone (card §6l). QWEN36's 253 MB crossed ibm's DC links, and
still **no seat could pull it**: at 21:34 UTC node1 and `.113` filed `Unavailable` for QWEN36's
first claim `e78441c7…` at the half-window. Every model-class attempt claim on this chain reaches
its half-window as Unavailable × quorum, is redrawn once, and voids by `ReceiptTimeout` with nobody
slashed on the shipped path (card §6m, correction). seat2's v5 production was paused for it.

**What happened on the free-prompt lane, read in code.** ADR-0077 Decision 8 and ADR-0082
Decision 9 are implemented: `kaspad/src/palw_fp_seat.rs` draws the intervals, `palw_panel.rs`
`fp_interval_seat_outcome_v1` recomputes the checkpoint state and replays them, and the openings
travel on their own authenticated lane. **The arm's two inputs, though, come off the whole-capture
pull it was built to retire**: the job and the prompt off an `FPM1`/`FPC1` payload
(`fp_job_material_for_claim`), and the committed output ids off the `FPC1` payload's CAPTURE
(`fp_committed_output_ids_v1`). The latter names `base0_material_decode_v1` — the DENSE tuple's
decoder — and a graph-v5 free-prompt capture is the FOLD (`base0_fp_material_encode_v2`,
`qwen25_a16_backend.rs:1162`), so even an `FPC1` that had arrived would yield no ids for a v5
claim. The panel's own comment on that function says where the ids belong: "on the seam … or in a
small served object of their own, so a seat that fetches no capture does not have to hold one".
This ADR is that object.

**What happened to the two public free-prompt claims (STL `b15ef21c…`, MIDI `d575928f…`).** The
rail was run without `--capture` and with `--retention-dir /root/fp-5f/outbox/traces` (card, step
9). Without `--capture` the submitter stages the question-only `FPM1`; the node's panel reads
`<appdir>/<network>/palw-retention/` (`kaspad/src/daemon.rs:1358`) and never that directory. The
executor's node therefore held nothing for either claim: every pull answered `NotHeld`, no interval
could be opened, and the seats abstained. The reading "their material exceeds the cap" was wrong
for those two claims; the cap failure is the attempt lane's, and any free-prompt claim staged
correctly would have met it next.

## 2. The requirement

**R1 — a seat's bytes are bounded by the answer.** What a seat needs to conclude on a model-class
claim, on either lane, is the job (fixed size), the prompt's ids (`4 × prompt_tokens`) and the
answer's ids (`4 × decode_tokens_executed`). Everything else it recomputes (ADR-0082 D9) or fetches
one interval at a time (ADR-0077 D8). Nothing a seat fetches may grow with the history.

**R2 — a cap never overrules an admission.** The constant's own doc states it; the shipped tree
violated it twice (8 → 16 MiB on 2026-08-28; 16 MiB against 253 and 748 MB on 2026-09-03). The
cure is not a third number: it is that nothing the transport would refuse is ever handed to it.

**R3 — "retained" means the serving node has it.** The data-availability obligation is discharged
by the node that answers pulls and opens intervals. A capture on a disk that node does not read is
not retained, whatever the rail printed.

## 3. Decisions

**Decision 1 — a third free-prompt payload: the answer envelope `FPA1`.**
`PalwFpAnswerV1 { material: PalwFpMaterialV1, output_token_ids: Vec<u32> }`, wire magic `FPA1`,
beside `FPM1` (the question) and `FPC1` (the question and the capture). Its decoder re-checks the
one binding it can prove itself — the prompt ids hash to the job's `prompt_token_ids_hash`, as
`FPM1` does — and nothing else. **The answer's ids are bound by the seat, to the chain**:
`output_commitment_v2(job_context_hash, ids, rendered_output_hash_for_family(ids))` must equal the
claim's `output_root` (ADR-0078 X6's recompute, through a new backend seam
`fp_output_root_v1(job, ids)` that builds the context exactly as `fp_recompute_checkpoint_root`
does). `PalwSeatDutyV2` carries `output_root`, read off the state's claim record where
`execution_root` and `trace_root` already are — a read view, not a state field. An envelope whose
ids do not recompute the root is refused BY NAME before any forward pass is spent, so a forged
envelope cannot make an honest claim's seat file nothing: the pool is iterated and the honest one
binds. Size: the job plus `4 × (prompt_tokens + decode_tokens_executed)` — under 3 KB at 512, 512
KB at 131,072, inside the material cap at every ladder the ruleset admits.

**Decision 2 — the resolver serves the envelope whenever the capture does not fit.** A node answers
a whole-capture pull for a claim with its retained material when that material is within
`PALW_MATERIAL_MAX_BYTES`, exactly as today, and otherwise with the claim's answer envelope: the
`<claim>.answer` file staged beside `<claim>.material` at submission (Decision 5), or, when there is
none, an envelope derived once from the retained capture through the seam and cached under that
name. This is a property of the RESOLVER the panel registers, not of the transport: the gossip
module, its caps, its budgets and its message types are unchanged, and no payload the transport
would refuse is ever handed to it. The envelope arrives at the asker as the same
`PalwTraceMaterialBroadcast` a served material does, is pooled the same way, and is read by the
seat's free-prompt arm as the job material it already accepts (`palw_fp_job_material_decode_v1`
reads all three magics) plus the ids it lacked.

**Decision 3 — nothing over the cap is pushed, and the block goes first.** `broadcast_palw_material`
refuses bytes over the cap before `hub().broadcast` — one warning naming the claim and the cap, no
message — and the attempt-lane producer submits its block BEFORE it announces material. Since
protocol 104 the announcement is a courtesy and the pull is the obligation ("announced, not
pushed", `palw_producer.rs`), so a skipped announcement costs a seat one pull it makes anyway,
while a block queued behind 748 MB cost the block. The panel's canonical-claim path already
submits the transaction first; it gains the same refusal. The cap stays 16 MiB: it now bounds what
a seat NEEDS (an envelope, a floor capture, a graph-v2 A16 capture), and a class's capture size
stops being a transport concern at all.

**Decision 4 — the interval arm serves both lanes.** For an attempt-lane claim of a class that
registers a checkpoint geometry, the seat runs the same arm the free-prompt lane runs, with the
lane's own sources: the job context and the prompt from the anchor (`job_for_anchor`, as
`job_anchor_for_claim` derives it today — never from the material), the answer's ids from the
attempt-lane envelope `ATA1` (`PalwAttemptAnswerV1 { anchor, prompt_token_ids, output_token_ids }`,
bound to the claim's `output_root` through the anchor-derived context by the same X6 recompute),
the interval count from the context's `declared_prefill_tokens` and `exact_decode_tokens`, the
checkpoint state recomputed from the prompt (D9) through a context-taking seam the free-prompt
seam now delegates to, and `k` intervals opened on the interval lane and replayed exactly. The
executor's resolver opens intervals of a raw family capture by taking the prompt from the anchor
the capture's own binding names, which the seat re-derives independently and the opening's ids
bind to. The whole-capture arm (`verify_material`) stays for every material that fits — the floor
and the graph-v2 class — and remains the fallback where a class has no geometry.

**Decision 5 — one retention directory, and the node names it.** The node reports the directory
its panel serves from in `getPalwProducerFacts` (a version-5 suffix field, node-local exactly like
`lockedBondOutpoints`; a version-4 reader sees nothing). `misaka-palw-fp-rail --submit` and
`misaka palw fp-submit` take `--retention-dir` when given; otherwise they use the node's own
directory when the node is on this host, and refuse to stage anywhere else, saying why. At staging
the submitter writes `<claim>.answer` (`FPA1`, from the worker result's `output_token_ids`) beside
`<claim>.material`, staged `.partial` and renamed under the same ordering as the material — so the
serving node never decodes a 700 MB capture to answer a pull, and a claim's answer is servable the
moment the claim is on chain. The rail's summary prints the directory the files went to and whether
it is the node's.

**Decision 6 — the ids read through the seam, on either retention form.**
`PalwExecutionBackendV1::fp_committed_output_ids` is implemented on the base0 backends over
`base0_material_decode_any_v1` — the fold and the dense tuple alike — and the panel reads a
capture's ids only through it. The panel names no family decoder; the fold gap in §1 closes with
it.

**Decision 7 — the attempt lane's verdict is by execution (added 2026-09-04, after §7.2).**
A seat that holds no material for an attempt-lane claim re-runs the canonical job the claim's
anchor implies and licenses the claim when the run reproduces its execution root, trace root and
priced work — `PalwExecutionBackendV1::execute_for_verdict`, which a base0 family answers from the
fold (ADR-0082 Decision 7): the producer's roots without the producer's tiles, 64 KiB in memory
where the dense run is 784 MB. The seat replays once per claim, and only after
`PALW_ATTEMPT_REPLAY_GRACE_DAA` (2) past the binding, so a material inside the cap that is merely
late still arrives by push first. Nothing is fetched; the 784 MB never moves. §7.2 measured why
this is not optional: no graph-v5 interval opening fits the opening lane, and 5f licensed these
blocks only because the uncapped push delivered the material — which Decision 3 closes. A replay
that does NOT reproduce the roots files nothing here (the opening lane decides from there); filing
`Invalid` from a replay is deliberately left until the replay has been seen agreeing on the fleet,
because a seat defect that produced mass `Invalid` would slash honest producers (ADR-0027).

## 4. What this costs, stated before it is measured

* **Bytes per seat, per claim**: the envelope (≤ 3 KB at 512, ≤ 512 KB at 131,072) plus `k` = 4
  openings (≤ 4 MiB each by the transport's backstop, `O(interval × row + log₂ leaves)` by
  construction). Independent of the capture's size, on both lanes. Before: 748 MB the transport
  refused, so zero bytes and no verdict.
* **Compute per seat**: unchanged from ADR-0082 D9 — one forward pass of the job plus `k`
  interval replays; the X6 recompute is one hash over the ids.
* **Executor disk**: one small file beside each material. A node that derives an envelope for a
  claim staged before this change pays one decode of the capture, once, and caches it.
* **The court**: unchanged and named. A terminal close still needs the ACCUSED capture on the
  closer's side (`refutation_for_free_prompt_index(&accused_bytes, …)`); the executor holds its own,
  a challenger of a claim whose capture exceeds the cap does not, and its pull answers with the
  envelope, which a close cannot be assembled from. The court then stalls at "the close needs the
  ACCUSED capture and this node holds none" — the standing behaviour on this chain, now bounded to
  the court rather than reaching certification. Assembling the close from an interval opening
  (the opening that contains the disputed step carries the operands the refutation names) is the
  follow-up, listed in §8.

## 5. Invariants the tests must hold

```
Y1   R1: a seat judging a model-class claim on either lane fetches at most the envelope plus k
     openings, whatever the capture's size; the envelope is under PALW_MATERIAL_MAX_BYTES at
     every ladder the ruleset admits.
Y2   An envelope whose ids do not recompute the claim's output_root is refused by name before
     any forward pass; with a forged and an honest envelope pooled in either order the seat
     concludes on the honest one.
Y3   No payload over PALW_MATERIAL_MAX_BYTES leaves this node: the push path refuses before
     the broadcast, the resolver answers an over-cap material with the envelope. The constant
     and the gossip module's behaviour on what it receives are unchanged (their tests pass
     untouched).
Y4   A block is submitted before its material is announced; an over-cap announcement is
     skipped with one warning naming the cap; the block propagates.
Y5   The submitter stages <claim>.answer beside <claim>.material under the same
     partial-then-rename ordering; the resolver serves the answer for an over-cap material and
     the material otherwise; with no .answer file it derives one from an FPC1 capture and caches it.
Y6   Attempt lane: the envelope binds to output_root through the anchor-derived context; the
     draw's counts come from that context; a claim whose material exceeds the cap reaches a
     Valid receipt on a seat that fetched no capture.
Y7   fp_committed_output_ids reads a fold and a dense tuple alike; the panel names no family
     decoder.
Y8   getPalwProducerFacts version 5 is read by a version-4 reader with the new field absent;
     an explicit --retention-dir always wins; a submitter given no directory and a node on
     another host refuses to stage, by name.
```

## 6. Order of work

1. Decision 3 (transport guards): `broadcast_palw_material` refusal; the producer's block-first
   order. Independent of everything below and safe alone.
2. Decision 1 (the envelope, the seam, the duty field) and Decision 6 (the ids seam on the base0
   backends).
3. Decision 2 (the resolver) and the seat's free-prompt arm reading the envelope.
4. Decision 5 (the submitter's `.answer`, the facts field, the rail/CLI default).
5. Decision 4 (the attempt-lane arm): the context-taking recompute seams, `ATA1`, the producer
   staging its answer, the resolver opening a raw capture, the seat's attempt arm.
6. The card's §6m fleet decision is reversible on the day 1–4 are deployed on ibm, `.113`, node1
   and seat2: seat2's v5 production resumes; QWEN36 claims start reaching Final.

## 7. Implementation record (2026-09-04, `palw-adr0084-served-answer`, from 5f head `b52502a6`)

**Landed — §6 items 1–5, all consensus-inert; `cargo check` and the crates' unit suites green
on the branch (see the commits for the exact test names):**

| decision | where | what |
|---|---|---|
| D1 `FPA1` | `consensus/core/src/palw_freeprompt_v3.rs` | `PalwFpAnswerV1`, encoder, decoder (prompt binding; no empty answer), `palw_fp_committed_output_ids_decode_v1` (`FPA1` direct, `FPC1` through the seam); `palw_fp_job_material_decode_v1` reads all three magics |
| D1 binding | `palw_backend.rs`; `palw_producer_v2.rs` | `fp_output_root_v1` (ADR-0078 X6); `PalwSeatDutyV2.output_root` off the claim record |
| D2 resolver | `kaspad/src/palw_panel.rs` `serve_material_or_answer` | the material when ≤ `PALW_MATERIAL_MAX_BYTES`, else `<claim>.answer`, else derived once from the capture (`FPC1` → `FPA1`, raw capture → `ATA1`) and cached |
| D2 seat | `palw_panel.rs` `fp_committed_output_ids_v1` | ids off `FPA1` or `FPC1`-through-the-seam, bound to `output_root` by name before any forward pass; a bound envelope retained under `foreign/` |
| D3 | `protocol/flows/src/flow_context.rs`; `kaspad/src/palw_producer.rs` | `broadcast_palw_material` refuses over the cap before the broadcast; the producer submits its block before it announces |
| D4 `ATA1` | `consensus/core/src/palw_attempt_v2.rs` | `PalwAttemptAnswerV1 { anchor, prompt_token_ids, output_token_ids }`, encoder, decoder |
| D4 seams | `palw_backend.rs`; base0 floor / A16 / Qwen3.6 backends | `fp_job_context_v1`, `checkpoint_root_for_context_v1`, `checkpoint_covered_bound_for_context_v1`, `output_root_for_context_v1`; the free-prompt seams delegate to them, so there is one context per claim on either lane |
| D4 seat | `palw_panel.rs` `interval_seat_outcome_v1` (was `fp_interval_seat_outcome_v1`), `attempt_committed_output_ids_v1` | the interval arm keyed on the context; on the attempt lane it runs after the whole-capture arms and before the pull, with the job and prompt from the anchor and the ids from `ATA1` or a held capture, bound under the anchor-derived context |
| D4 executor | `palw_producer.rs`; `palw_panel.rs` `open_retained_interval`, `backend_for_raw_capture_v1` | the producer stages `<attempt>.answer` beside its material; the resolver opens intervals of a raw capture with the prompt the anchor derives |
| D5 | `rpc/core` v5 `palw_retention_dir`, `rpc/grpc` field 26, `rpc/service`, `flow_context::palw_declare_retention_dir`; `misaka-palw-fp-submit` `FpStaging.output_token_ids` / `<claim>.answer`; the rail; `misaka palw fp-submit --capture` | the node names its directory; the submitter stages the envelope under the material's ordering; the rail defaults to the node's directory and refuses one not on this host |
| D6 | the three base0 backends | `fp_committed_output_ids` over `base0_material_decode_any_v1`; the panel names no family decoder |

**Found by the loopback devnet run (2026-09-04, three nodes, two graph-v5 producers on this
branch) and fixed in `75bac7be`:** the resolver that opens intervals of a RAW attempt-lane capture
picked the first held class whose codec decoded the bytes — every base0 family's codec decodes a
graph-v5 tuple, and with the floor first in the class table the floor's backend was chosen, derived
a floor prompt, and no interval ever opened: the seats held the envelope for fifteen minutes and
served nothing. The capture's job context names its shape profile, which is the class id, and the
row must match it. Measured on the same run before the fix: 784,693,059-byte materials with a
336-byte `ATA1` beside each; the push guard refused every announcement and the blocks still
reached all three nodes (Y4); each seat pooled the other executor's envelope under `foreign/`
(Decision 2 on the attempt lane); the artifact-less seat filed `Incapable`.

**Not landed here, by name:**

* **W7-shaped end-to-end evidence.** No devnet drill in this record certifies a claim whose
  capture exceeds the cap through the interval arm (Y1, Y6). The unit suites pin the payloads, the
  bindings, the staging ordering and the RPC round trip; the loopback devnet (card §6a's three
  kaspads and two v5 producers) is where Y1/Y6 are measured, and that run is the next step before
  the fleet rebuild.
* **The opening cost of a raw capture.** `open_retained_interval` on an attempt-lane capture
  decodes the whole retention twice per request (`capture_shape`, then `open_fp_interval`) — the
  free-prompt path already does — ~0.15 s and 575 MB peak for a 253 MB QWEN36 tuple (the exporter's
  measurement), more for 748 MB. Bounded by the serve throttles; a class-tagged retention file
  would remove the first decode.
* **The exporter** (`tools/palw-jobs-export`) still reads the answer's ids off `<claim>.material`;
  `<claim>.answer` is a few kilobytes and is the better source now.
* §8's list: the court's close over the cap (U-07c), ADR-0065 D4's activation, the cap itself.

### 7.1 The devnet, run 3 (22:05–22:33 JST): the opener was right about the leaves and wrong about the ladder

Every a16 claim on both producing nodes answered the asking seat with "the capture's leaves do not
reproduce the step root the binding committed". A diagnostic that decodes the retained material
(`inspect_devnet_material`, env-gated, ignored) said otherwise about claim `49b75d87…`: 6,630,544
leaves, 6,630,544 tiles, no duplicate and no gap, and the flat root, the level-12 fold root and
the committed root are one hash. What differed was the ladder: the opening assembler asked
`step_range_opening_root_v1` — bounded by `PALW_STEP_LEG_MAX_LEAVES` (2^22 = 4,194,304) — and
`.ok()` turned its `LeafCountOutOfRange` into a root mismatch. Three siblings on the same step
space reached for the same uncapped functions and would have refused next: the seat-side opening
verifier, the challenger replay (ADR-0085), and `base0_material_matches_claim_capped_v1`, which
reports any pulled graph-v5 material as a mismatch. `1fcb209a` passes the ruleset's cap every
one of them already held, makes the dense rebuild one spelling
(`base0_dense_step_leaves_capped_v1`) for the verifier and the opener, and pins 2^22+1 leaves
under a 2^23 cap with two tests that fail on the uncapped line.

**Two consensus-side siblings are NOT changed here**, because they are validity rules:
`check_execution_step_refutation_opened_v1` (palw_step_refute.rs) walks a refutation's range
opening with `step_range_opening_root_v1`, and `check_step_refutation_v1`'s `open_against`
(palw_step_leg.rs) walks a single-leaf opening with `step_opening_root_v1`; neither has a ruleset
in scope. So a refutation over any class whose space exceeds 2^22 leaves — graph-v5 at n_ctx 512
is 6.6 M — does not walk, and the court cannot refute such a claim today on any node. Loosening
that is a fork without an activation; it is item U-08 for its own ADR, with a flag.

### 7.2 What the same material says about the opening lane, measured

The class's leaves per call, read off the retained attempt material: 103,008 per prefill position
(122,024 at the position that selects a token), 6,508,520 for the 63-position prefill, 122,024 for
one decode call. `Base0FpIntervalGeometryV1` puts call 0 (the whole prefill) and call 1 into
interval 0, and one decode call into every later interval (checkpoint cadence 1). An opening
carries `PalwStepRangeOpeningV1.leaf_hashes` — 64 bytes per leaf of its range — so on this class
interval 0 opens at **≈424 MB** and a decode interval at **≈7.8 MB**, against
`PALW_INTERVAL_OPENING_MAX_BYTES = 4 MiB` (ADR-0077 D8, W10) and a seat ceiling
(`palw_fp_interval_opening_ceiling_v1`) that prices an opening as `positions × row_bytes`. No a16
interval opening can be served under the format and the caps as they stand; the gossip cap's own
rule — "a family whose widest registered row opens larger than this raises it HERE, before the
class is registered" — was not applied when graph-v5 was registered, and no cap reaches 424 MB.
Y1 therefore cannot be shown on this class by this branch; the root fix was necessary and is not
sufficient. Two shapes of fix exist and this ADR decides neither:

* **Siblings-only openings.** Drop the per-leaf hashes; the seat recomputes the interval and
  checks the range root from the siblings; a fault is located from the seat's own leaves (the path
  to one wrong leaf needs only the honest siblings). A decode interval falls from 7.8 MB to the
  seed rows plus the state chunk; interval 0 stays the prefill replay, which the seat performs
  anyway. A wire-format change (V4) with court consequences for ADR-0085's challenger, which
  derives paths from the served leaf hashes.
* **Per-position prefill intervals and caps sized in leaves.** Split interval 0 by position
  (6.6 MB each), price the seat ceiling in leaves, raise the transport cap to the widest interval.
  Keeps the format; every a16 opening is then 6.6–7.8 MB plus the state chunk.

Run 3 also submitted a **4-token** graph-v5 free-prompt claim (`a7b0a4bc…`) whose FOLD material
was 3,690,311 bytes — inside `PALW_MATERIAL_MAX_BYTES`, so the drill's own verdict line said Y1
was not exercised; the ~750 MB claims on testnet-11 are the same material at 512 tokens. Run 4
(`1fcb209a`, `MAX_TOKENS=16`) is recorded below when it finishes.

### 7.3 Decision 7 on the devnet (run 5, 23:22 JST, `3c6ea28b`)

Run 3 also showed what the seats do with an attempt claim they hold no material for: nothing.
The chain is pool → own retention → foreign file → the interval arm, and none of those produces a
graph-v5 verdict once Decision 3 stops the push; 5f licensed these blocks only because its push
had no cap. Decision 7 was added for that (§3) and run on the devnet: node-1 replayed both of
node-0's a16 claims after the 2-DAA grace and reproduced the execution root and the trace root
exactly, from the fold, in one execution each — and refused them over `work Some(6630544) vs
priced 0`, because an attempt claim is built with `work_leaves == 0` (the state's spelling for the
class's canonical job) and the rule compared the replay's count against it. `c24c3b9d` states the
rule once (`replay_licenses_v1`: roots reproduce; work reproduces when the claim prices it) with
tests for the unpriced, the priced and the mismatching cases. Run 6 records the licence.

### 7.4 Run 6 licences by replay; the fleet is on `c24c3b9d` (2026-09-05, 00:00–00:50 JST)

Run 6 (`c24c3b9d`, 4 tokens, nothing else on the CPU): node-1 replayed node-0's a16 claim
`…2d155f08` after the 2-DAA grace and filed **Valid** — "licensed by replay — the anchor's job
reproduces the claim's roots … 7s; no material moved" — from the fold, in seven seconds, where the
producer's dense draw is minutes; node-2 (no artifact) filed Incapable. That was the gate for the
fleet. Run 5 had also served interval 0 of an a16 attempt claim as **424,359,978 bytes** in 14 s
(the §7.2 figure, measured) and the transport dropped it without a line; the asking seat stayed at
"0 openings held". The 16-token free-prompt fold capture was 9,788,323 bytes — still inside the
material cap, so the drill's Y1 verdict line says "NOT exercised" up to about thirty tokens on
this class.

Fleet, in the order the runbook fixes (seat2 → .113 → ibm → verify), binary sha `5c21b1a87fb2e868`
= `c24c3b9d`, fingerprint `71b35c25…` unchanged on every host: seat2 respawned in 15 s (15:19:52Z);
.113's node, seat4 and three pool slots in 18–27 s each; ibm's node1 in 36 s; ibm's **node0
acknowledged the SIGINT (RPC and P2P servers stopped, "panel stopping" at 15:31:14Z) and then sat
for 16 minutes at 0 % CPU with all 47 threads in futex wait** — the public node's recorded
silent-hang signature, this time on the way down. SIGKILL at 15:46:56Z; systemd respawned it in
21 s and it printed the fingerprint at 15:47:47Z. seat2's `--palw-produce
--palw-producer-class=4277d84f…` were restored from `c-seat2.sh.pre-5f-pause` (the diff was those
two flags and nothing else; the paused script is kept as `c-seat2.sh.paused-until-20260905`) and
seat2 respawned at 15:48:48Z with the producer loading its artifacts. The cosmetic `0085e2bb`
(the licence line names the replayed leaves) is on the branch, not on the fleet.

By 00:50 JST run 6 had replayed every a16 attempt claim its producers put on the chain — node-0
eight of node-1's, node-1 four of node-0's, each licensed in 7–8 s, zero Unavailable on either —
and ibm's node0 was filing Valid receipts and a ReceiptLicensed of its own eighteen minutes after
its forced restart, with blocks flowing through it: the post-swap check the public node's hang
record prescribes.

### 7.5 U-08 lands behind a fence: the court walks at the ruleset's ladder (2026-09-05)

§7.1 left the two consensus-side walks at `2^22` because loosening them is a fork. They are now
loosened *behind an activation*, `Params::palw_court_ladder: Option<ForkActivation>`, with the
`palw_da_court` contract: `None` on every shipped preset, byte-identical fingerprint while absent,
`Some(never())` collapses to `None`, a scheduled height keeps old and new builds peers, and
`always()` is a real rule difference. The fence is read in exactly one place,
`palw_court_step_ladder_at(daa, court)` in the virtual processor, at the BLOCK's acceptance DAA:
`PalwCourtParamsV2::max_step_leaf_count()` past the fence, `PALW_STEP_LEG_MAX_LEAVES` before it.

The ladder is now an ARGUMENT all the way down: `adjudicate_court_close_v2` and
`adjudicate_close_proof_v2` take `step_ladder: u64`; the `Arithmetic` arm calls
`check_execution_step_refutation_capped_v1`, whose structural pass is
`check_step_refutation_capped_v1` (its `open_against` walks `step_opening_root_capped_v1`) and
whose operand-run openings walk `step_range_opening_root_capped_v1`. The uncapped names are the
capped ones at `2^22`, and every caller outside the court still uses them. The shape pass needed
no change — since ADR-0082 D1 it compares the leaf count at the accusation's own claim.

Pinned at the boundary, per the off-by-one rule: with the base0 honest matmul fixture, a ladder one
leaf short of the refutation's own `step_leaf_count` refuses by name
(`Leg(LeafCountOutOfRange { got, max })`), at exactly it the step is `NoFaultFound` again, and at
`2^22` the capped name answers what the uncapped one always did; the court test repeats the first
two as `DoesNotAdjudicate` versus `ExecutorGuilty`. What is NOT exercised: a refutation over a
space wider than `2^22` walking to a verdict — the fixtures are small, and the only class with
such a space is graph-v5 on the real artifact, whose court close still needs U-07c's opening-fed
refutation to exist at all. So on testnet-11 the fence stays `None` until (a) the close can be
assembled from an opening and (b) an operator schedules it; arming it earlier changes nothing a
court can reach.

### 7.6 The serving side has a load, and nothing bounded it (2026-09-05, testnet-11, `1d81feec`)

Decision 8's lane was specified per request: a seat asks, the executor opens, the gossip lane
refuses a SECOND asker for a pair already in flight. Nothing in the ADR says how many DISTINCT pairs
one executor opens at once, and the live panel answers that question for it — three seats drawing
four intervals each is twelve, and every one of them is a fold replay wanting gigabytes.

Measured on claim `019efe78…`: six to eight concurrent openings on a 24 GB host put it 12 GB into
swap, each opening went from ~30 s to 137–398 s, and twice the node stopped serving entirely for
forty minutes with nothing in its log and no response to SIGINT. The same host also recomputed each
interval per asker — 187 twice, 101 twice, 11 three times — because the served bytes were thrown
away.

Two panel-local additions, consensus-inert: a `served_openings` cache keyed by (claim, request
index, disputed leaves — the last read off the chain, so it belongs in the key), and an
`opening_gate` that serialises the heavy work on the blocking thread the resolver already runs on.
Queued, the same twelve cost the same total CPU and the first answer arrives in a fraction of the
time. **The lesson is the shape of the omission, not the constant:** a request-scoped rule with no
node-scoped bound is a rule about one caller in a protocol that has many.

## 8. What is deliberately not decided

* **A close assembled from an opening (U-07c).** §4 names the stall. The refutation of a step
  inside interval `j` needs the operands the interval's opening already carries; making
  `refutation_for_free_prompt_index` take an opening instead of a capture is a court change with
  its own invariants (the challenger's operands must be the executor's, proven against the leg
  roots), and its own ADR.
* **Raising or deriving `PALW_MATERIAL_MAX_BYTES`.** After Decision 2 nothing a seat needs
  approaches it; the whole-capture pull for the court is the only consumer that could want more,
  and U-07c removes that consumer. A cap that follows the largest class is the 2026-08-28 rule
  restated, and it was wrong twice.
* **Arming ADR-0065 Decision 4 on testnet-11 — already the case, and the card's item (b) was a
  misreading.** `palw_rc_shipped_params()` (what `--netsuffix=11` resolves to) runs
  `palw_rc_arm_phase1`, which sets `palw_unavailable_abstains = Some(ForkActivation::always())`
  from genesis; the test `the_rc_ruleset_arms_the_unavailable_abstains_rule_from_genesis` pins it.
  The "None on every shipped preset" reading (card §6m) came from the pin on the RAW preset
  (`MAINNET_PARAMS.palw_unavailable_abstains.is_none()`), which is the assembly's input, not the
  ruleset the fleet runs. So an `Unavailable` quorum already abstains on this chain; nothing here
  to activate, and §6m's correction ("nobody is slashed on the shipped path") stands for a second,
  independent reason.
* **A retention directory flag on kaspad.** The directory is derived from the app dir and reported
  over RPC; a flag would be a second place the answer lives.

## 9. Number hygiene

This is ADR-0084. The README's index records 0080–0083 as resident (0083 on
`palw-daa-bits-priced-rows`); its "next free number" line still said 0080 and is corrected with
this row. A concurrent claimant renumbers the later writer, per ADR-0036 Decision 5.

## Mission alignment amendment — 2026-10-07

capture stays homeはtransport/storage最適化として維持できる。envelopeとbounded openingsだけで外部verifierの局所化・closeが成立し、開示拒否も客観的に裁定できることを必須条件にする。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0084-the-ids-ride-the-capture-stays-home.md -->

</details>

<a id="source-0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm"></a>

<details>
<summary>0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md — 原文 L1–L807</summary>

[原文を開く](../adr/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md) · SHA-256: `95c3c9e1273bf5b1746184130de27ed1194e5fa9f4683a6bf8419bc3367ac2f7`

<!-- ADR_SOURCE_BEGIN:0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md -->
~~~~~~~~markdown
# ADR-0103 — The context is held off the chain, and the chain carries a root, an opening and a logarithm

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-11 on `docs/adr-0103-the-context-is-held-off-the-chain` (from
  `feat/adr-0099-sharded-seat` at `3e9c3b59`); **IMPLEMENTED 2026-09-11 on
  `feat/adr-0103-held-context`** (§10: what each Decision became, the generator's numbers, ten
  corrections to the text below, and what is not done). Nothing is armed: the fence
  (`Params::palw_held_context`) is `None` on every shipped preset and Some-only in the fingerprint,
  and a network that wants what this ADR describes MINTS with it (ADR-0092 Decision 4;
  `palw_held_context_mint_v1`). Testnet-11 does not move. Every number in §1 is one the tree's generators already printed or an ADR already
  recorded, cited by section; every number in §3 that is not is labelled *arithmetic* and is the
  generator's to print before it is normative (ADR-0092 §5).
* Amended 2026-09-12 by [ADR-0118](0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md):
  the regime may also be armed at a HEIGHT on a network minted before it (testnet-11's flag day is
  one line, `PALW_RC_HELD_FENCE_DAA`, and the operator's). V4 and the tiled prompt ids are required
  at the fence only from genesis; past it the fence commits V4 itself and a held class commits its
  own Merkle ids. On such a network the frozen ladder, not the regime, bounds a held class's
  context (ADR-0118 §4).
* Builds on: [0097](0097-a-models-fit-is-a-lookup-and-the-entrance-says-its-limits-before-the-first-token.md)
  (the nine walls and their homes; §1.4: 2M is refused past eight layers; Decision 5's table, whose
  rows this ADR answers one by one), [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md)
  (a seat holds a shard; §1.2's two transfer forms, of which the *resume* form is Decision 2 here),
  [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the one-move court is a consensus object and convicts on a live devnet; licensing per shard),
  [0082](0082-the-close-is-flat-in-the-context.md) (R4; Decision 2's dissection; Decision 3's
  derived arity; Decision 4 as amended — a checkpoint at every position; Decision 7's fold;
  Decision 9's seat; §10.3's quadratic term; §8's two refusals this ADR reverses by name),
  [0092](0092-the-ladder-is-minted-once-and-the-clock-is-what-binds.md) (the ladder is in the
  identity; the clock binds; a wider model is a new mint), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (an opening is the fold's digests and a frontier; the court's address is a block, then a leaf),
  [0098](0098-the-panels-coverage-is-a-number-and-a-seat-that-found-a-lie-files-nothing-else.md)
  (the coverage is a generated number; a seat that found a lie files the one thing),
  [0062](0062-data-availability-court.md) (an accusation names what is missing; a disclosure is
  hash-checked and bounded), [0081](0081-long-context-the-input-is-a-state-chain.md) Decision 3 /
  [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) Decisions 8 and 16
  (the tiled ids; the sampled interval; `PanelDa`), [0093](0093-the-court-can-try-a-fused-row-and-the-responder-is-what-is-missing.md)
  (the responder a fused site still needs), [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
  (a fenced kernel stays out of the identity — the shape every new thing here takes), and the
  close-cut ADR authored as 0102 on `feat/adr-0096-partb-drill` (§9: it renumbers) — a close too
  wide for one carrier is cut once, which is what carries a one-move court's object when it is wide.
* Amends: ADR-0082 R4, Decisions 3 and 9 and §8 (for a class under this fence only — §7);
  ADR-0092 Decision 1 (what a ladder is minted against, once it costs no rounds); ADR-0097
  Decision 3 (the geometry ceiling's form); ADR-0062 Decision 1 (what an accusation may name);
  ADR-0099 §1.2 (the resume form is chosen, for wide classes, by derivation). Supersedes nothing.

## 0. The sentence this ADR is

**A 2M context is not refused by one number. It is refused by six terms that grow with the
context, and every one of them is a term the chain carries, walks or waits on for something the
executor or a seat could hold instead.** ADR-0097 named the walls and ADR-0099 built the seat that
holds a shard of the model; what neither moved is that the *chain's own work* on a claim — the
ladder a court walks, the window a court waits, the chunk count a leg enumerates, the ids a
commitment carries, the product a registration validates, and the prefix a seat recomputes — is
linear in the context. This ADR moves each of them off the chain: a court that opens at a named
leaf and walks no ladder (D1), a seat that resumes from committed state it fetches rather than
recomputing the prompt (D2), a state map whose checkpoint is an append (D3), ids that never ride
(D4), a dissection that starts where the accusation points (D5), and a registration gate that costs
one position (D6). What the chain carries per claim becomes **a root, an opening and a logarithm** —
constant or `O(log C)` in the context `C` — and what grows with `C` is **held**: by the executor for
the claim's life, and by a seat as its shard's state, each bounded by a budget the plan prices and a
window the certification drill enforces (D7). The fit stays a lookup: the generator prints every
wall's *order* beside its number, and a class whose chain wall is linear is refused by name (D8).

The operator's framing (2026-09-11), adopted whole:

> 2M contextそのものが無理というより、「2M contextを今のPALWの証明・裁判・DA・搬送ルールのまま
> チェーンに載せると、帯域と時間の壁にぶつかる」… 巨大な状態・prompt・KV → off-chain / seat / shard
> 側に保持 → chainには commitment / root / 必要なopeningだけ … 2Mでもchain上の負荷はほぼ一定〜
> 対数的に近づける方向が正しいです。

That is the requirement (§2), and the six terms are §1.

## 1. What was measured

Read on `feat/adr-0099-sharded-seat` at `3e9c3b59` on 2026-09-11. Nothing was re-run for this
ADR: every figure is what `palw-model-fit` printed on 2026-09-10
(`docs/history/palw-model-fit-testnet11-2026-09-10.md`, ADR-0097 §1), what ADR-0082 §10 and ADR-0086 §1/§7
measured, or what ADR-0098 §1.2 generated — each cited where it is used. The RC's own constants:
ladder `2^26`, close `2,250,000` bytes / `27` carriers, turn deadline `42` DAA, terminal rounds
`2`, `window_court` `3,000`, `window_receipt` `600`, standard transaction `120,000` bytes
(`PALW_RC_WINDOWS_V1`, `palw_fp_devnet_v3.rs`; ADR-0097 §1.1).

### 1.1 Six terms, and the order each one grows with the context

`C` is the class's `n_ctx`; `L` its layer count; the *order* column is the order of the chain's own
term in `C`, read off the predicate that computes it.

| term | the chain carries / walks / waits on | order in `C` | where it lives | why it is linear |
|---|---|---|---|---|
| the ladder | `max_step_leaf_count ≥` the whole context as prefill, in leaves — `C × leaves_per_position` (103,008 a prefill position on dense graph-v5, ADR-0086 §1) | **linear** in leaves, so `⌈log₂⌉` rounds, each two clocked moves | `PalwCourtParamsV2`, inside `palw_ruleset_id_v2` | the bisection court (`PalwBisectLadderV1`) walks the JOB's leaf space to find one leaf; `2^26` is the widest the RC's window prosecutes at arity 2 with no headroom (ADR-0092 §8) |
| the court window | `(2 × (⌈log₂ ladder⌉ + ⌈log_k(C / 16)⌉) + terminal + 1) × 42 + 216 < 3,000` (`worst_case_duration_with_history_daa`, `palw_court_arity_v1`) | **logarithmic** in `C` on the history term — but the LADDER term is the leaf count's logarithm and the leaf count is linear in `C`, and every round is 84 DAA of wall clock | the lattice windows, the same id | the ladder's rounds are counted before the dissection's (ADR-0082 §10.4: the RC row is 65 moves, 26 of them the ladder's) |
| the state chunks | `L_attn × 2 × ⌈C / 16⌉ ≤ 65,536` (`tiled_kv_state_geometry_v3`) | **linear** — a count bound | `PALW_STEP_LEG_MAX_STATE_CHUNKS`, a code constant the checkpoint leg enforces | the v3 map indexes `(kind·L + layer)·chunks_per_slice + block`, so a slice that grows a block MOVES every later index and a per-position checkpoint re-hashes what did not change: 696,516,608 bytes serialised a job at 512 (ADR-0082 §10.3), a term **quadratic** in `C` |
| the public-DA payload | `C × 4 ≤ 120,000` under `PublicDa` | **linear** | `PALW_STANDARD_TX_BYTES` | the ids ride the commitment transaction (ADR-0077 D16: under `PanelDa` they do not) |
| the geometry ceiling | `C × L ≤ 2^24` (`validate_geometry`, `PALW_STEP_MAX_ENUMERATION`) | **linear** — a product | `palw_step.rs`, a code constant that gates every `ClassRegistered` | it bounds two walks that still visit every position — `step_leaf_count_capped_v1` and `canonical_step_coordinates`, "their driver is the CONTEXT's `declared_prefill_tokens`" — after `worst_case_step_leaf_count_capped_v1` was made a closed form (`palw_step.rs`'s own doc at the constant) |
| the seat (not a wall) | the prefix a seat recomputes before its first drawn interval, `C` positions (ADR-0082 D9); the draw's `N` counts DECODE calls, `max(1, ⌈(D − 1) / interval⌉)` (ADR-0098 §1.1), and interval 0 is the prefill whole (ADR-0086 §1) | **linear** compute per seat per claim | ADR-0082 D9; `base0_fp_interval_count_for_v1`; `PALW_FP_SEAT_INTERVAL_SAMPLES_V1 = 4` | "a seat recomputes the cache from the prompt it holds; it never fetches the history" |

Three terms are already what this ADR wants, and are listed so that nobody re-solves them: the
**close** is flat in the context after ADR-0082 (192 bytes for eight times the context, §10.1; the
Merkle prompt-id term is 64 bytes a doubling, ADR-0082 §1.1) and stays so here; the **licence** is
`3 × shards` receipts, constant in `C` (ADR-0100 D4); a **DA disclosure** is one event and its path
(ADR-0062 D3, SA-2). And one term is not the chain's at all and is left where it is: the executor
**retains** the cache for the claim's life — 276 GiB for the K3 stand-in at 2M (ADR-0097 §1.5) —
which is disk, priced by `claim_retirement`, and is what "held" means.

### 1.2 The same six at 2M, in the tree's own predicates

*Arithmetic from §1.1's formulas over the generator's recorded rows; the generator prints the
normative table when D8 exists.* Dense graph-v5 (28 attention layers), and the K3 stand-in of
ADR-0097 §1.3 (`stand_ins::KIMI_K3_AS_HYBRID_V1`, 24 attention + 68 recurrent layers; 3,323,778,104
leaves at 512 → 6,491,754 a position):

| term | dense graph-v5 at `2^21` | K3 stand-in at `2^21` | against |
|---|---|---|---|
| leaves, the ladder's `need` | `2^21 × 103,008 ≈ 2.16 × 10^11`, 38 binary rounds | `≈ 1.36 × 10^13`, 44 rounds | `2^26`, 26 rounds |
| court window at arity 2 | `(38 + 17) × 2 + 3 = 113` moves → `4,746 + 216 = 4,962` DAA | `125` moves → `5,466` DAA | `2,999` |
| court window at arity 64 (the history in 3 rounds) | `(38 + 3) × 2 + 3 = 85` → `3,786` DAA | `(44 + 3) × 2 + 3 = 97` → `4,290` | `2,999` — **the ladder alone overruns; no arity fits** |
| state chunks, v3 map | `28 × 2 × 2^17 = 7,340,032` | `24 × 2 × 2^17 = 6,291,456` | `65,536` |
| public-DA payload | `8,388,608` B | the same | `120,000` |
| geometry | `58,720,256` (the generator's row) | `192,937,984` (the generator's row) | `16,777,216` |
| the seat's recompute | `2^21 × 0.0938 s` (ADR-0082 D7's un-captured forward, the real dense artifact) `≈ 54.6 h`; at the a16 host's ≈ 1 s a token (ADR-0097 §1.5) `≈ 24 days` | the adder's rate (ADR-0099 D6; a K3 has none measured) | `window_receipt = 600` DAA ≈ 20 h at testnet-11's 120-second cadence (ADR-0082 §1.6) |

Two readings of that table, both true: the ladder and the window are one term (ADR-0092 §3: leaves
are cheap, rounds are not, and the leaf count is what makes the rounds), and the seat is the only
term with no ruleset number in front of it — which is why ADR-0097 Decision 5 put the seat first.
The ceiling's refusal of 2M "past eight layers" (ADR-0097 §1.4) is the first wall met, and the one
that means the least: raise it alone and the next five refuse in the order above.

### 1.3 What the tree already holds for each term

Checked against the tree rather than against the ADRs, in ADR-0092 §2's spirit — an ADR that asks
for what the code already does is the most expensive kind.

| this ADR needs | where it already is |
|---|---|
| a court that convicts at a named leaf with no search | `ShardCourtAccused` (ADR-0100 D1): acceptance arm, fold arm, signing context in the V3 set; **convicts on a live devnet** (ADR-0100 §9, run 2: leaf 0, `void_and_slash`, −38,520 sompi, no human step). Refuses a fused leaf `NeedsDissection` (`palw_shard_court_v1.rs`) |
| a checkpoint at every position, prefill included | ADR-0082 Decision 4 as amended: `PalwCheckpointCadenceV1::PerPosition` on every tiled-map class, read off `state_chunk_map_id` and therefore inside the class id |
| an opening that is the fold's digests, never the leaves | ADR-0086 D1–D3: `Base0FpIntervalOpeningV4`; the seat's leaves are its own; a fault has a block address; the block-leaves lane names the leaf (D6) |
| a resume opening a shard seat can start from, priced | ADR-0099 §1.2 / D2: the two transfer forms, "both committed material, so both verifiable"; the plan prints both and chooses neither |
| the seat's draw as a pure function of chain facts | `palw_fp_interval_draw_v1` (ADR-0077 D8), `k = 4`, over `N` |
| the prompt ids as a tiled root | `palw_prompt_ids_v1`, trace format 4 (`PALW_V2_TRACE_FORMAT_VERSION_MERKLE_IDS`), `Params::palw_prompt_ids_form_v1()` genesis-only; armed on the carded mainnet, testnet-11 keeps `Flat` (ADR-0081's 2026-09-06 note) |
| a commitment that carries no ids | `PanelDa` (ADR-0077 D16), `Params::palw_panel_da`, armed on the card only |
| a DA court whose disclosure is one named unit | ADR-0062 D1–D5 behind `Params::palw_da_court`, armed on testnet-11 (its 2026-09-06 rollout record) |
| a registration gate that costs no `n_ctx` factor | `worst_case_step_leaf_count_capped_v1` — closed form, `O(nodes)`, pinned by `a_sparse_leaf_profile_costs_the_same_at_every_context`; its two siblings are named there as still walking |
| an arity derived from the window and the carrier | `palw_court_arity_v1` (ADR-0092 D3) — with the ladder's rounds inside its divisor |
| a close wider than one carrier, filed by the node | the close-cut ADR on `feat/adr-0096-partb-drill` (authored as 0102, §9 here): the cut moves to consensus-core and the node's court files parts |
| the walls as a lookup, per wall, with `need` / `have` | `palw_model_fit_v1` (ADR-0097 D1) — without an order column |

So most of this ADR is re-plumbing shipped parts so that the chain's term is the logarithm and the
linear term is held. The parts that are new are: a state map whose index does not move (D3), a
seat whose sampling unit covers the prefill and whose starting state is fetched (D2), an output-id
form (D4), a DA accusation that names a tile or a chunk (D4), the fit's order column and its gate
(D8), and one fence.

## 2. The requirement

> **R-held — for a class under this ADR's fence, everything the chain carries, checks or waits on
> per claim is bounded by a constant or by `log C`: the commitment, the licence, each court move
> and the number of moves, a DA disclosure, and the work a validating node does on a registration.
> What grows with `C` is held off the chain — by the executor for the claim's life, and by a seat as
> its shard's state — and each held term is bounded by a budget the plan prices and a window the
> certification drill enforces. The fit stays a lookup: the generator prints every wall's order,
> and a class whose chain wall is linear is refused by name.**

ADR-0082 R4 stands inside it for the court and the executor's commitments; where R4 said a SEAT
fetches nothing that grows with the context, R-held says a seat fetches its shard's state and the
CHAIN carries nothing that grows with the context — the seat's fetch is the one linear term this
ADR keeps, and §3 Decision 2 says why it is the honest one.

```text
   a 2,097,152-token prompt, one claim              what the chain holds for it
   ──────────────────────────────────               ────────────────────────────
   the executor: the cache (276 GiB on a K3),        roots: step, checkpoint, prompt ids,
     retained for the claim's life; a checkpoint       output ids, output — CONSTANT
     root that is an APPEND each position (D3)      licence: 3 × shards receipts — CONSTANT
   a seat: its shard's state at the interval's      a court: one named leaf, its openings,
     start, FETCHED and verified (D2, D7);            ⌈log₂ leaves⌉ × 64 B of path — LOG
     P positions replayed; a receipt                a fused leaf: 1 + 2⌈log_k(C/16)⌉ moves,
   any holder: the ids, served under the root         each one carrier — LOG
     the chain names (D4)                           DA: one tile / chunk / block + path — LOG
                                                    registration: O(nodes) — CONSTANT
```

## 3. Decisions

**Decision 1 — the court opens at a named leaf, and the leaf ladder is walked by nobody.** Under
the fence, a claim's arithmetic is disputed only by an accusation that NAMES what is wrong and
carries what refutes it, adjudicated by the chain in one move. Two kinds: `ShardCourtAccused`
(ADR-0100 D1, unchanged) for a step leaf — the committed inputs opened against the step root, the
artifact openings against the class root, the terminal check (`check_execution_step_refutation_capped_v1`)
run by the chain; and its sibling `CheckpointAccused` for a checkpoint chunk that is not the
composition the class's map defines over the committed rows it covers — the chunk opened against
the checkpoint root, the rows against the step root (the `KCacheWrite` / `VCacheWrite` roles
ADR-0082 §10.2 bound), the composition recomputed by the chain and compared byte for byte. The
bisection court over the whole step space (`CourtOpened` on `PalwBisectLadderV1`) is refused by
name for a class under the fence, so the ladder's ROUNDS leave the clock. `max_step_leaf_count`
stays inside the ruleset id as what it also always was — the depth that prices a Merkle path, 64
bytes a level — and **ADR-0092 Decision 1 is amended for such a network: the ladder is minted at
the top of the CARRIER's budget, not the clock's**, because it costs no rounds; a `2^48` ladder is
3,072 bytes of path in a close and no wall clock anywhere. Who names the leaf is unchanged: a seat
that replayed it (ADR-0098 D2/D3, and the block-leaves lane of ADR-0086 D6 turns a block address
into a leaf off the chain), or any Active bond above the floor that is not the claim's own
(ADR-0062 SA-1); a false accusation costs `min(claim.reserved, floor)` (ADR-0100 D1). A fused leaf
is Decision 5's. What the accused is asked: nothing — the roots are theirs, the class's artifact is
the class's, the arithmetic is the court's — which is why there is no responder and no clock at any
leaf but the fused one.

**Decision 2 — the seat's unit is an interval of positions over the whole job, prefill included,
and a seat resumes from committed state it fetches.** Stated as the derivations they are:

* **The interval.** `N = ⌈(prefill + decode) / P⌉`, where `P` is the class's
  `seat_interval_positions`, DERIVED at certification: the largest power of two for which, on the
  slowest certified seat, `fetch(C) + replay(P positions at the last interval)` fits
  `window_receipt` with the drill's stated margin — ADR-0082 D9's derivation with the fetch term
  added, enforced where seats are measured (ADR-0075 D7's drill; `palw-certify`'s route), recorded
  on the certification beside the replay floor. The draw is `palw_fp_interval_draw_v1` over `N`,
  unchanged; interval 0 is `P` positions and never the whole prefill.
* **The route.** Derived from the class, never chosen by a seat: `Recompute` (today's route, ADR-0082
  D9) where `C × rate_slowest ≤ window_receipt`; `Resume` otherwise. Under `Resume` a seat asks the
  executor — or any holder, ADR-0101's serving rule — for the resume opening of ADR-0099 D2: the
  checkpoint chunks of ITS SHARD's layers at the interval's start under the checkpoint root (a
  recurrent layer's state; an attention layer's K and V tiles over `0..p`), verifies every chunk
  against the root (D3 makes that a shard-local check), replays its `P` positions with its shard's
  kernels from the previous shard's committed rows (ADR-0099 §1.1: a shard's input is another
  shard's committed output, opened against the step root; shard 0 from the ids it holds), and
  compares every committed row of its shard, the checkpoint root at the interval's end and — for
  the interval that selects tokens — the ids. `Valid`, `Incapable`, a fault at an address (ADR-0098
  D3), or nothing, as today. A seat's fetched bytes are `O(C)` — the shard's state at the
  interval's start — and that is the one linear term this ADR keeps, off the chain, bounded by
  the shard budget the plan prices (D7) and by `window_receipt × bandwidth`.
* **What this reverses, said plainly.** ADR-0082 R4 ("nothing a seat fetches grows with the
  context except through a logarithm"), D9 ("it never fetches the history") and §8 ("a fleet mode in
  which seats FETCH the cache — refused as a protocol default") were written for a seat that could
  recompute 131,072 positions in hours. At 2M there is no window that holds a recompute (§1.2:
  54.6 h on the fastest measured dense path against a 20-hour window), so the bounded seat is the
  one that fetches its shard's state; keeping R4's sentence would keep the class off every network
  that can hold it. R-held keeps R4's *substance* — the chain carries nothing linear — and moves the
  linear term to where a budget can bound it.
* **What this is not.** ADR-0081 §3's prefill-segment chain, still withdrawn (ADR-0082 §7). The
  executor's job is one context and one claim; nothing about the model's view, the leaf enumeration
  or the checkpoint leg changes — a tiled-map class already checkpoints every position. What
  changes is the SEAT's sampling unit and where its starting state comes from.
* **Coverage** (ADR-0098 D1) is computed over the new `N` and printed; ADR-0098 D4 stands — the
  deterrent is coverage × `claim.reserved`, and `k` is a mint-time number. Decision 9 prices it.

**Decision 3 — state chunk map v4: the index is append-only, the checkpoint root is a frontier, and
the chunk cap is a depth.** `state_chunk_leaf_hash_v1(map_id, index, bytes)` binds the index, and
the v3 tiled map's index moves when a slice grows (§1.1) — that is the quadratic term, and no
constant raises it away. v4 (`PALW_TILED_KV_STATE_CHUNK_MAP_NAME_V4`, `tiled_kv_state_geometry_v4`)
is two levels: the checkpoint root is a Merkle root over the SLICES' sub-roots (one slice per
`(kind, layer)`, `L_attn × 2` of them, plus a recurrent layer's state slices), and each sub-root is
the frontier of that slice's blocks in position order — the leaf binds `(map_id, slice, block,
bytes)`. A block appended to one slice moves no other index; the open tile (the last, partly filled
block of every slice) is one leaf replaced per position until it closes. Hashing per position is
`O(slices × depth)`, the bytes hashed per job are `O(C × row × slices)` — linear, once — and the
executor retains a frontier per slice beside the cache it already retains (ADR-0082 D4: the cache
is prefix-stable). Three consequences: a shard seat verifies its fetch against its own slices'
sub-roots and `log(slices)` siblings (D2's shard-local check); the court's bottom opening (ADR-0082
D2) is one tile with `log(blocks) + log(slices)` of path, the same order as v3's;
`PALW_STEP_LEG_MAX_STATE_CHUNKS` (a count, `65,536`) is replaced for v4 classes by
`PALW_STEP_LEG_MAX_STATE_DEPTH` (a path bound), the count check in `tiled_kv_state_geometry_v3`
becomes a depth check, and every reader that materialises a chunk list becomes closed-form
addressing over `(slice, block)` — as `worst_case_step_leaf_count_capped_v1` was made for leaves.
A class is its map (`state_chunk_map_id` is in the class id), so v4 is a new row per family —
graph-v7, from graph-v5 for the dense lineage and from ADR-0102's graph-v6 for the hybrid — and no
shipped row moves.

**Decision 4 — the ids never ride above one standard transaction, and a DA accusation names a
tile.** A class under the fence is admitted only in the forms whose chain term is logarithmic:
the prompt ids as the tiled Merkle root (trace format 4; ADR-0081 D3 / ADR-0082 D5 — the fence's
"until its inputs exist" condition is met by the class itself), the OUTPUT ids as the same tiled
root — ADR-0082 U-07b, taken here: `PalwFpOutputIdsFormV1::Merkle`, so the decode pin's flat id
list becomes a root and the refutation of a gather at a generated position opens one tile — and
`PanelDa` (ADR-0077 D16): the commitment carries no ids, `PublicDa` is refused by name where
`C × 4 > PALW_STANDARD_TX_BYTES`. **Public is a serving fact, not a carriage fact:** where a
network wants its prompts public, the ids are SERVED under the root the chain names, by the
executor and by any holder (ADR-0101's rule), and misakascan reads them from a holder as it reads
material today; where it does not, they are the panel's. The DA court generalises ADR-0062 D1's
"name what is missing": `ProducerDefaulted.missing` becomes `PalwMissingV1::{ TraceEvent(index) |
PromptIdsTile(index) | OutputIdsTile(index) | StateChunk { checkpoint, slice, block } |
StepBlock(block) }`; the disclosure (D3 there) is the named unit with its path — a tile of ids is
`tile × 4` bytes, a state chunk at most `PALW_STEP_LEG_MAX_STATE_CHUNK_BYTES`, a block of leaves at
most `4,096 × 64` — each inside one carrier by construction; D2, D4 and D5 of ADR-0062 apply
unchanged, and SA-2's "checked by hash arithmetic, never by execution" is kept exactly.

**Decision 5 — a fused leaf's dissection opens at the named leaf, and the arity is derived with
the ladder term at zero.** `ShardCourtAccused` at an `AttnFused` leaf no longer answers
`NeedsDissection`: under the fence it OPENS the dissection session of ADR-0082 D2 at that leaf — the
accusation is the challenge, the responder's root claim is the first clocked move — so a session's
moves are `1 + 2 × ⌈log_k(p / 16)⌉ + terminal`, and `palw_court_arity_v1` and
`palw_attn_court_admits_row_v1` take `ladder = 0` for a class under the fence (the leaf ladder that
term counted is Decision 1's, and it is not played). The responder is ADR-0093's, unchanged and
still unbuilt (*2026-09-11: built before this branch, `c9be7676`, and drilled on the held rows —
§10.3 item 9*): a family with no `attn_tile_claim` is refused under the fence exactly as
`FusedAttentionNeedsTheKaryCourt` refuses today, and ADR-0093 D2's narrowing of the mercy arm rides
this activation. The carrier half of the pair rule is unchanged (ADR-0082 D3: `k × (4 + 8 + 8 ×
lanes)` inside one carrier at the widest registered tile; at a 128-lane head every legal arity
fits, ADR-0092 §8). *Arithmetic*, at `p = 2^21`, tile 16 (`2^17` tiles), the RC's 42-DAA clock and
216-DAA reserve, terminal 2:

| arity `k` | history rounds `⌈log_k 2^17⌉` | moves | DAA | fits `2,999`? |
|---|---|---|---|---|
| 2 | 17 | 37 | `1,554 + 216 = 1,770` | yes — and it is what the derivation selects (the smallest that fits) |
| 4 | 9 | 21 | `1,098` | yes |
| 16 | 5 | 13 | `762` | yes |
| 64 | 3 | 9 | `594` | yes |

With the ladder's 38–44 rounds gone, the RC's own window holds a dissection at 2M at arity 2; the
generator's table (D8) is the normative one.

**Decision 6 — the registration gate costs one position, and the geometry ceiling is a
per-position budget.** `step_leaf_count_capped_v1` and `canonical_step_coordinates` become closed
forms over `PalwLeafShapeV1`, as `worst_case_step_leaf_count_capped_v1` was made: the enumeration
is position-major with at most three position shapes (a prefill position, the position that selects
a token, a decode call — ADR-0086 §1's 103,008 / 122,024), so a leaf index resolves to
`(position, node_slot)` by one division and a table of at most `PALW_STEP_MAX_NODES_PER_TABLE ×
layer_count` prefix sums, and a job's count is three multiplications. `PALW_STEP_MAX_ENUMERATION`
— a product with `n_ctx` whose whole stated reason was those walks — is replaced for a class under
the fence by two bounds with no `C` in them: `layer_count ≤ PALW_STEP_MAX_LAYERS` and
`nodes_per_position ≤ PALW_STEP_MAX_NODES_PER_POSITION`; `n_ctx` is bounded by the ladder's depth
(leaves `≤ 2^depth`) and by its own `u32`. It gates `ClassRegistered`, so it is a ruleset move in
ADR-0097 D3's shape and lives under the fence. `a_sparse_leaf_profile_costs_the_same_at_every_context`
is extended to both siblings and to the gate as a whole (§5, invariant 5).

**Decision 7 — a seat holds a shard of the model and of its state, and the plan prices the
fetch.** ADR-0099 D2's plan gains the per-interval fetch as a column: for shard `i` at position `p`,
`Σ_{attention layers in i} 2 × p × kv_row + Σ_{recurrent layers in i} state` — worst at the last
interval, `O(C)` — and `palw_shard_plan_for_seat_v1` takes a bandwidth and the window beside the
seat budget, so "the fewest shards a seat can hold" becomes "the fewest shards a seat can hold AND
resume inside `window_receipt`". The certification drill certifies a width only when the slowest
certified seat resumes its widest interval inside the window (ADR-0075 D7; ADR-0099 U-02/U-03 are
its units), and the number is on the certification. Shard 0 resumes from the ids and fetches
nothing (ADR-0099 §1.2), which is the argument for placing the embedding and the first layers on the
widest seat. `Incapable` stays the honest verdict for a seat that cannot (ADR-0082 D9).

**Decision 8 — every wall prints its order, and R-held is a predicate.** `palw_model_fit_v1`
gains `order: Constant | Logarithmic | Linear | Unpriced` on every row, classified from `need` at
`C`, `2C` and `4C` by the second difference — an unchanged `need` is constant, a constant first
difference is logarithmic, a doubling one is linear — and prints beside the chain walls the HELD
terms (the executor's retention, the seat's fetch, the seat's replay) with their orders and the
budget each is checked against, so "constant on the chain, linear where it is held" is a printed
table and not a sentence. `verify_class_admission_v8` (v7 with this fence) refuses a class under
the fence whose chain wall is `Linear` — `LinearInTheContext { wall }` — before any number is
compared. §1.1's table is pinned as the limitations it lists by
`consensus/core/tests/palw_adr0103_held_context.rs`, ADR-0097's discipline: the day one moves, a
test says so.

**Decision 9 — what earns and what is at stake do not change, and the deterrent at 2M is priced
before it is armed.** ADR-0074 D5 (a quantum is leaves), ADR-0082 D10 (prefill priced at zero on
the free-prompt lane; decode leaves earn) and ADR-0080 D3's invariance are untouched: a claim's
reservation is its decode leaves' whatever its prompt, so nothing here raises a bond with the
context. ADR-0098 D4's deterrent — coverage × `claim.reserved` — is therefore the number a network
under this fence reads at mint: with `N ≈ C / P`, the row-replay coverage `1 − ((N − k) / N)^s`
falls as `s·k / N`, so a fixed `k` buys a vanishing catch on a 2M prompt; either `k` or `s` scales
with `N` (`s × k × P` is the panel's whole replay, linear in `C` and spread over seats, each bounded
by its window), or the bonded watchdog (`--palw-challenge`, certain at one inference a claim) is the
deterrent and its cost at 2M is that inference's. The generator prints ADR-0098 D1's inverse table
at 2M; this ADR chooses no number.

## 4. What this costs, stated before it is measured

* **Chain, per claim under the fence:** the commitment — roots, constant; the licence — `3 ×
  shards` receipts, constant in `C`; a court — one carrier-bounded object (cut once when wider,
  the close-cut ADR) with `⌈log₂ leaves⌉ × 64` bytes of path, or a dissection of
  `⌈log_k(C / 16)⌉` rounds each inside one carrier; DA — one tile, chunk or block with its path.
  Per registration, `O(nodes)`. Nothing per claim grows faster than a logarithm.
* **Executor:** the cache for the claim's life (`claim_retirement`, 3,000 DAA on the RC), linear
  and held — 276 GiB for the K3 stand-in at 2M, 112 GiB for the dense 1.5B (ADR-0097 §1.5);
  hashing `O(slices × depth)` a position; serving resume openings whose size is the shard's state,
  bounded by the shard and not by the job — the interval lane's caps
  (`PALW_INTERVAL_OPENING_MAX_BYTES = 4 MiB`, `PALW_MATERIAL_MAX_BYTES = 16 MiB`) do not hold a
  resume opening and a class under the fence declares its own, read off the plan.
* **Seat:** the fetch plus `P` positions, inside `window_receipt` by construction (D2, D7);
  `Incapable` where it cannot.
* **Identity:** one fence; per family a new row (graph-v7: the v4 map, the Merkle output ids,
  `PanelDa`); the ladder minted as a depth; trace format 4; the DA accusation's new variant and
  `CheckpointAccused` in the committed context set the shard court's V3 precedent already opened.
  A new mint carries all of it (ADR-0092 D4); testnet-11's and devnet's fingerprints are
  byte-identical with the fence declared.

## 5. Invariants the tests must hold

```
1  Under the fence no CourtOpened bisection is accepted for the class; every conviction on it is
   a one-move object or a dissection opened at a named leaf; ADR-0100's devnet drill re-run under
   the fence convicts the same tampered leaf with the same slash.
2  A close's bytes and a DA disclosure's bytes at C = 512, 32,768 and 2^21 differ by at most
   64 bytes per doubling of C (the path term) — swept by the generator under the fence.
3  The court window's need under the fence is (1 + 2·⌈log_k(C/16)⌉ + terminal) × deadline +
   reserve with the arity derived at ladder = 0; at zero history it is terminal × deadline +
   reserve; the RC's window admits the dense row's dissection at 2^21 at the derived arity.
4  A v4 map's checkpoint at p + 1 is the checkpoint at p with one leaf replaced per slice or one
   block appended per slice: hashes per checkpoint ≤ 2 × slices × depth, no index moves, and the
   bytes serialised per job at 512 are ≤ positions × slices × row × 2 (the §10.3 quadratic term
   is gone, asserted as an order against the derived bound).
5  The registration gate visits the same number of nodes at C = 2 and at C = 2^21, on
   step_leaf_count_capped_v1, canonical_step_coordinates and the gate as a whole; a
   ClassRegistered at 2^21 × 93 layers validates in the time one at 512 does.
6  The seat's draw covers the prefill: on a job of (prefill, decode) the interval count is
   ⌈(prefill + decode)/P⌉ and interval 0 is P positions; a seat that resumes from fetched chunks
   and a seat that recomputes reach the same receipt on honest material and the same fault
   address on tampered material; a fetched chunk that does not verify against its slice's
   sub-root is refused by name and never replayed from.
7  The ids never ride: a commitment of a class under the fence carries no ids; a DA accusation
   naming a tile, a chunk or a block is answered by that unit and its path, inside one carrier;
   a PublicDa registration whose flat ids exceed the standard transaction is refused by name.
8  Every wall's order is printed and pinned: §1.1's orders as limitations on the shipped
   presets, and under the fence no chain wall of an admitted class reads Linear.
9  Economics unchanged: quanta, pwu and ticket inputs are identical for the same decode leaves
   at every prompt width (ADR-0082 Z7, restated under the fence); no bond grows with C.
10 The fence is None on every shipped preset, Some-only in the fingerprint, the schedule id and
   the fork id; it refuses arming without palw_kary_court, palw_shard_court and trace format 4
   at or below its height; both shipped fingerprints are byte-identical with it declared.
```

## 6. Order of work

Nothing below is built before the measurement above it exists; steps 1 and 2 move no fingerprint.

1. **Decision 8 first, because it measures everything after it.** The order column, the held
   terms' rows, and §1.1 pinned as limitations. Consensus-inert.
2. **Decision 6's closed forms as a refactor** — identical results, pinned at every context by the
   cost test — with the ceiling's replacement behind the fence. The refactor is inert; the
   ceiling's move is not.
3. **Decision 3's map v4 and Decision 2's seat**, node-side and drillable on the loopback devnet
   with the fold classes: the interval count over positions, the resume opening served and
   verified, the receipt; then the tampered-chunk drill (invariant 6).
4. **Decisions 1 and 5 behind the fence** — `CheckpointAccused`, the bisection refused, the
   accusation at a fused leaf opening the session — on the floor class first (ADR-0093 §6's order:
   its arithmetic is integer end to end), because the fused half waits on the responder.
5. **Decision 4** — the output-id form, `PanelDa` required above the standard transaction, the DA
   accusation's `missing` kinds and their disclosures.
6. **Decision 7's drill:** a devnet class registered under the fence at the widest width this
   Mac can seat (the geometry ceiling is gone; the seat's window is what binds — the drill says
   which width), a claim, a stratified panel resuming from fetched state, a conviction at a named
   prefill leaf.
7. **The mint** (ADR-0092 D4): a K3-class network prices its card with ADR-0097's generator under
   the fence, ADR-0099's plan with D7's column, and ADR-0098's inverse table for Decision 9.

**Done when** a class registered under the fence at a context the shipped geometry ceiling refuses
is admitted with every chain wall printed `Constant` or `Logarithmic`; a claim on it is licensed by
seats that fetched their shard's state and replayed `P` positions each; a deliberately corrupted
prefill leaf on it is convicted by one object; and the same job's quanta equal what the same decode
leaves earn at 512.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0082 R4 ("nothing a court carries, a seat fetches, or an executor commits grows with the context except through a logarithm") | kept for the court and the executor's commitments; **amended for the seat** under this fence: a seat fetches its shard's state (linear, held, budgeted) and the CHAIN carries nothing linear (R-held) |
| ADR-0082 Decision 9 (the seat recomputes the cache from the prompt; never fetches the history) | kept as the `Recompute` route where the window holds it; the `Resume` route is derived beside it (D2) |
| ADR-0082 §8 ("a fleet mode in which seats FETCH the cache — refused as a protocol default") | **reversed for a class under the fence**, with §1.2's reason: no window holds a 2M recompute |
| ADR-0082 Decision 3 (the arity derived with the ladder's rounds in the divisor) | the derivation is kept; the ladder term is zero under the fence because the ladder is not played (D5) |
| ADR-0082 Decision 4 as amended (a checkpoint at every position; the v3 tiled map) | the cadence is kept; the map's index scheme is replaced by v4's append-only one (D3), which §10.3 named as the only thing that could remove the quadratic term |
| ADR-0082 U-07b (the output ids as a tiled Merkle root, not decided) | taken (D4) |
| ADR-0092 Decision 1 (the ladder is minted at the top of the wall-clock budget) | amended: a ladder that costs no rounds is minted at the top of the carrier's budget (D1); Decisions 2–4 kept whole — this ADR is the new mint D4 describes |
| ADR-0097 Decision 3 (the geometry ceiling is a wall with a name) | kept on the shipped rulesets; replaced under the fence by a per-position budget (D6) |
| ADR-0097 Decision 5 (what a K3-class network must mint with, wall by wall) | each row answered: the seat (D2, D7), the geometry (D6), the ladder (D1), the window (D5), the close (unchanged; ADR-0093 still owed), the chunks (D3), the public-DA payload (D4), the answer (ADR-0096 D5, unchanged) |
| ADR-0097 §1.4 ("2M is refused past eight layers") | stands on every shipped ruleset; a class under the fence meets no such wall |
| ADR-0099 §1.2 / Decision 2 (two transfer forms, priced, neither chosen) | the resume form is chosen by derivation for a class whose recompute exceeds the window (D2); the plan gains the fetch column (D7) |
| ADR-0100 Decision 1 (the one-move court, for a seat holding a shard) | becomes THE court for a class under the fence (D1), and opens a dissection at a fused leaf (D5) |
| ADR-0098 Decisions 1 and 4 (the coverage is generated; the deterrent is coverage × reserved) | kept; computed over an `N` that now counts prefill intervals (D2), and priced at 2M before a mint (D9) |
| ADR-0062 Decision 1 (an accusation names a trace event) | generalised to a tile, a chunk or a block (D4); D2–D5 and SA-2 unchanged |
| ADR-0077 Decision 8 (the seat verifies one interval), Decision 16 (`PanelDa`) | the interval now counts positions, prefill included; `PanelDa` is required above the standard transaction |
| ADR-0081 §3 (the prompt as a chain of prefill segments) | **still withdrawn** — D2 changes the seat's unit, not the executor's job; nothing here is a segment |
| ADR-0093 (the responder) | unchanged and still the prerequisite of D5; a family without it is refused under the fence as today |

## 8. What is deliberately not decided

* **Any number.** `P`, `k`, `s`, the ladder's depth, the fence's height — each is the generator's
  or the drill's (ADR-0092 §5), and the card's author reads the table.
* **KV continuation across jobs** (ADR-0077 §8, ADR-0082 §8, ADR-0096 D5, ADR-0097 D6). Still
  refused. Named here because D2 moves it: once a seat resumes from a committed checkpoint it
  fetched, a *licensed* claim's final checkpoint is the same kind of material as any interval's
  start, and what remains against a continuation job is ADR-0072's unit (one inference, one ticket)
  and the licence dependency, not the seat. A 2M conversation that re-prefills 2M tokens a turn is
  the executor's cost, not the chain's; the successor that prices a continuation is its own ADR.
* **Who pays for a seat's fetch bandwidth**, and the interval lane's cap for a resume opening: a
  plan quantity, declared by the class, not a protocol number.
* **The responder** (ADR-0093): owed as before; D5 makes it the only thing between a fused class
  and a 2M dissection inside the RC's own window.
* **ADR-0100 D5's order** (paid seats and the fuzz gate before a sharded class bears weight):
  inherited unchanged by a class under this fence.
* **The streaming inventory** (ADR-0102 §8): the prerequisite for measuring a K3-class artifact at
  all, and not this ADR's.
* **Whether the per-slice sub-roots should be per shard** rather than per `(kind, layer)`: a
  shard is a plan quantity and the map is a class quantity, and binding the map to a plan would
  make a re-plan a new class. Per slice, with the plan's shards as contiguous slice ranges, is the
  form D3 takes; a measurement of the seat's verification cost at 2M could move it.

## 9. Number hygiene

0103 is the next free number after ADR-0102 (whose §9 says so; the README row agrees). Claimed on
`docs/adr-0103-the-context-is-held-off-the-chain`, branched from `feat/adr-0099-sharded-seat` at
`3e9c3b59`, on 2026-09-11. **The next free number is 0104.**

**0102 was claimed twice, and the rule decides it.** `feat/adr-0099-sharded-seat` carries
[0102 — the embedding lift is read per token](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
(authored 2026-09-10 per its own §9, committed `3e9c3b59` at 04:41 JST on 2026-09-11 after the
scratchpad wipe it records); `feat/adr-0096-partb-drill` carries a second 0102, *a close too wide
for one carrier is cut once, and every filer shares the cut* (authored and committed `90ec2317` at
05:02 JST on 2026-09-11), whose own §8 says "a concurrent claimant of 0102 renumbers the later
writer". The later writer is the close-cut ADR, by both dates; it takes 0104 when it lands beside
this file, and every citation of it here says "the close-cut ADR" rather than a number, so nothing
dangles when it does. Neither branch is edited by this one.

## 10. Implementation record (2026-09-11)

Built on `feat/adr-0103-held-context`, branched from `252b7b3b` (ADR-0093's responder, `c9be7676`,
already in it), commits `c204bf0f` … the branch head. **The fence is `None` on every shipped
preset, and both shipped fingerprints are byte-identical** (`shipped_presets_have_pinned_fingerprints`
green); testnet-11 does not move. A network that wants this ADR mints with
`palw_held_context_mint_v1` — the one spelling the generator, the tests and `kaspad
--palw-held-context-devnet` share.

### 10.1 What each Decision became

| Decision | where it lives | what pins it |
|---|---|---|
| **1** the one-move court is the court | `palw_checkpoint_court_v1.rs` (`CheckpointAccused`, tag 43 — after ADR-0093 D8's `CourtAttnRootClaimedAnchored` at 42 where the branches met — a COMPLETE_V4 context: the chunk opened against the checkpoint root, the cache-write rows against the step root, the composition recomputed); `CourtOpened` refused by name once the fence is armed (acceptance and fold), and the panel's challenger half opens none; a ladder past the bisection's clock admitted only over COMPLETE_V4 (`PalwConsensusParamsV2::validate`) and only with the fence armed from genesis (`Params::validate_palw_v2`); the accusation's prompt carriage (`PalwShardCourtAccusationV1::prompt_ids_opening`, built by `palw_refutation_prompt_carriage_v1` and read by the opened adjudicator — §10.3 item 11) | the checkpoint court's tests on v3 and v4; the regime's fold tests; `a_ladder_past_the_clock_needs_the_regime_from_genesis`; `a_refutation_rides_the_list_on_a_flat_network_and_one_tile_on_a_merkle_one`; `under_the_merkle_prompt_form_the_drill_convicts_and_the_seat_recomputes_the_jobs_ids` |
| **2** the seat's unit is positions, and a seat resumes | `palw_held_context_v1.rs` (intervals, route, `P`, fetch); base0's interval geometry counts in STEPS (`Base0FpIntervalUnitV1::Positions`, one replay loop over windows); the Resume route (`Base0FpResumeOpeningV1` under a bit-30 request on the interval lane, `base0_fp_verify_resume_v1`'s slice-local check, `base0_fp_accept_resume_v1`); seam verbs `fp_held_route_v1` / `open_fp_resume_v1` / `fp_accept_resume_v1`; the panel derives the route from the class and `window_receipt` and asks for the state beside the interval | `every_held_graph_v7_interval_opens_and_a_recomputing_seat_licenses_it`, `a_held_interval_that_resumes_inside_the_prompt_opens_and_is_licensed`, `a_seat_that_resumes_reaches_the_verdict_a_seat_that_recomputes_does`, `a_held_class_seats_its_prompt_in_intervals_and_a_lie_in_it_is_a_fault` |
| **3** map v4 | `palw_step_leg.rs` (leaf `(map, slice, block)`, promote-odd slice trees, the frontier, the depth cap 48); `palw_state_chunk_map.rs` (layouts, one dispatch for leaves, roots, paths, top leaves); graph-v7 rows (`qwen25_a16_profile_v7`, `qwen36_profile_v7`); the producer's per-position capture folds a held checkpoint as an APPEND (per-slice frontiers — one tile a slice and `depth` nodes a position) | `the_held_fold_is_an_append_and_its_root_is_the_courts`, `the_held_composition_folds_and_its_root_is_the_courts`; both fused dissection drills on graph-v7 (dense and hybrid) |
| **4** the ids never ride | Merkle prompt ids and `PanelDa` required at or below the fence (assembly); the payload wall under the fence; a PublicDa carrier past one standard transaction skipped by name (`palw_fp_objects_from_accepted_txs_under_held_v3`); the held DA court (`palw_held_da_v1.rs`, tags 44/45, the `held_da_missing` collection behind tail 0xA3) | `under_the_held_regime_public_ids_past_one_transaction_are_skipped_by_name`, the held DA court's fold tests |
| **5** a fused leaf's dissection opens at the leaf | `open_at_named_leaf`, `worst_case_duration_held_daa`, `palw_court_arity_held_v1`, `palw_attn_court_admits_row_held_v1`, `palw_court_params_held_at_v2` | the fold tests; invariant 3 in `palw_adr0103_held_context.rs` |
| **6** one position | closed-form coordinates and index; the per-position budget in `validate_geometry`/`validate_shape` for a held map | `the_registration_gate_visits_the_same_nodes_at_every_context` and D6's own tests |
| **7** the plan prices the fetch | `PalwShardV1::fetch_bytes_at_v1`, `palw_shard_resume_ms_v1`, `palw_shard_plan_for_seat_within_window_v1`; `palw-shard-plan` prints the column | `the_shard_plan_prices_the_fetch_and_the_window_binds_the_shard_count` |
| **8** every wall's order | `palw_model_fit_v1.rs` (`PalwFitOrderV1`, the held and held-network regimes, the held terms); `verify_class_admission_v8` (`HeldMapNeedsItsFence`, `LinearInTheContext`, `ChainWallOrderUnknown`); `palw-model-fit --preset held` (`docs/palw-model-fit-held-2026-09-11.md`) | `consensus/core/tests/palw_adr0103_held_context.rs` |
| **9** priced before armed | `palw-seat-coverage` §7 | `the_held_mint_moves_no_economics` |

### 10.2 The normative numbers (the generator's, replacing §1.2's and §3's arithmetic)

`palw-model-fit --preset held` — testnet-11's lattice minted with the fence at a `2^48` ladder:

| | dense graph-v7 at 512 | at 32,768 | at 2^21 |
|---|---|---|---|
| per-position budget (nodes) | 677, constant | 677 | 677 |
| ladder as a depth | 26 levels, logarithmic | 32 | 38 |
| close | 87,743 B, logarithmic | 88,127 | 88,511 — 64 bytes a doubling |
| window | 13 moves → 762 DAA | 25 → 1,266 | **37 moves × 42 + 216 = 1,770 DAA at arity 2** |
| state proof | 12 levels | 18 | 24 |
| ids on the commitment | 0 (PanelDa) | 0 | 0 |
| held: retention / fetch / replay | 28 MiB / — / 512 | 1.8 GiB / — / 2,048 | 112 GiB / 112 GiB (Resume) / 2,048 positions |

Every chain wall of the dense, the hybrid and the K3 stand-in's graph-v7 rows reads constant or
logarithmic from 512 to 2M, and every one admits; the K3 stand-in's ladder binds first, at
43,821,980 positions. Decision 5's table is confirmed at arity 2. D7: at a 512 GiB seat the dense
row resumes its last 2M interval on one shard at 100 Mbit/s and above (2.7 h of the window's 10);
the K3 stand-in needs six. D9: the shipped `k = 4` catches a one-token lie in a 2M prompt with
1.94 % at five seats on the dense row (`N` = 1,025) and 0.03 % on the K3 stand-in (`N` = 65,568,
`P` = 32); 90 % needs `k` = 379 or `s` = 589 on the dense row.

### 10.3 Corrections to this ADR, found while building it

1. **§1.1 called the close already flat. It is not, on either shipped preset**: the order column
   reads it `Linear` — the prompt ids are flat there, and the generated-token pin is priced at the
   whole context (`decode_pin_price_v1(profile, n_ctx)`). Under the fence the pin is priced at the
   trace cap (`PalwCourtCostShapeV1::decode_bound`), because one job decodes at most
   `PALW_V2_MAX_TRACE_EVENTS` calls; which is also why Decision 4's Merkle OUTPUT-id form
   (ADR-0082 U-07b) is not needed for R-held — the output-id term is bounded by the cap — and it is
   not built.
2. **Decision 8's classifier read `C`, `2C` and `4C` by the second difference.** Three points
   cannot tell a logarithm with a ceiling (a dissection at arity 64 gains one round in six
   doublings: first differences `0, 1`) from a line. It reads seven (`C … 2^6·C`) and calls a need
   linear when its last growth is at least eight times its first nonzero growth.
3. **Decision 1's ladder is a NETWORK number and the regime was written per class.** A ladder past
   the bisection's clock makes the bisection unplayable for every class, so `CourtOpened` is refused
   for every claim once the fence is armed, every class's window is read on the held clock, such a
   ladder is admitted only with the fence from genesis, and a class that registers no held map on a
   held network keeps its shipped walls (`PalwFitRegimeV1::HeldNetwork`) — never refused for an
   order.
4. **Decision 4's "PublicDa is refused by name where `C × 4 > PALW_STANDARD_TX_BYTES`"** became: the
   fence requires `PanelDa` at or below its height; a held class judged without it reads its
   payload wall `Linear` and is refused `LinearInTheContext` at every width (invariant 7's first
   clause and invariant 8 need the stricter reading); and at the commitment, a PublicDa carrier
   past one standard transaction is skipped by name.
5. **Decision 2's `P` "derived at certification"** would have had to be a consensus object before
   the executor could open by it and a seat draw over it. `P` is a class function
   (`palw_held_interval_positions_v1`: the widest power of two whose opening fits the interval
   lane's cap and which cuts the context into at least the draw's `k` intervals), and the CLOCK is
   the certification drill's check (`palw_held_seat_interval_positions_v1`,
   `palw_shard_plan_for_seat_within_window_v1`).
6. **The seventh wall.** The advertised free-prompt cap was bounded by the IPC frame's 4,096 ids
   (`PALW_V2_MAX_PROMPT_TOKENS`), which made a 2M prompt inexpressible whatever the six terms said.
   A held mint advertises the regime's own (`PALW_FP_HELD_MAX_PROMPT_TOKENS_V1` = `2^26`), admitted
   only over COMPLETE_V4 with the fence from genesis. The worker's request frame
   (`PALW_V2_MAX_FRAME_BYTES`, 256 KiB — about 60,000 ids) is a node-local transport bound and is
   not raised here.
7. **A soundness gap this regime exposed:** nothing tied a checkpoint chunk to the committed
   cache-write rows it summarises. Under the fence a dissection's bottom and a seat's resume both
   stand on checkpoints, so `CheckpointAccused` is the object that makes a false checkpoint
   convictable, not a convenience beside the shard court.
8. **The data-availability court is not required at the fence's height.** The held DA court arms
   with `palw_da_court` (it reuses ADR-0062's `DefaultDisputed` phase with a sentinel index); a
   held network without it withholds into ADR-0077 SA-5's void.
9. **Decision 5 said the responder was "unchanged and still unbuilt".** ADR-0093's responder was
   built before this branch (`c9be7676`); both fused dissection drills run on the held rows here —
   exact on real committed rows, and a forged row convicted at its bottom.
10. **Decision 9's premise** — "a claim's reservation is its decode leaves' whatever its prompt" — is
    ADR-0082 Decision 10's numerator, which `validate_palw_v2` refuses to arm on any network today
    (audit D M-1). The mint changes no economics (`the_held_mint_moves_no_economics`); a network
    that wants Decision 9's sentence at 2M arms Decision 10 with it, which is not this ADR's move.
11. **Decision 1 said the one-move court runs `check_execution_step_refutation_capped_v1`
    "unchanged". Under trace format 4 it cannot convict a free-prompt claim at all** — found by the
    live drill, not by a test. That entry point compares a refutation's carried id list against a
    FLAT `prompt_token_ids_hash`; a held network commits the ids as the tiled root (Decision 4), so
    every refutation that carried the list read `InputSetNotCanonical` — no verdict — and a
    refutation that carried none could not adjudicate a gather. ADR-0081 had built the opened
    adjudicator and deferred the carriage field "to the genesis cut that arms
    `palw_prompt_ids_merkle`"; the held mint is that cut. Built: the accusation carries the gather's
    one tile (`prompt_ids_opening`, outside the session id — it is bound by the job's root; a new
    field on tag 38, whose fence is `None` on every preset and which no chain carries), the verdict
    reads it through `check_execution_step_refutation_opened_capped_v1`, the ceiling prices it, and
    one helper decides the carriage for the one-move accusation and the bisection close's
    `ArithmeticOpened` arm alike. So the close term stays the path Decision 1 promised — one tile
    and `⌈log₂ tiles⌉` siblings — rather than the whole prompt.
12. **Three readers still hashed the prompt flat on a Merkle network**, each silently: every
    family's `operand_openings_for` ran the flat check to learn which artifact rows the court
    resolves, so it recorded none for a gather (now `check_execution_step_refutation_carried_capped_v1`,
    the check the chain will run); the seat's prefix recompute (`base0_fp_recompute_state_*`,
    `base0_fp_seat_state_memoized_v1`) refused an honest job's own ids, so every interval past the
    first filed `Incapable` (it now takes the network's form); and the seat's capture sampler graded
    the flat carriage, so each of its 32 draws was "not a sample" at trace level and the seat filed
    nothing. The live drill showed all three at once; `under_the_merkle_prompt_form_…` pins them.

### 10.4 What was run

The consensus-core suite, the base0 library suite, kaspad's PALW tests and the SDK's — green; the
ADR-0103 integration tests (invariants 2, 3, 5, 8, 9 and 10 at 2M on the held mint, §1.1 pinned
as the shipped presets' limitations); invariant 4 on the producer's capture; invariant 6 at the
seat (resume against recompute, a tampered fetched chunk refused by its slice, a shard's slices
verified alone); invariant 7 at the carrier and the held DA court; invariant 1 in the fold and the
acceptance arm, and on a live devnet by `HELD=1 scripts/drills/misaka-palw-shard-court-devnet-drill.sh`:

* **Run 1 (2026-09-11 11:58 JST) found §10.3 items 11 and 12.** Three devnet nodes, the floor class
  only, `--palw-held-context-devnet` on every node; node-0 committed canonical claim `aeff845e…`
  (tx `ed3f166a…`) with its capture corrupted at step leaf 0 at 12:05:06, and block `8bdbb377…`
  carried its `FreePromptCommitted`. At 12:13:26 both seats drew intervals `[0, 1, 2]`: one found
  interval 0 does not replay in leaves `[0, +4096)` and fetched the block, the other filed
  `Incapable` — "the served ids do not hash to the job's prompt_token_ids_hash" — and neither
  accused: every sample of the tampered capture read no verdict under the Merkle root. Stopped at
  12:26 with the drill still at step 2 of 5.
* **Run 2 (12:57 JST), PASS — invariant 1 on a live devnet.** The same drill on the fixed build
  (`c593e78a`). Node-0 committed canonical claim `20995717…` (tx `84b525d8…`) at 13:04:58; at
  13:08:57 both seats' first duty round found leaf 0 of the served capture does not recompute and
  filed `ShardCourtAccused` carrying the prompt's tile (sessions `fa17158f…`, `d0c24a1e…`);
  13:09:37 block `50b97e87…` carried one accusation and every node folded it; the claim reads
  `voided court_fraud` on node-1 and node-2 over RPC; bond 0's collateral fell from 1,110,106,160
  to 1,110,067,640 sompi, −38,520 — to the sompi the slash of ADR-0100's run 2 without the fence,
  which is invariant 1's "the same tampered leaf, the same one-move conviction, the same slash".
  No node built a `CourtOpened`, no seat filed `Incapable`, and no sampler refusal was logged.

### 10.5 What is not done

* **No 2M claim ran.** The dense row's cache at 2M is 112 GiB and its recompute 54.6 hours
  (§1.2); this Mac seats neither. The "done when" is met at devnet widths — the held graph-v7
  rows' intervals, resumes and convictions in the tests, the one-move drill under the fence — and
  at 2M in the generators' tables.
* **A seat that names a leaf without holding the capture cannot yet accuse it.** Decision 1 took
  ADR-0100's premise that "its interval opening IS the refutation's inputs". For BASE-0's openings
  it is not: an opening carries the interval's committed leaves as hashes (block roots where the
  capture is retained sparse) and the block-leaves lane carries leaf hashes, so a seat can name the
  leaf off the chain (ADR-0086 D6) but holds none of the committed tiles a refutation reveals. The executor serves those tiles only as a close annex, and only for a leaf an open
  session names (ADR-0085 §6 item 4) — and under the fence no session opens. The held DA court
  cannot compel them either: its units are ids, chunks and leaf-hash blocks, not a step tile. At
  devnet widths every seat holds the capture and accuses directly (§10.4, run 2); at the widths this
  ADR is for, a liar that commits a wrong leaf is named but not convicted. Two moves close it, and
  choosing is a protocol decision: (a) the executor serves a named leaf's annex on a panel seat's
  signed request — the one-move court's stand-in for the session that used to authorise it (the
  interval lane's `u32` index cannot address a leaf, so it is a new request) — which an honest
  executor answers and a liar simply does not; so (b) the held DA court gains a unit for a leaf's
  committed evidence (its output tile and input rows, each with its opening against the step root,
  bounded by the court's close ceiling), whose withholding defaults the executor exactly as a
  missing tile of ids does. (b) is the one that makes silence convictable, and (a) is its
  off-chain fast path; recommended together. **Closed by [ADR-0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)**,
  which built both — the leaf request rides the interval lane after all, under bit 29 with the leaf
  in a signed field — and drilled each to a slash with no seat holding the capture (ADR-0111 §8.3).
* **A resume opening past the interval lane's 4 MiB** (a 2M shard's state is gigabytes): §4 and §8
  already say the class declares its own lane cap off the plan; no such lane is built.
* **A shard seat's replay from its own slices.** The fetch is verified slice-locally; replaying one
  shard from that state is ADR-0099's shard replay and is not rebuilt here.
* **The worker's request frame** for prompts past ~60,000 ids (§10.3 item 6).

### 10.6 Decision 2's price, amended (2026-09-11): a replay pays for its history

Decision 2 priced a seat's replay as a line: `n_ctx × replay_ms_per_position` decided the route,
and `budget / replay_ms_per_position` bounded the width's clock. But a position's attention reads
every row before it. ADR-0110 §9.3 measured the consequence on the context vectors' thin row: from
4,096 to 32,768 positions (8×) the seat's recompute grew 41×. The route was therefore optimistic
exactly where it matters. It sent seats to recompute contexts they could not recompute inside the
window, and a seat that overruns files nothing.

**The amendment** (`consensus/core/src/palw_held_context_v1.rs`, consensus-inert like the rest of
the regime's pure half):

* **The knee.** `palw_held_attention_knee_v1(profile)` is the class's own multiply-accumulates for one
  position over its attention's for one row of history, read off the registered geometry. The
  numerator counts the attention layers' `q`, `k`, `v`, `o` projections and every layer's MLP. The
  denominator is `2 × heads × head_dim` per attention layer. The unembedding is left out of the
  numerator because a recompute drops the logits, and leaving work out can only make the knee
  smaller, the side a bound must err on. The dense tier's real row (Qwen2.5-1.5B) has a knee of
  15,232 rows. The context vectors' thin row has 28. A class with no attention layer has
  `u64::MAX`, and its replay is the old line to the millisecond.
* **The price.** `PalwHeldReplayCostV1 { ms_per_position, knee_positions }`. The rate is still the
  family's measured row (SA-4's source), taken as the cost with an empty history, which is how it was
  measured. `replay_ms_v1(first, count)` prices positions `first .. first + count`, each against every
  row before it: `count + (count·first + count(count − 1)/2) / knee` positions' worth, rounded up
  once and saturating.
* **The route** is `Recompute` exactly when `replay_ms_v1(0, n_ctx)` fits the seat's budget.
* **The width's clock** is the LAST interval's replay, `replay_ms_v1(n_ctx − P, P)`, plus the fetch.
  `P` is halved from the wire's and the context's bound until that fits. The class's chain-facing `P`
  (`palw_held_interval_positions_v1`, §10.3 item 5) is unchanged; this is the certification's check.
* **The shard plan** (`palw_shard_resume_ms_v1`) prices the same last interval and gives each shard
  its layers' share of it.

**What it moves**, at testnet-11's windows (`window_receipt` 600, a 10-hour seat budget):

| | the line | with the history |
|---|---|---|
| widest context the dense row RECOMPUTES | 1,058,823 positions | 165,012 positions |
| the dense row's last 2M interval (`P` = 2,048, the wire's) | 70 s | 2.68 h |
| one whole-model shard resumes it (112 GiB fetch) down to | ≈ 27 Mbit/s | ≈ 37 Mbit/s |
| the thin vector row's route on the held devnet (`window_receipt` 40) | recompute to 70,588 positions | recompute below 1,961; resume above |

D7's sentence in §10.2 reads, amended: at a 512 GiB seat the dense row resumes its last 2M interval
on one shard at 100 Mbit/s in 2.7 h of fetch and 2.7 h of replay, 5.4 h of the budget's 10.

**The model is a bound, and the vectors check it.** On the thin row the model grows 63× from 4,096 to
32,768 positions. The engine measured 41× for the seat and 22.6× for produce. The model is
steeper, which is the direction a price that sends seats to recompute must err in. The 4,096 and
32,768-position vectors now take the Resume route, and their `document_id`s were re-pinned for that
alone. Only `seat.route` and the intervals' `resume_bytes` moved; every root, count, size and verdict
still reads as ADR-0110 §9.2's table. So the pinned suite now exercises both routes: 512 positions
recompute, and 4,096 and 32,768 resume.

**What it does not move.** SA-4's turn deadlines (`palw_context_ladder`, `palw_court_deadline`) still
price a court move's replay linearly. Those are consensus numbers on every shipped preset, and this
section does not touch them. At the widths a shipped network executes (a free prompt is capped at
4,096 ids), the history adds 1.7% at 512 positions and 13.4% at 4,096, which the deadlines' margins
absorb. ADR-0082 Decision 9's width bound for a non-held seat (`palw-certify`) is likewise
unchanged. A held network that prices court moves at a wide context takes this price with it, and
that is a ruleset move, not this amendment.

### 10.7 The eighth wall, found planning the 2M vector (2026-09-11)

§10.2 says every chain wall of the dense row admits from 512 to 2M. It does, but a wall that is
not the chain's refuses first. The A16 tier's catalog attention ops bound a history at
`A16_MAX_DOT_LEN` = 2^18:

* `a16_attn_values` refuses `kv_len > 2^18`;
* the fused site's tile arithmetic refuses more than 2^18 positions;
* the engine's fused arm composes those ops over the whole history;
* the fast kernels mirror the same refusals.

So no A16-family class executes past position 262,143. At the 262,145th row the forward fails with
`DotTooLong`, and the producer, the seat and the court all stop there. The widest A16 context
anything in this tree can produce, replay or adjudicate is 262,144 positions.

The bound was set for projections ("one power of two above the largest real reduction in the family
— Qwen2.5's `d_ff` and vocabulary"), when no attention history was longer than a few thousand rows.
Its exactness argument has room for a longer history. A value product is below `2^30`, so `2^26`
rows sum inside `i64`, and a row of `2^21` Q24 exponents sums below `2^45`. But a separate history
bound is a change to catalog ops the court runs, so it is a ruleset decision. It is inert on every
shipped network, because a free prompt there is at most 4,096 ids. It also carries a precision
question the arithmetic does not answer: at `2^21` positions a near-uniform row's Q24 probabilities
are about `8/2^24`, three bits each. It is recorded here, and ADR-0110 §9.5 carries what it means
for the 2M vector. Nothing is changed.

**Decided 2026-09-11: the bound stays** — **reversed the same day by the operator, and implemented as [ADR-0116](0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md):** a held class's history bound is the regime's 2^21, read off its registered map; every other class keeps 2^18. The reasoning below is kept as the record of what the reversal accepted — above all that the bound moving makes 2M executable and not cheap. The operator asked that the open design calls be made
where the gain is large. This one's gain is nil today and its cost is not small:

* **What raising it would buy now: nothing a network runs.** Every shipped network admits a free
  prompt of at most 4,096 ids, 64 times under the bound; no class, lane or vector beyond 262,144
  positions is proposed except the 2M vector, which is a measurement.
* **What would still stop 2M after it.** The seat's replay grows with the square of the width
  past the knee (§10.6). The 131,072-position vector's seat took 819 s of its 44.7 minutes on a
  12-core host (ADR-0110 §9.5), and sixteen times that width is, by the same rule, up to 256 times
  that seat — days, which no court clock in this tree is sized for (ADR-0092). A bound raised alone
  would trade a refusal by name for a replay nobody finishes.
* **What it would cost.** A catalog op the court runs changes: a ruleset move on every network,
  and on testnet-11 another flag day beside DAA 3,500's. A constant alone is also not enough:
  three-bit probabilities at 2^21 need a wider probability format, which is new kernel semantics,
  not a bigger number.

So the refusal by name stays the answer at 2M (`the_widest_runnable_vector_is_2_18_and_the_2m_one_is_refused_by_name`),
and the question reopens with any one of: a lane that admits prompts past 2^18; a seat that replays
2^19 positions inside the court's wall clock; a probability format with at least eight bits at
2^21.

## Mission alignment amendment — 2026-10-07

context held off-chainは維持するが、公開checkpoint/history/FOLD境界から外部verifierが不正stateとfused coneを裁定できる必要がある。producerのretained stateを読み出せることを「公開にadjudicable」の条件にしない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md -->

</details>

