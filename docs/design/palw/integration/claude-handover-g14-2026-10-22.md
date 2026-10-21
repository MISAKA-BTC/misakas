# Claude 引き継ぎ — G14、2026-10-22

**G14は未完。武装・配備・本番採択は禁止状態を維持。** ユーザーから「ここからclaudeに引き継ぐ、pushしてG14の残りを記述して」と依頼された時点の記録。現在の未完実装と失敗を含め、作業branchへ保存する。preへの統合は今回の依頼に含めていない。

## 0. 再開場所と守ること

- Repo: MISAKA-BTC/misakas、remote origin。branch **codex/g14-completion-after-refactor**。
- Codex checkout: `/Users/wata/.codex/worktrees/rfc14-g14-integration-candidate/MISAKA-testnet`。別のcheckoutでもremote branchを取得して再開できる。
- `/Users/wata/Downloads/MISAKA-testnet` はユーザーのdirty checkout。変更を上書きしない。
- 今回のWIP直前の検証済みHEADは **884906e64**。今回のcommitは材料familyのWIPとこの引継ぎを含む。commit自身のhashはgit logで確認する。
- 以前のrefactorだけはpreへ **537843abe0ffb6e058b2ca95068192fddac386f9** で統合・push済み。後続G14作業はpreにない。
- 最初に root AGENTS.md、docs/AGENTS.md、docs/PRINCIPLES.md、元のcodex-handover-court-rfc14-2026-10-20.md、RFC-0026と下記台帳を読む。凍結したモデル二階層・モデル単位審査禁止・計算の暗号学的証明禁止を維持する。
- 全cargo/rustc/native test/重い測定は `/Users/wata/.cache/misaka-heavy-lock.sh`。実行中は全repo Rust/Cargo編集を凍結。exeはcopy2でpinしSHAを残す。別環境でも同じ排他・source固定・binary同一性を確保する。
- 失敗前のsource/patch/logを保存してから修正する。旧wire/tag/ID・既存caps・期限・assertionを黙って変更しない。仮値や推定を測定値にしない。
- docs編集後は `python3 docs/maintenance/check_docs.py`。現在はbaseline43件＋以前のimport由来3件=46件でFAIL。baseline一括更新禁止。今回新規参照欠損は0。
- このhandover作成時、retry2 runnerはexit1で終了。native processは残っていない。再開時にも実processを確認する。/tmpだけを唯一の引継ぎ元にしない。

## 1. 採択済みの隔離実装方針（本番採択ではない）

- ADR-0192: F02の実供給と配送/収録前提、F03のowner独立共通frontier・全合法分岐事前予約。
- ADR-0194 A: X3旧tag保持、service別version/namespace。
- ADR-0197 A: 裁定と未決責任archiveを分離。名目depositはescrowではない。
- ADR-0198 A: 初回notice **a ≤ d ≤ a+3,120**、original hard **a+10,000**。
- ADR-0199 A: parameter/promptを含む認証materialと終端intent、未完成bodyの先着占有を避ける。
- ADR-0188 B: tag230はMerkle root/count/path。draw上限は未採択。
- ADR-0200 C1: F02にdigest/path開示courtを接続。
- ADR-0201 S1: 元期限内に実際に受理・完成した正しい履行だけ当該sideを金融解消。期限後の資料だけでは解除しない。
- ADR-0202 A/RFC-0035: 実入力UTXOで裏付けた独立運用escrow。旧rentはburn、旧depositは名目。金額・新料金・本番有効化は未承認。
- role-consentについて追加の形式的A/B確認は不要と再検証した。既承認RFC-0035の実本人同意/current authority/期限/支出上限を実装する。匿名slot=producerと仮定しない。動的な有償provider獲得は採択していない。候補内のdecision review参照。

## 2. 検証済みの現在地

詳細は [active completion ledger](g14-active-completion-ledger-2026-10-21.md)。個別component合格でT01〜T15全体を完了扱いしない。

