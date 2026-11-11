# 同一資料caseのSelection継続予約

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

目標はStudioから提出した同一claimの受付・認証・Finalを通し、当選時の権利消費とブロック生成を確認すること。今回の変更は、その前提となる資料caseの継続権を予約計算に反映するもの。公開受付や有料case受理の成功は主張しない。

## 前回の接続方針の訂正

[元計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)I3と[RFC18](../../../rfc-v2/0018-interactive-negative-court-and-defaults.md)は、初回MaterialRequestをnotice窓内に受理すれば、資料到着後の検査・局所化・反証を同じCaseId・scope・原時計でhard内まで継続することを要求している。

したがって、[前回記録](../selection-native-notice-2026-11-11/README.md)の「Selection行も常に元notice期限前に固定する」という接続上の見立てを訂正する。初回MismatchNoticeと、受理済みMaterialRequestの後続行指定は別の操作である。後者を新規notice扱いして締め切ったり、資料を取得する前に行指定を強制したりしない。期限後の新規case作成は許可しない。これは採択済み設計への実装修正であり、期限を延長する設計変更ではない。

## 予約実装

- `SelectionContinuationBudgetV3`に、後続行指定が可能になる相対時刻、署名行指定のservice cost、Bind/Open/待機/Begin/Part/Finishの各幅を固定。全値をSelection branch identityへ含める。
- 数値・FullIds・tokenizerの既存搬送と分け、SelectionのBindと搬送を後段に配置する。全供給者のSelection Openは同じcaseのBindを因果親とし、その後にBegin/Part/Finish、Verify、既存の共通Terminalが続く。
- Bindの内側署名wire、保持量・保持期間・rent、追加packetと因果辺、後続packet一覧のメモリを既存の共通予約へ加算。外側carrierと完全native receiptの費用測定は残件であり、内側wire下限を本番価格とはしない。
- unsigned quoteとsigned intakeが共有する予約確定前検査で、Bindの最早枠を`localized_by`より後に置く。後続の証拠搬送と署名証拠を受け付けるVerifyは`filed_by`まで、Terminalは`terminal_by - recovery`まで。Verify自体がcanonicalな証拠提出を兼ねるため、その期限を終端まで延ばさない。元資料取得の`material_ready_by`や全体検査の`inspection_by`へ後続証拠搬送を誤分類しない。
- 実際の早い割当だけでなく、修復時に使えるpacketの最遅枠も検査する。後続搬送のdraft TTLも別区間で評価し、失敗は全保護rankを確定する前に返す。

関連ソース:

- [共通compilerと後続搬送window](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/admission.rs)
- [Selection policyとidentity](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/admission/plain_selection.rs)
- [元scheduleとの確定前照合](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/admission/protected.rs)
- [native projection回帰コード](../../../../consensus/core/src/palw_court_shared_material_intake_v1/batch/plain_output/projection.rs)
- [署名前後の保護枠回帰コード](../../../../consensus/core/src/palw_positive_contract_v2/tfp1_original/preclaim/tests.rs)

回帰コードは、供給者ごとの全OpenがBindの後になること、時刻変更でidentityが変わること、localizationが予約より遅い場合、filing区間不足、署名wire予算不足、拒否時に兄弟rankの予約を残さないことを追加した。**実行は未実施**。既存の全量モデル再配布やHF取得可否認定を復活させていない。

## 初回要求の実行時点

[PreparedDrawnPublicManifestCaseV1](../../../../consensus/core/src/palw_positive_contract_v2.rs)へ`storage_row_at_v1`を追加した。実行中のblock/DAA/blue scoreで新規要求と既存重複行を検査し、親の`last_point`を先に書き換える必要をなくす。original admission/drawは既存prefixにrootedであることを引き続き要求し、旧wrapperと旧writerの動作を維持する。

[native writer回帰コード](../../../../consensus/core/src/palw_state_v2/tests/native_writers_v1.rs)には、正しい実行点、個別のblock/DAA/blue改変、同じprefixへの二度目の照会を追加した。まだproductionの有料受付へ呼出接続していない。このAPI単独で実費・予約枠・canonical受理を認定しない。

## 残る接続

初回MaterialRequestの期限前物理予約・実UTXO費用・canonical受理、同じrank/caseへのfill-once保存とrollback、必要資料の成立を検査する後続Bind writerが必要。予約があるだけで資料取得や検査完了を認定しない。実writerは期限内受理済みの元requestを再読込し、別CaseIdや別rankへの切替、二重消費を拒否する。

完全TFO3 coordinator、応答とterminal/回復・archive、Panel承認・Finalと全consumer、公開profileの検証・有効化は未完。公開T12の最後の保存観測は[DAA21239のquote不成立](../selection-native-notice-2026-11-11/README.md)であり、今回は公開照会・送信・配備を行っていない。Studio受付・当選時権利消費・生成ブロックの証拠はまだない。
