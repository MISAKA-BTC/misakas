# RFC07: selected approval を持つ最初の source の接続

状態: **UNCOMPILED / UNEXECUTED。有限epochのown-chain resolver・native journal・承認slot・履歴settlementからの新source event、直接Programの実producer/checkerとprivate共通profileを接続した。後続assignmentのversioned lineage、経済consumerと統合受入は残る。公開 activation や数値の承認ではない。**

## 現状の停止条件

旧 `FinalPathV1::PanelIndependent` は「Panel に依存しない Final」を意味し、OPV の window expiry から作られていた。恒久方針により、expiry や local replay は抽選検証者の承認を代替できないため、この event 生成は停止した。一方、現 V1/V3 collector は `PanelAssignment` に対して `PanelLicensed` を一律に拒否する。この二つを維持したまま、最初の paid/onboarding source が自然発生するとは扱わない。

承認済み source を旧 OPV path へ再ラベルする変更や、collector の circularity guard を削除する変更では解消しない。新しい version の bootstrap provenance と selected-approval source path/policy が必要である。RFC0016/0017/ADR の実装は対象外。

モデル単位の審査・資格・昇格を source の条件に追加しない。モデル二階層は凍結した [PRINCIPLES §9](../../PRINCIPLES.md#9-モデルの二階層とモデル審査の不採用2026-10-15ユーザー凍結前提) に従う。ここで扱う承認は全階層に必要な claim 単位の認証であり、登録モデルの 0.3 / 1.0 を変更しない。

## 新契約の境界

`misaka/palw/challenge/src/bootstrap_epoch_v1.rs` に独立した epoch 契約を置く。現時点で `Verified` な authority を返す関数、receipt の承認、source eligibility の変更は提供しない。

契約は genesis/ruleset/policy、凍結 population root、native anchor、その固定位置、epoch と有効期間を束縛する。人口を凍結した位置より後に anchor があり、anchor より後に epoch が始まる。native consumer は数値の大小だけでなく、これらが own canonical chain の実 checkpoint/registry/locked stake であることを検査しなければならない。caller が渡した header/hash/roster をそのまま authority としない。

最初の entropy の候補は、**当該 bootstrap cohort の仕事で採用が決まらない native anchor** とする。native PoW header を使う場合も、hash があることは予測不能性や偏りの小ささを示さない。採掘者の withholding/grinding、fork 選択、利用可能な hashpower、再試行の費用を明示して後続検証する。循環を除いた anchor を own state で認証できなければ `Unavailable` のままにする。LLM 計算の暗号学的証明は使用しない。

## 抽選と相関

cohort は epoch ごとに一度決まり、仕事・claim・retry の ID は抽選入力に入らない。人口は native bond ID 順の正規形で、各 weight は own frozen locked stake から算定する。operator ID や異なる bond を独立した運営主体の証拠としない。

slot ごとに総 weight 上の整数 ticket を取り、同じ検証者を再び選ぶことを許す。これは stake 重みを持つ voting slot であり、異なる人や独立した再計算の数ではない。鍵を分割しても総 ticket mass は増えない。ただし、鍵分割後に同じ seed が同じ signer を返すという不変性は主張しない。population は entropy より前に固定する。

modulo の偏りを避けるため整数 rejection を用い、attempt 上限を使い切った場合は固定 epoch の抽選を拒否する。別 anchor/claim ID に切り替えて都合のよい cohort を採用しない。これは宣言した entropy 仮定の下での抽選であり、entropy の仮定自体の検証ではない。

同じ cohort の全仕事に対する不正通過は相関する。`maximum_source_works` を native admission で消費し、一回の cohort compromise を各仕事で新しい独立試行のように計算しない。期限までに承認を得られなくても自動で初期 cohort を再抽選しない。

現 source は選択結果 Vec の実型・最大 slot 数から予約量を導き、allocation 前の callback が失敗した場合は capacity error を保持する。population の encoded Vec や program/class population は複製しない。選択結果は予約 owner を保持する。これは logical request の境界であり、physical RAM/worker/DB の完成を示さない。

## 接続する native consumer

以下が揃うまで bootstrap を有効化しない。

1. version/fence と native epoch row。public default は None。own canonical anchor、凍結前の registry/locked-weight root、政策の一致を認証する。
2. epoch を初回に固定する journal、source work 件数 cap、期限、reorg/restore/import の同一性。後から違う roster/anchor を同じ epoch と扱わない。
3. work の実 commitment、material scope、complete checker と source profile の固定。claim を変えて cohort を選び直させない。
4. signer/slot/epoch/work/statement/期限を束縛する承認 receipt。slot の再利用を拒否し、同じ signer の複数 slot を独立者の証明としない。報酬を重複発生させない。
5. own accepted quorum と historical settlement からだけ作る新 source event。旧 `PanelIndependent`/`PanelLicensed` の wire/ID を変更しない。
6. consumer から source・cohort・anchor への依存関係を認証する新 collector/policy。対象 profile やその依存先が source approval の seed/validity を決める cycle を拒否する。
7. 実 source 生成 → checker receipt → native quorum → beacon → 後続 assignment の private worker/harness。bootstrap を経ず synthetic Final を投入して代用しない。

## 後続検証

source control は canonical Borsh/hash framing、異なる network/policy/population、固定 epoch の再現、rejection 上限、allocation 拒否、owner 寿命、鍵分割時の coalition ticket mass を対象とする。すべて未実行。

正直な locked-weight の割合、cohort/slot 数、quorum、epoch/anchor/window、source 件数 cap、検閲・取得可能性、資本・報酬・grinding の収支は未承認である。ユーザー指定どおり実装後の一括検証で決める。mechanics-only の test fixture 値を承認済み parameter に昇格しない。

## 2026-10-18 native接続の実装境界

新しい `palw_selected_source_bootstrap_v1` は全presetで `None` とし、未検証の実activationは拒否する。outer tag 246 `SourceBootstrapV1` とaux137を追加し、既存のV1 final-path IDは変更しない。policy全体をSome-onlyのParams identityに含め、borsh-skippedのnative mirrorを同期・照合するため、休眠時の既存ruleset bytesは維持する。

policyはfreeze/anchorのDAA cutoff、開始・承認終了・stake解放位置、population/slot/source件数、実claimのbyte/work上限とown chain headerの厳密訪問上限を明示する。自動次epoch・再抽選・代替anchorは無い。context factoryはfrozen own stateのActive bondから正の利用可能slashable weightを数え、型から計算したVec予約後にpopulationを構築してownerを保持する。block/root/chain/importの権威は別のown-node resolverが検査する。factory単体や外部の同形bytesは権威ではない。

native foldはepochのroster/anchorを固定し、reorg可能な同じaux journalへ書く。人口が0の場合、またはpolicyのpopulation上限を超えた場合は、そのepochを固定Unavailableにする。上限超過は `population_refused` と空のrosterを保存し、巨大な人口のcloneや上限までの切り捨て抽選を行わない。固定drawのrejection上限に達した場合、あるいはDAAの飛びによりfreeze後の独立したanchorが存在しない場合も `draw_refused` として保持し、再抽選・代替anchorを使わず通常chainは進行する。これとローカルhistory/store/capacity不足は区別する。後者を `None` に落としてjournalだけ省略してはならず、typed faultでnative transition全体を保留する。fold/pre-object/rehearsal/templateと実chain walkへtyped faultを接続したが、未実行である。active policyのfreeze後にcontext自体が欠けた場合もcore側でhistory未取得として拒否する。

cutoffを越えるblockの生産者がblockを留保する場合のanchor取得可能性、固定epochの成立率、population上限と正直な参加率、source市場の供給・検閲条件は、機構だけでは保証しない。固定Unavailableがchainを止めないことと、有用なsourceを必ず得られることは別の受入項目であり、後続campaignで測定する。

最初のwork adapterは既存own kernel ledgerの直接program claimを対象にする。native class/program/artifact/plan scope、実claim body、payer/producer、全positionの計算費用を束縛する。旧Panelへの依存を避けるため、OPV lifecycleのclaimに別の新selected approvalを付ける。OPVのFinalは承認を代替せず、quorumの無いsource eventを生成しない。canonical input identityで同じ仕事の再包装をsource件数へ重複計上しない。pipeline/EXEC等のsource adapterを実装済みとはしない。

承認はepoch/policy/work/statement/deadlineに束縛されたactor署名を検査し、drawn slotを一度だけ消費する。同一signerが複数slotを持っても独立運営者とは扱わず、slot数に応じた追加支払は発生させない。新eventはnative accepted quorumと実kernel historical settlementの両方が揃ってからだけprojectionする。stake解放位置までにsettledしない仕事はeventを供給しない。

`misaka/palw/challenge/src/approved_source_v1.rs` は新しいprovenanceとcollectorを提供する。旧collectorのcircularity guardは保持する。新collectorは固定bootstrap policy/epoch/source scope、anchorがconsumer commitmentより前、仕事がそのcommitmentより後、canonical順序、仕事の重複除去と実lineageを検査する。native source getterから後続の実assignmentへこのcollectorを選択する配線と、実transitive lineage resolverは残件である。外部RPCのevent bytesや常にfalseを返す仮のlineage callbackを本番権威として使わない。

残る主な実装は、selected source workerによる不正material/無回答の自動告発、verifierのslash/liability・実収支consumer、後続beacon/assignmentのversioned policyとlineage、scope別adapter、実ノードharnessである。追加したdraw一致・canonical ordering・不足quorum・cycle拒否のsource testは未実行であり、native承認E2E・再起動/reorg/IBD/pruning・市場条件の受入結果ではない。全コードを先に完成させた後、同じ候補binary・同じtestnetの複数nodeと複数claimを使う一回の統合受入campaignで正常系と攻撃シナリオをまとめて実行する。保存・pushのための検証は開始しない。

## 2026-10-18 private selected checker の実配線

`source-v1` private profileは明示Borsh `(activation_daa, PalwSelectedSourceBootstrapPolicyV1)` を読み、policy以外を既存exact complete-v1 factoryに固定する。数値のdefaultは追加しない。activationは非zeroのscheduled fenceとする。genesis activationではpolicy.rulesetが自分自身を含むidentityを参照してしまうためである。scheduled fenceを正規化するbase identityをown resolverと照合し、完全なParams fingerprintにはpolicy全体を残す。public presetは引き続きNoneで、通常起動のrelease拒否を解除しない。

`PrivateSourceBootstrapCheckerSpecV1` は1 work・1 drawn slot・1独立fee inputに固定した実workerである。新しいin-process getterは同じown selected state、virtual DAA、durable import epochを前後で照合し、fixed roster/cohort、work statementと実native claim/materialを再検査してからretained observationを返す。workerはそこからsignerとslotを読み、operatorが供給したbounded canonical modelを登録済みparameter commitmentと照合し、直接Programの全position再実行とdecode照合を行う。取得不能・一致しないmodel・未判定・fault・期限超過では承認しない。replayが一致した場合だけ、epoch/policy/work/statement/deadlineに束縛した承認へ署名し、own rehearsal、明示fee cap、durable object/transaction WALを経て実mempoolへ提出する。再起動・reorg時は同じstatementと元のcarrierのみを再確認し、別inputへの置換やslot再抽選はしない。複数slotの重複signerを独立verifierとは数えない。

これはモデル審査ではなく、既にnative claimとしてcommitされた1 workのcheckerである。matching replay自体は認証を代替せず、native foldがselected quorumを数える。現時点のworkerは直接Programとlocal authenticated model取得に限定し、他scope、network model取得とobjective fault/無回答の自動告発、verifier slash・報酬収支は残る。

下流consumer向けにはnative source eventと実consumer claimの観測を追加したが、versioned transitive lineageは未解決である。`VersionedLineageUnimplemented` / `NativeConsumerAbsent` を明示し、collectorへ常にfalseのdependency callbackを渡さず、seedやLocked assignmentを生成しない。旧V1 collectorやsource Final pathを使った代用はしない。worker・getter・private factoryを含め全て未build・未実行であり、統合campaignの受入結果ではない。


## 2026-10-18 同一testnetでの source / paid 共通profileと実producer

新しい明示profile `source-paid-v1` は既存exact `paid-v4` factoryへoperator指定のsource policyだけを追加する。`--private-probe-source-policy=ABSOLUTE_BORSH` のcanonical Borsh `(u64 activation, PalwSelectedSourceBootstrapPolicyV1)` が必須で、A/B/JOINを含む全nodeが同じsalt・policy・Params fingerprintを使う。source-v1と旧paid-v4のfactory/意味は維持する。public presetはNone、manifestはreleaseApproval=falseであり、private値は検証・承認されていない。

同じnode constructorからpaid producer/provider/checker、V5 continuation、source producer/checker、readonly paid観測を起動できる。`--private-probe-paid-generation-export=ABSOLUTE_JSON` はrootのV5 own-state exporterへ接続する。exact original signed claim・expected sequence/current consentから得た330-byteの不変metadata fileはcontinuationの入力だけであり、source承認・支払や置換の権威ではない。paid-v4でも明示exportだけのreadonly nodeを使える。V5 continuationと元のpaid producer/replacementの排他制約は維持する。

`export_private_source_paid_bootstrap_v1` は別のentry pointで共通profileの完全なParamsを検査し、その実network/rulesetでartifactを再検査・署名する。旧paid-v4 exporterのexact factory制約を緩めず、旧profileのconsensus identityへ黙って新policyを混ぜない。primitive fixtureやstatement fileの作成自体はnative登録・conformance・source acceptanceの観測ではない。

`--private-probe-source-open=ABSOLUTE_JSON` は1つの実claimを元のproducer keyでOpenする。JSONはversion、claim、producer_bond/key、payer_key、funding_outpoint、maximum_fee_sompi、maximum_input_bytesを明示する。既存accepted claimを対象とする場合はproductionを省略できる。最初のsourceを自分で作る場合はproductionにbase_job、canonical Borsh model file、秘密のsalt、seal_funding_outpoint、claim_funding_outpoint、各stageのfee capとmaximum_workspace_bytesを明示する。seal/reveal/openは3つの別々の固定inputであり、自動補充・input置換は行わない。

実producerはcoherentなown epoch/stateから直接Programの登録classとpayerが既にpostした実jobを読む。paid enrollmentのあるbase jobは拒否し、paid claimが必要なsourceを自分で生成する循環を作らない。既存SDKの実work生成器へown reserved ledger projectionを渡し、有限workspace/work/materialを予約・照合した上で、parameter commitmentの一致、全positionの実行、decode、claim/evidence/commitmentを生成する。caller指定claim IDと一致した場合だけ不変commit WALを保存する。実行中にordinary blockが進んでも、最新own epoch/importと同じimmutable job/classを再照合できれば生成物は保持する。local fileはchain受付の代用にならない。

提出順はordinary native `SealClaim` → own ledgerでのseal受付・既存delay/TTL → `CommitClaimSalted` → own ledgerでの実claim受付 → `SourceBootstrapV1::Open` である。各stageは実rehearsal、既存bond/UTXO/maturity制約、明示fee capを通し、signed objectとexact carrierの不変WALを使う。再起動・reorgでは元の署名・carrier・inputのみ再送する。sealがexpireした場合に別inputや新しいsaltでやり直さず、UNKNOWN/未取得・不一致では承認やsource eventを作らない。その後のselected checkerは別の実署名slotとquorumを必要とする。

このproducerが自動作成するのはdirect Programのclaimであり、class登録・モデル審査・payer jobの代理作成ではない。class/jobは先に実native経路で受付されている必要がある。local model以外のnetwork取得、pipeline/EXEC等のscope adapter、fault/無回答の自動告発、selected verifierのslash/liability/実収支、後続consumerへのversioned lineage、全経路の受入campaignは引き続き残件である。コードは未build・未test・未実行であり、この節は合格証拠ではない。


## 2026-10-18 V6 downstream の明示private policyとworker接続

`source-paid-v1` の任意の追加file `--private-probe-paid-bootstrap-source-policy=ABSOLUTE_BORSH` はcanonical Borsh `(u64 activation, PaidBootstrapSourcePolicyV6)` を読む。policyの全値はoperator指定でありdefaultはない。absenceでは従来のsource-paid profileの意味とbytesを保つ。追加時はsource bootstrap policy ID、own network、source最大件数、非zero scheduled activationとnative mirrorを照合し、source/consumer policy以外をexact paid-v4 baselineへ固定する。全nodeと再起動時に同じpolicy content pinを必要とする。public presetと通常起動のrelease拒否は維持する。

V6を明示した新規producer workでは、署名前のdraftだけを `CommitWithApprovedBootstrapV6` へ変換する。declared commit DAAがactivationより前なら明示errorにし、実own DAAがactivationへ達するまで待つ。旧namespaceへのfallbackはしない。既存のsigned object/transaction WALを変換・署名し直すこともない。offline work exporterにも同じ新draft分岐を置き、V6の場合だけ `signed-paid-enrollment-v6.borsh` と `misaka.private-paid-native-work.v6` manifestを作る。旧exportのfile名/schemaはabsence時に維持する。ファイル作成はnative受付を意味しない。

新規V6 workのprivate paid specは `source_lock_funding_outpoint` と `maximum_source_lock_fee_sompi` を明示する。元claim用の全inputと別の固定inputで、実payerへ戻るsingle change、独立fee ceiling、bond-lock/maturity、実native rehearsalを検査する。V6にFund操作はない。own accepted enrollment/bindingが揃った後、`LockApprovedBootstrapV6` を `source-lock-v6.borsh` / `source-lock-carrier-v6.borsh` へ保存して実mempoolへ提出し、native effective lockを監視する。reorgでは同じ署名・input・carrierを再送し、spent coinやlocalmarkerを受付の代わりにしない。

private checkerはSDKの実material検査を維持し、own V6 effective lockが束縛するassignment/context/seedでだけ `ReceiptApprovedBootstrapV6` を認識する。cold outboxも新receipt namespaceを識別し、現在のown namespaceが異なる場合は同じ不変WALを保留する。旧receiptをV6へ再ラベルせず、old source fee/collectorのguardを削除しない。復元carrierは元inputの所属だけでなく、single changeの宛先、現在のpayer/fee、bond-lockとmaturityも再検査する。

このV6 private経路はnative consumerが対応するNonInteractive、interactive_planなし、review_scopeなしに限定する。compact/staged classはprovider fundingや実work生成の前に拒否し、他のscopeのconsumerが完成したとは扱わない。V6新規enrollmentとmaterial-generation原署名の互換は下記の別append V7契約で扱い、既存V5の原署名を黙って新namespaceへ変換しない。全てsource記述でありbuild/test/node受入はまだ実行していない。検証者の正直なstake・source供給/検閲・実費用/収益の数値選定は後の一回の統合campaignに残る。


## 2026-10-18 original V6 の明示 V7 material continuation

`--private-probe-kernel-paid-material-generation-v7=ABSOLUTE_JSON` は `source-paid-v1` と明示V6 policyを必要とする。private JSONのversionは7、`expected_generation_path` が指すnative generation metadataは既存のversion5・330 bytesのままである。旧V5 flagと同時には使えず、original paid producer/replacementとのWAL所有排他も維持する。元のsigned index16を読み、同じenrollment・commit・salt・source policyと原署名を保持したappend index19へ新しいouter署名を付ける。original index2は従来のV5/index15だけを使う。wrapperを新たなoriginalにしたり、旧署名・fileを再ラベルしたりしない。

V7 workerはown coherent epoch observationの同じretained native routeを用い、実producer key/bond、固定source policy/epoch、実class/jobのscopeとidentity、元のclassから導出した変更不能のlifetimeと有限受付期限を署名前に照合する。対応するscopeはNonInteractive・interactive_planなし・review_scopeなしである。current material generationのtyped resource fault、own history未取得・破損・terminal状態は保留する。current取得失敗をinitial consentとみなさず、元transactionの再送を許す場合も、同じown route上のActive initial recovery contractとsequence0の正確なroot/sponsor/nonce/consentを予約付きで読む。第三のgenerationでは新署名・fundingを行わない。SDK/nativeは両署名・現在のmaterial recoveryとpolicyをさらに再照合する。

追加fundingは元claim用の全input、元producer bondおよび元V6 source-lock用inputから独立した一つの固定inputとし、別fee ceilingを明示する。新WALは元producer directory内の `material-generation-v7-SEQUENCE` に保存する。既存V5 directory名は維持する。復元carrierは元の署名object、固定input、single payer change、現在のUTXO/payer/bond-lock/maturityとfee capを再照合する。再起動/reorgでは既存old/new carrierのどちらかだけを再送し、別input・再抽選・受付期限の延長を行わない。local WALやspent coinはnative受付を意味しない。

own accepted enrollmentが存在すれば新wrapper・replacement feeを作らず、同じprocessがsource Lock段階へ自動的に進む。手動のproducer再起動やcampaign node argv変更は不要である。original paid producerとV7 continuationは同じshared Lock helperを呼び、同じexclusive `producer.lock` の下で、元producer specのsource-lock input・元payer key・別fee capと元directoryの `source-lock-v6.borsh` / `source-lock-carrier-v6.borsh` だけを使う。continuationの追加inputはそのlock inputから独立している。元specにlock authorityが無ければV7起動を拒否し、別inputや暗黙のfee予算を補わない。

shared helperは新しいtyped own coherent source snapshotからexact same accepted enrollment・V6 binding・明示policy・producer key/bondを再照合する。既存effective lockはmonitorし、新しい抽選を行わない。Lock未受付なら同じindex17 object/transactionを復元し、期限・native rehearsal・payer/UTXO/maturity・single change・fee cap・残りworkspaceを検査してbroadcastする。async mempool queryの後にもown checkpoint/import epoch/DAAとrehearsalを照合し、変わっていれば次tickへ保留する。再起動/reorgでは元のLock WALと固定inputを使い、carrierだけ存在して原署名objectが欠ける場合は再構築しない。old/new continuation WALとsource-lock WALはともに不変で、local markerやspent coinをnative受付として扱わない。legacyV5 accepted時のmonitor動作は維持する。

namespace・source policyの原署名束縛、wrapperをoriginalへ使えないこと、rollback時のexact old/new選択と第三generation保留のsource controlsを追加したが、未実行である。build・test・node・測定は開始していない。これらと実work/selected quorum/後続assignment・精算・再起動/reorg/IBD/pruningを、同じ候補binary・同じtestnetの一回の統合campaignで受け入れる作業が残る。正直なstake、source供給/検閲、verifier実収益とslash/liability、自動告発および未対応scopeの課題をこのworkerで解消済みとは扱わない。
