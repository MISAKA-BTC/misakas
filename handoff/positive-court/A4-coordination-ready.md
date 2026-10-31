最新: P1/P2修正ee06d8173＋テスト更新d2047d64dを同一sourceで通常Cargo検証し271 PASS/0 FAIL。VALIDATION-A4-R2.mdとreview-r2/証拠を参照。以下のd746記述は初回履歴です。native actual writerはA0の新notice/role/profileとA2追加CASの統合を続行中です。

現在状況: d746fe455は初回検証候補であり、A0/A7のP1（open前認可）・P2（既受理packet再送）レビュー修正を適用するまで統合ready扱いを保留します。198件成功はその候補の履歴として保存済みです。

A4のready source commitは d746fe4550a5dd1c2fabc1925c08a82d9879aea6（codex/positive-court-a4）です。最終検証証拠は後続commitへ分離します。最新native/funding/attestation helperを同commitに含むため、A0は必要なA4固有差分だけ選択できます。

176 bodyの正本は consensus/core/src/palw_positive_court_v2/coordinator/native_body.rs と docs/design/palw/positive-court/A4-NATIVE-BODY-R1.md。PositiveNativeBodyV2 / PositiveNativeOperationV2、scan_native_body_v2 / decode_native_body_v2 / encode_native_body_v2。local kind0..7はResultAdmission、WitnessCommit、ExactFrame、SourceRequest、SourceResponse、Finish、Expire、PositiveCourtAttestation。0..6を保持して7を追加済み。鍵はnative registration/bondから解決しbodyに含めません。内側record/G14 responseはbounded opaque Vec、配列署名は4627 bytes固定。Finish/Expireにpeer clock/Passedはありません。

Courtはcore::palw_positive_court_v2::coordinator::contract::ContractCourtV2<C,V>。CはNodeChecker/CourtCheckerV2、Node側production adapterはmisaka_palw_tir_exec::node::positive_court_adapter::TirCourtCheckerV2です。Vはcarrier::PanelSignatureVerifierV2<R>でA0 evidence署名を実使用し、Rooted author/duty/current scope/clockをRで必須解決します。signed-frame全4627 bytes overheadを予算へ追加。認証済みcheck failureのattempt/wire消費をnative journalでrollbackしないことが必要です。

原子的受付はadmission::AtomicAdmissionCoordinatorV2<S,V>。CourtAdmissionViewV2::{resolve,resolve_committed}でNEW受付と既存Case/anchor/Court期限を分離し、実AtomicCourtStoreを呼びます。Fundingはfunding::FundingPreflightCoordinatorV2::{check,revalidate,prepare_atomic,commit_atomic}でNode由来quoteとactual carrier output額をcommit直前にも再照合します。SignatureCheckedFundingPreflightV2だけではUTXO lockを作れません。AdmittedCheckedFactV2は実store receiptと実A1終端をscope/case/resultへ結合します。

未接続の具体根拠は、nativeのValidatedTransaction/composed UTXO/current accepted-prefixと同一DB batchを持つAtomicCourtStore、rooted payer/role/prover/draw/duty/currentview resolver、A2 source diff＋A5 money＋A6 journalを適用するstate_v2 one-writerです。private整数holdや4KiB木を代用せず、未割当portは拒否を維持します。SourceResponseは既存G143形式をnative request.kind/prompt formから選ぶ方針です。

同じsource commitに対する通常Cargoで、共通契約30、A5 private会計29、Node結合62、core Court/recovery50、旧G14 shared27の計198件がPASSしました。証拠は docs/design/palw/positive-court/VALIDATION-A4.md と docs/evidence/positive-court-a4-integration-2026-10-31/ に保存済みです。これはnative実UTXO/T12Chain成功ではありません。公開activation変更なし。

後続取込: A0 rows/record b1c03b92e、dormant fence aa9dd6d66/e1a788dac/a874827f3、A1 purecore/A2 source選択取込63fbb6d66、body funding/record preflight/core薄adapter27016a9ef。新funding helperはpeer basis全fieldを独立quoteへ照合、record helperは外版本/expected Resultとのborrowed codec照合を候補変更前に行います。A1実検査器はcore一元実装で、exec旧pathは同一型reexportです。

実native resolverの調査結果: class/artifact/program/tokenizer/layoutは既存class登録から独立解決できます。一方、新176 Resultと既存registered job/input/context/seed/expected Minerの正規対応、新PositiveのProver/Source claim-duty割当、adopted service/funding schedule preimagesが未供給です。旧FP claim/job、kernel interim Panel、source bootstrap checker cohort、Gate C事後drawを新役割へ代入しません。A0へcanonical型/APIの確定を依頼済みで、欠落は拒否を維持します。

保存境界: native aux151のCourt/Source/Funding/JournalHeadは既存KernelRouteRow(old/new) deltaとtypedstoreからper-block UTXOdiff/acceptanceと同じcaller WriteBatchへ入れる方針です。実virtual UTXO setとtip materializationは別の既存replay段階であり、一つのbatchで全tipが同時更新されるとは称しません。A6 private archiveはlocal cacheで、raw RelatedRowへUTXO keyを流してtypedstore/cacheを迂回しません。
