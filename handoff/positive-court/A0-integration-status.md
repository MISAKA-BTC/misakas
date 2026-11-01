# A0 integration checkpoint

2026-11-01更新。workspace `/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet`、branch `codex/positive-court-a0`。RFC-0044 / ADR-0212の実装調整。A1〜A7への直接連絡はユーザーが明示許可した。元checkoutや各担当のdirty作業を一括コピーしない。

| 担当 | A0へ統合した範囲 | 未完了・保留 |
| --- | --- | --- |
| A0 | v2/r1共通型、frame作者認証、非循環funding scope、176/151/P、固定row `327393e8e`、無効のtyped fence、P所見 `069d54007`、record `d284f5902`、source read interface `49c86968c` | native live event・writer、実資金lock、P実draw、全consumer・復旧との結合 |
| A1 | narrow/wide source-only FullRelation、canonical tokenizer・row-profile、purewide admission `0df5f3cca` | wide admissionのRoot単体試験、native期待値と全資源予約、実LLM範囲/性能 |
| A2 | 通常推論・有限原資料応答、session seal、protected raw reopen meter復元、logical NoOp/fullCAS、common journal bridge `a87e7f789` | actual native duty/read factory、qualified paid receipt更新、実money owner |
| A3 | r1 Panel・one-pass narrow/wide CPU runner、typed→legacy adapterの同時保持修正、PALWTRC2 capture | Hello2固定plan CPU subsetの実行、汎用artifact startup、wide Panel証拠組立て、実LLM範囲/peak/性能 |
| A4 | Court/coordinator/body0..7、future-frame課金前拒否、Full carrier保持・RAM lease、readonly復旧、purewide sizing `40417c9e3`＋修正 | actual176 one-writer、canonical job/role/profile/schedule、実資金lockとP |
| A5 | 独立entitlement照合、旧7用途維持＋第8 WorkerPayment、first-source fee照合、readonly private会計receipt | 全native consumerと実coin lock/一回払い/安全返金・責任floor、P実承認 |
| A6 | aux151 scanner/restore、shared journal/実A1 local replay、caller-batch material store/actualdata reader `6e25d7631`、実SourceLedger/global差分/fee隔離 `3773083f9` | production NativeReplayEnvironment・actual roles/資金/lease、kind1 qualified spend、P2P/IBD/GC/T12 |
| A7 | 独立fixture/runner、future-copy反例、8用途fixture、A4 R5証拠監査。production79c3＋test-only fixture修正の固定候補で667件成功、原失敗と証拠を保持 | 実trained cohort inventory再照合済み、同候補での推論/登録/native/複数Node性能は未確認。過去候補件数を合算しない |

共通契約はrow段階29件、P所見後35件、record後41件とClippy成功を別snapshotに保存した。core/test sourceとconsensus libraryの型検査、fence専用8件も成功。各担当の別候補での成功と区別する。frame署名は作者の認証、FundingConsent署名は支払条件への同意だけである。row/cursor保存やP所見認証がnative権限を生成することはない。新Pは全LLMで実抽選の`Certified`のみを認め、`PublicCheckCertified`・旧S/K/V・local replay単独を流用しない。

native全経路・実UTXO精算・全consumer・複数Node/reorg/IBD/pruning・実LLM cold/warm測定は未完了。公開presetの有効化は行わない。実装全体の完了や認定profileの成立を宣言しない。

直近のA0実行では、A6 preflight追加後の共通契約42件（旧説明44件を訂正）とClippyが成功（対象source安定）。A4統合直後のcore型検査は4エラーで失敗し、missing opaque identity比較と未統合wide参照を修正した後、`d028330f8` のcore library型検査はsource安定で成功した。これは後続patchやnative実行の成功を含まない。A7 process/manifest Python8件も成功した。詳細は [A0検証記録](../../docs/design/palw/positive-court/VALIDATION-A0.md)。

全Caseの新native履歴は [journal契約](../../docs/design/palw/positive-court/NATIVE-JOURNAL-R1.md) に固定した。完全なlogical no-opはCase課金・履歴append前に判別し、実state/cost-changing失敗は保持する。actual origin/headerではfees/Panel metadataによる抽選再試行、origin計算rootとCourtの厳密結合、旧経済条件との整合が未解決。[origin annex](../../docs/design/palw/integration/positive-native-attempt-origin-r1.md) の非許可境界を維持する。


