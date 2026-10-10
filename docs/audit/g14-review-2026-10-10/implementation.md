# G14レビューに基づく実装修正

基点: `323ea161a5b627431fb831b358e8d1f88457261d`（Claudeの統合ブランチ、ADR-0176/177 budgetとLG14-Bのmerge後）。修正ブランチ: `codex/g14-prosecution-bounds`。初回レビューの対象は`34b6c3f5e`であり、元の不具合の数値・logは[feedback.md](feedback.md)に固定して残す。

## 初回の資源修正

- `from_served_rows_v1`から、存在しないcacheフィールド・型を参照するinitializerを除去した。新基点のlegacy held署名検証関数にも成功時の`Ok(())`が欠落し、consensusのビルドがE0317で失敗したため、成功returnを補った。拒否条件・署名検証は維持する。
- public-prosecution gateへprogramを渡し、wire dtypeによらず1要素16 BのTensor表現、param layer instances、whole-scopeのderived values、operands、clone/一時領域、repetition/modulusごとのbatched projectionsとmetadataを保守的に計上した。無効programとprogramに一致しないrelation/budgetは価格計算前に拒否する。
- artifact wire budgetでも`per_layer`paramの全layer instanceを数えた。
- `max_response_bytes`はclaimの最大position数以内で到達するhistoryを使い、全non-derived outputとwire headersを数える。program全体の巨大windowを、そのまま短いclaimの保持responseへ掛けない。
- `max_retained_state`へ全positionのserved responsesとprosecution metadataを含めた。`max_commit_bytes`を独立させ、carrier fitが保持state全体をCommitClaimへ載せるものとして判定しないようにした。pipeline inputs/commitmentsとtyped memory/retrieval/compositeの集約も更新した。
- 1 sessionのcollateral demanderを64、1 claimのproof sealを64に制限した。既存demanderの重複要求・既存sealerの置換は可能。満杯での新しいjoin/sealはrowsを変更せず拒否する。seal件数も対象claimのBTreeMap rangeだけで数え、全claimを走査しない。**既に開いたdemandの公開回答・default期限と、別bondのdirect FileProofにはこの制限を適用しない。**上限は参加者の担保・報酬metadataに対するものである。

canonical OPV fixtureが旧admission listを新しいdeny-listへ渡し、対象classを拒否していた試験設定も修正した。既存のtest eligibility hookへ移し、deny-listは空にする。routing/historyの拒否試験は、context内のresponse見積りが縮んでもworst filingが実carrierを超えることと、nodeで登録を拒否することを確認する。

参加者制限は経済的な参加・bountyの選択肢に影響する。そのため「満杯でも新outsiderがpublic rowsからconvictできる」「満杯でも公開defaultが遅延しない」を回帰テストにした。この変更から全verifierへのbounty公平性や監視者インセンティブの完成までは主張しない。

## 初回の検証

| 最終チェック | 結果 |
| --- | --- |
| `cargo test -p misaka-palw-kernel --locked` | **222 PASS / 0 FAIL / 1 ignored**。既知の`F-C4R4-17`をignoredのまま明記 |
| `cargo test -p kaspa-consensus --lib g14_canonical_ --locked` | **24 PASS / 0 FAIL** |
| `cargo test -p misaka-palw-sdk --test coverage_p1_huihui_9b --no-run --locked` | PASS。API callerのcompileのみ。実9Bモデル試験は未実行 |
| 変更したcore/kernel/SDK Rustファイルのformat check、`git diff --check` | PASS。既存の大きなprocessor/test moduleは必要箇所のみ変更 |

[最終kernel log](evidence/kernel-after-fixes.log)、[最終canonical log](evidence/canonical-after-fixes.log)、[SDK compile log](evidence/sdk-api-check.log)、[旧OPV harnessの拒否](evidence/canonical-opv-original-harness.log)。既存のunused/dead-code警告は残る。

追加テスト: `misaka-palw-kernel/tests/g14_review_regressions.rs`（5件）。元のRAM/served-state反例、層ごとのartifactとRAM ceiling、projection repetitions、満杯join/seal後のpublic conviction、満杯join後のDA defaultを扱う。既存carrier testでもretained stateがcommit carrierより大きい場合を確認する。

## G14の残条件と変更の範囲

資源修正と下記の再計算経路はG14の完成に必要な実装である。累積開示などの残条件があるため、全面的な完成・activation承認には至っていない。

1. ADR-0177に従い、G14は正しいモデルを保有/取得したverifierを前提とする。配布拒否だけでmodel取得を保証したり、モデルavailability penaltyを復活させたりしない。
2. 計算・binding不正のexact convictionと、claim witness不応答の客観的defaultを区別する。
3. canonical node試験のOPV class eligibilityはtest hookである。post-genesis bond・public reads・signed carriers・terminalの試験であり、production onboarding/eligibility全体の完成証拠ではない。16-unit privacy scopeと任意の対象faultのlocalization、累積再構成predicate、real-scale v4 courts、正規eligibility経路、期限内包含、beaconと外部soundnessの残gateは別途閉じる。
4. RAMには、認証・shape確認より先にdecodeするresponseのwire ceilingも別に計上した。RAMのcopy/metadata allowanceは現在の実装に対する保守的な見積りであり、全profileのallocator peakを測定して証明した結果ではない。実model最大profileでの実測・独立レビューは必要である。保持stateもkernel claim rowsの上限であり、provider registry/chunk lane等を含む全chainの保存上限ではない。責任期間後のglobal pruning/cleanupは本修正の範囲外。
5. 最新基点にはQ/B/R/F engineがmerge済みである。初回レビュー文書の「engine未統合」はその旧snapshotの状態であり、このHEADの欠落として扱わない。最新統合後の会計・経済安全性・期限の受入検証は引き続き必要である。
6. `per_layer`のartifact budgetを直したため、そのparamを持つdormant planのbudget bytes、plan root、class IDが変わり得る。derived bounds/APIにも`max_commit_bytes`を追加した。既存networkのactive rulesetへ適用する変更ではなく、事前登録fixtureの再生成と合意reviewを要する。fence/activation heights/fingerprintの変更は行わない。

Claudeへの依頼: このbranchの修正を比較・取り込み、最新の統合HEADでも試験を再実行してください。修正した資源反例のPASSを、上記G14残条件や外部reviewのPASSへ流用しないでください。

## 公開commitmentから不正を発見する経路

Claudeの統合commit `7d1c82324394198a7efb6226570d09b9e12f8f50`へmergeした上で、登録モデルを保有するverifierの再計算を実装した。ADR-0177 D7の前提はユーザーが再確認している。producerからモデルを強制取得する経路は追加しない。

`FreshVerifierV1::reexecute`は、公開job、program、登録artifactからdependency順にnodeを計算し、最初に異なるcommitmentを見つける。producerの`node_value`も、不正位置のヒントも使わない。それ以前のnodeは公開commitmentと一致するため、最初の不正nodeのinputsを認証できる。新しい`CommitmentMismatch` courtは認証されたinputsから値を独立に計算し、producerのoutput openingを要求せず、公開output commitmentとの不一致をconvictする。正しいclaimへの同じ告発は`NoFault`となる。

`OutsiderV1::check_computation`はsingle programとpipelineを扱う。pipelineのinputも公開job、R、再計算済みupstream outputsから構成する。新しい`EdgeCommitment` filingはproducerのinput openingを必要とせず、既存のsigned FileProofでadjudicateする。`SpecOutsiderV1::check_computation`はmemoryとcompositeの計算・decode・edgeを扱い、memoryの次stepへ認証済みのstate writesを引き継ぐ。最初のstateは公開line headまたは登録M0から取得する。retrievalのsnapshotには既存の認証・DA/default経路を使う。

旧`check`はDAの値開示を要求する経路として残る。新APIの`Clean`は計算・output/stateの判定であり、DA義務を免責する値ではない。モデル取得エラー、計算不正、DA defaultを同じ結果へまとめない。

資源gateでは、exact再計算、full output commitment court、全recordの認証、pipeline全stage/edge、memory全step、composite全componentの仕事量を計上した。retrievalのRAMは1 sliceだけでなく、検出に使う全snapshot・Merkle tree・sort・temporary valuesを含む。raw Tensorのdtype範囲外i128がwireの切り詰めで同じhashになるaliasも、計算前の認証で拒否する。

| 検証 | 結果 |
| --- | --- |
| 全kernel試験 | 232 PASS / 0 FAIL / 1 ignored。F-C4R4-17は既知FAILとしてignored |
| 全25 primitiveの各nodeへの嘘 | producer valuesを読むとpanicするartifact-only verifierで、位置を知らせず発見・court conviction。honest recordへの同じproofはdismiss |
| 63-position claimの62番目の嘘 | demand 0、disclosure 0。rowsから復元したledgerで発見し、通常のFileProofでconvict |
| pipeline | stage計算、upstream edge、R、画像input、decodeの嘘を値開示なしでconvict。honest edgeと偽upstream openingはdismiss |
| canonical node試験 | 26 PASS。Panel/OPV × Final前後 × quantize/garbage/decodeの12ケースを追加。新しいPanel外bondが自分のpruned nodeのpublic readsからfilingし、実bondをslash |
| typed node試験 | 5 PASS。pruned nodeのFinal後memory convictionとline rollback、composite model-stage convictionをproducer valuesなしへ強化 |
| SDK API | `coverage_p1_huihui_9b --no-run` PASS。実9Bモデル試験の実行結果ではない |

[全kernel](evidence/kernel-all-reexecution-final.log)、[canonical node](evidence/canonical-reexecution-last.log)、[typed node](evidence/typed-node-reexecution-final.log)、[SDK API](evidence/sdk-reexecution-api.log)。Final後のconvictionでは履歴上のphaseはFinalのままであり、convicted flag、経済liability、rollbackで結果を確認する。

## Claudeへの追加フィードバック

全域としてはまだ不十分である。今回の値開示なしのconviction経路を、累積開示predicateの代替とみなしてはならない。`cumulative_scope_allows_v1`は依然としてstubであり、旧DA responseの和からweightを再構成するF-C4R4-17が残る。モデルに依存する値をcommitmentのみで回答するprotocol、small-value court、全claim/aliasをまたぐscopeを、反例そのものと1 outsiderの追及可能性の両方で閉じる必要がある。

`3730cc90f382341635322fd21b77b56c9db41fc5`をmergeした。上表は7d1c82324を基点にした履歴であり、最新統合後の結果は以下に記録する。fence/activationは変更しない。

## 最新統合版のsegmented経路の修正

対象は3730cc90fのK2-TIR-v4/v5である。未compileのK2S WIP `4a6f20aaf`は、この検証には含めていない。

`prepare_reexecution_v1`は公開jobのprompt root、fed ids、登録param commitmentsを確認し、producerのnodeを読まずにstreaming再計算する。保持する結果はposition rootsと最初のdecode不一致だけであり、DAの各要求でモデルを再実行する必要はない。`OutsiderV1::check_segmented_computation`にも接続した。v5ではjobのids/countを入力として構成する。正しいclaimはproducer valuesなしでCleanとなり、異なるsegmentには公開pathのdescentを適用する。

segmented demandは最大64人のcollateral参加者とし、既存demandへのjoinでも1 bondあたり4 open sessionsを守る。拒否されたjoinはreserveを増やさない。参加者満杯でも、Panel外の新しいbondの直接proof、response、DA defaultを妨げない。bondごとのcountも当該claimのBTreeMap rangeに限定した。

v4 gateではprogramを検証し、program root、grammar、relations、boundaries、budgetsを再導出して照合する。RAMはdtypeのwire幅ではなくi128で数え、2 position、streaming履歴、node operands、evaluation temporaries、Merkle tree、encoded buffersを含む。verifier内部の全param cacheを除き、evaluatorへ渡すparamの複製もなくした。登録モデルをdiskから必要なrelation分だけ読む実装を前提とし、任意に全モデルをi128で保持するcallerのstoreは含めない。これはworking-memoryの保守的な式であり、実機のpeak測定ではない。

