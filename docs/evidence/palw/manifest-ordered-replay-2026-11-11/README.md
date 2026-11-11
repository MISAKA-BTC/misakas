# 資料要求の受理順と手数料の再演

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

[有料native writer](../paid-manifest-native-writer-2026-11-11/README.md)に、同じ実UTXO情報から事前判定と確定処理を別々に再演する処理を追加した。Studioの同一claimの受付・Final・当選時権利消費・対応blockという目標は未達成。production ingressへの接続完了ではない。

## 共通処理

[受理順マージ](../../../../consensus/core/src/palw_batch_court_v1/fold_order.rs)にOrdinary／Batch／Tokenizer／ManifestのV2を追加した。位置は前にあるordinary object数、同位置の三つのnative経路は実accepted transaction ordinal順とする。列挙前に全位置・ordinalの順序と重複を検査し、不正なら一件も処理しない。追加heapなしのslice cursorで処理し、旧V1は維持する。位置とordinalは実accepted streamから導出するcallerの責務であり、wireの指定値を受理しない。

[再演用seedと料金投影](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_carrier/replay.rs)は、検証済み取引のliteral要求・実手数料・先行控除・carrier block・包含点・ordinalを不変の所有物として共有する。parent rootも固定し、再演候補にもそのrootを保持する。各試行のfee台帳は独立所有で、試行を破棄してもseedは変わらない。消費済み台帳や別の再演候補を新seedへ変換できない。

[shared native writer](../../../../consensus/core/src/palw_kernel_route_fold_v1.rs)は同じseedから候補を作り、既存の署名・予約・二行保存を再検査する。料金投影まで成功した結果だけを返し、投影に失敗した場合もstate/journalを戻して候補台帳を破棄する。旧borrowed-ledger APIのfinishは失敗しないため、外部fee台帳に対する従来の原子性を維持する。

料金結果はactual fee、先行控除、今回charge、collectible feeを分ける。chargeは同じ要求のoriginal・packetから導出したpurposeへの実割当と照合する。reconciliationは合計だけでなく全recordの一致、親root・包含点、strict ordinal、txid重複、件数上限、加算overflowと保存則を検査する。別取引や別source blockへchargeを移して合計だけ合わせることを認めない。結果は読み取り用投影であり、それ自体を新たな支払や有料受理行としない。

## 静的監査による修正

再演候補をparent rootへ明示的に束縛し、別seedの候補から料金結果を作る経路を拒否した。また全状態rollback copyの見積もりを、通常設定でゼロを返す互換APIから、型に基づき明示計上するAPIへ変更した。一時leaseは試行終了時に解放する。全状態copyを本番の有界な局所undoへ置き換える残件は継続する。

## 回帰の範囲

- 三native経路とordinaryの混在、同位置順序、duplicate/逆順/範囲外/末尾を記述した。
- [seed・料金回帰](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/manifest_carrier/replay/tests.rs)は、別parent候補、消費済みseed化、purpose違い、原資・先行控除・source block改変、同額での取引差替え、重複txid、総額overflowを扱う。
- [既存の実署名native writer fixture](../../../../consensus/core/src/palw_state_v2/tests/native_writers_v1.rs)に、previewを破棄して同じseedからactualを再演し、state・charge・collectible結果を一致させるコードを追加した。同一prefixでの再実行はchargeゼロになり、初回chargeの投影と取り違えるとreconciliationを拒否する。V2 iterator経由でshared writerを呼ぶが、ordinaryは順序検査用markerであり、本番の複数carrierを含むRound/body通し試験ではない。

Rust build/testは[元計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い未実行。整形・差分・文書検査だけを実行する。

## 残る本番接続

実processorは現在も既存V1のBatch/Tokenizer受理を使う。PMR1の新V2 iterator・shared seed・料金照合をproduction accepted streamへ渡す入口は未接続である。TFO3の完全なoriginal coordinatorと、元のliteral original／draw／供給義務／料金policyをnative historyから復元する処理が先に必要となる。そこが揃うまで、新profileの受理を仮のpolicyやprepared rowの注入で開かない。

本番ではsource block/txid/ordinalをUTXO受理列から固定し、preview・observed・actualの全料金結果を照合してcollectible feeへ一回反映する。外側Round/body拒否時にはstate・fee候補を一緒に破棄する。後続paid case認証、Panel承認、Final、生成権の全consumerも未完。公開照会・配備・送信はなし。最後の保存済み公開観測は[DAA21239のquote不成立](../selection-native-notice-2026-11-11/README.md)であり、同一claimの公開受付・権利消費・対応blockは **NOT_PROVEN**。
