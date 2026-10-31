# A4 — 共通契約への接続と検証

2026-10-31。branch `codex/positive-court-a4`。初回実装の履歴は[A4-HANDOFF](A4-HANDOFF.md)。本記録がその後のA4状態を示す。

## 基準と実装境界

共通契約はA0 `a85152edc`、TIR annex `d615f62b6`、Panel evidence認証 `5901eff3f`、native/funding `42a60b840`、P所見 `069d54007`。A1の確定checker `40cbcb559`に、A1作業treeのport/hardeningをsource snapshotとして取り込み、A0共有のderivation/selector/provenance/16-byte payloadへ接続した。旧prototypeのID・署名domainを新DTOへaliasしていない。

A5 `56ef2750a`、A6 `32287dc73` / `30cfbdeda`を選択統合した。shared module/dependency登録はこの統合branchの差分として明示する。公開wire tag・native row・高さ・preset・モデル係数・Gate Cは変更しない。

## 接続した処理

- `coordinator/contract.rs`: A0 Result/Witness/FrameとNodeCheckerを結ぶCourt。署名、scope、登録context、case/result/witness/plan、現在時刻、全試行の最悪wire/workを照合する。frameはcarrier認証後だけcheckerへ渡す。自己申告cursorや保存済みPassedを受け取らない。
- `coordinator/carrier.rs`: A0の`EvidenceFrame::authenticate`を使うproduction verifier。rooted author/duty/current scope/clockの解決は必須portで、未割当実装は拒否する。Witness署名をpacket署名へ転用しない。同じPanel署名済packetは別relayから再送できる。
- `node/positive_court_adapter.rs`: 実A1 `TirNodeChecker`をCourtへ接続する。contextと終端のwitness/result/planを照合し、別witnessの検査結果を付け替えられない。計算一致だけで信用を生成するAPIはない。
- `coordinator/admission.rs`: A0 AtomicCourtStoreへのprepare/commit/transition/undo。Node由来context/intentとMiner署名を照合し、commitでbranch/generation/clock/義務/資金条件を再解決する。独自会計は作らず、実storeのopaque receiptだけを返す。
- `AdmittedCheckedFactV2`: 実受付receiptと実checker終端をscope/case/resultへ結び付けるopaque型。bind時は現在の受付済みCase/anchor/独立Court期限を必須`resolve_committed`で解決し、世代・時刻の非減少を要求。新規funding受付期限を既存Courtの期限へ流用しない。後続block/期限後の正当なcloseでは同scope・世代/時計の非減少・再解決一致を要求し、実storeがcurrent row/transition epochを検証する。reorg/ABAの古いtransitionを有効化しない。
- `coordinator/funding.rs`: A0の循環しないFundingScope、実Miner/payer署名、最終Result/Service/consentのリンク、rooted quote/料金表、source料金とCourt料金とcapital、実carrier由来output額の上下限を前検査する。opaque joint prepare/commitは直前にroot/quote/output/intentを再検査して実storeへ渡す。前検査をUTXO lockと呼ばない。
- `coordinator/native_body.rs`: A0予約176へ渡すv2/r1の8操作body案。allocation前scan、未知kind/余剰/巨大Vec/署名数/Exact関係数を拒否する。layoutと未接続境界は[A4 body正本](A4-NATIVE-BODY-R1.md)。live decoder/row writer/fenceをこのcodecだけで開かない。
- 既存private coordinatorのA6 journal: tag 0〜6/schema 1を保ち、7=搬送設定、8=fragment、9=buffer破棄を追記。旧readerは未知tagを拒否する。追加予約・部分buffer・重複/失敗packet・wire/packet上限をwrite-before-applyで保存し再生する。journal外で設定済みの初期objectは拒否する。

完全に認証された不正Panel資料は、実A1 opaque進捗のtyped outcomeに従いWitnessRejectedとする。資源不足・未裁定はIncompleteに分け、両者ともMiner反証へ昇格しない。ClaimRefutedは入力から全範囲を検査した終端と署名済み結果の不一致に限る。Court成功は既存source義務を解除する操作ではない。共通CourtにはA2 dutyを削除/完了扱いにする経路を設けていない。

## 資源と認証

A1のframe capは外284＋内16 bytesを含む。A4は全relation×frame cap×全試行に加え、署名4627 bytes/packetを最悪予約し、既に受理した同一canonical frameのrelay再送は課金しない。新しい署名済み不正packetの消費は保持する。ingressが公開鍵本体を毎packet搬送するなら2592 bytesも別途必要である。現在のprocess authorizationはrooted key解決を前提とする。native transactionのheader/包含/保存/undo費用はこのprivate packetサイズに含まない。

同時に保持するouter frame、bounded accepted-frame receipt index、A1 inner heap/interpreterのpeakはnativeのNode-derived quoteに含める必要がある。これらの定数やunit fixtureの実行は実LLMの容量認定ではない。

## 検証方法

通常Cargoでこのworktreeのsourceからcore library・A1・A4を結合する。前回の外部consensus Rust rlibを読むharnessは今回の最終検証に用いない。