retained stateには全positionのbitmap、最大64人のdemand/held metadata、proof seals、claim headersを計上する。v4のresponse payloadはblocksにあり、ledger rowsには保存しない。`max_commit_bytes`はgenerated idsを含む独立のcarrier boundである。concurrent sessionsの2は1 prosecutionの局所position数であり、全claimのmetadataは`max_retained_state`で別に計上する。局所化roundsも、適応的な最大10 path probes＋position demand＋filingの12に修正した。

raw Tensor providerにもcanonical range/length確認を追加した。I8の値へ256を加えた同じwire hashのaliasは、paramなら取得エラー、positionなら未認証materialとしてdemandとなり、false convictionを作らない。

| 最新の検証 | 結果と限界 |
| --- | --- |
| 全kernel | 259 PASS / 0 FAIL / 1 ignored。既知F-C4R4-17は未解決 |
| canonical node | 26 PASS。post-genesis・fresh pruned nodeの既存試験を再実行 |
| typed node | 5 PASS。pruned nodeのmemory conviction/rollback、composite stage、restart、eligibilityを再実行 |
| SDK | tiny BERT encoderとtiny XLM-R headの全element court試験1 PASS。9B APIは`--no-run` PASS |
| segmented node | baselineの14 PASS（history 8kを含む）。修正後は13 PASS（未変更の長時間history試験を除く）と、再計算へ強化したv5試験1 PASS |
| fault位置を教えないnode試験 | 共謀するproducerと他bondを前に、1 outsiderがpublic RPC recordと自分のモデルからrootsを作り、blocksの応答だけを適応的に読み、通常のsigned FileProofでconvict/slash/bounty。distinct position demandsは12以下 |
| 9B・8k shape fixture | RAM 55,528,466,948 B、全claim保持state 276,099,584 B、public bytes 21,843,011,715 B、12 rounds。既存RAM/state/carrier ceiling内。weights_loaded=false、artifact root syntheticのshape fixtureであり、実9Bモデルの実行・peak・期限保証のPASSではない |

[全kernel](evidence/seg-final-kernel.log)、[canonical node](evidence/seg-final-canonical.log)、[typed node](evidence/seg-final-typed.log)、[segmented node](evidence/seg-final-node.log)、[v5 replay](evidence/seg-final-v5.log)、[統合直後の全14 segmented node](evidence/merged-segmented-node.log)、[9B shapeの資源式](evidence/seg-bounds-replay-all.log)、[SDK tiny encoder/head](evidence/seg-final-sdk-encoder.log)、[SDK API](evidence/seg-final-sdk-api.log)、[fresh replayとmodel alias拒否](evidence/seg-streaming-final.log)。

### 残条件へのフィードバック

1. **累積開示scopeは未完成。** `cumulative_scope_allows_v1`はstubのままで、F-C4R4-17もignoredである。K2S WIPのwhole-value courtをcompile・soundness/completeness試験し、small masked valuesとlarge valuesのhiding protocolを実際のresponse、全claim/aliasのscope、reward判定まで接続する必要がある。再計算によるconvictionのPASSを、強制開示のprivacy predicateのPASSへ流用しない。
2. **最大DAの期限は未完成。** 9B・8kの1 positionは2,294 parts、約2.34 GBである。現在の固定20 DAAのresponse期限と、Final後の`court_deadline + proof_grace`だけの開始判定は、12 roundsとchunk inclusionの完了時間を保証していない。tag-113の最大Respond、chunk数を数えた期限、expiry直前のadaptive追及を実ノードで検証する必要がある。
3. **全familyと包含は未完成。** legacy filerの実ノード失敗、pipeline等のcanonical public entry/own-node reads、production eligibility、junk FileProof下のcourt budgetと期限内包含は残る。今回のgenesis cardを使うsegmented試験だけでC1/C2/C7の全域PASSとはしない。
4. **F-MEAS-07は現snapshotでは修正済み。** `required_reservation`は自己還流分を除くnet penaltyから導出している。旧handoverのgross-slash指摘を現在のbugとして再掲しない。p=0等のPESG/経済gateは別途残る。

この段階の判定は、登録モデル保有者による告発経路と資源式の改善がverified、G14全域は未完成である。armingの許可や外部soundness reviewの完了を意味しない。

## K2Sのsmall-value courtを統合・修正

未検証だった`k2/real-scale`の`4a6f20aaf`を取り込み、DAで値を開示せずcommitmentだけを返す小型nodeのcourtを完成させた。基点は引き続きintegration `3730cc90f`である。ユーザー確認済みのADR-0177 D7（正しい登録モデルを保有するverifier）を維持している。

WIPの`SegMaterialV1::own()`は削除した。同じpublic providerがproducer値と「verifier自身の値」の両方を供給できると、独立再計算をせず嘘をCleanにできるためである。verifierの値は認証済みjobと登録param commitmentsから内部で再計算し、一致した値だけをcourt inputsに使う。block readerはDA assemblerの明示的なnode commitmentsを保持し、省略値のzero placeholderをhashして代用しない。

不正位置は公開rootのdescentで特定する。局所checkは公開応答が揃い、認証できてから追加のprefix再計算を行う。応答待ちの再試行で毎回モデルを再実行せず、選択された2 positionの非公開値だけを保持する。任意の後方positionから、それ以前の全positionを暗黙に取得する経路は除いた。element proofだけを構築するAPIは、全入力値をpublic rootで認証して使い、correctness verdictを返す独立再計算とは区別する。

`WholeValue` courtは、そのnodeに価格付けされた場合だけ受理する。1 MiBのfiling上限に加え、全output要素のMatMul contraction、TopK、output hashを含むwork上限を適用する。小さい偽proofで価格付けされていない巨大nodeの全計算を起動する経路を拒否した。正しいwhole-value filingはNoFaultとなり、偽のcommitmentは認証されたinputsから再計算してconvictする。

wide128とwindowed dense/MoEの試験では、モデル依存値をすべて`Withheld`としてwire応答から除き、公開commitmentsとverifierのモデルだけから未知の不正位置を特定し、ledgerの通常FileProofでconvictした。実ノードの共謀試験も、このcommitment-only応答をblocksから読む。入力差し替え・別jobのtrace・garbageのnode試験も、fault位置をAPIへ渡さない再計算へ変更した。

RAMは公開応答を保持したまま行う追加再計算を含めて再算定した。decoderのcurrent/previous positionとhistory rows、局所position、selected valuesを同時に計上し、1-position encoderでは存在しないprevious positionを数えない。9B・8kのshape fixtureはRAM **67,191,831,824 B**、public **21,288,122,497 B**、保持state **275,829,248 B**、1 position **2,029 parts**、12 roundsである。これはweights未ロードの資源式検査であり、peak実測・実行時間・deadline保証ではない。

BGE-base 512はRAM **69,698,109,792 B**で、現行上限68,719,476,736 Bを超える。BGE-large 512は184,119,637,904 B、reranker-large 512は187,858,961,568 Bとなる。SDK試験の旧「これらはPASS」という前提を修正し、受入拒否をREFUSEDとして記録した。上限を緩めてPASSにはしていない。SDKがclaim carrierとretained stateを混同していた8 KiB assertionも、独立のcommit boundと全stateのpolicy boundに分けた。

### この統合で残る重要な指摘

1. **small-value courtの完成だけでは累積開示scopeは閉じない。** 大きなモデル依存値は依然clear応答であり、flat hiding tilesは未実装。旧routeの`cumulative_scope_allows_v1`とF-C4R4-17も残る。
2. **`court_scope_complete()`はreward gateではない。** WIPの「falseなら報酬を得ない」という文言は実装に対応していなかった。現時点ではreportにすぎないとsource/design文書を訂正した。これを実際のreward判定へ接続する作業は必要である。
3. **DA期限は未解決。** 2,029 partsのresponseと最大12 roundsを、固定20 DAAと現在のliability開始条件で必ず完遂できる証拠はない。誠実なproducerを帯域不足だけでdefaultにしない期限設計と、tag-113 terminalの実ノード試験が必要。
4. **大型encoderのRAMも未解決。** 現行gateの拒否は安全側の結果であり、その構成への対応完了を意味しない。局所materialの保持方法を改善し、実機peakも検証する必要がある。

作業中にupstream integration `0b73fd33f`を確認した。追加のverifier-pay/PESGとF-B1（generation lengthを満たさず短いclaimがfull rewardを得る問題）は、本段落の基点にはまだ統合していない。次の統合・再検証対象であり、旧snapshotの結果から解決済みとはしない。

### small-value統合後の検証記録

| 対象 | 結果 |
| --- | --- |
| 全kernel | **260 PASS / 0 FAIL / 1 ignored**。F-C4R4-17は未解決。全対象nodeの嘘のconviction、honest Element/WholeValueのNoFault、無価格のWholeValue拒否を含む |
| segmented実ノード | **13 PASS / 0 FAIL**。最新RAM式とcommitment-only応答、共謀、input/borrowed/garbage、decode、reorg/restart/pruned import。長時間の8k-history試験はこの最終runから除外 |
| SDK実configurationの資源検査 | **1 PASS**。BGE系のRAM拒否も明示的に確認する試験であり、これらのprofileへの対応PASSではない |
| 9B/8k shapeの再算定 | **1 PASS**。実weights・peak・期限の検証ではない |
| format / diff | 変更Rustファイルのrustfmt checkとgit diff --checkがPASS |

[全kernel](evidence/scope-verified-kernel.log)、[segmented node](evidence/scope-verified-node.log)、[SDK configuration](evidence/scope-sdk-config-final.log)、[9B RAM再算定](evidence/scope-9b-ram.log)。

長時間の別runも終了した。8k-historyはPASS（元batchはmaterialの重複readを数えた別test 1件がFAILであり、修正後は上記13件で再検証）。SDKのtiny BERT/XLM-R courtとgeometry全軸sweepもPASSした。SDK元batchのconfiguration testは旧「BGEは受入可能」というassertionでFAILし、その修正後のconfiguration reportが上記1件でPASSしている。別runの結果を、単一の全件green logであるかのようには扱わない。

[8k-historyを含む初回node batch](evidence/scope-node-initial-with-history.log)、[SDK tiny court・全軸sweepと旧config assertionの失敗](evidence/scope-sdk-sweep-before-config-report-fix.log)。型別courtではtiny BERTの1,155件、XLM-R headの1,209件のhonest element filingsが棄却され、各fixtureのembedding/projection/mask/attention/outputへの不正がconvictされた。


## 0b73fd33f統合とF-B1の修正

integration `0b73fd33f`を`c4966e5df`で取り込んだ。新しいverifier-pay、保証金safety項、PESG検出確率の試験を確認し、F-B1を修正した。固定報酬のjobに対し、指定数より短い正しい生成結果でも満額を得る受入規則が原因である。

現行GreedyにはEOS停止規則がないため、`max_new_tokens`を**必須の生成token数**として照合する。wire field名は保持し、単体program、pipeline stream、segmented decoderが同じ`generation_length_matches_v1`を使う。typed compositeのgenerative componentも単体programのbindingを使う。非生成型pipeline/encoderのzero-generation規則は別扱いである。短い実行が計算自体は正しくても、jobを完了したclaimとしては受け入れない。これにより、convictionが作れない正しい短縮計算を報酬対象から除外する。

これはdormant protocolの意味の変更であり、既存の「最大値で任意停止」との互換性はない。EOS等を許す場合は停止規則自体をjobへcommitし、courtで検証する別設計が必要になる。fence、network id、報酬額を変更していない。

