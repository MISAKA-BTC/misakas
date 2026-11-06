# A6 — accepted order・receipt完全性・費用下限のレビュー追補

2026-10-31、branch `codex/rfc42-a6-recovery`。既存候補 `d2bdb174c` / evidence `c276f63a3` を保持し、受理順序を `f68246e64`、receipt/counterとRoute review登録を `c14e39353` へ分離した。

最終候補の対象scopeは **30 passed / 0 failed / 0 ignored**（元15、順序6、counter9）。標準test stack、core opt0/debug0/incremental0/jobs2で実行。中間候補f682の21成功は別ログであり、この30へ加算しない。旧native61/route25は再実行していない。

## 同じaccepting blockの局所順序

同じblockでは `(accepted_tx_ordinal, event_ordinal)` を辞書順に比較し、accepted DAA・blue scoreの一致も要求する。異なるblockのordinalや時計を単純比較しない。

| 比較 | 条件 |
| --- | --- |
| Result.origin → Witness.origin | 厳密に前 |
| Witness.origin → PacketReceipt.origin | 厳密に前 |
| ServiceReceipt.duty → response | 厳密に前 |
| Witness.origin → Coverage.last_event | 同一event可 |
| 保持PacketReceipt.origin → Coverage.last_event | 同一event可 |
| ServiceReceipt.response → 同receipt.settlement | 同一eventのatomic確定可 |

next=0のCoverageはWitnessと同eventでもよい。next>0では全保持packetを検査するため、最初のpacketだけが正順でも後続の逆順を隠せない。別blockのlineage、正規accepted位置・carrier/preimage認証はnative history backendに残す。

## literal receiptと使用量

PacketReceiptは成功した1必須relationに対応する。全first_relationがnext_relation未満、かつkeyの一意性とreceipt数==next_relationを要求し、gap・全消失・余分なrowを拒否する。BorshのLE key順を数順とみなさず、保持rowだけを走査する。u64MAX cursorでもcursor値の回数だけループしたりbitmapを確保しない。257 relationの正例ではLE順0,256,1,2…を実際に検査した。

Coverage.frame_bytesは署名を除くframe全体であり、`sum(PacketReceipt.signed_bytes - 4627)` 以上、signaturesは成功packet数以上とする。Caseのunique_source_bytesはdistinct ServiceReceiptのraw bytes和以上、additional_workはその準備work和以上、transmitted_bytesはraw bytes和以上とする。費用・和はchecked arithmeticで計算する。失敗・retryも費用がかかるので等号へ狭めない。同unit番号でも別sourceのreceiptは独立に加算する。正確なwire・schedule preparation charge・native fundingはpreimage/backendで再確認する。

Caseごとのservice-prefix、Witnessごとのpacket-prefixは互いに分離している。全体O(N log N)を維持し、decoded row mapやcursor-sized collectionを追加しない。reportには追加読込みを実計上する。receipt一致、署名、保存されたMatchedから新しいreplay terminal・payment・creditを作らない。

署名fixtureはmandatory10/Matched/full cursorを維持し、成功Packetを0〜9まで揃えた19row/10kindへ更新した。各packetのframe IDとaccepted位置を分け、初期Journalは別の5row admission-only fixtureで検証する。

## Route手書きcodecと隔離generator

PalwKernelRouteStateV1をmanual wire PINNED registryのowner adapterへ登録した。review hashは `e341c97e…`。header/rows/aux順、ledger_cache skip、aux151のallocation前上限とduplicate拒否を読んで確認した。

隔離したsource copyでgenerator全体を実行したが、NumericalManifestV1の既存PIN不一致で **exit 1**。現行body hash `9bc64e…` がA6以前の `a5800d2dd` と一致することを確認し、旧PIN `2f0c22…` は変更していない。実treeの生成Rustは不変であり、隔離側に生じた生成差分も持ち帰っていない。Route登録漏れは修正済みだが、全体roundtrip成功とは扱わない。

原ログ、source hash、再実行条件は [order/counter evidence](../../../evidence/positive-court-a6-order-counters-2026-10-31/) に保存する。標準stackでの旧native復元overflow、native履歴preimage/funding接続など別の境界は [前候補の記録](A6-AUX151-VALIDATION.md) に保持する。rawログの末尾空白・空行は整形せず、source/Markdownのwhitespace検査から除外する。
