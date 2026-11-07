# ADR-v2-0047 — Court originalの実原資・予約・役割別署名

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 1本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md) / [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

Court originalを全分岐で受け付けるための実原資と、役割別の同意をまとめる。

## 2. 統合した判断の読み方

### 2.1

burn済みrentやcollateralへの名目exposureを、Panel／relayへの将来作業料の前払原資として流用しない。[旧ADR-0202](../adr/0202-court-original-funded-contract-and-role-consent.md)

### 2.2

実UTXOで裏付けた独立escrowの案と、poster／provider／worker等の署名済み責任・保持・金融条件を結び付ける。[旧ADR-0202](../adr/0202-court-original-funded-contract-and-role-consent.md)

### 2.3

全分岐予約のatomic admissionに必要なgraph・資源・資金・供給同意をまとめ、旧TFP1／custody署名から新しい義務を自動導出しない。[旧ADR-0202](../adr/0202-court-original-funded-contract-and-role-consent.md)

## 3. 維持する境界・後続改定

Aの詳細化・隔離実装は承認されていても、金額・新料金・本番規則・有効化は別。統合文書はその未決値を埋めない。

## 4. 隣接する判断との分担

共通予算は[ADR-v2-0035](0035-bond-budget-and-shared-reservations.md)、fee同意は[ADR-v2-0037](0037-fee-escrow-and-role-consent.md)、serviceとterminalは[ADR-v2-0042](0042-court-service-and-versioned-clock.md)・[ADR-v2-0045](0045-terminal-material-and-readiness.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0202-court-original-funded-contract-and-role-consent.md](../adr/0202-court-original-funded-contract-and-role-consent.md) | L1–L35 | L3–L3 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0202-court-original-funded-contract-and-role-consent"></a>

<details>
<summary>0202-court-original-funded-contract-and-role-consent.md — 原文 L1–L35</summary>

[原文を開く](../adr/0202-court-original-funded-contract-and-role-consent.md) · SHA-256: `ab4e490cec8181b7db7d9e65b07746bf7a84b8c0f878999d30935fb71fd6426b`

<!-- ADR_SOURCE_BEGIN:0202-court-original-funded-contract-and-role-consent.md -->
~~~~~~~~markdown
# ADR-0202 — shared court originalの実原資と役割別同意

Status: **Proposed / Review Required** (2026-10-21)。[RFC-0035](../rfc/0035-court-original-funded-contract-and-role-consent.md)に具体化。ユーザーはA（実UTXOで裏付けた独立escrow）の詳細仕様化・隔離実装を承認済み。金額・新料金・本番採択・pre統合・武装・配備の承認は含まない。

## 問題

既存rentはburnされ、既存depositは担保への名目exposureである。いずれも将来のPanel/relay作業料の前払原資へ流用できない。旧TFP1/custody署名も、新しい全分岐graphと新役割の金融/保持義務に同意していない。原子的な全分岐受付を成功させるには、実原資と役割別同意を新契約へ接続する必要がある。

## 選択肢

| 案 | 実資金の確保 | 影響 |
| --- | --- | --- |
| A（推奨） | originalの実入力で全額裏付けた独立運用escrow。実outpoint/script/spendとrooted once-only accountingを原子的に照合 | 元bond/予約資本・旧rent/feeの意味を維持。新しい専用資金/支出規則・wallet・UTXO検査と会計の実装が必要 |
| B | 新original carrier feeの実徴収額を運用口へ分配し、実資金と将来支払を追跡 | 新fee/collectible/coinbase/burnの会計経路が必要。旧rentの遡及再配分は不可 |

Aで詳細仕様化・隔離実装を進めることを推奨する。どちらも元claimに束縛された新署名と役割別同意、canonical相対graph、実accepting DAAでの期限導出が必要。producerが未同意の将来actorへ義務を課す経路を作らない。

## 承認の範囲

求めるのは資金方式の詳細仕様化・隔離実装の選択である。金額・draw上限・新料金・保持期間・配送前提・本番採択・旧claim移行・pre統合・武装・配備を含まない。数値や実profileが未決なら公開入口を拒否し、仮資金で合格にしない。RFC-0027のAccount Bondを本書だけで採択/実装済みにしない。

既存のF02/F03・RFC-0032 A・archive S1の承認は、ここで新しく必要になった資金保持/支払方式の採択を兼ねない。未回答の間も、方式に依存しないcanonical graph/依存閉包/署名/原子性と試験を続ける。実装・検証結果は未記録。

## 2026-10-21 ユーザー選択 — Aの詳細仕様化・隔離実装

ユーザーは「A：実UTXOで裏付けた独立escrow」を選択した。上の選択待ち表記は選択前の履歴である。実outpoint・支出・保存則と役割別同意を束縛する詳細仕様化・隔離実装に進む。金額・新料金・draw上限・保持期間・本番採択・pre統合・武装・配備を含まない。実UTXO/決済・全費用/資本・原子的original受付の接続と検証は未完であり、この選択だけから資金確保済み・G14完了を導かない。

## 隔離実装の第一段階

[RFC-0035 §8](../rfc/0035-court-original-funded-contract-and-role-consent.md#8-aの隔離実装の第一段階)に別TFO1 transport、新payer署名、厳密な実出力/返還鍵照合、同carrierのchecked fee budgetを記録した。[検証記録と残りの接続](../evidence/palw/g14-completion-2026-10-21/funded-original-stage1.md)は、準備検査と実UTXO lock/支払/原子的original受付を区別する。新入口は旧形式へ降格せず拒否する。rooted資金・同mergeset拘束・第三者の適格支出と全分岐受付は未完成であり、本番採択・有効化保留を維持する。


## 2026-10-22 接続条件と隔離部品

[RFC-0035 §9](../rfc/0035-court-original-funded-contract-and-role-consent.md#9-native-feeとsource-graphの接続--2026-10-22)は既存feeのburn/rent二重充当を修正し、UTXO受付前の同branch planと同一WriteBatchでのrooted delta/UTXO保存・undo/replayの接続条件を記録する。[数値geometryとtransaction-bound preflight](../evidence/palw/g14-completion-2026-10-22/numerical-geometry-and-funding-preflight.md)は全pathの導出と実transaction/entry/scope拘束を進める部品で、native支出認可・実escrow lock・完全original受付・金銭精算を成立させるものではない。Aの承認範囲と有効化保留を維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0202-court-original-funded-contract-and-role-consent.md -->

</details>