| 検証 | 結果と範囲 |
| --- | --- |
| 最新統合版・修正前のnode `g14_` | 136 PASS / 0 FAIL / 1 ignored。長時間の8k-historyは対象外 |
| 保証金policy | 35 PASS / 0 FAIL |
| verifier-pay + PESG | 11 PASS / 0 FAIL / 6 ignored。PESGの長時間全列挙は未実行 |
| F-B1修正後のkernel | 272 PASS / 0 FAIL / 7 ignored / 2 filtered。短い・一致・長すぎる生成を単体、pipeline、segmentedで試験。既知F-C4R4-17とPESG全列挙はignored。長時間Merkle全leafと9B shape再算定は前段で検証済みのため除外 |

[統合node](evidence/next-integration-node.log)、[保証金](evidence/next-integration-budget.log)、[報酬・検出](evidence/next-integration-kernel.log)、[F-B1 kernel](evidence/length-kernel.log)。全域のscope、期限、legacy追及、production eligibilityの残条件は、これらのPASSでは閉じない。

F-B1の実node回帰も1 PASS。正しいproducer署名とsalted sealを持つ1-token claimを、2-token jobに対して実際のmempool→template→blockに載せた。mempoolの構造・署名検査は通過するが、block foldは短縮claimを記録しない。同じjobの正しい2-token claimは記録されFinalへ到達する。最初のbatchは「mempoolが意味検査まで行う」というtest側の誤った期待で1件失敗し、既存13件はPASS。その期待を実装の責任境界に合わせて修正した1件を再実行した。

[実nodeの修正済み回帰](evidence/length-node-binding.log)、[初回batch（13 PASSとtest期待の失敗）](evidence/length-node.log)。


## legacy public filerの追加監査

`g14/legacy-filer`の`51d026bc9`を`e9eee4911`で統合した。初回実node試験は **1 PASS / 7 FAIL** を再現した。

中央の1 leafが一致したときに未確認の前半を正しいと扱う局所化は不健全だった。単独の不正leafの後で正しい値に戻るtraceを見逃す。旧unit testも`leaf < lie`という単調な不一致だけを生成していた。小型claimのfallbackを連続範囲比較に変更し、離れた位置の回答では未確認prefixを進めないようにした。最大要求数は`ceil(leaves / 1024) + 2`であり、34-session枠に収まらない大型claimはUnjudgedとする。大型claimには既存LG14-Bの認証済みsubtree descentを本番filerへ接続する必要があり、ここを完成と見なしていない。

公開応答のreaderにも認証を追加した。`answered(unit)`という台帳の状態だけでは、同じunitを名乗る過去のすべてのcarrierのbytesは認証されない。不正な応答が先にcacheを埋めると、後から正しい応答が来てもverifierが追及を止める危険がある。bindingをclaimのrootへ再認証し、rangeの位置・長さ・Merkle pathを検証してから保存・比較する。不正なbytesではlocalizerを更新せず、verifierの再計算も起動しない。

Final待機とreorg試験も修正した。前者は実際のrearmed deadlineを待つ。後者は両branchが同じFinal workを持つpost-Final reservationでundo/restoreを試験する。未Finalのheld branchは、Final済みの別branchに対して経済的に劣るため、heartbeatを増やしても本番のstrict-economic-win規則では復帰できない。この規則を緩めてtestを通していない。

追加の残条件として、claim共通のlive reservation上限64・生涯上限256がある。1つのSybil reservationを置く試験だけでは、共謀者が枠を埋めた場合にも新しいoutsiderがDA局所化を開始できるとは証明できない。proof優先は、未取得のmaterialを取得する経路の代用にはならない。詳細は[legacy設計ノート§13](../../design/palw/legacy-route-g14-filer.md)に記録した。

### legacy修正の検証記録

| 検証 | 結果と範囲 |
| --- | --- |
| core localizer / filer | **10 PASS**。単独の不正leaf、離れた複数の不正、正しいsuffix、不正carrierを拒否した後の正しい応答を含む |
| kaspad共通filer | **5 PASS**。default feature（EVMを含む）の`--lib palw_fraud_filer`。容量整理後の再実行 |
| reservation fold | **7 PASS** |
| 実node E2E | 最終batch **7 PASS / 1 FAIL**。reorg harnessが本番の浅いtie-window（2 DAA）を超えた1件を修正し、個別再実行 **1 PASS**。単一batchの8/8成功とは記載しない |
| A2U | **3 PASS**。未有効化のfenceとADR-0175を維持 |
| shipping repin drift-only | **差分なし**。検査対象のpinとgateがすべて通過 |

実nodeの7件は、Final前のconviction、Final後のconviction、非応答のDA default、honest dismissal、別bondのreservation併存、restart、未有効化の対照試験である。DA default試験は、他の正当な回答義務を満たした後で対象sessionの実際のdeadlineを実blockで超える。reorg再試験は両branchのFinal workを等しく保ち、必要なblue workを最小限だけ増やして本番tie-window内でundo/restoreする。

[初回再現（1/7）](evidence/legacy-baseline-node.log)、[core](evidence/legacy-auth-core.log)、[fold](evidence/legacy-final-fold.log)、[最終node batch](evidence/legacy-final-node-after-cache-clean.log)、[修正済みreorg](evidence/legacy-final-reorg.log)、[A2U](evidence/legacy-final-a2u.log)、[repin](evidence/legacy-repin.log)、[repin test詳細](evidence/legacy-repin-harvest-tests.log)、[repin lib詳細](evidence/legacy-repin-harvest-lib.log)。

途中のbuildとnode runはディスク容量不足でも停止した。作業専用targetの再生成可能なincremental/codegen cacheを整理して再実行した結果が上記であり、容量不足をコードのPASSまたはFAILとは扱わない。未実装の大型探索・fused court・reservation飽和・包含期限を、これらの小型fixtureのPASSで閉じない。

[kaspad filer再実行](evidence/legacy-final-filer-lib-retry.log)。`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test -p kaspad --lib palw_fraud_filer --locked`は5件すべて成功した。残存warningはこの検証で修正対象としていない。

## beacon source-set freezeの統合

upstream `7d8c31270`（実装`91eb1bfd4`）を`b6a90a0d0`で統合した。epochの`release_daa`を初めて越えるblockで、parentから導出したeligible source setを固定する。後の登録・deny・DA失効でverifierが異なるsetを再導出しない。固定setをengine root、carriage、delta 174へ含め、reorgとrestartで復元する。有効化fenceは維持する。

`misaka-palw-panel --test stages`は **7 PASS**、`kaspa-consensus-core --test rfc0010_production_fold`は **22 PASS**。固定後の変更、reorg、restart、改ざん、期限後のprune、上限超過での拒否を含む。この追加fixtureの導出入力はReference setであり、読出しはChain経路を使う。実modelを正規OPV eligibilityへ登録してfreezeからdrawまで実nodeで通した試験ではない。

[engine stages](evidence/beacon-freeze-stages.log)、[production fold](evidence/beacon-freeze-fold.log)。この統合はsource setの後変更という残件を修正するもので、G14のscope・追及期限・大型filerの残件を閉じるものではない。

統合後の`--shipping --drift-only`も **差分なし**。検査対象の全pin・gateを通過した。[repin結果](evidence/beacon-repin.log)、[integration tests詳細](evidence/beacon-repin-harvest-tests.log)、[lib tests詳細](evidence/beacon-repin-harvest-lib.log)。

## legacy履歴paginationとfresh pursuitのbackfill

`aa99174ae`のnode readerには、DAA差をselected-chain block数の上限とし、件数上限で止まった走査を完了と扱う経路があった。1 DAAに複数blockが入ると過去の回答を読み落とし、tipの`answered(unit)`だけを見て`AwaitAnswer`に留まる。別claimが後からMismatchになった場合も、そのclaimがPendingだった間の走査は回答を収集していないのに、別caseの完了watermarkを継承していた。

実際に使うnode readerを`walk_accepted_lifecycle_page_v1`へ変更した。1ページの件数は固定上限で制限し、未読の次blockを返す。要求floor又はchain rootへ到達した時だけ完了とする。header、acceptance、merged block、selected parentを取得できない場合はerrorとして返し、完了範囲を更新しない。新しくMismatchになったcaseはbackfillを開始する。

ページを跨いでも開始時のtipとDAAを維持し、走査中に到着したblockは次のwalkで読む。開始tipが現在tipのancestorでなくなった時は、oldest pursued claimのacceptanceまで新しいbranchを読み直す。最近のincremental floorだけでdeep reorg後の古い回答を飛ばさない。成功したpartial pageの認証済み回答は、backfillの終了前でも追及に利用できる。

読出し時にもclaim execution rootへ認証し、現在の連続範囲localizerが実際に読むunitだけをmapへ保存する。binding 1件と最大32範囲／pursued claimであり、同じunitの重複carrierや任意の位置・幅のrangeを並べてもcache件数は増えない。これは本fallbackのcache件数の上限であり、未接続のLG14-Bや全node RAMの完成を意味しない。

| 検証 | 結果と範囲 |
| --- | --- |
| `cargo test -p kaspad --lib palw_fraud_filer --locked` | **7 PASS**。新規・後発pursuitのbackfill、partial pageでwatermarkを進めないこと、cache対象unitの上限を含む |
| `cargo test -p kaspad --lib accepted_objects_walk_tests --locked` | **4 PASS**。実際のConsensus API adapterとpaged readerを使い、8 block / 3 DAAの取りこぼし、ページ境界、新規tip、reorg、header/acceptance/block/parentの取得失敗と復旧を確認 |

[filer](evidence/filer-history-book-final.log)、[reader](evidence/filer-history-page-final.log)。default feature（EVMを含む）、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`で実行した。レベルは **V-unit**。実nodeのlate startからこのserviceでconvictionするC2全経路は未検証である。node-policyだけの修正で、consensus encodingやfenceは変更していない。

恒久的にprune済みの回答の復元は残件である。`palw_state_v2.rs`のDA admissionは既回答unitの再要求を`DaUnitAlreadyAnswered`として拒否する。取得できない古い回答を完了と偽る経路は塞いだが、claimの責任期間中の保持・公開取得又は認証付き再要求と、その期限まで閉じる必要がある。

## LG14-B producer応答のDA worker接続

`4de664175`でも、実サービスの`palw_da_claim_answers_v1`は`LegacyHeldV2`を「tag 158専用」として拒否していた。pure responderはあるがdue dutyから呼ばれず、honest producerが対応materialを保持していても応答を作れない。

既存の`rcore_da_answers_v1`から、materialの検証・必要ならjob再実行・memory ledger予約を維持してtag-158 responderを呼ぶよう変更した。base0-codecのfold保持はretained levelと必要なblock replayで、dense保持は自身のtile hashからstep treeを作る。checkpoint要求には保持checkpoint leaf又はchunkからtreeを作り、CKWは既存のleaf proverを使う。scopeとclaim bindingを先に検査し、作った応答をconsensusの`palw_legacy_held_check_answer_v2`で認証する。署名前にclose ceiling、署名後にcarrier rideを検査する。既回答・session終了後のtag 158はqueueから外す。tag 55の既存builderも共通dispatchから使う。

| 検証 | 結果と範囲 |
| --- | --- |
| `cargo test -p kaspad --lib p2_7_disclosure_policy --locked` | **8 V-unit PASS**。dense保持のstep node・checkpoint node・CKW、planted materialの検証と再実行、model-copy CKW拒否、違うclaim/unit・unsigned・close ceiling超過の拒否、既存のDA policy |
| `cargo test -p kaspad --lib lg14b_ --locked` | **6 V-fold PASS**。honest foldの試験は実workerと共通署名builderでstep/checkpoint/CKWを作り、transitionでsessionが閉じることを確認。残る5件は既存pure planner/terminalのfold回帰 |
| `cargo test -p kaspad --lib tir_court_e2e --locked` | **19 PASS**。tag 55のIR event・step・dense/fold responderと既存courtの回帰 |
| `cargo test -p kaspad --lib the_panel_pursues_a_named_leaf_serves_its_evidence_and_answers_the_held_court --locked` | **1 PASS**。既存のsource wiring pinを新dispatchへ更新 |
| `cargo test -p kaspad --lib palw_fraud_filer --locked` | **7 V-unit PASS**。履歴/cache/filerの回帰 |

[DA policy](evidence/legacy-responder-policy-final.log)、[LG14-B fold](evidence/legacy-responder-fold-final.log)、[IR回帰](evidence/legacy-responder-ir-final.log)、[dispatch pin](evidence/legacy-responder-dispatch-pin-final.log)、[filer](evidence/legacy-responder-filer-final.log)。default feature、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`で実行した。初回は新しいnode-local answer enumに未対応のIR test patternを修正した。dense CKWのfixtureもleaf 0のembeddingを要求してscope検査に拒否されたため、対象を計算leafへ変更し、embedding要求の拒否を別途assertした。model-copyの開示制限を緩めていない。

