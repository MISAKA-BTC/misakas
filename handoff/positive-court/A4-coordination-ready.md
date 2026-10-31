A4のready source commitは d746fe4550a5dd1c2fabc1925c08a82d9879aea6（codex/positive-court-a4）です。最終検証証拠は後続commitへ分離します。最新native/funding/attestation helperを同commitに含むため、A0は必要なA4固有差分だけ選択できます。

176 bodyの正本は consensus/core/src/palw_positive_court_v2/coordinator/native_body.rs と docs/design/palw/positive-court/A4-NATIVE-BODY-R1.md。PositiveNativeBodyV2 / PositiveNativeOperationV2、scan_native_body_v2 / decode_native_body_v2 / encode_native_body_v2。local kind0..7はResultAdmission、WitnessCommit、ExactFrame、SourceRequest、SourceResponse、Finish、Expire、PositiveCourtAttestation。0..6を保持して7を追加済み。鍵はnative registration/bondから解決しbodyに含めません。内側record/G14 responseはbounded opaque Vec、配列署名は4627 bytes固定。Finish/Expireにpeer clock/Passedはありません。

Courtはcore::palw_positive_court_v2::coordinator::contract::ContractCourtV2<C,V>。CはNodeChecker/CourtCheckerV2、Node側production adapterはmisaka_palw_tir_exec::node::positive_court_adapter::TirCourtCheckerV2です。Vはcarrier::PanelSignatureVerifierV2<R>でA0 evidence署名を実使用し、Rooted author/duty/current scope/clockをRで必須解決します。signed-frame全4627 bytes overheadを予算へ追加。認証済みcheck failureのattempt/wire消費をnative journalでrollbackしないことが必要です。

原子的受付はadmission::AtomicAdmissionCoordinatorV2<S,V>。CourtAdmissionViewV2::{resolve,resolve_committed}でNEW受付と既存Case/anchor/Court期限を分離し、実AtomicCourtStoreを呼びます。Fundingはfunding::FundingPreflightCoordinatorV2::{check,revalidate,prepare_atomic,commit_atomic}でNode由来quoteとactual carrier output額をcommit直前にも再照合します。SignatureCheckedFundingPreflightV2だけではUTXO lockを作れません。AdmittedCheckedFactV2は実store receiptと実A1終端をscope/case/resultへ結合します。

未接続の具体根拠は、nativeのValidatedTransaction/composed UTXO/current accepted-prefixと同一DB batchを持つAtomicCourtStore、rooted payer/role/prover/draw/duty/currentview resolver、A2 source diff＋A5 money＋A6 journalを適用するstate_v2 one-writerです。private整数holdや4KiB木を代用せず、未割当portは拒否を維持します。SourceResponseは既存G143形式をnative request.kind/prompt formから選ぶ方針です。

同じsource commitに対する通常Cargoで、共通契約30、A5 private会計29、Node結合62、core Court/recovery50、旧G14 shared27の計198件がPASSしました。証拠は docs/design/palw/positive-court/VALIDATION-A4.md と docs/evidence/positive-court-a4-integration-2026-10-31/ に保存済みです。これはnative実UTXO/T12Chain成功ではありません。公開activation変更なし。
