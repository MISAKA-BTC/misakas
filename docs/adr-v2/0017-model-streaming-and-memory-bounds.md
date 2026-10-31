# ADR-v2-0017 — モデルstreaming・mmap・weight／captureメモリ上限

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0005](../rfc-v2/0005-bounded-resources-and-long-context.md)

## 1. この文書の役割

大きなmodelとcaptureを扱う際の読み込み・常駐・streamingの資源境界をまとめる。

## 2. 統合した判断の読み方

### 2.1

operand inventoryはleafを順次処理して構築し、同じrootのために全modelの複製を抱えない。[旧ADR-0106](../adr/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md)

### 2.2

weight読取り・replay・capture保持を明示budgetで分け、materialize可能量と論理的なstep空間を同一視しない。[旧ADR-0112](../adr/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) [旧ADR-0121](../adr/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md)

### 2.3

read-only mmap等でartifact共有を行う案はnode-local実装と合意上のroot照合を分離する。ローカルの空きメモリを新たな合意入力にしない。[旧ADR-0136](../adr/0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md)

## 3. 維持する境界・後続改定

原文のRAM比率・scratch・測定値は対象hostと版の条件付き。統合によって全platformで実測済みにはならない。

## 4. 隣接する判断との分担

context admissionは[ADR-v2-0018](0018-long-context-measurement-and-admission.md)、stateの保存容量は[ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md)、court materialの保持義務は[ADR-v2-0046](0046-court-close-and-liability-archive.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md](../adr/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) | L1–L246 | L15–L30 |
| [0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md](../adr/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) | L1–L436 | L15–L32 |
| [0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md](../adr/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) | L1–L296 | L15–L34 |
| [0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md](../adr/0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) | L1–L120 | L12–L13 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model"></a>

<details>
<summary>0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md — 原文 L1–L246</summary>

[原文を開く](../adr/0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md) · SHA-256: `050b3ac9654e360f4049322868e7db56e5ca638fef99679e88832dd854d6aed0`

<!-- ADR_SOURCE_BEGIN:0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md -->
~~~~~~~~markdown
# ADR-0106 — An inventory is a stream of leaves, not a copy of the model

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-11 on `feat/adr-0099-sharded-seat`; **W1–W7 IMPLEMENTED the same day,
  consensus-inert** (§9): the leaf, its preimage, the tree, the layout rules and every opening are
  the ones the court already verifies — what changed is how a node BUILDS them, and every root,
  count, byte total, placement and document id it builds is byte-identical to the materializing
  builder's (§1.3, §1.4). No preset, fingerprint or rule moves. W8 is the ADDER's run, not an
  operator's (ADR-0099: the adder measures, the chain recomputes) — this ADR is what lets an
  adder's machine measure a model larger than its memory; measured here on the 2B, extrapolated
  for the 35B (§1.5). W9 and W10 are stated, not built.
* Builds on: [0049](0049-palw-adjudication-contract.md) Decision G (the canonical inventory and its layout
  rules), [0099](0099-the-adder-measures-the-chain-recomputes-and-a-seat-holds-a-shard.md) (the shard plan, which places inventory rows),
  [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the held measurement), [0102](0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
  Decision 3 (a graph-v6 registration pins the INVENTORY root — so building the inventory became
  part of registering and serving) and §8 (which named this ADR's subject as the K3 prerequisite).
* Amends: ADR-0102 §4 and §6 step 4 (the 33 GiB copy is gone from measurement and registration).
  Supersedes nothing.

## 0. The sentence this ADR is

**An artifact inventory's root is a Merkle root over leaves in canonical order, and nothing in it
needs the rows held at once — so a node builds it as a stream: every row planned, then read in
canonical order a block at a time, checked by the one layout rule, hashed into the tree and
dropped.** Before this ADR every node that measured or registered a hybrid class copied every row
of the model into memory first: 5.8 GB for the 2B, more than 33.5 GiB for the 35B-A3B — more than
the machines that measure. The inventory's FORMAT does not change; only the way it is built.

## 1. What was measured

### 1.1 The copy

`palw-class measure --network testnet-11 qwen35-2b.palwq36` on the tree before this ADR
(`252b7b3b`, warm page cache, this Mac): **max RSS 5,816,369,152 B**, peak footprint 3.33 GB,
18.9 s. The builder held every row as an owned `Vec<u8>` until the root was computed, and made
whole-tensor temporaries on the way (a matmul's codes and exponents copied whole before being
chunked, every parameter table widened to its lane count). Under graph-v6 the registered root IS
this inventory's root (ADR-0102 D3), so for the 35B-A3B (≈ 33.5 GiB of rows) there was no host on
the fleet that could measure, register or resolve the class at all.

### 1.2 The stream

The same command on this ADR's tree: **max RSS 164,331,520 B** (peak footprint 121,946,664 B),
18.3 s. The inventory pass alone (the measurement also computes four context rows' walls and shard
plans): 2.1 s, 134 MB RSS, 94 MB footprint — 829,914 leaves in 1,266 tensors. Of what remains, the
artifact's own parameter store (34.8 MB, loaded at open by the reader, not by the inventory) and
one 16 MiB read block are the largest parts.

### 1.3 Byte-identity on the real artifact

| check | result |
|---|---|
| the Measured Model document (`--out`) | **byte-identical**, 5,592 B; id `045a3322f0c6bbe7…` |
| the printed report | identical but for the `written:` path |
| `palw-class verify --artifact` of the OLD document with the NEW binary | identical report, all 42 fields "recomputed, equal" |
| inventory root | `189c78352da5e4931c53a13e7f9f3bb5…` |
| inventory bytes | 2,484,279,940 |

### 1.4 The fixture record

Fourteen cases — five graphs (v1, v2, v5, v6, the v6 artifact-row profile) over the one-row lift and
over a per-token lift, a fixture with tensor-backed group exponents, a fixture with a wrong-length
exponent table, and the fuzz corpus's tiny class under v5 and v6 — were printed by the builder
**before** the emitter existed (`252b7b3b`) and are pinned: root, leaf count and bytes for the nine
that serve, the refusal *message* for the five that do not. The emitter reproduces every line
(`the_emitter_reproduces_the_materializing_builders_record`).

### 1.5 The 35B: the adder's measurement (W8)

Not run here, and not the operator's to run: under ADR-0099 whoever ADDS a model measures it, on
the machine that holds its artifact, and any node re-derives the document's deterministic half
with `palw-class verify --artifact`. What this ADR owes that adder is that the measurement fits the
machine, whatever the model. From the 35B-A3B's geometry (40 layers, 256 routed
experts, `moe_dim` 512, hidden 2048, the 2B's tile of 8 rows): ≈ 13.8 M leaves in ≈ 113 k tensors.
The stream holds the plan (one small entry per tensor), one read block and the frontier's
`⌈log₂ n⌉ = 24` peaks; the materialized digest (one 64-byte leaf plus a coordinate per row) would
hold ≈ 2 GB, and the copy ≥ 33.5 GiB. What will dominate the stream's RSS is the reader's own
parameter store — every per-row triple of every expert, ≈ 0.7 GB, resident since `open_artifact` —
which is the loader's design, not the inventory's. So an adder with a 24 GB machine and a 33.5 GiB
artifact can now measure it (the copy could not have been built there at all); the RSS their run
prints is theirs to report, beside the replay rate the document already calls self-reported.

## 2. The requirement

A streamed build is the SAME inventory as the materialized one: the same rows in the same order
under the same checks, so the same root, leaf count, byte total, shard placement, Measured Model
document id and `verify` verdicts. A root that moves between them is a defect, never a migration.
And there is one description of the rows — two hand-written builders would be two descriptions of
one computation, which is the correspondence defect ADR-0049's inventory was built to end.

## 3. Decisions

**Decision 1 (W1) — the leaf over borrowed parts.** `artifact_leaf_parts_v1(name, layer, row_start,
bytes)` is the leaf; `artifact_leaf_v1(&operand)` delegates to it. `PalwArtifactLeafHasherV1` takes
the position and the DECLARED length first — the preimage carries the length before the bytes — and
then the bytes in any chunking, so a row larger than any buffer is still one leaf; `finish` refuses a
stream whose absorbed length is not the declared one, the only way a streamed leaf could differ.

**Decision 2 (W2) — rows without bytes, and the rules spelled once.** `PalwArtifactRowDigestV1`
(coordinate, length, leaf), `PalwArtifactInventoryDigestV1` (the rows under the same checks; its
`opening_v1(index, bytes)` takes the row's own bytes back, checks them against the leaf and returns
the materialized opening byte for byte) and `PalwArtifactInventorySummaryV1 {root, leaf_count,
artifact_bytes}`. The layout rules have ONE spelling, `PalwInventoryLayoutCheckerV1`, which checks a
row given the rows before it; the held inventory's check is that checker run over the slice, so a
stream is refused by the same rule, at the same row, with the same error.

**Decision 3 (W3) — one emitter, sinks for the rest.** `qwen36_visit_inventory_rows_v1` is the only
place a Qwen3.6-family inventory's rows are laid out. It PLANS first — the profile walked slot by
slot, every refusal the layout has raised in slot order before a byte is read, each tensor's rows
described (`Stored`, `Zeros`, `Params`, `Small`, `Rope`) — then READS in canonical order, tensor by
tensor. The old builder's "first pushed coordinate wins" is restated per tensor: where several slots
plan one tensor, an EQUAL plan adds nothing (rows are a function of the plan), and different plans
are merged by offset with the first-planned kept, holding that one tensor. Every hybrid graph plans
exactly one tensor pair twice — a full-attention layer's rotation table and its clamp, named by both
rotations — with equal plans; the buffered merge is pinned on a synthetic plan because no shipped
graph reaches it. The sinks: `qwen36_inventory_v1` (the bytes kept — the court's opening path, still
used), `qwen36_inventory_digest_v1` (leaves kept), `qwen36_inventory_summary_v1` and
`qwen36_inventory_measure_v1` (nothing kept per row). The old builder's body is gone; its record is
the pin of §1.4.

**Decision 4 (W4) — row-sized scratch.** `Qwen36ArtifactV1::tensor_len` (present exactly when
`tensor` would answer) and `read_tensor_range_into` (through the file descriptor on a mapped store,
never the mapping — the reason `artifact_root` gives: faults run at 6 MB/s where reads run at
1.3 GB/s, and a faulted page stays resident). A parameter table is read through the engine's own
rule (a full table verbatim, a singleton to every lane, a head table repeated) and never widened into
a copy. The read size is not part of the result: pinned equal at 1, 7, 64 and 4096 bytes.

**Decision 5 (W5) — the measurement streams.** `palw-class measure`/`verify` hold one entry per
TENSOR, its rows' bytes summed: `palw_artifact_bytes_from_inventory_v1` places a row by its tensor
and layer and adds its length, so a tensor's sum places exactly what its rows place (pinned on every
case against the per-row placement).

**Decision 6 (W6) — the registration root streams.** The SDK's `registered_root_of`, the memoized
graph-v6 root a producer registers and the chain-registered arm resolves, is the summary's root.

**Decision 7 (W7) — the frontier.** `PalwArtifactMerkleFrontierV1` keeps one peak per level and
folds them from the lowest at the end; carrying an unpaired node upward untouched IS promotion, so
its root is `artifact_root_v1`'s at every size (pinned for 1–300 leaves, around powers of two, and
1,100). `PalwArtifactInventoryStreamV1` is the checker, the frontier and a byte count.

