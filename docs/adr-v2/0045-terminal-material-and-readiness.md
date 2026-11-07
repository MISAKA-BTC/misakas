# ADR-v2-0045 — 終端material受付・非独占提出・readiness opening

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0014](../rfc-v2/0014-court-master-contract.md)

## 1. この文書の役割

大きな終端証拠の受付と、readiness rootのopeningを外部者が取得する経路をまとめる。

## 2. 統合した判断の読み方

### 2.1

認証済みmaterialとterminal intentを分ける案を記録し、提出者の占有により正当な終端提出が妨げられる問題を扱う。[旧ADR-0199](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md)

### 2.2

readiness rootがあるだけでは相手のdigest／pathを復元できないため、そのopeningを実際に供給する契約を別に設ける。[旧ADR-0200](../adr/0200-d2-readiness-root-opening-availability.md)

### 2.3

codecの選択、書込み・保持・料金・提出上限の承認を一つの「実装済み」にまとめない。[旧ADR-0199](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md) [旧ADR-0200](../adr/0200-d2-readiness-root-opening-availability.md)

## 3. 維持する境界・後続改定

A案の詳細化・隔離実装承認を、本番wire／新料金／保持契約の採択へ拡張しない。未開示への処置は適用契約と客観的受付で判断する。

## 4. 隣接する判断との分担

shared queryは[ADR-v2-0044](0044-shared-tir-query-data.md)、応答資格は[ADR-v2-0043](0043-court-eligibility-and-initial-notice.md)、実資金は[ADR-v2-0047](0047-funded-original-and-role-authorization.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0199-shared-court-terminal-material-and-nonblocking-ingress.md](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md) | L1–L25 | L3–L3 |
| [0200-d2-readiness-root-opening-availability.md](../adr/0200-d2-readiness-root-opening-availability.md) | L1–L25 | L3–L3 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0199-shared-court-terminal-material-and-nonblocking-ingress"></a>

<details>
<summary>0199-shared-court-terminal-material-and-nonblocking-ingress.md — 原文 L1–L25</summary>

[原文を開く](../adr/0199-shared-court-terminal-material-and-nonblocking-ingress.md) · SHA-256: `d96b38a24ea12f01bb37e8c3340aff08d1ec3559bd77b469fe698a46e798ba72`

<!-- ADR_SOURCE_BEGIN:0199-shared-court-terminal-material-and-nonblocking-ingress.md -->
~~~~~~~~markdown
# ADR-0199 — 共有court終端materialと非独占提出方式

Status: **Proposed / Review Required**。Aの詳細仕様化・隔離実装を利用者が承認済み。本番規則は未採択。詳細は[RFC-0032](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md)。

## 背景

F03のためのmulti-block証拠受付に、先着filerの未完成prefixをroot/path唯一slotの所有権として導入すると独占を再発させる。現候補はcomplete bundleまでで、その保存方式を実装していない。既存fixtureの24終端はparameter openingとcarriageを必要とし、共有DATAの閉集合だけでは入力が足りない。

## 選択肢

**A（詳細仕様化・隔離実装承認済み）: 認証済み原始materialと終端intentを詳細仕様化・隔離実装する。** parameter/promptも含む認証済み入力から既存builderを動かし、任意filerの不完全bodyを共有終端slotの独占権にしない。既存契約による供給・保持が足りなければ受付を拒否する。新しい保持者義務・資本額をこの承認で本番導入しない。

**B: 独立した有料filer別streamを詳細仕様化する。** 別filerを妨げない一方、有限資本/容量で共謀者の無限再充填下の完了を示す必要がある。Active bondやfeeがあることだけでは証明できていない。

## 判断範囲

2026-10-21、利用者は「A：認証済みmaterialと終端intent」を選択した。Aの詳細仕様化・隔離実装を進める。本番規則の置換、公開wire/新料金/新保持契約の採択、pre統合・push・武装・配備は含まない。選択だけではF02/F03/T05やG14完成にはならない。採択前でも既存builderのread-only adapter、認証・資源・依存調査と試験は継続する。