- 884906e64: Output→通常Stopの共有FullIds、普通のSelection D×Vと実tile/replay。97件等の今回件数と混ぜない。このcommitの記録は87 unique parent＋quota worker2件成功。
- b28128f45: 実native Source policy/登録DAA/current source pledgeとConstraint Stop接続、83 parent＋worker2件。
- 0bbdf02c6: Source専用inventory、retained Constraint/Tokenizer borrowed builder、82 parent＋worker2件。論理予約byteは物理RAM/CPU実測ではない。
- parameter、Input0、Fixed/history、実Prefix、Tokenizer EOF/key producer、signed terminal/fresh recheck等の段階実装は台帳と各evidenceにある。
- pruning: **2883727b6** が既存harnessのpostprune pacingを修正。production tick/deadline/cap/assertionを弱めず対象1件118.49秒＋既存実サービス6件275.94秒成功。全shared funded T14は未完。
- payload: [2026-10-22測定](../../../measurements/2026-10-22-devnet-payload-carrier.md) は4loopback nodeで600RPC受理/600.028秒、観測したselected chain受理90、未観測510、最後mempool505。一般的飽和・予約service・G14全体・door達成を意味しない。旧7tx quiet devnet記録と区別する。owned node/appdir/keyは片付け済み。
- RFC-0027/ADR-0193のZIP反映は資料として完了。新Account Bond実node、実Panel利益、heartbeat増加率は未測定。10万MSKが十分とはしていない。

## 3. 今回pushするWIPと直近の失敗

[material-family-closure](../../../evidence/palw/g14-completion-2026-10-22/material-family-closure.md) と同名directoryが正本。

実装内容: Constraint既知codec/preflightとD×V×(V+1)潜在domain、Prefix version11の実境界、数値全root/input→全role Finish、Tokenizer実raw/EOF/Generated key-bound provenance、selected-root runtime material。認証済みmaterialは既存native primitive/current signed intent/fresh recheckへ渡す。新しいsemantic provider namespaceは捏造せず未解決に残す。Prefix生成token selection/stopやexact selected-path最小readsetも未完。

検証の3段階を混同しない:

1. initial: 新しいabsolute packet-ID metadataをHash64と誤記してcompile fail。既存APIの `[u8;64]` に修正。初期source/log/hash保存済み。
2. retry1: core release build成功。全graph反復走査＋無関係StepRun逆引きで数値joinが長時間CPU処理。owned groupを意図的に停止しexit143。成功扱いしない。sample/interruption/source保存済み。
3. **retry2: core release build成功、97 unique parent成功・2失敗。quota worker2実行成功。runner最終exit1。native consensus build/testは未到達。** 数値join、Prefix、Tokenizer、Finish直接lookup旧scan同値、旧TIR court/parameter群、t12同一性9・activation5は成功。`retry2/test-summary.json` はinterleaved worker出力を別計上している。

残る2失敗:

- `shared_constraint_material_builder_reconstructs_real_native_original_and_rechecks`: `palw_court_shared_constraint_native_tests.rs:204`、raw `retry2/core-selection-13.log`。
- `shared_constraint_stop_terminal_uses_full_ids_without_row_and_preserves_boundaries`: 同455行、raw `retry2/core-selection-7.log`。
- 両方ともfindingのseed_idだけが異なる。full materialと、material_idを変え再署名したsparse/EOG-only intentをfull findingで比較している。**まず同じselected material/intentを使った旧native builderとのexact equalityで検証する。** seed/domainを変えたりassertを消して通さない。原因確定/修正/再実行が必要。
- Finish探索は同じ既存hash(scope, Finish)と実StepLeaf ordinalへ直接アクセスする修正済み。全target×全roleチェックとlocal work上限は維持。完全なCPU校正ではない。
- テスト側にもdependencyごとのgraph全scanが残り、数値joinは約数分。未適用のtest-only索引化候補あり。fixture縮小やcap引上げをしていない。

再実行元は evidence `material-family-closure/retry2/verify.py`。repo/out絶対パスを自分の環境と新しい空のretry directoryへ直してからheavy lock下で実行する。既存outは上書きしない。source/hash/pinned executableを保持する。最後にactual native FullUTXO testまで到達させる。

## 4. 未適用候補（pushに同梱、動作未検証）

場所: `docs/evidence/palw/g14-completion-2026-10-22/claude-handover-candidates/`。sha256.json、各NOTES/独立review付き。**候補patchを置いたことは実装済みではない。** 各applycheckを再実行し、追加差分をレビューしてから適用する。

1. `test-indices/family-test-indices.patch`: 現tree上のtest-only修正。Numerical/Prefix/Selectionの実graphを一度だけ独立索引化。membership/role/leaf/absolute ID/roots照合を維持・強化。小graph oldscan/direct同値testは残す。
2. `preclaim-combined/combined.patch`: native-origin＋aggregate＋original requirementsの3候補を機械統合。現材料family＋Finish treeでapplycheck済み、未compile。actual native paired preparationだけからprivate holderを作り、**FP reserve前**にSource/numerical/allroot InputCoverを導出。旧TFP1経路に新Source/quotaを強制しない。missing実service namespaceはtyped missingで保持。tail native admission拒否は解除しない。aggregate draftは完全な成功/admission権限を与えない。
3. `preclaim-role-catalog/preclaim-role-catalog.patch`: combined適用後。既存SourcePledgeの実producer/bond/current key/operator/Active/署名/期限を再検査。匿名potential slotとは別扱い。新全branch assignment/service operator/semantic provider/fee-capital-retention-recovery契約はtyped missing。旧署名を新責任への同意へ流用しない。