これはproducer応答の接続である。outsiderの共通filerはまだLG14-B descent・tag 159・fused terminalを使わず、公開tag-158履歴の収集も未接続。上記fold fixtureはfree-prompt worker factsを使っており、実serviceのtickからcanonical node上で完走するV-node試験ではない。非base0 codec、全familyの正規eligibility、最大profileのRAM・時間・期限内包含も完成判定していない。consensus encodingとactivation fenceは変更していない。

## LG14-B outsiderの共通filer接続

前節の未接続状態は`42d634926`時点のsnapshotである。共通filerの実service tickから、base0-codecのown replayを所有する`PalwLegacyReplicaV2`とLG14-B controllerを呼ぶよう接続した。producerのcaptureをverifierへ渡さず、自身のreplay予約とcaptureを保持する。foldはretained levelと必要blockだけを読む。denseは自身のstep hashからtreeを作る。treeのleaf数・rootとown bindingを照合し、公開bindingのstep countが異なる場合はjob/count terminalが必要としてUnjudgedに留める。

公開履歴readerは選択中のunitに一致するtag 158だけをclaim execution rootへ再認証し、mapへ保存する。次の既回答unitを選ぶと古いhistoryのbackfillを再開する。学習済みfrontierはdescent round数を上限として保持し、矛盾する重複を拒否する。CKWは1件だけ保持する。reorgではbinding・frontier・witnessとwalk/cacheを破棄する。case終了時はown replayとmaterial予約も解放する。frontier学習とdescent・terminal生成はblocking workerで行う。

first divergent leafがnon-fusedなら、公開bottom hashとfrontier、自分の登録modelと一致prefixからtag 159を作り、courtのverdictがExecutorGuiltyであることを確認する。embeddingのmodel-copy CKWを要求しない。fusedなら認証済みCKWを取得し、自身のhistoryからfused openingを作り、courtがGuilty又はNeedsDissectionと判断したaccusationだけを生成する。署名前にclose ceiling、署名後にcarrier規則を検査する。own courtが開いたらheld loopへ進行を委ね、on-chain conviction/defaultでsettleする。queue中でもchainのoutcome・既回答状態を読み、重複したreservation/demand/terminalを除く。

step treeの一致だけでexecution MismatchをHonestにしない。checkpoint・trace・job/countの別terminalが必要としてUnjudgedに留める。これはその違反の追及完成ではなく、未確認の不正をHonestへ誤分類しない検査である。

| 検証 | 結果と範囲 |
| --- | --- |
| `cargo test -p kaspad --lib lg14b_ --locked` | **7 V-fold PASS**。actual node controllerがreservation→binding→frontier→tag 159でgather／matmulをFinal前後にconvict。既回答の公開historyからfused CKWを学習しactual terminal→held courtでconvict。actual controllerのfrontier／CKW session非応答がDA defaultになる。honest claim、署名前ceiling、unsigned拒否、step一致だけでMismatchをclearしないguard、below-fence対照も含む |
| `cargo test -p kaspad --lib palw_fraud_filer --locked` | **8 V-unit PASS**。後から選択した既回答unitのbackfillと既存book／linear fallback回帰 |
| `cargo test -p kaspad --lib accepted_objects_walk_tests --locked` | **5 V-unit PASS**。paged Consensus API adapterでtag-158改ざん・重複・未選択unit・異なるclaim rootを拒否し、正しい選択回答を失わない |
| `cargo test -p kaspad --lib p2_7_disclosure_policy --locked` | **8 V-unit PASS**。producer DA workerの既存policy回帰 |
| `cargo test -p kaspad --lib tir_court_e2e --locked` | **19 PASS**。tag-55 IR responder/court回帰 |

[controller fold](evidence/legacy-filer-descent-fold-final.log)、[book](evidence/legacy-filer-descent-book-final.log)、[public reader](evidence/legacy-filer-descent-reader-final.log)、[producer](evidence/legacy-filer-descent-producer-final.log)、[IR](evidence/legacy-filer-descent-ir-final.log)。default feature（EVMを含む）、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`で実行した。初回reader testのlocal import不足を修正して再実行した。初回compileのbackend traitに存在しないprompt-form読出しも、chain classからformを渡す形に修正した。

fold fixtureのjobは既存のsupplied public free-prompt job／Attempt envelopeを使い、own replayとcontrollerを注入する。実serviceの`fraud_filer_start_replay_v1`、ledger予約、mempool→template→block、fresh-nodeのpaged readを一続きにしたV-nodeではない。production startupは現在free-promptをUnjudgedとする。step countやcheckpoint/traceのみの不一致、非base0 codec、全familyの正規eligibilityと最大profile、reservation飽和・期限内包含・恒久prune後の再取得も残る。held-court後半のchain readerに別のDAA差ベースのblock capがあるため、そのpaginationも必要。consensus encodingとactivation fenceは変更していない。

## held-court後半の公開履歴paginationと認証cache

`cac19f6d8`で残した後半readerも修正した。実service tickの`attn_held_objects_page_from_chain_v1`は、DAA差を件数へ変換せず、1ページ40,000 selected-chain blockまで読む。cursorと開始tipを保持して要求floorまで続け、読み終わるまではfiling候補をheld builderへ公開しない。newest-firstの走査で最後の認証済みrootを残し、次ページのより古いrootへ置き換える。最新のsuffixに立つfilingではなく、全対象範囲で最古のfilingを選ぶ。

accepted carrierにはfoldが拒否したlifecycle objectも含まれるため、rootを現在dutyのexecution/class/artifact/leaf・anchor・subrootsへ認証してから保存する。disclosureは選択中のstep-6 `(checkpoint, chunk)`だけを対象に、binding・checkpoint opening・chunk membershipをconsensus predicateで検査する。1 walkのcacheはroot 1件＋選択chunkごと1件で、重複・decoy・未選択disclosureを保存しない。unit選択が途中で変わった場合も、既に走査したprefixにそのunitの古い回答がある可能性があるため、選択digestの変更でbackfillをやり直す。

cached filingを取得した後にもtickでanchorのbranchを確認する。reorgではfiling・pending page・challenger evidenceと旧branchのqueued challenger moveを破棄し、元の古いfloorまで読み直す。branch確認に必要なdataを読めない場合は旧filingでmoveを送らず待つ。ページのheader/acceptance/block/parent取得失敗ではcursorを進めない。正しい登録モデルの保有前提とすべてのactivation fenceを維持し、consensus encodingは変更していない。

| 検証 | 結果と範囲 |
| --- | --- |
| `cargo test -p kaspad --lib accepted_objects_walk_tests --locked` | **6 V-unit PASS**。新testはactual Consensus API adapterで、8 block / 3 DAA、ページ中の新tip、最古filing、refused decoy・重複、partial suffixの非公開、branch変更と古いfloorの再読出し、選択変更時のbackfill、認証済みselected chunkと改ざん拒否、取得失敗を検査 |
| `cargo test -p kaspad --lib held_court::tests --locked` | **6 V-unit PASS**。held route／queue deadline／arity／actual service wiring pin |
| `cargo test -p kaspad --lib held_court_e2e --locked` | **20 V-fold PASS**。fixtureのchain読出しも新しい認証filterと`note_history_page_v1`を使う。held conviction・honest acquittal・非応答default・step-6のchunk要求とCheckpointAccused・restart・forfeit復旧・非seat追及・ledger回帰 |
| `cargo test -p kaspad --lib lg14b_ --locked` | **7 V-fold PASS**。outsiderのheld loopも新filter/page publishingを使い、common filerからのfused convictionと既存回帰を確認 |

[reader API adapter](evidence/held-history-page-adapter-final.log)、[held policy](evidence/held-history-policy-final.log)、[selected-history held fold](evidence/held-history-court-fold-selected-final.log)、[outsider LG14-B fold](evidence/held-history-legacy-fold-final.log)。default feature、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`で実行した。初回test fixtureがanchorを含めないsiteの0-position layoutを使ったため、chunkのfixtureだけanchor付きsiteのpositionで作り直した。認証条件は緩めていない。

この試験もV-nodeのfull service完走ではない。後半fold fixtureはpublic object listを1 completed pageとして読み、pagination自体は別のAPI adapterで検査する。最大historyの読出し時間・包含期限、全profileの実model RAM、責任期間内の恒久retention、prune後の公開取得・replay-safe再要求、全familyの正規eligibilityは残る。IR／dense courtの別の旧history walkerまで変更したものでもない。


## X8R統合と公開bindingの直接証拠

Claude統合 `cc4757d04bcf3629337c9037fff47cd356275a15`を`123d8254e`でmergeした。EXEC v2のlifecycle kind fenceを6、LG14-Aを7とし、`ALL`を8要素にした。deadlineのarm primitiveではwork-session holdとdispute holdのどちらも有効にし、claim retirementでも両方のrecordを片付ける。merge後のdefault-feature kaspad buildが成功し、公開history API adapter **6 V-unit**、lifecycle/A2U core **31 V-unit**がPASSした。

共通filerはexecutionとtraceだけでHonestを判定していたため、output-only不正を取りこぼしていた。own replayのoutput rootも保存・比較し、三rootのどれかが違えば公開binding取得へ進める。公開bindingが回答するjob/class/seed/context/prompt/traceのidentity違反、shape/count違反、公開token pinが証明するoutput不正は、subtree descentより先に既存kind-4 builderとreporter doorへ渡す。型・version・root・evidence digest・carrier capと実gateのrehearsalを維持し、独自の有罪判定やunsigned carrierの直接queueは追加しない。

claimのjob identityはproducerのserved contextから作らず、新しいread-only Consensus APIでtipのclaim/liability/vesting rowを読む。完全な直接証拠の判断にはDAのanswered flagを要求しない。公開carrierが正しいbindingを含んでもwrong-job DA armがその回答を拒否する場合、binding自身からのIdentityMismatchは提出可能である。subtree descentのshared-answer規則は従来どおりanswered＋認証を要求する。reporter bookのdedup、commit/file/reveal、gate再確認と最大2 handoffを使い、直接証拠が揃えば未送信の探索carrierを取り下げる。

history cacheでは、bindingだけ正しくpinが壊れた新しいeventが最初に採用され、古い正しいpinを隠せる可能性があった。binding-only回答を保持してjob/shape証拠へ使えるようにしつつ、認証済みeventが来ればその1件へ置き換える。後から来た壊れたeventでは戻さない。ページ内のcollectとbookへのmergeの両方で同じ優先規則を使い、entry数は増やさない。最古の正しいpinをまだ読めていないpartial pageでstep treeが一致しても、descentの失敗を理由にpursuitを捨てない。未読／partialのbackfillが完了するまでMismatchとown replayを保持する。

