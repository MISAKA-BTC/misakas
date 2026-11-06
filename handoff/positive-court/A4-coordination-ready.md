# A4 共通Court — 引継ぎ

Workspace: `/Users/wata/.codex/worktrees/a4-g14-court/MISAKA-testnet`。Branch: `codex/positive-court-a4`。

現在のsourceは`c1292bc08`。A0/A1/A2/A5/A6と直接調整し、A4実装・受付/資金前検査・独立反例・native入力引継ぎ・readonly復旧を並列で進めた。新job授権のRFC/ADRはA0で未採択であり、actual176 one-writer・実coin lock・native完全履歴・T12/reopen/IBDの完成は主張しない。公開presetはNone。

## 実装したAPI

- `coordinator::contract::ContractCourtV2<C,V>`。actual A1 checkerはcore一元実装。`tir_checker::TirCourtCheckerV2`が薄い委譲を行い、exec旧pathは同一型reexport。
- `carrier::PanelSignatureVerifierV2<R>`。candidate open前にも現在rooted prover/opportunity/author/resourceの認可を必須にする。Gate C事後drawをopen資格へ代用しない。
- `admission::AtomicAdmissionCoordinatorV2<S,V>`。new intakeと既存caseの`resolve_committed`を分離。実store receiptとactual checked factをscope/case/resultへbindする。
- `funding::FundingPreflightCoordinatorV2`。`check_native_admission` / `prepare_native_atomic`でpeer basisと独立quoteの全fieldを照合。実UTXO/lockを前検査だけで生成しない。
- `record_ingress::preflight_witness_record_v2`。外版本/操作/rooted Resultとborrowed A0 recordを候補変更前に照合。class/jobからのactual A1再構成・全関係検査は別途必須。
- `native_body`。version2/revision1、local operations0..7。ResultAdmission、WitnessCommit、ExactFrame、SourceRequest、SourceResponse、Finish、Expire、PositiveCourtAttestation。原本は`A4-NATIVE-BODY-R1.md`。keysはrooted stateから解決し、clock/Passedをpeer fieldにしない。
- `CommittedCourtRecoveryStoreV2` / coordinator `recover_committed_case`。現在のactual storeが既存case receiptをreadonlyで返すrequired port。署名・scope・immutable links・現在rootを再検査しfresh coordinator ownerへbind。new prepare/commit/新受付期限を使わず、hold/epoch/journal/undoを増やさない。native具体implは未供給。

## 重要な修正と選択commit

- `ee06d8173`: candidate認可/context/plan/start/heap確保をmutation前へ。全accepted frame IDを64×関係数の予約済みindexで保持し、既知packetは認可/署名確認後の無課金ACK。
- `488317d3e`: future/order/header条件をCase課金前へ移動。未来frameのコピーは無課金・journal非append、後で同じ署名を正順受理可能。current relationの不正算術は費用保持。
- `1ae6c23cd`: 旧private prototypeは自己完結するnarrow-only。Wide WIPの3symbol依存を除き未対応は明示拒否。RFC43 snapshotを全importしない。
- `c578682a9`: actual UTXO first passのFull factoryからowned provenanceを採取し、実entries/fee/body owner/session/origin/ordinalをphase twoへ保持。通常UTXO適用は一度のみ。後段Court consumerは未接続。
- `f5fcf628b` / `946bf2dee`: raw Borsh各doorでactive176 bounded preflight、別用途readerはraw176除外。global enum/KindFence ALLは未変更。
- `786c00227`: native予約のOk(None)をFull前にtyped capacity faultとして拒否。real leaseを保持する。A0のallocation-free identity hashと共通source due DATAを選択取込。
- `0c7c2bcf3`: readonly recovery API、A5 genuine getter425の14行＋4試験、actual book/actual A1再bind試験。
- `c1292bc08`: A2 `4da761423`のcore6filesを選択取込。op3/op4 logical relayはusage/history/fee/Caseを変えず、`changes_case()==false`。旧union/receiptのliteral CASは維持。source dueは共通7fieldへ統一。

A0/A1/A2/A5の既取込ファイルは重複cherry-pickせず、上記A4固有差分を選択する。

## native実装に残る供給点

1. 新Resultとcanonical registered job/input/context/seed/expected Minerの正規対応。旧header・旧FP job・kernel interim Panel・source checker cohortを新授権へ流用しない。
2. 実current accepted-prefix、branch/anchor、generation/Case epoch、rooted roles/opportunities、有限profile/schedule preimage、全試行/全一時heap/保持/復旧lease。
3. A5 new-version protected outpoint、共通mempool/block predicate、same-mergeset provisional locksとqualified spend。旧TFO1/TFP1のcontract IDへ新Scope IDを入れない。
4. A2source＋A5money＋A6complete case journalを同じTransitionBuilder checkpointとKernelRouteRow old/new deltaへ入れるactual one-writer。per-block canonical delta/UTXOdiff/acceptanceは既存caller WriteBatchへ。virtual tip/UTXO set materializationは別の既存replay段階。
5. 全Case eventのactual accepted material保持/再生、支払イベントからのreceipt typed refresh、現在row/funding/lease/epochからのnative readonly復旧receipt。A6 private archiveはlocal cacheに限定しrawRelatedRowでUTXOを書かない。

kernel header欠落は明示拒否。認証済みでCase状態/費用を変えたsemantic failureはその費用とraw eventを確定し、Errだけを見て全rollbackしない。完全NoOpとtransport拒否はCase journalへappendしない。ordinary TX fee/historyはnative層で独立。

## 検証

`d2047d64d`の271件は後日のfuture-frame指摘前の履歴。`488317d3e`は284件、`0c7c2bcf3`は295件とconsensus全tests型検査が成功。各候補を合算せず原ログとcommit/treeを`docs/evidence/positive-court-a4-integration-2026-10-31/`へ保存する。最新`c1292bc085cbfc1ca24a8ac0a962ab1096a88f23`は同source通常Cargoで300 PASS/0 FAIL、consensus全tests型検査成功、文書新規欠落0・既知43。`docs/design/palw/positive-court/VALIDATION-A4-R5.md`と`final-r5/`の原ログ・commit/tree・118source SHAを参照。

再実行は`scripts/ci/positive-court-a4-ci.sh`。通常CargoのRust source build。既存互換native RocksDB archiveのみ任意で再使用し、consensus Rust rlibを代用しない。重複するcore/Node公開API driverを件数に含む。構造unitやprivate book成功はactual native受付/UTXO lock/T12/reopen/IBDの代用ではない。