`0d678c8be`までの追加統合: A2のsession seal・raw reopen時の全meter復元・logical NoOp/CASと実remote接続、A4のFull保持・RAM lease必須化・readonly Court recovery receipt、A5の用途別entitlementと第8用途WorkerPayment、A6の共有journal codec/連鎖と実A1検査器によるlocal replay foundation、A7の8用途独立fixtureを含む。actual native SourceRead/roles/funding/one-writer/P/全consumer/IBDは未接続である。

同候補の共通51件・private会計53件とClippyは成功した。旧共通44件の説明は42件へ訂正し、原ログは保持した。A1 source-only full-relationは`d657007ce`、A3 one-pass typed captureは`da941bb55`、両canonical tokenizer修正は`4bf984f35 / dbe95bacc`へ統合した。A2 source journal bridgeは`a87e7f789`へ統合した。typed→full-relation adapterとactual native環境は未接続。別候補の件数を合算しない。

最初のaccepted native assignment経路はレビュー済みの新RFC/ADR案`08b0db8ba`への明示採択を照会中。旧PAV2/PAV3・L1・発行規則の置換を許可する案ではない。採択前に新authority/wireを実装しない。


`79c3f9bca`でsource候補を固定: A1のimmutable metadata/plan/checker borrow API、A2の意味不変lint cleanup、A6のcaller-batch journal store・actual accepted-data readerと独立レビューを統合した。先行`dbe95bacc`のsourceではcanonical/full-relation/r1/typed32件＋source専用37件がsource安定で成功。古いlib filterの0件実行は合格に数えず別原ログを保持。現候補のconsensus library型検査はexit0・全対象source不変で成功。A7全対象rerunは同sourceで行い、別候補証拠を加算しない。

A7へ`79c3f9bca`固定sourceでの独立回帰開始を依頼した。A6の実SourceExternalState追加候補`1b7fff400`とA3のtyped→FullRelation逐次adapterは後続レビュー・接続・同候補検証が必要。新nativeAssignment案の採択回答は未受領。


後続candidate `1d6641471`: A6実SourceLedger/global差分をfee修正と失敗隔離試験を含めて統合。[独立レビュー](../../docs/evidence/positive-court-a0-2026-10-31/integration/a6-source-replay-readonly-review-20261101.json)。owner199の53件は[A6記録](../../docs/design/palw/positive-court/A6-SOURCE-REPLAY-HANDOFF.md)の候補だけを示し、Root79やphase1の38件に合算しない。wide構造認識`50339b43a`は新7＋既存golden14の21件と対象Clippyが成功。明示V2 logit primitivesを`82146b3b9`へ選択統合したが、その新5試験とprofile resolver/旧未対応入口ガードは次の検証範囲。


`b3da72953`まで: explicit wide data primitives・profile resolver・旧record/StepSpace/bindingの未対応wide拒否を統合。stage2はprofile11/source2/data5の18件が同sourceで成功、Core strict Clippyは等価style整理後に成功。A7の79正式runはA5旧fixture6件失敗の原本を保持、`7463c2f69`のtest-only修正をA0 `b3da72953`へ統合しA7再実行を依頼した。owner55件は16MiB stackの別候補。A3 adapter `01b5fed24`は同時保持予約の不足が見つかり保留し、persistent予約とlive materialの保守的合算を修正中。P/native actual job/roles/money/全consumer/復旧・IBDと実LLM性能は未完了。新assignment RFC/ADR案の明示採択は未受領。


2026-11-01追記（A0 `ab2d12ea8` 時点）: A3の同時保持予約修正済みadapter、A1の明示wide Step APIとmixed Fixed/Hist回帰を統合した。ownerのA3 42件／A1 11件は各sourceの範囲であり、Root同候補の全経路成功へ加算しない。lowerer final r2はRootで31件成功し、旧goldenとresident/streamed変換、wide数値・独立3way fixtureを含む対象sourceは試験前後不変だった。SDK幅pinとprofiled schema1 class-record wrapperは統合し、Root結合試験を実行中。実学習済みモデルの登録完了実績ではない。

