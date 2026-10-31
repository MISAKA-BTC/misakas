# A4 — 最終共通Court候補の検証

2026-11-01 JST。source: `c1292bc085cbfc1ca24a8ac0a962ab1096a88f23`。本番有効化は変更していない。

| 対象 | PASS | FAIL |
| --- | ---: | ---: |
| A0共通契約・record・rows・allocation前検査 | 42 | 0 |
| actual A5 private会計・readonly receipt | 33 | 0 |
| actual A1とA4結合・body/funding/record・復旧 | 93 | 0 |
| core coordinator・原子的受付・private recovery | 55 | 0 |
| 旧G14 shared | 27 | 0 |
| dormant fence | 8 | 0 |
| A2実G14 geometry・有限source台帳 | 30 | 0 |
| lifecycle allocation前検査 | 3 | 0 |
| 登録metadata読取 | 5 | 0 |
| native provenanceの構造・資源反例 | 4 | 0 |

**同一sourceで計300件成功、失敗0。** `cargo check -p kaspa-consensus --tests`も成功した。文書検査は新規欠落0、既知43。core/Nodeから同じ公開APIを試すdriverも含むため、300件がすべて独立した安全性主張ではない。過去の候補の件数は加算していない。

[原ログ](../../../evidence/positive-court-a4-integration-2026-10-31/final-r5/ci.log)、[command・commit/tree・118変更sourceのSHA256](../../../evidence/positive-court-a4-integration-2026-10-31/final-r5/validation.json)、[独立監査の範囲](../../../evidence/positive-court-a4-integration-2026-10-31/final-r5/independent-review.json)を保存した。再実行は`scripts/ci/positive-court-a4-ci.sh`。通常CargoのRust source buildであり、任意再使用したのは既存互換native RocksDB archiveだけである。

## 確認した境界

- 未認可候補・異なるopaque context・資源不足ではopen前後の候補/attempt/clockを変えない。
- 既受理packetおよび未受付future/order/headerのコピーでCase予算を消費しない。認可と署名は省略せず、正順に来た同じpacketは受理できる。現在必要なrelationの署名済み不正算術は費用を保持する。
- sourceのlogical request/unit relayは旧row CASを省略せず、Case usage・history・feeを増やさない。資料完了と支払完了を分離する。
- ResultAdmissionのpeer basisと独立quoteを完全照合し、borrowed recordを候補変更前に検査する。
- actual Full validatorからのprivate owned provenanceを通常UTXO適用の一度だけの経路に接続する。未設定RAM予約を成功扱いせず、Full前にtyped resource faultを返す。構造反例はFull-active native受付の成功試験ではない。
- 既存caseのreceiptをreadonlyで再発行し、新hold/epoch/journal/undoを作らない。actual A5 private bookとactual A1 factでfresh coordinatorへの再bindを確認した。新受付期限を既存のearned義務へ適用しない。

## 残る接続

actual176 one-writer、実UTXO lock、native完全case journalの再生、native具体readonly store、T12・DB reopen・reorg・IBDは未完了である。A0は新job授権のRFC/ADRを未採択としており、旧header/jobの無断流用や仮の成功で埋めない。global176 live登録とKindFence ALLも保留、公開presetはNoneのままである。

`488317d3e`の284件と`0c7c2bcf3`の295件は、それぞれ[native-ports-r3](../../../evidence/positive-court-a4-integration-2026-10-31/native-ports-r3/validation.json)、[recovery-r4](../../../evidence/positive-court-a4-integration-2026-10-31/recovery-r4/validation.json)に別候補として保存した。初回198件・271件の記録も履歴として保持する。