各候補は独立read review済みだがcompile/native未実行。combinedの実native fixture filterは `g14_native_funded_candidate_fixture_generator_v1`、既存paired acceptance/undo/t12回帰も必要。小fixtureの数値rootが0の場合、それで非空の全分岐を検証したと言わない。

## 5. G14全体の残作業（元のscopeを維持）

1. **全分岐の原子的受付**: originalの実native contextから全合法要求・応答・証拠・依存契約・担当者と本人同意・保持/期限・CPU/heap/IO/転送/保持・必要資本・実feeを導出し一括予約する。callerのcomplete/paidフラグ、profile hash、自己署名manifestだけをauthorityにしない。
2. **F02**: accepting block由来の実供給を合意記録。明示配送/収録前提下の供給済みdefaultと、未供給no-slash・保持・復旧を同じ実native経路で接続する。現在guardは拒否中心で、実default成立まで未完。
3. **F03**: owner全占有/継続再充填でもeligible outsider1人が開示→局所化→裁定を有料公開fair ingressで完走する。共有frontier構造だけでは未完。未完成body先着vetoを作らない。
4. **期限/依存/署名/relay**: inclusive3,120とoriginal hard10,000をcollect/disclosure/retention/Final/bond/依存契約へ原子的に適用。現在literal署名権限、offline Minerで第三者relay、全境界-1/=/+1と旧claim保持。
5. **金融/終了効果**: 実UTXO-funded escrowのlock/qualified spend/徴収/Panel・relay支払/返還/全責任解除/pruning/undo。S1の期限内実履行のみ当該責任解消。archiveの名目額保存は実原資確保でも返還可能性でもない。
6. **D2**: C1 digest/path開示courtをnative署名・draw・料金・状態・wire/fenceへ接続。tag230 B形式だけでは入手可能性を保証しない。finalizeの実必要parameter/readsetを受付時に確保。draw capは未採択、Noneを成功扱いしない。実nodeの全family検証。
7. **P2/実測**: 先読み・1位置化・multiproofと証拠全体の比較。実CPU/IO/転送/保持費用、全original同時負荷、実fee、reserved supplyを測る。**実モデルdoor ≤3,120は未達**。署名だけでSC64を超える場合も隠さない。
8. **RFC-0026 T01〜T15実campaign**: honest/false challenger、永続無応答、偽署名/偽完了、normal/paid/invalid各flood、1 outsider対全colluder、最遅期限、offline signer/relay、同bond複数claim、internal fold lie、1〜10,000追跡と最終だけfalse、hard集中、供給不足時独立claim進行、reorg/restart/pruned IBD、old/new fenceとshipping identity。**全体完了を証明できたT項目はまだ0**。

## 6. 最重要のnative統合上の穴

現UTXO pipelineはselected parent/merged blocksのUTXO受理を進めた後でPALW ordered rehearsal/final foldをする。後段でescrowをlockしても、先に受理された後続merged blockの支出を止められない。

必要: 実validated carrier→同じordered prefixの完全native graph/契約/current authority→後続依存merged block受理より前のprospective lock→FP/frame/graph/funds checkpoint→UTXO＋PALWの単一WriteBatchとreorg/rebuild。

structural TFO全部をlockするとMissing/Invalid時のordinary fallbackを壊す。generic SkipScriptChecksも禁止。actual outpointとrooted stateに限定したqualified spend authorizationを作る。既存accepted_txs_from_acceptance_dataはmissing/indexをskipしtxid照合も完全でないため、完全funding universeのauthorityに流用しない。既存commit_utxo_stateのUTXO/MuHash/acceptance/PALW同一batchを維持する。

## 7. 報告と完了判定

完了は「componentが増えた」「build成功」「公開資料に保存した」で置換しない。各T項目と上記全作業について実native scopeの証拠を示す。失敗/中断/候補/測定/仮定/未採択を分ける。本番金融値や新義務を決める必要が出た場合は具体的RFC/ADRと根拠で確認する。既に承認された詳細実装方針を再質問しない。

ユーザーはここからClaudeへ引継ぐ。Codex側の追加実装はこのsnapshotで止め、pushしたbranchと本書を渡す。G14完了、pre統合、武装、配備を宣言しない。
