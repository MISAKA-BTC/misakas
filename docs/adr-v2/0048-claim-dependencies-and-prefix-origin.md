# ADR-v2-0048 — Claim依存関係・独立進行・prefix生成元

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0008](../rfc-v2/0008-claim-dependencies-and-liability.md)

## 1. この文書の役割

未解決の計算を正しいと扱わず、独立した仕事だけを進めるための依存identityをまとめる。

## 2. 統合した判断の読み方

### 2.1

claim Aの責任を保持したまま、Aに依存しないBだけを進める。stage 1の局所期限とstage 2のcheckpoint・fork choice・settlement変更を区別する。[旧ADR-0186](../adr/0186-hold-unresolved-liability-advance-independent-claims.md)

### 2.2

prefix exportは生成元の署名済みstatementを実受理claim IDへ結び付け、consumerは受理済みidentityを参照する。source不在を独立入力へ変換しない。[旧ADR-0208](../adr/0208-authenticated-prefix-work-dependencies.md)

### 2.3

新consumerのhard deadline・material保持を全sourceの既存署名期限と予約内に収め、再利用によりsourceの責任を勝手に延長しない。[旧ADR-0208](../adr/0208-authenticated-prefix-work-dependencies.md)

## 3. 維持する境界・後続改定

ADR-0208は採択済みでも実装・検証・有効化未完という原文の状態を保持する。旧cohortへexportを後付けしない。

## 4. 隣接する判断との分担

仕様は[RFC-v2-0008](../rfc-v2/0008-claim-dependencies-and-liability.md)、基礎contextは[ADR-v2-0025](0025-context-and-input-commitments.md)、保存・同期は[ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0186-hold-unresolved-liability-advance-independent-claims.md](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) | L1–L31 | L3–L6 |
| [0208-authenticated-prefix-work-dependencies.md](../adr/0208-authenticated-prefix-work-dependencies.md) | L1–L21 | L3–L3 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0186-hold-unresolved-liability-advance-independent-claims"></a>

<details>
<summary>0186-hold-unresolved-liability-advance-independent-claims.md — 原文 L1–L31</summary>

[原文を開く](../adr/0186-hold-unresolved-liability-advance-independent-claims.md) · SHA-256: `daf1d76e3ce7473b9f6233380658e2e7a9f0c4a6d257a60949edf399aeeea5c1`

<!-- ADR_SOURCE_BEGIN:0186-hold-unresolved-liability-advance-independent-claims.md -->
~~~~~~~~markdown
# ADR-0186 — 未解決 claim の責任を保持し、それに依存しない claim を進める

- **Decision:** Accepted — ユーザー指示 2026-10-17。
- **Scope:** RFC-0018 の共有 safe frontier と、それを読む決済・fork choice・pruning。
- **Implementation:** 段階 1・段階 2 とも休眠 fence(実装中)。
- **Related:** [RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)(設計凍結の手続き)。

## 決定

**遅い claim A を正しいことにして追い越さない。A の責任を未解決のまま確実に保持し、A に依存しない B だけを進める。**

- 段階 1(次回 fence の候補):現行の frontier 式を保ち、claim ごとの未解決期限 `受理 DAA + L_class` を受理時の資源・期限の整合で保証する。期限到達は不正の証明ではなく、係争中の担保・証拠を解放しない。B の各段階(Final・認証・保留解除・支払い出力・`settled`・native safe)を別々に測る。
- 段階 2(別の休眠 fence):未解決集合を担保・証拠付きで checkpoint に拘束し、依存の単位を claim とその依存関係にして、独立した claim の認証・決済・`settled` を進める。checkpoint・fork choice・settlement・native lifecycle・pruning/IBD を一緒に変える。

## 理由

DAG による並列生成と、個別の仕事・送金を独立に確定できることは別問題である。現行規則は安全だが、計算上独立な B・C まで共有の確定で A を待たせる。モデル別 frontier の最小・最大・多数決や時間経過による無視は、待ち続けるか責任を失うかのどちらかになる。

## 帰結

- 段階 1 は既存の bounded admission・detection policy・retention の規則の上に置く。
- 段階 2 は G14(未解決 A を外部の正直な検証者が後から反証できること)を checkpoint の要件に含める。
- fence 前の動作と testnet-12 の identity は変えない。

## 追記(2026-10-18):段階 1 の測定と RFC-0018 (ii)(iv) の変更