## 4. What this does not change, and what it does not reach

* **Consensus: nothing.** The leaf domain and preimage, the promote-odd tree, the layout rules, the
  openings and the court's checks are unchanged; so is every ruleset id and fingerprint.
* **The dense and BASE-0 builders** (`a16_inventory_v1`, `base0_inventory_v1`) still materialize.
  Their classes are 1.5 B-parameter and below, so they are not what blocks a K3-class model; the
  same emitter-and-sinks shape applies when they are next touched.
* **The court's openings** (`operand_openings_for`, `attn_site_evidence` in the hybrid backend) still
  build the materialized inventory to open a handful of rows — for the 35B that is still the
  33.5 GiB copy, on the path a seat takes when it is sampled. That is W9 (§6).

## 5. Invariants the tests hold

1. The parts leaf and the streamed leaf are the operand leaf; a short stream is refused —
   `the_parts_leaf_and_the_streamed_leaf_are_the_operand_leaf`.
2. The frontier's root is `artifact_root_v1`'s at every size —
   `the_frontier_root_is_the_promoting_root_at_every_size`.
3. The digest inventory roots, checks and opens as the materialized one, and a stream of the same
   rows yields its summary or its refusal — `the_digest_inventory_roots_checks_and_opens_as_the_materialized_one`.
4. The emitter reproduces the old builder's fourteen-line record —
   `the_emitter_reproduces_the_materializing_builders_record`.
5. On every serving case: the digest's rows are the materialized rows digested, the summaries are
   equal, one entry per tensor places what the rows place, every read size yields the same rows in
   canonical order, and openings from the digest are the materialized openings —
   `the_digest_is_the_inventory_on_every_case_and_every_read_size`.
6. First-planned-wins per tensor, equal plans once, and the only tensors any shipped graph plans
   twice are a rotation table and its clamp — `a_tensor_planned_twice_keeps_the_first_planned_row_of_each_offset`.
7. A written-and-reopened (mapped) artifact streams the inventory of the in-memory one —
   `a_mapped_store_streams_the_same_inventory`.
8. On this host's real 2B artifact: §1.3 (a recorded run, not a test — the artifact is not in CI).

## 6. Order of work

1. **W8 is the adder's, and it needs nothing more from this tree.** Whoever adds the 35B-A3B runs
   `palw-class measure` where the artifact is; a second party's `verify --artifact` on its own
   copy is the check. No fleet host and no operator run is on the path (ADR-0099).
2. **W9 — openings from the stream, a separate ADR.** The plan already IS the recipe for any
   tensor's rows (`Q36PlannedTensorV1`), so an opening needs only the root (cached once per holding
   as a digest, ≈ 64 B per leaf, or recomputed) and the one tensor's rows re-read; a `.pidx`
   sidecar is the persisted form of the same recipe. Then `operand_openings_for` and
   `attn_site_evidence` stop copying the model.
3. **W10 — shard seats** measure and serve their shard's tensors: the plan filtered by the shard's
   placement (ADR-0099), the rest never read.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0102 §4, "a graph-v6 root costs one pass that copies the artifact's rows" | a pass that holds one read block (§1.2) |
| ADR-0102 §6 step 4, "a streaming inventory" | done for measurement and registration; openings are W9 |
| the Qwen3.6 builder's body (a slot loop pushing owned rows, a seen-set, a final sort) | one emitter; its record pinned (§1.4) |

## 8. What is deliberately not decided

* **Whether the reader's parameter store should itself stream** (≈ 0.7 GB for the 35B, resident
  from open). It is the loader's design and the engine reads it per token; §1.5's measurement says
  whether it matters.
* **The sidecar's format** (W9), and whether a seat caches a digest or recomputes the root.

## 9. Number hygiene and implementation record

Written as ADR-0103 on `feat/adr-0099-sharded-seat` on 2026-09-11 (~10:00 JST) and **renumbered
0106 the same day**, under the rule every ADR's hygiene section states (a concurrent claimant
renumbers the LATER writer). What the branches held when this was written:

| number | claimant | first committed | outcome |
|---|---|---|---|
| 0102 | the embedding lift (`feat/adr-0099-sharded-seat`, `3e9c3b59`) | 04:22 JST | keeps 0102 |
| 0102 | the close cut (`feat/adr-0096-partb-drill`, `90ec2317`) | 05:02 JST | renumbered **0104** (`fix/panel-pays-consensus-rent`) |
| 0102 | the heartbeat trap (`b805fc3d`, released to testnet-11 on `release/t11-2026-09-11`) | 05:45 JST | renumbered **0105** on `main` (`40ac431b`) |
| 0103 | the held context (`docs/adr-0103-the-context-is-held-off-the-chain`, `bd280d15`) | 05:21 JST | keeps 0103 |
| 0103 | this ADR | ~10:00 JST | renumbered **0106** |
| 0107 | the share-growth ADR (`fix/share-growth-counts-final-work`) | 2026-09-11 | 0107 |

