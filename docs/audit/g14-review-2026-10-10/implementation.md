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
