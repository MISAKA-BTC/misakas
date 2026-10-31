# A0 共通契約 r1 — 検証記録

2026-10-31。対象は `codex/positive-court-a0` の共通crate・docs。Node演算・Panel生成・native受付・支払の全体検証ではない。

| 検査 | 結果 | 証拠 |
| --- | --- | --- |
| `cargo test -p misaka-palw-positive-contract --offline` | 13 PASS、0 FAIL | [raw log](../../../evidence/positive-court-a0-2026-10-31/tests.log) |
| `cargo clippy -p misaka-palw-positive-contract --all-targets --offline -- -D warnings` | PASS | [raw log](../../../evidence/positive-court-a0-2026-10-31/clippy.log) |
| `cargo metadata --offline --no-deps --format-version 1` | member/default-member登録確認 | [summary](../../../evidence/positive-court-a0-2026-10-31/cargo-metadata-summary.json) |
| 文書検査 | 新規欠落0、既知43 | [raw log](../../../evidence/positive-court-a0-2026-10-31/docs-check.log) |
| 別エージェントの読み取りレビュー | 3指摘を修正し再確認。残る重大指摘なし | [record](../../../evidence/positive-court-a0-2026-10-31/readonly-review.txt) |

ソースのSHA-256は [snapshot](../../../evidence/positive-court-a0-2026-10-31/source-snapshot.json)。テストは実ML-DSA-87の役割・actor・署名scope、field改変、登録root・branch・ruleset・正規job/execution/inputの照合、旧形式回避、range/上限/切断/余剰bytes、全Panelで共有するcase identity、独立Pythonの固定bytes/ID vectorを確認した。

レビューで発見した問題と修正は、(1) Witnessのprofile/version照合、(2) payload長の検査迂回を防ぐprivate framing field、(3) 任意execution nonceによるcase予算更新を防ぐtrusted contextのjob/execution/Miner/input固定。反例は契約テストに含めた。current stateでのatomic commit再検証、同じcaseでの異なるResult拒否、dutyとrequestの一体化も接続契約へ反映した。

公開runtimeへの接続・有効化なし。既存Nodeが新しいnative objectを受理する変更はなく、`require_native_activation`は拒否する。旧G14の全回帰を実行済みとは報告しない。実LLMの負荷測定とcold/warm供給、原資・全consumer・restart/reorg/IBDはA1〜A7の結合時に確認する。

## 追加統合の記録

初期13件と上記snapshotは履歴のまま保持する。row契約`327393e8e`に至る共通ソースで**29件PASS**、Clippy `-D warnings`成功。[source/commands/exit/hash](../../../evidence/positive-court-a0-2026-10-31/integration/native-rows-review-r2.json)、[tests](../../../evidence/positive-court-a0-2026-10-31/integration/native-rows-review-r2-0.log)、[clippy](../../../evidence/positive-court-a0-2026-10-31/integration/native-rows-review-r2-1.log)。同じソースで実行したことをsnapshotで照合した。前段で検出したtest内の`useless_vec`によるClippy失敗も別名の原logで保持した。

追加反例は全10row variantのcanonical decode、切断/余剰bytes、kind/ID/version、全cursor未完了の終端拒否、Panel拒否とPositivePassedの矛盾、署名Result/Service/Consent/Planの置換、funding origin/outpoint取り違え、fee/capital/surplus保存とoverflow、残義務とPanel完成の分離。これは固定DTOと署名linkの試験であり、core読取APIのcompile・native受理・originの実履歴認証・live UTXO・A1再演算・P抽選承認の検証ではない。

A1の10件と既存Court7件も、[別candidateの記録](../../../evidence/positive-court-a0-2026-10-31/integration/a1-court-20261031T114816.summary.json)で確認した。run中にA2/A5のworkspace登録が進んだため、manifest差分を保存し、最新の全体統合成功と区別する。A3側のr1/funding互換検証は[owner記録](../../../../handoff/positive-court/A3-native-compat-r1.md)を原文のまま保持する。