A7の旧79c3正式失敗（661成功/6失敗）と、test-only fixture修正後の固定候補 `c2e422625`（667成功/0失敗、role worker ignored 1、16MiB test stack）を別の原記録として取り込んだ。これは現在のwide/SDK/A3統合候補の結果ではない。source役割別の残予約保護は [接続条件](../../docs/design/palw/positive-court/ROLE-RESERVATION-R1.md) に具体化し、A4/A6がread-only reviewした。既存source replay53件はこのnative保護の実装実績ではなく、actual mapper/残予約ownerは引き続き未供給。


Root `afc37f144` まで: A3 explicit wide CPU producer（owner49件・Clippy、Root alias `9689b3369` / evidence `f03fc56d3`）とmetadata/full-layout join・V1 decode拒否を統合。Root `017bff082` ではrecord6件とStep/data11件が標準stackで成功し、SDK全16件は同じbinary・明示16MiB stackで成功。SDK標準stackの全suite停止と追加4件中1件停止は原本を保持し、LLDBでshipped Params生成中の既存apply_object_innerのstack probeへ局所化した。標準stackを直したとは扱わない。現在のnewrun/metadata/decode合成sourceは別のRoot試験中。A1 row-only metadata view、A3 capture2/Hello2、A4 explicit8B sizing、A0 reviewerのwide source-only checkerを並行作業中。新nativeAssignment案の採択回答は引き続き未受領。


Root `4e60a7253` checkpoint: `afc37f144` と同じRust sourceのmetadata8／decode4／wide runner10／専用positive-panel adapter8、計30件は標準test stackで成功・source不変。先行node-only adapterの0件は [count audit](../../docs/evidence/positive-court-a0-2026-10-31/integration/wide-run-safety-count-audit-20261101.json) で明示除外し、専用featureで実行し直した。A1 row-profile/helperの確定sourceをRoot `a01b79dda / 941b78c86`、owner証拠を`66e1aad31`へ取り込み、Root currentのSDK/Core/exec(positive-panel) library strictClippyは成功。A1の9+1件はowner983b5277aの別候補で、Root30件へ加算しない。SDK16MiB条件と標準stack停止は前段のまま。

当時のNode wide全範囲checkerは `/tmp/positive-a0-wide-node-exact-v2.patch` の未適用案。初回frozen artifact b004...は独立レビューで重複型名・未定義helper・unused警告が見つかり、修正とFixed/Hist試験追加を依頼中。未コンパイルの7試験を成功に数えず、Rootへは適用していない。A3capture2はsource/review段階、追加buildは空き容量確認まで待機。A4はexplicitV2 pure sizingを実装中で、private fence/version2許可はまだ変更しない。


2026-11-01後続（Root `5a2f18ba9`、A7資料 `da5c48c2e`）: 初回Node artifactの拒否を保持し、修正版64a36f7を `4753ee4be` へ統合した。A3 capture2（PALWTRC2）の最小work検査順序修正まで統合済み。同じRust sourceでNode wide10・row-profile9・classID1・旧Court12・capture/adapter19・旧fullrelation/expanded16、計67件が標準test stackで成功した。[件数監査](../../docs/evidence/positive-court-a0-2026-10-31/integration/wide-node-capture-count-audit-20261101.json)。実native受付・信用・実LLMの成功は含まない。

strict Clippyの初回7件のneedless_borrow失敗と、修正未適用のまま始めて停止した再検査を原本のまま保持した。実修正 `b89631b38` 後のSDK/Core/execライブラリClippyは成功した。[訂正監査](../../docs/evidence/positive-court-a0-2026-10-31/integration/wide-node-lint-correction-audit-20261101.json)、[修正後記録](../../docs/evidence/positive-court-a0-2026-10-31/integration/wide-node-capture-clippy-corrected-20261101.json)。誤ったcommit subjectを試験成功の根拠にしない。

`5a2f18ba9` は巨大object dispatchを11個のprivate noninline handlerへ機械的に分割した。元153 arm本文・156 variant・guard順・rollback wrapperの一致を独立source検査したが、ビルド・標準stack・goldenは未確認。[固定入力と検査器](../../docs/evidence/positive-court-a0-2026-10-31/integration/object-dispatch-stack-inputs/)。A4 purewide sizing `13e1fe213` はowner試験中で、新規integration13・scalar2・compile-fail1は成功報告、旧GEN価格全targetはParams生成のstack overflowで停止した原記録を保持中。Root stack修正の検証はA4終了後に行う。

