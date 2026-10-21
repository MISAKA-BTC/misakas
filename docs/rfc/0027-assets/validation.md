# RFC-0027 / ADR-0193 — 検証記録

**日付:** 2026-10-20。文書作成・ローカル検算の結果であり、採択・合意実装・node-level達成・経済妥当性・有効化を宣言しない。

## 1. 実施結果

| 確認 | 結果 | 範囲・限界 |
| --- | --- | --- |
| 整数会計・資金フローの16検算 | PASS（16/16） | 合成例の数式のみ。実際の全分岐、署名、資源供給、裁定、reorgは未実装 |
| 新規RFC/ADRの番号重複 | このZIPと会話の範囲で衝突なし | remote branchの予約は未確認 |
| 文書索引の更新 | 実施 | 既存の索引ずれも生成器で更新。状態を採択済みに昇格させない |
| 今回のリンク欠損の増分 | **0件** | 入力前後の欠損集合を比較 |
| repository全体の `check_docs.py` | **FAIL（既存欠損）** | 入力に既にある6件のbaseline外欠損が未解決 |
| 既存 `known-link-issues.json` | 変更なし | 既存の欠損をbaseline追加で隠さない |
| Rust / node / 統合試験 | 未実施 | 合意コード、runtime設定、fenceを変更していない |
| Panel費用・参入・REAL/heartbeat実測 | 未実施 / UNKNOWN | 合成資本感度表から経済達成を推定しない |

repository全体には欠損参照49件があり、そのうち43件は既存baselineに記録済み、残る6件も今回の作成前から存在する。入力前にはRFC索引のずれもあり、生成器で更新した。検査の「new」は既存baselineに対する意味であり、今回追加した欠損という意味ではない。

## 2. 実行したコマンド

```sh
python3 docs/maintenance/check_docs.py --json-out /tmp/docs-before.json
python3 docs/rfc/0027-assets/reproduce_examples.py --json-out /tmp/examples-results.json
python3 docs/maintenance/generate_indexes.py
python3 docs/maintenance/check_docs.py --json-out /tmp/docs-after.json
```

`check_docs.py` は前後とも終了コード1。数式検算と索引生成は終了コード0。生成後の参照欠損集合から入力前の集合を引いた差は空集合である。文書検査だけで合意の意味や実装の安全性を保証しない。

## 3. 入力に存在する未解決6件

以下は診断文字列であり、壊れたリンクを新たに作らないためコードとして記載する。

- `new link issue x1: missing-doc-file|docs/rfc/0024-assets/ADR-DRAFT-palw-court-door-3200.md|docs/rfc/0024-assets/RFC-DRAFT-palw-court-door-3200.md|`
- `new link issue x1: missing-doc-file|docs/rfc/0024-court-door-3200-to-ctx-2m.md|docs/evidence/palw/g14-measure-2026-10-18|`
- `new link issue x1: missing-doc-file|docs/rfc/0025-assets/ADR-DRAFT-scalable-court-v2.md|docs/rfc/0025-assets/RFC-DRAFT-palw-scalable-objective-court-v2.md|`
- `new link issue x1: missing-doc-file|docs/rfc/0025-assets/README.md|docs/rfc/0025-assets/RFC-DRAFT-palw-scalable-objective-court-v2.md|`
- `new link issue x1: missing-doc-file|docs/rfc/0026-assets/coverage.md|docs/rfc/0026-assets/cross-rfc-normative-scan.json|`
- `new link issue x1: missing-doc-file|docs/rfc/0026-assets/coverage.md|docs/rfc/0026-assets/inventory.json|`

## 4. 記録ファイル

[作成前JSON](docs-before.json) / [作成前log](docs-before.log) / [作成後JSON](docs-after.json) / [作成後log](docs-after.log) / [数式テストlog](examples.log) / [数式出力](examples-results.json) / [まとめJSON](validation-results.json)。

最終パッケージには適用前ZIP由来のファイルとのunified diffを含め、別の未変更ファイル集合に対して `git apply --check` と実適用の一致を確認する。その結果はパッケージ側のチェック結果に記載する。外部repositoryへのpushや実装branchへの統合ではない。

## 5. 未達gateを残す

RFCのAB01–AB16はnode-levelでは全て未実施。G-Panel / G-Entry / G-REAL / G-ConcentrationはUNKNOWN、G-CompatibilityとG-Courtは未達。10万の最低額と十分なPanelインセンティブ、実用的なclaim/REAL頻度が両立するパラメーター域はまだ測定していない。

責任単価・bounty・burn比率・料金表・支払成熟・境界等号・実inclusion前提・新旧資本readerはレビュー/実装を要する。Proposedの文書や16件の算術PASSによって、RFC-0026 F02/F03またはG14のgateを解除しない。


## 6. 2026-10-20 追記測定（§1–5は初版時点の履歴）

前節までの「未実施」は初版作成時点の記録。今回、旧node/外部検証の実測再集計、shadow、合成負荷を実行した。[追記測定レポート](measurement/measurement-report.md) / [実行結果](measurement/results/summary.json) / [解析単体試験log](measurement/tests.log)。

1,365再集計・保存則検査と32単体試験はPASS。過去の外部検証21 sample・node時系列17 run（2,563 claim）を集計した。新Account Bondのnative node実測、Panel実費・実支払、採用REAL/heartbeat、F02/F03全分岐は依然として未実施/未達。数値の参考結果をgateの完了へ昇格させない。今回の文書検査と再現性の結果は[追記検証JSON](measurement/validation-update.json)を参照する。