| 検証 | 範囲 |
| --- | --- |
| `palw_fraud_filer::tests` | **10 V-unit PASS**。三root一致だけがHonest、output-only/trace-onlyの追及とbackfill、未読／partial pageでのdescent失敗からの追及維持、既存controller/cache/wiring回帰 |
| `lg14b_` | **8 V-fold＋1 V-unit PASS**。新しいwrong-job testは実v7のheader anchorとexecution keyからjob identityを記録し、全PanelのValid後、Final前後にpublic bindingからactual objective foldがconvictすることを検査。実DA answered sessionは不要。delta再適用・revert・carriage reloadも検査。output/count/cacheのtestは正しいFP targetを手で構成したbuilderのV-unitであり、outputのfull node又はfull fold完走ではない |
| `accepted_objects_walk_tests` | **6 V-unit PASS**。actual API adapterのpagination、改ざん拒否、selected unit、backfill/reorg回帰 |
| `reporter_filer::tests` | **21 V-unit PASS**。commit/file/reveal、dedup、gate拒否、期限、restart、handoffの既存回帰 |

初回のwrong-job fixtureは古いv2 wrapperを使いjob identityを記録せず、IdentityNotRecordedを正しく返したため、testのacceptanceだけ実v7へ移した。次の実行はobjective offence fenceがfixtureで休眠していたため、R-core+ from genesisの設定に合わせextrasもtest-armした。判定条件や認証条件は緩めていない。

公開bindingのidentity違反とcountの直接証拠に加えても、G14全域は未達である。4096 idsを超えるcanonical promptのPromptNotAnchored自動提出、checkpoint/traceの残る局所化、free-promptの公開job/inputからのstartup、非base0 codecと全familyの正規eligibility、full service→public read→mempool/template→conviction、post-retirementの自動発見、最大profile・恒久retention・累積scope・全枠飽和・期限内包含は残る。wrong-job fixtureは公開eventをtestへ直接渡すため、新規nodeのhistory歩行から証拠提出までの一続きのV-nodeとは数えない。新しいread APIはconsensus encodingを変更せず、activation heightとADR-0177の登録モデル保有前提を維持する。

検証log: [merge lifecycle](evidence/integration-x8r-lifecycle-final.log)、[merge reader](evidence/integration-x8r-public-history-final.log)、[public binding fold / builder](evidence/public-binding-legacy-final.log)、[reader回帰](evidence/public-binding-reader-final.log)、[reporter回帰](evidence/public-binding-reporter-final.log)。

最終filer回帰も **10 V-unit PASS**。[filer結果](evidence/public-binding-filer-final.log)。統合と修正後の `scripts/t12-repin.sh --shipping --drift-only` は **361 ok、差分なし**。検査対象の全pin・gateを通過し、pinの書換えは行っていない。[repin結果](evidence/public-binding-repin-final.log)、[integration tests](evidence/public-binding-repin-harvest-tests.log)、[lib tests](evidence/public-binding-repin-harvest-lib.log)。


## 大型promptの公開入力証明を共通filerへ接続

`58fcf4038`で残した、4096 idsを超えるattempt入力の自動証明を追加した。小型promptは従来のIdentityMismatchが判断する。大型promptでは、shapeとcountが正しい公開bindingをclaimの記録済みjob anchorへ認証し、既存の`palw_prompt_not_anchored_admit_v1`と`palw_prompt_not_anchored_fault_v1(Whole)`を使う。anchorからcanonical prompt rootを再計算して不一致なら、既存kind-4の`PromptNotAnchored { Whole }`を作る。producerのprompt ids、capture、Panel receipt又はDA answered flagは証明の入力にしない。現在のheavy-budgetで受理できるprefillまでを計算対象とし、gateのchargeとWholeのrentをそのまま維持する。

実serviceの直接証拠判定はblocking workerへ移した。起動前にhost ledgerへtemporary prompt ids・Merkle tree・証拠bufferの上限を予約し、guardをworker終了まで保持する。メモリ不足は追及を終了する理由にせず、次tickへ待たせる。root memoは既存の8-entry bounded cacheを使う。これは直接証拠の局所的な予約であり、全node・最大modelのRAM測定の完成を意味しない。

| 検証 | 結果と範囲 |
| --- | --- |
| `cargo test -p kaspad --lib lg14b_ --locked` | **9 V-fold＋2 V-unit PASS**。新V-unitは65,536／2,097,152 context（8,191／262,143 prompt ids）のshape-correctなsynthetic bindingで、他anchorのprompt rootだけをrelabelした入力をPromptNotAnchoredへ変換する。honest input・別execution rootを告発せず、kind-4のheavy chargeがprefillを保持する。新V-foldは同じcontext幅のheld fixtureを実v7 header anchorでacceptし、全3 Panel seatのValid後、Final前後でpublic bindingからactual objective foldがCourtFraudへconvictしproducerをchargeする。delta apply/revertとcarriage reloadも従来fixtureで検査する |
| `cargo test -p kaspad --lib palw_fraud_filer::tests --locked` | **10 V-unit PASS**。三root、partial history、backfill、cache、boundsとservice wiring回帰 |
| `cargo test -p kaspad --lib reporter_filer::tests --locked` | **21 V-unit PASS**。既存reporter doorのcommit/file/reveal、gate、deadline、dedup回帰 |

[LG14-B / large-prompt結果](evidence/large-prompt-legacy-final.log)、[filer回帰](evidence/large-prompt-filer-final.log)、[reporter回帰](evidence/large-prompt-reporter-final.log)。default feature、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`で実行した。初回fold fixtureにFlatネットワーク上のnon-held profileを使ったため、courtがMerkle prompt証明を正しく拒否した。fold fixtureだけ既存held profileを用いる形へ修正した。prompt formのconsensus規則は変えていない。

synthetic bindingのtree rootはfixtureのcommitmentであり、最大profileの実modelを実行した試験ではない。class admissionは既存test fixtureで、通常のproduction eligibilityではない。公開eventをtestへ渡すV-foldであって、fresh-nodeのpaged read→service tick→reporter commit/file/reveal→mempool/template→blockのV-nodeではない。この変更で大型promptの直接証拠生成は接続したが、free-prompt startup、checkpoint/traceの残る局所化、全family／最大profile、post-retirement discovery、reorgによるown replayのjob変更、累積scope、予約枠飽和、恒久retentionと期限内包含は引き続き完成条件である。consensus encoding・activation height・ADR-0177は変更していない。


## Reorg後のjob更新と旧blocking replayの破棄

最新のClaude integration `589d9aa3e`（remote-minerのsalted drill adapter）を`7bd999b17`でmergeした。remote binaryの`cargo check -p misaka-palw-remote --features rpc --bin palw-remote-miner --locked`は成功。上流記録のlive 9-node drillをここで再実行したという意味ではない。[merge check](evidence/remote-drill-merge-check.log)。

共通filerのcandidate refreshはjob factsだけを上書きし、旧受理blockのHonest／Mismatch判定を保持していた。実行中workerもclaim idだけを持つため、reorgで同じclaimが別header上へ移った後、旧jobの遅延結果を新jobへ適用できた。3rootが同じでも、headerから導出するjob anchorが変わるため、旧結果をHonestとすれば新branchの違反を追及しない。

candidateの受理block・job・producer・role等の公開snapshotが変われば、caseをPendingへ戻す。旧own capture、frontier、CKW、binding、探索区間、selected unit、送信回数と直接証拠handoffを破棄し、cacheとwalk watermarkも更新する。実tickは無効化したcaseの未送信reservation/demand/terminalとqueue deadlineを取り下げる。別caseの終了だけでは進行中のhistory walkを巻き戻さない。jobは同じだがpublic-history branchが変わった場合は、そのown replayを保持しつつ学習済み探索・selected unit・送信debounceと未送信carrierを破棄し、新branchで再要求できるようにする。

blocking replayは起動時candidateのsnapshotとhandleを保持する。refresh後のcandidateと一致しない完了結果は、Honest／Mismatch／Unjudgedのどれにも適用せず、retry回数も消費しない。実行中のworkerはdetachやabortをせず、旧workerが終了するまで次のreplayを開始しない。fence-offのbook clearでもhandleを保持し、後続tickで完了したcaptureを破棄して予約を解放する。これでstarted blocking workerの実行中に予約を忘れたり、上限を越えて別workerを開始したりしない。

| 検証 | 結果と範囲 |
| --- | --- |
| `cargo test -p kaspad --lib palw_fraud_filer::tests --locked` | **14 V-unit PASS**。新4件は、3rootが同じで受理blockだけ違うjobへのHonest／Mismatchの無効化、旧結果と旧worker失敗の拒否、新jobの追及とbackfill、実blocking worker＋memory ledgerの終了前／結果保持中／破棄後の予約、clear後の単一worker維持、同jobでのbranch reset時の再送復旧、別case終了でhistory cursorを巻き戻さないこと、Panel seatになったbondがpublic laneを離れることを検査 |
| `cargo test -p kaspad --lib lg14b_ --locked` | **9 V-fold＋2 V-unit PASS**。Final前後のgather/matmul/job/input conviction、fused conviction、DA defaultとhonest twin等の回帰 |
| `cargo test -p kaspad --lib accepted_objects_walk_tests --locked` | **6 V-unit PASS**。actual Consensus API adapterのpaged public reader、branch reset、selected unitと認証回帰 |

[book / worker](evidence/filer-reorg-job-final.log)、[LG14-B回帰](evidence/reorg-job-legacy-final.log)、[reader回帰](evidence/reorg-job-reader-final.log)。default feature、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`。consensus encoding・fenceは変更していない。

このjob変更の修正はnode policyのV-unitであり、稼働nodeへ競合branchを投入してservice tickから新jobをreplayしconvictするV-nodeはまだない。full-serviceのfresh start/reorg、free-promptの公開job/input bootstrap、checkpoint/traceの残る局所化、全family／profile、retired discovery、累積scope・全枠飽和・恒久retentionと期限内包含は未完で、G14全域の完成判定は行わない。


## 公開FP commitment/inputからの独立再実行とpost-Final FPの追及

共通filerは従来、free-promptを無条件にUnjudgedとしていた。node起動時のFP bootstrapを追加し、recorded accepted blockのacceptance dataが指すmergeset blockから0x4a transactionを読む。accepted transaction id、claim hash、domain/class/bond、三root、work leaves、記録済みjob pinとartifact rootを照合する。accepted carrierが保持されていない場合やinputが未取得の場合はPendingを維持し、replay retryを消費しない。30 DAAの再試行間隔を設け、input待ちの最古claimが他claimのreplayを塞がない。job変更でこの待ち時間もresetする。

PublicDA user jobはaccepted payloadのidsを使い、canonical jobは自身のjob anchorからbackendで導出してrootとcountを照合する。PanelDA user jobは、nodeへ既に届いたFPM1/FPC1/FPA1のjob wireを認証済みjobのcanonical encodingへ先に照合し、同じjobの正しい長さのinput prefixだけを読む。untrusted jobやvectorのdeserializeをせず、u32::MAX等の不正なids countもallocation前に拒否する。その後、共有prompt admissionでroot/countを認証する。producer captureとoutput tailは長さの整合のみを確認し、deserialize・copy・実行・判定の入力にはしない。diskのown-retentionやproducer private checkpointも参照しない。このfallbackは配布・取得保証ではない。PanelDAではstranger向けgossip自体が制限されるため、inputを拒否されても追及できるpublic DA bootstrapはまだ必要である。

actual replay workerは、モデル保有前提どおりfresh backendからexecute_free_promptを実行する。host ledgerの見積りにはjobのdeclared decode ceiling contextを使い、FP inputのcopy／canonical derivation／Merkle-root検証より前にfull replay reservationを取得する。producerが宣言したexecuted countでメモリを小さくしない。own runのfactsからactual executed contextを導出し、三root、own capture/range、own output/job pinを持つbase0-codec replicaを保存する。attemptも同じrun assemblyを使う。producerのoutput token列やcaptureを再実行の入力にはしない。