A1 purewide admission `78e274630` はsource固定・独立レビュー中、Root未統合。A3 Hello2は固定plan CPU subsetのsource作成中。A7の実trained3モデルinventoryを統合し新manifest8件のサイズ/SHAをRoot再照合した。原context8192/8192/2048と学習済みcheckpointを維持し、旧失敗narrow成果物4件の欠損も記録する。inventoryは新たな推論・登録・全位置・Court・credit試験ではない。Panel wide collector/geometry-only依存追跡とbounded証拠組立ては別途実装中。native Assignment案の明示採択・actual role残予約owner・資金・P/全consumer・複数Node復旧と実LLM性能は引き続き未完了。


Root `c85d5a9c9` 結合初回: SDK16、Core integration7 targetsの53（新wide sizing13、旧GEN価格全14、旧close/range/GEN、状態/carriage golden2、T12の15）、既存ingress/archive/receipt7、scalar/golden/deadline/G14の14が、標準stack・同一sourceで成功した。一方、新admission10は8成功/2失敗であり、候補全体PASSとは扱わない。[初回件数監査](../../docs/evidence/positive-court-a0-2026-10-31/integration/wide-admission-stack-first-count-audit-20261101.json)。失敗binaryはgzip復元SHAまで固定した。両失敗はArtifactをcommitted operandへ誤算入した試験oracleに局所化し、規則を変えず、既存F7の真の2source coneへcap1拒否を移すtest-only修正をA1で作成中。

同候補のSDK/Core/exec strict library Clippyと新wide sizing integration target Clippyは両方成功した。[原記録](../../docs/evidence/positive-court-a0-2026-10-31/integration/wide-admission-stack-clippy-20261101.json)。SDKの以前停止した実Params経路は今回標準stackで成功した。static disassemblyの固定frameはcommon inner8224B、最大batch153296Bで、12関数を独立再計算した。[監査](../../docs/evidence/positive-court-a0-2026-10-31/integration/stack-split-prologue-audit-20261101.json)。旧771840Bは別sourceの参考であり、呼出全体・RSS・最小stack認定を意味しない。旧arm全文一致と既存回帰が、全156 variantの実行網羅を意味することもない。paired-original post-write rollbackとPanelLifetime共通guardのfocused testは別途作成中。

A3 Hello2はsource `1b14d1ecc`（固定plan CPU/session subset、直接CPU APIの事前admitはcaller責務）まで凍結し未実行。汎用artifact startupの割当前scanner/retained decodeはA1/A3で別途実装準備。A5は同block DAA/blue矛盾のDATA-only修正 `2e63b1421` を凍結したがnode側の新2試験は未実行。A2のsource journal新3試験とA6の実DBreopen/replay bridgeも準備中。いずれもnativeの権限・実精算・P・実LLM登録の完了へ昇格しない。


Root `7a6935de6` 固定候補: 修正後のHello2 actual OS childを含む6・旧cell12・旧capture4・widecapture7・run memory1の30、purewide admission10、Core integration positive_source_v2の40、actual paired post-write rollback1＋PanelLifetime service/pause2、native_funds DATA calculator17、wide producer7、計107件が標準stackで成功した。[件数監査](../../docs/evidence/positive-court-a0-2026-10-31/integration/hello2-source-clock-corrected-count-audit-20261101.json)。独立監査で重複なし・2189source前後一致・raw/archive/hash一致を照合した。最初のCore libfilter0件はguardで停止して不算入、正しいintegration40を別記録で実行した。最初の36件提案はmemory1とproducer7のtarget選択を訂正し、cell関連の実数は37（107の一部）である。

初回Hello2 childはmacOSのaccepted nonblocking socketにより5成功/1失敗、試験helperでblockingを明示しdeadlineを維持して修正した。追加Core fixtureの引数不足E0061も原ログを保存し、正常fixture(None,2)へ修正した。失敗・訂正・成功を別sourceに保持し、子helperのignoredを成功へ加算しない。新3fold回帰は実apply_objectとstate/journal/reservation復元・licence更新を検査するが、lifetimeの事前認証row fixtureはnative ingress/実抽選/全履歴の検証ではない。

同7a sourceのexec/positive-accounting-node strict library ClippyとHello2 integration target Clippyは成功した。[記録](../../docs/evidence/positive-court-a0-2026-10-31/integration/hello2-source-clock-clippy-20261101.json)。実trained3モデルharnessはstartup未上限・runtimequote・processRSS/timeoutを区別してsource統合したが、Rust compile/quote/推論は未実行。Panel one-pass tape/tracer/materialの12file案は初版のprivate型・課金順・hash前入力cap指摘を修正したr3の独立review中で、まだRoot未適用。A6実DBreopen→replayのsource6試験も未実行。全native job/role/funding/P/全creditconsumer/復旧/IBD/実LLM登録資格の完了は宣言しない。


