# TFO3の再照合と有料資料caseの継続

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前ターンは方針確認だけでauthoritativeな実装変更がなく、no progressと分類する。今回は[元計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)の通し実装を継続した。Studioの同一claimの公開受付・Final・当選時の権利消費・対応blockは **NOT_PROVEN**。

## 今回の接続

[native link](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/native_link.rs)の`restore_tfp1_pairing_v1`は、current stateの204とnative frameからprofile/resource/obligationを導出し、元の実取引から取り出したcompanion署名とpromptを再照合する。元包含originを変更できず、再構成した全admission recordと198、literal FP/companionと204を照合する。新しいescrowの再作成は要求しない。これはnative history collectorそのものではなく、そのcollectorが得たliteral取引と元受付入力を照合する入口である。FP/payer署名・UTXO資金・受理履歴の再検証はcollectorと元受付plannerの責務として残る。

同じファイルのTFP1専用`rooted_drawn_batch_case_v1`を、199のpending行だけでは継続できないよう修正した。実drawから導出したphysical rankのpacketで205を読み、claim/Source/original、case、元要求のorigin、request digestの一致を確認する。別rankの支払記録や旧extrasだけのpending行は継続権限にならない。restoreは二重chargeをしない。一般の旧`PreparedPublicOriginalV1`の読み取りや、pending caseが成功を保留する意味は変更しない。

[fixture](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original.rs)は、[実fee割当・二行writer](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_request.rs)が返した状態をSelection認証とsnapshot復元へ渡すよう変更した。別取引、包含ordinal、初期供給量、未支払pending、別rankの支払行を拒否する回帰コードを追加した。元companionとdrawを再構成し、同じ資料caseへ戻す経路も記述した。companion/request/Selection署名は実署名だが、FP/payer署名・funded frame・draw・予約価格はfixtureであり公開受付の証拠ではない。

## 履歴接続の具体的な残件

独立diffレビューでは、旧profileの継続経路を変更していないことと、明白な型・所有権問題がないことを静的確認した。供給量改変の回帰は、通常prepareに通る値でも保存行の再照合で拒否するよう修正した。なお198は`material_request_ready_by`を直接保持せず、同じ締切になる別入力を区別できない。元planner/resource preimageの再導出は必須であり、新restore入口だけで歴史的入力の真正性が成立したとは扱わない。要求digestも署名対象の同一性であり、PMR1のliteral bytesとtxid/originの一致はhistory collectorで確認する。

独立担当が[既存archive reader](../../../../consensus/src/pipeline/virtual_processor/processor/positive_assignment_archive.rs)を静的調査した。`load_positive_accepted_carriage_v2`は選択鎖、acceptance ordinal、actual carrier、元UTXO入力とFull fee、親状態とdelta prefixを有界に照合する。ただし`NativeArchiveBoundaryV2`はAssignment用であり、そのままTFO3の権限として使用できない。

1. TFO3受理の同じcheckpointで、実行直前のstate root/journal位置、selected parent、実取引・UTXO入力の照合値とfeeを保存する。Frame/book/escrow/198/204と部分確定させない。
2. 受理時のfenceに従うouter payload復元とTFO3専用boundaryを接続し、上記bounded readerを再利用する。current tipやcaller DTOで欠けた元prefixを代替しない。
3. 受付時の完全profile/plannerからmanifest scope rules、readiness、usage、供給義務と全保護枠を再導出する。198の期限・予算と204のresourceは照合値であり、その入力一式を保存していない。
4. 201のdraw行はmember digestを持つため、実際の選出一覧・beacon/規則を認証済みhistoryから供給する。旧Assignment archiveや任意の一覧を転用しない。
5. pruning後も必要な期間は、この履歴の有限な保持・費用を受付前に予約する。完全TFO3 coordinator、PMR production dispatch/collectible fee、応答・裁定・Finalからreceipt spend/blockまでの接続を続ける。

Minerのモデル全量配信やHF取得可否認定は追加しない。今回の資料はclaim固有資料と局所openingである。公開probe・配備・送信は実行していない。Rust検証は元計画どおり通し接続後へ留保し、整形・差分・文書検査を実施する。
