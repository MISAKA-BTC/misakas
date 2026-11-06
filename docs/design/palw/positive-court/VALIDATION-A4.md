# A4 — 共通契約接続の検証記録

2026-10-31。対象source commit: `d746fe4550a5dd1c2fabc1925c08a82d9879aea6`。実装APIは[A4接続記録](A4-CONTRACT-INTEGRATION.md)、body schemaは[176 body案](A4-NATIVE-BODY-R1.md)。

このsourceに対する後日のA0/A7レビューで、候補openの権限確認と既受理packet再送課金に不足を検出した。以下は当時の試験結果であり、同commit単独のnative統合承認ではない。修正後の結果は別source・証拠として記録する。

通常CargoでA4専用のfresh targetからRust sourceをビルドし、同じsource候補に対して下記すべてが成功した。旧外部Rust rlibを使う初回harnessの成功を合算していない。

| 検証 | PASS | FAIL |
| --- | ---: | ---: |
| A0共通契約・TIR・funding・P所見 | 30 | 0 |
| 実A5 private会計 | 29 | 0 |
| A1検査器/port・実A4 Court・受付・body/funding結合 | 62 | 0 |
| core coordinator・原子的受付・transport・A6 recovery | 50 | 0 |
| 旧G14 shared Court回帰 | 27 | 0 |

計198件。core/Nodeの両driverから同じ公開API試験を実行するものも含むため、198件すべてが独立のセキュリティ仮定・別シナリオという意味ではない。独立担当は別のscalar期待値と不正入力を用い、実A1 checker・正式A0 frame署名・production carrier・実A5 private storeでrole/根拠の境界を試験した。

[raw CI log](../../../evidence/positive-court-a4-integration-2026-10-31/ci.log)、[使用環境・source hash・command](../../../evidence/positive-court-a4-integration-2026-10-31/validation.json)、[A1取込時snapshot](../../../evidence/positive-court-a4-integration-2026-10-31/a1-import-snapshot.json) を保存した。再実行は `scripts/ci/positive-court-a4-ci.sh`。

native RocksDBの既存互換archiveだけを`ROCKSDB_LIB_DIR`で最終linkに使用し、Rustのcore/checker実装を外部成果物で代用していない。途中のdisk-fullと旧assertの失敗は診断用targetログに残し、成功回のsource/原ログを上記で識別する。

文書検査は新規参照欠落0、既知43。baselineは変更していない。raw log末尾の採取時空行は保持し、sourceの空白検査と分ける。

## 合格範囲と残るnative境界

未認証packetの無汚染、正式署名の流用拒否、全被覆・終端照合、signed bad candidateのRejectedと正直な予算不足のIncomplete、再送・試行・branch/replay、実store receiptとの結合、二重close/undo、受付期限とCourt期限の分離を確認した。bodyは0〜7を固定し、P所見は837＋4627 bytes、Rooted draw承認をcodecで生成しない。

live176受付・実UTXO lock・native raw view/role/全試行lease・A2差分/A5 money/A6 journalを一つのDB batchへ適用するwriter・実Node DB reopen/IBDの成功はこの結果に含めない。これらの根拠が未供給の入口は拒否し、成功stubで埋めていない。公開activation・旧claim・model二階層・Gate Cを変更していない。
