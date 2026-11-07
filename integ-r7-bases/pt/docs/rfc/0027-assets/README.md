# RFC-0027 補助資料

[提案RFC](../0027-miner-account-bond-and-panel-incentives.md) / [提案ADR](../../adr/0193-miner-account-bond-for-panel-incentives.md)。すべて文書レビュー用であり、有効化資料の完成を意味しない。

- [source-review](source-review.md): 添付ZIPの根拠、行範囲、未同梱ADR-0192との境界。
- [source-manifest](source-manifest.json): 入力ZIP・参照ファイルのSHA-256。
- [economic-examples](economic-examples.md): 最低額内包、費用償還、自己還流、資本感度の合成例。
- [reproduce_examples.py](reproduce_examples.py) / [examples-results.json](examples-results.json): オフライン整数会計の16検算。
- [validation](validation.md) / [結果JSON](validation-results.json): 文書検査、既存欠損、試験の未実施範囲。

## 2026-10-20 追記測定

[測定レポート](measurement/measurement-report.md) / [計画・仮定](measurement/plan.json) / [解析コード](measurement/measure.py) / [単体試験](measurement/test_measure.py) / [結果要約](measurement/results/summary.json)。旧実測の再集計と追加shadow/合成実験であり、新Account Bondのnode実測完了ではない。