新しいactual FP fold testから、FP Finalにvesting rowがないため、candidate discoveryとreservationが拒否し、さらに通常DA admissionも拒否することが分かった。既存の休眠palw_legacy_public_filer_v1 fence内で、この3箇所を共通のpost-Final predicateへ変更した。FPにはunvoided・unexpiredのliability rowとDA回答期限内のtrace retentionを要求する。unmatured vestingを持つ既存attemptの判定は維持する。FPのpursuitを責任期間内に開始した場合はda_rekeyの単一writer経由でliability expiryも延ばし、予約中に次のDA要求が失効することを防ぐ。新field、wire encoding、activation heightは追加していない。

この段階でもG14全域は未達。PanelDA withheld inputのcourt bootstrap、canonical FPのbad input-rootからbootstrap前に直接証拠／DA defaultへ進む経路、prefix-state等の公開stateからのstartup、非base0 codec・全familyの正規eligibility、full-service fresh-nodeのmempool/template包含、post-retirement discovery、恒久prune後の取得／再要求、累積scope・全枠飽和・最大profileの実測RAM／期限内包含が残る。FPM/FPC/FPAのinput prefix読出しの成功を、producerのprivate stateなしに全PanelDA claimを再実行できる証明とは数えない。ADR-0177はユーザー確認どおり維持する。


| 検証 | 結果と範囲 |
| --- | --- |
| current kaspad lib-test binary `palw_filer_replay::palw_filer_held::e2e:: --test-threads=2` | **24 PASS**（LG14-B 10 V-fold＋2 V-unit、既存held replay 12件）。新FP V-foldはactual accepted-tx extractionとFP claim fold、全3 PanelのValid、fresh own-modelのproduction replay helper、actual common controllerのreservation→binding→frontier→tag159→CourtFraudをFinal前後に検査する。Final後の追及は元のliability期限直前に開始して期限を跨ぐ。Final後のfrontier非応答はProducerWithholdingへdefaultし、CourtFraudと混同しない。expired liabilityとbelow-fenceのcandidate／reservation／DA admission拒否、liability hold、delta apply/revertとcarriage reloadも確認した |
| `cargo test -p kaspad --lib palw_fraud_filer --locked` | **18 V-unit PASS**（book/controller 15、FP input/replay 3）。PublicDA三root再現、producerの短いexecuted countに依存しないown run/pin、canonical derivation、PanelDA inputのexact-job/root/count確認、truncated/trailing carrier拒否、capture/output tailを解釈しないprefix読出し、input待ちのretry fairnessを検査 |
| current kaspad lib-test binary `accepted_objects_walk_tests --test-threads=2` | **7 V-unit PASS**。新API adapterでunaccepted exact decoy、別job、accepted mergeset tx、txid/index source error、domain／work leaves／recorded pin／laneの不一致、失われたacceptance/blockと復旧を確認。既存paged readerの認証／reorg回帰もPASS |
| current kaspad lib-test binary `reporter_filer::tests --test-threads=2` | **21 V-unit PASS**。既存reporter doorのcommit/file/reveal、dedup、gate拒否、期限とrestart回帰 |

[held / FP fold](evidence/fp-bootstrap-held-final.log)、[filer / input](evidence/fp-bootstrap-filer-final.log)、[accepted reader](evidence/fp-bootstrap-reader-final.log)、[reporter](evidence/fp-bootstrap-reporter-final.log)。default feature、CARGO_INCREMENTAL=0、CARGO_BUILD_JOBS=2。binary直実行の3群は直前の同じsourceでcargoがbuildしたkaspad_lib-7d8fbaae022c6e79を用いた。

初回の新FP fixtureは短い仮署名をstateless extractionが拒否し、署名wire長をML-DSA-87定数へ合わせた。次にfixtureのFP job pubkeyが登録producer bondのkeyと違うためBondKeyMismatchを正しく返し、共通held fixtureのjob keyを登録値へ合わせた。さらにFinal後の予約拒否を再現し、上述の3箇所の共通predicateとliability holdを修正した。通常signature検証を緩めておらず、fold fixtureのsignature callbackは従来と同じtest doubleである。API adapterも小型のfake chainであり、full nodeのacceptance／startupからconvictionまでの一続きのV-nodeとは数えない。


coreのlifecycle/A2U回帰は **31 PASS**、既存legacy dispute foldは **7 PASS**。通常featureのkaspad binary checkも成功した。[lifecycle](evidence/fp-bootstrap-core-lifecycle-final.log)、[dispute fold](evidence/fp-bootstrap-core-dispute-final.log)、[binary check](evidence/fp-bootstrap-binary-final.log)。Final後のFP予約拒否の修正前の再現logは[こちら](evidence/fp-bootstrap-fold-debug.log)。警告は既存のunused/dead-code等が残る。

最終sourceの `scripts/t12-repin.sh --shipping --drift-only` は **361 ok、差分なし**。fresh harvest logの再読込もexit 0。pinの書換えは行っていない。[repin](evidence/fp-bootstrap-repin-final.log)、[harvest tests](evidence/fp-bootstrap-repin-harvest-tests.log)、[harvest lib](evidence/fp-bootstrap-repin-harvest-lib.log)、[log再検証](evidence/fp-bootstrap-repin-replay-final.log)。


## 回答済みDA materialの番号付き再取得