```sh
CARGO_TARGET_DIR=target/a4-fresh scripts/ci/positive-court-a4-ci.sh
```

coreのDB dev依存をビルドする場合だけ、互換native RocksDB archiveを`ROCKSDB_LIB_DIR`に指定できる。Rust consensus実装やcheckerの代用品にはしない。個別command、結果、source hash、使用環境は今回の検証記録に保存する。

実A1/A4結合では独立scalar期待値を持つtiny TIRを実行し、全必須関係、欠落、偽Panel、無認証packet、署名流用、別context、再送、期限、branch/replayを試す。A5結合では実PrivateAccountingStoreの明示test原資を使い、hold冪等性、原子的失敗、事実/receipt照合、二重close、undo、期限後の正当close、古いtransition拒否を試す。test balancesはnative UTXOの試験ではない。

認証済みpacketの検査失敗でもattempt/wire/役割別結果は消費・更新される。native handlerはそのsemantic errorを理由にcase全体をrollbackして予算を戻さず、受理済み失敗packetもjournalへ確定する。未認証CarrierErrorの拒否とは分離する。

## A0/A5/A6に残るnative接続要件

以下は成功stubで埋めていない。現在のnative未割当portは拒否を維持する。

1. A0のnative carrier/row割当から、選択viewのPanel/prover資格・author key・実署名済packet・有限duty/cutoffを解決する。鍵や現在時刻を受信packetの自己申告から作らない。
2. 実UTXO、payer consent、Node-derived full-frame/peak/全分岐quote、source/provider義務、既存money engine、capacity/case/liability/journal/undoを同じDB batchでcommitするAtomicCourtStoreを提供する。旧FundingTermsのexecution rootへ新Result/Panel rootを代入しない。
3. A0が管理するFundingScopeの明示projectionを、最終Result/Service ID・payer・実原資へ結び付ける。Result/service/fundingの循環commitを作らない。
4. actual A1終端を持つAdmittedCheckedFactとnativeの現在行から、A5のopaque transitionを作る。DTOの合格boolean、旧claim ID、local replayをGate Cの代わりに使わない。
5. A2 CanonicalDataDutyV2の根拠となるcanonical notice/service opportunity/source catalog/全Panel union予算を、上記受付と同じnative状態へ保存する。private 4KiB資料木や整数holdを代用しない。
6. 共通Courtのcandidateと認証済frame・失敗/再送・全予算・会計を同じnative journalで復元する。今回の共通Court replay試験は実frame再実行の決定性検査であり、実OS crash/fsync/IBDの完成を意味しない。追加tag付きA6 archiveは旧private prototype専用で、新共通DTOの保存形式ではない。

これらを未実装のまま公開claimを受付・決済可能にする変更は本branchにない。旧Fraud Courtと既存claimの裁定・信用条件を新形式へ読み替えない。

ResultAdmissionのpeer literal basisは`check_native_admission` / `prepare_native_atomic`でNodeが再導出した前検査capabilityのbasisと完全照合してから準備する。peer digestを任意の料金表のauthorityにしない。op4 responseはaccepted request.kindとnative prompt formから既存raw Borsh ArtifactOpening/PromptIdsOpening/Flat Vec<u32>を選択し、indexは既存openingに従う（Flat=0）。新enum/tree/料金台帳を作らず、A2のsealed native source read/completionとの接続で認証する。

Witness recordにはA0 `d284f5902`のborrowed header/generated codecが割り当てられ、`record_ingress::preflight_witness_record_v2`が外body版本・操作・rooted Resultとのbindingを候補変更前に照合する。full class/job/programはnative登録から再構築してA1へ渡す。record破損はcoordinatorを開く前の搬送拒否であり、Witness拒否やattempt消費へ自動転換しない。

## 確定sourceと検証

初回検証sourceは`d746fe4550a5dd1c2fabc1925c08a82d9879aea6`。通常Cargoの198件、source hash、合格範囲は[VALIDATION-A4](VALIDATION-A4.md)を参照。その後のA0/A7独立レビューで候補open前のrooted権限確認と既受理packet再送の非課金に不足が判明したため、同commit単独をnative統合readyとはしない。修正と追加検証を後続source・証拠へ分離する。

A1実検査器は`3b26100c0`のpure core実装へ一元移動し、`core::palw_positive_check_v2` / `palw_positive_contract_v2`とexec旧pathを同一型で公開する。A4薄adapterは`coordinator::tir_checker`を正本としexec側はreexportのみ。A2 `f8c79c92f`の有限source台帳・既存G14 geometry・private typed writerを選択統合した。A0のtyped dormant fenceとraw176 predecode境界は公開presetを変えず取り込んだ。

独立レビュー修正後のsourceは`d2047d64d0db68dee44c92a4a8c8025eedf0d458`、同一sourceの271件成功は[VALIDATION-A4-R2](VALIDATION-A4-R2.md)を参照。required候補認可・全opaque context照合・bounded accepted-frame ID indexと無課金再送を実装し、終了時indexを解放する。native役割/notice/料金表/leaseはA0の後続契約を待ち、成功を代用しない。