ADR-0181 の設計凍結に従い、設計の変更をここに記録する。値はどれも変えていない。

- **段階 1 の測定。** 6 つの原因(資料応答の遅延、正当な court、虚偽告発、再抽選、court 容量の飽和、遅延 claim の継続投入)ごとに定常遅れを測った。どれも min(保持時間, L) + 投入間隔 + 認証時間 以内だった(G14 完全性証明 §13.8)。node の restart と genesis からの replay は、fence をまたいで resolved prefix まで一致した。
- **RFC-0018 (ii):mergeset の class 順(休眠の `palw_dag_sibling_attribution_v1` の中)。** class を最初の member の priority で並べると、attempt の多い model ほど先頭に来る。16 carve の予算の下では、1 attempt の model が公平な取り分の 13 分の 1 しか入らなかった。class の順を `H(domain ‖ set seed ‖ class)` に変えた(set seed は mergeset の attempt priority の hash)。SET の関数である性質、round robin、class 内の順、16 carve は変えていない(§13.9)。
- **RFC-0018 (iv):Panel の own receipt の class lane(node policy、consensus の変更なし)。** 1 本の in-flight carrier では、court の queue が空にならない間、OwnReceipts の site に slot が回らない。own receipt は class 数に関係なく窓を外した。V07 の readiness lane と同じ型の lane を class ごとに置いた。own receipt は、main slot が取れないときだけ、その class の lane(自分の確認済み output、1 本ずつ in flight)に乗る。lane は 1 seat あたり 16 本まで。他の chain の carrier の子は作らないので、relay の性質は V07 と同じである(§13.10)。
~~~~~~~~
<!-- ADR_SOURCE_END:0186-hold-unresolved-liability-advance-independent-claims.md -->

</details>

<a id="source-0208-authenticated-prefix-work-dependencies"></a>

<details>
<summary>0208-authenticated-prefix-work-dependencies.md — 原文 L1–L21</summary>

[原文を開く](../adr/0208-authenticated-prefix-work-dependencies.md) · SHA-256: `60c8daa15b83fc5bc1d1cd7103014db35e6a82880d07262313784b934418f37a`

<!-- ADR_SOURCE_BEGIN:0208-authenticated-prefix-work-dependencies.md -->
~~~~~~~~markdown
# ADR-0208 — 再利用 prefix の生成元 identity を明示する案

Status: **Accepted / 採択済み** (2026-10-28)。実装中、検証・有効化は未完。

## 背景

[RFC-0040](../rfc/0040-authenticated-prefix-work-dependencies.md) のとおり、現行の prefix descriptor は生成元 work identity を含まない。[RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md) が要求する入力依存を root の偶然の一致から推定することはできない。

## 決定

既存 wire は保ち、新 version の署名済み prefix export statement と消費側 reference を追加する。生成元は受理前に network、work namespace、nonce、自己参照しない base claim commitment、完全な descriptor と有限な責任・材料期限を署名する。consensus が実際の claim ID と受理点に結び付け、消費側はその受理済み identity を署名対象にする。将来の受理 block を producer に先取り署名させない。export は生成元の受理時に commitment され、その正しさは生成元の claim 単位の再実行・court で検査する。署名・hash は同一性だけを認証する。

新 fence 後の非 genesis prefix を再利用する新 claim は、受理済み export への明示 reference を必要とする。source 不在を独立な入力に変換しない。消費側の hard と材料保持は推移的な全 source の既存署名期限・予約内に収め、新たな消費で source を延長しない。旧 cohort の条件は変えず、旧 claim に export を後付けしない。

## 帰結

消費側の実際の依存、生成元退役後の責任、pruning/IBD 後の資料を同じ identity で追跡できる。一方、新規 export/envelope の wire、検査経路、資料保持、移行試験が必要になる。既存の未測定値や欠けた資料を、この提案の採択で埋まったと扱わない。

## 採択記録

2026-10-28、ユーザーが「採択して実装・検証を進める」と明示承認した。生成元と消費側の自己参照しない base commitment、および推移的な署名済み保持期限の範囲を含む。採択は実装完了や試験通過の記録ではない。preset と arming は実装・検証・証拠 review の完了後に変更する。
~~~~~~~~
<!-- ADR_SOURCE_END:0208-authenticated-prefix-work-dependencies.md -->

</details>

