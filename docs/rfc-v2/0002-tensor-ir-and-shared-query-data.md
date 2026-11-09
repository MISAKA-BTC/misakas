# RFC-v2-0002 — TIR・Kernel・共有Queryデータ

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。初回commit済みの区間境界を含む全kernel関係を有界なnative checkerへ接続する。整数の型・scale・丸め・overflow・演算順序を固定し、未知kernelや未分解の巨大演算を受理しない。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。native実行のInit・Step・Endと全relation familyの認証・完全性は16/19に従う。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

決定論的TIR、leaf符号化、演算の意味は本書が所有する。[16](0016-miner-execution-commit-and-material-supply.md)のcanonical trace、[17](0017-noninteractive-positive-verification.md)のroot一致、[19](0019-model-free-node-and-bounded-evidence.md)の有界な局所primitiveが同じ型・形状・丸め・位置を参照する。Nodeに全programや全重みの無制限読み込みを要求しない。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0002](../rfc/0002-palw-tensor-ir.md)、[旧0029](../rfc/0029-owner-independent-shared-tir-query-data.md)。

**関連RFC:** [01 推論・生成Job・入出力](0001-inference-and-generative-jobs.md) / [03 モデル登録・変換・Artifact](0003-model-onboarding-and-artifacts.md) / [05 資源上限・長文脈メモリ・計測](0005-bounded-resources-and-long-context.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [14 Court統合契約](0014-court-master-contract.md) / [15 設計凍結・延期・不採用・未採択・変更管理](0015-freeze-deferred-rejected-and-governance.md)。

## 1. TIRの正本を一本にする

旧0002を基盤とし、旧0029のowner非依存共有query/dataを本RFCへ統合する。TIRの型、演算、整数意味、静的検査、commitment、reference evaluator、versioned kernel、query identity、認証済みデータ搬送を同じ文書で読めるようにする。

モデル取得・変換・登録の手順は03、計測時のメモリ最適化は05、courtの開廷・義務・時計・判決・金融効果は14へ分離する。TIRの演算定義をCourt側にもう一度定義しない。

## 2. Canonical programと実行意味

`TirProgramV1`、型・shape・range・資源上限、tensor／layoutの拘束、命令ごとの正規意味を維持する。reference evaluatorが定める意味と、最適化kernelの実装を分ける。高速化やfused実装は、登録済み計算関係を変更する理由にならない。

静的admissionは、実行可能性・資源・検証可能性を確認する入口であり、全推論の正当性を先に証明する入口ではない。versioned kernelによる拡張を維持し、廃止された汎用ML VMや計算validity proofを復活させない。

## 3. 共有Queryと公開Data

旧0029のclosed query scope、署名済みrequest intent、公開認証データ、source-data上位要素、bounded stream、prefix／EOF、rooted query、数値disclosureを収録する。

queryの同一性と誰が提出権を持つかを混同しない。owner枠を占有した者の協力がなければ正直な外部検証者がデータを使えない構造にしない。共有データの取得と、literal signerが自分の応答義務を果たしたという事実は別である。後者の帰責は14が扱う。

## 4. CommitmentからCourtへ渡す境界

TIR側は、program／tensor／trace／parameter／dependencyのidentity、opening、局所計算の入力関係を供給する。どの不一致で誰を有罪とするか、供給不履行をどう扱うかは14に一元化する。Merkle openingの成功だけで算術の正しさ、義務の履行、支払解除を作らない。

## 5. 互換性と受入

旧wire・署名domain・program identity・kernel versionの意味は変更しない。未知version、別root、別query、途中の欠落、改変、余分なbytes、資源上限超過を拒否する。reference／最適化実装の一致、query重複、全owner占有、復元後の同一性は、それぞれ独立の検査対象とする。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0002 | 1–455 | [RFC-0002: PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor prog](#source-v1-0002-L00001) |
| 旧0002 | 505–731 | [7. Fused kernels (built last)](#source-v1-0002-L00505) |
| 旧0002 | 1245–1417 | [II.5 Quantisation as data (R3)](#source-v1-0002-L01245) |
| 旧0002 | 2079–2295 | [II.12 Local-LLM coverage: active kernels and an explicit extension queue (R8/A8)](#source-v1-0002-L02079) |
| 旧0002 | 2315–2357 | [14.8 Generic bounded preparation of real parameter commitments](#source-v1-0002-L02315) |
| 旧0002 | 2384–2645 | [14.11 Requested-claim identity and actual-weight node delivery path](#source-v1-0002-L02384) |
| 旧0002 | 2803–2854 | [14.21 Staged flat-v4 commitment material for large withheld values](#source-v1-0002-L02803) |
| 旧0002 | 2929–2980 | [14.23 Staged all-dtype internal flat-v5 material and fault kinds](#source-v1-0002-L02929) |
| 旧0002 | 3098–3373 | [14.26 Dormant flat-v5 public hash-unit demand, service and default](#source-v1-0002-L03098) |
| 旧0002 | 3410–3515 | [§14.33 — Protected candidate reads from one anchored public snapshot (2026-10-11)](#source-v1-0002-L03410) |
| 旧0002 | 3587–3876 | [§14.36 — Explicit v7 native candidate registration (2026-10-11)](#source-v1-0002-L03587) |
| 旧0002 | 3906–4059 | [§14.44 — Bounded authenticated external stage inputs, component v12](#source-v1-0002-L03906) |
| 旧0002 | 4093–4367 | [§14.49 — Rooted native execution headers and staged request funding, v11](#source-v1-0002-L04093) |
| 旧0002 | 4421–4748 | [II.14.57 Independent original credit delivery — audit32/redemption33](#source-v1-0002-L04421) |
| 旧0029 | 1–224 | [RFC-0029 — owner-independent shared TIR query data](#source-v1-0029-L00001) |

---

<a id="source-v1-0002-L00001"></a>

### 旧RFC-0002 / 原文1–455行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

### RFC-0002: PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **適用時期の現行方針（2026-10-17 文書化）:** 本書が参照する画像・動画生成および RFC-0004 の改善に限り **mainnet 後へ延期・当面無効**。初期 mainnet の必須実装・提供・有効化に含めず、mainnet 開始による自動有効化もしない。共有のテキスト・固定登録・検証・会計は維持する。過去の Status・実装・試験・全面有効化計画はこの範囲の有効化承認ではない。[正式な範囲・無効化要求・再開条件](../governance/post-mainnet-scope.md)。今回 node コードは変更しておらず、runtime の無効化は未確認。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Draft — implementation in progress: Phases A–C passed Gate 1 on 2026-09-28 (see *Implementation status*) |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry), 04/04a (execution, arithmetic), 05 (canonical work), 08 (verification), 09 (court), 14 (node), 16 (fences) · all networks (dormant until armed) · `consensus/core` (admission, court), `misaka-palw-base0` (engines), `misaka-palw-sdk`, `misaka-palw-extension` |
| Working branch | `codex/rfc02-implementation` (2026-10-10 revision); the original text-only branch was `rfc/0002-tensor-ir` |
| Related | ADR-0135 (a model is data; Decision 7 the kernel boundary, historically called VM), ADR-0038 A4 (100 % catalog coverage), ADR-0040 (BASE-0; Decision E order-free accumulation), ADR-0047 (A16), ADR-0049 (adjudication contract), ADR-0052 (QWEN36 ops), ADR-0057 (backends below the semantic boundary), ADR-0069 (e2e adjudicability), ADR-0082/0093 (fused attention and dissection), ADR-0102 (fenced kernels), ADR-0103/0116 (held context, history bound), ADR-0145 (canonical work), ADR-0171/0172 (new verification and kernel-only extension) |
| Part II | Model onboarding (header-only preflight, streaming loader, runtime packs, quant registry, feature scope, staged enablement) — appended 2026-10-01; §II.7.5 records decided rules, not yet built. §§II.10–II.12 add Hub evidence, future-family extension and local-LLM ≥90% coverage; revised 2026-10-06 to remove the VM residual, all open acceptance work |

#### Current requirements and historical records — 2026-10-10

The accepted revision in [§II.14](../rfc/0002-palw-tensor-ir.md#ii14-completion-contract--2026-10-10) applies the 2026-10-10
design review: complete the public compiler interface, preserve efficient sparse/stateful computation,
derive bounded resources, and bind the whole computation through public verification and terminal court.
It keeps the deterministic tensor IR and versioned kernel boundary. It does not declare any unfinished
implementation, model, resource envelope, coverage target or activation gate complete.

| Current requirement | Authoritative sections | Historical material retained below |
| --- | --- | --- |
| Typed bounded tensor/state graph; **25 frozen v1 primitives**; reusable versioned extensions when necessary | spec 04b and §II.14.2 | The original 30–50 primitive estimate and Gate 1 development sequence |
| Reference/independent/backend bit equality; separate source-checkpoint fidelity | §1.2, §II.4–II.5, §II.14.3 | Architecture fixtures and old conversion measurements prove only their recorded scope |
| Approved probabilistic whole-claim verification, bounded dispute replay and public conviction | Current design boundary, §II.13, mission amendment, §II.14.6; RFC07/14 | Routine full replay in the original admission/verification profile |
| Model acquisition is outside consensus; fixed model identity and claim-specific evidence remain required | ADR-0177 and §II.14.6 | Old model-availability/serving gates are not reinstated |
| Resource ceilings, canonical work and bond-bounded economic credit are separate quantities | ADR-0176, §II.14.4–II.14.5 | Historical PWU measurements are not authorization to enlarge B/R/F rights |
| Completion needs the public extension path and real-checkpoint staged evidence | §II.10–II.12, §II.14; [implementation ledger](../design/palw/tir/rfc0002-implementation.md) | Old “not yet built” annotations record their date, not a current code audit |

#### Current design boundary — 2026-10-06

[ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
selects versioned kernels and bounded declarative plans, not BVM/GVM or a universal VM.
TIR is a typed tensor/state graph with bounded scan, not an uploaded ISA or arbitrary verifier.
Existing layer-template code sometimes called a “canonical VM” is a kernel evaluator. New combinations
of active primitives need a frontend and plan; missing semantic, memory, checker or court relations
require a reusable kernel extension and coordinated activation. “SegWit-class” means versioned
extension, not automatic soft-fork compatibility; unknown rewarded operations fail closed.

For new large-model profiles, [RFC07 Part V](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md)
and [RFC11](../rfc/0011-permissionless-model-and-long-context-onboarding.md) govern ordinary verification:
bind encoded constraints, perform approved small probabilistic checks covering the whole claim,
then require positive receipts, DA and the challenge window before Final. Exact bounded cone replay
is the dispute backstop, not compulsory routine replay of every selected segment. Admission checks
plan structure, coverage and bounded verifier/court resources using kernel rules; it does not prove
a future inference by recomputing it. Reference evaluators, integer semantics and legacy static-
admission/replay profiles below remain conformance and historical baselines. This amendment changes
no live class or active checker. §§II.11–II.12 replace the former residual-GVM route; ≥90% coverage
remains an unmeasured acceptance target.

#### 概要(日本語)

- **目的。** active Kernelで表現できる新アーキテクチャの登録を、モデル名ごとのcoreリリース待ちから解放する。
  新モデルの全演算・taskがactive Kernelで表現できれば、HF config → lowerer → Canonical IR / plan → 登録へ進められる。
  本当に新しい演算は汎用Kernel更新を待つ。モデル名ごとのallowlistやVM fallbackを使わない。
- **今の境界。** canonical kernel evaluator は「層テンプレート + 45 個の kernel id」(`PalwShapeProfileV3`)で、GDN・fused attention・router の
  ような **層単位の kernel がそのまま consensus の命令** になっている。新しい種類の演算は kernel 追加 = リリース = protocol upgrade。
- **v1 の完成条件は「モデル」でなく「アーキテクチャ群」(§1)。** Dense・GQA/MQA/sliding・MoE・線形 attention(GDN)・SSM(Mamba/Mamba2)・
  recurrent(RWKV)・hybrid・routing 変種の代表モデルを **全部いったん分解し**、モデル固有 op を 1 個も足さずに表現できる最小の
  primitive 集合を逆算してから v1 を凍結する。Gate 1と正式仕様04bのv1は25個で、30〜50個は当初の見積り。
  必要な共通意味論は新versionで拡張できる。Qwen3.8(K 16 / V 48)は適合テストの1本にすぎない。
- **なぜ MISAKA で現実的か。** 汎用 VM の最難関は数値の決定性だが、MISAKA は既に **整数のみ**(ADR-0040)。丸めは名前付きの
  lossy site だけにあり、**丸め・saturation・shift・除算を含まず、全中間値が証明済みの範囲に収まる区間**(order-free region, §3.2)では
  加算順序が結果を変えない。規則は一つ: **fused kernel は order-free region の中だけを並べ替えてよく、lossy site を跨いだ融合で
  値を変えてはならず、commit 点の値は 1 bit も変えてはならない。** この下では GPU(int8 dp4a / IMMA / Metal)も許容誤差なしの厳密 backend になる。
- **有界。** ループ・ジャンプ・データ依存 shape は無い。時間方向の再帰は「位置軸上の bounded scan」(§4.2)として一箇所だけ。
  登録時に型・shape、範囲(overflow・不正 shift・0 除算・範囲外 index・過大確保が無いこと)、コスト(計算・メモリ・状態・court 最悪値)、
  court が 1 tile を上限内で裁けることを **すべて静的に証明** し、できなければ登録拒否。
- **court はモデルを知らない。** 知っているのは IR node・入力 commitment・param・出力 commitment・canonical 意味論だけ。争いは
  参照インタプリタで cone を 1 つ再計算して終わる。A4(カタログ 100 % カバー)は **primitive 集合に対して一度だけ** 満たせばよい。
- **ビット一致の範囲。** 参照インタプリタ == 独立の第 2 実装 == 出荷する全 backend はビット一致を要求する。HF の浮動小数モデルとの
  比較は **忠実度指標**(固定評価集合での top-1 一致率・KL・perplexity 差、ファミリーごとの閾値)で、ビット一致ではない(整数量子化は
  意図的に出力を変えるため)。
- **順序。** 意味論を固定し、参照実装・範囲／コスト検証・lowerer・checker・courtを揃える。
  高速実装と証拠生成の性能測定は並行し、実サイズで実現不可能なloweringを後から発見しない。
  既存kernelはIR patternのfused実装としてビット一致する範囲で利用する。
- **工数感。** 2〜4 人で 6〜9 か月。既存の Qwen3.6-35B(16/32)も踏んでいる GDN の k≠v 不具合の修正は、本 RFC とは別の小さな
  修正として独立に進められる(§7 Phase 0)。
- **名称と「VM ではない」こと(09-28)。** PALW-TIR は有限の静的グラフ + 静的な層スケジュール + 位置方向の scan だけで、実行中の
  分岐・ループ・ジャンプ・呼び出しは無い。reference evaluator はそれを評価するだけで、program counter を持つ VM ではない。
  題名を Canonical ML IR から **PALW Canonical Tensor IR** に改めたのはこの誤解を避けるため(crate 名 `misaka-palw-tir` は元から一致)。
- **frontend とコンパイラ基盤(09-28 評価)。** 主経路は HF の `config.json` + `transformers` の modeling コード → lowerer。
  ONNX・GGUF は v1 凍結後の任意の第 2 frontend(consensus 外)。MLIR は consensus にも参照ツールにも入れない — 誰が MLIR 等で
  TIR を生成してもよく、正準エンコーディングと検証器がそのまま接点になる。採らない理由は Alternatives の表。
  Rust 製の pliron(MLIR 風)も評価し、参照 lowerer の内部 IR には今は採らない(検証器・正準形・評価器は consensus 側が自前で持つ
  必要があり、pliron はその二重定義になる)。第三者の利用は自由、凍結後に任意の dialect adapter、lowerer の Gate 2 で再評価。

- **Part II(10-01 追記)。** モデル受け入れの標準経路 — header だけで読む preflight、streaming 変換と runtime pack、量子化 descriptor、feature scope、convert / register / mine の段階的な有効化(consensus に触れる §II.7.5 は決定済み・未実装)。本文は末尾の Part II。

#### Summary

Replace the consensus meaning of a PALW class — today a layer-template graph whose nodes name one of
45 hand-written kernels — with **PALW Canonical Tensor IR v1 (PALW-TIR v1)**: the frozen set of 25
architecture-neutral integer primitives, and a class program that is a static, bounded DAG over them,
carried as chain data. **v1 is frozen on an architecture corpus, not on a model**: it is accepted only
when representatives of every major decoder family lower to it without a single model-specific
primitive. Native kernels, including every kernel we have today, become optional fused kernels: node
software that is byte-identical to the reference evaluator at every commit point and never part of
the identity. The court knows the primitives and nothing about models.

**Terminology: an IR, not a VM.** A PALW-TIR program is a finite static DAG, a static layer schedule
and a scan over positions whose trip count is the job's length. It has no program counter, branch,
jump, loop or call, and every cost is known at registration. The *reference evaluator* evaluates such
a program; it is an interpreter only in the sense that it walks the graph (in `misaka-palw-tir`).

#### Motivation

##### 1. Where the boundary is today

ADR-0135 Decision 7: a graph the active kernel evaluator expresses is permissionless; a new op is
a protocol upgrade. The original ADR called this the “canonical VM”; it is not a future model VM.
The problem is how coarse the existing kernel relations are.

- **The graph is one fixed schema.** `PalwShapeProfileV3` (`consensus/core/src/palw_step.rs:409-500`)
  holds four node tables (`pre / gdn / attn / post`, ≤ 64 nodes each), per-layer weight templates,
  geometry scalars, and a layer schedule that is either attention or GDN by `full_attention_interval`
  (two hard-coded variants, V1 `(i+1)%n` and V2 `i%n`, the second added for Kimi: `:752-765`).
- **An op is a kernel.** Each node names a `kernel_semantics_id`. The 18-variant `op_kind` tag
  (`:191-235`) is only a label (`MulElem` carries a requantize kernel, `SoftMax` a router top-k). The
  catalog holds 45 kernels: 7 float, 10 BASE-0, 12 A16, 16 Q36, plus 1 + 4 fenced
  (`palw_step_refute.rs:286-331`). BASE-0's ten are primitives; the Q36 and Kimi kernels are whole
  subgraphs (`GatedDeltaNet`, `AttnFused`, the router, the MoE combine).
- **Conventions live in code, not in the identity.** GDN value head `vh` reads key head
  `vh % k_heads`, hard-coded three times (engine `misaka/palw/base0/src/qwen36.rs:1239-1262`, float
  reference `qwen36_reference.rs:273-277`, court `palw_step_refute.rs:3218-3222`). The schema has one
  `gdn_heads` (`palw_step.rs:459`); the key-head count is not in the class id. The state chunk maps
  size the conv row as `(2k+v)·gdn_heads` (`palw_state_chunk_map.rs:1038-1039,1109-1111`) where the
  true width is `2·k_heads·hd + v_heads·hd` — correct only when `k_heads == v_heads`. Qwen3.6-35B
  (16/32) and Qwen3.8-27B (16/48) are both on the wrong side of it (`fp_recompute.rs:1296-1306`).
- **Semantics also live in names.** Economic compute detects MoE by the substrings `"router"` and
  `".routed"` in weight names (`palw_economic_compute_v1.rs:297-325`). `UnsupportedOp` is derived from
  `verification_ccu > 0` (`palw_model_registry_v1.rs:792`), a proxy. The manifest's `graph_ir_root`
  is a placeholder used only in tests (`palw_model_registry_v1.rs:29-37`).

##### 2. What a new architecture costs today

| Lineage | Consensus code | New kernels | Runtime / tools |
| --- | --- | --- | --- |
| BASE-0 | ≈3,100 lines | 10 | `engine.rs`, `plan.rs` |
| Qwen2.5 A16 | ≈2,100 (+ A16 tier 1,237) | 0 | ≈13,000 lines of engine/backend/convert |
| Qwen3.6 | ≈4,450 incl. court arm | 16 + 1 fenced | ≈10,600 |
| Kimi K3 | ≈1,700 | 4 fenced | none; no e2e certificate |

Every family has needed a profile module, a court arm per kernel, an engine, a converter and an SDK
lineage (`docs/palw-model-onboarding-sdk.md:80-82`). That is a release per family, and each release
is a fence, a drill and a flag day. Adding `QWEN_GDN`, `MAMBA2` or `DEEPSEEK_MOE` as ops of a new IR
would reproduce the same problem a few years later.

##### 3. Why this is tractable here and would not be elsewhere

The hard part of a general ML VM on a chain is numerical determinism: reduction order, FMA
contraction, rounding modes, vendor kernels. **MISAKA already removed it.** Every live class is
integer-only (ADR-0040 A: no libm, no float on the consensus path). Inside a proved no-overflow bound,
integer addition is exactly associative (ADR-0040 E). Information is lost only at *named* sites, each
with its own rule (ADR-0040 C1, ADR-0047, ADR-0052). ADR-0057 already lets a backend fuse and reorder
below a committed row provided the row is byte-identical.

The step from "a catalog of kernels" to "a catalog of primitives" therefore needs no new numerical
theory. It needs the kernels decomposed along the lossy sites they already have, and the checks the
kernels perform by hand (bounds, shapes, court cost) turned into static analyses over a program.

#### Goals and non-goals

**Goals.**

- G1. A class program is data: a static DAG over a closed primitive set, hashed into the class id.
- G2. **v1 is complete for an architecture corpus (§1), with zero model-specific primitives.**
- G3. Every admissible program is adjudicable by construction; A4 is proved once, for the primitives.
- G4. Every admissible program's ranges, costs and court ceilings are derived at registration, exactly.
- G5. The court knows no model. It interprets primitives over committed values.
- G6. Native speed stays available through fused kernels outside the identity — built last.
- G7. The canonical work vector (ADR-0145) is derived structurally, with no name heuristics, and stays
  representation-neutral (PALW-WK-3).
- G8. Today's classes keep running unchanged; today's kernels become fused kernels of IR patterns.
- G9. A tool answers "is this model permissionless?" from a Hugging Face `config.json`, with the same
  function consensus admission calls.

**Non-goals.** Turing completeness; loops with data-dependent trip counts; jumps; recursion; dynamic
shapes. Floating point on the consensus path (ADR-0053 stands). Arbitrary user code (WASM, eBPF, CUDA)
as consensus semantics. zk proofs of inference. Changes to collateral, panels, the lottery or the
claim lifecycle. Sampling (it stays in the logits scheme and the FP job, RFC-0001).

#### 1. The conformance corpus and the v1 freeze criterion

This section is the RFC's acceptance test and is fixed **before** any implementation.

##### 1.1 The corpus

| # | Family | Representatives (at least one real checkpoint each, plus reduced configs) | What it exercises |
| --- | --- | --- | --- |
| C1 | Dense Transformer | Llama 3.x, Qwen2.5/Qwen3 dense, Mistral 7B, Gemma 2/3 | RMSNorm variants (Gemma's `1+w`), SwiGLU/GeGLU, RoPE variants (YaRN, partial), logit soft-capping, tied embeddings |
| C2 | Attention variants | Llama (GQA), Falcon/PaLM-style (MQA), Mistral / Gemma (sliding + global) | head grouping, windowed history, alternating layer schedules |
| C3 | MoE | Mixtral 8×7B, Qwen-MoE / Qwen3-MoE, DeepSeek-V2/V3 | top-k routing, shared experts, grouped (group-limited) routing, normalised and unnormalised gates |
| C4 | Linear attention | Qwen3.6 / Qwen3.8 GDN (16:16, 16:32, 16:48, 24:72, 32:128 head ratios) | gated delta rule, short causal conv, L2-normalised keys, unequal key/value heads |
| C5 | SSM | Mamba, Mamba2 | selective scan, `softplus(dt)`, `exp(A·dt)` decay, conv window, grouped B/C |
| C6 | Recurrent | RWKV-6/7 | token shift, data-dependent decay, WKV state |
| C7 | Hybrid | Qwen3.6 (attention + GDN), Jamba (attention + Mamba + MoE), Kimi-style schedules | per-layer block schedules, mixed state kinds |
| C8 | Routing variations | top-1/top-2/top-8, shared + routed, grouped top-k, expert bias terms | selection semantics and tie rules |

Each family is exercised at two scales: **reduced configurations** (same architecture, small widths
and depths, random and extreme-range weights — for bit identity and fuzzing) and **one real small
checkpoint** (≤ 3B where one exists — for fidelity).

##### 1.2 The freeze criterion

PALW-TIR v1 is acceptable only if **all** of the following hold, and it is frozen (its `prim_set_id`
fixed) at the moment they first hold together:

1. **No model-specific primitive.** Every corpus family lowers to the primitive set. A primitive is
   named and defined by mathematics (`MatMul`, `Gather`, `IntExp`), never by an architecture or a
   model (`QwenGdn`, `Mamba2Scan`, `DeepSeekRoute` are refused by name).
2. **Minimality.** No primitive is expressible byte-identically by the others at equal court cost.
   The target is 30–50 primitives.
3. **Stability under the last family.** If lowering the last family added to the corpus forces a new
   primitive, v1 is not frozen: the new primitive must be general (useful to at least two families,
   or a standard mathematical operation), and the whole corpus is re-lowered.
4. **Three-way bit identity.** For every corpus program, on every generated input:
   `reference evaluator == independent second implementation == every backend that ships`, byte for
   byte, at every commit point.
5. **Fidelity against the float original.** The integer program is compared with the family's
   Hugging Face float reference on a fixed evaluation set — top-1 agreement, KL divergence and
   perplexity difference — against per-family thresholds set in Phase A. The reference is pinned:
   the `transformers` version the fixtures name (5.17 at Gate 1), eager attention, and per-position
   decode — `transformers` disagrees with itself across attention backends and between prefill and
   decode (soft-capping under sdpa, dynamic NTK and LongRoPE frequency switching). **This is not a bit-identity
   requirement**: static integer quantization changes outputs by design, and ADR-0053 withdrew
   tolerance from consensus. Fidelity decides whether a lowering is *useful*, never whether a claim is
   *valid*.
6. **Legacy conformance.** Each of today's integer catalog kernels (the 45 less the 7 float kernels,
   plus the fenced `RequantizeByToken`) is expressed as an IR segment and is byte-identical to it (so today's semantics are a subset of v1, and today's kernels can become fused
   kernels).
7. **Static verifiability.** Every corpus program passes the range, cost and court analyses of §3
   within the proposed ceilings, or the ceiling is revised with a stated reason.

Qwen3.8 (K 16 / V 48) is one test in C4, not the goal. Under the criterion, `16:32`, `16:48`, `24:72`
and `32:128` are shape data, not kernels.

#### 2. Architecture

```
HF config.json + transformers modeling code (primary) · ONNX / GGUF importers (optional) · custom
             │   (outside consensus: anyone may write these)
             ▼
   Architecture lowerer (per family, data-driven templates)
             ▼
   High-level ML graph (RMSNorm, attention, GDN, MoE … as library subgraphs)
             ▼
   PALW Canonical Tensor IR v1 (PALW-TIR) ── tensor shape/index · integer arithmetic · reductions
                                     routing/indexing · quantization boundaries · bounded state
═════════════════════ CONSENSUS BOUNDARY ═════════════════════
   Static verifier (types · ranges · costs · court cones)   → admission
   Reference evaluator (slow, obviously correct)            → the court, the meaning of "correct"
             │
   Backends (CPU · Metal · CUDA), generic primitive kernels + fused kernels   → node software
```

Consensus knows the IR and nothing above it. Who wrote a lowerer, an importer or a Metal kernel
confers no protocol authority.

**Frontends and compiler infrastructure.** The primary frontend is a Hugging Face checkpoint: its
`config.json` read conservatively (every key that can change the math is understood or the model is
refused, never silently ignored) and its semantics taken from the `transformers` modeling code, which a
new architecture ships with on release day. ONNX and GGUF importers are optional secondary frontends
after the v1 freeze; they target the same high-level graph and add nothing to consensus. No compiler
infrastructure is a dependency of consensus or of the reference tools: anyone may produce TIR with
MLIR, TVM or a script, because the canonical encoding (§4.1) and the verifier
(`palw-class check-architecture --tir`) are the whole interface.

#### 3. The primitive set (starting hypothesis)

Phase A derives the final list from the corpus. The table below is the hypothesis Phase A starts from
and must confirm, extend or shrink.

**The granularity rule.** A primitive boundary sits at an exact integer operation or a named lossy
site; everything above that is library (§3.3). Finer primitives do not raise the commitment cost,
because commit points are chosen per cone (§6), not per node — the lower bound on granularity is
exactness at the lossy sites, the upper bound is that no primitive knows an architecture.

##### 3.1 Types and shapes

Tensors are raw integers. Quantization parameters are op attributes (`Requantize(mult, shift)`) or
parameter tensors from the artifact (per-channel scales, ADR-0050 D); there are no float values and no
scale types.

| Type | Use | May be committed |
| --- | --- | --- |
| `i8`, `i16` | activations (BASE-0 int8, A16 codes), weights | yes |
| `i32` | narrowed accumulators, fixed point Q[k], logits, state | yes |
| `i64` | exact wide accumulators inside a segment | **no** (the A16 rule "i64 never crosses a step boundary", generalised) |
| `idx` (u32) | token ids, positions, selection indices | yes |

Rank ≤ 4. Every dimension is a constant except the symbolic `H` (history length,
`1 ≤ H ≤ min(pos + 1, window) ≤ history_bound`, PALW-EX-16).

##### 3.2 The order-free region

**Definition.** An *order-free region* is a connected set of nodes in which (a) every node is an exact
primitive (§3.3 kinds S and E), (b) no node saturates, shifts right, divides or rounds, and (c) the
range analysis (§5.2) proves every intermediate value, **for every association and every order of
every sum in the region**, within its declared type. Only inside an order-free region may an
implementation reassociate, reorder, regroup, tile or parallelise arithmetic. A region ends at every
lossy primitive, every selection primitive, every commit point and every state write.

Condition (c) is the reason the range analysis bounds a sum by `len · max|term|` rather than by the
final value: a partial sum in any order is then also inside the bound.

##### 3.3 Primitives

| Kind | Primitives | Notes |
| --- | --- | --- |
| **S — structure** (exact, no arithmetic) | `Reshape`, `Permute`, `Slice`, `Split`, `Concat`, `Broadcast`, `Pad(const)` | static bounds, checked at admission; head tiling and grouping are `Reshape`+`Broadcast` |
| **S — indexing / routing** | `Gather(axis)`, `Scatter(axis)` (candidate), `Iota` | `Gather` over a pinned table is also how narrow-input activations and RoPE tables are evaluated (data, not arithmetic) |
| **E — exact arithmetic** | `Widen`, `Add`, `Sub`, `Mul`, `Neg`, `Min`, `Max`, `Abs`, `ShiftLeft` | results in a declared type proved to fit |
| **E — linear algebra** | `MatMul(acc)`, `BatchedMatMul(acc)` | contraction length bounded by range analysis |
| **E — reductions** | `ReduceSum(axis, acc)`, `ReduceMax(axis)`, `ReduceMin(axis)` | |
| **L — quantization boundaries** (lossy, each names its rule in `R = {HalfAwayFromZero, HalfUpSRDHM, Floor, Saturate}`) | `Requantize`, `Rescale`, `Narrow(shift, rule, to)`, `Clamp(lo, hi)`, `ShiftRight(n, rule)`, `DivConst(d, rule)` | the only primitives that lose information |
| **L — integer transcendentals** | `IntExp`, `IntLn`, `IntRsqrt`, `IntRecip`, `IntSigmoid` (candidate: library) | the fixed-iteration algorithms of 04a Decision F and ADR-0052 D, constants pinned by hash; never libm |
| **X — selection** | `Compare(cmp)`, `Select`, `ArgMax`, `TopK(k)`, `Sort` (candidate) | ties break to the **lowest index**; `TopK` returns its set in index order (ADR-0052 B) |
| **T — bounded state** | `StateRead`, `StateWrite` (saturates to the declared range), `HistAppend`, `HistRead(window)` | the position-axis scan of §4.2 |
| **T — within-step scan** (candidate) | `BoundedScan(body, axis, len)` | only if the corpus shows a recurrence *inside* one position (see §4.2) |

About 40 primitives, in the target range. Four properties follow:

- **Activation functions on narrow inputs are data.** An `i8`/`i16` activation (GELU, SiLU, a new one
  next year) is `Gather(table, x)` over a 256- or 65,536-entry table pinned in the artifact — PALW-EX-5
  ("a transcendental evaluated at registration is data") applied to activations.
- **Head layouts are data.** `vh % k_heads` (tiling) and `vh / (v_heads / k_heads)` (grouping) are
  both `Reshape` + `Broadcast` + `Reshape`. Which one a model uses is in its program and its class id.
- **MoE is explicit.** Routing is `TopK` → `Gather` over expert weights; grouped routing is
  `TopK` over group scores → `Select` mask → `TopK`. Canonical work counts active experts from the
  graph, not from weight names.
- **Composite ops are library subgraphs**, not primitives: `RmsNorm`, `LayerNorm`, `SoftMax`, `Silu`,
  `GeGLU`, `L2Norm`, RoPE, attention, the gated delta rule, the selective scan step, the WKV step, the
  causal conv, the MoE combine. The library (`tir_library_v1`) is data used by lowerers and by fused
  kernel patterns. Consensus sees only expanded primitives.

`Scatter`, `Sort`, `IntSigmoid` and `BoundedScan` are marked *candidate*: Phase A keeps them only if a
corpus family needs them and the others cannot express them at equal court cost.

#### 4. The program, `TirProgramV1`

##### 4.1 Structure

```
TirProgramV1 {
  version: 1,
  prim_set_id: Hash64,                                            // PALW-TIR v1
  inputs:  { token: idx, pos: idx },                              // one position per step
  params:  [ParamDecl { name, dtype, shape, per_layer: bool }],   // bound to artifact inventory tensors
  consts:  [ConstDecl { name, dtype, shape, bytes }],             // small inline constants, ≤ 64 KiB total
  states:  [StateDecl { name, kind: Fixed | Hist, dtype, shape, range, window?, per_layer: bool }],
  blocks:  [Block { name, carry_in, nodes: [Node], carry_out }], // e.g. pre, attn_layer, gdn_layer, moe_layer, post
  schedule: { layers: u16, kinds: [u8; layers] },                 // which block each layer runs
  outputs: { logits: NodeRef, logits_scheme_id },
  history_bound: 2^18 | 2^21,
}
Node { prim: u8, attrs: canonical bytes, inputs: [Ref; ≤ 8], out: { dtype, shape }, commit: bool }
Ref  = Node(i < self) | CarryIn | Param(j) | Const(j) | State(j) | Input(j)
```

- **The layer loop is a schedule, not a loop.** `schedule.kinds` names, per layer, the block that
  layer runs (`layers ≤ 1024`). This generalises `full_attention_interval` V1/V2 to any pattern —
  Gemma's sliding/global alternation, Jamba's attention/Mamba/MoE mix, a model whose first layers are
  dense and the rest MoE — without a schema version. Per-layer params and states are indexed by layer.
- **Refs point strictly backward.** Node order is the canonical topological order; the node index is
  the node's identity in commitments (`input_refs` already enforces this, `palw_step.rs:640-652`).
- **Canonical encoding.** Borsh, in a normal form admission enforces (no dead nodes, no duplicate
  consts, canonical attribute order, shapes fully inferred and equal to the declared `out`), so the
  program hash is the program.
- **Identity.** `graph_ir_root = H(TirProgramV1)` becomes a real field (the ADR-0135 placeholder). The
  class id commits to `graph_ir_root`, `artifact_root`, the tokenizer and the logits scheme, and **not**
  to node-local knobs (`n_threads`, `repack_on`, which today are inside the class id,
  `palw_step.rs:466,478`).

##### 4.2 Time: the bounded scan over positions

A program computes **one position**: the logits for `token` at `pos`, reading and writing its states.
A run of `T` positions is the scan of that step over `0..T-1`, bounded by `history_bound`. This is the
bounded scan, and it lives in exactly one place:

- `Fixed` states carry recurrences: the GDN state `S`, the Mamba state `h`, the RWKV WKV state, conv
  windows, token shift. `StateWrite` saturates to the declared range (a named lossy site), so the
  recurrence stays inside its interval whatever the data. (Its *real* bound is ADR-0052 E's
  contraction argument; the saturation makes the analysis total, not tighter.)
- `Hist` states are append-only histories: the KV cache. `HistRead(window)` yields the last
  `min(pos + 1, window)` rows, which covers sliding-window attention with no dynamic slice.

Prefill is the same step at positions `0..T-1`. A backend batches prefill however it likes
(PALW-EX-12's "one pass over the weights" is a scheduling rule and is unchanged), but its committed
values must equal the per-position semantics. For recurrences this means a chunked-parallel prefill
(Mamba2's SSD form, chunked GDN) is a fused kernel only if it reproduces every per-position lossy site
— possible but hard, and exactly the kind of work that comes last.

**Within-step scans.** If a corpus family has a recurrence *inside* one position (none of C1–C8 is
known to), Phase A adds `BoundedScan(body, axis, len)` with a static `len`, a body that is itself a
TIR block, and the body's cost multiplied by `len` in every analysis. Until then it is not in v1.

#### 5. Static verification (admission)

Admission of an IR class runs four analyses, each one pass, linear in the unrolled node count. A
program that any analysis cannot prove is refused; there is no "probably fine".

##### 5.1 Types and shapes

Every node's output type and shape is inferred and must equal its declared `out`. `H` propagates
symbolically. A shape error is a refusal, never a panic (PALW-EX-5's totality rule).

##### 5.2 Ranges

Each tensor gets an integer interval. Inputs and params take their type's full interval (a weight
declared `i8` is `[-128, 127]` — weights are not trusted to be small); states take their declared
range; `H` takes its bound. Each primitive has a sound transfer function (for `MatMul`,
`len · max|a| · max|b|`, which also bounds every partial sum in every order, §3.2(c)). Admission must
prove, for every node:

- **no overflow** of any exact primitive's declared type, including every partial sum;
- **no illegal shift** (every shift amount static and within `[0, width)`);
- **no invalid divisor** (`DivConst` divisors are nonzero constants; division inside the transcendental
  algorithms is proved nonzero by their domains);
- **no out-of-bounds index** (`Slice` bounds static; `Gather`/`Scatter` index intervals within the
  gathered axis; `TopK` `k` ≤ axis length);
- **no oversized allocation** (every tensor and every state within the size caps of §5.4).

The result is a guarantee handed to every runtime: *inside an order-free region, overflow cannot
happen.* That guarantee is what makes reassociation and fusion safe.

##### 5.3 Costs

Each primitive has closed cost formulas. The analysis reports, per position and at the worst case
(`H = history_bound`):

- **compute**: MACs, elementwise ops, transcendental evaluations;
- **memory**: bytes read and written, peak live bytes in canonical order;
- **state**: bytes of `Fixed` and `Hist` state per layer and in total;
- **court worst case**: the largest terminal recomputation any single disputed tile can require (§6).

The formulas upper-bound the reference evaluator's work; their coefficients are benchmarked with a
margin in Phase D. Admission refuses a program over any network ceiling.

---

<a id="source-v1-0002-L00505"></a>

### 旧RFC-0002 / 原文505–731行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

#### 7. Fused kernels (built last)

A fused kernel is `(pattern, implementation)`: an IR subgraph template with exact structural
constraints (primitives, attributes, shapes, ranges) and native code for it.

- **F-1 (byte identity).** For every input in the pattern's declared domain, a fused kernel must
  produce, at every commit point inside or at the edge of the pattern, the bytes the reference
  interpreter produces. There is no tolerance (ADR-0057 D1).
- **F-2 (what may move).** A fused kernel may reorder, regroup, tile, vectorise or parallelise only
  inside order-free regions (§3.2). It must reproduce every lossy site's result; fusing *across* a
  lossy site is allowed only if the lossy site is still computed on exactly the same value.
- **F-3 (outside the identity).** A fused kernel is node software. It appears in no object, class id
  or fingerprint, and needs no fence (ADR-0057 D1/D2). Shipping one is a node release.
- **F-4 (the gate).** A fused kernel ships only with a differential gate that fires: random and
  range-extreme inputs against the reference evaluator *and* the independent second implementation,
  per backend (ADR-0057 D3).
- **F-5 (matching).** A pattern matches by structural equality after canonicalisation. An unmatched
  subgraph runs on generic primitive kernels.
- **F-6 (GPU).** Every primitive is integer and every order-free sum is order-independent, so an integer
  GPU backend (int8 `dp4a`, tensor-core IMMA, Metal integer SIMD) is an *exact* backend under F-1, not a
  tolerant family. ADR-0053's objection was to tolerance, which this does not need.

**Today's kernels are kept, as fused kernels.** Freeze criterion 6 expresses each of the 45 kernels as
an IR segment. Where the segment is byte-identical on the differential gate, the kernel becomes that
pattern's fused implementation; today's GDN, router and attention engines keep their speed.

**Runtime residency (node software, like the fused kernels).** Where a class's weights are held is the
node's decision, under ADR-0112's budget, with the tiers read off the program's dataflow — pinned
params read whole every forward, routed rows a route selects (a mixture's experts), gathered rows an
input selects (embeddings, n-gram tables) — and nothing read through a page fault:
[`docs/design/palw/tir/runtime-residency.md`](../design/palw/tir/runtime-residency.md). It changes
no byte any commitment sees (the identity is tested at the floor, at a fifth and against the page
cache), appears in no object, id or fingerprint, and ships as a node release.

#### 8. The tool: `palw-class check-architecture`

```
palw-class check-architecture --config config.json [--weights-index model.safetensors.index.json]
palw-class check-architecture --tir program.tir
```

A lowerer (non-consensus, `misaka-palw-sdk`) turns a Hugging Face config into a program using the
library templates; the verdict comes from `tir_admit_v1`, the same function consensus admission calls,
without network state.

| Verdict | Meaning |
| --- | --- |
| `ADMISSIBLE` | registrable as data; fused kernels cover every hot pattern |
| `ADMISSIBLE_GENERIC` | registrable as data; some patterns run on generic kernels (estimated slowdown printed) |
| `LOWERABLE_UNVERIFIED` | lowered from an architecture whose float reference is remote code the tool cannot run offline; the verdict above it holds, the fidelity column is empty |
| `EXCEEDS(limit, value, cap)` | a size, range, cost or court ceiling fails |
| `NEEDS_PRIMITIVE(name)` | outside active kernel semantics: report `KERNEL_EXTENSION_REQUIRED` with relation and limits, then propose a reusable semantic/checker/court extension (§II.12); no VM fallback |
| `NOT_LOWERABLE(reason)` | no lowerer template for this family; the model may still be expressible by hand |

A legacy mode reports the same verdicts against today's `PalwShapeProfileV3` catalog, so the tool is
useful before the IR is armed.

#### 9. Proposed Spec text (new chapter `spec/palw/04b-tensor-ir.md`)

Applies past `palw_tir_v1`. Legacy `PalwShapeProfileV3` classes keep chapter 04 unchanged.

- **PALW-TIR-1 (the primitive set).** An IR class's execution MUST be the evaluation of its
  `TirProgramV1` over PALW-TIR v1. No other operation has consensus meaning.
- **PALW-TIR-2 (integers only).** Every value on the consensus path MUST be an integer of a declared
  type. ADR-0040 A applies to the interpreter and to every backend.
- **PALW-TIR-3 (named loss).** Only lossy primitives MAY lose information, each by its declared rule.
  Every other primitive MUST be exact.
- **PALW-TIR-4 (order-free regions).** An implementation MAY reassociate or reorder arithmetic only
  inside an order-free region (§3.2). Everywhere else it MUST follow the program's order.
- **PALW-TIR-5 (no wide commitment).** An `i64` tensor MUST NOT be a commit point.
- **PALW-TIR-6 (bounded structure).** A program MUST be a DAG with strictly backward refs and a static
  layer schedule of at most 1024 layers; no dimension other than `H` MAY be non-constant.
- **PALW-TIR-7 (canonical form and identity).** Admission MUST refuse a program not in normal form.
  The class id MUST commit to `graph_ir_root` and MUST NOT commit to node-local knobs.
- **PALW-TIR-8 (shapes).** Admission MUST infer every shape and refuse any mismatch.
- **PALW-TIR-9 (ranges).** Admission MUST prove from type-worst-case inputs that no exact primitive or
  partial sum overflows, no shift is illegal, no divisor can be zero, no index is out of bounds and no
  allocation exceeds its cap. A program it cannot prove is refused.
- **PALW-TIR-10 (state).** A `Fixed` state MUST declare its range and `StateWrite` MUST saturate to it.
  A `Hist` state MUST be append-only and bounded by `history_bound` and its window.
- **PALW-TIR-11 (selection).** Selection primitives MUST break ties to the lowest index; `TopK` MUST
  return its set in index order. A selection that feeds a param `Gather` MUST be a commit point.
- **PALW-TIR-12 (cost).** No worst-case cost of §5.3 MAY exceed the network's ceiling.
- **PALW-TIR-13 (court cones).** Every commit point's cone MUST fit the court ceilings; every reduction
  over `H` MUST be dissectable; admission MUST derive each `Fixed` state's checkpoint interval.
- **PALW-TIR-14 (commit points).** Block outputs, logits, selections that feed a param `Gather`, and
  every state write MUST be commit points.
- **PALW-TIR-15 (the court).** A terminal refutation of an IR class MUST recompute the disputed tile
  by interpreting its cone over PALW-TIR v1. `court_catalog_root` MUST commit to `prim_set_id`.
- **PALW-TIR-16 (canonical work).** The work vector (PALW-WK-1) of an IR class MUST be derived from the
  program's structure; commit points and `tile_len` MUST NOT change it (PALW-WK-3).
- **PALW-TIR-17 (backends).** A backend MAY implement any subgraph natively, provided every commit
  point is byte-identical to the reference evaluator. A fused kernel MUST NOT appear in any
  consensus object.

#### Activation plan

The order is the point: **corpus → semantics → reference evaluator → verifiers → lowerers → court →
fused kernels → migration.** Fused kernels come last so that no performance need can pull a
model-specific semantic back into the IR.

| Phase | Work | Consensus change | Exit gate | Effort (2–4 people) |
| --- | --- | --- | --- | --- |
| **0** (independent, may run now) | (a) fix GDN `k_heads ≠ v_heads` in the *live* kernels — it already affects Qwen3.6-35B (16/32); (b) `check-architecture` legacy mode | (a) its own small fence on testnet-12; (b) none | (a) a 16/32 and a 16/48 fixture replay and adjudicate end to end | 2–4 weeks |
| **A. Corpus** | Lower every corpus family (§1.1) to a high-level graph and then, on paper and in a throwaway prototype, to candidate primitives; set fidelity thresholds per family | none | a primitive list that meets freeze criteria 1–3 on paper | 4–6 weeks |
| **B. Semantics** | The normative definition of every primitive (types, shapes, exact integer semantics, rounding rule, range transfer function, cost formula); golden vectors `consensus-vectors/tir-v1/` | none | reviewed spec text; vectors for every primitive incl. range extremes | 3–4 weeks |
| **C. Reference evaluator** | `misaka-palw-tir`: slow, obviously correct, no SIMD, no fusion; and an independent second implementation from the spec alone | none | both pass every golden vector; they agree on fuzzed programs | 4–6 weeks |
| **D. Verifiers** | `tir_admit_v1`: types, ranges, costs, court cones, normal form, size caps; cost coefficients benchmarked; admission CPU time measured | none | every corpus program verifies; mutation tests (overflowing, OOB, oversized, non-dissectable programs) are all refused | 4–6 weeks |
| **E. Lowerers** | Library templates and lowerers for C1–C8; quantisation/calibration tooling; fidelity runs; legacy conformance of the 45 kernels | none | freeze criteria 4–7; **v1 frozen, `prim_set_id` fixed** | 6–8 weeks (overlaps C/D) |
| **F. Court and admission** | Interpreter court arm, generic `H` dissection, generic checkpoint replay, structural work derivation, admission v10 — all behind dormant `palw_tir_v1`; drills | dormant fence | devnet drill registering **Qwen2.5-A16 as an IR program** (same weights): same logits as the legacy class, same verdicts on the forged-output battery (8/8); a drill that crosses the fence on the shipping binary | 6–8 weeks |
| **G. Fused kernels** | Legacy kernels as fused patterns first; then CPU/Metal/CUDA kernels for hot patterns | none (node releases) | F-4 gates fire and pass per backend | continuous |
| **H. Migration** | Arm `palw_tir_v1` on testnet-12 at a post-launch flag day; first new family as pure data; mainnet: IR is the admission path for new classes (`palw_tir_only_v1`) | fences | registration → panel → Final for a model no core developer wrote code for | — |

Total to Phase F: about 6–9 months with two to four experienced people. `palw_tir_v1` is a `Params`
fence like ADR-0102's: a dormant network fingerprints as if it did not exist; when armed, the params
fingerprint gains `prim_set_id` and the IR ceilings. On testnet-12 it would join the first fence list
after the capacity steps (ADR-0160), at a height chosen when Phase F's drill passes.

#### Implementation status

- **Gate 1 (2026-09-28): Phases A–C.**
  - **Corpus:** `docs/design/palw/tir/corpus-v1.md`, with every semantic claim cited to the `transformers` modeling code.
  - **Normative text:** `docs/spec/palw/04b-tensor-ir.md`.
  - **Crate `misaka-palw-tir`:** the reference evaluator with golden vectors `consensus-vectors/tir-v1/`. The BASE-0 frozen KAT and the A16/Q36 kernels are byte-identical as IR segments.
  - **Crate `misaka-palw-tir-lower`:** HF `config.json` + safetensors → frontend-neutral high-level graph → f32 reference. It matches `transformers` 5.17 on 57 tiny architectures and is estimated to cover about 91 % of decoder-only HF checkpoints by count.
  - **The primitive set came out at 25**, below the 30–50 target: minimality removed every composition. `Requantize`, `Rescale`, the shifts, `IntRecip` and `IntSigmoid` are library segments, byte-identical to the legacy kernels. `Scatter`, `Sort`, `BoundedScan` and `BoundedMap` are out, and a user-body `BoundedReduce` is refused. The additions are an internal-only `i128`, `Log2Floor`, and a `Div` with a tensor divisor, which carries every rounding shift.
  - **Deviations recorded in 04b §14:**
    - `HistAppend` returns the window.
    - Fixed state is committed at checkpoints rather than at every position (a per-position GDN state is far above the step-leaf cap).
    - `token_bound`.
    - Committed operands are checked against their proven intervals (PALW-TIR-33: out of interval is a malformed commitment and the producer loses).
    - Params get a 2^40 sanity bound instead of the 2^28 cap.
- **Gate 2 (running):** the composite library, conformance against the live kernels, `tir_admit_v1`, quantisation and fidelity, the independent second implementation, and the Phase F integration design.
- **Found on the way (legacy code, outside this RFC):**
  - A court arm that could panic on hostile committed lanes. It is being fixed as a node update.
  - The fenced Kimi court arms disagree with the model: zeroed KDA state, equal-head-dim MLA, and a softmax router where HF uses a grouped sigmoid.
  - For GDN, HF maps value head `vh` to key head `vh / (v_heads / k_heads)` (grouping). The live kernel's `vh % k_heads` equals it only after a value-head reordering at conversion, which Phase 0 checks.

#### Alternatives

| Alternative | Why not |
| --- | --- |
| Keep adding kernels (status quo) | A release, a fence and a drill per family; conventions hide in code and bite later (the GDN k≠v defect) |
| An IR that grows per model (`QwenGdn`, `Mamba2Scan`, `DeepSeekRoute` as ops) | The same problem, one level down, in a few years. Refused by freeze criterion 1 |
| Generalise kernels one by one | Worth doing for live bugs (Phase 0), never changes the nature of the boundary |
| A Turing-complete deterministic VM (WASM-like) | Halting and metering become runtime problems; the court must bound an arbitrary program; nothing an LLM needs requires it |
| Cryptographic proofs of model computation | Permanently prohibited by the user's 2026-10-15 decision and PRINCIPLES §8; use sampled replay, public localization, deterministic court and the explicit validator-approval assumptions |
| ONNX / StableHLO as the consensus IR | Open op sets, float semantics, implementation-defined corners. Good lowerer *inputs*, not consensus |
| ONNX importer as the primary frontend (evaluated 2026-09-28) | A new architecture reaches ONNX export later than `transformers`, sometimes never, which defeats day-one registration; exporter-specific decompositions (attention, RoPE, caches) must be pattern-matched back into composites; QDQ carries float scales and ties-to-even rounding and is re-quantized anyway. Kept as an optional secondary frontend after the freeze |
| Upstream MLIR as the compiler infrastructure, a MISAKA dialect (evaluated 2026-09-28) | A C++/LLVM toolchain in a Rust node for an IR of ~40 primitives whose lowering is template expansion; consensus could not depend on it in any case. Allowed for third-party compilers (the encoding is the interface); revisit for Phase G code generation, outside the identity |
| pliron (Rust-native, MLIR-inspired: dialects, ops, regions, verification, printing, passes) as the reference lowerer's internal IR (evaluated 2026-09-28) | What it provides is what consensus must own itself: the typed DAG, the verifier, the canonical encoding and the evaluator live in `misaka-palw-tir`, and admission and the tool call the same verifier (G9). A pliron `palw.tir` dialect would be a second definition of every primitive's type and shape rules plus a second representation with translation layers, not less work; what the lowerer adds on top is domain logic (configs, weight layouts, float reference, calibration) it does not help with. Also 0.x with a breaking minor release about every six weeks and no declared MSRV, against this repository's pinned toolchain. Third-party compilers may use it; an optional dialect adapter (pliron ⇄ canonical bytes) after the freeze; re-evaluated at the lowerer's Gate 2 if the lowerer grows non-trivial rewrite passes |
| RLX (a Rust ML compiler/runtime: HIR → MIR → LIR, memory planning, ONNX/GGUF import, multi-backend) as the compiler or runtime | Float, JAX-shaped semantics and a fast-moving API; the byte-identity rule (F-1) needs integer kernels written against this IR. Reference material for Phase G backends and the secondary frontends |
| Cranelift as the IR | An SSA code-generation IR without tensor semantics; a candidate for CPU code generation of generic kernels in Phase G, outside the identity |
| TOSA's integer profile as the semantic base | The closest existing integer specification and useful prior art, but its rescale rounding is not the frozen rules the live kernels use (ADR-0040 C1/C2), and it has no state, position scan, commit points or court cost |
| Structured control inside a step (`BoundedMap`, `BoundedReduce` with a user body) | Heads, experts and channels are tensor axes, so batching needs no control flow; a user-body reduction is order-dependent unless proved associative and exact, which breaks §3.2. `BoundedScan`/`BoundedMap` enter only if the corpus needs them (Phase A) |
| Bounded extension programs for what the IR cannot express | Withdrawn under ADR-0172: use a generic versioned semantic/checker/court kernel extension, not a second ISA execution model or guest fallback (§II.12) |
| A float IR with tolerances | ADR-0053: tolerance turns fraud proofs into votes on noise |
| Every primitive committed | Multiplies step leaves several-fold against the 2^22 cap; cones give the same adjudicability at today's commitment cost |

#### Security and economic analysis

- **Unadjudicable programs (A4).** An attacker registers a program the court cannot try and lies in it
  for free. Closed: every node must be a v1 primitive and every cone within ceilings. Residual: an
  interpreter defect — two independent implementations, golden vectors, fuzzing, and the legacy
  conformance against kernels that already survived the red-team battery.
- **Overflow to break order-freedom.** A program whose partial sums overflow would make hosts with
  different reduction orders disagree, so an honest seat looks like a liar. Closed by §5.2 from
  type-worst-case inputs and partial-sum bounds; weights cannot escape a bound that does not trust them.
- **Cost under-metering.** A program cheap on paper and slow in reality would stall panels. Cost
  formulas upper-bound the reference evaluator, coefficients carry a benchmarked margin, and the
  worst-case step is capped. A registrant with a private fused kernel is faster than the network until
  the kernel is public — P5's "efficiency is rewarded" — while seats run generic kernels at the metered
  cost. Whether weight (ADR-0069) should also require a measured generic-backend throughput floor is
  open.
- **Mis-fusion.** A fused kernel applied to a subgraph it does not implement convicts whoever ran it,
  never the network: the court runs the interpreter. F-5 and F-4 make it unlikely.
- **Selection games.** Near-ties decided by rounding would make "which expert" a vote on noise.
  PALW-TIR-11 fixes the tie rule and commits the selection.
- **Identity grinding.** Encodings of one program would give free class ids. Normal form makes the
  encoding unique; node-local knobs leave the id.
- **Registration DoS.** Linear-time admission with capped sizes; the 1 BILI burn and the 4-per-block
  cap stay; admission CPU time is measured in Phase D and becomes a ceiling if needed.
- **Canonical work.** Derived structurally: `dense_matmul` = `MatMul` with a non-gathered `Param`;
  `routed_expert_matmul` = `MatMul` whose `Param` passes through a `Gather` indexed by a `TopK`
  (active experts only); `attention_*` = primitives over `H`; `recurrence` = the update cones of
  `Fixed` states; `normalization` = cones feeding `IntRsqrt`; the rest `other_verified_ops`; bytes from
  `HistRead`/`HistAppend` and the artifact layout. Commit points and `tile_len` feed none of it.
- **Overall surface.** One interpreter of ~40 small primitives replaces 45 bespoke court arms and
  their per-kernel shape rules.

#### Compatibility and migration

- Live classes and kernels are untouched; legacy admission (v9) stays for existing lineages.
- After freeze criterion 6, each legacy kernel is also a fused kernel of an IR pattern. A legacy class
  may be re-registered as an IR program (new class id, same weights, same outputs); nothing forces it.
- The ADR-0135 manifest's `graph_ir_root` and `runtime_version` become real (`runtime_version` =
  `prim_set_id`); `UnsupportedOp` becomes `NEEDS_PRIMITIVE(name)` from the verifier.
- Economic compute's name heuristics are retired for IR classes.
- ADR-0135 Decision 7 is unchanged in wording; its scope widens because the VM expresses more.

#### Open questions

1. **The final primitive list**, decided by Phase A and frozen in Phase E — in particular `Scatter`,
   `Sort`, `IntSigmoid`, `BoundedScan`, and whether Kimi's KDA or DeepSeek's MLA need anything new.
2. **Existing `AttnFused` vs "dissectable".** Does today's fused attention compute exact two-pass sums
   over the history? If it rescales a running sum, legacy conformance fails for it and the difference
   must be resolved before the freeze.
3. **Integer fidelity of the corpus.** Static int8/A16 quantisation with frozen activation scales may
   lose too much on some families (outlier activations in large dense models, long-context SSMs). The
   thresholds and the calibration tooling are Phase A/E work; a family that cannot meet them is a
   quantisation problem, not an IR problem, and must be reported as such.
4. **Program carriage**: inline in the admission carriage up to 256 KiB, or an artifact leaf with only
   `graph_ir_root` on chain.
5. **Weight for IR classes (ADR-0069)**: is the drilled primitive set enough, or does each program still
   need its own end-to-end drill (responder availability was ADR-0093's gap, not arithmetic)?
6. **A generic-backend throughput floor** as a condition of weight.
7. **Normalization** in the work vector: `IntRsqrt` cones, or folded into `other_verified_ops`.
8. **A8/A16**: separate families inside the IR, or dtypes of one family.
9. **The GDN head convention** in the live kernel (`vh % k_heads` tiling vs grouping): Phase 0 must
   check it against the model's reference semantics; the IR makes the choice explicit.

#### Decision

<Open.>

---

---

<a id="source-v1-0002-L01245"></a>

### 旧RFC-0002 / 原文1245–1417行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

#### II.5 Quantisation as data (R3)

A model's weights reach the lowerer in whatever storage format its publisher chose. Each format used to be a Rust decoder, one
more release per format. They are now **descriptors**: JSON files (`misaka.palw.quant-format.v1`, `quant-formats/*.json`) that say
where a tensor's fields are, which tables it indexes and, as expressions, what each element is, read by **one** interpreter
(`quantfmt`). The registry refuses what it does not describe, by name; a new format is a file with test vectors and no change
to the crate.

##### II.5.1 The descriptor

**Status: draft.** Additive changes are allowed until the first runtime pack is published; then the schema freezes and a later
change is `quant-format.v2`. The identity of a descriptor is the BLAKE2b-256 digest of its canonical JSON (the text's layout does
not matter).

```jsonc
{
  "schema": "misaka.palw.quant-format.v1", "name": "BNB_NF4", "doc": "…",
  "ids":    [ { "scheme": "ggml", "id": 2 },                                            // a GGUF tensor type
              { "scheme": "config", "method": "bitsandbytes",                           // a quantization_config
                "when": [ { "path": "load_in_4bit", "one_of": [true] },                 // …told apart by other keys
                          { "path": "bnb_4bit_quant_type", "one_of": ["nf4"] } ] } ],
  "params": { "bits": { "config": "bits", "default": 4, "allowed": [2, 4, 8] },         // from the configuration
              "bs":   { "from_role": { "role": "qstate", "path": "blocksize" } } },     // or from a tensor's JSON document
  "config": { "inert": [ … ], "skip": "llm_int8_skip_modules", "skip_match": "path",
              "lm_head": "unless_skipped", "checks": [ { "path": …, "one_of": [ … ], "message": "…" } ] },
  "tables": { "name": { "bits": 8, "signed": true, "hex": "…" } },                      // constants the format indexes
  "layout": { "kind": "blocks" | "tensors" | "virtual", … },
  "decode": { "target": "integers" | "floats" | "tensor", … },
  "tests":  [ { …a vector… } ]                                                           // mandatory
}
```

**Vectors are mandatory and are checked when a descriptor is loaded.** A vector is a valid random input and the expected output as
computed by an *independent* implementation; `QuantFormat::from_json` decodes every vector and refuses the descriptor if one
element differs by a bit. A descriptor that fails its own vectors does not enter the registry.

##### II.5.2 The three layouts

| Layout | A unit is | Decodes to | Used for |
| --- | --- | --- | --- |
| `blocks` | a block of `elems` elements in `bytes` bytes, fields at byte offsets (`u8`, `f16`, …) | the stored integers under group scales (`q`, `scale`, `zero`, `min`, code range, offset term) or floats | GGUF's ggml types |
| `tensors` | a *module's* tensors, each a **role** (a name, a suffix, accepted dtypes, a rank, required or not) | the stored integers under group scales (`group.size`, `group.index`, `q`, `scale`, `zero`, `min`, `code`, `order`, `offset_term`) or floats (`value`) | GPTQ, AWQ, FP8, compressed-tensors, bitsandbytes |
| `virtual` | tensors a checkpoint stores packed, **served as float tensors under another name**, of any rank up to four (the first lane may be an expert axis) | a tensor: `axes`, a `shape` expression per axis, and one `value` expression | MXFP4 as Hugging Face stores it (`<name>_blocks` + `<name>_scales` served as `<name>`) |

A target that is `integers` is *lowered from the stored integers* (nothing is re-quantised: the codes are the weights, and each
row's group scales become exact integers at the row's unit, `lower/qlinear.rs`); a target that is `floats` or `tensor` decodes to
floats and the weight takes the ordinary W8 path of Part I's lowerer. Which a format is follows from what its elements are, not
from how it is stored: a codebook (NF4, FP4, FP8) is floats; an integer code under a scale is integers.

##### II.5.3 Virtual tensors

A `virtual` layout is the answer to a format whose stored tensors are not the tensors the model uses. `DescribedSource` wraps a
checkpoint, lists the *served* names instead of the packed ones, answers a shape from the role tensors' headers, and evaluates any
element range of the served tensor without materialising the rest. OCP MXFP4 as Hugging Face stores it (gpt-oss) is one line:

```
roles  blocks  <name>_blocks  U8 [E, out, in/32, 16]      scales  <name>_scales  U8 [E, out, in/32]
axes   e, i, o          shape  [dim_blocks[0], dim_blocks[2] * 32, dim_blocks[1]]
value  fp4x2[(blocks[e, o, i / 32, (i % 32) / 2] >> (4 * (i % 2))) & 15] * 0.5 * e8m0(scales[e, o, i / 32])
```

A leading expert axis and per-block scales over a 3-D tensor are a lane `e` and a read `scales[e, o, i / 32]`: nothing in it is
specific to a model, and every binding that reads the float export reads the served tensor unchanged.

##### II.5.4 The expression language

Closed, total and deterministic; the whole of what a descriptor can say (`quantfmt/expr.rs`).

- **Values.** Integers are `i64` and every operation is checked (an overflow, a division by zero, a shift out of range is an
  error, never a wrap; `/` and `%` truncate toward zero, as C). Floats are IEEE `f64` with `+ − × ÷` only and every result must
  be finite. There is no loop and no recursion, so every expression ends, and no transcendental function, so a decoded value does
  not depend on a platform's libm.
- **Grammar.** `c ? a : b`, `|| && | ^ &`, comparisons, shifts, `+ - * / %`, unary `- ~ !`, `name[i, j]`, `f(a, b)`, literals. A ternary
  evaluates each arm only for the lanes that take it, so an out-of-range read in the arm not taken is never an error.
- **Names.** the *lanes* (`e`, `i`, `o`, `g`, `blk`, …: columns evaluated a batch at a time), the format's *parameters*, a block's
  *fields* or a tensor's *roles* read by index (`qweight[i / 8, o]`), `dim_<role>[axis]` (a role's dimension, known from its header
  alone), `has_<role>` (an optional role is present), `float_<role>` (a role is stored in a floating dtype, which tells apart a
  role that accepts bytes or floats), and *tables*.
- **Functions.** `float int abs min max floor sext bits pow2`; the small float formats' decoders `f16 bf16 f32 e8m0 fp8e4m3
  fp8e5m2`; and the **rounding functions** `rnd32 rndbf16 rndf16 rnd(x, kind)` (to nearest even to binary32 / bfloat16 / binary16;
  `kind` 0, 1, 2, a constant a descriptor can take from the checkpoint), by which a descriptor reproduces a library's float32
  arithmetic *exactly*: a float32 `+ − × ÷` is the correctly rounded float32 result of the float64 operation on float32 operands
  (a format with `p' ≥ 2p + 2` significand bits, here 53 ≥ 50, does not double-round), so `rnd(a * b, 0)` is bit-for-bit a float32
  multiplication.
- **Parameters read from a tensor.** A parameter may be read from a role that holds a UTF-8 JSON document (`from_role`: a dotted
  path, an `int`, `float` or `string` through the parameter's `map`, a default for an absent key). The JSON is read by an exact
  reader that keeps numbers as text, so a decimal is parsed once and correctly rounded. bitsandbytes stores each module's logical
  shape, block size, dtype and double-quantisation offset this way.

##### II.5.5 Reading `quantization_config`

Every top-level key of the configuration is *read* (a parameter's `config` path), declared *inert* (it does not change what the
stored tensors mean), named by a *check*, or refused by name: a key the descriptor does not know may change the meaning, so a model
that carries one is refused with the key's name. `skip` names the key that lists the modules kept in float, and `skip_match` how
an entry names a module — `exact`, `contains`, `path` (transformers' bitsandbytes rule: the module's last component, its whole
name, or a path prefix) or `regex` (the subset `^ $ . .*` and escapes, which is what Hugging Face's `re.match` entries use; anything
else is refused); `lm_head` says whether the head is stored in the format. A `quant_method` can announce several descriptors,
told apart by `when` conditions on other keys (bitsandbytes: `load_in_4bit`, `bnb_4bit_quant_type`, `load_in_8bit`); another
descriptor under the same conditions is refused, one under others is added.

##### II.5.6 Refusals, and what is known and not yet described

An unknown type is never misread. A GGUF type no descriptor describes, a `quant_method` nothing reads, or a configuration a
descriptor refuses is a refusal that names it (the scheme, the type id, the name the metadata gives it, how many tensors and which
need it), lists what *is* described, and gives the safe paths: use the model's original safetensors, re-quantise it to a described
type, or supply a descriptor file (`--quant-format <file>`, or a pack's `quant.descriptors`) — no code change is needed.

`quant-formats/known-undescribed.json` lists the methods that are known, so that nobody mistakes a refusal for a verdict on the
model: HQQ, AQLM, VPTQ, SpQR, EETQ, FBGEMM-FP8 and BitNet (descriptors are expressible in principle; none written), QuIP/QuIP# and
HIGGS (the weights are decoded and then rotated by Hadamard transforms at run time, which is not a static decode; refused until a
transform feature exists), torchao (tensors serialised as Python objects, not safetensors tensors: refused, not described). The
preflight prints the entry beside the refusal.

##### II.5.7 The descriptors this build carries, and their evidence

39 built-in descriptors, each with vectors:

| Descriptors | Layout, target | The vectors' reference |
| --- | --- | --- |
| 29 ggml types: `F32 F16 BF16 F64`, `Q4_0 Q4_1 Q5_0 Q5_1 Q8_0`, `Q2_K Q3_K Q4_K Q5_K Q6_K`, `IQ4_NL IQ4_XS`, `TQ1_0 TQ2_0`, `MXFP4 NVFP4`, `Q1_0 Q2_0`, `IQ2_XXS IQ2_XS IQ2_S IQ3_XXS IQ3_S IQ1_S IQ1_M` | `blocks`; floats for the four plain types, integers (grid codebooks are constant tables) for the rest | gguf-py's own numpy dequantisers on 8,700 random blocks (`Q1_0`, `Q2_0`: a numpy transcription of the C); the tables are parsed out of `ggml-common.h`, not typed |
| `GPTQ` (v1 and v2, 2/4/8 bits, act-order, per-row) | `tensors`, integers | a torch re-implementation of AutoGPTQ's dequantisation |
| `AWQ` (GEMM, 4-bit) | `tensors`, integers | a torch re-implementation of AutoAWQ's `dequantize_gemm` |
| `FP8_BLOCK` (DeepSeek-V3, Qwen3-FP8) | `tensors`, floats | the reference `weight_dequant` over torch's float8 |
| `CT_PACK_QUANTIZED`, `CT_FP8_CHANNEL`, `CT_INT8_CHANNEL` (compressed-tensors) | `tensors`, integers / floats / integers | a torch re-implementation of compressed-tensors' `unpack_from_int32` and `dequantize` |
| `MXFP4_HF` (gpt-oss) | `virtual`, tensor | `transformers.integrations.mxfp4.convert_moe_packed_tensors`, the library's own function |
| `BNB_NF4`, `BNB_FP4` (also double-quantised), `BNB_INT8` | `tensors`: floats, floats, integers | `tools/bnb_ref.py`, a numpy/torch re-implementation of bitsandbytes' stored format and of its CUDA kernel's arithmetic |

**Evidence, stated plainly.** Only the ggml types (gguf-py) and `MXFP4_HF` (transformers) are checked against the maintainers' own
code. For GPTQ, AWQ, compressed-tensors and bitsandbytes the quantisation libraries are not installed in the offline environment
and the vectors come from re-implementations of their documented behaviour and source. The end-to-end fixtures (a tiny random
decoder quantised in numpy per each format's specification, `tools/gen_quant_fixtures.py`, 33 fixtures) are checked against
`transformers` with the dequantised weights substituted; that checks the *lowering* of a format, not the library's output. A
first checkpoint from the real quantiser belongs in the corpus (§II.8) and would settle it.

##### II.5.8 What is exact, and what the formats cost

- **Integer targets are exact in their integers.** The lowering carries the stored codes as they are (nothing is re-quantised); a
  row's group scales become integers at one unit per row (`|a| ≤ 2^20`). An fp16 scale has 11 significant bits, so every scale
  within 2^9 of its row's largest is exact there (every checkpoint seen so far). A float32 scale (bitsandbytes' `SCB`) has 24, so it
  is rounded at 2^-20 of its row's largest; any scale that is not exact is counted (`quant_inexact`), a relative error of at most
  one part in a million in that scale.
- **A 4-bit code costs a byte in v1** (the program's weight dtypes are `i8`/`i16`); a packed `i4` dtype is a v2 candidate in
  `freeze-v1.md`. A 4-bit checkpoint's artifact is therefore about twice its packed size.
- **Float targets take the W8 path**: the dequantised weights are quantised per row by the lowerer's policy. For a format whose rows
  were quantised row-wise by absmax this recovers the same integers; for a codebook format (NF4, FP4, FP8) it is a re-quantisation,
  measured by the pack's reference fit.
- **bitsandbytes, in detail.** The 4-bit weight is a `uint8 [⌈N/2⌉, 1]` of the flattened row-major weight, the even element in the
  high nibble, with one `absmax` per `blocksize` flat elements and a 16-entry code table *stored in the checkpoint*, so the format
  is table-agnostic. **Double quantisation is exact**: the nested absmax is `nested_quant_map[absmax] · nested_absmax[block / 256] +
  nested_offset`, two float32 operations, each a `rnd(·, 0)` of a float64 operation on float32 operands. An element is the float32
  product of its code and its absmax, rounded once to the module's dtype (bitsandbytes' CUDA kernel's arithmetic; its pure-PyTorch
  fallback rounds the code table to that dtype first and differs by up to one bfloat16 ulp). For FP4 the kernel's literal for code 1
  (0.00520833) differs from the stored table's (0.0625 / 12 in float32) in the seventh digit; the descriptor follows the stored
  table. **Refused by name**: `bnb_4bit_quant_storage` other than `uint8`, and a hardware-reordered int8 `weight_format`.
  **`llm_int8_threshold` is inert (decided 2026-10-01).** Above 0, LLM.int8 splits every matmul at run time (activation columns past
  the threshold go through float16 against the dequantised weight columns, the rest through int8); that is how bitsandbytes *runs* a
  layer, not what is stored, and `BitsAndBytesConfig`'s default is 6.0, so most int8 checkpoints published carry it. The stored
  weights define the model, whatever the threshold. The pack's fidelity measure against the Hugging Face reference (§II.4.4) says how
  far the integer program is from the library's own run, and it must still pass; the `SCB` rounding stays counted
  (`quant_inexact`).

##### II.5.9 Adding a format

1. Write the descriptor: `ids`, `params` and `config` (what the configuration says), `layout` and `decode`.
2. Write an *independent* reference for the vectors (the library's own function where it can be imported; else a transcription from
   its source, said so in the descriptor's `doc`), generate random valid inputs and the expected values, and add them as `tests`.
3. `QuantFormat::from_json` checks the vectors; the registry refuses a redefinition of a type, a method or conditions it holds.
4. Supply it with `--quant-format <file>` (or a pack's `quant.descriptors`) for a conversion that needs it; promote it to
   `quant-formats/` and the built-in list when it is general.

No code change is needed unless the language itself cannot say the format; a gap in the language is a general addition (`from_role`
and `float_<role>` and `rnd` were added for bitsandbytes, `virtual` for MXFP4), tested once, and available to every later format.

---

<a id="source-v1-0002-L02079"></a>

### 旧RFC-0002 / 原文2079–2295行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

#### II.12 Local-LLM coverage: active kernels and an explicit extension queue (R8/A8)

*Added 2026-10-03; revised 2026-10-06 under ADR-0172. The historical 90/10 VM split is withdrawn.
This is not a present 90 % measurement. It is narrower than A6's all-Hub denominator and broader than the old
decoder-only safetensors corpus. [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md) define kernel-only extension.*

##### II.12.1 Define “90 % of local LLMs” before counting

Publish a dated, reproducible frame `L_listed` of publicly discoverable models offered for **local text generation**,
including local chat/reasoning/coding checkpoints, MoE and hybrid models, safetensors and GGUF-only releases,
pre-quantised weights and adapter-plus-base chains. The initial discovery frame is the Hugging Face Hub's pinned
text-generation and GGUF listings plus the local-model registry entries in Ollama's library; add other public
registries with a named snapshot and method, and disclose missing catalogues. HF's [Hub API](https://huggingface.co/docs/huggingface_hub/package_reference/hf_api)
supports model listing and revision metadata; its [GGUF catalogue](https://huggingface.co/docs/hub/gguf) is a
separate format signal. [Ollama's library](https://ollama.com/library) is an additional local-distribution source.
No finite catalogue is literally every model “in the world”; the claim must name the surveyed sources and date.

The counted unit is one pinned model checkpoint or complete base-plus-adapter chain, with its intended local task;
exact mirrors of the same weights collapse by digest, while changed weights remain distinct. Multiple quant files
of one checkpoint are a **format-coverage** stratum, not extra votes in the headline. `L_files` is the subset whose
complete weights and task metadata are publicly readable; inaccessible gated entries, missing files and unresolved
bases stay in `L_listed` with named blockers. Unknown use rights remain in `L_files` and fail the `source` gate as
`RIGHTS_UNCONFIRMED`; they are never silently removed from the denominator. Report both denominators, feature-family
and size strata, and repo-weighted and demand-weighted views. A vision-chat model counts for its advertised full
input task only when the needed RFC-0003 profile exists; a decoder-only extraction is separately labelled.

##### II.12.2 Actual kernel routes and counted gaps

For every `L_files` member, run §II.10.3's source → lower → pack → admit → seat → claim/`Final` funnel at its real
size. Name the route `ACTIVE_KERNEL`, `KERNEL_EXTENSION_REQUIRED` or another exact blocker; publish kernel/suite/
plan ids and retain earlier blockers if an upgrade later succeeds. A pass requires a registered whole-task class,
independent capable seats and actual `Final` under the active binary. New probabilistic profiles require approved
encoded/algebraic checks, whole-claim error accounting, positive receipts, DA and the elapsed challenge window,
with bounded exact court on dispute—not routine full segment replay. An inactive proposal, a tiny fixture or a
`Final` for a partial task is not a pass.

The target is **at least 90 % active-kernel passes over `L_files`**, with a one-sided 95 % lower confidence bound at or above
90 % when a probability sample is used. Report the exact numerator, denominator, weights, sample design and date.
Publish the residual extension/resource queue: `KERNEL_EXTENSION_REQUIRED`, `CHECKER_LIMIT`, `COURT_LIMIT`,
`DA_LIMIT`, `SEAT_CAPACITY`, `NEEDS_JOB_PROFILE`, `SOURCE` or the actual first failing gate. Count a later extension
only after activation and the same full-task drill. Unsupported, untested and resource-limited models remain
failures in the denominator; there is no automatic route for “the other 10 %”. Report each kernel family, the
aggregate end-to-end rate and the still-uncovered fraction.

##### II.12.3 Engineering order and the upgrade rule

Close frequent format, importer, quantisation, static-feature, real-size admission and seat-throughput failures in
RFC-0002 first. Group missing relations into reusable versioned kernels rather than one extension per model.
Add semantic/checker/court vectors and whole-statement soundness/resource evidence, shadow-test, coordinate
fingerprinted activation, then rerun the cohort. Missing importers stay off chain; missing canonical jobs remain
RFC03 tasks. A heavy operation outside active limits is **not** covered: measure tiling/composition improvements,
then propose a bounded kernel/limit change with checker, DA, court and panel evidence. Do not implement a VM
fallback. A frontend pack cannot change consensus semantics or bypass a cap.

A8 closes with published `L_listed`/`L_files`, ≥90 % active-kernel success meeting the confidence requirement,
real-checkpoint `Final` references, sustained panel evidence and a complete residual table. This is not 100 %
coverage: remaining kernel/resource/source/rights failures prevent an unrestricted coverage claim. RFC11's
broader all-HF target retains `D_all`; neither target implies the other.

#### II.13 Immutable challenge-policy binding and later conformance evidence (2026-10-08)

[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the sole
post-commit challenge protocol. For a new versioned class/profile, bind the active KernelDescriptor digest,
VerificationPlan root, `challenge_policy_id`, program/artifact/tokenizer/input-schema/layout/context commitments
and RFC14 dispute-plan digest before any challenge randomness. Resolve checker/challenge/soundness as the approved
immutable tuple in the [Kernel design](../design/palw/models/versioned-kernels.md); a declarative plan cannot select weaker randomness, omit relations or upload a checker.

| Immutable identity/statement bindings | Later conformance or activation evidence |
| --- | --- |
| Kernel descriptor, semantic/checker/court/soundness suites, challenge policy, plan and material/layout identities | Actual source references, beacon/lock descriptor, challenge seed, selected queries/vectors, transcript, outcomes |
| Canonical class/profile/candidate id fixed before future work | RFC13 ConformanceCommitment submission provenance and BeaconConformanceEvidence tied to that fixed id |

Do not put an actual beacon or sampled row into class identity: a new beacon does not rehash the same class. Changing
artifact/layout/plan/policy creates the appropriate new class/profile commitment under a separately versioned encoding;
legacy hashes and already-bound claims keep their old meaning. No new codec/hash domain or network activation is
assigned by this document. Claims/receipts/cache keys/material manifests bind the applicable identity and policy.

RFC11 §17 defines Static Admission → Beacon Conformance → Active Eligibility. RegisteredDormant can precede the
beacon, but neither conformance agreement nor an unknown-op sample promotes it to Active. Full semantic/constraint/
court/public-material/resource admission remains mandatory; the beacon selects tests, not semantics. Independent
runtime-pack conformance follows RFC13, while per-claim verification and public prosecution use the same RFC07 policy.
All newly proposed bindings/lifecycle gates need an explicit implementation/migration before use.

#### Mission alignment amendment — 2026-10-07

§5、§II.7、§II.10–II.12のadmissionにpublic-verifier dispute completenessを追加する。静的cone/cost検査、kernel catalog、conformance、readyな独立seat、admission jury、FamilyCertifiedだけでは報酬・weightを有効にしない。boundary/history/checkpoint、fused reductionのdissectionとcloseも、fresh non-seat verifierが公開取得した証拠から作れる必要がある。「登録時にadjudicabilityが確定する」はこの追加gateを含む場合に限る。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)を適用する。

canonical TIRの実行statement、実仕事量、承認済みkernel/planは正確に保持する。容量倍率による経済的権利の細分化を、計算量を小さく偽装する意味へ読み替えない。
全reward profileのclaim/block/reward/Final weightは同じproducer資本・期間の予算へ束縛し、正当な計算と公開検証の条件を維持する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

#### II.14 Completion contract — 2026-10-10

This revision incorporates the model-extensibility design review. All six requirements below are
part of RFC02 completion, together with the existing A1–A8 criteria. The implementation ledger
records evidence and unresolved work; passing a newly added unit test cannot close the whole RFC.

##### II.14.1 Scoped support and the public extension interface

The support record key is **checkpoint revision × complete task × context range × storage format
and arithmetic version × kernel/verification/dispute plan**. Record structure/lowering, actual
weight import, source fidelity, real-size execution, ordinary verification, public localization/
terminal court and independent real-node Final/redemption as distinct stages. Each has measured
evidence, `FAIL`, or `NOT_RUN`; a downstream pass is never inferred. Record partial tasks, fixtures,
and reduced contexts explicitly. They cannot count as real-size/full-task coverage in A6/A8.

Complete both §II.11 paths through the same node binary: independent canonical TIR and a
content-addressed declarative frontend pack. They consume all math-affecting config keys, tensors
and components, use bounded expansion before allocation, and cannot depend on an official signer,
model name, registry entry or compiler. Unknown semantics fail the shared admission gate. Source
equivalence remains unverified until independently reproducible conversion and fidelity pass.

##### II.14.2 Bounded dimensions, sparse computation and state

Preserve v1 bytes, meanings and courts. New dimension/index/state semantics require a versioned
program/kernel contract covering the entire execution, checker and court path. Token length,
patch count and audio-frame count have declared maxima and job-committed actual lengths. No claim
or later slice can enlarge them. Sparse access specifies index length/range, duplicate policy,
selection and tie rules, and the binding of selected values to the original state/weights.

Long-context acceptance must derive cost and evidence bounds at the requested length; raising
the v1 history constant alone is insufficient. MoE lowering must preserve selected-expert work;
sparse attention must preserve sparse access; recurrent updates must preserve state reuse. For
any unavoidable asymptotic or constant-factor expansion, report the reason, real-size ratio and
resource failure separately from structural representability. Measure execution, proof creation,
hashing, retention and delivery early, alongside bit-exact backend conformance.

##### II.14.3 Storage, arithmetic and fidelity

Storage decoding (including GGUF, MXFP4 and FP8), consensus arithmetic (width, rounding, saturation,
normalization), and backend scheduling are independent identities. Decoding a format does not
establish fidelity; a backend changing committed results requires new semantics, not a tolerance.
Fix the evaluation corpus, contexts, metrics and acceptance thresholds before the run. Measure
logits and task quality, routing agreement for MoE/sparse selection, long-context degradation,
and state saturation/continuity where applicable. Keep failed and missing dimensions visible.

##### II.14.4 One derived resource contract

Derive from the admitted program/plan and the committed job dimensions a resource vector for:
maximum execution operations; executor working memory; state/history bytes; evidence-generation
work; ordinary verifier work; on-chain checker work; court bytes/operations/rounds; evidence
retention/delivery; and aggregate concurrent claim/session load. Registration, claim admission,
proof ingestion and court processing must read the applicable same derived contract, rather than
trusting independently declared cheap values. Arithmetic is checked; reject overflow, false
shapes, enormous broadcasts and compiler expansion before their large allocation/work begins.
Per-block, per-bond and network-wide aggregate limits are required in addition to per-claim caps.

An executor's chosen memory budget does not set consensus capacity. Validator/court memory bounds
remain independently enforced even when a producer owns a large GPU. New resource-contract or
limit semantics require an explicit version/fence; legacy admission keeps its exact meaning.

The pre-activation `palw_probabilistic_constraints_v1` route now prices structural claim
admission from the registered commitment envelope and public schema, using the same byte-work
tariff and persisted block budget as public courts. Program, pipeline, segmented and typed
claims check their class envelope before copying traces; registrations must fit admission's
share and a worst court must fit the work reserved for proofs. Onboarding judgements and kernel
objects use the same budget transition. This changes hypothetical admission behind that dormant
fence; it does not change the live legacy route or the frozen TIR/plan grammar. SDK program
preflight applies the same admission/proof-work limits. This is **partial resource accounting**:
the byte tariff does not certify hardware instruction counts, and the complete executor,
evidence-generation, aggregate state/network and semantic checker resource vector remains open.

##### II.14.5 Work and economic conservation

Resource bounds, canonical work and economic credit are separate. Apply defined canonicalization
to dead/duplicate/redundant expressions; do not claim to decide semantic usefulness in general.
Preserve work identity across roots, slices, riders and model copies, and charge derived rights
to the same principal and window. Excess PWU, redundant computation, model duplication, splitting
and resubmission cannot enlarge B/R/F ceilings. A faster correct evaluation is valid: claims bind
results, state and constraints, not a count of physical GPU instructions. Test role swapping and
the complete accounting path, not only a work-count helper.

##### II.14.6 Whole-statement public verification and completion evidence

A new reusable feature ships as **semantics + type/range rules + costs + ordinary checker +
authenticated evidence binding + localization + terminal court**. Execution-only support cannot
enable rewards. Bind source weights, routing, previous state, KV positions, checkpoint/slice
continuity and outputs across the full graph. Sampling guarantees require whole-claim probability
accounting; primitive court coverage alone does not establish detection probability.

Required adversarial drills include shape/product overflow and huge broadcasts; tiny frontend
expansion bombs; substituted/duplicate experts and weights; reset state, shifted KV and skipped
slices; silently ignored config/tensors; wrong backends and artifact roots; floods of invalid
proofs/courts against an honest producer; duplicate work under different labels; and aggregate
concurrent overload. Attribute invalid requests' costs to the attacker and preserve the honest
producer's opportunity to answer. A fresh non-seat verifier must reach objective conviction using
public authenticated material, and an honest independent participant must reach Final/redemption.

The public onboarding verifier now derives the attempt, bound kernel program, Finals and v3
source seals from one complete op-211 row snapshot. Every page must retain the same DAA, roots,
header and total count; ordered rows, advancing cursors, completeness and both reconstructed
roots are checked before verification. The candidate, descriptor, program, plan and parameter
link must agree. This checks consistency with the served roots, not their canonical-chain
authentication: the caller still needs its node/state-proof trust. The 128 MiB retained wire-byte
ceiling is local reader policy, not a bound on total verifier RAM or a consensus capacity.
Complete-check attempts now dispatch from the same snapshot to exhaustive file replay, with the
chain's domain bounds checked before tensor collection. Pinned pagination on a continuously
advancing node, real-checkpoint public conviction and independent Final/redemption remain required
completion evidence. `palw-class kernel-preflight` also accepts canonical TIR without a frontend
or model configuration, reports the shipping schedule and applies the node's hypothetical bounds
at exactly the requested positions; it grants no registration or reward eligibility.

Use pinned real checkpoints for dense baselines, GDN/KDA and unequal QK/V heads, MoE/routing,
Mamba/hybrid state, sparse/compressed attention and complete multimodal tasks. The review's model
names are discovery candidates, not a supported list; pin revisions and verify their exact required
features before choosing tests. RFC03 supplies encoder/image/audio stage bindings; RFC07/11/14
supply whole-claim verification and G14. Conditional verification (A) remains: obtain the correct
model externally; do not reintroduce model-distribution consensus. Missing source access or a
zero detection floor remains a failed security condition, never repaired by type/cost checks.

---

<a id="source-v1-0002-L02315"></a>

### 旧RFC-0002 / 原文2315–2357行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

###### 14.8 Generic bounded preparation of real parameter commitments

The SDK and `palw-class kernel-params` prepare the existing segmented-kernel v3 parameter map
from canonical artifact byte ranges without model/config/frontend selection. The raw-width
builder reads every element once and reproduces both commitment trees using checked bounded
buffers; it allocates no complete tensor or i128 expansion. Every model-instance workspace and
the aggregate payload cap are checked before any payload read. Decoder/encoder parameter roles
follow the explicit descriptor memory model; unknown memory models refuse, and v5 job-input slots are
excluded only under encoder semantics. Partial preparation publishes no map.

On the full real Qwen2.5 pilot artifact, all 1,636 instance roots and the complete param map agree
with decoded reference hashing. Streaming measured 57.985 s and 38,223,872-byte maximum process
RSS, versus 223.809 s and 4,131,471,360 bytes for the decoded diagnostic. These local debug
measurements concern producer/verifier **root preparation**, with explicit off-chain caps. They
change no wire preimage, kernel identity, resource price, network activation or economic right.
They do not prove source-model fidelity, execution, public node prosecution or Final. Commands,
bounds, raw evidence and the remaining full-scope work are in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).


##### 14.9 Exact context and actual-weight node execution evidence

Local executor conformance must compare exactly the requested positive number of positions,
within the canonical program's history bound. It must refuse zero or oversized requests before
reference parameter acquisition, rather than silently substitute a shorter context.

The generic `check-kernel-node-execution` SDK example first checks every actual decoder
parameter instance against a supplied prepared v3 map under explicit off-chain limits, then
compares the node's typed CPU executor with the integer reference. It requires no ModelSpec,
model name or official compiler. This is a local diagnostic, separate from registered-model
public verification and chain admission.

On the full real Qwen2.5 pilot artifact, the predeclared two-position protocol authenticated
all 1,636 instances and compared 790 commit points and both complete vocabulary logits;
every comparison matched. Four node conformance tests passed. Comparison elapsed was
381.881 s; whole-process maximum RSS was 6,442,958,848 bytes. That process includes reference
and native execution plus preparation; it does not establish native producer speed, ordinary
verifier memory bounds, source-model fidelity, full context/task support, public conviction or
Final. The same-node-binary registration and actual-weight public claim/court path remain
required. Protocol, commands, scope, raw results and hashes are in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).

---

<a id="source-v1-0002-L02384"></a>

### 旧RFC-0002 / 原文2384–2645行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### 14.11 Requested-claim identity and actual-weight node delivery path

An outsider consuming a segmented public record must bind it to the **requested claim id**,
not only the class header and trace roots. The final delivered id is not fed into the trace.
`SegmentedClaimRecordV1::view_for_claim` recomputes the canonical claim id from its job,
producer, all delivered ids and evidence root; it also checks canonical parameter ordering,
prompt authentication, exact output length/vocabulary, context and evidence input binding.
Its header must be anchored in authenticated class/chain state. This local identity check is
not a substitute for chain inclusion or an authenticated RPC state proof.

The actual-weight 32-position Qwen2.5 delivery witness now has a node integration test:
registration data in 32,000-byte ordinary chunks, prompt tiles, salted claim carriers,
production RPC op 210 JSON, outsider proof, block court, reservation slash, honest Final,
coinbase redemption and a second node's replay. Activation validation, parameter attestation
and OPV eligibility are still the parent's explicit test seams. Excluding the eligibility
hook, the class is `NotOnboarded`; the stateful program cannot use the small-stateless complete
check. The separate V2 inventory root is not registered or confused with the kernel root.
These mechanics do not establish model onboarding, public DA acquisition or whole-claim G14.
The full requirement and these gates remain in force.

The segmented bootstrap had a metadata cycle: only OPV can register v4/v5, OPV eligibility
requires kernel binding/conformance, and kernel binding required a registered class. The
conformance-only candidate path in §14.12 breaks this dependency **without granting executable claims,
rewards, Final weight or beacon-source rights** before the existing independent gates pass.
The legacy Panel bootstrap is not an allowed fallback for segmented classes. Sampled
conformance remains a non-reward signal pending its digest/refutation courts; its signal or
a test hook cannot substitute for release eligibility.

Allocation-free v3 leaf counts are now used for court byte pricing. Rows and strided columns
retain exactly their existing leaf coverage, wire sizes and tree-choice tie rule, and the
actual 32-position plan root is unchanged. This eliminates transient index vectors created
merely to count elements during repeated ledger reconstruction. No descriptor, price,
commitment preimage, resource ceiling or activation flag changes. Final test evidence and
remaining full-scope work are recorded in the
[RFC02 implementation ledger](../design/palw/tir/rfc0002-implementation.md).


##### 14.12 Conformance-only class preparation

`RegisterConformanceClass` is route version 1, new inner tag **21** of outer tag 110.
It requires both the existing route fence and `palw_panel_free_v1`; shipping fence validation
is unchanged. The preparing signer must be a non-exiting bonded actor. It carries exactly a
known descriptor, canonical program, exact verification plan and parameter commitments. No
model name, `ModelSpec`, frontend vendor or compiler approval participates in this contract.

Preparation calls the same registration validation for active kernel semantics, ranges,
parameter instances, independently attested parameter root, prosecution bounds, court work,
OPV economics and carrier envelopes. It omits only the eligibility prerequisite that would
otherwise depend on the conformance it is being prepared to obtain. A successful candidate
consumes one ordinary adjudication run and its derived admission byte work from the shared
block budget; it cannot spend the public-proof reserve. At most **eight candidates per bond**
can stand. The ordinary wire ceiling still applies. This per-bond catalog bound is not a
proof of the remaining system-wide resource and honest-liveness requirements.

A candidate lives in ledger **table 28**, keyed by its eventual OPV class id, with value
`borsh(preparing bond, ClassRecordV1)`. The root is extended only when that table is nonempty:
`H("misaka-palw/kernel/ledger-conformance-extension/v1"; previous root || collection root)`.
The collection domain is `misaka-palw/kernel/ledger-collection/conformance-classes/v1`.
Older object discriminants, metadata encodings and roots of ledgers with no candidates remain
unchanged. Row reconstruction checks the candidate's key against its canonical OPV identity.
The preparing bond receives no exclusive ownership of the computation or promotion priority.

`KernelBoundV1`, conformance checking and derived eligibility can read the checked candidate
metadata. They still require the same V2 canonical program, live binding over that exact
parameter root, conformance statement, policy, resources and clocks. Execution paths read
only ordinary registered classes: preparation alone opens no job/escrow/claim, pays no reward,
creates no Final, supplies no Final weight and contributes no work-beacon source. Ordinary
`RegisterClassV2` promotes the same identity only after its independent derived eligibility
passes, and consumes the candidate. The candidate cannot make a stateful or large class
eligible for the small-stateless complete check; sampled evidence still does not gate release
rewards pending GAP-70. No production activation is implied.


##### 14.13 Bounded v3 artifact-identity refutation

Tag 105 appends `ArtifactMismatchProofV1::TileV3` as proof discriminant **2**.
The old instance-set and row variants retain discriminants 0/1, encoding, judgement and
fold behavior. The new court binds a carried parameter map to the exact bonded kernel root,
authenticates one v3 tile against the named instance, and authenticates the canonical V2
inventory piece against the registered class root. Authenticated dtype/shape differences or
different bytes at common coordinates refute the binding. Rank, dtype, instance, leaf index,
leaf count, canonical piece length and both roots are checked. A copied or disjoint opening
proves no fault. Both row and strided column tiles are supported: a forged column tree must
remain refutable even when its paired row root is truthful.

`ArtifactTileOpeningV3` uses the existing leaf wire bytes, with lengths checked before
allocation: rank ≤ 4, elements ≤ 4,096, path ≤ 64. It does not change the kernel leaf codec.
The inventory material is checked before hashing against its 32 KiB piece and 32-level path
ceilings. This court opens neither a full model tensor nor a whole execution trace.

Only this appended proof spends the shared block court/run budget, before decoding the
stored program or judging either opening. Its work tariff is serialized proof bytes plus
stored canonical program bytes plus 16 times opened element count. A refutation may use the
existing proof reserve. A charged judgement that proves nothing slashes the challenger's
free collateral by `dismissal_fee_v1(work)` and persists that fee and budget as an accepted
dismissal. An over-budget object runs no court and pays no judgement fee. Neither outcome
slashes/defaults the honest binder. A valid proof uses the existing one-time reservation
slash and reporter share. The dormant route fence and shipping validation remain unchanged.

The SDK can stream one V2 inventory opening with a 32 KiB piece buffer, its existing bounded
read-ahead window and logarithmic sibling-subtree frontiers, rather than retaining all
weights or all inventory leaf hashes. The core `tile_refutation_v3` builder checks public
openings through the same pure court. Model acquisition remains conditional; this adds no
consensus model-weight retention or provider obligation. Refutation here requires an authenticated
opening of the **bound** tensor as well as the verifier's true inventory bytes. An arbitrary
opaque false commitment with no public opening is not made refutable by this comparison
alone; that residual binding/prosecution requirement remains open. The generic
`prepare-artifact-tile-court` diagnostic reauthenticates all v3 instances before producing
an honest and a one-element false-binding witness; it consults no model/compiler allowlist.

The predeclared tests and full-scope exclusions are recorded in
[`rfc02-artifact-tile-protocol.json`](../design/palw/tir/evidence/rfc02-artifact-tile-protocol.json).
This identity court cannot substitute for stateful release conformance, full computation
verification, source fidelity, full task/context support or shipping activation. The v5
bridge must still explicitly distinguish artifact parameters from its two job-input roles;
v3 root preparation alone does not resolve that inventory contract.

##### 14.14 Descriptor-scoped model inventory and job-input roles

A model artifact binding must authenticate **descriptor identity, complete canonical program,
model inventory root and kernel parameter root together**. The challenger cannot choose the
role contract. K2-TIR-v4 treats all declared params as artifact bytes. K2-TIR-v5 treats its
validated trailing `input.ids` and `input.count` as job inputs, leaving the preceding params
as model bytes. Identical tensor names under v4 do not grant the v5 exclusion. Unknown kernel
semantics cannot copy a memory-model number to gain it; a new implemented descriptor needs
its own explicit contract.

The role scope retains the complete validated program, including its input references. It
must not remove the last two declarations and validate a rewritten graph, read synthetic
job placeholders from the model source, or count job inputs as missing model tensors.
Inventory leaves retain the existing tensor-name/layer/byte-offset hash grammar and
32 KiB piece limit. Their count and index are derived from the descriptor's model instances.
The entire scope belongs to the authenticated statement, even when two scopes share a root.

`PalwTirModelInventoryV2`, bounded streamed model roots/openings and the pure
`verify_artifact_mismatch_v2` judge implement this role foundation for exact current v4/v5
identities. The new judge accepts instance-set proofs and bounded v3 tiles; legacy whole-row
proofs remain in the old judge. The BERT and XLM-R weight-bearing fixture trial tests honest
no-fault, changed/missing/surplus model weights, copied source/kernel roots and rejection of
unknown semantics. It proves mechanics at the fixture's scope, not a full checkpoint task.

**Foundation snapshot (`d5ed40277`): node integration was still required.** §14.15 records
the subsequent consumer implementation. Historical `ArtifactBoundV1` (104) and its challenge
(105) retain their all-parameter inventory and existing wire meaning. They cannot be converted
into v5 statements using a later binding or names inferred from an untrusted proof. A new
versioned, signed and rooted statement must persist the full scope, apply collateral and
shared resource budgets, and expose only a matching matured attestation before a v5 candidate
can pass through the ordinary node. The new pure APIs do not supply that statement, activate
an encoder, establish release conformance, resolve opaque-root withholding, or prove RFC02
completion. Large/stateful conformance, full-task public G14 and the other §II.14 requirements
remain mandatory.

##### 14.15 Signed model artifact statements in the node consumer

The new route inner kinds **22 `BindModelArtifactV2`, 23 `RefuteModelArtifactV2` and
24 `RegisterModelConformanceClassV2`** ride signed outer 110 and require both its
probabilistic-constraints fence and the dormant PanelFree fence. The central allocation
and exhaustive inner fence table name all three. The bare kernel ledger refuses them:
only the node consumer can authenticate chain collateral and the stored scope.

A signed statement commits the exact current v4/v5 descriptor, complete canonical program,
model inventory root and parameter map. Its content id binds descriptor, program digest
and both roots. Aux **46** stores the immutable statement/liability record, **47** its bounded
bond index, and **48** the admitted candidate's chosen statement. Tables 43–45 belong to
the provider court; the earlier model allocation overlapped them and is corrected before
shipping activation. Table 42 remains reserved
for the existing chunk lane. Each bond can retain eight statements, including decided
records. Identical content is reusable by any candidate registrant; the binder's identity
owns liability and grants no execution or reward priority. The reservation is the existing
100-BILI binding amount and uses the existing challenge/liability horizons.

The fixed-size stored header suffices for reservation, deadline and work reads. Per-bond
committed collateral and both live-duty withdrawal gates count the reservation until the
stored liability horizon or refutation. Release is a clocked read, requiring no new global
per-block scan of model programs. Binding and refutation spend the same blue-score block
budget as other admissions/courts; admissions cannot spend the prosecution reserve. The
metadata work tariff is `p² + 64p + 64b`, with `p` canonical program bytes and `b` map/filing
bytes. It is a conservative structural envelope, not execution work or economic credit.
A charged invalid proof pays the policy's dismissal fee; an over-budget proof is not judged.
The public decoder checks PC cardinality, ordering, tile bounds, inventory name/piece/path
counts and trailing bytes before any corresponding allocation. Legacy whole-row proofs
are not admitted here. Valid fraud slashes the statement reservation once under the existing
49% challenger share; self-operator bounty collection is refused.

A matured statement attests only the exact descriptor/program/PC tuple carried by inner 24.
The consumer delegates the ordinary conformance-only validation, including ranges, plan,
prosecution and carrier bounds, and records its chosen statement. Its temporary PC
attestation is restored before ledger rows are persisted: it never enters the global
attested-root list and cannot be borrowed by an unrelated class. This creates metadata,
not an execution registration. Historical 104/105 and their inventories are unchanged.

The node mechanics trial uses a lowered weight-bearing BERT fixture and ordinary signed
carriers, with independent replay. The companion SDK capture also covers the XLM-R fixture.
This is not a published full-checkpoint task/context support PASS. Generic scoped release
conformance, full computation G14, the arbitrary opaque/withheld bound-root case, the complete
resource/economic/liveness contract and shipping activation remain open under §II.14.

##### 14.16 Descriptor-scoped finite-domain conformance reference

`ModelConformanceDomainV2` derives a finite input domain from the complete validated
program and verification plan, using the exact current v4/v5 descriptor. For v4 it
includes every nonempty token word of lengths `1..=plan.max_positions`; for v5 it
includes every word of lengths `1..=L`, with the encoder's canonical ids padding and
count injected as job inputs. Stateful decoder cases start from the reference's initial
state independently and replay the complete prefix. Repeating one token does not cover
this domain. Stateless programs use the same domain without a dependency-based shortcut.

Before model tensor decoding or execution, the constructor caps input cases, model leaves,
raw model bytes, a conservative reference RAM envelope, and reference work. The envelope
includes all retained node values at worst-case H, declared states and constants, decoded
model parameters, job inputs, operand clones and primitive scratch. These are local
reference limits, not a change to network admission limits or proof that the full shared
resource vector is complete. The constructor also checks the plan's program binding and
ranges. Callers cannot supply a smaller input count or work estimate.

The reference validates every inventory coordinate, re-roots the exact source bytes,
recomputes the v3 parameter commitments from those same bytes, and hashes every node's
shape, dtype and integer values for every input case, including stage inputs and state
writes. The result binds descriptor, complete program, plan, source and PC roots, case
count and input order. Comparing claimed implementation roots proves equality with this
reference result; it does not establish the identity or independence of the implementations
that supplied those roots, or fidelity to a source floating-point checkpoint.

At the `f7f93d0d3` reference snapshot this was a pure dependency without a node consumer.
The authenticated consumer is specified in §14.17 below. Large
vocabularies/context domains exceed exhaustive enumeration; they still require the scalable
GAP-70 digest/refutation path. No input subset, sampled signal or raised limit substitutes
for that requirement, and no shipping fence is armed here.


##### 14.17 Authenticated finite-domain conformance and direct execution registration

Inner **25 `CompleteModelConformanceV2`**, carried by authenticated outer 110, requires
both probabilistic-constraints and PanelFree fences. It names a scoped candidate and a
matured model statement. Any active bond may submit the complete source inventory and
three claimed implementation roots; neither the binder nor the first registrant receives
priority. The consumer checks the exact descriptor/program/plan/source/PC relation and
every input in §14.16's bounded domain before writing a fixed-size conformance row in aux
**49**. Aux **50** counts judged checks per blue-score block, with a maximum of two.

Before decoding the stored program or proof, a judged attempt reserves shared admission
work equal to `p² + 64p + 64b + 2^26`, checks the actor's free collateral, and charges the
actor's dismissal fee on success or failure. Proof bytes, operand counts, names and piece
lengths are bounded before allocation. Invalid attempts create no conformance record and
do not slash the model binder. An already checked class is not rerun. A successful check
selects its proved statement in aux 48, so an earlier candidate with a false declared source
root cannot permanently poison the class.

Ordinary inner 13 `RegisterClassV2` may then derive OPV eligibility from that exact checked
scope. Admission rechecks descriptor standing, denial, binding standing, plan prosecution
bounds, carrier fit, court work and economics, and the complete-check policy. Temporary PC
attestation is removed before persistence. This path introduces no model-name or compiler
allowlist, V2 decoder identity, persistent global attestation, or new payout. A candidate
alone is not an execution registration or beacon source.

This consumer certifies the bounded integer function and the source-to-PC relation. Claimed
role roots do not certify independent runtime implementations or source checkpoint fidelity.
Large vocabularies/context remain outside exhaustive enumeration and need GAP-70. The full
shared resource/liveness contract, real-checkpoint task/context coverage and the remaining
§II.14 requirements stay open. Shipping fences remain dormant.

---

<a id="source-v1-0002-L02803"></a>

### 旧RFC-0002 / 原文2803–2854行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### 14.21 Staged flat-v4 commitment material for large withheld values

`misaka-palw-kernel::merkle4` implements the flat hiding-width layout proposed in
`k2-real-scale.md` §4.2 as a **new hash and wire grammar**, not a v3 leaf-rule
replacement. This stage provides commitment construction, public hash-only
openings, filer-owned value openings, bounded root descent and material resource
bounds. No descriptor, class, candidate statement, route object, reward source
or shipping fence selects it. A commitment helper is not a terminal court or a
completed large-model prosecution path.

The canonical layout flattens the row-major tensor across rank/line boundaries.
Let `N` be its checked element count. For `N >= 64`, it has `floor(N/64)` leaves:
all but the last contain 64 elements; the last contains 64 through 127, including
the entire ragged tail. A 65-element tensor has one 65-element leaf; a 129-element
tensor has one 64-element leaf and one 65-element leaf. No padding, short tail
hash or interior hash of that short tail is published. Tensors below 64 elements
have no flat-v4 leaf contract and still need a bounded whole-value alternative.
Rank is at most four; zero dimensions, overflow and non-committable dtypes are
refused. The primitive arithmetic and the existing `I64`/`I128` commit prohibition
are unchanged.

The flat leaf commits its dtype, flat leaf index, exact count and raw-width
little-endian elements under the new flat-v4 leaf domain. A binary tree carries
an odd final node upward; separate flat-v4 node and typed-tensor domains bind
both tree structure and dtype/rank/dimensions. A hash opening carries bounded
metadata, one leaf digest and its exact path; it contains no elements. A value
opening additionally carries exactly the declared span's bytes, at most 508
bytes for the existing committable dtypes. Both codecs check rank/count/path and
carrier limits before allocating variable fields, and refuse surplus bytes.

A model holder authenticates each public reply against the committed typed
value before comparing it with its own tree. A differing leaf is terminal
material for a future relation court. Otherwise, the first differing bottom-up
sibling identifies a strictly smaller divergent branch, including odd carried
nodes. At most `ceil(log2(leaves)) + 1` replies identify a divergent leaf. A
cached tree reads paths in logarithmic time; it does not copy/hash the whole
tree on every probe. The root-only raw-width builder retains a fixed stack
with a conservative 8 KiB scratch bound and does not expand bytes into an i128
tensor. Optional retained trees expose their derived live digest/vector bounds
and can refuse a caller's retention limit before hashing or tree allocation.

The 64-element rule supplies a minimum encoded width; it does **not** prove
entropy, cryptographic privacy of predictable values, or cumulative disclosure
safety. No ordinary participant is required to acquire the model through
consensus. Before selecting this format, the full profile must also bind its
masked nodes and source commitments, implement the union of each leaf's
primitive dependency lines and terminal court, derive court/RAM/work/retention
and aggregate budgets, connect authenticated DA/default and verifier descent,
and pass fresh actual-node prosecution/Final trials. It cannot inherit reward
eligibility from the existing v4/v5 plans. Existing commitments, plans, claims,
candidate v2 statements and every §II.14 completion requirement remain in force.

---

<a id="source-v1-0002-L02929"></a>

### 旧RFC-0002 / 原文2929–2980行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### 14.23 Staged all-dtype internal flat-v5 material and fault kinds

`misaka-palw-kernel::merkle5` and `flat_court5` add a separate, explicitly
selected internal evidence profile. Every node value with at least 64 elements,
including I64/I128 intermediates, uses the new flat-v5 hash namespace with
64..127-element leaves. Smaller node values and registered parameters retain
v3. This closes the component gap where a first wrong wide intermediate could
not use the flat leaf relation court. It does not make wide values legal TIR
commit points: the existing program validation and all 25 integer primitive
semantics remain unchanged. Flat-v4 domains, bytes and public APIs retain their
previous meaning through version-specific aliases of shared implementation.

The explicit v5 fault envelope dispatches four terminal court kinds:

- **Leaf:** recompute a hash-only output leaf from authenticated operand leaves,
  including wide internal operands, using the frozen reference evaluator.
- **Whole:** recompute every element of a sub-64 node and compare its typed v3
  commitment. Producer output values are not required.
- **Malformed:** authenticate the typed header preimage and convict a mismatch
  in dtype, shape or profile grammar. Rank/count caps apply before allocation;
  overflow and zero-dimension metadata can be authenticated without constructing
  the claimed tensor. Unbound headers dismiss.
- **Decode:** authenticate logits at the position selecting the delivered token,
  then compare a rival with at most two filer-owned leaves. Equal logits select
  the lower index. An out-of-range delivered index is convicted from the bound
  node coordinate and declared length, with no value leaves required.

The profile identifier binds this grammar; the envelope has version 5 and a
strict fault-kind discriminant. Unknown versions/tags, truncation, surplus
bytes and oversized nested counts are refused. Source authentication,
validated state/history wiring and bounded dependency recording are shared
with §14.22. The per-filing limits remain one MiB, 2^30 abstract work units,
32 MiB scratch and 1024 operand leaves. Derived limits are checked before
reference evaluation. These are component bounds, not measured network or
worst-job resource guarantees.

Synthetic kernel tests cover all 25 primitive samples and merged tail leaves,
first wrong wide intermediates, small values of all six dtypes and scalar rank,
header type/shape/overflow/grammar mismatches, decode position and tie rules,
and bounded codecs. Independent material oracles cover all six dtypes,
extreme I64/I128 values, root-only hashing and logarithmic divergent-leaf
localization. No descriptor, candidate, class or shipping node route selects
this profile. These tests do not establish real checkpoint support, privacy
entropy, actual-node prosecution, Final/redemption or full RFC02 completion.

Still required: worst-position/leaf admission for the entire profile, typed
node statement binding, public hash-unit DA/default/session budgets and
deadlines, shared aggregate resource accounting, public-chain acquisition,
fresh model-holder prosecution and independent Final/redemption. The original
§II.14 checkpoint/task/context/fidelity, conformance, economic and honest
liveness requirements remain open. Shipping fences stay dormant.

---

<a id="source-v1-0002-L03098"></a>

### 旧RFC-0002 / 原文3098–3373行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### 14.26 Dormant flat-v5 public hash-unit demand, service and default

Inner kinds 33/34/35 add `DemandFlatModelVectorV3`,
`AnswerFlatModelVectorV3` and `SettleFlatModelVectorDemandV3`, under the existing
route/PanelFree fences. Aux tables 59–63 store fixed requests, requester slots,
public responses, open-request counts and unavailability. Shipping presets and
the existing v2/v3 statement encodings are unchanged.

A request identifies a committed position/occurrence/node and optionally a
flat leaf index. Its grammar has no model-file, parameter, row/column or clear
value request. Admission reserves structural work before reconstructing the
stored statement, independently checks its scope and binds the fixed request
to the exact node index/count, segment root, extent, declared dtype and resolved
shape. Small nodes allow only node commitments; large nodes additionally allow
hash leaves of 64–127 elements. A response binds the two node/position paths
and, where requested, the flat-v5 hash path. The explicit answer version and
strict ten-KiB carrier reject old answer grammars, unbounded nested counts,
wrong units/shapes, copied roots, surplus bytes and truncation.

Each requester owns four slots **shared with v2**, and each flat vector permits
at most 32 simultaneously open requests. Open requests reserve the requester's
own collateral. Their service deposit covers the maximum bounded-answer fee
floor and is paid to the statement poster on authenticated service. Published
answer bytes and their digest can be read through the node's native route read
API even after the requester recycles its slot; a fresh verifier independently
authenticates them and uses its own model copy for values and operands. The
response fold reads only fixed request metadata and bounded hash paths; it
decodes no stored program, parameters, trace or clear values. As in v2, these
bounded responses use payload carriage rather than public adjudication slots,
so spending the court count does not prevent service.

A request must leave the existing response deadline and proof grace inside the
statement's liability horizon. An unresolved request keeps the poster's
reservation committed after that horizon. An explicit bounded close accepts
timely public service, or marks the statement **unavailable**, slashes its
remaining reservation once and pays the authenticated requester. A copying
closer cannot take that bounty. Later service cannot erase an earlier request's
missed deadline. Availability default is separate from integer conviction.
The original statement row remains immutable on default; the auxiliary status
is authoritative for refutation, demand and reservation accounting.

The actual-node synthetic trial reads hash openings from persisted public node
state, localizes a late false leaf through bounded adaptive probes, reconstructs
its operands from an independent model copy and files a successful court. An
independent replay node reproduces public replies and ordinary challenger payout
redemption. Additional trials exercise shared v2/v3 slots, invalid replies,
responses after a full court count, slot recycling, held liability and default.
These are native read-API trials, not a deployed RPC/transport drill or real
checkpoint/fidelity acceptance. General first-fault verification and streamed
localization across all relations, proof pre-emption seals, public retention/
pruning, the complete shared resource vector and aggregate DA/network load,
execution Final, privacy/economics and honest liveness remain open. No shipping
fence is activated; full RFC02 completion is still unproven.


##### §14.27 — Generic streamed flat-v5 detection (dormant component, 2026-10-11)

`flat_detect5::check_flat_claim_v5` consumes the authenticated public job, an
independently acquired model checked against registered v3 parameter commitments,
and a public provider exposing only node commitments and flat hash paths. It
uses the frozen streaming integer evaluator, visits positions/occurrences/nodes
in order and authenticates each node path against the statement's segment roots.
`Clean` requires exact agreement at every node and every delivered greedy id; it
is not a sampled security receipt or an execution Final.

The first divergent large value is localized with at most the layout's derived
`localization_replies` hash probes. A small value uses the existing sub-64 whole
relation. The verifier records the dependency union, opens only its own values,
authenticates earlier-node sources and registered weights, and constructs the
existing bounded flat-v5 filing. This covers common primitive wiring, carry,
initial zero states, previous state writes, previous history windows, constants,
public position/token ids, and encoder job ids/padding/count. Decode uses its own
logits and at most two value leaves, including the lowest-index tie rule and
out-of-logits ids. Every emitted filing survives the strict wire roundtrip and
the same terminal court the node uses. No new primitive or statement grammar is
introduced.

Missing or borrowed paths and invalid hash metadata yield an exact point DA
request, never a clean verdict. A malformed committed tensor for which the
producer cannot serve the expected typed hash path still needs the existing
DA service/default process; this detector does not invent a malformed-header
conviction. Wrong/missing acquired weights, an inconsistent job or failed
reference evaluation are reported as inconsistent, not as producer guilt.

The detector retains current/previous position values, with an extra previous
position copy alongside the existing replay engine, and the replay engine's
bounded history rows. It does not retain all positions. It still hashes all
reference values, reads all node paths in a clean scan and builds full own
Merkle trees for filing operands. A retry after a DA request replays the prefix;
caching public responses does not remove that execution cost. Consequently this
component does **not** prove real-size practicality, bounded whole-job delivery,
shared verifier/resource accounting, or completion before the liability horizon.
Those requirements remain mandatory before practical or reward acceptance.

Synthetic tests cover all 25 primitives, state/history across positions and
generated-token feedback, small and I128 intermediate values, encoder inputs,
decode ties/out-of-logits ids, bad acquisition and public-path withholding or
substitution. The actual-node late-leaf trial now drives this generic detector
from persisted public answer bytes through point requests, a court, challenger
payout redemption and independent replay. Native evidence remains a synthetic
Gather case; it is not all-feature real-node or real-checkpoint acceptance. RPC
transport/retention, proof pre-emption seals, aggregate load/honest liveness,
execution Final, checkpoint fidelity/complete tasks, A6/A8 and economics remain
open. The shipping fences stay dormant; RFC02 completion is not established.


##### §14.28 — Versioned local replay budgets over the frozen flat scope (2026-10-11)

`flat_replay6::FlatReplayEnvelopeV6` is a new, read-only resource projection over
the independently checked flat-v5 scope. Its new v6 domain binds the unchanged
v5 scope root. No existing v3 statement, v5 resource serialization/root, court
profile, route discriminant or shipping fence is reinterpreted. The native
`flat_model_vector_replay_envelope_v6` read API derives it from the authenticated
stored candidate view; the local verifier independently rederives all fields.

The registered-model replay adapter checks acquired dtype, shape and element
count against the program declaration before canonical-value scanning or full
commitment hashing, even if a substituted tensor has the supplied parameter
root. The acquisition provider must separately cap its own allocation/transport
before constructing and returning a Tensor.

The projection reports reference tensor **payload**, separately from artifact
storage: every `Tensor.data` value is an i128 even for i8 storage. It derives
maximum position payload, largest operand payload and retained history-row
payload from bounded shapes and actual layer occurrences, without allocating
weights, tensors, trees or position arrays. It prices full v3 acquired-parameter
commitment hashing, both v3/flat node hashing (including earlier sub-64 H and
filing trees), node reads and bounded hash localization replies. Each public
provider call reserves a full ten-KiB native response allowance, including
missing or cached responses; this is a reservation, not measured wire bytes.
Other transports must enforce that carrier bound before parsing.

`FlatReplaySessionV6` carries chosen local ceilings across DA responses. Before
model acquisition or replay it verifies the entire derived envelope and the
class extent, checks tensor payload ceilings and reserves one whole worst-case
replay's parameter/trace hash allowance. A withheld or inconsistent attempt
still spends that allowance. Provider calls spend session-wide node/hash/read
byte quotas before delegation. Attempt/hash/read exhaustion returns an explicit
limit result, never a clean verdict or a producer conviction. Cached reads and
repeated requests cannot reset the same session's counters. The native late-leaf
trial uses this path from independently derived node resources to a successful
court and payout replay.

These are **local verifier limits**, not a new source of economic rights or
network admission. A caller starting a new local session must pay/account for
that work separately. The existing unbounded v5 checker remains available as
a lower-level API. The payload quantities are not peak allocator memory: model
residency, position copies, transient evaluator/sort buffers and allocator
capacity still require complete accounting. They also do not prove wall-clock
or CPU maxima, metadata/prompt/path-verification hash costs, RPC/retention costs,
whole-job deadline feasibility or aggregate per-block/per-bond/network load.
These remaining quantities must join the shared complete resource vector and
its consensus admission before practical or reward acceptance. Proof pre-emption
(including equivalent proof encodings), real checkpoint/task/fidelity acceptance,
execution Final, A6/A8 and the other full RFC02 conditions stay open.


##### §14.29 — Actor-bound sealed evidence and equivalent-carrier counterexamples (2026-10-11)

A proof-byte digest and earliest matching beneficiary lookup alone do not protect
against another valid encoding of the same fault. Two different authentic greedy
rivals can convict the same delivered id. An out-of-range delivery ignores the
rival. An already inline-bound prompt permits a redundant authentic prompt tile
in a relation filing. These carriers have different exact digests while producing
the same conviction. The unchanged flat-v3 native route remains unsealed: a
copyist submitting the alternate carrier first receives its bounty. That is a
security counterexample, not a protected-route acceptance result.

`flat_seal6` adds a separate staged v6 contract. Its salt-bearing commitment binds
vector, authenticated actor and exact bounded proof bytes under new domains.
Every reveal requires the **filer's own mature seal**. It has no unsealed-filer
fallback and does not search other actors' opaque seals for a beneficiary.
Copying a finder's seal fails actor binding; changing its proof fails exact-byte
binding. A new correctly rebound seal posted after observing a reveal cannot
enter a court before its own maturity, including for an equivalent carrier.
An independently prepared mature proof is still admissible; the contract does
not claim protection against censorship until a later copy matures.

The local book has fixed-size metadata rows keyed by `(vector, actor)`, a maximum
of 1,024 rows with a caller-selected smaller total and per-actor bound, and no
stored proofs or historical seal list. An occupied row cannot be overwritten.
A successful court consumes its row once; a failed court leaves it unspent.
Both states retain their bounded slot until explicit, constant-key expiry.
Maturity must precede expiry, which is clipped to the vector's immutable horizon;
all DAA additions are checked. Fixed slot/time checks and bounded raw-carrier
hashing run before a deferred court callback, so an absent, immature, expired or
mismatched seal cannot force a variable stored-program/court decode. Rejected
filings still need consumer fee accounting.

This is **not yet a consensus seal route or protected payout implementation**.
The consumer must authenticate actors and vector/context identity, persist rows
and shared quotas, reserve the same original capital, charge proof/admission work,
and atomically apply one conviction and one payout per vector. A future explicit
versioned protected statement must refuse the old unsealed refutation route;
merely adding a seal object beside that bypass does not close the gap. Economic
fees, aggregate capacity and honest inclusion/deadline feasibility must be
verified before this path is accepted for rewards. Frozen candidate/route grammar,
resource roots and dormant shipping fences remain unchanged. Full RFC02, real
checkpoint/task/fidelity, execution Final and A6/A8 acceptance remain open.

##### §14.30 — Protected native flat candidate statements and evidence seals v6 (2026-10-11)

The v6 protected statement wraps the unchanged v3 candidate body. Its separate
trace-root domain binds the v3 root and the fixed v6 maturity, TTL and capacity
policy. It retains the admitted flat-v5 resource scope and candidate-only status.
Append inner kinds 36 `PostFlatModelVectorV6`, 37 `SealFlatModelProofV6`, 38
`RevealFlatModelProofV6` and 39 `ExpireFlatModelSealV6` behind the existing dormant
kernel-route/panel-free fences. Formal rules are in spec 04b §18. Existing v3
bytes, root preimages, inner 31–35 semantics and resource scopes retain their
meaning; the protected statement cannot enter inner 32's unsealed court.

The consumer derives the actor from its authenticated signing bond and persists
one immutable fixed row per `(vector, actor)`. The salt-bearing seal binds actor,
vector and exact proof bytes. A reveal requires that actor's own mature seal;
there is no unsealed fallback, borrowed beneficiary or earliest-byte-sealer
search. Wrong salt, copied seal, changed equivalent carrier, immature/expired
seal, changed context or an honest/malformed proof cannot settle the vector.

Posting burns the shared admission tariff and holds the route policy's original
seal deposit. Four rows per bond and 1,024 total rows bound simultaneous pending
seals. Retirement/withdrawal and all common capital accounting read this same
reservation. Expired pending deposits remain held until a bounded explicit close
forfeits them; a decided vector releases deposits on close. Occupied slots cannot
be replaced or extend their clocks. Maturity is three DAA; expiry is the lesser
of posting plus 100 DAA and the immutable vector liability horizon.

One proof-reserve debit covers fixed metadata, bounded raw hashing, stored
program/PC/body work and the admitted terminal court, before variable decoding.
Registration includes this debit in its worst-court admission. This avoids a
normal-admission flood consuming the honest reveal's reserved capacity. Refused
proofs charge the authenticated filer. A successful fold spends the seal,
releases its deposit, settles the original vector collateral once and credits
49 percent to the authenticated finder atomically. It grants no execution Final,
conformance certificate or model/task support PASS.

This is the consensus consumer implementation of the previously local §14.29
contract. Native synthetic trials, exact commands and results belong in the
implementation ledger. Whole-network invalid-proof/seal saturation, transport,
retention, worst-job inclusion/deadlines and honest liveness still need evidence.
The full resource contract, real-checkpoint fidelity/task/performance, independent
execution Final/redemption and A6/A8 are unchanged completion requirements. No
shipping fence is activated by this increment.

##### §14.31 — Protected candidate SDK and offline CLI (2026-10-11)

`misaka-palw-sdk::flat_candidate_v6` and `palw-class flat-v6` expose the protected
post, seal, reveal and explicit-expiry operations. Preparing a v6 post first
requires all three source implementation records to agree on the original v3
root for the selected class and binding. It preserves the job, trace and scope,
then rebinds those agreed records to the protected root. It cannot convert
divergent implementation results into declared agreement. Native admission
still derives and checks the actual bound context and resource scope.

Sealing produces a public signed commitment and separate private material that
binds network domain, vector, signing bond, fresh OS-random salt and exact proof
bytes. The CLI creates the private file with mode 0600 on Unix, refuses existing
outputs, bounds input bytes and nested counts, and omits the proof/salt from its
report. Reveal preparation refuses a different network/bond, borrowed row,
changed proof or salt, immature/expired/spent row before writing a public proof.
The supplied row and height are an offline precheck; the native consumer remains
responsible for authenticated chain state and authoritative clocks.

The SDK signs the existing network/bond/bytes message under the existing ML-DSA
context. The CLI writes reviewable objects and, when needed, the existing bounded
generic `ObjectChunk` parts for `misaka palw submit-object`; it sends no carrier.
Generic shared-group capacity does not establish honest proof inclusion before
the seal/vector horizon. Public RPC acquisition, authenticated snapshot/row
discovery, whole-job retention, protected large-proof liveness and aggregate
invalid-proof/seal saturation therefore remain completion requirements.

SDK codec/signature/file trials and actual-node admission/court/payout replay
using these helpers belong in the implementation ledger. These convenience APIs
grant no conformance, checkpoint fidelity, model/task coverage, reward eligibility
or execution Final, and activate no shipping fence. Full RFC02 acceptance stays
open until all independent requirements have evidence.

---

<a id="source-v1-0002-L03410"></a>

### 旧RFC-0002 / 原文3410–3515行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### §14.33 — Protected candidate reads from one anchored public snapshot (2026-10-11)

The protected SDK reader consumes the existing `getPalwKernelRows` (op 211)
response grammar; no new RPC allocation or consensus object is introduced. One
complete snapshot must preserve the tip, both roots, header and declared row
count across all pages, with strictly advancing rows/cursors. The common snapshot
rebuild checks both served roots and refuses a header newer than the tip.

`FlatSnapshotAnchorV6` separately supplies the locally authenticated tip,
network/ruleset domains and ledger/aux roots. All five must match before protected
reads become available. A caller must authenticate that anchor to its own chain
or a state proof. Copying the anchor from the same remote pages proves only
consistency; the SDK/CLI must not report independent chain membership in that
case. An offline file cannot attest its own chain trust.

Candidate reads bind the requested vector/class/model statement to the explicit
protected profile and independently rebuild the same program, parameter/input
bindings and immutable resource scope used by the native court. Seal export
requires the authenticated signing bond's row, an open candidate, and an unspent
mature seal before its expiry at the anchored tip. Private salt/exact-proof guards,
current bond status, clocks and terminal fault validity remain rechecked by reveal
preparation and native acceptance. Snapshot membership grants no fidelity,
execution rights, reward eligibility or Final.

`palw-class flat-v6 snapshot` accepts one op-211 JSON response per NDJSON line and
a separate anchor JSON file, then exclusively writes the checked native seal row
and reports its anchored height for `flat-v6 reveal`. It uses the existing local
128-MiB decoded snapshot ceiling plus a bounded JSON page envelope. JSON byte,
row-count and hex bounds precede allocation of additional binary copies; this
is not a peak-memory, consensus capacity or network-wide load guarantee. Unknown
fields, malformed hex/tables, stray terminal cursors, incomplete/moved snapshots,
root/anchor/context substitutions, unavailable routes and closed/immature/spent
seals fail without publishing an output. Files are never overwritten.

The node component trial uses the actual op-211 response builder and JSON grammar,
a separately authenticated local consensus anchor, a reconstructed protected
candidate/seal, and the existing own-chunk terminal court/settlement/replay.
Synthetic/offline fixture roots establish reader behavior, not chain admission or
actual-checkpoint/full-task acceptance. Public network transport, automatic local
anchor/state-proof acquisition, retention/pruning, whole-job deadline/liveness,
shared aggregate resources and the rest of §II.14 remain completion requirements.


##### §14.34 — Bounded TopK point counterexamples and native-storage selection (2026-10-11)

The frozen v1 rule remains score descending with lower index first on a tie,
followed by ascending selected indices. The reference evaluator now uses a
K-candidate heap for K <= 64 and at most 2K indices in bounded partition blocks for larger K. Selecting the whole axis
emits its known index set directly. The typed execution backend uses the same
bounded-memory strategy and reads the original storage through offset/strides,
including transpose, slice and broadcast views. It does not widen or gather the whole score
source for TopK. Both changes preserve primitive identities, canonical work,
existing program/job limits and exact results; they are backend improvements.
Repeated bounded partitions retain the best K indices of each prefix and cost
O(N) in total before the O(K log K) index ordering. The implementation uses the
in-place linear-time contract in the pinned
[Rust 1.93.0 slice implementation](https://raw.githubusercontent.com/rust-lang/rust/1.93.0/library/core/src/slice/mod.rs).
The ordinary callback-based verifier below retains its separately priced heap
scan; it does not assume cheap repeated reads of remote score material.

An explicitly selected `topk-point-court/v7` adds a separate bounded fault codec
over the unchanged flat-v5 internal/v3 parameter commitments. It is **not**
selected by an existing class, vector, protected reveal, descriptor or default
proof decoder. Its context must bind the exact new court id and the ordinary
program/job/segment/parameter context. Registered parameters or authenticated
node scores are supported; uncommitted/public full-tensor sources retain the
existing court and are refused by this point contract.

Range faults open one selected id outside N; order faults open an adjacent
non-increasing pair, including duplicates. A dominance fault opens a selected
member, a rival score at the same axis row, and the adjacent selected members
around the rival's claimed insertion gap. Those neighbors authenticate omission
from a correct ascending set. A larger rival score, or an equal score with a
smaller rival index, convicts. Every wrong set either has a range/order defect or
excludes a true TopK member that outranks one of its members. A correct set cannot
produce any of these authentic counterexamples. All requested points are bound
to their exact typed roots, coordinates and source wiring; missing, borrowed or
malformed material dismisses. The court opens at most three output and two score
leaves and never reconstructs the N-element axis.

The versioned relation scope derives selection work/auxiliary memory separately
from proof bytes, court work and scratch. The local checker precharges a complete
scan attempt before any score callback; missing acquisition consumes that attempt.
Its 32-MiB selection auxiliary ceiling is a local ordinary-verifier limit, not a
producer memory limit. The 128-KiB filing cap bounds legal parameter and
internal-node score widths; byte, nested-count, shape and path bounds precede allocations. Frozen v1
continues to forbid i128 parameters; i64 parameters and i128 internal-node scores
retain their existing types. Selection/source residency, hashing, evidence acquisition/retention and aggregate
load remain separate prices. A consumer must bind the complete all-relation
scope, normal verification, DA and reserved transport before selecting this court
in a node/reward route.

Missing score material does not convict or default the producer in this court.
Conditional external-verification mode A remains unchanged; this component adds
no consensus availability obligation.

The synthetic 10,485,760-score, K=2 trial executes the reference/backend and scans
an incorrect committed selection. Its terminal filing is 17,485 bytes, below the
independently derived 37,744-byte envelope. This proves a large **fixed axis**
component, not a ten-million-token checkpoint or maximum-length job. Frozen
flat-v5 whole-axis admission still refuses that relation, and existing native
proof routes still refuse v7 bytes. Bounded token/patch/audio lengths, H-axis
selection, all-plan/shared network budgets, full model/task/fidelity/performance,
independent execution Final/redemption and A6/A8 remain required. No shipping
fence or full RFC02 acceptance is activated by this increment.

---

<a id="source-v1-0002-L03587"></a>

### 旧RFC-0002 / 原文3587–3876行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### §14.36 — Explicit v7 native candidate registration (2026-10-11)

Native object 40, `RegisterFlatModelCandidateV7`, now registers a **non-reward
conformance candidate** for the complete mixed-court contract. The v4 decoder
or v5 encoder descriptor identifies the artifact/job role; the independently
derived v7 court-set, profile, maximum positions, encoder binding, resource
scope and replay envelope identify its verification semantics. The consumer
requires a Matured/Final model binding with exactly the submitted descriptor,
canonical program root and complete registered parameter-instance root. Neither
an official compiler nor a model-family name substitutes for that binding.

The kernel independently validates canonical normal form, exact range proof,
every real parameter instance and the complete scope/replay declaration. It
checks guaranteed court work and component reference-memory policy. Table 29,
`flat-conformance-v7`, stores the binding and complete derived plan separately
from execution classes. Its conditional state-root extension preserves the
historical root when empty; row restoration rederives the stored contract.
Old and new candidates share the same eight-entry bonded-owner catalog and
withdrawal cleanup. Registration creates no execution class, OPV eligibility,
job, reward weight or execution Final right.

The bounded canonical object decoder checks the program and parameter counts
before allocation. Native binding hashing is charged to ordinary block budget
and the actor's original collateral before checking authenticity, including
failed attempts. Canonical preparation has its own conservative metadata debit
before decode/range/unrolling. It cannot consume proof-reserved work. The SDK
prepares the exact plan and signs object 40 under its actual network/lifecycle
signature domain; historical v6 preparation and signing do not silently opt in.

Five kernel candidate tests, three new actual-node tests and two new SDK tests
cover identity/codec, complete declarations, rows/root/rollback/withdrawal,
shared old/new quotas, paid refusals and protected proof-budget headroom. The
actual-node suite includes all 18 prior conformance regressions and replays its
resulting state on an independent node. A synthetic 10,485,760-score candidate
at 2,097,152 maximum positions is accepted from bounded metadata. Its source
weights, tokens and maximum-length trace are not materialized; this is not an
executed long-context checkpoint.

The actual pinned Qwen2.5-1.5B checkpoint's 14,011-byte program and 1,636
parameter instances still fail this candidate contract: the present admission
tariff is 7,557,621,401 against the 536,870,912 ordinary allowance, and complete
scope derivation independently refuses an ordinary relation exceeding terminal
court caps. These are recorded refusal results, not model admission passes.
Pricing and bounded ordinary-relation court refinement must be completed
without raising the caps merely to admit this model.

This registration increment does not yet connect v7 posts, actor-owned
seal/reveal, chunk transport or selected-value DA. Those consumers, complete
shared resources/economics, real tasks and independent source fidelity, fresh
outsiders and honest execution Final/redemption remain required. Keep the
original A6 denominator and confidence/participation conditions and A8 complete
task cohort. The user's current goal additionally requires A6/A8 at least 90%;
no inaccessible checkpoint or failure is removed to manufacture that rate.
Conditional external mode A and dormant shipping fences remain unchanged.

##### §14.37 — Frozen bounded job lengths and history-selection component (2026-10-11)

`FlatLengthTemplateV7` adds a bounded declarative specialization contract for
token, patch and audio counts. It binds a canonical maximum-shape v1 program
and a strictly ordered map of fixed parameter/state/carry/node axes. Both the
maximum graph and every requested actual specialization independently pass
normal form, exact ranges and complete scope/replay derivation. The immutable
admission binds the template, actual counts, specialized program, positions and
scope/replay roots; enlarged/rebound declarations fail. Zero means an absent
modality. Existing H, primitive attributes, constant bytes and state roles are
not rewritten. Text decoder counts include the prompt and all fed-back tokens;
the final delivered token is excluded. Encoder counts bind its prompt at one
position. Patch/audio inputs still require their separate typed stage/root
binding and are refused on the text-only route.

Adversarial length execution exposed a production trace bug: a provider's
larger or differently typed parameter could reach evaluation despite a smaller
declaration. Buffered, streaming and stage traces now check exact declared
shape, dtype, payload length, representable values and layer namespace before
evaluation. The frozen interpreter already required this contract. Correct
executions, roots and tie semantics remain unchanged. Five length tests and
three trace regressions exercise the actual shared runtime, not a substitute
executor.

`flat_history8` separately implements a **version 8 history-selection
relation**, without relaxing the frozen v1 prohibition on H-axis TopK. At
position p, its source has H=min(p+1,window) rows and its IDX output has
min(Kcap,H) selected rows. Selection is score-descending with lower local id
breaking ties. Explicit window-local-to-absolute coordinates prevent treating
rolled-window ids as global token positions. Work is O(N log K), selection
auxiliary memory is O(K), and exact source/output node coordinates and roots
bind bounded range/order/dominance counterexamples. Its independent court
authenticates at most two source leaves and three output leaves. The session
precharges reference scan/hash and every bounded selected-value callback;
failed acquisitions and retries consume their shared allowance.

Eight history-component tests cover all six dtypes, warmup H=1 with Kcap>H,
flat layout thresholds, window rollovers, i128 extrema, prepaid failures,
hostile codecs, coordinate substitutions and 966 exhaustively enumerated small
selection tuples. Metadata reaches 10,485,760 maximum positions without
allocating position arrays or history images. This does not execute a
ten-million-token checkpoint. It does not prove the source HistAppend/state
continuity, define a complete typed v8 model graph, register/execute that graph
in a node, bind media stages, or close the shared economic/resource vector.
Those obligations remain open alongside full RFC02 acceptance.

##### §14.38 — Independent actual-checkpoint source diagnostic (2026-10-11)

Qwen/Qwen2.5-1.5B-Instruct revision
`989aa7980e4cf806f80c7fef2b1adb7bc71aa306` now has an independent Hugging Face
Transformers 4.48.3 / Torch 2.5.1 float32 eager-CPU reference from its actual
BF16 safetensors. Source file bytes are pinned and checked before and after
execution. The reference uses the installed built-in model class, offline
loading and no remote code. The diagnostic protocol fixes exact artifact,
source manifest, tokens, arithmetic units, jobs and thresholds before the
integer comparison. Revision labels are informational to this tool; chain
authentication and checkpoint provenance remain separate obligations.

For four fed-token sequences of lengths 30/31/32/30, all 123 next-token-logit
positions meet the predeclared thresholds. Top1 agreement is 96.747967%, mean
KL 0.001367878, slope 0.998682249, correlation 0.999847941, maximum absolute
logit difference 1.506592512 and RMSE 0.070122785. This is empirical original
source fidelity, separately from bit equality between integer implementations.
It supersedes the need to treat the previous local HL comparison as an
independent HF reference; it does not extend its diagnostic scope.

Measured whole-process wall time/RSS are 26.67 seconds/6,366,314,496 bytes for
the source reference and 242.18 seconds/2,151,088,128 bytes for the integer
comparison. These separate processes include import/load/hash overhead and
may be affected by concurrent builds. They are observations, not a service
throughput guarantee or complete execution/evidence/hash/store/delivery cost
certificate. The diagnostic's context is 32; original context 32,768,
complete chat/code/translation tasks, routing/state fidelity and public native
verification/Final remain untested by this run.

The dated census frame still has D_all=3,117,871 and D_files_listing=2,247,414
(72.081687%). These are listing/shape observations, not registration/Final
coverage. On that historical frame, the listed subset alone cannot supply
90% of D_all. Keep inaccessible/gated/missing entries and nonresponses in the
denominator; obtain new admissible evidence rather than reducing it. There is
no measured complete-task active-kernel A8 cohort or hook-free independent
Final/redemption result. A6/A8 and the full RFC02 goal remain unmet.

##### §14.39 — Prepaid exact scheduled-metadata admission (2026-10-11)

The v7 candidate's overly conservative byte-times-maximum-schedule price is
replaced by two paid stages. Reserve B² + 64B + 8,832P + 16,384 before hashing
parameter commitments or decoding the canonical program. From that typed
decode count the actual scheduled node occurrences N without constructing
wiring or tensor values. Reserve 16N*1,024 before range/scope/replay work, and
reuse the decode. The conservative metadata pass count covers the actual
prepared writer/offset constructions, TopK source walks, occurrence framing
and reference replay. Declared instance counts are checked before building
the per-layer set.

The two debits remain one adjudication attempt. Combined per-attempt and shared
block work must fit the ordinary allowance; neither stage borrows the proof
reserve. Invalid canonical bytes retain the first debit. Invalid scopes,
insufficient remaining metadata budget and retries preserve work already spent.
This changes admission pricing only, not canonical PWU, reward weight,
registered identities, scope/replay roots or terminal court caps.

The actual Qwen metadata has 314 template nodes and 7,658 scheduled nodes.
Its preparation price becomes 211,670,361 + 125,468,672 = 337,139,033, within
the 536,870,912 ordinary allowance. Its frozen ordinary court scope still
refuses independently; this removes one metadata-price obstacle and does not
establish model registration, native execution or Final. The earlier 307-byte
synthetic candidate's complete price is now 139,113 + 49,152 = 188,265.
The historical tariff/refusal observations remain retained rather than edited
to look like successful current admission.

Five new staged-cost adversarial tests pass, alongside the five existing
candidate regressions. Exact inputs, paid-attempt accounting, compiled binary
identities and the retained Qwen scope refusal are recorded in
[the staged-pricing verdict](../design/palw/tir/evidence/rfc02-staged-price-v7-verdict.json).
These tests do not establish full-model execution, native Final or A6/A8.

##### §14.40 — Native protected mixed-graph prosecution, v7 (2026-10-11)

The registered v7 scope/replay now has explicit native post, own seal/reveal,
expiry and public node/hash/selected-TopK-value demand/answer/default carriers
(inner tags 41..47, following candidate tag 40). The complete stored candidate,
scope and replay are rederived for posting and prosecution. Ordinary v5 faults
and bounded point TopK7 faults share that exact namespace; v3/v6 carriers cannot
bypass the protected route. An optional text length admission fixes one current
program specialization and its exact token count, scope and replay. It does not
make one registered class dynamically shaped or accept patch/audio inputs.

Protected v7 seals share the v6 four-per-bond and 1,024-global limits. The
original vector catalog still shares eight statements per bond with v2/v3/v6.
Requester slots are shared in both admission orders with old DA routes. Public
answers authenticate fixed saved coordinates and type/shape/root without
decoding the stored program. There are at most 32 open demands per vector and
16 KiB per answer. Invalid filings retain actor processing costs; only an own
mature seal enters guaranteed proof work. Own tag113 target4 chunks bind the
same actor and vector and survive saturation of the generic chunk catalog.
The last chunk plus its court requires two guaranteed runs, without increasing
the ordinary or proof ceilings.

A fresh, non-genesis bond registered on an independently synced node acquires
public op211 pages and requested evidence, independently checks a synthetic
sparse/state graph, seals its own proof, convicts and redeems the actual UTXO
credit. Full IBD independently recreates that credit; pruning import checks all
retained deltas and complete tip/rows. The original poster reserve funds one
49% finder/default share and the remaining burn. The paged RPC conversion is
exercised locally; this is not a live TCP service measurement.

The recorded build set passes 27 native, 16 SDK, 41 core and 155 kernel tests
(239 distinct; one hardware benchmark ignored). Thirteen tests are new. Kernel
trace-parameter and staged-pricing changes are included. The later resource10,
checked-settlement and graph9 changes are not included; the native-to-core
source difference is a static allocation inventory/test pin only. Compiled
source preimages, immutable binary hashes, repaired fixture trials and scope
limits are preserved in
[the native verdict](../design/palw/tir/evidence/rfc02-native-mixed-v7-verdict.json).

An honest protected candidate has clean verification but still creates no
execution class, reward eligibility or ExecutionFinal right. This evidence uses
a synthetic 65-axis TopK/Gather/state program, not the actual checkpoint. Honest
execution Final, complete typed H/media jobs, shared resources/economics and
actual-model A6/A8 acceptance remain open. Mode A and dormant shipping fences
remain unchanged.

##### §14.41 — Independent free-running task diagnostic failure (2026-10-11)

An additional frozen context-32 protocol runs the actual pinned Qwen source
and integer artifact on three arithmetic/translation/code prompts, each with
eight requested output tokens. Each implementation feeds its own generated
tokens. All 75 fed positions and 24 generated outputs remain in the denominator;
EOS affects answer display/scoring only, not the fixed execution count.

The generated-token agreement is 22/24 (91.666667%), below the predeclared 95%
threshold. Exact generated jobs are 2/3, meeting only that separate threshold.
Both source and integer task-answer fractions are 1/3, below the predeclared
100% answer threshold. Arithmetic differs at two positions after EOS; translation
outputs `Hello.` instead of the exact frozen `Hello`; the eight-token code job
does not finish its fenced answer or emit EOS. These failures are retained;
output formatting, thresholds and job membership are not revised after scoring.

The source process completes in 15.62s at 6,367,248,384B maximum RSS; the integer
process completes its computation in 206.33s at 1,974,124,544B RSS and returns
failure for the unmet threshold. The independent tokenizer-based scorer also
returns failure. These are separate diagnostic observations, not service
throughput or task/cohort acceptance. Four bounded-protocol/scoring refusal
tests pass, but do not turn the measured result into a pass. Frozen protocol,
all source/integer/scorer outputs, process logs, tool/binary identities and
limitations are recorded in
[the failed generation verdict](../design/palw/tir/evidence/qwen25-independent-greedy-task-verdict.json).
The earlier 123-position teacher-forced logit diagnostic remains a distinct
passing result. Neither result establishes complete task benchmarks, maximum
context, native execution Final or A6/A8 coverage.

##### §14.42 — Complete ordinary point graph and resource components, v9/v10

An explicit v9 graph contract now uses authenticated scalar output and exact
dependency openings for all 24 ordinary frozen primitives. Registered MatMul
second operands use the existing PC-v3 column layout; internal nodes retain
flat-v5 or small legacy commitments. Committed TopK uses point7 and public/
initial TopK, malformed headers and decode use their separately checked v5
relations. The complete scope covers all scheduled occurrences, history
initialization, both sides of the 63/64-element grammar boundary and maximum
H, with conservative 127-element flat tails. No frozen v5/v7 scope or resource
ceiling is changed.

The mixed court independently rederives that full scope and prices both its
metadata preparation and the terminal. Its locator replays from acquired
registered parameters and authenticated job input, then requests bounded
typed output/header/hash evidence for the first mismatch; it needs no supplied
fault coordinate. Typed value replies support all six dtypes. Missing evidence
creates a demand, not a fabricated conviction. Retry/hash/read/TopK work is
prepaid by the local replay session. These are versioned components; native9
and execution-class/job11 integration are separate acceptance steps.

Resource10 independently binds that complete scope and derives 21 dimensions
for primitive operations, validation/copy/index traffic, hashes, provider calls,
reference payload/container allowance, evidence storage, delivery/reply/retry,
complete court/metadata work, raw parameter storage/tree scratch and owned
peak payload. Its streaming reference consumer checks borrowed raw byte width,
dtype and shape before i128 expansion, authenticates parameters, and atomically
reserves plan/block/bond/network allowances before callbacks. Failed callbacks
and retries retain the debit. This local component neither installs persistent
native quotas nor measures process RSS, transport retention or coin fees.

Checked reference settlement preflights every accumulated u64 total, funds
payouts/burns from prior original collateral debits and FinalReward from spent
escrow, and rolls back a rejected or unbalanced receipt batch atomically.
Existing reservations remain releasable after another lane slashes collateral;
only new allocation rejects a collateral overhang. Seven new conservation
tests and 58 existing route/OPV/verifier/pipeline tests pass, alongside eight
resource tests and seven point/four mixed-graph tests. Exact build scopes and
retained failures are recorded separately; the earlier native7 full-library
run does not include these later changes. Actual measured point/parameter
checkpoint evidence follows in its own observation. Complete native resource/
escrow enforcement, typed H/media jobs, honest Final and A6/A8 remain open.

---

<a id="source-v1-0002-L03906"></a>

### 旧RFC-0002 / 原文3906–4059行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### §14.44 — Bounded authenticated external stage inputs, component v12

The opt-in `flat_inputs12` component binds a frozen program-v2 external stage,
its exact maximum v1 view, the immutable real source-PC namespace and bounded
token/patch/audio axis map. Every present modality must bind an actual external
input axis. Job admission fixes actual lengths and positions; maximum and actual
graphs independently pass validation, interval analysis and complete scope9.
Source-weight and persistent-state axes cannot be resized by the input map.
Both maximum and actual input payloads are bounded at16 MiB raw/32 MiB expanded
i128, with at most16 inputs and16,384 real parameter instances.

Acquisition checks job/admission/root counts and real source PC before provider
calls. It atomically prepays a local input attempt and its complete raw/expanded
payload. Dtype, shape and exact bytes are checked before expansion, followed by
the declared interval and each exact job-input root. Failed expensive reads
retain their complete allowance. Authenticated input values and combined PC
are private, with immutable accessors; virtual input roots occupy the canonical
v1-view parameter namespace after the real weights.

Five synthetic tests pass. A three-position patch3/audio5 stage produces
16/32/48 under independent arithmetic, the v2 interpreter and exact v1 trace.
Patch/audio reductions, an explicit previous-position state read and its write
use the same full scope9/court9: honest proofs dismiss, wrong committed outputs
convict, and borrowed input/state openings refuse. Wrong job/source/scope/count
and insufficient allowances refuse before callbacks. Immutable binary, exact
dependency preimages and selected commands are retained in
[the component verdict](../design/palw/tir/evidence/rfc02-typed-inputs12-verdict.json).

This component serves job-constant external tensors across admitted positions.
Per-position media inputs, random/earlier-stage sources, post-write stages,
native transport/retention/codec, resource10/quota11 integration, job funding
and media Final remain open. Its local allowance bounds input callbacks and
raw/i128 payload only; metadata, PC/tree hashing, scratch and provider-internal
work require the shared native resource contract. Real source-PC continuity is
distinct from original-model fidelity. No real-media task or A6/A8 credit is
added; RFC02 completion remains open.

##### §14.45 — Generic public checkpoint localization and fresh context128

The complete ordinary locator now authenticates its full scope and requested
relation before any evidence callback. Public hash descent locates an unknown
divergent leaf, and authenticated headers cover all six dtypes, including
malformed metadata needed for prosecution. Two additional graph tests pass
(six graph tests total); this does not repeat the four earlier tests as new.

The actual pinned Qwen checkpoint trial independently matches all7,658 honest
node roots, then finds occurrence1/node27/element1500 after35 agreeing roots.
Nine public queries return5,853 bytes; the17,816-byte proof convicts through
the same complete scope9/court9. Missing headers/values require acquisition or
dismissal. The fixture retains an authentic and a false R/PC tile pair, separately
from full integer-artifact PC continuity. Its frozen fixture, executables,
source identities and diagnostic limitations are recorded in
[the generic checkpoint observation](../design/palw/tir/evidence/qwen25-real-generic9-observation.json).
The public service and actual native prosecution remain separate checks.

A newly lowered context128 program changes both history windows from32 to128
while retaining the exact parameter declarations, all1,636 registered parameter
instances, schedule and quantization recipes. All four positional table fills
are independently reconstructed. Borrowed mapped payloads authenticate against
the same PC; no second1.835-GB integer artifact is produced. The earlier32-position
calibration is reused explicitly.

The frozen128-position diagnostic evaluates two complete128-token sequences
against independently loaded original source weights. It passes the unchanged
predeclared logit gates: top1 agreement252/256 (98.4375%), mean KL0.00230502,
slope0.99773344 and correlation0.99953118. Integer auditing plus execution takes
574.41s at2,438,201,344B maximum RSS; source inference takes8.61s at6,544,982,016B.
These different diagnostic processes are not a controlled performance comparison.
[The context observation](../design/palw/tir/evidence/qwen25-borrowed-context128-observation.json)
retains failed scale-decoding and disk-space trials, exact protocol/tokens,
immutable executable hashes and source outputs. This establishes context128
logit fidelity, not original context32768, a completed free-running task,
native Final, media support or A6/A8. The prior frozen greedy-task failure
remains unchanged. RFC02 completion remains open.

##### §14.46 — Funded execution and persisted quota components, v11

The independently checked class binds a matured native source header, exact
candidate9 program/PC, maximum scope/replay/resource10, trusted quota policy,
fixed liability horizons, reward ceiling and original producer reserve. The
text-job record separately fixes requester/producer/nonce, prompt and complete
fed positions, actual scope/resource/quota roots and an original escrow debit.
Its semantic request identity cannot change with admission time or a funding
carrier; native also needs a permanent requester/nonce once-only index.

Private job state permits one protected claim and one terminal settlement.
Final pays only original escrow after all retained evidence, pending own proofs,
DA and latest-answer grace close. Fault settlement refunds that escrow and funds
finder/burn only from actually collected original producer collateral. Amount
overflow, wrong roots, duplicate settlement and anchored state rewind refuse
before mutation. A trusted DAA jump may close/refund an expired job without
reopening computation/source proof windows. Late trace-DA default requires its
immutable establishment time within liability; unavailable external R is not
a source-mismatch judgment. Twelve synthetic execution tests pass against the
final20-test quota snapshot, with both the initial disk refusal and immutable
passing executable retained in
[the execution verdict](../design/palw/tir/evidence/rfc02-execution11-verdict.json).
Original-class restoration independently rechecks all original candidate/source/
policy roots, fixes private admission permission to false and preserves access
to funded-job settlement after source revocation. It creates no fault from a
revocation flag. Fresh funded policies require every future guarantee kind.

The quota component persists cumulative consumed/reserved amounts for21 axes
at subject, block, producer and network scopes under a rooted trusted policy.
Baseline is once-only; producer actions alone spend their reserved allowance.
Public challenger attempts spend their own ordinary allowance. Failed expensive
work stays spent, and admission floods cannot consume matured own court/DA/chunk/
close guarantees. Expired terminal collection prepays a fixed current ordinary
close envelope and uses a private token bound to current state and the genuine
terminal book; it restores no old credits. Twenty synthetic quota tests pass,
superseding earlier10/11/12/14/17/19 counts rather than adding them, in
[the quota verdict](../design/palw/tir/evidence/rfc02-persisted-quota11-isolated-verdict.json).

Quota restoration requires an independently rooted native state anchor. A paid
terminal attempt leaves the subject open until the genuine terminal book and
atomic payout authorize its private close token. Class-maximum court/chunk and
native metadata-restore costs are reserved independently of the smaller actual
job scope; native must guard the priced stored-post cap before copying/decoding.

These components do not authenticate carriers or spend native coins themselves.
Atomic native policy/nonce/quota/escrow/UTXO application, protected challenger
entitlements and retained-source lifecycle remain required. Bounded terminal
quota-row collection now requires authenticated permanent spent-request/job
anchors and a paid row operation; same-interval consumption is preserved. This first funded-job grammar refuses patch/audio. No native
honest execution Final, actual complete-task or A6/A8 credit follows; the full
goal remains active.

##### §14.47 — Predeclared complete free-running task diagnostic, context128

A distinct protocol is fixed before source/integer outputs: arithmetic,
translation and a complete Python addition function, maximum32 generated tokens,
first predicted EOS included, and no EOS at the cap counted as incomplete.
Answer rules,95% same-position token agreement, at least2/3 exact jobs and100%
task success for both models are predeclared. Missing output tokens count as
unequal. This protocol does not change the prior fixed-eight-output failure.

Independently loaded original source and the fresh context128 integer program
both complete all three tasks correctly. Generated counts are3/2/12 including
EOS; all17 output token IDs and all three jobs match exactly. An independent
tokenizer/semantic scorer passes the frozen rules. Six bounded-policy/scoring
controls pass. Source inference takes12.28s at6,480,707,584B maximum RSS; integer
source-PC auditing and dev-profile execution take261.35s at2,009,333,760B.
The differing processes do not certify production throughput. Exact protocol,
immutable executable/source identities, outputs, scorer and earlier trials are
retained in
[the complete-task observation](../design/palw/tir/evidence/qwen25-borrowed-context128-complete-task-observation.json).

This is a three-task offline diagnostic with short24/20/36-token prompts. It is
neither original32768-context coverage nor a probability-sampled active cohort.
Its EOS-stop grammar is distinct from the first native fixed-count job contract;
native execution Final, public fresh-node acquisition, other models/modalities
and A6/A8>=90% remain required. Neither its success nor the earlier context32
failure is substituted for the retained population denominators.

---

<a id="source-v1-0002-L04093"></a>

### 旧RFC-0002 / 原文4093–4367行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### §14.49 — Rooted native execution headers and staged request funding, v11

The native-only class checker rechecks the canonical program, complete class
scope and replay against the original independently admitted candidate9. It
retains the original class resource10 root without materializing its quantity
vector. Public imports and SDK candidate checking still independently derive
that vector. Actual-job preparation independently derives resources once; its
fixed record binds class, prompt/input, requester, producer, canonical nonce,
reward and quota-plan root. Fund restores that exact original native preparation
and rechecks live source/policy eligibility at the current block hash and DAA,
original debit and the whole admitted liability interval. A plan for the same
position count cannot substitute another prompt or actor.

The immutable class record additionally commits the exact original native2
R/PC proof bound. For canonical program size B and N committed instances,
PC bytes are 4 + the sum of 67 bytes per unlayered instance or 69 per layered
instance; max proof bytes are PC bytes +128KiB. Source-court work is
B² +64B +8832N +64(max proof bytes +7) +65536; owned scratch is
8B +512N +8(max proof bytes +7) +65536. Computation and source bounds must both
be represented in the independently derived Court/Chunk quota plan. Native
must compare the proof bound with the authenticated original binding header
and prepay all actual restores and fixed-row processing under unchanged action
limits; these formulas do not prove any actual model fits those limits.

A separately rooted fixed class header can restore and settle an existing
funded book without reconstructing its program/scope/resource vector. It is
readonly and cannot grant admission. Source revocation cannot strand original
escrow. Historical settlement requires an authenticated, originally verified
court identity with exact R/PC, subject, proof and original inclusion time
within claim liability. A refuted flag or new late proof is insufficient. An
existing global R/PC theorem may stop another exact-scope claim's Final; only
its original paid finder can receive a bounty. Native checks operator identity,
and a finder who is that claim's producer receives no bounty: the whole actual
collected slash is burned and the requester is refunded.

[The component verdict](../design/palw/tir/evidence/rfc02-execution11-native-headers-verdict.json)
records 26 pure tests and an immutable replay, with stable owned-source
preimages. The initial17/23/25 trials and the compiler refusal are retained.
Concurrent dependencies were not uniformly frozen for this component run.
Authenticated original state and actual coin debit/payout remain native boundary
fixtures in these tests; funded native Final/redemption, the accepted native
model cohort and the A6/A8 90% requirement remain unproven.

##### §14.50 — Public-snapshot streaming outsider consumer, v9

[The local consumer verdict](../design/palw/tir/evidence/rfc02-mapped-R-ordinary9-local-consumer-verdict.json)
records 12 SDK controls for an ordinary-point outsider. It authenticates the
public candidate/binding/trace and complete original R windows before its own
model execution, uses bounded selected DA and unknown-node/leaf descent, and
predebits local reference, source, metadata, hash, delivery and retry budgets.
It does not receive a prepared fraud coordinate or proof. Source acquisition
remains external under mode A; this is not a consensus source-distribution duty.
Special TopK/history/Decode relations return an explicit separate-court demand.

The actual Qwen fresh-node run now authenticates all1,445,797 R leaves and
all1,636 PC instances before executing its own7,658 nodes. The finder receives
no prepared fault coordinates, proof or archived replies. It obtains43 public
DA demands through44 attempts, locates an unknown ordinary fault and constructs
its own17,816-byte proof. Native scope9 seal/maturity/reveal, the original100
collateral slash, its own49-unit actual UTXO and independent IBD replay pass.
The final immutable run takes155.22s, with2,669,395,968B maximum RSS. Selected
service reserves123 callbacks and962,560 wire bytes; it does not claim an
unmeasured discovery guarantee under malicious ordinary traffic exhaustion.

Its first run refused the explicit local IndexSteps budget before opening the
model; that refusal is retained. The next local budget was fixed in
[protocol v2](../design/palw/tir/evidence/rfc02-actual-outsider9-protocol-v2.json)
before the next finder output, without changing network or court limits.
Coordinated prebuild capture and actual Cargo/.d selection preserve1,442 local
compiled inputs for the immutable node and SDK binaries. The actual1, SDK12 and
existing35 native regressions all pass in
[the final outsider receipt](../design/palw/tir/evidence/rfc02-actual-outsider9-final-verdict.json).
This ordinary-point,
class-maximum32/actual-position1 result adds no honest funded execution Final,
requester escrow redemption, accepted model cohort or A6/A8 acceptance.

##### §14.51 — Protected quota rows and bounded execution11 carriers

[The quota receipt](../design/palw/tir/evidence/rfc02-protected-quota11-isolated-verdict.json)
supersedes20 pure controls with40, including protected paid-seal/fragment
reservation, fixed authenticated selected rows, operator nonce witnesses and
source-court costs across21 resource axes. It does not substitute a whole-map
restore for the priced four selected rows. Expensive failed attempts stay paid;
unused expired reservations cannot recreate same-interval credits.

[Five carrier controls](../design/palw/tir/evidence/rfc02-native11-bounded-carriers-verdict.json)
check bounded framing, exact actors/nonces, job-specific outer trace identity,
salted own proof identity and source/computation wrappers. The carrier module
is compiled, but execution11 Fund/Commit/court/DA/Final routes are not activated.
Its unexported fold draft is excluded from the compiled source receipt.

Proof-root-bound protected DA2 begins only after a proof exists. It does not
guarantee unknown-coordinate discovery after ordinary demand exhaustion.
Native execution11 must reserve and authenticate a separate bounded paid
discovery entitlement before proof construction, then bind its conversion to
the same owner's exact proof and new maturity. Producer delivery and fixed
readonly terminal costs must be independently reserved before admission.
These remaining guarantees and actual Fund→Final→redemption remain required.

The protected distinct component pool is413: prior361 plus14 execution,
20 quota,12 SDK, five carrier and one actual fresh-node test functions.
Repeated35 regressions, local intermediate8/33/37 snapshots and immutable
replays add zero. This engineering test count is not a model denominator or
completion percentage; accepted native honest-Final model records remain0.

#### 改定 — 2026-10-12（seatingとモデル保有、単一seat scope：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 詳細は[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md) と[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4。

Proposal Aの `class_seated`（executor以外のモデル保有operatorを `seat_count` 以上、registrant・executorを除いた独立operatorの下限）は、モデル配布監査ではなく、実際にclaimを検証できるPanelを形成する条件として維持する（ADR-0177 D1補足：Miner・Panelは保有必須、登録者に配布義務はない）。Panel=1暫定modeでは、単一seatで全検証scopeを覆える経路だけを対象とし、旧複数seat前提を一括定数置換しない（RFC-0010 2026-10-12改定）。

##### §14.52 — Paid pre-proof discovery and one-slot proof conversion, v13 (2026-10-13)

`flat_quota11::discovery13` defines an explicit v13 policy and bounded fixed-row
transition API. A discovery ticket needs an authenticated original funded claim,
independent owner/operator, original deposit receipt and permanent native nonce
absence. It needs neither a prebuilt proof nor a known fault coordinate. Its
policy commits the original quota policy and a bounded request allowance.
Producer admission must cover ordinary lifetime answers plus discovery answers
for all four retained job slots and the eight post-proof protected demands.
Underfunding or arithmetic overflow refuses; declaring a ticket does not reserve
producer deliveries retrospectively.

The ticket uses the original v11 global, actor and job retained-slot counters
and all 21 cumulative resource axes. Admission reserves request envelopes,
fixed journal operations and the existing independently derived seal/court/chunk
plan together. Requests debit before decoding or calling a provider; failed
expensive attempts stay paid. They consume reserved capacity even when ordinary
capacity is exhausted. Immutable selected-row roots, exact per-attempt costs,
ownership, maturity, original interval and deadline are independently checked.
All returned rows must be applied in a single native journal transaction.

The frozen v11 request tariff used here covers fixed request metadata, not a
full class/post restore. It also does not cover a complete original PC leaf of
4096 i128 values. An integrated consumer must price those larger operations in
separate, explicitly consented supplements before restoring or decoding them;
the v13 ticket alone grants neither operation.

Conversion authenticates a new exact own proof seal on the same claim, vector,
owner/operator and original deposit receipt. It takes a new proof maturity and
transfers one retained slot and only the prepaid proof guarantee to the existing
v11 protected row. Unused discovery capacity becomes consumption, never an
ordinary refund. Same-interval retirement likewise consumes unused guarantees;
after an authenticated native rollover, collection pays a new ordinary fixed
journal operation and restores no expired credits. Native must preserve a
permanent spent-ticket/nonce anchor through row collection.

This is a bounded discovery reservation component, not a completed native route.
The v13 policy must be bound and checked by the eventual versioned native
Fund/claim/demand/answer/close path. A configured request count is not a theorem
that every unknown relation can be localized within that count or before its
deadline. Original-model acquisition remains external (mode A). Native atomic
coin/nonce/row application, operator-wide capital and rotation accounting,
complete localization bounds, aggregate adversarial liveness, funded execution
Final/redemption and all §II.14/A6/A8 acceptance remain open. No shipping fence,
frozen TIR byte, v7/v9 court, v11 policy root or existing seal grammar changes.

The local-LLM census now reports its shape threshold separately from A8
acceptance. A header-only report with zero measured active-kernel full-task
Finals always has `meets_target=false` and `acceptance_status=NOT_RUN`, even
when its shape-ready lower confidence bound exceeds 90%. Historical shape
measurements are retained; they are not Final evidence.


##### 14.53 Prospective native funded and typed-stage namespaces

Inner 63 (`FundedExecutionV11`) and inner 64 (`StageExecutionV14`) are separately
fenced native integrations of the complete-scope checker, original requester
escrow, principal nonce, derived shared quota and typed source/input contracts.
Their policy identities and prospective state extensions do not change frozen
v1/v7/v9 meanings. Funded proof chunk target 6 belongs to inner 63's owning
fence and original protected seal, including failed fragment/assembly charges;
it does not acquire ordinary chunk-pool priority. Typed stage14 source inventory
covers real original weights/state and never synthetic job-input slots.

Collection of an expired protected seal by a third party requires authenticated
rows for both current payer and original seal owner, plus the complete original
subject/global counters and terminal anchor. Price the full fixed restore and
journal to the payer's current ordinary allowance, decrement exactly the original
owner/subject/global retained counters, and delete once. Collection must never
refund old guarantees or recreate a proof attempt. Empty selected maps do not
prove absence of other retained seals or discovery tickets.

These are prospective implementation contracts. The native monetary, discovery,
court/DA and independent Final gates, real full-context/checkpoint conditions
and A6/A8 population criteria in II.14 remain required. Shipping presets keep the
new fields absent and arming validation remains closed. Current code and actual
measurement status are recorded in the [implementation ledger](../design/palw/tir/rfc0002-implementation.md).

##### 14.54 Original producer consent and additional source obligations

Consent18 precedes Fund and commits the exact original preparation, requester
and producer principals, operator nonce, reward, quota interval, expiry and
derived availability/fee policy roots. Old metadata acceptance does not grant
these rights. A requester cannot allocate another producer's answer capacity,
catalogue slots or recipient R merely by naming that producer. A consumed
consent and funded principal nonce remain spent across row collection.

Source16 supplements frozen v11 with an independently derived original-PC
answer obligation, allowing a complete original leaf within a bounded 128-KiB
carrier. It covers original headers, subtrees and original 4096-value leaves,
never a smaller reinterpreted leaf. Its checked quote commits the original
class, real parameter/source inventory, job quota plan and policy. A rooted
fixed cached quote is authenticated against those original native roots; a raw
serialized price vector cannot grant capacity. The source guarantee shares
all 21 global and producer resource aggregates while preserving the v11
subject's original attempt/quantity meanings. Answers debit before payload
decode, including failed attempts. Native pending-request authentication,
exact per-request attempts, original deadlines and original producer ownership
are additional obligations of the consumer.

Trace17 separately covers complete original class/post restoration by the
requesting actor and producer. Source16 and Trace17 are granted only under the
producer's original Consent18, and paid caller entitlement precedes expensive
restore. Neither supplement is a Final authority. Public expired collection
authenticates the original terminal and source row and pays a new current
caller tariff. It cannot move expired reservations into the current interval,
refund them or resurrect an obligation.

DA fee income requires explicit capped consent and actual collected caller
collateral. Original reward plus all credited fees and configured external or
default gains must remain within the original producer liability. Unpaid or
partly paid recipient R remains held across sparse DAA jumps; the reuse clock
starts at actual queue payment. Illustrative policy numbers do not establish
economic deterrence or guaranteed carriage. Localization turns, simultaneous
response capacity and whole-path deadlines require their own authenticated
policy bounds and measurements before independent Final can open.

Physical carriage must use actual consensus compute, transient and contextual
storage mass. The inner 128-KiB codec limit is not a delivery guarantee: that
payload plus signature/transaction overhead can exceed the actual 500000 mass
block. A carrier contract must bound the complete payload, inputs, outputs,
signature scripts, output scripts, sigops and contextual UTXO storage, and
authenticate the governing mass constants from Params. New delivery leases
must share block room with existing H1/readiness lanes, include paid failed
attempts and proof transport, and prevent fast slot reuse from exceeding the
promised service rate. A mass quotation alone does not establish that scheduler.

##### 14.55 Original physical leases and native carrier selection

Native text Consent23 and Fund24 explicitly bind the original Consent18,
Pursuit19 and physical service/lease22 roots. Frozen Fund2 and Consent18 are not
silently upgraded. A consent23 preparation cannot fall back to Fund2 to omit
its physical obligations. Global lease admission includes failed answer attempts,
original proof fragments and localization/transport deadlines. An ordinary stream
or owned proof stream retains its original slot until its original deadline;
answering early, copying a pending ID or converting a discovery owner to a proof
owner cannot manufacture an additional concurrent stream.

The native mempool/template path must authenticate the original pending unit,
producer/owner, remaining attempt and live lease against the template's exact
selected-parent state. A route tag, parser success, fee or caller-declared mass
does not grant priority. Recheck the complete populated transaction against its
original payload/input/output/script/sigop/contextual-storage envelope. Limit
UTXO variants of the same unit/part and take at most one such variant in the
reserved selection for a block. The original unit survives transaction fee,
signature encoding and UTXO changes; source/trace/proof and text/stage namespaces
remain distinct. Local index count and retained-byte ceilings also apply, and
every removal or original-unit retirement releases its exact local entry.

Selection uses the same virtual read snapshot as transaction validation. It may
not substitute the mutable store tip if the selected-parent state is unavailable.
Existing H1/readiness carriage and the ordinary fee market share the actual block
mass left by the native packet. A service boundary crossed by a DAA jump must not
be ignored merely because the new DAA is not an exact multiple of the period.
Boundary selection alone does not prove the promised duration: bounds on DAA
advancement, relay/admission, template refill, competing leases and actual
inclusion require separate measured evidence. A node-local reserved index is not
a censorship theorem or an independent Final authority.

The current consumer service remains a non-authorizing probe until the reserved
builder and its delivery assumptions are verified. Stage14 requires its own
original physical consent/lease before using this lane; a text23 lease cannot
authorize a stage response. All shipping activation fields remain absent.

---

<a id="source-v1-0002-L04421"></a>

### 旧RFC-0002 / 原文4421–4748行

出典: [0002-palw-tensor-ir.md](../rfc/0002-palw-tensor-ir.md)。配置理由: 主題別統合。

##### II.14.57 Independent original credit delivery — audit32/redemption33

Native Final evidence must open original consensus rows under an independently
pinned header, selected parent and owning ruleset. A producer's serialized audit
record is a projection to compare against rederived rows, never an authority to
mint a checked Final. Explicit auxiliary/payout proof labels authenticate their
exact original commitment positions. New envelope semantics require their own
opening verifier and cannot use an old-version or raw-root fallback.

New Final27 credits have a unique payment identity binding original job,
funding, settlement and payee. A pooled queue for another job cannot stand in
for a previously consumed original credit. Redemption33 checks the actual
coinbase at transaction index0 against the pinned block's transaction Merkle
root, including the owning Crescendo mass-commitment fence and exact missing
subtree grammar. Complete authenticated before/after payout tables prove the
same canonical drain prefix that the transition clears. Its contiguous outputs
must preserve original order, amounts and payee scripts, and the exact original
credit queue must disappear. The resulting evidence names a real outpoint and
the original credit; it does not infer maturity, current unspentness or a later
successful spend. Independent node validation and those UTXO lifecycle checks
remain part of the completion and A6/A8 evidence.

##### II.14.58 Text29 native lane, accepted-spend redemption and the A6/A8 receipt rule (2026-10-15)

> **Change marker — 2026-10-15 (RFC text amended; implementation is an unverified
> source checkpoint).** This section adds three normative rules and amends
> II.14.57's last paragraph: the "UTXO lifecycle checks" it left open are now
> specified by rule 2 below. No shipping fence, preset or release height changes.

1. **Text29 action grammar.** Inside the funded namespace (inner 63), actions
   27–40 belong to Text29 and are judged only past BOTH `palw_funded_execution_v11`
   and `palw_native_final_v27`; below either fence the bodies ride unjudged.
   Action 32 is the common paid paged part for namespaces 28 and 29: the part that
   completes a paid file's Part/Assembly/Court debits also reveals it. Action 34
   has no body and is refused by name. Action 40 carries an explicit collection
   kind: 0 retires one expired paid owner, 1 collects the next indexed public
   answer/default cache, 2 retires a genuinely terminal job. Kind 2 requires every
   paid owner, caller row, public cache and physical stream to be retired already;
   it then retires the source guarantee, the shared quota subject and the joint
   Queue30/physical22 leases once under a kernel-checked original terminal grant
   (`check_text_terminal_v29`), returns the finite owner-catalogue slots, and keeps
   the immutable original credit receipt, consumed-consent anchor and nonce counter.
   Text29 producer reserves and requester DA bonds count in the bond's unavailable
   collateral beside the v11 sums.
2. **Independent redemption is an accepted spend.** A producer Final credit of
   Text29 uses the SAME unique credit identity as Final27
   (`native_final_credit_queue_v27(job, funding, settlement, payee)`), so audit32 and
   delivery33 verify it without any Text29-private row DTO (`namespace = 29`).
   Redemption is complete only when spend35 shows the exact original credit
   outpoint consumed by a non-coinbase transaction whose id is in the complete
   accepted-id list of an independently pinned chain block: the verifier recomputes
   `AcceptedIdMerkleBranchHash64(selected-parent root || sorted mergeset ids)` and
   compares it with the pinned header. Consensus acceptance implies maturity, prior
   unspentness and a valid payee signature; the DAA clock is re-checked against the
   owning ruleset's coinbase maturity. No producer, Panel seat or serving-node flag is
   an input. Current unspentness of a not-yet-spent credit is not claimed.
3. **Measurement Final is not A6/A8 evidence.** An isolated calibration34 devnet
   (`NativeFinalPurposeV34::IsolatedCalibration`, distinct genesis, pinned snapshot)
   may produce real Final/delivery/spend receipts for measurement. Every audit report
   names its class (`final_receipt_class`: `measurement-final` or `qualified-final`).
   The A6/A8 aggregator counts the `final` stage only for a `qualified-final`
   audit32 report and the `redemption` stage only when that report also carries a
   verified delivery33 and spend35; a measurement Final stays in the denominator as
   the named failure `MEASUREMENT_FINAL_ONLY`. The node and every auditor load the
   calibration snapshot through one shared factory, and the auditor refuses a node
   whose announced params fingerprint, schedule id or genesis differs from its own
   reconstruction. A qualified Final requires a shipping network whose Final27 policy
   has the `Qualified` purpose; no preset has one, and creating one is a user decision.

##### II.14.59 Versioned length fields and lead defaults (2026-10-15)

> **Change marker — 2026-10-15 (RFC text amended).** Items 1–4 are *lead defaults
> pending user review* (reversible). Implementation is an unverified source checkpoint.
> No public preset, fence or release height changes.

**Versioned length fields (normative).** A job length is never reinterpreted: a change of
maximum, history growth rule or modality unit is a new contract version. Contract 36
(`LengthContractV36`) binds the program root, a history rule, the maximum positions and one
declaration per modality axis:

| Field | Meaning |
| --- | --- |
| `rule = FrozenV1 { history_bound }` | v1 semantics unchanged: `maximum_positions ≤` the program's frozen `history_bound` |
| `rule = Window8 { plan, window, selection_cap }` | the v8 window/selection relation (`flat_history8`); the plan root binds graph coordinates, row shape, window and cap; cost and evidence bounds are derived from that plan **at the requested maximum** (`HistorySelectionPlanV8::scope`) |
| `token { maximum, unit_bytes = 4, profile = 0 }` | exact u32-id axis, `maximum ≤ maximum_positions` |
| `patch` / `audio { maximum, unit_bytes, profile }` | RFC03 axes: committed bytes per patch / audio frame and the RFC03 encoding/preprocessing profile root |

A job commits its maximum lengths before Fund: every present axis is
`1..=maximum`, every absent axis is 0, and no claim or later slice can enlarge them.
For an unknown stopping point, Commit fixes the actual lengths and applies `check_actual`
against that original maximum; Fund never requires the future generated ids. A window
Text job's token axis counts evaluated positions (prompt plus generated ids fed back into
the decoder, excluding the final returned id that is not evaluated again). Attachment
checks the original request maximum, and Commit, Final and independent extraction check
the actual extent against the same Length36 contract. The
RFC03 patch/audio axes decode (so their bytes ride unjudged below their fence) but are refused
by name until their typed stage contract and fence ship (`MEDIA_LENGTHS_ARMABLE_V36 = false`).
A Text/Stage class that adopts contract 36 is itself a new class version; existing
Text29/Stage28 classes keep their v1/v28 meanings. Still open: the complete typed v8 graph
(HistAppend continuity, KV/state binding) and the RFC03 media stage contracts.

**Lead defaults (pending user review).**
1. *Stage28 credit identity.* Stage28 keeps its own credit identifier (fixed before its
   settlement exists). The audit record carries it as `payout_queue` with `namespace = 28`;
   audit extraction authenticates it against the original header and the immutable kind-14
   receipt; redemption33 skips only the v27 re-derivation for namespace 28. No consensus change.
2. *Master32 composite Final.* Composed at audit time from the components' individually
   genuine, settled Stage28 Finals in one pinned state's verified rows, the owning Stage28 policy
   and public tokens/outputs (each output authenticated against its component's trace
   openings). The composite is **not a consensus object**: no carrier, row or credit.
3. *Qualified Finals for validation.* A LOCAL purpose `LocalQualifiedValidation` judges Finals
   under the Qualified conditions but is bound to an isolated devnet genesis and loadable only
   through the v34 snapshot path (`palw-class native-calibration34 --purpose local-qualified`).
   It is never a public preset, t12/mainnet or release height. Reports mark
   `local_qualified_validation_network`.
4. *Below-fence rule.* While Final27 is inactive, funded actions 25–40 are neither rejected nor
   applied: validation passes them unjudged and the fold returns before any read or write, using
   the identical predicate (Final27 active ∧ OPV active).

**Real-checkpoint chain stages.** Register (class bound to a mature unrefuted source with its
derived maxima) and seat (job funded with the producer's original consent — the Panel-free
lane's seat) are re-derived from the same verified full aux opening as Final and redemption.

##### II.14.60 Typed window graph v37 and the window class (2026-10-15)

> **Change marker — 2026-10-15 (RFC text amended; unverified source checkpoint).**
> New dormant fence `palw_window_class_v37` (absent on every preset, Some-only hashed,
> `Some(never())` collapsed, refused when armed except `always()` on an isolated
> calibration34 devnet). No release height.

`flat_history8` proves only the selection over an authenticated history image and leaves
that image's continuity open. **`WindowGraphV37`** closes it for a whole program:

1. **KV/state binding (append).** For every declared history (a layer's K, V or recurrent
   state image) and every position `p`, the newest row of the image at `p`
   (`H_p = min(p+1, window)`) equals that position's committed writer-node output.
2. **State continuity.** Every older row of the image at `p` equals the image at `p-1`,
   shifted by exactly one row once `p ≥ window`. A reset, skipped, duplicated or reordered
   row is a two-point counterexample (`WindowFaultV37`: `Append` or `Continuity{slot}`) whose
   points authenticate against the claim's segment roots; its filing/court bounds derive from
   the graph (`scope`), never from a declaration.
3. **Selection binding.** The v8 selection plan's `graph_root` is the outer graph's root and
   its source must be one of these continuity-bound histories (same node, dtype, row shape).

**Window class v37** (funded action 45) registers a class from one registered Text29 class:
its `LengthContractV36` must use the `Window8` rule whose plan is the graph's selection plan,
with the same program root and maximum positions; native re-derives the record (program, real
PC, inventory, length/graph/scope roots) from the authenticated Text29 class and the program's
frozen header. Registration creates no job, reward, consensus weight or Final right; the window
job lane (fund/commit/court using `WindowFaultV37` and the v8 selection court) is the next
class-version step. **A-2:** below `palw_window_class_v37` (or below Final27) action 45 is
neither rejected nor applied — validation passes it unjudged and the fold returns before any read
or write, with the identical predicate. The route decoder's funded action ceiling becomes 45.

###### II.14.60a Window-class job path (2026-10-15, unverified source checkpoint)

A window job **is a Text29 job**: preparation, producer consent, Fund, Commit, close/Final,
collection, audit32 (namespace 29), delivery33 and spend35 are exactly Text29's. Funded action
46 (`u16 37 ‖ job ‖ window class`) lets the job's **producer** opt its Funded, uncommitted job
into a registered window class of the same Text29 class — producer consent is required because
the window courts add liability beyond the original Text29 consent. Attachment re-derives the
window class, binds the graph to the actual program (`check_window_graph_program_v37`: node
count, flat slots, history `[min(p+1,W), row]` and writer `row` shapes at the boundary
positions), requires the job's maximum positions within the class's and both window court
envelopes within the job's original paid filing bound. A window-bound job additionally accepts,
through the same paid paged29 owner/Part/Assembly/Court path and the same conviction and
settlement, a filing in its own envelope (`u16 37, kind, len, body`; the frozen v25 envelope
keeps kinds 0..=2): kind 3 = `WindowFaultV37` (append/KV binding or continuity), kind 4 = the
job's deterministic v8 `HistorySelectionFaultV8` (`window_selection_job_v37`). Actions 45/46
ride unjudged below `palw_window_class_v37` or Final27 (A-2); the route decoder ceiling is 46.

**Source-audit correction (2026-10-15; implementation unverified).** Coordinates and
history/writer shapes alone do not bind a selection court. Registration and attachment
must additionally bind the program root, actual HistAppend state and input writer, the
actual selection input, and its Idx dtype and position-specific output shape. All court
work/scratch/restoration must fit the original paid envelope as well as filing bytes.
The frozen v1/v2 grammar refuses TopK on symbolic `H` and cannot express the required
`min(H, selection_cap)` output. Accordingly the source guard is a defensive refusal,
**not a completed positive window execution path**. A separate versioned execution/source
contract is still needed; the v37 declarations above and standalone v8 relation do not
establish that implementation, native Final or RFC02 completion.

##### II.14.61 Genesis model set: eight text-generation models in one `.palwart` format (user decisions, 2026-10-15)

**This replaces the initial seven-model target below** (kept as history). The user named ten genesis models, dropped #7
(Gemma 4 26B-A4B, overlapping #8) and #9 (Qwen3-32B, overlapping #6), and chose the uncensored variant of each (huihui
abliterated; Dolphin where no huihui variant exists) — #8 included since 2026-10-16, rebuilt from huihui's abliterated
Qwen3.6-35B-A3B GGUF. Ids keep the original numbering. These
models are the next network's genesis (RFC-0019 edition 0, the 1.0 tier); testnet-12, its preset and its DAA 14,000 release scope
are unchanged. **Every genesis model is served at a 32K context (32,768 tokens; user decision 2026-10-16).** A PALWQ361 artifact
bakes its context into its header and rotary table, so #8 is rebuilt with `qwen36-convert --context 32768` from
`huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF` @ `acd5abe0` (Q8_0), streamed both ends (a measuring pass, then an output
pass into `palwart pack --from-pipe`, bit-identical to the one-pass file), into the new `Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW`;
the earlier 512-position `Misakachain/Qwen3.6-35B-A3B-PALW-runtime` stays as history. #1 is re-converted with the A16 converter at
`--n-ctx 32768` (the published 8k file stays as history).

**Scope: text generation only** (user policy, 2026-10-15). A genesis class maps token ids to next-token logits. Vision towers and
projectors (mmproj), image/audio/video embedders and processors, and multi-token-prediction (MTP) heads are not computed, not
covered, not admitted as claims and not rewarded; image paths will not be enabled. The artifact refuses such components by name
(`check_text_scope_v3`: job inputs, out-of-scope params, tensors and files, program kinds) and so does the genesis claim route
(`palw_genesis8_admits_input_v1`: anything but token ids).

| # | Source (pinned revision) | Architecture | License | Route |
| --- | --- | --- | --- | --- |
| 1 | `dphn/Dolphin3.0-Qwen2.5-1.5B` @ `60c996e2` (re-converted at `--n-ctx 32768`) | Qwen2 dense | Apache-2.0 | BASE-0 A16 |
| 2 | `huihui-ai/Huihui-Qwen3.5-2B-abliterated` @ `b2e291a6` | Qwen3.5 Gated-DeltaNet hybrid | Apache-2.0 | TIR |
| 3 | `huihui-ai/Huihui-Qwen3.5-9B-abliterated` @ `05b9e7c9` (tentative: no official Qwen3.8 9B) | Qwen3.5 GDN hybrid | Apache-2.0 | TIR |
| 4 | `huihui-ai/Huihui-gemma-4-12B-it-abliterated` @ `060ea173` | Gemma-4 Unified (dense, multimodal wrapper) | Apache-2.0 | TIR |
| 5 | `huihui-ai/Huihui-gpt-oss-20b-BF16-abliterated` @ `b73ec897` | gpt-oss MoE (sinks, sliding window) | Apache-2.0 | TIR |
| 6 | `huihui-ai/Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-GGUF` @ `a0b17c71`, bf16 GGUF | Qwen3.8 GDN hybrid (community SFT) | Apache-2.0 | TIR |
| 8 | `huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF` @ `acd5abe0`, Q8_0 (rebuilt at `--context 32768`) | Qwen3.6 GDN hybrid + MoE | Apache-2.0 | PALWQ361 integer runtime |
| 10 | `huihui-ai/Llama-3.3-70B-Instruct-abliterated` @ `fa133346` | Llama 3 dense | Llama 3.3 | TIR (not required to be end-to-end adjudicable) |

Every genesis model ships as **one versioned `PALWART3` artifact** (`misaka/palw/tir/artifact/src/palwart_v3.rs`): an index and
streamed segments holding a canonical manifest (source repository and revision pin, license, attribution), the program record, the
per-architecture weight section (A16, PALWQ361 integer, TIR inventory; MXFP4 exact integer reserved), the tokenizer, chat template,
source configuration, license texts and calibration inputs. The legacy `.palwart`/`.palwtir`/`.palwq36` formats stay readable under
their own magics and identities (kept as inner roots, never reinterpreted); the catalog's `artifact_root` is the `PALWART3` root.
Published artifacts go to `Misakachain/<SourceName>-PALW` (#10's name begins with "Llama", its card says "Built with Llama", and
the Llama 3.3 license, Acceptable Use Policy and notice ship with it).

Calibration measures each site's activation range on the float reference (absmax, per-position and per-channel maxima); the
integer program, and every adjudication over it, is exact and deterministic for any pinned calibration set, so the calibration
length is a fit choice, not a correctness one. A recurrent (Gated DeltaNet) class is calibrated on one sequence as long as the
context it serves (freeze-v1 §5.2, enforced by the converter): #2, #3 and #6 calibrate on one 32,768-token sequence. Dense classes
use four 512-token texts and declare 32,768. The texts are git blobs at one pinned commit; the token file and the measured
statistics travel in the artifact and the manifest records texts, tokenizer, lengths and policy. The float reference's threaded
kernels and its node-by-node runs of positions are bit-identical to the one-thread, position-by-position reference, so the
statistics depend neither on the core count nor on the run length. The memory design for the long context (one full-sequence
carry, no score matrix, opt-in carry spill, RAM accounting) is [RFC-0020](../rfc/0020-long-context-calibration-memory.md). A fit guard
(`palw-tir-fidelity --exec`: held-out short text, and the held-out next 32K tokens with a 32-window drift curve; mean KL <= 0.1,
top-1 >= 0.8, every window's KL <= 0.1) withholds the index of a class outside the tolerance where it can be measured on the
conversion host. The genesis list is
`consensus/core/src/palw_genesis8_catalog_v1.rs` (dormant: edition 0 is built only from supplied roots); the conversion pipeline is
`scripts/genesis/genesis10_pipeline.py`; per-model status, coverage (architecture × layer kind) and open items are in
`docs/design/palw/genesis8-status.md`.

Adjudication is unchanged in kind (II.14.62): sampled deterministic re-execution, public localization and the deterministic court;
genesis listing is economic only, and every claim still needs its own Gate C approval. Open at this writing: registration binding
of the `PALWART3` root and K2-TIR kernel activation.

###### History: the initial seven-model activation target (user decision, 2026-10-15; superseded the same day)

The initial rewarded set must include all seven models below: the six dense size
tiers remain, and the 35B-A3B hybrid MoE is an additional model, not a replacement
for 32B dense. All seven target block production, coinbase and Final weight from
the start of the new network. This is a completion requirement, not evidence of
present eligibility or an instruction to bypass admission.

| Tier | Selected model repository |
| --- | --- |
| 1.5B | `Misakachain/Qwen2.5-1.5B-PALW-A16-runtime` (original `Qwen/Qwen2.5-1.5B-Instruct`) |
| 8B | `meta-llama/Llama-3.1-8B-Instruct` |
| 14B | `Qwen/Qwen3-14B` |
| 24B | `mistralai/Mistral-Small-3.2-24B-Instruct-2506` |
| 32B | `Qwen/Qwen3-32B` |
| 70B | `meta-llama/Llama-3.3-70B-Instruct` |
| 35B-A3B | `Misakachain/Qwen3.6-35B-A3B-PALW-runtime` |

The user fixed the 35B-A3B target to the original **PALWQ361 integer runtime**:
`palw-runtime/qwen36-35b-a3b.palwq36`, 36,492,831,232 bytes, SHA-256
`5fe1d15caf0e544b718a4a2398db4f902b9a6722b266dd53824fbc9723578b29`.
The other `.palwq36` variant and the Claude-related GGUF files do not substitute
for this artifact. Its tokenizer, complete original context and canonical
conversion still require their own pinned identities and execution evidence.

The dated publisher revisions and Git/LFS file inventory are recorded in
`docs/design/palw/tir/launch/initial-seven-models-v1.json`. They are preparation
metadata: every selected runtime variant, canonical artifact/inventory, real PC,
tokenizer, context, random/decode rule and source conversion must still be pinned
before execution. Existing A16 and Qwen36 runtime formats do not automatically
become Source28/TIR artifacts or inherit a new class's rights. Missing pins remain
`NOT_RUN`, not a silently substituted checkpoint or a successful release gate.

The launch gate requires **7/7**, with independently reproduced full advertised
tasks, honest and corrupted-trace courts, native registration/funding, genuine
Final, coinbase delivery and accepted redemption under the target ruleset. Mistral's
vision encoder/projector are part of complete-model coverage; the MoE routing,
expert weights and recurrent state remain included in the 35B model. A nominal
4-bit weight size is neither verifier RAM nor an admissible cost quote. For 70B,
measure complete preparation, verification and evidence carriage within the
owning Panel=1 one-day DAA opportunity; no unmeasured smaller context may stand
for the full requested class.

The existing qualified-bond allocation (`alpha = 1.5`, 30-day average) and shared
Q/B/R/F caps apply. Seven entries do not imply equal one-seventh coinbase shares,
synthetic founding bonds or a model-specific multiplier. Keep the statistical
A6/A8 populations and their failure denominators unchanged: this launch set is
a separate all-required engineering gate. Activation is held until every model
passes, including the added MoE; no shipping preset is changed merely by listing
the target models here.

##### II.14.62 Verification assumptions (user decision, 2026-10-15)

[PRINCIPLES §8](../PRINCIPLES.md#8-llm計算の暗号学的証明を使用しない--2026-10-15ユーザー決定)
governs this RFC and every implementation and future plan. Use sampled replay,
algebraic spot checks, public localization, deterministic adjudication and
economic deterrence. Cryptographic proofs of model computation are permanently
excluded. Merkle openings, tensor commitments, hashes and signatures authenticate
data identity and origin; they do not establish correctness of the whole model
computation.

Consensus credit authentication requires the prescribed randomly selected
validator approvals under an explicit honest-fraction assumption. A matching
local replay, an expired challenge period, an optimistic component Final, a
complete Master aggregate or a private escrow payment does not replace those
approvals. The all-seven block/coinbase/Final-weight gate must inspect the actual
ordinary model/claim approval path and original bond caps. Report missing
approvals, unknown detection rates and absent execution evidence explicitly.

##### II.14.63 Unsupported models and consensus upgrades (user decision, 2026-10-15)

When a model cannot be supported under the current RFC02 consensus rules,
implement the required **hard fork or soft fork** to support it. Record the exact
model/task/context and missing feature. Existing-rule frontend or lowering
changes need no consensus change; a new primitive, grammar or validity extension
uses an explicitly versioned hard fork when old nodes would disagree. Use a
soft fork only when the new validity conditions restrict the old valid set and
old nodes accept the resulting blocks. Determine compatibility from the actual
rules; never reinterpret frozen old bytes or primitive semantics.

Both paths need explicit activation, node/migration implementation, original
source and full-context evidence, independent replay and public deterministic
court, DA/resource/deadline/economic checks, sampled-validator approvals, Final,
redemption and reorg verification. Unsupported models earn no provisional rights
by bypassing those checks. PRINCIPLES §8 remains mandatory in either upgrade.
The current implementation and validation backlog is tracked in
[the remaining-work document](../design/palw/tir/rfc0002-remaining-work-2026-10-15.md).

---

<a id="source-v1-0029-L00001"></a>

### 旧RFC-0029 / 原文1–224行

出典: [0029-owner-independent-shared-tir-query-data.md](../rfc/0029-owner-independent-shared-tir-query-data.md)。配置理由: 主題別統合。

### RFC-0029 — owner-independent shared TIR query data

- Status: **Draft / Proposed / Review Required** (2026-10-21).
- Companion: [ADR-0196](../adr/0196-owner-independent-shared-tir-query-data.md).
- Governing contracts: [RFC-0026](../rfc/0026-court-policy-master-contract.md), [ADR-0192 detailed contract](../design/palw/integration/rfc26-service-shared-frontier-contract-2026-10-20.md).
- Scope: isolated source-query/intent primitive and rooted data adapter candidate (§7). No production installer/lifecycle, complete paid admission, supply, signer-duty completion, default or activation is implemented.

#### 1. Closed query scope

The existing `TirSharedRequestSetV1` derives all legal source queries from the original frame/source/native rules/shared profile: every permitted interior level, leaf, overlapping run, row, tile and canonical ID chunk. Existing query IDs stay unchanged. A separate `shared-tir-query-universe/v1` identity binds claim/allocation/rules/profile/source and complete cardinality. Owner, requester, relay, queue order and first arrival are not inputs.

The in-memory frontier is keyed by canonical ordinal in that immutable universe. Repeated requests coalesce and different legal choices coexist. There is no owner reservation credential or requester-specific exclusive slot in this primitive. This is not a proof that a native node admits an outsider while all owner seats are occupied: that requires original funded admission, native ingress and node T05 below.

#### 2. Signed request intent

`SignedSharedRequestV1` field order is version `u16`, universe `Hash64`, ordinal `u64`, query ID `Hash64`, literal signer bond, signature bytes. Its message binds the native caller's network domain and every non-signature field under `shared-tir-request-intent/v1`; ML-DSA-87 uses the distinct `court-shared-request/v1/mldsa87` context. A requester's own bond key is resolved from state. A requester or relay does not become the owner of the query or its response data.

The signature Vec decoder validates the exact existing ML-DSA-87 length before allocation. The other fields are fixed-size. Unknown payload version, wrong universe/ordinal/query, wrong network/key/signature or absent bond changes no frontier state. A valid duplicate is coalesced, not converted into another shared obligation. This does not waive the real carrier's fee or refund its funding automatically.

The signature proves identity of this intent, not current original/profile authorization, funding, entitlement to slash or correctness of model computation. The candidate query set remains a caller-supplied native-source view; a future native coordinator MUST reconstruct it from the accepted original and adopted profile. No producer-supplied scope hash is an admission certificate.

#### 3. Authenticated public data

Only a `PalwTirPublicAuthenticatedUnitV1` produced by the existing original-root source authenticator may fill an ordinary query. The unit's source and exact request must match. No hash-only response, declared sender, caller timestamp, fabricated completion flag or decodable response DTO substitutes for that capability. A previously authenticated alternative opening is not a new fault or a new reward.

Canonical ID chunks are different: they are provisional until the complete ordered ID sequence authenticates against the original outer trace root. The frontier accepts the resulting `PalwTirPublicAuthenticatedIdsV1` capability and serves borrowed canonical slices; it does not independently mark a raw ID chunk authenticated. Rows/tile authentication still needs the existing authenticated rows/IDs anchor. No body is reused across a different original source scope.

These capability values are intentionally not deserializable. The in-memory frontier is not serialized into consensus state. The rooted candidate in §7 retains raw bounded data and reauthenticates it against the current native original context before constructing capabilities; a serialized “authenticated” bool is forbidden.

A public data opening is not a particular custodian's signed response. Sharing data does not discharge that signer's duty, release bonds, certify credit, decide a semantic court, or promise Panel revenue. The full protocol still needs fresh adaptive response signatures and existing qualified custody rules. Keys for funds are not delegated.

#### 4. Source-data upper components

`source_data_upper` covers every source query without allocating the query universe:

- Step/rows/tree nodes: for every level, multiply its width by the maximal first-node bound. The last ragged node has no larger frontier; all intermediate levels remain included.
- Leaves: all positions at the existing worst-leaf unit bound.
- Runs: every count from 1 through the existing legal run limit and every valid start, with that count's unit bound. Overlaps are retained.
- Tile nodes: all rows and levels; trace tiles and ID chunks separate full chunks from the last partial chunk.

All sums/products use checked arithmetic. Overflow returns failure without partially modifying the accumulator. The derived query count must equal the closed-set cardinality. Profile namespace changes cannot lower identical source body bounds.

Outputs are query count and sums of the existing maximum body bytes, parser steps, hash input bytes and resident-unit RAM. They are **partial derived uppers**, not measured runtime CPU or total physical service capacity. They omit request/response signatures, actual carrier/fragments, IO, frontier/indices/journal/retention, reorg copies, full-ID EOF/dependency work, preparation/work/restart/witness/semantic court costs and actual delivery. These omissions are open work, not zeroes. Even the resident-unit sum is not a funded storage/heap admission certificate for the new frontier.

Overlapping data may be shared only with authenticated transformations and their full costs. This conservative source-unit sum is sufficient for its stated source component, not a necessary lower bound proving all optimized designs impossible.

#### 5. Required native protocol

Before activation, the candidate must reconstruct the accepted original/source/rules/profile, preserve original `a + 3,120` notice and `a + 10,000` hard bounds, fix notice equality in the boundary contract, and bind all legal adaptive queries, dependencies and late branches at original acceptance. A request arriving after a claim's permitted opening cannot reset its clock. A timely initial claim notice and later adaptive steps need separate native state; the timeless prototype does not invent that state.

The native coordinator must atomically reserve the complete graph, collect actual fees/funding, establish current literal court signers and dependency horizons, and install holds. Failure leaves none of these partially installed. It must reserve actual carriage/CPU/IO/memory/retention under the existing caps, derive first-block supply, and retain no-default recovery when supply/assumptions fail. The preceding body upper alone cannot enable this path.

Responses exceeding one existing control/part envelope need bounded streaming with scoped Begin/Part/Finish, signed literal responder, deterministic EOF authentication and rollback. No new larger ceiling is assigned here. Data sharing and obligation completion require separate effects; outsider requests cannot create unrestricted unpriced liabilities for custodians.

The local rooted candidate namespace/codec in §7 needs explicit public schema review; no lifecycle tag is allocated here. It never reinterprets tag 230, old tag 245, X3 container, service v2 or RFC-0028 clock rows. Existing dormant fences and real-network empty arming records remain. Old claims keep old rules; the in-memory candidate is not wired into them.

#### 6. Verification and limits

Selected tests cover actual native source-unit bounds versus a complete tiny-source oracle, ragged tree maxima, checked sum overflow, actual ML-DSA-87 request signatures, duplicate/different choices, profile/network/key/coordinate substitution, original-root data authentication, provisional/sealed IDs, malformed/truncated signature lengths and no mutation on refusal. Tiny execution and injected bond registration are fixtures, not actual node registration, payment, network capacity, signer availability or model measurements.

Full T05 still requires an unfinished-evidence outsider under all owner coalition occupation/refilling, every legal choice and latest response, duplicate roots, real carrier/fee/UTXO admission, shared authenticated data through deterministic adjudication, same-build supply and T01–T15, restart/reorg/pruned IBD and t12 identity. No adopted numeric capacity/fee, automatic bond release, model exclusion or cap increase is introduced.

Checkpoint `b318ea946` selected result: **67 passed / 0 failed / 0 ignored** (query/frontier 11, fresh identity/gate regressions 26, same fresh core executable native replay 30), non-test consensus check and existing generator idempotence succeeded. [Raw logs, initial failures, hashes, exact scopes and source patches](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#owner-independent-shared-source-data-candidate). This does not complete the native protocol in §5 or node T05.


#### 7. Rooted native data candidate (after b318ea946)

Local appended key/row tags are: `CourtSharedContext(claim)` 14/20, `CourtSharedQuery(request_id)` 15/21, and `CourtSharedIds(claim)` 16/22. Payload versions are 1. All old enum ordinals/codecs remain unchanged. These tags are candidate schema additions, not public adoption or lifecycle allocation. No production original coordinator installs the context, and no object dispatcher invokes the new data functions. Context presence is NOT a full reservation, timely notice or court-opening certificate.

Context fixes original claim/allocation/source/native rules, a separate shared profile derived from the bound service-v2 profile, canonical universe identity/count, original acceptance/hard/material clocks. It must match the CURRENT rooted original, registered class/layout and unresolved service binding. An intent is resolved against this actual native view, not a caller-supplied query set. The requester's existing Active/above-floor bond and real ML-DSA intent are checked; requester identity is not query ownership. Duplicates coalesce without another write, and distinct queries are separate rows. The opening stores actual accepting block/DAA and historical intent identity. A requester later retiring does not invalidate that accepted fact or privatize public data.

These are data operations inside `[original_acceptance, original_hard)`; they do not amend any existing owner's response deadline or the `+3,120` notice rule, define notice equality, or open a new court after its notice period. Initial notice and original complete all-branch admission remain unimplemented. No fee exemption follows from the private/native primitive: actual carrier funding/fee admission still needs integration.

Query rows retain bounded raw response bytes, digest and actual accepting context. Whole-ID rows retain the raw sequence/rows root and context. Source hash, request coordinate and full-ID outer EOF are reauthenticated on writes, public reads and carriage consistency. A matching storage digest or expected carriage root does not bypass original-root authentication. ID queries derive available data from the full sealed sequence, never from a per-chunk authenticated flag. Valid data availability has no signer-duty, credit, bond, refund or default effect. Fresh signed adaptive responses remain separate required work.

The internal functions use native checkpoints and the existing `StagedCourtSeed` journal/map/index writer. Query opening metadata and completed data/IDs/context are immutable; book/profile removal or substitution cannot strand them. Original material TTL stays unchanged, while an unresolved binding retains needed rows past it. No signed foreign promise is extended. Journal undo/replay must restore the same rows/indices. Raw clone/ID-byte hash workspace uses concrete typed allocation requests before allocation; new ID/unit runtime capabilities keep their leases attached. Rooted wire preflight/heap visitors are regenerated with the schema. Extra source reconstruction, per-row reauthentication, hashes, indices, copies, IO and retention are still costs to price/calibrate. This adapter alone does not establish a complete memory/CPU/physical-service profile.

The selected native fixture fills all eight existing owner slots through the existing verified-half Open path, confirms the outsider cannot Open another owner, then exercises the independent rooted shared request/data path with real requester signatures. It checks coalescing, other choices, invalid body/EOF refusal, read/reload, tampered raw data, journal undo/replay and retention. Original registration/signatures/payment/context installation are fixtures; there is no real lifecycle carrier, full original reservation, adversarial refilling/latest-answer campaign or node T05 acceptance yet.

The selected rooted-data checkpoint verifies **69 passed / 0 failed / 0 ignored**, non-test consensus check and regenerated inventory consistency. [Exact native/source fixture scope, raw logs, hashes and duplicate-count handling](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-shared-data-native-adapter-candidate). The first honest case was strengthened to a committed arithmetic lie and a valid-codec/digest but wrong-source body; these are data identity tests, not model correctness certification. Native lifecycle/funding/original all-branch admission/supply and node T05 remain incomplete.

#### 8. Signed bounded stream envelope candidate

The separate `SignedSharedStreamV1` envelope (payload version 1, no lifecycle tag) binds network, claim, native shared scope, target, literal bond signer and every Begin/Part/Finish field with ML-DSA-87 under `misaka-palw/court-shared-stream/v1/mldsa87`. Its target is either the exact unit ordinal/request ID or one `FullIds` sequence for the scope. Full IDs are not uploaded once per canonical ID chunk. A unit target must still be resolved against the actual native set and must reject ID queries in favor of the single full-sequence target; a valid bond signature alone does not authorize a Ready custodian or authenticate source data.

The slot hash binds version/claim/scope/target/signer and excludes declared body hash, length, action, nonce and accepting clock. A different Begin cannot allocate another slot for the same actor/target. The declared stream hash additionally binds length and body hash. The future native writer must derive the applicable body bound from the current original source (the existing full-sequence bound for FullIds) and reject declarations above it; a signed length is not an authoritative resource quote. It must make Begin immutable (exact retry is idempotent without redating), preserve other eligible signers' independent slots, and reject replacement/restart rather than silently treating it as a newly funded branch. The complete legal retry/restart graph and funding remain open; this envelope does not adopt a new restart policy.

Part bytes retain the existing native 16,384-byte ceiling. This is a body-part bound, not a total carrier envelope bound: the new signature and all fixed fields, transaction framing/signature, fees and fragment costs must be accounted separately. The recovered quiet-devnet observation with a 17,408-byte payload does not validate this different signed envelope or its load capacity. The decoder checks that ceiling and exact existing signature length before allocating their vectors. Checked geometry requires exact current offset and `min(remaining, 16,384)` bytes, including the final partial part. Empty, oversized, reordered and post-EOF parts fail geometry. This pure geometry function cannot certify that a prefix was accepted. Signatures authenticate the original full envelope, including part bytes, offset and stream identity; a third-party relay cannot rewrite the signer or action.

This checkpoint implements the envelope, signature identity and geometry only. There is no rooted stream/chunk row, dispatcher, Ready eligibility resolver, carrier fee path, retained cursor, EOF assembly/source authentication, complete graph reservation or service supply for it yet. The internal whole-body adapter in §7 remains separate. In particular a valid envelope is neither an authenticated public body nor evidence of signer-duty completion. Future rooted prefix storage must avoid repeatedly cloning the whole growing body, reserve concrete typed memory before allocation, use actual accepting block/DAA, authenticate the completed body/full IDs against the original source, and atomically roll back failed EOF without discarding its prior prefix. Reload/reorg/pruning and all added signature/hash/IO/copy/retention costs still require integration and tests. No cap, deadline, old wire meaning or real-network fence changes here.

The envelope checkpoint selects **73 passed / 0 failed / 0 ignored** (15 final query/frontier, 27 codec/identity, 31 final same-executable native replay), plus successful non-test consensus check and unchanged generator output. [Exact scopes, raw logs, hashes and excluded intermediate repeats](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#signed-bounded-shared-stream-envelope-candidate). These tests do not implement the future native writer described above.

#### 9. Rooted signed-prefix candidate (after 33738ea26)

The private adapter now adds candidate key/row `CourtSharedStream(slot)` 17/23 and `CourtSharedChunk(slot, offset)` 18/24; payload versions are 1. Earlier tags, payload meanings, profile namespaces and existing lifecycle dispatch remain unchanged. No production coordinator installs this context or dispatches these controls. This extends the isolated Draft, not public schema adoption or complete original admission.

At ingress the adapter resolves the CURRENT original/context/source/set and actual original shared preparation. It requires that preparation's literal Ready signer (Ready accepted no later than the operation), the current Active/above-existing-floor bond, and the actual bond-key signature/network domain. A unit resolves its exact ordinal/request ID, requires an already accepted query, and excludes ID queries; FullIds is one scope-wide target using the existing complete-sequence bound. FullIds permits original public-data prefetch, as the prior internal whole-ID helper did; it creates no requester-imposed custody duty or free production ingress. Begin must follow query opening when applicable and stay within the source-derived body bound. Original hard/material clocks remain authoritative. No new notice, duty deadline, clock extension or fee exemption is created.

A slot belongs to one actual Ready signer/target, independently of owner slots and declared body hash. First accepted Begin is immutable; exact retry adds no write or later clock. Other eligible signers have independent slots. This bounds actor identities by the original preparation, but does not prove their availability: fresh adaptive signature availability and original complete funding/service assumptions remain open. It does not yet provide the full outsider-to-conviction T05 route. No default or responsibility rule is inferred from this data protocol, including a signer submitting an invalid body or stopping midstream.

Stream metadata retains fixed declaration/opening, accepted cursor/head, actual last block/DAA, optional actual EOF context and original material horizon. A Part stores only its bounded bytes, previous head, fixed offset/stream, accepting context and signed-intent digest. The canonical chunk key prevents multiple rows at an offset. A new Part links exactly one chunk; the writer checks that link and immutable opening without cloning or assembling the growing prefix. Chunk rows use the existing shared court ownership wrapper, so full-state checkpoints share retained chunk payloads; a mutation detaches only the affected bounded chunk. Global state/map/index checkpoint work and copies of other existing rows still remain costs, not an O(1) whole-transition claim. Duplicate identical Part is a no-op; changing its bytes, offset/stream, declaration or redating the retained prefix fails. Metadata and chunks use the existing typed map/index/journal writer and the unresolved-service material hold. A historical accepted Ready/intent fact is not invalidated by the current key exiting; current bond activity is still required on every new ingress.

EOF assembles the retained canonical prefix under an exact typed byte-vector lease, checks head/clock/length and the existing native body checksum, then authenticates original source data. The complete IDs body uses the existing concatenation of canonical ID responses, with exact total count and one rows root. Provisional IDs are held under a concrete typed lease; chunk decode workspace and the final capability remain separately leased. Only original outer-root EOF authentication can publish IDs. Unit data uses the existing source decoder/openings and sealed IDs where required. No successful data authentication certifies arithmetic or completes a signer duty.

Publication and EOF metadata use one outer native checkpoint. Failure preserves the prior prefix/root/journal, and a failed EOF does not authorize replacing that signer's Begin. Legal retry/restart/fault outcomes still need the complete funded interaction graph; this candidate does not silently add an unreserved restart. A different valid uploader is independent. A successful EOF publishes through §7's existing internal data writer and retains the raw chunks, never a trusted serialized capability flag. Native carriage rechecks the prefix chain, source scope/Ready history, accepted clocks and completed original authentication; orphan/missing/overrun or tampered chunks fail. Undo/replay and original material indices/holds include both new rows. EOF does not release bond/material holds or supply a new termination reason.

The small-chunk codec fixture exposed that the existing shared/standard Arc wire scanner omitted the conservative alignment allowance already used by typed heap ownership. Both wire scanner wrappers now include that unchanged typed allowance using checked arithmetic; hard caps, typed heap accounting and Borsh bytes are unchanged. Scalar and zero-byte payload tests prevent unrelated Vec slack from hiding the discrepancy. This is a preallocation accounting correction, not an adopted capacity increase.

Hashing envelope and chunk Borsh bytes now streams directly into the hash state instead of making a temporary copy of each Part. Message identity is compared against the previous byte-serialization computation in the signature test. EOF assembly, source reconstruction, repeated data reauthentication, signatures, IO, indices, journal copies, retention, codec/transaction framing and failed branches still have costs. They have not been calibrated or included in a complete paid service profile. Existing Part/body/source bounds are not total carrier capacity or fees. No cap, dependency promise, production number or real-network fence is increased/armed.

This checkpoint's final selected verification is **86 passed / 0 failed / 0 ignored** (wire 11, codec/identity 28, same final executable native 32 and query/frontier 15), with non-test consensus and generator checks succeeding. [Raw failures, corrective source snapshots, final hashes and exact fixture boundaries](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#rooted-signed-shared-stream-native-adapter). These do not establish a production paid ingress, complete original graph/fresh duty, actual supply, full T05/T01–T15 or real-model door feasibility.

#### 10. Explicit fresh data-response candidate (after fa1104eed)

Data uploaded by someone else, a completed signed stream, an authentic source opening and a matching local replay do not by themselves record the designated signer's fresh data reply. A separate `SignedSharedDataResponseV1` (version 1; no lifecycle tag) binds network, original claim/shared scope, exact request ID, literal signer, the first accepted query block/DAA and the authenticated current data fingerprint. Its ML-DSA-87 context is distinct from request/stream/native controls; signature length is checked before allocation. No zero-fee permit or signed computational-validity assertion is introduced.

The native adapter resolves the actual query/set/context, original preparation/Ready signer and current Active/above-existing-floor bond; it reauthenticates the public source data before verifying and recording the reply. Unit fingerprints hash the canonical authenticated source response with scope/request/source identity. ID fingerprints bind the canonical requested chunk and rows root only after the entire original ID sequence is sealed. An upload's signer and the signer recording this data reply need not be identical. This shares data, not signing keys or another party's duty. A response without matching authenticated data, from a non-Ready actor, against a different opening/fork/network/scope/fingerprint or with a forged signature is refused without installing a receipt.

The accepted query's block hash must already be known: the new response is not accepted in that same opening block. This avoids a signed reference to a block containing its own response signature. A later block may share the query's DAA; no new mandatory one-DAA latency is imposed on other native controls or on this response. Reaccepting the same shared query/data on another branch changes the opening block/DAA binding and requires another signature. An existing canonical query cannot be redated by a duplicate/coalescing requester.

Candidate key/row `CourtSharedResponse(request_id, signer)` 19/25 is appended; old ordinals/meanings remain. The receipt fixes query/data/signing identity, actual accepting block/DAA, intent digest and original material horizon. One actor cannot overwrite or redate it; an exact retry adds no write. Native map/index/journal/checkpoint, unresolved-service retention and carriage context validation cover this row. Loading a receipt rechecks the actual original/query/Ready fact and authenticated data fingerprint/availability time. Current bond activity is an ingress check, not a reason to invalidate an already accepted historical response later.

`SharedDataResponseStageV1` is a current rooted data-stage view, **not a funded assignment or complete semantic court duty**. It reports actual query/Ready facts and optional authenticated data availability/receipt. While data is absent its resource eligibility is unknown (`None`); once available, the resource first-block convention is `max(query_daa, ready_daa, data_available_daa) + 1`, with checked arithmetic. This is the existing accounting convention for a later first accepting DAA block, not a per-turn deadline, guarantee of supply/inclusion, mandatory response delay or permission to fault. Late/missing data or unknown eligibility does not exonerate a different earlier upload/preparation breach; that still requires the complete causal duty/reservation graph. The original hard boundary remains an outer bound, and operations at/after it fail. The complete original coordinator still has to declare/fund the actual obligation and each legal branch before any production assignment or default rule can rely on it.

A data receipt does not complete preparation work, semantic dissection/root/round/choice, arithmetic adjudication, final witness work or claim authentication. It creates no credit, refund, bond/material release or default authority. Service default guards remain denial-only even after a data response exists. Full signed duties and their response deadlines, delivery/inclusion/honest-signature assumptions, legal restart/failure branches, complete original reservation/fees, actual supply and node T05/T01–T15 remain open. Data response signatures, reauthentication, codec/transaction overhead, IO, indices/journal/checkpoints and retention are extra work to price; this checkpoint does not assign them zero cost or adopt a numerical profile. Existing fences/presets, caps, dependency promises and notice/hard limits are unchanged.

Final selected validation for this data-response checkpoint: **89 passed / 0 failed / 0 ignored**, non-test consensus check and generator consistency succeed. [Exact selectors, real-signature/codec/native fixture boundaries, hashes and duplicate-count exclusions](../evidence/palw/g14-integration-candidate-2026-10-20/README.md#explicit-fresh-data-response-native-candidate). Native tests are in-memory verified-half original/Ready/payment fixtures, not complete paid public ingress, actual supply, fresh semantic signer availability or node T05 acceptance.
#### 11. Derived signed data transport components

Parent `49892b675`. The closed query geometry now derives signed-envelope/control components for every distinct state-advancing DATA operation. It counts one shared query opening, every possible literal custodian's bounded Begin/Part/Finish stream, and each custodian's fresh response for each query. The custodian upper comes from the original frame's pledge, not a caller-selected participant count. Unit responses use the existing source bounds, canonical Part size and actual Borsh envelope codecs. IDs use one FullIds stream per signer; their logical chunk-queries still each have a fresh response.

The result contains stream/Part/control counts, streamed body bytes, total and maximum signed-envelope bytes, and the longest individual stream. Grouped source maxima cover the universe without enumerating all queries or allocating body/signature buffers. Serialization width exemplars are never signed, submitted or treated as acceptance facts. Arithmetic is checked; overflow refuses the component derivation. The existing source-only result remains separate and preserves its meaning.

This is not a complete service profile or funded admission certificate. Envelope signature count is not a calibrated signature CPU charge. Outer lifecycle/transaction wrappers, UTXO/mass/fee, retries/coalesced requests/rejected input and normal ingress budgets, preparation/restarts, data reauthentication, semantic/court/archive, IO/storage/byte-DAA and physical capacity remain outstanding. No zero prices or assumed delivery capacity fill those terms. An upper over distinct state changes does not bound an unbounded number of resubmitted packets. No new wire tag, original signature obligation, public dispatcher or production parameter is adopted.

The new component test uses concrete Borsh serialization at exact/ragged Part boundaries, enumerates all tiny source queries independently, checks single FullIds accounting and all original custodian slots, and checks atomic overflow refusal. These are derived bounds on tiny fixtures, not node load, real-model timing, mining throughput, Panel profit or heartbeat measurements. F02/F03, complete funded original graph, T01-T15 and measured door<=3,120 remain incomplete.
#### 12. Query-local rows identity without per-read ID ownership

Parent `52c20b99e`. Shared StepNode/StepLeaf/StepRun authentication now uses its original step root without reading, hashing or copying the unrelated complete IDs row. Trace RowsNode/TileNode/TraceTile still check the actual complete IDs against the original outer trace root on each authentication; they retain only an opaque source/trace/rows identity anchor, not an owned copy of the sequence. An exhaustive query match preserves provisional IDs behavior. No serialized anchor, trusted seal flag or model-computation certification is introduced.

`rows_anchor_from_complete_ids_v1` calls the unchanged complete-ID validation before constructing the existing private rows-identity capability. It does not use the native accepted-pointer constructor as a shortcut. Missing/modified IDs or another scope remain insufficient to authenticate trace data. The original complete-ID validation and all native source-authentication branches stay intact. FullIds ingestion still uses its owned retained IDs result; only per-query ownership is removed.

Data access is local authentication, not validation of the entire state. In direct corrupted-state test fixtures a valid step unit remains independently readable, while trace access and whole-state carriage loading reject corrupted/missing IDs. Those fixtures are not accepted native mutations. Loaded IDs rows and finished streams still undergo their original context/outer-root checks. Shared original/Ready/funding remain verified-half fixture assumptions.

The existing two signed-stream native fixtures add real trace data, reject trace publication before IDs, accept it after exact EOF, and check both data reads and full carriage/undo/replay. The source test compares borrowed-anchor and owned-ID authentication and rejects changed/short IDs and wrong rows identity. This is source-level elimination of unnecessary work/copy, not a measured CPU/RSS saving or calibrated full profile. Trace outer hashing, duplicate native reauthentication, EOF allocations, signed carriers, original branching, fees/IO/retention/supply and full G14 conditions remain outstanding.
#### 13. Nonexclusive numerical disclosure frontier

Parent `65dd205cd`. `PalwTirNumericalFrontierV1` is an unrooted numerical-material component for one fixed initial TIR phase. A disclosure is keyed by its path through the existing pinned child cuts. All siblings remain accessible; exploring one child never removes another. Identical material coalesces, changed material at the same path is refused, and all source-independent shape/interval/numerical fold checks are reused from the native timed phase. Hash-only localization does not replace numerical fold checking.

The native timed `apply_round` retains its error order, zero-window check, pending-state mutation and deadline update. Expanding its two extracted numerical helpers reproduces the original body byte-for-byte. Native `apply_choice` and the phase's Borsh fields are unchanged. Numerical projections do not apply a choice transaction, move the clock, authorize a signer, create a duty or grant default. Their inherited historic clock is explicitly not a new accepted-move clock; projections must never be installed as live phases or used for absence attribution.

For `N` terminal history tiles, at most `N-1` ranges disclose: each internal node has at least two nonempty children. This is per one fixed root, not a complete original quota over every leaf/root/claim. Projection checks path length before copying state and refuses missing ancestors/out-of-cut children. Stored disclosures are bounded by that geometry. Map/copy/replay/IO/retention and allocation costs still need native admission; this non-wire component is not a memory/capacity certificate.

Root construction checks only internal initial numerical shape. A future rooted coordinator must authenticate the actual original/source/program/leaf, native finalize, fixed rules/profile and literal producer/court signer, and install one canonical root per admitted logical scope without owner/first-requester exclusivity. An actor-specific legacy session ID or signature must not be reinterpreted as this new authority. The current component has no shared signature domain, notice, funding, native writer, public dispatcher or settlement transition. It does not complete F03 or RFC-0023 independent progress.

The new integration case starts from actually admitted native roots in the existing 24-position/two-position-tile dense-GQA corpus fixture, tests arities2/4 and honest/false-sum roots, explores all siblings and checks terminal arithmetic with the existing bottom checker. Bad folds refuse atomically, duplicates coalesce, changed disclosures fail, all twelve honest terminal ranges remain available and the native state/clock is unchanged. Fixture signatures use the existing exact-message test verifier, not actual ML-DSA/UTXO node admission. No production capacity, all-owner node T05 or measured real-model door is claimed.
#### 14. Named root binding without a fabricated session

Parent `e69fbcac4`. `check_named_tir_root_for_claim_v1` validates a named leaf's root against the actual retained claim and registered TIR class/program, class artifact root, execution/trace roots, native arity context and finalize predicate. It creates no challenger, ladder, court session or phase. The native session-based root admission keeps its existing session/phase/terminal checks and calls the same resolved-claim validator. The shared content-check body and native guards remain byte-identical.

This is a read-only numerical/source-binding predicate, not signer authentication, timely notice, a source service-profile certificate or paid admission. A caller must separately reconstruct actual public-original/source/profile identity, check a fresh signature in the new shared scope, establish canonical root uniqueness, admit clone/finalize work, fees/funding and dependencies, and install the complete reserved graph atomically. Wire-supplied rules or a legacy session signature cannot authorize the new shared path. The predicate does not inspect current native session availability or manufacture a session to evade it.

The existing numerical-frontier integration case now checks roots before any session exists, refuses missing claim/wrong leaf/arity mismatch/substituted execution root without state change, then admits the same root through the old native session path and explores all numerical branches. It uses the existing native-claim dense-GQA corpus fixture and exact-message signature verifier, not a public-original frame, funded shared ingress or real ML-DSA/UTXO node admission. Root binding and finalize do not establish that all reduction totals are truthful; existing bottom adjudication remains necessary.

No new wire/row/key, signature interpretation, deadline or production rule is introduced. Initial-notice equality remains the separate RFC-0031/ADR-0198 choice. Canonical shared root/signature authority and complete original/financial/supply integration remain unfinished.

#### 15. Private canonical numerical root retention candidate

After the producer-signed read-only predicate at `ca476389a`, the isolated candidate adds one immutable native root slot keyed by **(original claim, named leaf)**. Owner, requester, relay and policy ID do not allocate another slot. The original shared scope still binds allocation/source/rules/profile. Native acceptance checks the literal current producer key/activity/existing floor and complete numerical finalize against the retained public original; missing public original/context has no Base-claim fallback. The accepted row retains the complete stripped-program root carriage, signature, accepting native rules/point, original material horizon and authenticated producer public key. Later key changes or exit do not invalidate this historical statement; new submissions still require current native eligibility. This is a numerical statement, not computation credit.

Every repeat is freshly validated. The same fixed numerical root coalesces even with another valid signature or alternative legal finalize opening; it neither overwrites the first signed evidence nor re-dates acceptance. Changed policy/arity/elements/totals cannot replace the slot or create a parallel slot. The internal installer checkpoints and restores on refusal. Its clone allocation is admitted from the actual typed control/row/key inventory before cloning; map/index/journal allocation remains in the native writer. This is not a complete CPU/IO/fee/storage/supply profile or paid original admission.

Only new private candidate key22/row28 are appended; existing key0..21/row0..27 meanings remain. Native heap/field inventory, allocation-free nested wire preflight, row structure/context checks, state root, journal and carriage reload are updated together. The public signed-root DTO remains serialization-only: no public lifecycle decoder/dispatcher is introduced. The native snapshot row's reader is used after native carriage shape preflight/reservation and checks fixed signature/key lengths. A peer-supplied authenticated boolean is never accepted. Snapshot validation rechecks original scope, stored-key signature and numerical content; it does not require the historical key to remain current or reset the accepting point.

The immutable row can reconstruct the existing nonexclusive numerical frontier. Its logical identifier is original/leaf scoped, not a fabricated actor session, and its phase timing comes from the actual retained native rules/point. The returned frontier is arithmetic material only: projected historic deadlines must never be installed or used for absence/default. Signed shared rounds, rooted all-child disclosures and adjudication remain to connect. Per-root N-1/N geometry is not a complete per-original capacity reservation.

A minimum material TTL does not authorize root deletion. Automatic TTL sweep skips this immutable row; there is no qualified pruning/release path yet. Retention and repeated signature/hash/source/finalize work are uncalibrated costs to close in the full profile. The original service/source/claim dependencies, all-branch reservation/funds, native notice coordinator, public ingress, actual processing opportunity, T05/T01-T15 and measured real-model door remain incomplete. No pre integration, production schema/fee adoption, arming or deployment follows. Initial-notice A (inclusive a+3,120) is separately approved for private detailed implementation in RFC-0031/ADR-0198; it is not a notice/funding certificate for this root slot.

The first nested-root codec check exposed a pre-existing String visitor mismatch in artifact tensor names: typed String ownership counts the pinned two-times UTF-8 byte capacity envelope while wire preflight counted once. In the retained GQA corpus this was a 391-byte shortage in params, finalize and root requests. The shared String wire visitor now uses checked two-times byte capacity, matching the existing typed inventory; serialization, canonical bytes, hashes and configured memory/court caps are unchanged. Standalone UTF-8/String and nested golden-root codec tests cover this correction. These are allocation-request comparisons, not observed RSS, verification CPU, physical capacity or a fee calibration.

#### 16. Private signed numerical disclosure retention

After canonical root retention at `cee7f70a8`, an isolated candidate stores immutable signed numerical rounds. The logical root ID binds native network, original claim/shared source-profile scope, native policy, leaf, arity, literal producer and the root's elements/totals. A round signature in a new ML-DSA domain binds that root ID, the exact child path, the hash chain of actual retained ancestor round contents, producer and all child values. It excludes future accepting timestamps; native rules/point and authenticated current producer key are captured by the internal writer. Owner/requester/relay/signature nonce do not allocate another slot. Old session round signatures are not this authority.

The round key hashes root ID and path, excluding parent alternatives so that a different parent cannot fork the same logical slot. Prefix paths are strictly shorter. Preparation walks only these prefixes, authenticates every ancestor's stored-key signature, checks the actual parent-content chain and reuses native shape/interval/Sum/Max validation before projecting that child's numerical range. It does not recursively validate whole sibling subtrees or scan all disclosure rows. Every sibling remains a legal independent path. An actual u32 history has at most u32::BITS binary path levels; native pinned-cut/terminal/round-budget checks also apply. This is a derived codec bound, no new time or capacity parameter.

A fresh round must come from the canonical root's literal current active producer above the existing floor and the actual receiving native policy/point. It cannot precede root/ancestor acceptance or pass the original hard interval. An equivalent freshly validated numerical statement coalesces without overwriting the first signature/key/point; changed content cannot replace the slot. These are data conditions only. Historical records verify their stored keys independently of later current-key/floor changes. New private candidate key23/row29 append to unchanged key0..22/row0..28. Heap/scanner, fixed-length key/signature and derived path decoder bounds, native row/context validation, index/journal, snapshot preflight/reload and material holds are updated together. The signed public DTO remains serialization-only and has no lifecycle dispatcher.

The arithmetic projection helper accepts no accepting timestamp or response window and does not update a deadline. Its inherited historic root clock must never be installed or used to attribute absence. Native per-response duties, actual supply/delivery/inclusion, timely initial notice, fees/capital/all-branch reservation, funded public ingress, bottom adjudication integration, qualified pruning/settlement and T05/T01-T15 remain separate. Root/phase/control clone terms are derived from actual typed shapes and reserved before internal validation; source analysis, repeated cryptographic/numerical CPU, snapshot amplification, IO/fees/storage and physical capacity remain uncalibrated full-profile costs. Minimum material TTL is not deletion authority, and automatic sweeps skip immutable round records.

The history24 GQA public-original fixture is extended to retain all eleven internal binary disclosures, visit siblings in reverse order and reach all twelve terminal ranges. It uses actual ML-DSA controls over the existing verified-half original/registration/payment and synthetic unit-cost service book. Functional scope includes re-signed wrong root/parent/signer and bad Sum refusal, legacy domain refusal, duplicate state/journal invariance, typed row preflight/codec bounds, undo/replay, native carriage reload, missing-parent refusal and minimum-TTL retention. It is not actual UTXO/funding/rotation/supply or a node all-owner/refilling T05 campaign. No production schema/fee adoption, pre integration/push, arming or deployment follows.

#### 17. Private per-terminal bottom comparison records

After `8dbe9aa2f`, a private candidate connects stored terminal paths to the unchanged native `check_tir_dissect_bottom_v1`. A new filer-signed domain binds network, original/leaf/root, path, actual ancestor-content chain and complete carried bottom. Filer eligibility uses the existing current bond/activity/floor and requires no owner slot; another eligible filer may submit equivalent evidence for that terminal. Receiving native policy must match the canonical root policy, with actual acceptance after root/ancestors and before the existing shared-data hard bound. No initial-notice, service supply or paid reservation certificate is inferred.

The adapter checks the exact existing TirDissection proof byte count before payload cloning, without a full serialization buffer (one Borsh enum tag plus the actual carried body). The native close ceiling is unchanged. This is the proof-body gate, not a complete new control/carrier/mass/fee bound. The original header, leaf index and committed preimage are pinned before invoking the native checker, which can otherwise return a carriage conviction before its later narrowed-leaf check. The registered program is rehydrated from state, and the native binding/source/MAC rules and canonical operand checks remain in force. Unknown/invalid/unadjudicable evidence and cost excess are refusal, never victory or default.

Results are deliberately local: `NoMismatchFound` or the native fault/evidence identifier for this terminal. A no-mismatch does not certify the whole claim or close other branches. A mismatch may expose the producer's false signed numerical statement while the original committed execution is honest; it is not automatically a global ExecutorGuilty/claim-void/slash event. The adapter does not use the legacy session's global win/loss mapper. Responsibility, claim-wide termination, all unfinished liabilities, escrow/fees/rewards and release/pruning still require the full adopted coordinator and end-effect contract. No comparison result alone pays, slashes, returns collateral, licenses Final or authenticates consensus credit.

Candidate key24/row30 append to unchanged key0..23/row0..29. The immutable slot is root/path scoped, excluding filer/proof nonce, and retains the first complete signed evidence, authenticated filer key and actual native rules/point. An equivalent revalidated local result coalesces without replacing its first evidence or acceptance time. The stored outcome checksum is data identity, not a proof/certificate/checked flag: native reload recomputes signatures, ancestry and numerical evidence and compares the checksum. Snapshot-only nested decoding follows actual native shape preflight/reservation; path, signature and key vectors use existing bounds. No signed public DTO decoder/lifecycle dispatcher is introduced. Known root/phase/control/binding/program copies are shape-admitted before validation; evaluation/source/snapshot amplification, full CPU/IO/transport/mass/fees/storage and physical calibration remain unclosed. TTL alone cannot delete evidence.

The public-original GQA fixture now compares two separate candidate histories over the same honest committed execution: honest disclosures, and a validly folding signed Sum partition with opposite errors subsequently pushed into children. Terminal comparisons distinguish no mismatch from signed-statement faults while original claim and producer/filer balances stay unchanged. Original registration/pledge signature/payment and service book remain verified-half/synthetic fixture assumptions; new controls use real ML-DSA. No actual UTXO/funding/supply/rotation or node all-owner/refilling T05 is claimed. Full admission/reservation/notice/opportunity/end-effect integration, T01-T15/R-D2/tag230 and measured real-model door remain incomplete; production adoption, pre integration/push, arming and deployment remain absent.


#### 18. Private numerical evidence byte ceilings and original-wide inventory

The original-wide candidate derives every eligible numerical root and every branch from the literal original, authenticated registered program/layout and explicitly resolved native court/demand rules. It aggregates repeated occurrences, actual post positions, unsaturated/saturated histories and ragged output tiles. Root count is independent of retained roots and owners. Disclosure and terminal path bytes are counted over the full forest. These analytical results authenticate neither a policy nor funded admission.

The old class close sizer is not substituted for this inventory: its root branch starts after position0, where the old dialogue uses a whole close. A shared root may name a one-tile history. The private root checker now enforces the existing old root carrier allowance explicitly, with a streamed Borsh count before source/signature evaluation. Its payload ceiling subtracts the old lifecycle/session/arity/signature framing and existing signer-reference allowance from the unchanged carrier cap. An actual old DTO serialization checks that derivation. The new shared header is added separately; no larger cap is adopted and no old tag/codec is repurposed. The byte predicate alone establishes no valid root or global verdict.

The request-set inventory bounds each canonical root, internal disclosure and terminal record once. Root payload uses that enforced allowance; terminal payload uses the already enforced native close ceiling minus the old proof tag. Actual new DTO framing, fixed signature length and every path byte are added. Ceilings are conservative upper bounds, not observed usage, minimal capital or a model-exclusion criterion. The new root control upper exceeds the old single-carrier allowance, so successful payload checking does not establish carriability. Bounded fragmentation/reconstruction, transaction/mass, repeated/invalid ingress, source/semantic/court/archive, CPU/IO/storage/retention/fees, native-policy authentication, funding and atomic original admission remain required. F02/F03/T05 and G14 remain incomplete. This is an unadopted private candidate, with no public lifecycle, production rule replacement, arming, pre integration/push or deployment.


#### 19. Private complete-bundle numerical fragment transport

A separate `numerical-manifest/v1` / `numerical-body/v1` namespace carries the unchanged serialized producer/filer-signed root, round or bottom. DATA stream target tags and existing lifecycle tags are untouched. Manifest binds network, original scope/claim/leaf, kind, total bytes and body hash; parts bind its identity and exact byte offset. The part ceiling is the existing owned-DATA part size. Manifest and part readers reject unsupported versions and unbounded part lengths before payload allocation. These hashes establish data identity only; unsigned parts never substitute for the inner actor signature or numerical checking.

Sender hashes the Borsh body without an intermediate whole-body buffer, then emits canonical parts through one reusable bounded buffer. Retaining emitted parts is the caller's separately admitted work; emission/prefixes are not consensus acceptance. Complete-bundle reception checks actual native network/scope/grammar ceilings, count, stream identity, contiguous offsets, exact nonfinal/final part lengths and body hash. A borrowed parts reader feeds the existing allocation-free native DTO shape scanner, reserves the actual typed shape before nested decoding and retains that ticket with the decoded owner. It never concatenates a full receive body. Global unbounded `BorshDeserialize` implementations are not added to the signed control DTOs. Local resource refusal remains distinct from native semantic refusal; no peer quote or checked flag is trusted.

An internal adapter requires the actual native point/network and retained original context, then dispatches parsed controls to the existing root/round/bottom folds. Signatures, actor eligibility, original/header/policy, canonical root/ancestor/path, byte gates and numerical evidence are rechecked there. Root transport must produce identical state/journal to direct typed submission; terminal outcomes remain local, without slash/credit/settlement. Invalid, incomplete or altered bundles mutate no state. No public lifecycle/block dispatcher invokes the adapter.

This component handles **already complete bundles**. It does not allocate consensus prefix ownership or let an incomplete first filer block another filer. Multi-block prefix persistence/retention, bounded paid public ingress, fair reserved completion under malicious attempts, restart/reorg/pruned IBD, transaction/mass and full hash/scan/decode/reconstruction/signature/CPU/IO/storage/fee calibration remain required. A fragment's codec size is not a whole transaction/carriability guarantee. No actual all-owner/refilling node T05, service opportunity, default, funded original admission, real-model door or G14 completion is claimed. Production adoption, pre integration/push, arming and deployment remain absent.
