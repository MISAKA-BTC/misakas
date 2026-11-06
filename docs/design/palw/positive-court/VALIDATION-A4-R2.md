# A4 — 独立レビュー修正後の検証

2026-10-31。sourceは `d2047d64d0db68dee44c92a4a8c8025eedf0d458`、P1/P2実装修正は `ee06d8173`。初回198件の結果とは別の候補である。

| 対象 | PASS | FAIL |
| --- | ---: | ---: |
| A0共通契約・record・rows | 41 | 0 |
| A5実private会計 | 29 | 0 |
| A1検査器・同一型port・A4実結合・body/funding/record | 89 | 0 |
| core coordinator・受付・transport・private recovery | 53 | 0 |
| 旧G14 shared | 27 | 0 |
| dormant fence | 8 | 0 |
| A2実G14 geometry・有限source台帳 | 24 | 0 |

計271件。重複する公開API driverを含み、271個の独立した安全性主張ではない。通常CargoでRust sourceをビルドした。native RocksDB archiveだけを既存互換成果物からlinkし、外部consensus Rust rlibを使っていない。

[原ログ](../../../evidence/positive-court-a4-integration-2026-10-31/review-r2/ci.log)と[実行command・commit/tree・変更source SHA256](../../../evidence/positive-court-a4-integration-2026-10-31/review-r2/validation.json)を保存した。文書検査は新規欠落0、既知43。初回の再実行では移動前テストの旧`encode_payload`参照がcompileに失敗し、A1の正本テストとA0 `ExactPart`へ更新した後に全対象を再実行した。途中ログは診断用targetに保持する。

この後のA7レビューで未受付future frameの反復課金を検出し、`488317d3e`で受付順序確認を課金前へ移した。この271件のsourceだけをその追加問題の修正証拠とはしない。

## 修正した境界

- 候補openにrequired rooted prover認可を追加し、自己署名だけでattemptや既存候補を変更できなくした。opaque context全体・plan・資源確保・checker開始が成功するまで時計を含むCourt状態を変更しない。Gate Cの事後drawをopen条件へ流用しない。
- 同じ署名済みWitnessでもlimitsなどが異なるcontextを冪等ACKしない。A1のprivate identityを比較するだけで別検査器を実装していない。
- current Witnessの受理済みframe IDを64×関係数の有限heap予約で保持する。rooted認可・正式packet署名を再確認してから既知packetを課金前にACKし、古いprefixの再送もwire・work・attempt・coverageを増やさない。
- 別の署名済み不正packetは費用を保持する。非canonical count、素材不正、無認証署名を区別し、Miner有罪を合成しない。終了時はreceipt capacityを解放する。
- ResultAdmissionのpeer basis全fieldと独立quoteを照合し、borrowed実行記録の外版本・操作・expected Resultを候補変更前に照合する。

## まだ合格に含まない範囲

live176 one-writer、実UTXO lock、native role/schedule/lease、完全case履歴の実DB reopen・IBDはこの結果に含まない。A0はcanonical aux151 deltaと既存per-block UTXOdiff callerbatchへの統合を採用した。job/origin循環を避ける新light post-PoW noticeと有限role/profile preimageはA0側で確定中である。旧job・interim Panel・source checkerを新役割に代入しない。公開presetは変更していない。

A2の追加修正 `3d764a0c1`（冪等sourceイベントでもactual旧union/receiptを完全CAS）は、このsourceの後続取込対象であり、この271件の成功へ含めていない。
