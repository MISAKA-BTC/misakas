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
