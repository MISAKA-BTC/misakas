# MAS1文書検証 — 2026-11-20

## 判定

**今回の文書変更について、原本保存・現行契約・入力版からの増分検査はPASS。リポジトリ全体の旧検査は、入力版由来の不整合が残るためPASSではない。**

この判定は文書の整合性と保存に限定する。モデル解析器、QwenのG-COST、BFT/native状態機械、LLM実機性能、公開有効化を検証したものではない。

## 原本と適用範囲

| 項目 | 結果 |
| --- | --- |
| 入力ZIPの既存ファイル | 2928件をすべて保持 |
| 入力と同じbytesのファイル | 2868件 |
| 更新した既存ファイル | 60件 |
| 更新前bytesの保存 | 60件すべて一致 |
| 変更した統合冊内の歴史payload | 20件すべて不変 |
| 新規の正本・設計契約 | 5件 |
| 現行統合冊数 | RFC20冊 / ADR50冊を維持 |
| 本番有効化・wire割当・native実装の完成宣言 | なし |

## 実施した検査

1. `python3 docs/maintenance/check_docs.py` を変更前と変更後に実行。新しいリンク問題の入力版比増分は0件。既存の欠落資料参照等は残した。
2. `python3 docs/rfc-v2/check_integrity.py` を変更前後に実行。旧sourceとの7件の不一致は入力から継承し、今回増加なし。
3. `python3 docs/adr-v2/check_integrity.py` を変更前後に実行。旧source/metadataの指摘は入力162件から改訂後159件。今回変更した現行編集領域のchecksum更新により3件が解消し、新規指摘は0件。元のsource digest・歴史payloadは書き換えていない。
4. `python3 docs/maintenance/revisions/2026-11-20-model-admission-safety/check_revision.py` はPASS。全入力ファイルの保存、変更宣言、before原本、歴史payload、20/50冊、論理JSON、担当入口を確認。これはnative実装のテストではない。

入力版では、文書checkerの旧known baselineを超える参照問題が930件あり、元baseline内の43件も残る。未同梱のevidenceやコードへの参照を架空ファイルで埋めず、known-link-issues.jsonを一括更新して成功に見せていない。

raw結果はvalidation/baseline/、validation/post/、差分はvalidation/input-relative-delta.json、原本保全はvalidation/revision-check.jsonへ保存した。外部URLの継続可用性、未知の計算短絡、実ネットワークの停止活性はこの検査の対象外。

## 設計レビュー

[DESIGN_REVIEW.md](DESIGN_REVIEW.md)の否定試験は実装前の仕様要件であり、実行済みのnative試験と表示しない。UNKNOWNの仮承認、少数・重複署名、burn解除、世代更新、staleな解除、個別モデルからrule全体の復帰、評価から報酬への転用を拒否する設計を確認した。

## 未完の根拠

具体的な適合TheoremRule、実用suiteと閾値、有界なkernel解析器、非報酬評価と停止controlの実装、実rosterとfault前提、資源・包含・再起動/reorg/IBDの試験、公開activationは別の完了gateである。BFT停止証明又はこの文書検査を、それらの代替にしない。