前節のFP startupを`20c746436`でpushした後、permanently prunedな公開回答の追及を進めた。単に`DaUnitAlreadyAnswered`を外すと、古いaccusation署名の再送でverifierの担保・session枠を消費できるため、番号付きの明示的要求を休眠tag 156へ追加した。要求とstate/readerの仕様は[設計追記](../../design/palw/legacy-route-g14-filer.md#回答済みunitの再取得codex2026-10-10)を参照。

`PalwDisputeClaimV1`の末尾へpending reacquisition mapを追加した。既存のdispute writer、delta 200、tail 0xE2を使う。このrecordはnever-armedなLG14-A専用であり、live variantのwireを変更していない。従来のtest-armed carriageの保存形式は変わる。新tag 156のA2 wire pinは`0xd43488cfc9640788`とし、既存tagとint-12 variantのpinは変更していない。新tagは休眠fence 7がownerで、below-fenceのstateless skip／fold拒否、signature processor、heartbeat carriageへ配線した。有効化heightは変更していない。

公開view／op 204のJSONにFlat回答のrow範囲を追加した。個別answeredにないEventでも、historical Flatがcoverしたtile 0は再取得へ進む。隣tileと範囲外rowはansweredにしない。

| 最終ソースのコマンド | 結果と検証範囲 |
| --- | --- |
| `cargo test -p kaspad --lib palw_filer_held::e2e --locked` | **25 PASS**。LG14-Bは11 V-fold＋2 V-unit、既存held replayは12件。新試験は全Panel Validの小型fixtureで、公開carrier喪失後のFinal前後の再取得→CourtFraud、Final後の非応答→ProducerWithholding、期限・予約DAA・counter・bindingの拒否、旧署名の拒否、producer duty、corrupt answer拒否と担保を確認。全foldでdelta再適用／巻戻しとcarriage reloadを検査 |
| `cargo test -p kaspad --lib palw_fraud_filer --locked` | **18 V-unit PASS**。既存15件＋FP input/replay 3件。Flat historical回答のAwaitAnswerと番号付き再取得、tile／row境界の反例を既存試験へ追加 |
| `cargo test -p kaspad --lib accepted_objects_walk_tests --locked` | **7 V-unit PASS**。欠けた古いsuffixを完了とせず、認証済みprefixを保持し、新tipの再公開を読めることをfake ConsensusApiで検査 |
| `cargo test -p kaspad --lib reporter_filer::tests --locked` | **21 V-unit PASS**。reporter doorの既存回帰 |
| `cargo check -p kaspad --bin kaspad --locked` | **成功**。通常featureのnode／署名processorをcompile |

node側合計 **71 PASS**。[held](evidence/reacquire-held-verified.log)、[filer](evidence/reacquire-filer-final.log)、[reader](evidence/reacquire-reader-final.log)、[reporter](evidence/reacquire-reporter-verified.log)、[binary check](evidence/reacquire-binary-final.log)。CARGO_INCREMENTAL=0、CARGO_BUILD_JOBS=2、default feature。既存unused／dead-code警告は残る。

初回の新fold試験は、open session中の旧accusationに`DaSessionAlreadyOpen`が先に返ることを考慮していなかった。拒否を両方で確認し、session終了後には`DaUnitAlreadyAnswered`を厳密に確認した。初回A2検査は新tagのpin未追加だけがFAILで、全既存pinは同値だった。追加中のFlat対応をbuild中に編集したため、一度古いcoreと新nodeの組合せでcompileが失敗した。その後ソースを固定して全群を再buildした。最終PASSだけを上表へ計上する。

新fold試験はpublic carrier vectorを消去するfixtureであり、実DB pruningではない。jobは従来held fixtureのpublic job、独立model replayを用い、signature verification callback／class admissionには既存test doubleを使う。履歴adapterも別のV-unitである。これをfull service startup→mempool/template→実block inclusionのV-node、全familyのpruned start成功と数えない。TIR／pipeline、FP job自体の再取得、input未取得bootstrap、飽和枠、最大profile、累積scopeと期限は残る。34-session予算を消費するため、無制限のrestartを保証しない。

coreの最終source回帰はlifecycle/A2U **31 PASS**、共通filer **10 PASS**、legacy dispute fold **7 PASS**。新tag 156を含むfence-off twinは、None／never／未到達heightでrootとdeltaの互換性を確認した。[lifecycle](evidence/reacquire-core-lifecycle-final.log)、[filer](evidence/reacquire-core-filer-final.log)、[dispute fold](evidence/reacquire-core-dispute-final.log)。nodeの71件と合わせ、この範囲の回帰は **119 PASS**。全G14達成とは判定しない。

最終sourceの `scripts/t12-repin.sh --shipping --drift-only` はexit 0、**361 ok・差分なし**。network／genesis／ruleset等のshipping pinを書き換えていない。[repin](evidence/reacquire-repin-final.log)、[fresh harvest tests](evidence/reacquire-repin-harvest-tests.log)、[fresh harvest lib](evidence/reacquire-repin-harvest-lib.log)。


## PanelDA User入力の公開court bootstrap

前節の再取得修正`6a8da1fb8`を基点に、入力未取得のため独立replay開始前に止まるlegacy FP経路を追加した。registered modelを持つoutsiderが、Pendingのまま公開予約を取り、row-0 bindingとinput unitを取得できる。Merkleは既存32-id tileを64個まとめた2,048-id chunkを開示し、global count/index、既存input outer rootと部分木pathを認証する。末尾のragged subtreeも同じodd-node promotionで検証する。Flatはchunk 0だけで入力全体を既存hashへ照合する。network formとregistered profileの選択は共有関数に固定し、carrierからformを推測しない。

新unitはnever-armedなtags 157/158の末尾へappendした`PromptIds { chunk }`であり、`ClaimInput / Demanded`、model bytesなしである。tag 156の再取得でもこのunitを扱う。A2のwire pinは156=`0x7e5d332147511ca1`、157=`0xc7acaf87a3fefb35`、158=`0x380636cbf4ee26ab`へ更新した。他のtag、既存int-12 variant、activation heightを変更していない。公開回答のfoldとhistory reader、producer capture responder、共通署名builderは明示的なnetwork formを使う。

要求／再取得のprocessor admissionでは、回答body＋bindingの上限を、court close ceilingと100,000-byte single-carrier body allowanceの小さい方へ照合する。DA workerはtag 158を1本で運ぶため、multi-chunk court上限だけでは不十分である。回答processorと署名builderも実サイズを同じ範囲へ制限し、署名前に拒否する。このためoversized Flat入力をhonest producerへの回答不能な義務にしない。これはその入力のG14到達性を立証したものではなく、chunk化／許可profileとsession予算の受入条件は残る。

入力accumulatorのcopy、hash/proof temporary、workerとの引継ぎにhost memory ticketを持ち、入力待ちではreplay回数を消費しない。取得途中のcaseはpursuedだがheld replay slotを使わず、別Pending claimを実行できる。実worker結果をcaseへ戻す共通handoffにinput accumulatorとretry時刻を含めた。binding学習時にfallback localizerの範囲も初期化する。reorgはinput／bindingを破棄し、job／acceptance変更はcaseをresetする。成功したown replay又はsettlementでbootstrap ticketを解放する。

新しい小型V-fold試験では、PanelDAのidsをFP transactionから除き、変更したprivacyを含むjobをproducerが実際に再実行してcommitする。outsiderはPendingから公開回答だけを学び、自身のfresh registered-model backendで再計算し、既存production judge／localizer／terminalを通る。全Panel Valid後のFinal前後のCourtFraud、Final後のinput-only ProducerWithholding、公開carrierコピー喪失後の番号付き再取得→conviction、honest replayの無告発を確認した。全foldでdelta再適用／巻戻し、carriage reloadを検査する。signature verification／class admission callbackは既存test doubleで、PanelDA privacy fenceもtest-armedである。実DB pruning、登録後のordinary bond、fresh-node startup／RPC／mempool／template／実block包含を一続きに立証したV-nodeとは数えない。

| 検証 | 結果・範囲 |
| --- | --- |
| core `palw_legacy_held_da_v2` | **9 V-unit PASS**。1～8,192 ids、tile／chunk境界、ragged subtree、改ざん／欠落／別chunk／過剰proof、Flat whole hash、network form、scope。synthetic bindingのproof検査であり、その全sizeの正規class admissionや実model実行ではない |
| core `palw_lifecycle_objects_v2` | **31 V-unit PASS**。A2／A2U、既存wire pin、guarded owner、fence-off互換性 |
| core `palw_legacy_public_filer_v1` | **10 V-unit PASS**。公開localizer／予約の既存回帰 |
| node `palw_filer_held::e2e` | **26 PASS**。LG14-B 12 V-fold＋2 V-unit、既存held replay 12件。新PanelDA入力bootstrapの4分岐と再取得を含む |
| node `palw_fraud_filer` | **20 V-unit PASS**。新input認証／worker handoff／memory ticketと、入力待ち／reorgの試験を含む |
| node `accepted_objects_walk_tests` | **7 V-unit PASS**。fake ConsensusApiから選択PromptIdsをexplicit formで認証し、corruptコピーを捨て、欠けた旧suffixより前の新tip回答を読む |
| node `p2_7_disclosure_policy` | **8 V-unit PASS**。actual producer DA worker／署名builderから新input unitを作る既存dense fixtureを追加 |
| node `tir_court_e2e` | **19 PASS**。共通署名builderのform引継ぎを含む既存IR回帰 |
| node `reporter_filer::tests` | **21 V-unit PASS**。既存reporter doorの回帰 |

core `lg14a_legacy_dispute_fold`は **7 V-fold PASS**。actual ML-DSA-87署名を用いるprocessor `t12_legacy_held_da_gate`は **1 V-unit PASS**で、Flat／Merkle両形式のcoherent fixture、新input demand／answer、改ざん署名・below-fence拒否、court／single-carrier上限を検査した。通常featureの `cargo check -p kaspad --bin kaspad --locked`も成功した。上表と合わせ、この修正範囲の回帰は **159 PASS**。

[core input](evidence/fp-input-bootstrap-core-final.log)、[lifecycle](evidence/fp-input-bootstrap-lifecycle-final.log)、[core filer](evidence/fp-input-bootstrap-core-filer-final.log)、[dispute fold](evidence/fp-input-bootstrap-core-dispute-final.log)、[held](evidence/fp-input-bootstrap-held-final.log)、[node filer](evidence/fp-input-bootstrap-filer-final.log)、[reader](evidence/fp-input-bootstrap-reader-final.log)、[DA worker](evidence/fp-input-bootstrap-da-worker-final.log)、[IR](evidence/fp-input-bootstrap-ir-final.log)、[reporter](evidence/fp-input-bootstrap-reporter-final.log)、[signature gate](evidence/fp-input-bootstrap-processor-gate-final.log)、[binary check](evidence/fp-input-bootstrap-binary-final.log)。nodeはdefault feature（EVMを含む）、signature gateは `--features evm`、coreはdefault feature。`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`。nodeのbinary直実行は、最終sourceのfiler 20件をcargoでbuildした同じkaspad_lib binaryを使った。既存unused／dead-code警告は残る。

最終sourceの `scripts/t12-repin.sh --shipping --drift-only` はexit 0、**361 ok・差分なし**。同じfresh harvestの再読込もexit 0。shipping pinを書き換えていない。[repin](evidence/fp-input-bootstrap-repin-final.log)、[harvest tests](evidence/fp-input-bootstrap-repin-harvest-tests.log)、[harvest lib](evidence/fp-input-bootstrap-repin-harvest-lib.log)、[log再検証](evidence/fp-input-bootstrap-repin-replay-final.log)。

最初のPanelDA fixtureは、accepted-payload extractionのPanelDA fence引数を既存PublicDA試験同様のfalseにしており、正しくstateless拒否された。fixtureだけを明示的にtest-armし、productionのfenceを緩めていない。gate試験では、Merkle networkの大きな入力もchunkは小さいという性質をFlatのwhole-inputと取り違えた期待を訂正した。FlatとMerkleのcoherentなgenesis形式を別々に通す。初回gate buildはdisk不足で失敗し、自分の古いbuild cacheを整理して再実行した。最終PASSだけを結果へ計上する。

G14全域は未達。34-session予算に入力取得も含むため、8kは4 input unitだが最大contextや任意回数のrestartを保証しない。accepted FP commitment自体の恒久prune、canonical FP bad root、prefix-state、非base0／全familyの正規eligibility、全枠飽和、最大profileのRAM／時間、fresh-node serviceと責任期間内包含、累積scope等は残る。ADR-0177 D7の登録モデル保有前提を維持し、model distribution義務とactivation heightは追加しない。

## 共謀者の予約数による新verifierの締出しを除く

`4d01414de`を基点に、claim共通の64-live／256-lifetime上限を除いた。担保をburnさせることは経済的な抑止であり、担保を払う共謀者に追及を排除されない保証ではない。予約admissionは、Active／floor、本人の担保余力、正しいclaim roots、once-per-bond/claim、絶対hard deadlineと本人のsession予算を維持する。他bondが予約を先に取得又はreleaseしても、これら本人の条件を消費しない。

stateの上限を本人へ帰属させるため、1 bondあたりretained recordを64に制限した。live、released/lapsed deposit、返金済みだが別ownerのdepositのためrecordが残るclosed replay guardも数える。derived `(bond, claim)` indexのwriter／load／delta rebuild／consistencyとadmissionが同じlive＋closed集合を使う。本人のreleaseでは保持予算を回復せず、既存のobjective settlement又はretirementでrecordを消去して回復する。held depositの重複entryと、known owner／matching sessionを持たないreacquisitionをload時に拒否する。

保持recordに帰属するdistinct bond identityをBとすればowner/claim rowは64×B以下となる。各rowは高々1件のpending reacquisition、各closed rowは高々1件のheld depositを持つ。固定のclaim人数上限をowner数に比例するstate上限へ変更したものであり、固定global RAM上限や無制限arrivalへの包含期限を立証しない。現foldはclaim recordをcloneするため、最大人口でのCPU／RAMとresponse処理・包含は引き続き受入条件である。RPC op 206は`retainedRecords`／`retainedCap`を末尾へ追加する。rooted field、object encoding、activation heightは変更していない。

coreの新attack fixtureは、genesis後にcoalition bondと新verifier bondを実foldで登録し、64同時又は256生涯の予約を先取りさせる。新verifierは65人目／257人目として本人の予約とsessionを開き、非応答defaultを成立させる。DA-8の16回共有counterだけは既存試験同様にcarriage fixtureで消費済みに設定する。counterが本人のreserved sessionへ影響しないこと、全depositの返金、index回収、毎blockのdelta適用／巻戻しとcarriage reloadを検査する。別の新試験は本人のclosed records 64件でadmissionを拒否し、zero-exposureの65件目を追加した偽carriageも拒否する。最初のretirement試験はtrace retentionとretirement clockを混同した期待だけがFAILであり、state自身のdeadline queueへ修正した。

小型PanelDA FP試験にもcoalitionの予約を追加した。Final前の64同時、Final後の256生涯、Final後の64同時をそれぞれ満たしてから、公開controllerのinput bootstrap→own model replay→CourtFraud又はinput-only DA defaultを通す。署名／class admissionは既存callbackのV-foldであり、mature ordinary bondを用いるfull-node包含や全familyのC6/C8達成とは数えない。

| 最終sourceの検証 | 結果 |
| --- | --- |
| `cargo test -p kaspa-consensus-core --test lg14a_legacy_dispute_fold --locked` | **9 V-fold PASS**。新しい人数先取りattackと本人の保持予算／偽carriage／retirement回収、既存fence-off互換性・deposit・期限・公開予約sessionの回帰 |
| `cargo test -p kaspa-consensus-core --lib palw_legacy --locked` | **19 V-unit PASS**。公開filer 10件＋held DA 9件 |
| current core lib-test binary `palw_lifecycle_objects_v2 --test-threads=2` | **31 V-unit PASS**。A2／A2U、全wire pinの同値、owner/fence guard |
| `cargo test -p kaspad --lib palw_filer_held::e2e --locked` | **26 PASS**。LG14-B 12 V-fold＋2 V-unit、既存held replay 12件。PanelDAの旧64/256閾値を超える予約から公開入力bootstrap→独立replay→Final前後のCourtFraud／input-only DA default、honest twinを含む |
| current node lib-test binary `palw_fraud_filer --test-threads=2` | **20 V-unit PASS**。book／input／worker handoff／memory／reorg回帰 |
| current node lib-test binary `accepted_objects_walk_tests --test-threads=2` | **7 V-unit PASS**。accepted history API adapter／selected authenticated input／paged/reorg回帰 |
| current node lib-test binary `reporter_filer::tests --test-threads=2` | **21 V-unit PASS**。既存reporter door回帰 |
| `cargo check -p kaspad --bin kaspad --locked` | **成功**。通常node binaryのcompile |

この修正範囲の回帰は **133 PASS**。coreはdefault feature、nodeはdefault feature（EVMを含む）、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`。binary直実行は最終sourceでcargoがbuildした同じcore／node lib-test binaryを使用した。既存unused/dead-code警告は残る。[core fold](evidence/reservation-isolation-core-fold-final.log)、[core unit](evidence/reservation-isolation-core-unit-final.log)、[lifecycle](evidence/reservation-isolation-core-lifecycle-final.log)、[node held](evidence/reservation-isolation-node-held-final.log)、[filer](evidence/reservation-isolation-node-filer-final.log)、[reader](evidence/reservation-isolation-node-reader-final.log)、[reporter](evidence/reservation-isolation-node-reporter-final.log)、[binary check](evidence/reservation-isolation-binary-final.log)。初回node fixtureの署名builder呼出しは署名bytesをcallback引数へ渡してcompile拒否され、既存APIどおりcallbackへ訂正した。最終PASSだけを計上した。

最終sourceの `scripts/t12-repin.sh --shipping --drift-only` はexit 0、**361 ok・差分なし**。fresh harvest再読込もexit 0。pinを書き換えていない。[repin](evidence/reservation-isolation-repin-final.log)、[harvest tests](evidence/reservation-isolation-repin-harvest-tests.log)、[harvest lib](evidence/reservation-isolation-repin-harvest-lib.log)、[log再検証](evidence/reservation-isolation-repin-replay-final.log)。

G14全域は未達であり、owner数に比例する最大response負荷、fresh-node service、全family、最大contextの34-session予算、未取得FP job／canonical input／prefix-state、累積scopeと期限内包含が残る。

## canonical FPの入力admission失敗から公開証拠へ進む

`b483440c8`を基点に、canonical FP jobが不正なprompt count/rootを自己整合的にcommitした場合の追及停止を修正した。既存のidentity検査はFP job pinの一致を検査するが、公開jobそのもののcanonical inputを検査しない。own-modelで導出した入力がjobの宣言と合わなければ、nodeはreplayの前に止まり、attempt専用の`PromptNotAnchored`にも進めなかった。

既存contradiction 13のproof enumへ`FpCanonicalWhole { job } = 2`を追加した。jobを公開claimの記録済みpin、認証済みbinding contextのjob hash／seed／count／hash／tokenizer／context ceiling、classとexecutorへ照合する。正しいjob bytesであることを確認してから、登録classのcanonical countと`fp_canonical_anchor_v1(job)`から導くrootを比較する。producerのids、capture、secret stateを証拠builderへ渡さない。count不正はwhole rootを計算せずに判定する。root不正は既存heavy budget内で計算する。

kind-3／kind-4はpublic-filer fence前にこの追加proofを明示的に拒否する。processorとfoldのheavy課金も同じfenceを読む。非認証jobとcount不正はheavy slotを使わず、root判定は既存per-claim memoと262,144-id budgetへ帰属する。nodeは一時ids/treeとproof bufferをhost ledgerへ予約し、hashingをworkerへ移す。rentは宣言countを保守的に価格へ含める。既存Tile/Wholeのwire tag、state layout、activation heightとshipping pinを変えない。旧V1 readerは従来どおりcontradiction 13を拒否する。

追加proofのrentもpublic-filer fenceを読む。旧`palw_offence_heavy_prompt_ids_v1`／`palw_object_rent_ceiling_v2`は、この旧buildが知らないFP proof tag 2を0で価格づけする。新しいfence-aware価格関数をprocessorの受入処理とUTXOのcoinbase rent burnに同じDAAで接続した。fence前のunknown proofに新しいrentをburnしてcoinbaseが食い違うことを防ぐ。kind-3／kind-4、fence前後とoffence-attribution未有効の価格をcore unitで確認した。実coinbaseのraw FP proof carrierによるchain-path検証はfresh-node受入条件へ残す。

nodeはcanonical input admissionの失敗でも、認証済みaccepted FP payloadをPending bootstrapとして保持し、公開bindingのreservation／demandへ進める。保持はaccepted jobの認証直後、小さいinput用host ticketで行い、backendのceiling context導出やfull replayのメモリ予約より前に置く。full replayのメモリ待ちだけで直接証明を止めない。canonical Pendingは入力ids未保持でもreplay retry対象へ残し、メモリが使えるようになれば正しいjobの独立replayへ進める。bindingを得たらproducerのprompt idsを要求せず、公開jobとbindingからkind-4 reporter filingへ進む。binding非応答は既存`ProducerWithholding`に進む。ordinary V3／V4 canonical jobを対象とし、PublicDA／PanelDAを認める。prefix/eval/tensor-tail等の別job意味論をこのproofへ混ぜない。

G14全域は未達。最大contextのinput session予算、恒久pruneされたaccepted FP jobの取得、prefix/state等のbootstrap、全familyの正規eligibility、checkpoint/trace局所化、fresh-node full-service／包含／restart／IBD／reorg、最大population/profileの実測RAM・期限、累積scopeは残る。登録モデル保有を前提とするADR-0177 D7と、activationのfail-closed方針を維持する。

heavy課金は現行のonce-per-claimを引き継ぐ。ソース上、whole-root memoは8 entryのFIFOであり、より多い小promptのclaimを交互に提出すると退避済みrootを再計算し得る。claim集合の課金済みflagだけではその実workを束縛しない。今回の単独claimのroot判定PASSを、多claim下の実CPU上限又はproof spamへの期限内包含の証明と扱わない。この予算／memo退避の反例を実際のblockで検証して閉じることは、次の残課題である。

最終sourceの検証は **103 PASS**。coreの`palw_offence`は40 V-unitで、新proofのV3／V4、floor Flat／Merkle、2M prompt、honest input、root／count不正、job nonce／anchor／class／executor／tokenizer／count／hash／mode差替え、別claim／lane、kind-3／kind-4、fence前の拒否とheavy課金を検査する。nodeのLG14-Bは15件で、このうち新しい入力試験は6 caseを1 testとして数える。実際に受理されたFP claimを全PanelのValidでlicenseし、公開bindingのreservation／demand／answerとproduction controller／proof builder、actual fold adjudicator、delta reapply／revert／carriage loadを通す。root／count不正のFinal前後conviction、binding非応答のDA default、correct-input twinの非提出、replay回数0とcanonical Pendingのretryを確認した。入力専用のsynthetic step／checkpoint rootとclass／signature callbackを使うV-foldであり、モデル全体の実行やfresh-node full-serviceの証明ではない。

残るnode回帰はfiler/input 20 V-unit、public reader 7 V-unit、reporter 21 V-unit。通常feature（EVMを含む）の`cargo check -p kaspad --bin kaspad --locked`も成功した。`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`を使用し、nodeのbinary直実行は最終sourceをcargoでbuildした同じkaspad_lib binary、coreは最終shipping harvestがbuildしたcore lib-test binaryを用いた。[core](evidence/canonical-fp-core-final.log)、[LG14-B](evidence/canonical-fp-node-held-final.log)、[filer/input](evidence/canonical-fp-node-filer-final.log)、[reader](evidence/canonical-fp-reader-final.log)、[reporter](evidence/canonical-fp-reporter-final.log)、[binary check](evidence/canonical-fp-binary-final.log)。unused/dead-code警告は残る。

最終sourceの`--shipping --drift-only`は **361 ok、差分なし、exit 0**。fresh harvestのlog再読込もexit 0。shipping pinを変更していない。[repin](evidence/canonical-fp-repin-final.log)、[harvest tests](evidence/canonical-fp-repin-harvest-tests.log)、[harvest lib](evidence/canonical-fp-repin-harvest-lib.log)、[再読込](evidence/canonical-fp-repin-replay-final.log)。

初回node試験のcorrect-input twinはfixtureのlegacy prompt generator／network-wide Flat formを使い、classのCoreV1／Merkle formと合わずFAILした。fixtureを正しいfamily ruleとclass formへ修正し、productionの検証predicateやfenceを緩めていない。[初回log](evidence/canonical-fp-node-initial.log)。最終PASSだけを上記へ計上した。

## whole-prompt rootをblockの受入から最終foldまで保持する

`3bb912bec`を基点に、once-per-claimのheavy課金と実際のroot再計算を対応させた。従来の共有8-entry FIFOは、別threadが9件のrootを計算するだけで対象rootを退避できる。新しいnegative controlでは262,143 idsの同じrootを3回検査すると786,429 idsを再計算した。これはroot関数のcache反例であり、RPC経由の攻撃又は許可blockの反例を立証するものではない。

`PalwPromptRootBlockGuardV2`は同じblockの同期受入・single-object rehearsal・最終foldにthread-local memoを共有する。実chain processorは両phaseを覆うouter guardを保持し、単独の受入／fold入口も自身のcold scopeを作る。block内ではprocess cacheのwarmthを使わず、認証済みclaimのrootを退避しない。保持weightは262,144 ids以下、各entryは少なくとも1を消費する。budget外の呼出しがoverflowした場合も純粋なrootを返すがcacheしないため、既存rootを追い出せない。scopeは別block、別thread、panic/unwindから復元・隔離され、guardは別threadへ移せない。観測helperはcacheを作成・拡張せず、実呼出し回数を測るだけである。state、root値、wire、fence、activation heightは変更していない。

新node試験は2Mのsynthetic committed rootsを持つactual own attemptと全PanelのValid licenceを使う。3本の失敗Bystander kind-3、正しい全seat kind-3、executor kind-4の5本をactual rent 13,107,150 sompiでsigned carrierとして個別に重量測定した。合計transient massは357,140で500,000以下。productionのpriced accepted filterとactual foldを通す。単独の受入は1 root・262,143 ids・9 hits、単独foldは1 root・262,143 ids・1 hit。実chainと同じouter guardで両phaseを連結し、途中で別threadが旧process cacheを退避させても、全体は1 root・262,143 ids・11 hitsで、scope外／overflowの呼出しは0。cold scopeから2回再現し、CourtFraud、executor offence消費、full-seat lockの取得、carriage reloadを確認した。

class admissionは既存fixture seam、commitmentsは入力検査用syntheticである。個別carrierの重量測定は同じfixture funding outpointを再利用するため、5本の実UTXO spendが同じblockへ包含できる証明ではない。fresh-node startup／RPC／mempool／coinbase／実採掘の一続きのV-node試験とは数えない。初回の9-claim cycling fixtureは合計mass 1,486,700で上限を超え、反例として棄却した。したがって前節の多claim案を許可blockの攻撃成立と扱わない。[mass上限で拒否された初回試験](evidence/block-prompt-node-mass-initial.log)。helper名の初回compile失敗も記録し、最終結果だけを計上する。[初回compile log](evidence/block-prompt-node-initial.log)。

| 最終sourceの検証 | 結果 |
| --- | --- |
| core `palw_attempt_rules_v1` | **12 V-unit PASS**。9 roots・4巡、cold pass、scope復元／thread隔離／overflow、旧process cacheのnegative control |
| core `palw_offence` | **40 V-unit PASS**。canonical FP・既存attribution／heavy課金の回帰 |
| core `palw_legacy` | **19 V-unit PASS**。公開filer／held DA回帰 |
| consensus `t47_model_class_attribution::` | **15 PASS**。新しいprocess-cache退避のpriced gate／受入／fold、既存block budget／slot fairness／各contradictionの回帰 |
| `cargo check -p kaspad --bin kaspad --locked` | **成功**。通常feature（EVM含む）のnode binary |

合計 **86 unique PASS**。先行する`t18u_`の4件はT47の15件へ含まれるので重複計上しない。coreはdefault feature、consensusは`--features evm`、`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`。core offence／legacyとT47の直実行は同じ最終sourceをcargoでbuildしたlib-test binaryを使う。既存警告は残る。[core root](evidence/block-prompt-core-final.log)、[offence](evidence/block-prompt-core-offence-final.log)、[legacy](evidence/block-prompt-core-legacy-final.log)、[t18u subset](evidence/block-prompt-node-final.log)、[T47](evidence/block-prompt-node-attribution-final.log)、[binary check](evidence/block-prompt-binary-final.log)。

最終sourceのshipping repinとfresh harvest再読込はexit 0、**361 ok・差分なし**。pinを書き換えていない。[repin](evidence/block-prompt-repin-final.log)、[harvest tests](evidence/block-prompt-repin-harvest-tests.log)、[harvest lib](evidence/block-prompt-repin-harvest-lib.log)、[再読込](evidence/block-prompt-repin-replay-final.log)。

次の提出阻害を発見した。`kaspad/src/palw_panel.rs::build_lifecycle_tx_priced_v1`はcompute-mass relay feeだけを使い、protocol rentを含めない。重いwhole proofの必要rent 13,107,150 sompiを満たさずprocessorで落ちるcarrierを作り得る。`palw_fee_funding_floor_v1`も2倍のstandard relay feeだけを基準にするため、重いproofの必要額未満でrefillを止め得る。fee builderとfunding policyの修正を次の優先事項とする。今回のテストはrentを明示的に支払っており、nodeの現builderの提出到達性を立証していない。G14全域の残条件（最大context/session、恒久prune FP job、prefix/state・全family、full service、人口／model peak・期限、累積scope）は引き続き未達。