P所見型`069d54007`追加後は共通契約**35件PASS**、Clippy成功。[commands/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/p-attestation-common-20261031T124350.json)と[raw log](../../../evidence/positive-court-a0-2026-10-31/integration/p-attestation-common-20261031T124350.log)を保存した。new6件は固定837-byte codec、全binding差替え、別draw record、三verdictの実署名、旧署名context拒否を確認する。Pの実抽選・投票受理の完成実績ではない。

A2の未保護tail反例は修正commitを含むcandidateとして受理した。[独立読取監査](../../../evidence/positive-court-a0-2026-10-31/integration/a2-protected-tail-readonly-review.json)はsourceとowner evidence hashを照合し、新たなbuildを行っていない。A3のstatus_checks追従と共に取り込んだ後のsame-candidate再検証を別に実施する。

primitive record codec`d284f5902`追加後の共通契約は**41件PASS**、Clippy成功。[source/commands/result](../../../evidence/positive-court-a0-2026-10-31/integration/primitive-record-common-20261031T125130.json)、[raw log](../../../evidence/positive-court-a0-2026-10-31/integration/primitive-record-common-20261031T125130.log)。新6件は独立Python byte vector、borrowed IDs、長さ・生成数・Result/Witness binding等を検査する。A1/A3によるrecord入口の実演算接続は別候補で検証する。

coreの[全test source typecheck](../../../evidence/positive-court-a0-2026-10-31/integration/core-rows-fence-tests-typecheck-20261031.json)はsource変動なしで成功。consensus libraryの[dispatcher typecheck](../../../evidence/positive-court-a0-2026-10-31/integration/consensus-positive-dispatch-typecheck-20261031.json)も成功した。これはテストの実行や新Positive claimのnative受理ではない。

typed fenceは専用targetの**8件PASS**を確認した。[command/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/positive-fence-eight-tests-r2-20261031.json)、[raw log](../../../evidence/positive-court-a0-2026-10-31/integration/positive-fence-eight-tests-r2-20261031.log)。全preset無効、future/never identity、全profile fieldのcommit、raw176事前遮断、独立policy hash vector、上限/overflowを確認した。初回は共有testを外部targetへ移す際のimport不足・private visitor呼出でcompile失敗し、その原logも保持。private visitorの直接試験はunit側へ分離して残し、public APIを試験のために広げていない。この追加unit試験のruntime実行は未確認。実行環境はdebug0/core opt0、test thread stack 16MiBであり、本番stack/RSSの資格測定ではない。

A3のexact`6337ea50d`に対する[129件の同candidate検証](../../../../handoff/positive-court/A3-joint-6337ea50d.md)を選択統合した。A2修正版・A3・A1・default-offの結果であり、その後のP所見・record codec・native source APIの成功へ合算しない。A5 phase3は[ownerの別candidate記録](../../../../misaka/palw/positive-accounting/evidence/phase3-registered-final/README.md)を保持する。

source read API`49c86968c`は[core library typecheck](../../../evidence/positive-court-a0-2026-10-31/integration/native-source-read-port-typecheck-20261031.json)がsource変動なしで成功した。default `Unavailable`を実native授権として数えず、A2が実VP/private constructor/ConsensusProxyへ接続する段階は未完了。

## pure core移植・Court・構造restoreの選択統合

A6の割当前scanner試験追加・逐次hash変更後、共通契約は**42件PASS**、Clippy `-D warnings`成功。対象sourceは実行中不変。[commands/env/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/post-restore-common-20261031.json)、[test log](../../../evidence/positive-court-a0-2026-10-31/integration/post-restore-common-20261031-0.log)、[Clippy log](../../../evidence/positive-court-a0-2026-10-31/integration/post-restore-common-20261031-1.log)。codec・identity・固定rowの試験であり、native履歴認証や実資金lockの成功ではない。

