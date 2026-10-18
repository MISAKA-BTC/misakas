# RFC07: selected approval を持つ最初の source の接続

状態: **UNCOMPILED / UNEXECUTED。新契約に加え、有限epochのnative journal・承認slot・履歴settlementからの新source eventと独立versionのcollectorを実装中。own-chain resolverの全呼出境界、実worker、後続assignment consumerは未完了。公開 activation や数値の承認ではない。**

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