**The next free number is 0108** (the same statement `main`'s README makes since `40ac431b`).

* **2026-09-11** — written and implemented the same day:
  * `consensus/core/src/palw_artifact.rs` — `artifact_leaf_parts_v1`, `PalwArtifactLeafHasherV1`,
    `PalwArtifactMerkleFrontierV1`, `PalwInventoryLayoutCheckerV1` (the held check now runs it),
    `PalwArtifactRowDigestV1`, `PalwArtifactInventoryDigestV1`, `PalwArtifactInventorySummaryV1`,
    `PalwArtifactInventoryStreamV1`, `PalwArtifactInventoryV1::summary`, the streaming tests;
    `palw_shard_plan_v1.rs` — `PalwInventoryRowMetaV1` from a row digest.
  * `misaka/palw/base0/src/qwen36.rs` — `tensor_len`, `read_tensor_range_into`.
  * `misaka/palw/base0/src/inventory.rs` — the plan (`qwen36_inventory_plan_v1`), the canonical
    emission (`qwen36_emit_plan_v1`), `qwen36_visit_inventory_rows_v1`, the four sinks, the
    `adr0106_cases` record and tests.
  * `misaka-palw-sdk` — `palw-class measure`/`verify` through `qwen36_inventory_measure_v1`;
    `registered_root_of` through `qwen36_inventory_summary_v1`.

## Mission alignment amendment — 2026-10-07

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0106-an-inventory-is-a-stream-of-leaves-not-a-copy-of-the-model.md -->

</details>

<a id="source-0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact"></a>

<details>
<summary>0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md — 原文 L1–L436</summary>

[原文を開く](../adr/0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md) · SHA-256: `ee5c3eed6d12750c1d2901dc5b06e4a839000e6fad0bee7f523be8b2b032fe29`

<!-- ADR_SOURCE_BEGIN:0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md -->
~~~~~~~~markdown
# ADR-0112 — A class's weights are read within a budget the operator states, and the budget is a fifth of the artifact

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-11 on `feat/adr-0103-held-context`, from the operator's decision of the
  same day after the testnet-11 producers were measured (§1): "the same model and the same weights,
  with the way they are run changed from resident-whole to explicit streaming; the formal target is
  a model five times the memory available to it". **Decisions 1–5 and 7–8 IMPLEMENTED the same day
  (§10)**; Decision 6's fleet measurement waits on the operator's go, because it runs a new binary
  on a fleet host. Consensus-inert: no object, acceptance rule, fence, parameter or fingerprint
  moves. A fleet takes it by an ordinary rebuild, and a node that states no budget takes the ratio where
  its host can spare it (Decision 2, amended the same day).
* Builds on: [0052](0052-palw-qwen36-hybrid-class.md) (the mapped container: a 33 GiB file the
  runtime maps rather than reads, and the note that residency was left to the page cache),
  [0067](0067-a-registered-class-is-served-by-any-node-that-holds-its-artifact.md) (a node holds
  what it chooses; `--palw-class-cache-bytes` bounds which artifacts load — this ADR bounds what
  of one artifact is in memory), [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (every chain term at 2M is constant or logarithmic; the model's cost is the host's), and
  [0110](0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
  (a figure is a measurement with a host and a time, never a guess).
* Supersedes: the residency paragraph of ADR-0052 §"what remains" and the `MADV_WILLNEED` expert
  cache it described (§7). Amends nothing in consensus.

## 0. The sentence this ADR is

**A class's weights are the same bytes on every machine that holds them. Which of those bytes are
in memory at any moment is a decision, and it is the runtime's, made under a budget the operator
states, never the kernel's. The always-set is read once and pinned; the routed experts are read as
the router chooses them, together and through the file descriptor, and held under what the budget
leaves; nothing the arithmetic reads arrives through a page fault. The budget's default is a fifth
of the artifact's weight bytes, within what the host can spare, and the class's floor — the
always-set and one token's experts — is the least it may be. A stated budget below the floor is
refused at startup, by name and with the numbers; a default the host cannot spare leaves the class
on the page cache, and says so.**

## 1. What was measured

On 2026-09-11, on the three testnet-11 producers that run the Qwen3.6 class, with nothing changed
on any of them. The class is `PALW-QWEN36` (Qwen3.6-35B-A3B, `graph-v3`), a 33.27 GiB artifact
mapped by `Qwen36ArtifactV1::open_artifact`; the hosts are 8-core virtual machines with 23 GB of
memory and virtio disks.

**The draw.** One draw of the class's canonical job — 7 prefill positions and 2 decode calls, nine
forward passes — takes 17 to 20 minutes on `ibm node0` and on `C seat2` (their own logs: 16
draws of 17–19 minutes on node0, 16 of 17–18 on seat2, between 09:07 and 13:54 local). The
class ticket's probability is 1.0 (the per-class retarget has eased it to the floor), and the two
producers won 16 of 32 network draws against bits. The chain advanced 101 DAA in 22 hours: one
block every 13 minutes against a 2-minute cadence. **The lottery is the protocol's; the 17
minutes are the draw's.**

**What a draw reads.** `/proc/<pid>/io` sampled every 5 s across one whole draw on node0 (block
`#3` at 13:33:57 to `#4` at 13:54:00, 20 min 3 s):

| | one draw |
|---|---|
| `read_bytes` (bytes the process caused to be read from storage) | **12.8 GiB** (13,759,242,240) |
| major page faults | **3,012,123** |
| average storage rate | 11 MB/s |
| the artifact's pages resident in the process, sampled | 28 MiB to 1.2 GiB, oscillating |

Nine forward passes read 12.8 GiB: the always-set (§1's next paragraph) is evicted between tokens
and re-read, and the routed experts are read as 4 KiB faults. The device is not the limit:

| the same file, the same day, `dd iflag=direct bs=4M`, 512 MiB from offset 8 GiB | |
|---|---|
| `ibm` | **845 MB/s** |
| `C` | **563 MB/s** (its producer running) |

`misaka/palw/base0/src/mmap.rs` already recorded the mechanism when the root pass was moved off
the mapping: a cold page through a mapping is one synchronous 4 KiB fault, and on these disks
fault readahead never engages — "not under `MADV_SEQUENTIAL`, not under `MADV_WILLNEED`, not
with the device's readahead window raised" — 6 MB/s against 1.3 GB/s through reads sized to a
tensor. The root pass took that lesson. The forward pass did not: every weight it reads is
`map.i8_slice`, a fault.

**What the artifact is.** Read off its header by `scripts/verify/misaka-qwen36-artifact-directory.py`
(the directory of 62,393 tensors and 52,623 parameter rows; the script prints this table and the
budget arithmetic for any `.palwq36`, so a host can be sized before a node starts):

| | bytes | share |
|---|---|---|
| routed experts (256 a layer, 8 routed a token, 40 layers) | 30.94 GiB | 93.0 % |
| attention and recurrence projections, norms | 1.23 GiB | 3.7 % |
| unembedding (248,320 × 2,048) | 0.49 GiB | 1.5 % |
| embedding (the same) | 0.47 GiB | 1.4 % |
| shared experts and routers | 0.14 GiB | 0.4 % |
| **the always-set** (everything but routed experts) | **2.34 GiB**; **1.86 GiB** without the embedding | |
| one routed expert (gate, up, down, exponents, five rows) | 3.09 MiB | |
| **one token's routed experts** (8 × 40) | **0.97 GiB** | |
| parameter rows, in the header, all read into memory at open | 0.71 GiB | |

Nine tokens route to about 64 of each layer's 256 experts (eight draws of 256, nine times: the
expected union), so a draw's experts are about 7.7 GiB of the file whatever holds them.

**The other term.** The node's own working set, which this ADR does not touch and states because
it is the rest of the host: `ibm node0` (the class's producer, a panel seat, two owned dense
artifacts) holds 8.9 GB of anonymous memory with 3.5 GB more swapped; `ibm node1` (a floor
producer and a seat, the same two dense artifacts, no mapped class) 13.6 GB with 1.8 GB swapped;
`C seat2` 14.2 GB. Two of them share `ibm`'s 23 GB. The class's residency cache existed
(`Qwen36Residency`, ADR-0052's note) and was never wired into the producer; it prefetched with
`MADV_WILLNEED`, which on these disks prefetches nothing.

**What was NOT the cause.** The operator's first reading named two more suspects, checked and
cleared: the Qwen3.6 runtime allocates no context-length cache at start (`Qwen36Cache::new` holds
the recurrence's fixed state and empty key/value vectors that grow by the token, 60 MiB at most on
this class), and the draw is nine forward passes, not thirty-two. A 128K context vector was running
on the maintainer's Mac at the time (ADR-0110 §9.5); it shares nothing with the fleet.

## 2. The requirement

A class artifact of `S` bytes runs — produces, replays, is judged — on a host whose memory for it
is `B ≥ S / 5`, with the same bytes and the same class id it has anywhere else, at a wall time
bounded by what it must read: `(routed experts per forward − hits) × forwards / storage bandwidth
+ compute`, never by page faults. **R-5**: the ratio the class is certified at is five. The floor
below which a class does not run at all is stated by the class (§3 Decision 3), and it is far
under a fifth for this one: 2.83 GiB against 6.65.

Three things the requirement does not ask for, said so nobody re-solves them:

* **A smaller model.** The weights are the artifact's, byte for byte; residency changes no bit of
  arithmetic, and the artifact root, the class id and every committed row are unchanged (§5 I-1).
* **A different job.** The canonical job stays `(7, 2)`; changing it changes the class id, which
  is a registration, not a runtime (§8).
* **Determinism the tree does not already have.** Every kernel a forward pass runs is exact
  integer arithmetic under ADR-0040 Decision E, so two hosts agree bit for bit whatever their
  residency; the model's identity is `artifact_root` (ADR-0052), a digest over every weight byte,
  every parameter row and the shape. Both already exist; this ADR relies on them.

## 3. Decisions

**Decision 1 — the forward pass reads weights through the file descriptor into memory the runtime
owns, never through page faults on the mapping.** `Qwen36ArtifactV1::tensor` returns
`TensorBytes`, a handle: a slice of an owned store or the mapping, or an `Arc` on bytes the
residency holds. Under a residency every weight a projection reads is a held handle. The mapping
stays: it is where the header and the embedding table are read, and it is the whole path when the
operator asks for the page cache (`PageCache`, the behaviour before this ADR, kept so the two can
be measured against each other).

**Decision 2 — the budget is one number, stated by the operator, and its default is a fifth of the
artifact's weights.** `--palw-class-resident-bytes=<bytes>` on `kaspad`; `Qwen36ResidencyPolicyV1`
in the runtime: `FifthOfTheWeights` (nothing said), `Bytes(b)`, `PageCache` (`0`). The ratio is
`QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1 = 5`, one spelling. For the Qwen3.6 class that is
7,145,529,856 bytes: 6.65 GiB.

> **Amended 2026-09-11, before any host ran it: the default is a fifth within what the host can
> spare.** Read against the fleet the day this ADR landed (§10.5), a fixed fifth was 6.65 GiB of
> anonymous memory on hosts that had none to give — `ibm` with two nodes and 6 of 7 GB of swap in
> use, `.113` with all 15 GB of its swap in use and three pool slots at their 6 GiB cgroup limit —
> and anonymous memory is what the kernel cannot reclaim the way it reclaims the page cache's
> pages: the default would have turned a slow draw into a killed node. So when the operator states
> nothing, `kaspad` measures what the process can take at startup — `MemAvailable`, or its memory
> cgroup's headroom where that is smaller — less `PALW_CLASS_NODE_RESERVE_BYTES_V1` = 16 GiB, the
> node's own working set (§1: 9–14 GB, which a node at startup does not hold yet), and opens the
> class under `FifthWithin(spare)`: a fifth if the spare bytes hold it, the spare bytes if they
> hold less but at least the floor (Decision 3), and **below the floor the page cache — never a
> refusal**, because nobody stated a number — with a warning that names the spare bytes, the
> floor and the flag. A default measured once is spent across the files one load maps. Where no
> reading exists (macOS) the default stays the fifth. A **stated** budget is unchanged: honoured,
> and refused below the floor. The ratio the class is certified at (R-5) does not move; what
> moved is that the node no longer assumes the host has it.

**Decision 3 — two tiers, told apart by name, and a floor.** Every tensor that is not a routed
expert's (`blk.N.ffn_expert.K_*`) and not the embedding table is the always-set: read once at open
into owned memory, in parallel through the file descriptor, and never given back (1.86 GiB here).
Routed experts are an LRU of owned buffers under what the budget leaves (4.79 GiB here, about five
tokens of them). The embedding table is neither: a token reads one row of it,
`embedding_row` reads that row (2 KiB) directly, and the 0.47 GiB table is never in memory. An
expert's five parameter rows ride with its tensors — read when it is admitted, given back when it
is evicted — so the 0.71 GiB of rows the old path copied into memory at open is not copied
(the non-expert rows, kilobytes, still are). The floor is the always-set plus one token's routed
experts: `1.86 + 0.97 = 2.83 GiB`, below which a forward pass would re-read what it just read. A
budget below the floor is refused at open, by name, with both terms and the floor in the message.

**Decision 4 — a layer's experts are read together, the moment its router commits.**
`admit_experts(layer, chosen)`: the chosen experts not yet held are read in parallel (one expert
is 3.09 MiB in three tensors, their exponents and five rows; eight of them land in tens of
milliseconds at the measured device rates) before the first is computed. Nothing depends on the
prefetch: an expert read outside an admission — a test, the inventory pass, a court replaying a
tile — is admitted on the way. Prefetch across layers is impossible and not attempted: layer
`L+1`'s routing is a function of layer `L`'s output.

**Decision 5 — the class's identity and its arithmetic are untouched, and a test says so at the
ratio.** `a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size`: a fixture
with the class's expert count, held within a fifth of its bytes, computes the owned store's logit
rows token for token while never holding more than its budget; at the floor too; and through the
page cache too. `the_budgeted_paths_read_what_the_owned_ones_hold`: the root is one digest whether
the expert rows are owned or in the file, the embedding row is the table's row, and a walk over
every expert through a budget that holds a sixth of them evicts and re-reads them all correctly.

**Decision 6 — the fleet measurement, and the numbers this ADR is judged by.** The same host, the
same file, the same tokens, `qwen36-run` (the bench tool, ADR-0052) under `--resident page-cache`
and under the default: bytes read from the file per token, time per token, hits and evictions.
Then a producer restarted with the default: its draw's storage line (Decision 8) and its draw
time. The arithmetic says a draw reads about `9 × 0.97 GiB` less the hits — 8 GiB at 563–845 MB/s
is 10–15 s — plus compute; the measurement is what §10 records. This decision is the one not
taken by the maintainer's session, because it runs a new binary on a fleet host.

**Decision 7 — the operator's arithmetic, printed.** At load, for every mapped holding: the
budget, its two tiers, how many tokens of experts the second tier holds, and what the host has
available (`MemAvailable`, or the memory cgroup's headroom where smaller — amended 2026-09-11);
a warning when the budget exceeds that, because a budget the kernel reclaims is the page cache
with extra steps; and, for a default the host could not spare (Decision 2's amendment), a warning
that the class is on the page cache and why. The node's own working set (§1, 9–14 GB on these
hosts) is outside this ADR and inside that arithmetic: two kaspads on one 23 GB host leave no
budget, and that is the operator's to change.

**Decision 8 — what one draw reads from storage is a log line, not a sampler.** After every draw
the producer prints the process's `read_bytes` delta (Linux; "not counted" elsewhere) and, per
mapped holding, what the loader read, in how many misses of how many expert lookups, what it
evicted and what it holds. The number that explained the fleet's draws was found with a sampler on
the host; it is a line an operator watches now.

## 4. What this costs

* **Memory:** the budget, plus one layer's experts in flight (a handle a projection holds
  outlives an eviction of its entry; the floor is sized for it), plus the mapping's header pages.
  Against today: today the process held 8.9 GB anonymous and paid 3.5 GB of swap for the 1.2 GiB
  of file pages it could keep; under the default it holds 6.65 GiB of weights it chose.
* **Open:** the always-set's 1.86 GiB through the file descriptor in parallel — seconds — beside
  the root pass the node already makes (a minute cold).
* **A draw:** its experts, once each, at the device's sequential rate; its always-set, never
  again. The hit rate between tokens of one draw is small (§1: 64 of 256 a layer over nine
  tokens) and the ADR does not count on it.
* **Code:** one accessor type, one residency struct, one open path, one flag, two log lines. No
  consensus crate changes.

## 5. Invariants the tests must hold

1. **I-1, identity.** A budgeted mapping's `artifact_root`, every tensor and every parameter row
   equal the owned store's; a forward pass over it produces the owned store's rows, at a fifth, at
   the floor, and through the page cache. (`a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size`,
   `the_budgeted_paths_read_what_the_owned_ones_hold`, and the existing
   `a_mapped_artifact_runs_identically_to_an_owned_one`, `the_mapped_root_equals_the_owned_root`.)
2. **I-2, the budget.** Between admissions the residency never holds more routed-expert bytes than
   the budget leaves after the always-set; at the floor it holds at most one token's.
3. **I-3, the floor.** A stated budget below the always-set plus one token's routed experts is refused at
   open with both terms and the floor in the message; the floor itself opens.
   (`a_budget_below_the_floor_is_refused_by_name`.)
4. **I-4, no faults.** Under a residency no weight a projection reads is a slice of the mapping:
   pinned tensors and expert parts are held handles, the embedding row is read directly. Held by
   construction (`tensor`'s residency arm returns `Held` for both tiers) and by the storage line's
   major-fault count in the fleet measurement (Decision 6).
5. **I-5, the policy's arithmetic.** A fifth rounds up; `0` is the page cache; the expert-name
   rule classifies routed experts and their rows and nothing else.
   (`the_residency_policy_arithmetic`.)
6. **I-6, the seam.** The lineage contract carries the policy (`PalwModelLineageV1::load`), the
   dense lineage ignores it, `kaspad`'s two duties pass what the operator stated, and a holding
   reports its residency in its summary line and its stats through `residency_stats_of`.
7. **I-7, the default takes only what is spare** (amended 2026-09-11). With nothing stated, the
   budget is at most a fifth and at most what the host has available less the node's reserve;
   below the floor the class opens on the page cache, identical in root and rows, with the reason
   kept — never refused. (`a_default_budget_is_a_fifth_within_what_is_spare_and_below_the_floor_the_page_cache`,
   `a_budgeted_holding_reports_its_residency`, `a_cgroup_limit_bounds_what_the_default_may_take`.)

## 6. Order of work

1. The accessor and the residency in `misaka-palw-base0` (`qwen36.rs`, `mmap.rs`): Decisions 1,
   3, 4, 5 and their tests. **Done.**
2. The seam: the policy on the lineage contract; `kaspad`'s flag, both duties, the load-time
   arithmetic and the draw's storage line (Decisions 2, 7, 8). **Done.**
3. The bench tool under the policy (`qwen36-run --resident-gib | --resident page-cache`). **Done.**
4. Decision 6 on `C` or `ibm`, with the operator's go: the bench tool under both policies, then
   one producer restarted on the default, its storage line and draw time recorded in §10.
5. The unarmed integration build and the `main` merge carry it (consensus-inert; fingerprint
   unchanged).

## 7. Supersession

| what | by |
|---|---|
| ADR-0052's note that expert residency "remains the page cache's, with a measured note" and the `Qwen36Residency` LRU (`MADV_WILLNEED` admission, `MADV_DONTNEED` eviction, on the engine, used only by the bench tool) | Decisions 1, 3, 4: residency is the artifact's, owned buffers read through the file descriptor, on every engine over the artifact. The `MADV` advice stays in `mmap.rs` as a utility nobody calls. |
| the root pass's rule "per-token expert access stays on the map, whose resident-set behavior is the reason the map exists" (`mmap.rs`) | withdrawn: the resident-set behaviour was the reason the draw took twenty minutes. The map's reason is now the header and the embedding row. |
| `docs/palw-practical-runtime-plan-2026-08-26.md`'s residency line ("always-set は pin, routed expert 以外を open 時に MADV_WILLNEED") | Decision 3, with the pin an owned read. |

Nothing in consensus moves. `PalwExecutionBackendV1` is unchanged; the lineage contract gains one
parameter.

## 8. What is deliberately not decided

* **A single-forward ticket.** The operator's sketch proposed that a draw be one prefill pass
  rather than the canonical job's nine, so a low-memory host reads its experts once a draw rather
  than nine times. It would cut a draw's storage traffic by about nine and its compute likewise —
  and it changes the canonical job, hence `pwu_per_inference`, hence the class id: on testnet-11
  the class is minted in genesis, so it is a re-mint, and on a live registry it is a new class.
  Decision 6's numbers decide whether it is worth a ruleset move: at the device rates measured, a
  nine-pass draw reads for 10–15 s, which is the order of its compute, so the loader alone brings
  the draw from twenty minutes to under a minute. Recorded, with the arithmetic, and not taken.

  **Decided 2026-09-11: not taken** — **reversed the same day by the operator, and implemented as [ADR-0117](0117-a-draw-is-one-forward.md)**, which also corrects two statements here: the canonical job is NOT part of the class id (`class_id` hashes the graph profile alone, so the job changes by a fence and needs no re-mint), and the committed path runs eight forward passes, not nine (`P + D − 1`; the ninth was the old path's). The reasoning below is kept as the record. The operator asked that the open design calls be made where
  the gain is large. What a one-forward job would buy depends on the host (§10.5):

  | host | a draw today | with this ADR's loader | with a one-forward job as well |
  |---|---|---|---|
  | `C` (about 4.5 GiB spare) | 17–18 min, page faults | about 8 GiB read at 563 MB/s, 15 s, plus compute | about 1 GiB, 2 s, plus a ninth of the compute |
  | `ibm`, `.113` (nothing spare) | 17–20 min | unchanged: the page cache (§10.5) | 12.8 GiB → about 2.8 GiB, still through faults at 11 MB/s: about 4 min |

  Where the loader has memory it already brings the draw inside the chain's two-minute cadence,
  and one forward only makes a fast draw faster. Where it has none, one forward still leaves the
  draw at twice the cadence. The fix there is the node's own working set, 9–14 GB of anonymous
  memory a node holds for itself (the bullet on it below), not the job. Against that gain, the job is
  part of the class id through `pwu_per_inference`. On testnet-11 the class was minted in genesis,
  so changing the job means re-minting it: a flag day for every operator. A class's market row is
  also keyed by its class id (ADR-0087), so positions on the old class could only be sold back.
  The gain is not large enough for that. What would reopen it is Decision 6's measurement: a draw
  on `C` under its budget that still takes longer than the cadence.
* **Streaming the always-set.** Below the floor (2.83 GiB here, a ratio of 11.8) the always-set
  would have to be read per layer per token — 1.86 GiB a token, 17 GiB a draw. The dial exists in
  the design; the ratio this ADR certifies does not need it. **The ratio is a property of the
  mixture, and a class that is not one has a floor near its size**: the maintainer's
  `qwen35-2b.palwq36` (2.28 GiB, one routed expert a token of 37 MiB in each of 24 layers) has a
  floor of 1.81 GiB — a ratio of 1.3 — and the default fifth is refused on it, by name, as
  Decision 3 says. Such a class runs with a stated budget at or above its floor and gains
  Decision 1's read path all the same (§10.3).
* **The dense tier's owned artifacts.** `Base0ArtifactV1` files (1.67 GiB each, two on every
  fleet node) are read whole into anonymous memory. The same loader would fit them; they are small
  against the hybrid and are not this ADR's problem.
* **The node's own working set.** 9–14 GB of anonymous memory per `kaspad` on the fleet (§1),
  3.3 GiB of which are the two owned dense artifacts. What the rest is has not been measured; it
  is the other term of every host's arithmetic, and it is a measurement to make, not a guess to
  write here.
* **Reading and computing in one layer overlapped.** Decision 4 reads a layer's eight experts,
  then computes them. Computing the first while the eighth lands is a later refinement of the
  same design, worth at most a layer's read per layer.

## 9. Number hygiene

Written as 0112 on `feat/adr-0103-held-context`. Every local branch and `origin/main` were listed
before the number was taken (0108 and 0109 are `origin/main`'s, 0110 and 0111 this branch's); no
branch holds an 0112. **The next free number is 0113.**

## 10. Implementation record (2026-09-11)

### 10.1 Where each Decision lives

| Decision | where | what pins it |
|---|---|---|
| **1** the handle | `qwen36.rs`: `TensorBytes`, `Qwen36ArtifactV1::tensor` / `tensor_sized` / `group_exponents`; `mmap.rs`: `read_i8_at`, `read_u8_at` | I-1's tests; every kernel call site takes `&TensorBytes` |
| **2** the policy | `Qwen36ResidencyPolicyV1`, `QWEN36_RESIDENT_FRACTION_DENOMINATOR_V1`; `kaspad --palw-class-resident-bytes`, `palw_class_residency_v1` | `the_residency_policy_arithmetic` |
| **3** the tiers and the floor | `Qwen36ResidencyV1` (pinned map, expert LRU, `expert_param_extents`), `open_artifact_with_residency`, `embedding_row`, `param_rows`' expert arm, `artifact_root`'s merged walk | `a_budget_below_the_floor_is_refused_by_name`, `the_budgeted_paths_read_what_the_owned_ones_hold` |
| **4** the admission | `Qwen36ResidencyV1::admit` (rayon over the misses), called from `moe()` and the planned walk's `RouterTopk` | the fifth test's lookup count |
| **5** identity at the ratio | — | `a_budgeted_artifact_computes_what_an_owned_one_does_at_a_fifth_of_its_size` |
| **7** the arithmetic | `kaspad::palw_backends::load_class_holdings_v1` (the info line, the `MemAvailable` warning) | — |
| **8** the storage line | `storage_snapshot_v1`, `log_draw_storage_v1`, `process_storage_read_bytes_v1`; the producer's `produce_one` | — |
| the seam | `PalwWeightResidencyV1` on `PalwModelLineageV1::load`; `PalwClassSdk::load_artifact_with`; `residency_stats_of`; the holding's summary | the SDK suite; `kaspad`'s holdings tests |

### 10.2 What was run

The `misaka-palw-base0` suite, the SDK suite, `kaspad`'s `palw_backends` tests, clippy on the three
crates — §10.3 has the counts. The fleet measurement (Decision 6) has not run.

### 10.3 Numbers

**The suites** (2026-09-11, debug): `misaka-palw-base0` 426 passed, 0 failed, 10 ignored;
`misaka-palw-sdk` 25 passed; `kaspad`'s `palw_backends` 12 passed, the budgeted holding's among them.

**A real mapping on the maintainer's Mac** (12 cores, 24 GB; `qwen35-2b.palwq36`, 2.28 GiB, one
routed expert a token in 24 layers, so nothing to evict; the file warm in the page cache, which is
the case that flatters the old path): six prefill positions and eight decode calls, the same
tokens through both policies.

| | the page cache | a budget of 1.90 GiB |
|---|---|---|
| open | 4.6 ms | 71 ms (0.94 GiB pinned) |
| prefill, six positions | 2.14 s (357 ms a token) | **0.54 s (91 ms a token)** |
| decode | 13.2 tok/s | 13.0 tok/s |
| produced ids | `[11, 1879, 0, 3555, 830, 11, 1879, 0]` | the same |
| nonzero logits | 248,196 of 248,320 | the same |
| peak resident | 2.23 GB | 2.26 GB |
| read from the file | (faults) | 1.82 GiB: 0.94 pinned, 0.88 of experts, 99.4 % hits after |

The rows are identical (I-1 on a real artifact). Even with every page already cached, the prefill
is four times faster through owned buffers than through the mapping, which says something about
faults and copies before it says anything about disks. What it says about disks is §1's 845 MB/s
against 11 MB/s, and that is Decision 6's measurement to make.

**The fleet** (Decision 6): not run.

### 10.4 The integration and the `main` merge

Two things this ADR changed were in use on the fleet lineage, and neither branch alone showed it:
ADR-0106's inventory test asks `Qwen36ArtifactV1::extent` whether a reopened artifact is the
mapped store, and hands `tensor`'s result to the writer as `&[i8]`. `extent` had gone with the
`MADV` residency it served; it stays, as the directory read it is, and the test borrows the
handle. On `integ/t11-adr0103-unarmed` after that: `misaka-palw-base0` 432 passed, 0 failed,
10 ignored; the SDK suite and `kaspad`'s `palw_backends` (12) pass.

On `merge/0103-0110-0111-into-main`: the release build prints the t11 fingerprint `ecbdbc22…`
and schedule `8cacc12a…`, both unchanged, with all 26 fleet flags and
`--palw-class-resident-bytes`; `shipped_presets_have_pinned_fingerprints` is in the
consensus-core suite that passes there (2,069). The Lints job's three gates (`fmt`, `clippy`,
`pq-guard`, on the pinned 1.93.0) pass. `origin/main`'s own run of that job stops at its first
step — rustfmt reports 28 files — so its clippy had never run; behind it were four clippy errors
in consensus-core's tests, and four findings this lineage had added (three rustfmt diffs, a
`byte_char_slices`, and an `is_multiple_of` that only shows once the crates before it are clean).
This lineage's are fixed on the feature branch; `origin/main`'s on the merge.

### 10.5 The fleet's hosts, and the default amended (2026-09-11)

Read on the three hosts before any restart (13:0x UTC, `/proc` and the cgroup files only):

| host | memory | swap in use | nodes holding the Qwen3.6 class | what each node held for itself |
|---|---|---|---|---|
| `ibm` | 23 GB | 6 of 7 GB | node0 (producer) | node0 7.3 GB anonymous + 4.6 GB swapped; node1 (dense) 13.6 + 2.2 |
| `C` | 23 GB | 4 of 19 GB | seat2 (producer) | 12.7 GB anonymous + 2.3 GB swapped, 9.0 GB of the file cached |
| `.113` | 23 GB | 15 of 15 GB | the explorer's node (panel) | 4.3 GB + 5.0 GB swapped; pool slots 04, 05, 06 at their 6 GiB cgroup limit |

Under the default as first written each of those nodes would have taken 6.65 GiB it could not be
given. Under the amendment: on `ibm` and `.113` the spare bytes are under the floor and the class
stays on the page cache, as today, with the warning; the slots' cgroups leave nothing; `C`, with
about 20 GiB available at a restart, spares about 4.5 GiB — between the floor (2.83) and the fifth
(6.65) — and takes that. What a draw then costs on `C` is Decision 6's measurement; the other
hosts' draws speed up only when the node's own working set (§8) is measured and made smaller, or
the operator states a budget and accepts the swap it costs.

This is the rollout's order, not the ADR's: the fleet was moved to the DAA 3,500 build by another
session the same afternoon, and this ADR's first host waits for that rollout to finish.

## Mission alignment amendment — 2026-10-07

operatorの読取budgetはresource boundとして維持し、public verifierのartifact認証とcold fetch、証拠取得、局所化の最悪時量を別に計測する。budgetを超えるprofileの新gateを開けない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0112-a-classs-weights-are-read-within-a-budget-the-operator-states-and-the-budget-is-a-fifth-of-the-artifact.md -->

</details>

<a id="source-0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders"></a>

<details>
<summary>0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md — 原文 L1–L296</summary>

[原文を開く](../adr/0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md) · SHA-256: `9523e8e53dca4984a8580d15ef50ccac721d22c631a8509751f9043af6b4abdd`

<!-- ADR_SOURCE_BEGIN:0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md -->
~~~~~~~~markdown
# ADR-0121 — A held capture is served from its fold as the replay streams, and a node holds two ladders

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED and IMPLEMENTED 2026-09-12 on `feat/adr-0103-held-context`, at the operator's
  instruction ("node 側の held 経路（ADR-0120、数日規模）に進む" — the number moved to 0121 when the
  model-seed session claimed 0120 the same day, §8). The node half of
  [ADR-0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md),
  whose §7 mapped it. **Consensus-inert:** no consensus object, rule, parameter or fingerprint
  moves; every object a node now builds by streaming is byte for byte the object the whole-capture
  path built, and every class that is not held is served, checked and named exactly as before. Rides
  testnet-11's held build (DAA 6,000) because that is the build in which a held class first exists.
* Builds on: [0103](0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
  (the regime; Decision 2's intervals of `P` positions), [0119](0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
  (a held class's ladder is the regime's `2^40`), [0086](0086-the-opening-carries-the-fold-not-the-leaves.md)
  (the V4 opening is the fold's digests; a seat names the leaf from a served block),
  [0085](0085-the-close-is-assembled-from-what-was-served.md) (the close annex and the close from
  served intervals), [0111](0111-a-seat-may-demand-the-committed-leaf-it-needs-to-judge.md)
  (a leaf's evidence), [0082](0082-the-close-is-flat-in-the-context.md) Decision 7 (the fold).
* Amends: ADR-0086 Decision 6's block index, for a held class only (§3 D3), and its block answer,
  which now serves a block straddling a range's end as its leaves inside the range, named by the
  range's root walk (§3 D5); ADR-0082 Decision 7's "the retain level is the ruleset's ladder's", for
  a held class only (§3 D3). Closes ADR-0119 §7's
  first three items; §7 below keeps the rest.

## 0. The sentence this ADR is

**A node serves, checks, names and prosecutes a held capture from its fold as the replay streams —
the retained digests and the two edge blocks for an opening, one block for a block answer, the
step's blocks for a leaf's evidence, the disputed leaves' tiles for an annex — and never holds a
vector the size of the job or of the interval; it walks a held class at the class's ladder and lays
a capture out whole only below the network's; and a held class folds at level 12, with a block
request that counts from its interval.**

## 1. What was found

ADR-0119 moved a held class's ladder to the regime's `2^40` on the chain and left the node where it
was: every backend was built at the network's ladder
(`with_step_ladder_cap(court.max_step_leaf_count())`), so a held producer refused any job past
`2^26` leaves — about 650 positions of the dense row — and a claim in that gap was a job its seats
signed `Unavailable` for. Raising the number
the node passes was not the fix, because the survey of 2026-09-12 (ADR-0119 §7) found that number
doing two jobs, and the routes it guarded not streaming:

* **One knob, two jobs.** The backend's ladder priced and refused a job, and it was the only guard
  in front of the whole-capture paths that allocate a vector of `step_leaf_count` hashes from a
  count that arrived in a gossiped blob (the dense re-execution of a fold, `leaves_by_position`, a
  dense material's check). At `2^40` a few hundred bytes could ask for `2^46`.
* **Replays that did not stream.** Every interval replay — the executor's opening, the seat's V4
  verdict, the block answer, the naming, a leaf's evidence, the annex — sized a dense capture to the
  WHOLE step space for a window of an interval or two, and kept every tile of the window beside it:
  27 GB of leaf vector at 4,096 positions of the 1.5B dense row, a process abort at the held ladder;
  the seat's verdict alone walked 845 MB of range for one interval of the 512-position class, 13.5
  GB at `2^21` positions; the naming held six megabytes of tiles a position to compare one block.
* **A level derived where it had to be pinned.** Producer and seat each derived the fold's retain
  level from their ladder (`max(⌈log2 ladder⌉ − 20, 12)`): 20 at `2^40`, a block of 64 MiB against a
  4 MiB interval lane, and a held interval's width `P` is derived from one digest per `2^12` leaves.
* **A block index that runs out.** A block-leaves request carries sixteen bits of GLOBAL block index;
  at level 12 that reaches leaf `2^28`, which a held job of the dense row passes before its 3,000th
  position.

## 2. The requirement

A node answering or judging a held claim holds, at most, the fold's retained tree (`leaf_count / 64`
bytes at level 12), the two blocks an opening's range cuts, the blocks one step reads, and the
disputed leaves' tiles — never the job's leaves or the interval's; a count out of a blob allocates
nothing past the network's ladder; and every object it builds is the byte string the whole-capture
path built, so neither the chain nor a seat can tell which path made it.

## 3. Decisions

### D1 — Two ladders: the class's walks, the network's materializes

Each family's backend keeps the number its caller passes — the network's `max_step_leaf_count`,
unchanged at every construction site (the SDK, the lineages, the workers) — as its
**materialization cap** (`materialize_cap()`), and derives the **class's ladder** from its own
profile (`step_ladder_cap()` = `palw_class_step_ladder_v1(network, profile)`: the regime's `2^40`
for a held class, the network's for every other). The class's ladder prices a job, refuses a capture
above it and walks every Merkle path and streamed replay; the materialization cap bounds every site
that lays a capture out whole. For a class that is not held the two are one number, so nothing
changes there.

The sites, classified in each of the three families:

* **The class's ladder:** the producer's fold (`execute_free_prompt_streaming`,
  `execute_for_verdict`), pricing (`fp_context_work_leaves_v1`), the interval count, the fold's
  interval opening, resume and block answer, the held DA court's step-range answer from a fold,
  the seat's V4 verdict and naming, the close from served intervals, a leaf's evidence from a
  fold, the operand openings' adjudicator run, and the fused-site dissection's site derivation.
* **The materialization cap:** the attempt lane's dense capture (`execute`, the drill's injected
  fault), the whole-capture prover (`refutation_with_prompt`), the bisection's prefix state, a dense
  material's check (`verify_material`), a dense retention's opening, block and step-range answer,
  the dense re-execution of a fold (`dense_capture_from_fold_v1`) and the fused site's honest
  re-execution (`attn_rerun_v1`). BASE-0 retains every capture dense, so every site of that family
  that reads a capture reads this.

A capture between the two ladders is refused by the whole-capture prover **by name**
(`base0_whole_capture_refusal_v1`: "past this node's materialization cap … judged by the streamed
routes … never laid out whole"), before a leaf vector is built from its count.

The panel reads the class's ladder where it judges a claim itself: the accused's filed output tile
is walked at the ladder of the class its binding names, and the capture sampler grades at
`class_step_ladder(class_id)` (resolved from the class's profile through the same two sources as
its prompt form, ADR-0118). The conformance battery counts a lineage's canonical job at the class's
ladder, as the gate does.

### D2 — Every held route streams

* **The replay** (`d532f89a`): a family's one required kernel verb is `replay_interval_into` — every
  tile of the window handed to a sink at its canonical leaf index, in leaf order, the index a cursor
  walked by the fold's own slot walk (`base0_position_tiles_v1`, shared by the capture and the
  replay, so the two cannot enumerate differently).
* **The seat's V4 verdict** (`9b447b55`): `Base0FoldRangeCheckV1` folds each whole block as it
  completes and compares it with the served digest, keeping only the edge blocks (and the first
  differing block's leaves); the root is walked from `[left] ‖ digests ‖ [right]` by
  `base0_fold_range_root_v1`, the consensus walk's own rule.
* **The executor's opening** (`17300012`, `37c9c7ca`): the span replay streams; each completed block
  is checked against the node the executor retained for it; only the two edge blocks and the seed row
  are kept, the siblings come from them and the retained nodes (`range_siblings_from_edges_v1`), and
  the opening is walked to the committed root before it is served. A block answer keeps its block.
* **The naming** (`ddb150e1`): the seat keeps the served block's own tiles and drops the rest.
* **A leaf's evidence** (`b2af4c94`): the family verb `fp_leaf_refutation_v1`, whose default is
  ADR-0085's annex path, is overridden for a fold by `base0_fp_leaf_refutation_from_fold_v1`: only
  the blocks the step's inputs and output live in are replayed, from the interval's anchor, each
  checked against its retained digest; the openings are walked from the retained tree's edges.
* **The close annex** (this ADR's last commit): the annex reads one tile per disputed leaf and
  nothing else, so a fold replays from the interval's anchor to the last disputed leaf and keeps
  those tiles (`base0_fp_disputed_tiles_from_fold_v1`), where it kept the interval's.

### D3 — A held class folds at 12, and a block request counts from its interval

`palw_base0_sparse_retain_level_for_class_v1(profile, ladder)` is 12 for a class under the held
regime whatever its ladder, and the ladder's own level for every other class, unchanged. The
producer's fold and the dense route's V4 tree take it; a seat never derives the level — it reads it
off the opening it verified, the panel's block request included. The retained set this costs is
`leaf_count / 64` bytes: 6 MiB for a 4,096-position job of the dense row, about 3 GiB at its `2^21`
positions, beside the hundred gigabytes of cache that job holds anyway.

A block-leaves request's sixteen bits are, **for a held class**, the block's distance from the block
holding the interval's first leaf (`base0_fp_block_request_number_v1`; the executor reads it back
against the opening it serves, `base0_fp_block_of_request_number_v1`). A held interval's width is
derived so that its digests fit the lane, which is at most `2^16` blocks, so the number always fits.
The served block still names its own first leaf. Every other class keeps ADR-0086 Decision 6's
global index, so no request a shipped node sends changes meaning.

### D4 — What is refused rather than built

Past the materialization cap, the whole-capture arms refuse by name instead of attempting the
allocation: the capture sampler's prover, the bisection's prefix state, the fold's dense
re-execution and the fused site's honest re-execution. Two consequences are this ADR's to state:
a held claim past the network's ladder is judged on the interval lane (the held route) and not by
the whole-capture sampler; and its fused-site dissection evidence is not built on this node (§7).

### D5 — A lie at an interval's edge is named from the edges

Blocks are aligned to the step space and intervals are not, so the first and last blocks of every
interval's range are whole in neither interval: the V4 opening carries no digest for them and the
block naming (ADR-0086 Decision 6) refuses them. A fault there reached no leaf — and, the seat's
address being `fold_edge_v1`'s left edge whenever the range starts off a block boundary, not even
the right edge — so a liar that knew the geometry could lie where no seat names a leaf, and a held
class has no bisection to fall back on (found by §4's run; the drill had to be moved inside a
block). Now:

* the executor serves a block that straddles the range's end as its leaves inside the range
  (`Base0FpBlockLeavesV1::cut_v1`; the fold's answer takes the seed row's from the opening and
  replays the rest), under the same request and number as a whole block;
* a seat whose fault is not a whole block asks for both edges (`Base0FpFoldRangeOpeningV1::
  edges_v1`) and names the leaf through `fp_name_the_edge_leaf_v1`: the served edges (its own where
  one is not served), the opening's digests and its siblings must walk to the committed root
  (`base0_fold_range_root_v1`, the rule the V4 verdict walks) — so an executor cannot serve edges it
  did not commit — and the leaf named is the first served one that differs from its own replay.

Node-side and consensus-inert: one request kind, a wider answer, a new seat verb.

## 4. What it costs

Time is what it was: a replay of the interval from its anchor, which every route already paid; the
annex's now stops at the last disputed leaf. Memory is §2's bound. The wire is unchanged for every
class that is not held; for a held class the one new meaning is the block request's number (D3), in
the build that first registers one.

**Measured on the real 1.5B weights (2026-09-12, one 12-core M-series host, release build).** The
held row (graph-v7) over `qwen25-1.5b-a16.palwart`, built at testnet-11's `2^26` ladder, running a
free-prompt job of 1,024 prompt positions and 4 decode tokens — **105,865,280 step leaves, past the
`2^26` = 67,108,864 the held producer refused before this ADR** — through every route
(`a_held_job_past_the_networks_ladder_on_the_real_dense_row`, env-gated):

| route | size | time |
|---|---|---|
| the producer's fold (the whole job) | material 4.24 MB | 91 s |
| interval 0 (512 positions): the executor's opening / the seat's verdict | 830 KB | 45 s / 45 s, Valid |
| interval 1 (512 positions): opening / verdict | 831 KB | 64 s / 46 s, Valid |
| interval 2 (3 decode steps): opening / the seat's anchor recompute / verdict | 1.11 MB | 9 s / 24 s / 0.4 s, Valid |
| a liar (a separate instance) with one wrong tile at leaf 79,010,838, inside interval 1: the seat's verdict | — | 46 s, `FaultInRange` at block 19,289 |
| the block, asked by its interval number 6,413 / the leaf named from it | 262 KB | 50 s / 8 s, leaf 79,010,838 |
| the liar's own fold builds its leaf's evidence / the one-move verdict | 16 KB | 4 s, `ExecutorGuilty` |

575 s end to end, a peak of 6.2 GB resident (the artifact, the model's cache and the process's
buffers; no vector of the job's leaves exists). The same drill runs at fixture scale in the default
suite, against a `2^12` ladder its job is past
(`a_held_liar_past_the_networks_ladder_is_named_and_convicted`). The liar is the dense tier's
drill fault, made in the fold's stream and replayed by the instance that lied
(`a16_execute_free_prompt_streaming_with_drill_fault_v1`; DRILL ONLY, the trait's contract), and
it moves exactly one committed tile (`the_drill_liar_moves_one_committed_tile_and_nothing_else`).

## 5. Invariants the tests hold

* `the_fold_range_root_is_the_consensus_root` — the segmented walk equals the consensus walk over
  every leaf, on trees, levels and ranges swept; a short or long path is refused as the consensus
  walk refuses it.
* `the_edge_blocks_give_the_spans_siblings` — the siblings from the two edge blocks and the retained
  nodes are the siblings from the whole span.
* `a_tree_deeper_than_the_default_leg_serves_its_honest_openings` — a tree past the default leg's
  depth opens at its own.
* `the_executors_own_close_is_the_capture_close` — a leaf's evidence from the fold is the capture
  close byte for byte, at every main-step leaf of a multi-interval job, with the tree kept at three
  small retain levels so a step's leaves and paths cross block edges.
* `a_held_executor_serves_blocks_by_their_interval_number_and_the_disputed_tiles_alone` — on the
  held row at the regime's ladder: the fold is at 12; every whole block of every interval is served
  by its number, past the first block included, and folds to its digest; the disputed tiles are the
  interval's own and nothing outside it; the shipped row's number is the block.
* `a_held_class_folds_at_twelve_and_every_other_class_at_its_ladders_level` — at `2^22…2^40`.
* `a_held_class_walks_at_its_ladder_and_materializes_at_the_networks` — built at `2^26`, the held
  row walks at `2^40` and materializes at `2^26`, the shipped row at `2^26` for both; the refusal is
  named between the two and the old refusal outside the class's ladder.
* `the_close_from_served_intervals_holds_and_asks_for_the_disputed_leafs_block` — the close's
  gathering names the block by its class's number.
* `a_held_liar_past_the_networks_ladder_is_named_and_convicted` — §4's drill at fixture scale: an
  honest held job past the network's ladder is Valid on its first, middle and last interval; a liar
  is `FaultInRange`, its block is served by number, the leaf is named, and the one-move court
  convicts on the liar's own evidence.
* `the_drill_liar_moves_one_committed_tile_and_nothing_else` — the drill's lie is one tile: the same
  answer and checkpoints, one retained block different, its own roots.
* `a_lie_in_a_block_that_straddles_an_intervals_edge_is_named_from_the_edges` — D5: a liar on the
  right edge of the middle interval is addressed at the left edge; that edge alone does not walk,
  the right one names the leaf and so do both, the block naming refuses an edge, and an honest
  executor's edges name nothing.
* `a_fault_at_an_intervals_edge_asks_for_both_edges_and_names_from_them` — the panel's fault arm
  asks for both edges and names through the edge verb before the whole-block path.

## 6. Supersession

ADR-0086 Decision 6 and ADR-0082 Decision 7 stand for every class that is not held. ADR-0119 §7's
first three items (the two knobs, the replays, the retain level) are closed here.

## 7. What is deliberately not decided yet

* **The resume opening's transport.** A resume carries the whole state at the interval's start over
  the same 4 MiB lane, and a class of `n_ctx` `2^21` always takes the Resume route; it needs its own
  transport, or chunks. The arithmetic, for the dense 1.5B row: the cache is 56 KiB a position
  (2 KV heads × 128 lanes × 4 bytes, K and V, 28 layers), so one 4 MiB answer holds the state of 73
  positions; cut into parts, the lane's per-peer serve budget (48 MiB a minute) moves about 880
  positions of state a minute from one executor. A seat resuming deep in a long job is therefore a
  question of a state-sync lane of its own and of which layers a seat holds (ADR-0099's shards),
  not of a larger cap on this one.
* **A fused site's dissection evidence past the network's ladder.** It is built from the honest
  re-execution's dense rows and the whole K/V history; a windowed builder (the rows the site reads,
  from the interval's anchor) is its own piece of work.
* **The worker frame and the gateway's prompt limit** (256 KiB, about 60,000 ids; 64 KiB of text),
  which bind only past the network's ladder.
* **An artifact's rotary width.** Found by §4's run: the shipped 1.5B artifact was converted at the
  512-position row, and its inventory — the root a class registers — commits a rotary row for each of
  those positions and none past them. A held class registered over that artifact can neither run nor
  be adjudicated past 512 positions, whatever its ladder (the engine refuses `PositionOutOfRange`;
  the court would open a row the root does not hold). A held class is registered over an artifact
  converted at its own `n_ctx` — 512 bytes of rotary table a position at the 1.5B head width, a GiB
  at `2^21` — and nothing on the chain or in the registration tooling checks it yet. The run
  extended the table with the same generator (its first 512 rows checked to be the artifact's own).
* **The served close's gathering, at an edge.** A challenger's close from served intervals (ADR-0085
  Decision 3) still holds the executor's leaves only for a whole block; a held class convicts through
  the leaf demand (ADR-0111) once D5 has named the leaf, so the close's edge is left for the classes
  that use it.

## 8. Number hygiene

Written as 0120 and renumbered the same day: the model-seed session claimed 0120 first (ADR-0119 §8
and the index say so). The next free number is 0122.

## Mission alignment amendment — 2026-10-07

streamed foldとtwo laddersは保存・再実行最適化として維持する。自分のFOLDから自分がopenできる試験に加え、別public bondが偽prefix/checkpointを公開openingから特定しcloseする試験を必須にする。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 不正の発見と、その後の客観的局所化・裁定の成立を分ける。honest seatの存在、raw sampling、選出outsider、quorumだけで全不正の検出を保証しない。ADR-0171の全constraint検査とresidual error、ADR-0172のversioned Kernel境界を維持し、巨大な無界replayやVM/TEE/BFT authorityを代替条件にしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0121-a-held-capture-is-served-from-its-fold-as-the-replay-streams-and-a-node-holds-two-ladders.md -->

</details>

<a id="source-0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it"></a>

<details>
<summary>0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md — 原文 L1–L120</summary>

[原文を開く](../adr/0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md) · SHA-256: `9e49f9bf364579866c5163717e8c93720b2f6860aa8e88041c042878f9bb7b91`

<!-- ADR_SOURCE_BEGIN:0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md -->
~~~~~~~~markdown
# ADR-0136: An artifact is mapped, not read, and a host holds one copy of it

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: PROPOSED 2026-09-17 on `feat/palw-exec-lane-and-validator-retirement`, with its first two decisions
implemented the same day. Node-side only: no consensus rule changes, no fingerprint moves.

## 0. The sentence this ADR is

A model's weights live in the kernel's page cache once per host; every producer and every seat on that host
reads the same pages and keeps only its own scratch, a replay starts only when the host has the memory to
finish it, and a seat that cannot replay a class proves nothing for it.

## 1. What was measured (2026-09-17, ADR-0135 drill on a 12 GiB VPS)

Seven fixture nodes, each holding the same 1.67 GiB Qwen2.5 A16 artifact (`--palw-class-artifact`), the
registry armed at DAA 20:

| reading | value |
|---|---|
| anonymous memory per node at rest, one node | 1.80 GiB (`RssAnon`; `RssFile` 3 MiB) |
| anonymous per node under the drill's first claims | 2.6–2.8 GiB (`RssAnon` + `VmSwap`) |
| the registering node (`--palw-register-class`) | 3.9 GiB |
| host after four minutes | `MemAvailable` 330 MiB, swap 8.4 GiB of 8.4 GiB, the OOM killer took `node-6` |

The cause is the dense container's loader: `std::fs::read` of the whole file, then a decoder that copied every
int8 tensor into its own `Vec` (`Base0ArtifactV1 { embed, unembed, layers[].wq .. w_down }`), so an artifact
was a private copy per process. The registering node paid twice more: the registration derives the A16
operand-inventory root by materializing every row's bytes (`a16_inventory_v1`) — a second 1.7 GiB — and the
allocator kept the freed arenas.

The same structure is why two 24 GiB Macs rebooted earlier the same day under seven and eight artifact nodes
(`eight-artifact-nodes-materialize-the-inventory-and-reboot-the-mac`): the streaming inventory (ADR-0106) had
removed one copy; the container's own copy remained.

## 2. Decisions

**D1 — the container's int8 slabs point into a read-only mapping of the file.** `Int8SlabV1` (owned or
`Mapped { map, offset, len }`) derefs to `[i8]`, so every reader keeps its slice and only the loader decides
where the bytes live. `decode_artifact_file_mapped_v1` decodes over `ReadOnlyMap` (the POSIX `mmap` already
written for Qwen3.6, ADR-0112) with zero copies of the weights; the digest is still recomputed over the whole
file once, which is the pass that pages it in. The dense lineage's `load` maps where the platform maps and
reads whole where it cannot (a non-POSIX host, as before). `MAP_PRIVATE` read-only: the pages are the page
cache's, shared by every process that maps the same file, and reclaimable under pressure instead of swapped.

**D2 — the registration root is streamed.** The A16 operand-inventory root at registration, in the class
context vector and in the e2e drill is `a16_inventory_digest_v1(...).root()` — one row's bytes alive at a
time, the same root (ADR-0106's streamed build is held to the materialized one by test).

**D3 — a replay respects the host's memory budget (node-local, never consensus).** Before a seat starts a
replay: `need = largest held artifact file + 512 MiB of scratch ≤ 70 % × (MemAvailable − 1 GiB)`; otherwise
the duty waits for a later tick, logged once a minute. Swap is not capacity: a host in swap finishes no replay.
The deadline still runs — a seat that never fits answers nothing, which the quorum prices as silence.

**D4 — readiness follows the budget.** A seat without the budget for a class proves no possession of it; the
standing proof expires by itself (ADR-0135's readiness age) and the class counts one seat fewer. The chain judges
the proof; the budget is the node's. Nothing about RAM enters consensus.

**D5 — the node reports what it holds.** After loading its artifacts a node logs PSS split into anonymous and
file-backed pages and what is swapped (`/proc/self/smaps_rollup`); `scripts/ops/palw-memory-census.sh` prints the
same split for every kaspad on a host, with `MemAvailable`. RSS alone misleads once files are mapped: a
mapped page appears in every sharer's RSS and in none of their anonymous memory. The reading that matters is
`Pss_Anon` per process and `Pss_File` summed over the host.

## 3. Results

Four nodes at rest, the same artifact, before and after D1 (`smaps_rollup`, MiB):

| | before: anon | before: file | after: anon | after: file (PSS share) |
|---|---|---|---|---|
| node | 1,830 | 6 | 178–211 | 437 |
| the registering node | 3,890 | 8 | 2,325 (D2 not yet in this binary) | 438 |
| host, summed | 11,255 + 8,300 swapped (7 nodes) | — | 2,923 | 1,752 = the file, once |

Seven nodes at rest after D1: anonymous 52–66 MiB per non-registering node, the file 1,752 MiB once for the
host, `MemAvailable` 9.3 GiB of 12 (before: 0.3 GiB and the OOM killer). **Under the drill's activity**
(phase 2, eight nodes, the class in PROBATION with eight ready seats, every seat replaying the floor's claims
and proving possession each span): 800–1,030 MiB anonymous per node — the scratch this ADR §4 names: retained
materials, KV caches, logits rows, the per-process compile — and the file still 1,753 MiB once (`Pss_File`
summed over eight processes), `MemAvailable` 9.4 GiB of 12. Before D1 the same eight nodes would have held
8 × 1.8 GiB of copies before any scratch. **D2 matters to every node, not only
the registrant**: on the D1-only binary, the moment the class registration landed, each node that resolved the
new class derived its operand-inventory root by materializing the rows — four nodes at 2.0–2.1 GiB anonymous
within a minute, 9.2 GiB on the host, swap in use — because the resolve runs on every producer tick and the
allocator keeps the freed arenas. With D2 the root is streamed on that path too. The completion condition the operator
set — four concurrent jobs do not multiply the artifact's physical memory by four — holds at rest by
construction (one file, one page cache); the reading under four concurrent replays is taken from the same drill
as its claims license (§7 records it).

## 4. What this ADR does not build, and the final form

* **One runtime per model per host** (the operator's design: a `misaka-model-runtime` holding Qwen2.5 ×1 and
  Qwen3.6 ×1, producer and seats asking it for jobs over a Unix socket). With D1 the weights are already one
  copy per host; what a shared runtime would still save is each process's own scratch (KV caches, retained
  logits rows, traces) and the per-process compile of the class program, and what it would add is an IPC
  boundary in the replay path and a second process to supervise. It stays the final form for a Kimi-class
  artifact whose scratch is itself large; it is not built here.
* **A host-global artifact cache manager** (content-addressed, pin the active classes, LRU-evict the rest).
  The page cache is the cache today; `ReadOnlyMap` carries `will_need`/`dont_need` for a pin policy when
  more than one large class shares a host.
* **Per-tensor scratch reuse inside a replay** and a measured (not estimated) per-class scratch for D3's
  `need`: the 512 MiB estimate is replaced by a measurement once the drill's replays report it.

## 5. Rollout

No consensus change: nothing in the fold, the carriage or the fingerprint moves. Every fleet node that holds a
dense artifact benefits on its next restart (the `.113` host's memory budget, ADR-0112's page-in counts). The
permissionless registry (ADR-0135) is armed only after this, because its readiness evidence assumes a host can
hold what it proves.

## 6. Number hygiene

0136 was free when written; the next free number is 0137.
~~~~~~~~
<!-- ADR_SOURCE_END:0136-an-artifact-is-mapped-not-read-and-a-host-holds-one-copy-of-it.md -->

</details>