実LLM後続 `cc22b0da1`: SmolLM2-360M/135M・Pythia-70Mの実trained artifactでCPU＋dense capture全leafの往復が3/3成功した。元context8192/8192/2048を維持し、自然文prompt16/decode1の短job、CPU＋captureは13.32/6.94/1.56秒。Pythiaの50304 logits全てi32超をi64のまま保持。4GiB測定cap拒否と8GiB別測定成功を原本で分け、独立auditはsource/binary/input/raw/RSSとSELF_CONSISTENCY_ONLY境界を確認した。[実LLM検証範囲](../../docs/design/palw/positive-court/VALIDATION-A0.md)。native登録・最大長全位置・Panel/Node全関係・role別性能/P/creditは未成立。

Panel r3は初回compileのchecked sum `?` 2不足をcc22で修正後、6件中3成功/3失敗（per-material source read capへ累積visitを流用したBudget）で停止した。必須run q予約・hash前RAM予約・quote内一時Vecも独立監査で検出し、r5別案をsource-review中。scopeを維持し旧失敗を成功へ置換しない。adopted43内のnativeclass登録配線はv10固定/mirror不足/旧facts依存の13consumer planをA1へ割当、新Assignment authority/P/fundingは未採択案と分離する。

Root既知履歴74 executableは2.57GBをgzip729MBへ完全保全し、実tempfile復元size/SHAを全件確認して元cache aliasesを解放した。[保全map](../../docs/evidence/positive-court-a0-2026-10-31/integration/history-binaries-preserved-20261101.json)。原raw/source/model/native archive/他owner cacheは保持。これは検証成功の追加ではない。A6 DB6件の実行は4GiBの開始余裕を確保してから行う。


2026-11-02 継続（Root `919309e93`）: Panel r5の独立したper-material cap、必須run/capture予約、hash前RAM取得、割当不要のnode_cost/quoteを統合した。旧cost式との一致・割当計測1件は標準stackで成功し、対象source前後一致を確認した。[leaf原記録](../../docs/evidence/positive-court-a0-2026-10-31/integration/panel-r5-leaf-20261102.json)。この1件はPanel全体の成功件数ではない。public Vec envelopeとProcess owner/shared absolute deadlineも統合したが、後続の同候補試験は別記録で扱う。

A6実DBreopenの初回buildは、依存`kaspa-wrpc-server`のcdylibがlibc++をlinkできずexit101で停止した。native14/exec17のruntime成功は0件。最終test targetだけの`-l c++`が依存cdylibへ届かないことを局所化し、元のraw/statusと実行前に復元照合した旧binary gzipを保持した。再試行はまだ行っておらず、stack増加・新target・C++再build・global RUSTFLAGSの変更は行わない。

A1は採択済RFC-0043内のnative class受付を接続中。登録受付後も旧rawfactsのwide非対応consumerへ流さない拒否境界を検査する。A3はboundedなlocal SourceProgram facade/bootstrapとgeneric startupのownerをsource実装中。A7のclass-onlyモデル解析はconstructor/native受付の前提資料がない範囲をNOT_RUNとし、元canonicalと最長解析probeを区別する。これらはRust実行・native登録・P・資金・creditの完了を示さない。新native Assignment案の明示採択は引き続き未受領。


同9193のexec結合buildはProcess試験の`&0..2`が`Range<&integer>`になるE0308で停止し、runtime0だった。test-only括弧修正 `ce016024d` 後は全指定targetのbuildが成功し、Panel one-pass/責任分離/必須予約の8件が成功した。しかしquoteのallocation試験1件は1回の割当を検出して失敗し、残scopeは未実行のまま停止した。原因は共通StepSpace `running_total`のmultiplicity Vec。validated programの16-block上限に従う固定配列へ移し、元の線形走査と加算・飽和順序、旧wire/fees/work値を保存した。V1 `from_program`の「既にvalidated」のcaller前提を新たに強制したとは扱わない。修正後の試験結果はこの失敗と別の原記録に残す。


