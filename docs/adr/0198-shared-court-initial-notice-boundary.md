# ADR-0198 — 共有courtのinitial noticeの等号

Status: **Proposed / Review Required** (2026-10-21)。利用者は2026-10-21にA（終端DAAを含む）の詳細仕様化・隔離実装を承認済み。本番採択・旧claim変更・統合・有効化ではない。

## 判断の背景

[RFC-0026 §2.2](../rfc/0026-court-policy-master-contract.md#22-境界表f09値は未決形式だけ固定)はnotice等号を未決としている。ADR-0192 A/Aの指示は共有frontierと実供給方式の詳細仕様化であり、この等号を選んだものではない。[RFC-0031](../rfc/0031-shared-court-initial-notice-boundary.md)に具体的な区間・境界表・接続条件を置いた。

## 選択肢

| 案 | 受理DAA条件 | 差分 |
| --- | --- | --- |
| A（推奨） | `a <= d <= a + 3,120` | 終端DAAを含む。値3,120と元起点を維持し、等号を明示する |
| B | `a <= d < a + 3,120` | half-open。最後のnoticeは`a + 3,119`で、Aより1 DAA早い |

利用者の回答「A：終端DAAを含める（推奨）」に従い、Aで詳細仕様化・隔離predicate実装を進める。供給の保証、hard/collect/応答窓の他の等号、turn duration・fee・wire・配送前提を同時に採択しない。数値は仮値で埋めず、残余の全graphと実費を別途閉じる。

## 判断の範囲

承認範囲はAによる詳細仕様化と隔離実装である。[docs/AGENTS.md](../AGENTS.md)の設計凍結に従い、旧規則の置換や本番採択・pre統合・武装・配備へ広げない。数値foldを保つ非排他的frontier、資本・全graphの調査と検証も継続する。

## 補記 2026-10-26(G14 lane CLOCK、未武装)

独立レビュー第 2 部 B-1 への対処として、kernel route の claim にも本 ADR の時計を適用する方式を選んだ(「X3 は kernel slice に適用しない」案は採らない)。X3(`palw_claim_hard_deadline_v1`)の高さ以降に commit された kernel claim は、Final 後・pre-Final default 後の liability horizon を `committed + 3,120 + court_deadline + proof_grace` 以上に保つ(`misaka_palw_kernel::claim_clock`)。notice a+3,120(inclusive)に filing した告発は同じ block で裁かれ、demand の経路も horizon に収まり、horizon は a+10,000 を超えない。hard a+10,000 と notice の値は変えていない。fence 未満は byte 同一。`palw_court_clock_v2` は legacy V2 dispute record 用(`palw_legacy_public_filer_v1` が前提)なので、slice には入れていない。
