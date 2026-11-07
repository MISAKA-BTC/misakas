# ADR-v2-0016 — Artifact所有・モデルlineage・adapter

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0003](../rfc-v2/0003-model-onboarding-and-artifacts.md)

## 1. この文書の役割

artifactの同一性・所有者・lineageとadapterを、モデルの不変性を破らず整理する。

## 2. 統合した判断の読み方

### 2.1

classのgraph、lineのowner、claimが帰属するrootを区別し、過去の公開更新方式を履歴として保持する。[旧ADR-0088](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)

### 2.2

同じartifact rootの所有帰属を一意にし、異なるweightsのpermissionlessな競争とは区別する。[旧ADR-0143](../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md)

### 2.3

adapter classをparentとadapterの組として明示する案を記録する。同じIDのweightsを上書きする経路にはせず、ADR-0175の後続不変性と照合する。[旧ADR-0163](../adr/0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) [旧ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)

## 3. 維持する境界・後続改定

旧model improvement／root更新の記述は採用済み不変性や初期mainnet延期を上書きしない。継続公開の旧要求を全モデルの強制配布義務へ戻さない。

## 4. 隣接する判断との分担

登録は[ADR-v2-0015](0015-permissionless-model-registration.md)、Positionとの帰属は[ADR-v2-0021](0021-positions-and-membership.md)、廃止・延期の範囲は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) | L1–L509 | L20–L48 |
| [0143-an-artifact-root-has-one-owner-on-the-chain.md](../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md) | L1–L240 | L20–L23 |
| [0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md](../adr/0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) | L1–L87 | L15–L20 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing"></a>

<details>
<summary>0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md — 原文 L1–L509</summary>

[原文を開く](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) · SHA-256: `9b2675d2e90f499d5c4339aa8318afc20841567bd1de844a2cb8ba5561aaad5e`

<!-- ADR_SOURCE_BEGIN:0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md -->
~~~~~~~~markdown
# ADR-0088 — the class keeps its graph; a line keeps its owner, and the owner keeps publishing

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 現行決定 — 登録内容は永久不変

[ADR-0175](0175-registered-models-are-permanently-immutable.md)により、**新規登録は自由、既存登録の更新・上書き・置換は禁止**とする。V1→V2→V3を同じlineで発行する設計を廃止する。`ModelVersionPublished`（previewを含む）、`ModelVersionPromoted`、`ModelVersionWithdrawn`はwire tagとfence前の履歴処理を残し、独立した`palw_model_immutable_v1`以降は拒否する。新しいweights・tokenizer・実行/kernel/verification bindingは新しい独立登録にする。classのgraph IDと固定rootを含むmodel registration ID（`line_id`）を分け、同一graph上の別weightsにも別のPosition/AMMを作る。

運用上のowner/developer/maintainer変更、利用量、retirement・報酬資格はモデルの内容を変更しない。元の登録のroot、versionと識別情報を移動させない。新規則のfenceは全presetで休眠し、以下の旧決定・実装記録はfence前の互換仕様として保持する。「owner keeps publishing」「new weights are a new version」「market stays」は新規則の権限ではない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

**Status:** PROPOSED 2026-09-05, **revised the same day** — design only (no implementation yet).
The first draft of this number (commit `4f198f6e`, "the class keeps its graph, and the exam
names its weights") made the successor of a model's weights the verdict of an on-chain exam:
candidates proposed by anyone, questions drawn by the beacon from a permissionless pool of pure
programs, answers tried by the court, a chained index of accepted improvements. The operator
withdrew that direction on 2026-09-05: *strengthening a model is one developer's work, not a
protocol's* — "分散モデル学習ではなく分散モデル市場" (not distributed training, a distributed
market). A Qwen-class 27B model is trained by one team; what the chain should do is record every
version that team publishes, keep the old ones, count how each is used, and let the market price
the line. This text replaces the first draft under the same number, renamed to say what it now decides
(README §"Number hygiene"). The draft's reasoning is summarised in §9 so it can be found again.
**Requested by the operator:** one registered address is one model-developing entity (1 アドレス =
1 モデル開発主体); that address alone updates the model, V1 → V2 → V3, continuously; every version
is recorded in a Model Registry that is "a Git for models" (weights hash, runtime hash, dataset
commitment, training config, evaluations, inference statistics, position, price); users use the
model and PALW processes the use; the position is priced on the MISAKA EVM's Model AMM; a line
that the market rates highly has the strongest reason to keep improving; and the design should
be the one that makes strengthening *easiest* (より強化しやすい設計).
**Builds on:** ADR-0087 (positions on a curve), ADR-0056 Decisions 3, 5, 6 and 7 (a class IS its
graph; the registrant's bond; duplicates; what the chain does not judge), ADR-0067 (classes are
chain data, kernels are the build), ADR-0069 / ADR-0075 (certification is of kernels, never of
weights), ADR-0059 (carve, never mint), ADR-0065 (a bond is the chain's address-shaped identity
for a PALW actor).
**Amends:** ADR-0087 Decision 1 (the market is keyed by a *line*, of which the class's own is the
first — byte-identical for every class that has only its own), Decision 4 (the registrant leg is
the line's *owner's*, and the owner may share it with an adopted contributor), Decision 7 ("a new
version is a new class" is narrowed to a new *graph*; new weights are a new *version* of a line,
and the market stays); ADR-0056 Decision 6's admission clause ("an attempt whose `artifact_root`
differs is admission-rejected") becomes "differs from every root in force for the class".

> **Amended (2026-09-07, design).** [0095](0095-a-position-is-a-membership-not-an-income.md) makes a position in a line a **membership**: the line's *owner* declares up to eight tiers by held units over a grant set the chain closes, and the fold then refuses a promotion before the declared lead has passed and refuses a straight-to-current publish while a lead stands — so Decision 2's version paths gain two refusals and Decision 12's rows gain the declaration. **A line is not a company and a position in it is not a share of one: see [0087 §0](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md#0-a-position-is-not-a-share--it-is-how-a-models-usefulness-is-held), which is the canonical statement, and §2.1 below for what it means for a line.**

## 0. The sentence this ADR is

A class is its graph and keeps it; it is the unit of work, share, certification and the court,
and none of that moves when weights do. A **line** is a model in the market's sense — a class,
an owner, a name — and the owner's developer publishes its **versions**, one signed object each,
as often as it likes; nothing is examined and nothing is voted. The chain keeps every version,
counts how each is used, records what the developer and anyone else *declare* about it, and
prices the line on ADR-0087's curve. Learning is one team's; research is open (a proposal is a
record, and adopting it pays); competition is between lines; the judge is the market.

