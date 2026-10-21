# ADR-0198 — 共有courtのinitial noticeの等号

Status: **Proposed / Review Required** (2026-10-21)。詳細仕様化の選択待ち。本番採択・旧claim変更・統合・有効化ではない。

## 未決の理由

[RFC-0026 §2.2](../rfc/0026-court-policy-master-contract.md#22-境界表f09値は未決形式だけ固定)はnotice等号を未決としている。ADR-0192 A/Aの指示は共有frontierと実供給方式の詳細仕様化であり、この等号を選んだものではない。[RFC-0031](../rfc/0031-shared-court-initial-notice-boundary.md)に具体的な区間・境界表・接続条件を置いた。

## 選択肢

| 案 | 受理DAA条件 | 差分 |
| --- | --- | --- |
| A（推奨） | `a <= d <= a + 3,120` | 終端DAAを含む。値3,120と元起点を維持し、等号を明示する |
| B | `a <= d < a + 3,120` | half-open。最後のnoticeは`a + 3,119`で、Aより1 DAA早い |

推奨はAで詳細仕様化・隔離predicate実装を進めること。供給の保証、hard/collect/応答窓の他の等号、turn duration・fee・wire・配送前提を同時に採択しない。数値は仮値で埋めず、残余の全graphと実費を別途閉じる。

## 判断の範囲

求めるのはA/Bによる詳細仕様化と隔離実装の選択である。[docs/AGENTS.md](../AGENTS.md)の設計凍結に従い、旧規則の置換や本番採択・pre統合・武装・配備へ広げない。未回答でも、数値foldを保つ非排他的frontier、資本・全graphの独立調査と検証は継続できる。