修正後 `1c90300a4` 同候補でexec65、Core全関係10＋Vec境界2、旧StepSpace独立walk4、bounded artifact5、計86件が標準stackで成功した。[独立件数・pin監査](../../docs/evidence/positive-court-a0-2026-10-31/integration/positive-a0-r5-1c903-independent-final-audit.json)。ignored child helper1と過去候補のleaf1/Panel8は合算しない。4798/4898のsource/input pin、binary・native archive・raw hashを独立照合し重複0・問題0。元のcompile失敗とallocation失敗は原本保持し、旧quote失敗binaryもgzipの圧縮hashとメモリ展開の原size/SHAを確認して保全した。256Bは固定配列の型上の保存量であり、生成frame/RSSの実測値ではない。これらは小fixture/旧corpusの実行で、generic startupの完全な資源上限・実LLM Panel/Node性能・native登録・P・資金・creditを認定しない。

同1c903の4 library＋新integration targetsのstrict Clippyは全3コマンド成功、source/native前後一致。A1固定19fileの最終レビューでは登録scopeのblockerは未発見だが、旧FreePrompt入口のknownwide拒否をfence off/on共通にする追加修正を依頼した。A7class-only source2fileは独立レビューを通過し、typecheck/buildは次候補で行う。


後続source候補 `e2d5a43e8`: A1 native dispatch/full court2 mirror/SDK/processor/fold、early Attempt/rider拒否、ordinary/eval FPのknownwide拒否、実trained3の実署名・実UTXO carrierテストsourceを統合。stream r2は旧Vec recordのBorsh/hashを維持し、1claimずつ読取り・hashし、EOF/digest/ML-DSA完了前に裁定を公開しない。A3 SourceLocal74bd/owned params9b/compile quote54d+d410も独立source review後に統合した。すべてこの新候補の型検査・runtimeは未実行。logical quote/helperはhost leaseそのものではなく、whole loaded ownerによる取得・保持の接続は追加review中。RFC0004 dormant・全public preset None・旧wire/型ID・P/credit権限は維持する。

実trained3のclass-only原文脈解析は別source4e672で実行し、Py full4/Smol2はoff-element参照のみcap拒否、全体INCOMPLETEを保持した。[検証範囲](../../docs/design/palw/positive-court/VALIDATION-A0.md)。A6はsource355/HEADfddの別runでnative14＋DB Court/Source17の31件が標準stackで成功し、C++標準libraryの依存cdylib伝播問題を標準Cargo links overrideで解消した。新候補へ件数を加算しない。今後はRoot最終source起点の保全branchへA6最小DBtestsだけを合わせ、既存A6 checkout/target/profile/native cacheを再利用する同候補検証計画を調整する。元A6branch/source/rawは保全する。

容量確保では原モデル・source・rawを保存し、旧Core依存の実test executableとCPU/class解析binaryは全件actual tempfile復元size/SHAを確認してgzipへ保全した。現Cargo出力へ手動でRust libraryを戻さない。初回stem指定にnode無効execの再build不要な2中間ファイル32815679Bも含めたことは[訂正記録](../../docs/evidence/positive-court-a0-2026-10-31/integration/root-core-native-reclaim-correction-20261102.json)へ明記し、以降はcompiled fingerprintの実依存とRoot由来を照合して限定した。原reclaim記録は書換えていない。


後続 `352113d4c`: Loaded908/evidence37ab、OS-main stdio join9d828、既存R5へのPanel bridge973を独立source review後に統合した。新子テストは実File→Loaded→CPU I64 endpoint→実stdin/stdout→usage/guard/reapを構造化markerで検査する設計で、libtestのPASS数へ合算しない。Takeの有限EOFはOS pipe EOFではなく、自然exit0も主張しない。bridgeの2caseは旧binding用oracleを先に構成するtestで、captureのrun guard1回が全testのCPU1回やcold cacheを意味しない。全新sourceは引き続き型検査・runtime未実行。

現Loadedは完全なexpected/verified step bindingを前提とし、trace無しの最初のMiner起動が完成したとは扱わない。Node検査条件や旧APIを弱めず、別のlocal非serialized class/job metadata＋ordinary forward componentを後続incrementで実装中。新native Assignment/Result/役割・資金・Pの設計採択は依然未受領。Root最終source＋A6最小DBtestsを固定した保全branchで、native実登録・同sourceの回帰を先行して実行する。