**And the market is a market in a model's usefulness, not in a company.** A line has an owner,
but the owner is a publisher of weights, not an issuer; a position in a line is not a share of
the owner's business, buys no income, no vote and no claim on anybody, and what it does buy is a
service from that line — the next version early, the private beta, the queue, the developer's
room — with the price moved only by the model actually being used. §2.1 states that in full, and
[0087 §0](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md#0-a-position-is-not-a-share--it-is-how-a-models-usefulness-is-held)
is the canonical version for the whole design.

## 1. What exists, and the wall this ADR goes through

* **A class IS its graph; the weights are a separate field written once.** `class_id ==
  profile.shape_profile_id()` — the borsh of the geometry, node tables included (ADR-0056,
  ADR-0067). `PalwClassStateV2.artifact_root` is written at the `ClassRegistered` arm and mutated
  by no path; the same graph with new weights is the same class and is refused `DuplicateClass`
  (ADR-0053's Family M dead end: "only a re-mint does"). So the operator's V1 → V2 → V3 on one
  model has no path today, and ADR-0087 Decision 7's "a new version is a new class" cannot be
  taken for weights at all.
* **Whose class it is, is already a chain fact.** `PalwClassStateV2.registrant_bond:
  Option<PalwBondKeyV2>` — "whose bond paid for this class to exist" (ADR-0056 Decision 3),
  `None` exactly for the classes a genesis registered. A bond is the chain's address-shaped
  identity: an ML-DSA-87 key, a collateral outpoint, a `payout_payload` the coinbase pays
  (ADR-0065). ADR-0087 already pays the registrant leg to that payload. This is the "registered
  address" the operator means, and it is post-quantum; an EVM address is its window (ADR-0089),
  never the owner.
* **Certification is of kernels, never of weights** (ADR-0069 Decision 2; ADR-0075 §4: "a
  fixture drill certifies kernels, not weights"). `pwu_per_inference`, share, budget, target
  seed, exposure, the court's ceilings: all functions of the graph. A new set of weights on a
  certified graph runs on the certified kernels and needs no drill, no binding and no code change.
* **The chain judges no quality** — ADR-0056 Decision 7, kept and now fully respected: "the gate
  checks *adjudicability*, never *quality* … a benchmark is an oracle." What the chain can count
  is *paid work*: every accepted claim names a class, an executor and a root, and the fold can
  attribute it to a version.
* **Where a claim's root lives.** An attempt claim carries `artifact_root` and admission compares
  it with the class's (`consensus/core/src/palw_admission_v2.rs:252`); a free-prompt claim
  carries none (`consensus/core/src/palw_fp_admission_v3.rs` never reads it) — the class has one
  root, so it never had to; the court's operand openings prove against `class.artifact_root`
  (`consensus/core/src/palw_state_v2.rs:9467`).
* **The market of ADR-0087 is per class, implemented** (`f8d54d9b`): `model_markets` and
  `model_positions` keyed by `class_id`, the rows in the state root only when non-empty, the
  carriage's tagged tail, `ModelBuy`/`ModelSell`, `palw_model_market: Option<ForkActivation>`,
  `None` everywhere.
* **The class's name is off the chain.** The worker manifest's `model_id` (`Qwen/Qwen2.5-1.5B/
  graph-v2`) is a catalog string; the profile carries no name and no tokenizer identity. A
  line's name is therefore a new chain fact, and the line's identity must not depend on a name
  alone (a name is squattable; an owner is not).

## 2. The requirement, and the shape that answers it

The operator's list, in this ADR's words:

| the operator's step | the chain's object |
|---|---|
| ① one address = one developing entity | a **line** has one **owner** bond; the founding line of a class is its registrant's |
| ② the developer strengthens V1 → V2 → V3 | `ModelVersionPublished`, signed by the line's **developer** bond, as often as it likes |
| ③ every version is recorded, none deleted | `PalwModelVersionV1` rows — root, parent, declared metadata, chain-counted usage — a bounded window in state, the whole history in the explorer |
| ④ users use the model; ⑤ PALW processes the use | claims name their root; the fold attributes each accepted claim to `(line, version)` and counts it |
| ⑥ the position is priced; ⑦ the AMM discovers the price | ADR-0087's market, keyed by line; ADR-0089's EVM face |
| ⑧ a highly-rated line has the strongest reason to keep going | the registrant leg is the owner's, per trade, for as long as the line trades — and an adopted contributor's share of it, per version |

And what makes strengthening *easy*, which the operator asked for beyond the list:

* **No gate but a signature.** A version is one object under one fence; no exam, no drill, no
  re-binding, no re-registration, no code change, no rent beyond carriage. The graph is
  certified; the weights are the developer's.
* **A preview before a promotion.** A developer can publish a version as a *preview*: its root
  is in force (executors may run it, users may ask for it, the chain counts it) while the line's
  *current* version stays what it was. Real usage of V(n+1) is measured before V(n+1) is made
  the default — A/B on the chain's own counters, then `ModelVersionPromoted`.
* **Rollback is a publish.** Re-publishing an older root as a new version is legal and cheap;
  the history says so.
* **A grace, so producers never fall off.** After a promotion the previous current root stays
  admissible for `PALW_VERSION_GRACE_DAA_V1`, so a producer that has not fetched the new
  artifact keeps producing.
* **Open research with attribution and pay.** Anyone with a bond may attach a **proposal** (a
  root and a note) to a line; the developer evaluates it off the chain, and if the next version
  adopts it, the version says so and the owner's chosen share of the registrant leg goes to the
  proposer for as long as that version is current. "学習は中央化、研究はオープン化" with a
  ledger.
* **Cold owner, hot developer.** The owner bond names a developer bond (and a maintainer); the
  key that publishes daily is not the key that can transfer the line. A stolen developer key can
  publish junk, which the market sees and the owner revokes; it cannot take the line.
* **Competition on one architecture.** More than one line may live on one class — Developer B
  founds `QWEN-B` on the same graph as Developer A's `QWEN-A` — and the market compares them.
  The class's share and cadence are the class's; a block is a block whichever line's root it ran.

### 2.1 A line is not a company, and its position is not a share of one

The operator's ⑥ and ⑦ above — "the position is priced", "the AMM discovers the price" — are the
two steps most easily misread, so this ADR says plainly what a line's position is. The canonical
statement is [0087 §0](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md#0-a-position-is-not-a-share--it-is-how-a-models-usefulness-is-held);
what follows is what it means once the market is keyed by a *line* rather than a class.

* **A line's owner is a publisher, not an issuer.** "1 アドレス = 1 モデル開発主体" makes the
  owner the one address that may publish versions of this model. It does not make the owner a
  company that sold something: the owner receives no proceeds from a position — a buy's net leg
  goes into the pair's reserve, never to a person (0087 Decisions 3 and 4) — and owes a holder
  nothing the fold does not itself enforce. The owner's only income from the market is the 1 % leg of
  each trade (Decision 8), **paid by the trader for the work of keeping the line alive**, not out
  of any holder's return, and it stops the day the line stops trading.
* **The seed is not capital, and nobody subscribed to it.** A market opens because somebody locked
  at least 100,000 BILI into the pair for good ([0090](0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
  Decision 2, accumulated over as many transactions as it takes —
  [0094](0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)). The seeder gets no position
  for it and can never take it back. It is the pair's floor, not a share issue, and there is no
  raise, no round, no allocation and no register of subscribers anywhere in this design.
* **What is bought is the model's use, and a service on top of it.** A line's position is the
  place where the model's own usefulness is held: every claim that ran this line is attributed to
  it (Decision 4), and five percent of the worker reward that use earned buys the line's pair and
  retires what it buys ([0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)). On top of
  that, [0095](0095-a-position-is-a-membership-not-an-income.md) lets the line declare what its
  holders get — the next version's artifact before it may be promoted, the private beta, priority
  in the queue, experimental modes, the developer's room — from a set the fold closes, in which
  **there is no bit that pays**. A holder buys access to what the model can do, and never a share
  of what anyone earns.
* **Competition between lines is competition between products, not between issuers.** Two lines
  may live on one class (Developer B's `QWEN-B` beside Developer A's `QWEN-A`) and the market
  compares them on usage the fold counted and on service the fold can check. Nothing here is a
  claim on either developer's business; a line that stops shipping simply stops granting anything
  ([0095](0095-a-position-is-a-membership-not-an-income.md) §4.6), on its own, with nobody filing
  anything.
* **The registry's rows are provenance, not a prospectus.** The weights hash, the runtime hash,
  the dataset commitment, the training config, the evaluations and the usage counts exist so a
  user can tell which bytes they are running and how much the network ran them. Where a row is a
  *declaration* the chain labels it as declared (Decision 5) and never endorses it; nothing in the
  registry is an offer, a projection, a forecast or a promise of a price, and no reader — the
  explorer, the wallet, misakaoptions.com — may present it as one.

**Words.** These are **positions** in a **line**, held by **holders**, conferring a **membership**;
never *shares*, *stock*, *equity*, *securities*, *dividends*, *investors*, *株*, *株式*, *配当* or
*出資*. As in 0087, this section states the design's properties, each checkable against the fold;
whether they satisfy a given jurisdiction's definition is counsel's question, not this ADR's.

## 3. Decisions

**Decision 1 — A line is `(class, owner, name)`; the class's own line is the first, and its id
is the class id.** `PalwModelLineV1 { line_id, class_id, owner: Option<PalwBondKeyV2>, developer:
Option<PalwBondKeyV2>, maintainer: Option<PalwBondKeyV2>, name: Vec<u8> (≤ 64), founded_daa,
current: u32, previews: Vec<u32> (≤ PALW_MODEL_PREVIEWS_V1 = 2), versions_published: u32,
contributor_permille_of_leg: u16, status: Active | Retired }`. Every class has one **founding
line** — `line_id = class_id`, `owner = registrant_bond`, `name` = the class's catalog name as
the registrant wrote it, version 1 = the registration's `artifact_root` — which exists
implicitly: no row is written until something about it changes (a version, a role, a retire), so
a chain where nothing happened has no line rows and ADR-0087's per-class market is, byte for
byte, the founding line's. A genesis class (`registrant_bond = None`) has an unowned founding
line: one version, no developer, no way to publish — the genesis card may name an owner bond
for a genesis class at the next re-mint (ADR-0075 §7's card gains `genesis_line_owners`), and
mainnet, born floor-only with every model registered later (ADR-0075 Decision 8), never has an
unowned model line. A further line on a class is founded by `ModelLineFounded { class_id, name,
founder: PalwBondKeyV2, root, signature }`: `line_id = H("misaka-palw/model-line/id/v1" ‖ class_id
‖ founder ‖ name)` — the owner is in the id, so a name is never squatted, only shared; at most
`PALW_MODEL_LINES_PER_CLASS_V1 = 64` lines per class; the founder becomes owner, developer and
maintainer; version 1 is `root`. The floor (`is_base_class`, no artifact) has no line.

**Decision 2 — A version is one signed object, and the developer signs it.**
`ModelVersionPublished { line_id, version, root, parent: Option<u32>, adopted_from:
Option<Hash64>, runtime_hash: Option<Hash64>, dataset_commitment: Option<Hash64>,
training_config_hash: Option<Hash64>, notes_hash: Option<Hash64>, preview: bool, signature }`,
signed by the line's developer bond over `palw_model_version_message_v1(network_domain, line_id,
version, root, parent, adopted_from, the four hashes, preview)` under
`PALW_MODEL_VERSION_MLDSA87_CONTEXT`. Refused unless: the line exists (or is a class's implicit
founding line with an owner) and is Active; `version == versions_published + 1` (versions are
dense and monotone — the "commit number"); the developer bond is Active; `parent` names an
existing version or is `None`; `adopted_from` names a proposal of this line; `root` is not the
current root (a no-op version is refused; re-publishing an *older* root is allowed — that is a
rollback); a preview is refused when the line already holds `PALW_MODEL_PREVIEWS_V1` previews.
The fold writes `PalwModelVersionV1 { root, parent, adopted_from, runtime_hash,
dataset_commitment, training_config_hash, notes_hash, published_daa, published_by, status:
Current | Preview | Superseded { until_daa } | Withdrawn, usage: PalwVersionUsageV1 }`. When
`preview` is false the version becomes **current** at once and the previous current becomes
`Superseded { until_daa: published_daa + PALW_VERSION_GRACE_DAA_V1 }`. `ModelVersionPromoted {
line_id, version, signature }` (developer) turns a preview into the current the same way;
`ModelVersionWithdrawn { line_id, version, signature }` (developer) takes a preview out of force
at once, or a superseded version out of force before its grace ends — a current version cannot
be withdrawn, only succeeded. The declared hashes are **declarations**: the chain checks their
length and nothing else, records who signed them, and labels them so in every surface (README
principle 4: the chain never takes the host's word — here it does not pretend to).

**Decision 3 — The roots in force for a class are the union of its lines'.** For a class at
`daa`: the founding `artifact_root` if the class has no line rows; otherwise, over every Active
line of the class, its current version's root, its previews' roots, and the roots of superseded
versions whose `until_daa > daa`. An attempt claim is admitted iff the root it names is in that
set (ADR-0056 Decision 6 item 5, restated); the set is bounded by lines × (1 + previews + 1). The
fold records, for every claim admitted while the class has line rows, the root it named
(`claim_roots: claim → root`, a state-root collection, removed with the claim), and the court's
operand openings prove against `claim_roots[claim]` when present and `class.artifact_root`
otherwise (`consensus/core/src/palw_state_v2.rs:9467`, amended). **A free-prompt claim names no root today**; under
this fence the free-prompt job gains `artifact_root` in its next version (v6), and until that
version ships a free-prompt claim is attributed to the class's founding line's current root and
replayed against it — a recorded gap, not a rule (§8).

**Decision 4 — Usage is counted by the fold, per version, and it is the one measurement the
chain makes.** `PalwVersionUsageV1 { attempt_claims: u64, fp_claims: u64, work_leaves: u128,
first_used_daa: Option<u64>, last_used_daa: Option<u64> }`, incremented at every accepted claim
that names (or is attributed to) the version's root, in the same arm that admits it. A voided
claim (court fraud) is subtracted at the voiding. These are paid inferences — ADR-0072's "the
ticket is the execution" — and nothing else; they say what was *used*, never how *good* it was,
which keeps ADR-0056 Decision 7 whole.

**Decision 5 — Evaluations are declarations, from anyone, and say who declared them.**
`ModelEvaluationPosted { line_id, version, evaluator_id: Hash64, score_permille: u32,
report_hash: Hash64, by: PalwBondKeyV2, signature }`, signed by `by`. At most
`PALW_MODEL_EVALUATIONS_PER_VERSION_V1 = 16` per version, first come; one per bond per version.
The fold stores `(evaluator_id, score_permille, report_hash, by, posted_daa)` and marks whether
`by` is the line's developer or maintainer (the line's own word) or a stranger's. No rule reads a
score. The explorer shows them beside the usage counters and the price, which together are the
"benchmark" the operator asked for as an indicator: what the developer says, what strangers say,
what was used, what it trades at — four columns, none of them a consensus verdict.

**Decision 6 — Roles: owner, developer, maintainer; and the owner may hand the line over.**
`ModelLineRolesSet { line_id, developer: Option<PalwBondKeyV2>, maintainer:
Option<PalwBondKeyV2>, contributor_permille_of_leg: u16, signature }` (owner) — `None` means
"the owner"; `ModelLineOwnerTransferred { line_id, new_owner: PalwBondKeyV2, signature }` (owner;
the new owner's bond must be Active; developer and maintainer are reset to the new owner);
`ModelLineRetired { line_id, signature }` (owner): the line leaves Active, its market is
`closed_to_buys` (ADR-0087 Decision 7's rule, now per line), its roots leave force after the
grace, its history stays. The registration exposure of ADR-0056 Decision 3 stays with the
registrant's bond whatever the owner does: it is the registration's rent, not the line's
title. Nothing here moves a position: ADR-0087 Decision 5 stands, and the operator's own rule —
development rights may transfer, positions may not — is exactly that.

**Decision 7 — Proposals: open research, recorded, and paid when adopted.** `ModelProposalPosted
{ line_id, root, note_hash: Hash64, by: PalwBondKeyV2, signature }` (any Active bond, at most
`PALW_MODEL_PROPOSALS_PER_LINE_V1 = 32` open per line, first come; the developer may
`ModelProposalClosed { line_id, proposal_id, signature }` to make room): `proposal_id = H(line
‖ root ‖ by)`. When a version's `adopted_from` names a proposal, the fold records the adoption
on both rows and, for as long as that version is current, pays `contributor_permille_of_leg` of
the line's registrant leg to the proposer's payout payload (Decision 8). The developer evaluates
proposals off the chain, in its own environment, on its own data — which is where a 27B model's
evaluation happens anyway; the chain records the candidates, the choice and the credit.

**Decision 8 — The registrant leg is the owner's, by line, and shared when the owner says so.**
ADR-0087 Decision 4's 1 % leg on every BILI leg of a line's market goes to the line's **owner**
bond's payout payload — the registrant's, for a founding line whose ownership never moved, so
ADR-0087's arithmetic is unchanged; burned when the line has no owner (a genesis line), as
ADR-0087 already burns it. When the current version was adopted from a proposal and the owner
has set `contributor_permille_of_leg > 0`, that share of the leg goes to the proposer's payload
and the rest to the owner's. The market row gains `owner_paid_sompi` and `contributor_paid_sompi`
beside `registrant_paid_sompi`'s meaning (the field is kept and now means the owner's total).
Nothing else in the split moves: 5 % burned, 1 % the leg, 94 % net, 12 % round trip.

**Decision 9 — The market is keyed by line, and the founding line's key is the class id.**
ADR-0087's `model_markets` and `model_positions` are keyed by `line_id`; `ModelBuy`/`ModelSell`'s
`class_id` field is renamed `line_id` (the fence is `None` everywhere; no chain carries the
objects). For a class with only its founding line every value is identical to ADR-0087 as
written. A buy on a non-founding line requires the line to exist and be Active; a sell never
requires anything but the units. Each line gets its own facade address on the EVM (ADR-0089).

Keying the market by a line changes what a position is *about* and nothing about what it is:
the thing priced is one publisher's model rather than a bare graph, and it is still bought from
the curve and sold back to it, still untransferable, still owed nothing by anyone. **A position
in `QWEN-A` is not a share of the entity that publishes `QWEN-A`** — it is a membership in that
line and a place in its usefulness, and §2.1 is where this ADR says so.

**Decision 10 — State, and no root bump.** New collections: `model_lines: BTreeMap<Hash64,
PalwModelLineV1>`, `model_versions: BTreeMap<(Hash64, u32), PalwModelVersionV1>` (the last
`PALW_MODEL_VERSION_HISTORY_V1 = 64` per line stay in state; older rows are evicted at the 65th
publish and live in the explorer — `versions_published` on the line keeps the count monotone),
`model_proposals: BTreeMap<Hash64, PalwModelProposalV1>`, `model_evaluations: BTreeMap<(Hash64,
u32, PalwBondKeyV2), PalwModelEvaluationV1>`, `claim_roots: BTreeMap<Hash64, Hash64>`. Each
enters the state root only when non-empty and the carriage carries them in a second tagged tail
(`0x88`) after ADR-0087's, so a chain with no line row commits the root and the bytes it commits
today (ADR-0087's implementation rule; the root rides in the header and may not move).
`PALW_STATE_V2_VERSION` stays at 20.

**Decision 11 — A consensus rule armed by activation, never by regenesis.** `palw_model_lines:
Option<ForkActivation>` on the params, top level, bare; below it the ten objects are refused by
name at acceptance and the class has one root; past it the founding lines exist and the objects
apply. Independent of `palw_model_market`: a registry without a market is useful, and a market
without lines is ADR-0087. The fingerprint moves only where the flag is set. Rents: a line
founding, a proposal and an evaluation pay `PALW_MODEL_OBJECT_RENT_SOMPI_V1` (§4) burned by
ADR-0075 SA-2's don't-mint mechanism, so the bounded tables cannot be filled for free; a version,
a promotion, a withdrawal, a roles change, a transfer and a retire pay carriage only — the
developer's daily objects are not taxed, which is the point.

**Decision 12 — What a participant reads and does.** RPC `getPalwModelLine(line_id)` (the row,
the current and preview versions, the roots in force), `getPalwModelVersion(line_id, version)`
(the row with usage and evaluations), `getPalwModelLines(class_id)`, `getPalwModelProposals
(line_id)`; `getPalwModelMarket` and `getPalwModelPositions` keyed by line. CLI: `misaka palw
line found|show|log|roles|transfer|retire`, `misaka palw version publish|promote|withdraw|
show`, `misaka palw proposal post|close`, `misaka palw evaluate`. The explorer's Model page is
the operator's picture: `QWEN-27B-001 · V1 ── V2 ── V3 ── V4 ← CURRENT`, each commit with its
hashes, its declared and stranger evaluations, its usage, and the line's curve beside it. The
class SDK's `add-model` flow gains `--line` and `--version`: a developer registers a class once,
then publishes.

## 4. What this costs, stated before it is measured

* **Objects:** a version ≈ 64 (line) + 4 + 64 (root) + 4 × 65 (hashes) + 4,627 (signature) ≈
  5.1 KB; a founding ≈ the same plus the name; a proposal/evaluation ≈ 4.9 KB. All under the
  standard mass; no chunking.
* **Fold:** O(1) per object; O(lines × 4) per attempt admission for the roots-in-force set
  (≤ 256 hashes per class at the caps); one map write per accepted claim for the usage counter.
* **State:** a line row ≈ 300 B; a version row ≈ 400 B (64 kept per line ⇒ ≤ 26 KB per line);
  a proposal ≈ 200 B (≤ 32 open); an evaluation ≈ 200 B (≤ 16 per kept version); `claim_roots`
  one 128 B row per live claim of a class with lines. A network with 20 lines and full histories
  holds ≈ 1 MB, the size of one ADR-0075 drill.
* **Rents (operator's numbers):** `PALW_MODEL_OBJECT_RENT_SOMPI_V1 = 1 MSK` for a founding, a
  proposal, an evaluation — the tables they fill are bounded and the rent is what makes the
  bounds cost something to reach; a version pays carriage only.
* **Latency:** a version is in force in the block that accepts it; a promotion the same; the
  grace for the superseded root `PALW_VERSION_GRACE_DAA_V1 = 4,000` (≈ 2.8 days at one block a
  minute) is how long a producer has to fetch the new artifact.
* **The fee table is ADR-0087's** — 5 % burned, 1 % the leg (owner, or owner + contributor),
  94 % net.

## 5. Security — the four principles, checked before it is built

*A free field is a free draw; silence is not a verdict; weight is what certification buys; the
chain never takes the host's word* (README §"Security amendments").

| # | attack | what stops it, and the residual |
|---|---|---|
| A1 | **A developer publishes broken or malicious weights as current.** | Producers keep the previous root through the grace; a preview lets them (and users) try first; the market and the usage counters are the judgment; the owner can withdraw a preview and succeed a current. A bad model costs its line, not the chain: weight and share are the class's and unchanged (principle 3). |
| A2 | **A stolen developer key.** | It can publish and promote; it cannot transfer, retire, set roles or the contributor share — those are the owner's, which the design keeps cold. The owner re-sets the developer; the junk versions stay in the history as a record of the theft. Residual: a stolen *owner* key is a stolen line, as a stolen key is a stolen anything. |
| A3 | **Name squatting / impersonation** (`QWEN-27B-001` founded by a stranger). | The line id includes the founder; two lines may share a name and the explorer shows the owner beside it; nothing routes by name. |
| A4 | **Filling the bounded tables** (lines, proposals, evaluations). | Per-class / per-line / per-version caps, first come, and a burned rent on the filling objects; the developer can close proposals. Residual: a well-funded party can hold a line's 32 proposal slots for 32 BILI until closed — the developer's slots to clear. |
| A5 | **Declared hashes that lie** (dataset, config, evaluation). | They are declarations, labelled as such, signed by a bond that can be named; no rule reads them; they are the explorer's and the market's to weigh (principle 4). |
| A6 | **Redirecting the fee leg.** | It follows the owner, and the owner moves only by the owner's signature; positions never move (ADR-0087 D5). |
| A7 | **A claim naming a root that is not in force.** | Refused at admission, as a wrong `artifact_root` is today; the set is bounded and read from the fold. |
| A8 | **Attributing free-prompt usage to the wrong version** (no root on the fp claim). | Recorded gap (Decision 3): counted on the founding line's current until job v6 ships; the court's replay for such a claim uses that root — an executor that ran a preview on the fp lane before v6 is one whose claim the court may misjudge, so the SDK refuses fp jobs on non-current roots until v6. |
| A9 | **A version that does not fit the graph** (wrong tensor shapes). | Executors cannot run it: no claims, no usage; the developer's problem, visible in the counters. |
| A10 | **Two lines racing on one class for the class's cadence.** | There is no race: the class's share and budget are the class's; every accepted block adds to it whichever line's root ran. Lines compete for users and for the market, which is what they are for. |

## 6. Invariants the tests must hold

* **L1 (one owner).** A line has exactly one owner; the founding line's is the registrant; a
  transfer is signed by the current owner; positions are untouched by any role object.
* **L2 (dense versions).** `versions_published` increases by one per publish; version `n` is
  refused unless `n == published + 1`; the history window holds the last 64 and the count never
  decreases.
* **L3 (roots in force).** `class_roots_in_force(class, daa)` equals the union of Decision 3
  over the class's Active lines; an attempt naming any other root is refused; a superseded root
  leaves the set at `until_daa`; a withdrawn preview leaves it at once.
* **L4 (attribution).** Every accepted claim that names a root increments exactly one version's
  usage; a voided claim decrements it; `claim_roots` has a row for every live claim of a class
  with line rows and none for a class without.
* **L5 (the leg).** ADR-0087 M1–M8 hold with the key renamed; the owner's and the contributor's
  legs sum to the registrant leg; a genesis line burns it; a transfer moves it from the next
  block.
* **L6 (declarations are inert).** No transition reads a score, a dataset commitment or a
  runtime hash; a property test over the fold with random declarations finds the same state
  root for the same roots and usage.
* **L7 (bounds).** Lines per class, previews per line, proposals per line, evaluations per
  version, versions kept per line — each refused at the cap.
* **L8 (replay and revert).** The deltas replay to the same root and revert; the carriage of a
  chain without line rows is byte-identical to ADR-0087's; with rows it carries the `0x88` tail
  and a legacy reader refuses it rather than misreading it.
* **L9 (the fence).** The fingerprint is unchanged where `None`; the ten objects are refused
  below it; the founding line answers the same values below and above it for a class whose
  registrant never published.
* **L10 (court).** A court against a claim of a superseded or preview root replays against that
  root after the grace and after later versions.

## 7. Order of work

1. State: lines, versions, proposals, evaluations, `claim_roots`; the founding line's implicit
   row; the roots-in-force reader; the usage counters at admission and voiding; the tail; L2–L8.
2. Objects and arms: the ten objects, their messages and contexts, acceptance-layer signature
   checks (the bond's stored key), rents; L1, L7.
3. Admission: `palw_admission_v2` against the set; the court's operand root from `claim_roots`;
   L3, L10.
4. Market: the key rename, the owner/contributor legs; L5.
5. Params: the fence, the fingerprint pin; L9.
6. RPC, CLI, the SDK's `--line/--version`, the explorer page.
7. The free-prompt job v6 (`artifact_root` on the job) — its own change, closing A8.
8. Devnet drill: register a class, publish V2 as preview, produce on both roots, promote,
   post a proposal, adopt it in V3 with a contributor share, transfer the line, retire it; then
   testnet-11 by activation.

## 8. Implementation record (2026-09-05, `palw-adr0088-0089-impl`)

**Landed — §7 items 1–6 (state, objects and arms, admission, market, params, RPC and CLI);
item 7 (the free-prompt job v6) and item 8 (the devnet drill) are not, nor the SDK's
`--line/--version` and the explorer page.** The fold half is commit `bf4ed1d8`; the rest is the
branch's closing commit.

| where | what |
|---|---|
| `consensus/core/src/palw_model_lines_v1.rs` | The constants (64 lines a class, 2 previews a line, 64 versions of history, 32 proposals a line, 16 evaluations a version, a grace of 4,000 DAA, a name of at most 64 bytes, a rent of 1 MSK on a founding, a proposal and an evaluation); the rows (`PalwModelLineV1`, `PalwModelVersionV1`, `PalwModelProposalV1`, `PalwModelEvaluationV1`); `founding_line_v1` — the class's own row synthesised, never written: its id the class id, its owner the registrant, its version 1 the genesis root; `model_line_id_v1` (class, founder, name — so a name is shared, never squatted) and `model_proposal_id_v1`; `split_owner_leg_v1` (Decision 8); the ten messages under ONE ML-DSA-87 context, `misaka-palw-model-line-v1`. |
| `palw_state_v2.rs` | `model_lines`, `model_versions`, `model_proposals`, `model_evaluations`, `claim_roots` on the state, **in the root only when non-empty** (Decision 10 — the same shape ADR-0087 §7 taught); the carriage tail `0x88`; delta variants with replay and revert; the ten objects appended after `ModelSell`; the fold arms — a founding pays rent, needs an active bond and a name unused on the class; a publish is the dense next version with its parent in the history, at most two previews, an adopted proposal marked; a promotion opens the grace on the root it supersedes; a withdrawal refuses the current; roles, transfer and retirement are the owner's; proposals and evaluations are any active bond's; `class_roots_in_force(class, daa)` and `model_version_of_root`; usage counted on the version a claim named (`note_claim_usage`) and uncounted when the claim is voided; `apply_palw_transition_v7` taking `PalwTransitionExtrasV1 { model_lines_active, .. }` (`v6` stays for callers that carry no extras). |
| `palw_admission_v2.rs` | Decision 3: an attempt's `artifact_root` must be one of the class's roots in force at the block's DAA, else `ArtifactRootMismatch`. |
| `palw_lifecycle_objects_v2.rs` | Ride gates for the ten objects; a buy's sink binding keyed by line. |
| `palw_model_market_v1.rs` | `contributor_paid_sompi` beside `registrant_paid_sompi`; the 1 % leg split owner/contributor by the line's permille. |
| `config/params.rs` | `palw_model_lines: Option<ForkActivation>` requiring `palw_model_market`; `palw_model_lines_fence` / `palw_model_lines_active_at`; pinned in the fingerprint; `None` on every preset. |
| `virtual_processor/processor.rs`, `utxo_validation.rs`, `body_validation_in_isolation.rs` | The ten objects refused by name below the fence at the block's DAA; each signature verified at acceptance against the pubkey the bond registry stores for the bond the object names, and the role checked against the line's row (the owner for roles/transfer/retire, the developer for a publish or a move, any active bond for a proposal or an evaluation); carrier fees collected once the fence is armed. |
| `kaspad/src/palw_panel.rs` | The ten names in the panel's transcript. |
| RPC | `getPalwModelLine(lineId)` (the row, its roots in force, its current root), `getPalwModelVersion(lineId, version)` (the row with its evaluations), `getPalwModelLines(classId)`, `getPalwModelProposals(lineId)` — ops 173–176 through core, service, gRPC and wRPC; `getPalwModelMarket` and `getPalwModelPositions` re-keyed to `lineId`; the integration test's round trips. |
| CLI | `misaka palw line-show`, `line-log`, `line-list`, `proposals` (readers, `--json`); `line-found`, `version-publish [--preview]`, `version-promote`, `version-withdraw`, `line-roles`, `line-transfer`, `line-retire`, `proposal-post`, `proposal-close`, `evaluate` — each signs the fold's own message with the bond's key, checks the key owns the bond the row names, and sends nothing without `--yes`. |

**Tests.** `palw_model_lines_v1` (6): a line id carries its founder; the founding line is the
registrant's with the roles defaulting to the owner (L1); a version is in force by its status and
the grace (L3); usage counts paid work and a voiding takes it back (L4); the leg is the owner's
unless the owner shares it (L5); every message binds every field. `palw_state_v2::tests::model_lines`
(8): the founding line is the registrant's and a publish moves the root with a grace (L1, L2, L3);
a preview is in force but not current until promoted and a withdrawal takes it out (L3); roles are
the owner's to set, a transfer resets them, a genesis line has nobody (L1); a proposal is adopted by
a version and the owner shares the leg (L5, L7); usage is counted on the version a claim named and
admission reads the roots in force (L4, L3); a founded line shares the class and the roots are the
union (L3, L7); evaluations are declarations from anyone and say whose they are (L6); the carriage
stays legacy until a line row exists and the deltas replay and revert (L8). The params test pins L9
(fingerprint unchanged where `None`, the ten refused below the fence). L10 is the court reading
`claim_roots`, covered by reading. The consensus-core suite is green at 1,868.

**What the implementation taught.** (1) The founding line is best *synthesised* rather than
written at activation: a class registered before the fence has a line the moment the fence is
armed, with no genesis edge and no object. (2) A free-prompt claim names no root today (the job
carries the class only), so its usage is attributed to the founding line's current version until
job v6 carries `artifact_root` — recorded as the open item it is, not hidden in a default.
(3) One signing context for all ten objects is enough because every message spells its own kind
first; ten contexts would have been ten places to get a domain wrong. (4) The rent is charged on
the three objects anyone may post (founding, proposal, evaluation) and on nothing an owner posts
about its own line: the owner already paid to own it.

**Not landed.** The free-prompt job v6 (§7 item 7); the SDK's `--line/--version`; the explorer's
line and version pages; the registry half of the devnet drill (§7 item 8: publish as preview,
produce on both roots, promote, propose, adopt, transfer, retire — the 2026-09-05 model-market
drill, ADR-0089 §9, exercised only the founding line's read and the market on both lanes); arming
on testnet-11. As with ADR-0087, the
acceptance-layer signature checks are covered by reading — a processor-level fixture for a signed
lifecycle object still does not exist.

## 9. What the first draft decided, and why it was withdrawn

model の次の weights を consensus exam で選ぶ案は採用しない。syllabus を書く主体と資金力で評価を偏らせられ、ユーザーと developer による model 選択を chain の verdict に置き換えるため。developer が weights を選び、chain は version・root・grace を記録する現行方式を採用する。

## 10. What is deliberately not decided

* Every `_V1` constant: previews per line, lines per class, proposals, evaluations, versions
  kept, the grace, the rent. The operator sets them when the flag is armed; §4 carries the
  examples.
* The genesis card's `genesis_line_owners` — whether the RC's genesis model classes get owners at
  the next re-mint or are re-registered by their maintainers as post-genesis classes.
* Whether a maintainer should also be able to *withdraw* a preview (an operational safety
  valve) — today only the developer can.
* A per-line *service* fee (a share of a claim's escrowed reward to the owner, for the use of
  the model) — a different ADR: it touches ADR-0042 Decision 10's carve and the coinbase, not
  the market.
* Whether a line may move to a *new graph* (a wider context, a new architecture) and keep its
  market — ADR-0087 Decision 7 stands for a new graph; a line-level graph migration is a market
  re-keying with its own ADR.

## 11. Number hygiene

This is ADR-0088, revised in place on 2026-09-05 before it landed anywhere but its own branch.
The README's next free number was 0088 after 0087's row; it became 0089 with this row and 0090
with ADR-0089's. The text lives on `palw-adr0088-0089-impl` (branched from
`palw-adr0084-served-answer` at `c303b3b9`, the tip that holds ADR-0087's implementation); the
draft's branch `docs/adr-0088-succession-exam` is superseded and should not be merged.
~~~~~~~~
<!-- ADR_SOURCE_END:0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md -->

</details>

<a id="source-0143-an-artifact-root-has-one-owner-on-the-chain"></a>

<details>
<summary>0143-an-artifact-root-has-one-owner-on-the-chain.md — 原文 L1–L240</summary>

[原文を開く](../adr/0143-an-artifact-root-has-one-owner-on-the-chain.md) · SHA-256: `af51e3d1c0e88ee070108c0131828db4b17660bae0b778b95e3db233d68dc805`

<!-- ADR_SOURCE_BEGIN:0143-an-artifact-root-has-one-owner-on-the-chain.md -->
~~~~~~~~markdown
# ADR-0143 — An artifact root has one owner on the chain, and competing weights stay permissionless

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## 2026-10-09 改定 — 永久不変なrootと独立モデルの一意性

[ADR-0175](0175-registered-models-are-permanently-immutable.md)を適用する。登録したcanonical artifact rootはそのモデルについて永久不変である。同一graphでも異なるweights/rootなら別model registration IDを得られる。`artifact_owners[(class_id, root)]`の一意性と旧class間で同じartifactを利用する規則は維持する。他モデルのrootの奪取、version更新によるroot置換を認めない。運用停止・retirementは登録定義を削除しない。

旧version evictionとroot解放のコードはfence前の履歴互換用である。新規則ではversion発行・withdrawを拒否するため、固定登録のrootをその経路で再利用可能にしない。休眠復帰は同一内容に限り、root ownershipと元の登録日時を維持する。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

**Status:** IMPLEMENTED 2026-09-18, **withdrawn from the 7,100 flag day and REWORKED** after the
adversarial audit of 2026-09-19 found two defects in the index this ADR installs. Both are fixed on
`fix/audit-2026-09-19`; `palw_artifact_root_ownership` stays `None` on every preset and arms on its
own day, after a drill crosses its own fence.

**What the audit found, and what the rework changes.**

**The uniqueness was global across classes, and that was wrong on this system's own terms.** §3
argued "a root is a file of weights; two classes carrying the same one are the same duplicate". But
`class_id` is `PalwShapeProfile::shape_profile_id()`, which hashes `n_ctx`, `n_batch`, `n_ubatch`,
`n_threads` and the runtime flags — so one artifact legitimately carries several classes, and
`@512` and `@2048` are two class ids over one root. The retired positional lookups were
CLASS-SCOPED and attributed both correctly; global keying refused the second class outright, a
configuration the chain accepted the block before. Worse, it let a stranger reserve any public
model's root — the root is computed offline from a downloadable file — for one line founding in
anybody's class, permanently and silently, because the acceptance filter drops the refused
registration and lets the block stand. **The index is now keyed `(class_id, root)`**, which closes
the defect this ADR was written for — a copy line inside ONE class taking that class's founding
root, which is what testnet-11 has — and restores the cross-class behaviour that was never in
question.

**The index was unbounded.** It is state-rooted and rides the pruning carriage, and it had no cap,
no eviction and no TTL while `PALW_MODEL_LINES_PER_CLASS_V1`, `PALW_MODEL_VERSION_HISTORY_V1`,
`PALW_MODEL_PREVIEWS_V1` and `PALW_MODEL_PROPOSALS_PER_LINE_V1` all bound their tables. A
publication with a fresh root bought a permanent row for a transaction fee, and the row outlived
the version: `model_versions` evicts past 64 per line and the owner row did not. §6 asked what the
CROSSING block costs and never asked what the steady state costs. **An owner row now lives exactly
as long as a version row that carries it**, so the index is a function of `model_versions` and
`classes` and inherits their bounds; a class's registered founding root is never released, because
the class row is its carrier.

Both are pinned by tests that reproduce the defect first — the front-run with a control, and the
boundedness by publishing past the history window and watching the index not grow with it.

**Builds on:** ADR-0087 (a position is never money settled), ADR-0088 (model lines and versions),
ADR-0091 (the reward buys the pair), ADR-0095 (a position is a membership).

## 1. The defect

Attribution asks "which line does this artifact root belong to", and the answer is decided by
iteration order:

```rust
// model_version_of_root / model_line_of_root, today
for (line_id, line) in self.model_lines.iter().filter(|(_, l)| l.class_id == *class_id) {
    …
    return Some(*line_id);   // ← the FIRST match, in BTreeMap id order
}
```

Two things are wrong and each needs its own fix.

**The chain admits duplicates.** Nothing stops a second line, founded by anyone, from publishing a
version whose root is already another line's — including the class's own founding root.

**The resolution is positional.** With duplicates admitted, which line collects depends on which
`line_id` sorts first in a `BTreeMap`. That is a consensus-visible outcome decided by a hash's byte
order, which is the kind of meaning no rule should carry.

The founding fallback does not save it. It fires only when `model_lines` holds no row *keyed by the
class id at all*, so a third party who founds a line first takes the class's own founding root, and
the class's fallback is disabled from then on.

Live on testnet-11: the Qwen2.5 class's founding root `1a7457f1…` is also carried by a copy line,
and the copy resolves first.

## 2. What this does NOT restrict

ADR-0088's competition stays exactly as it is. **Different weights on the same graph remain
permissionless**, by anyone, without the class registrant's permission:

```
Class A · graph Transformer-X · founding root R1 · registrant Alice

  Line A   R1   ← Alice's founding line, canonical owner of R1
  Line B   R2   ← Bob's fine-tune              ALLOWED
  Line C   R3   ← Carol's independent weights  ALLOWED
  Line D   R1   ← already owned by Line A      REFUSED
```

Only the *exact* root collides. A near-duplicate — a re-quantisation, a conversion, a one-byte
change — is a different root and therefore a different artifact, and the chain does not look inside
it. Judging semantic similarity would put a content opinion inside consensus, which this registry
exists not to have. Near-duplicates are a marketplace and reputation problem, and §8 is where they
are addressed.

## 3. Decision

**D1. An artifact root has one owner, and the chain stores it.** Rooted state gains

```rust
artifact_owners: BTreeMap<Hash64, PalwArtifactOwnerV1 { class_id, line_id, version }>
```

Uniqueness is **global, not per class**. A root is a file of weights; two classes carrying the same
one are the same duplicate the rule exists to refuse, and a per-class index would let the collision
move one level up and keep its ambiguity. The index nevertheless records the class, and every reader
passes the class it is asking about, so a root owned by another class's line answers "not yours"
rather than paying the wrong pair.

**D2. The positional lookups retire.** `model_version_of_root` and `model_line_of_root` stop walking
lines and read the index. Past the fence there is no "first match" to be decided by id order.

The line comes from the index; the **version** is then resolved *inside that one line* — the lowest
version of the owning line carrying the root and in force, and the index's own recorded version when
the line has no rows at all, which is how a class's synthesised founding line answers. A scan inside
a single line is not a walk across lines: no id order can reorder it. **Whether a root is in force is
not moved by this ADR** — `class_roots_in_force` still decides admission, and it decides it exactly
where it did.

**D3. One source for every attribution.** Usage attribution, ADR-0091's buyback, the owner fee and
version lookup all read the index and nothing else. A second way to answer the question is how the
two answers come to differ.

There were **three** call sites, not the two §1 shows. `uncount_claim_usage` — the subtraction a
voided claim makes — carried its own copy of the first-match walk, and one that never filtered by
force while the counting side always did, so on a line that withdrew and republished a root the two
could already land on different rows. Past the fence it asks the counting side's own question with
the claim's own accept height, so the subtraction lands where the addition did.

**D4. A founding root is reserved at registration, atomically.** `ClassRegistered` writes the class
row and the ownership of its founding root in one transition. The fallback that depended on "no line
row exists yet" is deleted with the window it opened: there is no moment when a class's root is
registered and unowned.

**A registration on a root another line already owns is refused**, with the same
`DuplicateArtifactRoot`. The alternative — admitting the class and leaving it unable to own its own
root — would create a class whose own attribution is paid to a stranger, which is the defect itself
rather than a milder form of it. A registrant who meets this has a remedy the chain need not
provide: a different artifact is a different root.

**D5. Every entrance refuses a duplicate.** `ClassRegistered`, `ModelLineFounded` and
`ModelVersionPublished` all reject an owned root with `DuplicateArtifactRoot`, through one helper.
Fixing a single call site leaves the others open, so the rule is written once and every path where a
root enters state is audited against it.

A line republishing a root **it already owns** is not a duplicate and keeps the row it has. The
question the index answers is *which line*, and moving the version on a republication would rewrite
where the claims that already counted point.

**D6. Activation canonicalizes what is already there, deterministically.** At the fence the index is
built from existing state by a total order:

1. a root equal to its class's registered founding root → the **founding line** wins, always;
2. otherwise the **earliest accepted version** wins;
3. a tie → deterministic id order.

Rule 1 is the one that matters: it is what returns testnet-11's Qwen2.5 root to the class that
registered it.

**D7. Legacy duplicate rows stay.** They are historical record and the chain does not rewrite
history. They simply stop being the answer: past activation they attract no usage, no buyback and no
owner fee for a root they do not own.

**D8. Nothing settled before the fence is recomputed.** Past payouts and buybacks are final, the
12,816 BILI already attributed included. The fence changes attribution from the fence, and no earlier
state root moves.

**D9. Its own fence.** `palw_artifact_root_ownership`, independent of the 7,101 bundle, so a failure
in either is one failure domain. It is dormant on every preset here.

## 4. Why the index and not a rule against duplicates alone

Refusing duplicates from the fence would leave the ones already on the chain resolved positionally
for ever, and would leave two ways to answer the question — the index for new roots, the walk for old
ones. D6 canonicalizes the existing rows *into* the index, so after activation there is exactly one
mechanism and one answer.

It also closes a race the announcement itself would otherwise open: between announcing a fence and
reaching it, a squatter could take a root that has no line row yet. D4 removes the window and D6
resolves anything already in it in the founding line's favour.

## 5. What must be proved

* the index answers before and after activation, and the answer never depends on id order;
* the legacy migration is deterministic, and founding-root precedence beats an earlier third-party
  version;
* a duplicate is refused at `ModelLineFounded` **and** at `ModelVersionPublished`;
* a class registration reserves its founding root in the same transition that writes the class;
* a reorg restores the index exactly, and a delta revert equals a fresh walk;
* a snapshot carries the index, and a node that loads one answers as a node that folded the chain;
* usage attribution and buyback attribution both read it, asserted separately;
* a different root on the same class is still accepted, from a bond that is not the registrant's —
  ADR-0088's competition, pinned so this rule cannot quietly eat it.

## 6. What the crossing block costs, and the one thing to measure before arming

The migration runs **before any object of the block that crosses the fence**, in the fold and in the
acceptance filter's pre-object base alike, so admission and the fold cannot disagree about whether
the index exists yet — and a block that crosses the fence and founds a line in the same breath is
judged by the canonicalised index rather than by the empty one it arrived at. Ordered the other way,
a squatter with a fast node would have had one block of warning to take a root the migration was
about to return.

That block writes one delta entry per distinct root, and nothing else. The count is bounded by the
state, not by a constant: at most one root per class plus one per version row, with
`PALW_MODEL_LINES_PER_CLASS_V1` (64) lines a class and `PALW_MODEL_VERSION_HISTORY_V1` (64) versions
kept per line. **The structural ceiling is therefore large enough to be worth measuring rather than
assuming**, which is this ADR's own lesson from the DAA-clock audit.

**The gate before arming is one number: the roots on the target network at the arming height.** It
is read from the chain, it belongs beside the launch runbook's §5c drill gate, and it decides
nothing else — a chain with a few hundred roots crosses in one ordinary block, and a chain with
hundreds of thousands needs the question asked again before a height is picked.

**Measured on testnet-11, 2026-09-19 at DAA 6,263: five classes, six lines, and SIX distinct
artifact roots.** The block that crosses 7,102 writes six delta entries. The ceiling was worth
computing; the answer is that this chain is nowhere near it, and the number is in this document so
the next reader takes the measurement rather than the ceiling.

## 7. What this does not decide

Whether the fence is armed, and at what height. That is the operator's, after a drill crosses it.

## 8. The part that is not consensus

Near-duplicate weights are a real problem and this ADR deliberately leaves them to the surface that
can judge them. A model page can say which line is the class's founding one, which are independent
weights, and which is a legacy duplicate row — `Registered by`, `Canonical founding artifact`, `This
line owner`, `Artifact relationship`. **Attribution is never resolved there**: the chain answers it,
and the page displays the chain's answer.
~~~~~~~~
<!-- ADR_SOURCE_END:0143-an-artifact-root-has-one-owner-on-the-chain.md -->

</details>

<a id="source-0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed"></a>

<details>
<summary>0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md — 原文 L1–L87</summary>

[原文を開く](../adr/0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md) · SHA-256: `296356fa72a6b61fe09629d265fb7a31cc1433e1d27879127940591250c5a6d7`

<!-- ADR_SOURCE_BEGIN:0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md -->
~~~~~~~~markdown
# ADR-0163 — An adapter class is its parent plus an adapter, and is listed

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-10-03 (RFC-0001 §2.10, lane U); **IMPLEMENTED the same day on `rfc1/serve`**, behind its own
  dormant fence `Params::palw_adapter_class_v1` — `None` on every preset and in no testnet-12 flag-day list, so no network's
  fingerprint, schedule or fold moves.
* Builds on: RFC-0004 §6.3 (composite artifacts, PALW-MIP-15; `palw_improve_composite_v1`), RFC-0002 (the IR class, its
  registration and its court), ADR-0075 (a class is a consensus object).
* Supersedes nothing.

## 0. The sentence this ADR is

A LoRA is not a new model: a class that is exactly its parent's inventory plus an adapter section is registered the way any IR class
is, and is then **listed** — one signed object that records the composite reference in the registry the court's composite openings
and a seat's possession proof both read — so an adapter costs a section, not a re-commitment of the model.

## 1. What exists, and the gap

RFC-0004 already built the composite: `artifact_root(Composite) = H(parent_class ‖ parent_root ‖ adapter_root ‖ le32(P))`, a candidate
lowered with its adapter's params last (`misaka/palw/tir/lower/src/lora.rs`, `lower::adapter_params_last`), its admission
(`palw_tir_candidate_artifact_admits_v1`: the family rule, the composite rule, every terminal close carriable in the composite form) and
the registry `improvement_composite_classes` (delta 99) that a court opening and a seat's adapter-section possession proof read. But the
only door to that registry is `CandidateSubmitted` (tag 80): a governed model line's epoch, its fees, its bar and its pool. A person who
wants to serve a LoRA over a registered class is not entering a competition; they have no line, and should not need one.

## 2. Decisions

1. **A listing object, tag 94: `AdapterClassListed { payload: PalwAdapterClassListingV1 { class_id, artifact: PalwTirCompositeRefV1,
   layout }, lister: PalwBondKeyV2, signature }`.** The class is a registered IR class (its `ClassRegisteredTirV1` was priced and
   admitted as any registration is, with the composite root as its artifact root); the listing adds the reference. Tag 94 is declared
   explicitly (spec 17 §17.0: a lane asks the core lane before taking a tag; 91–93 are RFC-0006's and RFC-0007's, in flight on their
   branches, and 94 is the next one free after them. If the core lane assigns it differently the number is the only thing that moves).
2. **Acceptance is the composite candidate's, without the line.** The lister's signature (ML-DSA-87, context
   `misaka-palw-adapter-class-v1`, over the network domain, the payload and the lister — so it is neither replayable across networks nor
   liftable onto another bond); then `palw_tir_candidate_artifact_admits_v1` over the class's record and the parent's, with full weights
   NOT allowed (a listing is for adapters; a full-weight class is a registration). It sizes an IR program, so it shares the block's one
   place at admission sizing with IR registrations and composite candidates (the walk drops a second by name, the block standing).
3. **The fold is the cheap second lock, and writes one row.** Before the write, every refusal: the lister an Active bond; the class
   registered and IR; the reference's artifact root the class's registered one; the parent a registered IR class whose registered root is the
   reference's `parent_root`; `p > 0` and the class not its own parent; the class not already listed. The write is
   `write_improvement_composite_class` — the one writer of the registry, journaled `ImprovementCompositeClass` (99), so replay and revert
   already cover it. Nothing else moves: no bond is locked, no pool is touched, no reward exists. Spam control is the registration's own cost
   (a class must be registered, and a registration is priced), plus one row per class and the sizing slot.
4. **Seat possession is the existing record.** A seat proves possession of a composite over its adapter section against the root this
   registry names (RFC-0004 §6.7, `improvement_composite_classes_v1`, the node lane's read); the parent's section is the parent's
   inventory, which the seat already holds. Listing therefore makes the adapter class a class seats can be asked about, and nothing in
   readiness changes: a derived class is a class with a possession proof over two sections.
5. **Dormant behind `palw_adapter_class_v1`.** Prerequisites, refused by name by `validate_palw_v2`: `palw_tir_v1` and
   `palw_improvement_v1` in force at or below it (the machinery lives behind both); a ConsensusV2 network; the bundle's mirror equal to the
   fence (`sync_palw_adapter_class_v1`, as every fold-read fence). Below it the acceptance walk drops the object by name and the fold refuses
   it (second lock). Written Some-only into the params and schedule ids, collapsed from `Some(never())`, named in `palw_fences_v1` and the
   fork-id probe; armed only by a salted drill entry (`--palw-drill-adapter-at`).

## 3. What this does not do

* It does not make an adapter servable: the node's worker must hold the parent and the adapter section and run the composite program, and
  the court's composite opening needs `palw_improvement_v1` armed. This ADR is the registry and the gate.
* It does not price an adapter class differently from its registration, and it adds no reward to the lister.
* A governed line's candidate still enters the registry through tag 80 and is untouched.

## 4. Evidence

`consensus/core/src/palw_adapter_class_v1.rs` (payload, message, form), `palw_adapter_class_fold_v1.rs` (the fold and its refusals, replayable
through the delta), `consensus/core/tests/palw_rfc1_fences.rs` (dormancy, Some-only fingerprint, `never()` collapse, prerequisites by name,
in no t12 list), and `t12_an_adapter_class_listing_is_gated_by_its_fence_its_signature_and_its_acceptance` at the processor (below the fence
dropped by name; armed, a stranger's signature refused, a signed listing of an unregistered class refused by the acceptance half). The
composite admission itself is RFC-0004's, exercised on real PEFT fixtures in `misaka/palw/sdk/tests/improve_composite_lora.rs`.

## Mission alignment amendment — 2026-10-07

adapter classはparentに加えadapter weights/versionを公開認証し、adapterによる誤計算も外部bondがlocalize/convictできるprofileを必要とする。parent certificateだけでadapter部分の新gateを免除しない。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0163-an-adapter-class-is-its-parent-plus-an-adapter-and-is-listed.md -->

</details>

