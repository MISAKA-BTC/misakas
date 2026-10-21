# ADR-0199 — 共有court終端materialと非独占提出方式

Status: **Proposed / Review Required**。未採択。詳細は[RFC-0032](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md)。

## 背景

F03のためのmulti-block証拠受付に、先着filerの未完成prefixをroot/path唯一slotの所有権として導入すると独占を再発させる。現候補はcomplete bundleまでで、その保存方式を実装していない。既存fixtureの24終端はparameter openingとcarriageを必要とし、共有DATAの閉集合だけでは入力が足りない。

## 選択肢

**A（推奨）: 認証済み原始materialと終端intentを詳細仕様化・隔離実装する。** parameter/promptも含む認証済み入力から既存builderを動かし、任意filerの不完全bodyを共有終端slotの独占権にしない。既存契約による供給・保持が足りなければ受付を拒否する。新しい保持者義務・資本額をこの承認で本番導入しない。

**B: 独立した有料filer別streamを詳細仕様化する。** 別filerを妨げない一方、有限資本/容量で共謀者の無限再充填下の完了を示す必要がある。Active bondやfeeがあることだけでは証明できていない。

## 判断範囲

利用者に求めるのはAまたはBの詳細仕様化・隔離実装の選択。本番規則の置換、公開wire/新料金/新保持契約の採択、pre統合・push・武装・配備は含まない。選択だけではF02/F03/T05やG14完成にはならない。採択前でも既存builderのread-only adapter、認証・資源・依存調査と試験は継続する。

## 受入条件

originalに束縛した認証とnative bounded入力、全合法分岐の実原資/予約、欠落時no-slash、他filerによる独占不可、元期限保持、restart/reorgと責任archive、実node T05を要求する。全modelに対する審査は行わない。未決単価・資源・配送/保持条件を仮値で埋めず、実装済み・測定済み・採択済みを区別する。
