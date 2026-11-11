# 普通TFP1のtokenizer資料予約 — 2026-11-11

コードcommit `970845a14`。この変更は[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)で維持したclaim資料・同一性照合の接続であり、HFの取得可能性認定ではない。モデル全量配信・Nodeによるモデル可用性samplingを新たな必須条件にしない。

## 実装した範囲

受理済みnative tokenizer登録から普通TFP1のbranchを導き、元claim・Source・prompt形式・登録IDを固定する。登録が候補より前のDAA/blue scoreにあること、受理時の再導出を要求する。単なるEOF宣言は登録の代わりにならない。

入力ID列と、生成順に並ぶrendering/EOG openingの2種類の資料上限を既存の数値・FullIds予約へ追加した。FullIdsは一度だけ予約する。新projectionのresource/profile/scopeは新domainを使用し、旧contextの意味を変更しない。

FullIds Finishより後の別phaseでsemantic Finishを処理し、生成IDとの照合が済む前に不正な順序の資料をimmutableな完成資料へ昇格させない構造とした。各semantic Finishにtokenizer照合のwork/IO全枠を保守的に加算し、その後のVerifyとterminal再検査も別に計上する。元のdraft期限、保持量、回復枠、実bookの容量不足による原子的拒否を維持する。追加依存辺のallocationは、個別加算だけでなく既存`expansion_workspace_v1`の全体edge予算も含めて評価する。

typed照合helperはnative prompt commitment、生成ID順、rendering、EOGを検査する。入力IDのhash一致は原文の再tokenizeを実施した証拠ではない。原文からの再実行はPanel側に引き続き必要である。

## 検証

最終source 7本のhashはビルド開始前後で一致。既存shared target、core opt-level 0、incremental無効、`-j2`でビルドした。新しいnative fixture 1件、既存Batch 28件、登録境界1件、受付境界1件、合計31件が成功した。

- [対象fixtureとbuildログ](tests.log)、[Batch回帰](batch-regressions.log)、[登録境界](registration.log)、[受付境界](intake.log)
- [source SHA-256](sources.sha256)、[binary SHA-256](binary.sha256)、[機械可読記録](summary.json)
- [最初のfixture lifetimeコンパイル失敗](initial-build-failure.log)、修正後の[r2結果](r2-test.log)と[r2 source](r2-sources.sha256)。r3は完了順序と追加費用・TTL検査を加えた最終版。

対象fixtureでは未登録・同時刻登録・別Source/ID/rendering/EOG・入力ID不一致を拒否し、再quoteの時刻変更でimmutable branch IDが変わらないことを確認した。予約ではFullIds共有、FullIds→semantic Finish→Verify→terminalの厳密な順序、追加費用、旧Finishだけなら収まる短いdraft期限の拒否、容量不足時のbook不変を確認した。数値資料の範囲はfixtureであり、実Studio Source全体のquoteではない。

文書検査3本は既存問題により非zero終了した。[比較記録](docs-validation.json)のとおり参照不備973件のcounter digestは直前版と一致し、RFC/ADR integrityの出力も直前と同一。[docsログ](docs-check.log)、[RFCログ](rfc-integrity.log)、[ADRログ](adr-integrity.log)を保存し、baselineは更新していない。文書検査全体の成功とは表示しない。

## 次の接続境界

新しい資料には新context専用のOpen/Respondとcaseに束縛された保存が必要。旧BCT2 tags 0/1や旧`MaterialTargetV1`の意味を再利用しない。既存のrange/fragment形状は共有できるが、署名対象・range ID・完了行・保護caseのnamespaceは新しく定義する。

native Finish writerは受理済みFullIdsとtyped照合helperで認証してからimmutable資料を保存する必要がある。今回のplannerだけでは、そのwriterを実装したことにならない。未認証のchecksum一致をclaim全体のfirst-wins完了へ書き込まない。

literal carrierのfee/署名/実workとRAM上限、bounded undo・snapshot・reorg・GC・repair、新contextからの全Selection/claim資料未着時の回復・終結、保護枠、TFO3のatomic受理、公開profile設定は未完。今回の数値下限は実機CPU calibrationや完全wire上限ではない。公開presetを変更していない。

公開T12への照会・登録・送信・配備はこの記録では行っていない。同一候補のquote成功、受付、Final、当選時receipt、main pushの証跡にはならない。モデルの取得不能証明・再試行裁定は追加しない。burn停止のnative書込み・受付/Final/reward gateは別の未完工程である。