A1 pure checkerのcore移植とA4 owned Court追加直後の[core型検査](../../../evidence/positive-court-a0-2026-10-31/integration/core-court-restore-typecheck-20261031.json)は**exit101**。opaque identity比較の欠落1件、旧private prototypeの未統合wide API参照3件を検出した。`96a6826e4`で比較を追加し、`d028330f8`でprototypeの対応範囲をi32へ明示限定した。採択済みwide設計を撤回した変更ではなく、未統合実装への暗黙依存を除く選択統合の修正である。修正後の[core library型検査](../../../evidence/positive-court-a0-2026-10-31/integration/core-court-restore-typecheck-r2-20261031.json)は**exit0**、全core/shared対象source不変。[失敗原log](../../../evidence/positive-court-a0-2026-10-31/integration/core-court-restore-typecheck-20261031.log)と[再検査log](../../../evidence/positive-court-a0-2026-10-31/integration/core-court-restore-typecheck-r2-20261031.log)を別々に保持する。これは後続のdirect decoder変更や実テスト実行を含まない。

A4の271件、A6の30件、A3の148件、A7の243件は、それぞれownerの固定候補・条件に対する履歴であり、このA0候補の合計件数ではない。A7の独立追跡で、未受付future frameの正規署名コピーがCase wireを反復消費する残件を確認した。[静的反例と範囲](../../../../handoff/positive-court/A7-A4-fix-review-815b7ca1c.md)を参照。A4修正488の必要2ファイルを`17990b173`へ統合し、順序/現在relation受付をCase課金前へ移した。反復futureコピー無課金と後続正順受理の反例テストも追加したが、この記録時点では新runtime結果は未確認。無認証/将来位置の搬送拒否と、現在relationのauthored算術失敗に対する費用保持を区別する。

A5 readonly private receipt復旧`c8d7238dd`を加えたleaf会計は**44件PASS**、Clippy成功、対象source不変。[commands/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/private-recovery-accounting-20261031.json)、[test log](../../../evidence/positive-court-a0-2026-10-31/integration/private-recovery-accounting-20261031-0.log)、[Clippy log](../../../evidence/positive-court-a0-2026-10-31/integration/private-recovery-accounting-20261031-1.log)。期限後の既存receipt読取、全intent照合、foreign store拒否、undo権限を増やさないこと等を含む。private bookの成功をnative再起動や実資金復旧の成功にしない。


## 2026-11-01 件数訂正と共有journal・用途別会計

以前の共通契約44件という報告は、1つの割当検査内の複数probeを独立テストとして数えた集計誤りだった。原ログのCargo summaryは42件であり、この文書の説明を訂正した。private会計の当時44件は正しい。[監査記録](../../../evidence/positive-court-a0-2026-10-31/integration/test-count-audit-20261101.json)を追加し、過去の原ログ・snapshot・digestは変更していない。

`0d678c8be` の同一sourceに対し共通契約**51件PASS**、private会計**53件PASS**、両crateのClippy `-D warnings`成功。[commands/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/journal-and-eight-purpose-leaf-20261101.json)、[共通契約log](../../../evidence/positive-court-a0-2026-10-31/integration/journal-and-eight-purpose-leaf-20261101-0.log)、[会計log](../../../evidence/positive-court-a0-2026-10-31/integration/journal-and-eight-purpose-leaf-20261101-1.log)、[Clippy log](../../../evidence/positive-court-a0-2026-10-31/integration/journal-and-eight-purpose-leaf-20261101-2.log)。borrowed journal codec/連鎖の固定形式と、既存7用途を維持して末尾に追加したWorkerPaymentの独立entitlementを含む。sourceは前後不変。native再生・実資金・P抽選・公開有効化の合格件数として合算しない。