## 受入条件

originalに束縛した認証とnative bounded入力、全合法分岐の実原資/予約、欠落時no-slash、他filerによる独占不可、元期限保持、restart/reorgと責任archive、実node T05を要求する。全modelに対する審査は行わない。未決単価・資源・配送/保持条件を仮値で埋めず、実装済み・測定済み・採択済みを区別する。

## 実装承認の限界

この承認で、認証済みparameter/prompt/step/trace/ID materialから既存builderへ接続する隔離候補を進める。sourceの認証predicateやbounded decoderだけでは、materialの保存・取得・原資・保持horizon・全分岐予約・処理機会を満たさない。新しい義務・価格・本番schemaの採択済みと扱わず、未決値は引き続きProposedとして残す。
~~~~~~~~
<!-- ADR_SOURCE_END:0199-shared-court-terminal-material-and-nonblocking-ingress.md -->

</details>

<a id="source-0200-d2-readiness-root-opening-availability"></a>

<details>
<summary>0200-d2-readiness-root-opening-availability.md — 原文 L1–L25</summary>

[原文を開く](../adr/0200-d2-readiness-root-opening-availability.md) · SHA-256: `95e023723a87246e34c014a428f671520a571c9439fae4c1ce1b181657f1fb78`

<!-- ADR_SOURCE_BEGIN:0200-d2-readiness-root-opening-availability.md -->
~~~~~~~~markdown
# ADR-0200 — D2 readiness rootの認証path供給

Status: **Proposed / Review Required**。ADR-0188 Bの詳細codec選択は承認済み。本ADRはBの選択をやり直すものではなく、rootに隠れた偽digestを第三者が実際に開ける経路を決める。[RFC-0033](../rfc/0033-d2-readiness-root-opening-availability.md)に規約・検証対象を具体化した。

## 問題

root単独からseatがcommitしたdigest/pathは復元できず、検証者自身の正しいモデルcopyだけでもその欠落は埋まらない。root/pathのpure認証器をnativeへ配線しただけでは、seatがopeningを一切渡さない反証不能な経路が残る。

## 選択肢

| 候補 | 内容 | 必須費用・残件 |
| --- | --- | --- |
| C1（推奨候補） | 承認済みF02契約へ、モデルbyteを含まないdigest/path開示courtを接続 | 元draw全許容分岐の予約・原資・保持とliteral seat authority、実供給、no-slash経路、公開要求/応答とqualified効果 |
| C2 | 全drawの補助digest materialが完成公開され、root一致と完全性を確認してからrootを受理 | 全digestの有料転送・保持・再取得、有限・非独占なmaterial公開とnative完全性検査 |

C1はBのtop-level rootの小ささと必要座標だけのopeningを維持し、F02の未供給規則を再利用する。一方でF02の原子的受付・実供給を完成させるまでnativeのready効果を許可できない。C2は全体転送を要するが、受理前の欠落を明確に拒否できる。どちらも未決のdraw上限や料金を決めた扱いにしない。

## 採択境界

利用者へ求める判断は、C1/C2の詳細仕様化・隔離実装だけである。新しい本番義務/料金/保持期間の採択、pre統合、武装、配備、モデル単位の審査/係数変更は含まない。未回答ならcodecの純粋な検査と、方式に依存しないoriginal coordinatorの作業を続ける。rootだけのready/slash入口を暫定的に開かない。


## C1の詳細仕様化・隔離実装承認 — 2026-10-21

利用者はC1（F02のdigest/path開示court）を選択した。上の未選択記述は当時の履歴である。literal signer・実供給とno-slash経路・全draw分岐の原資/予約へ接続する。draw上限・本番料金・保持義務の採択、pre統合・武装・配備、モデル審査の導入は含まない。
~~~~~~~~
<!-- ADR_SOURCE_END:0200-d2-readiness-root-opening-availability.md -->

</details>

