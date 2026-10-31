# testnet-12 G14 DAA 18,400

2026-10-30 のユーザー指定で、未公開の旧 G14 候補を 17,400 から **18,400** へ移す。配備は未実行。

対象は `PALW_T12_G14_17000_FENCES_V1` の既存採択済みリスト。歴史的な識別子名は維持する。int-13 は 15,500 のまま、`FULL_ACTIVATION_FLAG_DAY_DAA` は空欄のまま。追加機能の有効化完了を意味しない。

新高の記録は `gen_t12_18400_records.py` が生成する。過去の source・suite digest と許可済み waiver は履歴として保持し、最新候補の試験成功に読み替えない。旧 `G14-17400-ROLLOUT.md` と過去 logs/JSON は履歴のため変更しない。

現在の `fleet.env.example` は `G14_FLAG_DAY_DAA=18400`。`EXPECT_FP`・バイナリ SHA・REV は `__FILL_ME__` であり、実際の `require_release` は拒否する。17,400 候補の fingerprint を新高用にコピーしない。

公開候補を作る際は、同一 source manifest のビルドと対象試験、18,399/18,400/18,401 の境界・startup validation を完了し、`t12_repin_values` と通常の `t12-repin.sh` harvest から identity を取得する。配備用の REV・SHA・EXPECT_FP はその候補から設定する。公開配備はこの作業で実行していない。

実装状況・追加機能の未充足条件は [18,400 台帳](../../docs/design/palw/integration/g14-17000/release-18400-2026-10-30.md) を参照する。旧 runbook の日時見積もり・旧候補の commit・旧 fingerprint は今回の配備値ではない。