先行`5871a5b86`時点のconsensus全test source型検査も成功した。[source/result](../../../evidence/positive-court-a0-2026-10-31/integration/native-provenance-typecheck-20261031.json)、[原log](../../../evidence/positive-court-a0-2026-10-31/integration/native-provenance-typecheck-20261031.log)。Full carrier provenance・raw decode前の上限検査・readonly metadataを含むが、後続のRAM lease強制・source復旧・journal候補の型検査やnative実行成功を代替しない。


## 2026-11-01 canonical tokenizer・typed capture・source journal結合

`dbe95bacc`のsourceで、ExpandedPlan5・FullRelation11・既存r1 Panel7・typed capture9の**32件PASS**を確認した。[commands/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/canonical-typed-source-joined-20261101.json)、[原log](../../../evidence/positive-court-a0-2026-10-31/integration/canonical-typed-source-joined-20261101-0.log)。sourceは実行中不変。登録tokenizerとcanonical job tokenizerを独立に保持し、固定expected context/class/Resultの差替えを拒否する。typed capture→FullRelationの逐次adapterそのものはまだ未接続で、同target実行をadapter完成とはしない。

同記録のsource指定`--lib palw_positive_source_geometry_v2`は移動済みテストを選択できず**0件**だった。exit0でも合格実績に含めない。[その原log](../../../evidence/positive-court-a0-2026-10-31/integration/canonical-typed-source-joined-20261101-1.log)を保持し、正しい専用target`--test positive_source_v2`で**37件PASS**を確認した。[訂正command/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/source-journal-dedicated-20261101.json)、[専用target原log](../../../evidence/positive-court-a0-2026-10-31/integration/source-journal-dedicated-20261101.log)。後者も全対象source不変。raw source journalの完全Case binding・NoOp非append・G14認証・全Panel予算等を含むpure preparationの検証である。

A6のnative journal caller-batch保存とactual accepted-data readerを`6e25d7631`へ選択統合した。[独立read-only review](../../../evidence/positive-court-a0-2026-10-31/integration/a6-native-data-readonly-review-20261101.json)は新たなbuildをしていない。ownerの別候補38件（native11を含む）は[A6記録](A6-COMMON-JOURNAL-VALIDATION.md)を保持する。native writer/accepted role/資金lock/qualified支払を生成する実装ではなく、当A0候補の実行件数へ加算しない。


`79c3f9bca`のconsensus library型検査は**exit0、全対象source不変**。[commands/env/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/native-journal-data-typecheck-20261101.json)、[原log](../../../evidence/positive-court-a0-2026-10-31/integration/native-journal-data-typecheck-20261101.log)。A6のnative store/reader、A1のborrow APIとA2のlint修正を含む。これはlibrary型検査であり、同候補のnative unit実行・actual受理/精算・restart/IBDの合格ではない。A7へ同sourceの独立回帰を依頼した。


## 採択済みwide Q24のstructural foundation

`50339b43a`のTIR sourceで新structural7件・既存golden3件・golden_v2の11件、計**21件PASS**、対象Clippy `-D warnings`成功。全対象sourceは前後不変。[commands/source/result](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-and-goldens-20261101.json)、[原log](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-and-goldens-20261101.log)、[独立レビューとClippy](../../../evidence/positive-court-a0-2026-10-31/integration/wide-structural-review-and-clippy-20261101.json)。exact schemeと最終post I64 commitだけを構造認識し、他のI64 commit・state・carry・非最終logitsを拒否する。新codec・実LLM・native受理・広幅Courtの試験ではない。旧i32の単なるencode/decode roundtripをpre/post goldenの独立証拠と呼ばず、既存の固定golden targetを別に実行した。

採択本文・元checkoutでの登録実験原資料も取り込んだが、[統合境界](WIDE-Q24-INTEGRATION.md)のとおり外部実験の成功をA0のPositive Court完成実績へ加算しない。元evidence manifest73件のfile hashを照合し、原ログ・digest・実測値を維持している。
