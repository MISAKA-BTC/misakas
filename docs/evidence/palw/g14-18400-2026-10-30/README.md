# DAA 18,400 設定変更の小規模検証

`record-rebind-and-kit-guard.json` は、18400 record の再現性・過去 source/digest/waiver kind の保存、および実 deploy kit の `require_release` が未設定の `EXPECT_FP` を拒否する 2 試験の入力 hash と結果を記録する。

これは Rust の起動・境界試験、最新候補の全試験、新しい fence、公開配備の成功証拠ではない。それらは別途、同一候補の source manifest と実行結果を追加する。旧 `arming-runs-17400` は変更しない。
