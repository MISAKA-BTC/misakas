# RFC-0027 / ADR-0193 — 2026-10-20 追記測定パッケージ

**Proposed / Review Required。合意コード・パラメーター・fenceの変更、有効化はありません。**

## 内容と結論

[測定レポート](docs/rfc/0027-assets/measurement/measurement-report.md)を新設し、[RFC-0027 §18](docs/rfc/0027-miner-account-bond-and-panel-incentives.md)と[ADR-0193 §8](docs/adr/0193-miner-account-bond-for-panel-incentives.md)へ結果と判断を追記しました。

既存node実測17 run / 2,563 claim、外部検証21 sampleを再集計し、新たに1,320条件の受付列shadowと326条件の合成資本実験を実行しました。**新Account Bondを実装したnative nodeの実測ではありません。** 入力に新実装・元の大型artifactがなく、実行環境にRust toolchainもありません。Panelの実際の請求額・支払・参加率、採用REALとheartbeat依存の新規則下の実測は未実施です。

前回の説明値12,000 MSK/claimでは、Account10万に8件しか入りません。全責任の適法終了を仮に10,000 DAA後に置く条件では、Accountだけの継続上限は0.576件/720 DAA。30件/720 DAAを目標にすると同時417責任で、Accountだけで5,004,000 MSKが必要です。最低10万で同じ目標を維持するには、全jobを含む1件責任が239.80815347 MSK以下という条件付き逆算になります。これを料金承認には使いません。

10,000 DAAは自動返還期限ではありません。後続・未解決責任は保持し、releaseなしの試験では最初の8件以降停止しました。720 DAAは120秒slotの名目1日換算であり、実時計の測定ではありません。

外部検証のCPU時間は正常3 positionsで中央値141.85秒、late違反32 positionsで628.31秒（後者n=1）。実費単価がないためMSK換算は明示仮定による感度のみです。1 jobの運用費を含む参考責任396.63 MSKは上記239.81を超えますが、これも全job・同profileの完全責任額ではありません。4 sampleは証拠のwire上限で拒否され、成功報酬適格にはしません。

最低10万と責任連動追加の構造を維持しつつ、「10万で十分」「1件12,000が実用料金」「heartbeat増減を実測済み」とは記載していません。経済・node・F02/F03 gateの有効化保留を維持します。

## 適用方法 — どちらか一つだけ

このZIPは元repository全体ではなく、**初版提案を含む更新済み文書ファイル一式**です。合計50ファイル（元ZIPからの追加41、更新9）、前版からの追加・更新は31ファイルです。既存raw測定ファイルは変更・再収録せず、元ZIP内のものを参照します。

元の `misakas-pre(4).zip` に初版文書をまだ適用していない場合:

```sh
git apply --check /path/to/RFC-0027_ADR-0193.with-measurements.from-original.patch
git apply /path/to/RFC-0027_ADR-0193.with-measurements.from-original.patch
```

初版 `RFC-0027_ADR-0193_docs-proposal.zip` を適用済みの場合:

```sh
git apply --check /path/to/RFC-0027_ADR-0193.measurements-only.from-v1.patch
git apply /path/to/RFC-0027_ADR-0193.measurements-only.from-v1.patch
```

両patchを続けて適用しないでください。代わりに本ZIPの `docs/` を同じ相対パスへコピーすることもできます。共有branchで修正がある場合は上書きせず差分レビューを行ってください。索引・状態の再生成を含みますが、remoteへのpushは行っていません。RFC/ADR番号は前版のローカル仮割当を引き継ぎます。

## 再現

元ZIPの生ログを利用します。repository rootからPython標準ライブラリのみで解析を再実行できます。

```sh
python3 docs/rfc/0027-assets/measurement/measure.py --repo . --out /tmp/rfc0027-measure
python3 docs/rfc/0027-assets/measurement/test_measure.py
python3 docs/rfc/0027-assets/reproduce_examples.py --json-out /tmp/rfc0027-examples.json
python3 scripts/drills/adr0176_stage1_measure.py --self-test
```

[計画・仮定](docs/rfc/0027-assets/measurement/plan.json)、[結果要約](docs/rfc/0027-assets/measurement/results/summary.json)、全CSV/JSON、テスト・実行log、[検証記録](docs/rfc/0027-assets/measurement/validation-update.json)を同梱します。元資料の相対リンクは、元repositoryへ適用後に参照できます。解析時間とPanel処理時間は別の値です。

## 検証結果

再集計・資金保存則1,365/1,365、解析単体試験32/32、前版会計例の再実行16/16、旧collector self-testはPASS。2回の解析で環境・実行時間を除いた11出力が一致しました。29元資料のSHA-256を元ZIPと照合し、変更なしを確認しました。

両patchはそれぞれの未変更baselineへ `git apply --check` と実適用を行い、対象全ファイルのバイト一致を確認しました。[適用検査](PATCH_APPLY_CHECK.json) / [manifest](MANIFEST.json)。いずれも文書・解析の検査であり、node合意の検証ではありません。

repository全体の文書検査は、入力時点からある6件のbaseline外欠損でFAILのままです（全欠損49、うちbaseline43）。今回増やした欠損は0。baselineを書き換えて問題を隠していません。旧版の未実施記録は履歴として残し、追記測定で上書きせず証拠範囲の更新を明示しました。
