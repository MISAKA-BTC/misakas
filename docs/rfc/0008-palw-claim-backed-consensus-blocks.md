# RFC-0008: PALW EXEC lane — claim-backed work slices and transaction execution

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Revised Draft, 2026-10-10後続改定 — immutable identity, model acquisition non-interference, model-bond allocation and Rule E; MISAKA Torrent/Seeder rewards abolished; new implementation/economics/activation pending; historical evidence unchanged

Original proposal: 2026-10-03. Stable filename retained for existing links.

Spec closure 2026-10-15: R8-* requirement groups are closed or parameterized in text by [仕様閉鎖 — 2026-10-15](#spec-closure-2026-10-15). This is a document revision only; no implementation PASS, activation or fence height is implied.

> **設計の優先規則:** heartbeat・BASE-0を弱めず、現行mainのliveness構造を正とする。claim-backed sliceは既存ROUND/EXEC laneへ統合する。[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)の公開prosecution要件も維持する。文書の改定は実装・activationを意味しない。

## 0. 要旨と旧提案の置換

**既存EXEC laneを、Final済みclaimから得たtransaction permitだけでなく、進行中claimのauthenticated work sliceも運べるPALW execution laneへ拡張する。** block classは増やさず、REAL / EXEC / HEARTBEAT / BASE-0の4役とする。同じlaneを使っても、round creditとslice work creditは別の意味・別の台帳を持つ。

旧RFC8の「sliceを新しいalgo-11のselected-parent / consensus BLUE / DAA候補にする」「sliceでchain clockを進める」「floorのmerge admissionを変更してモデルBLUE shareを上げる」という方針は撤回する。本改定はheartbeatの条件・重み・頻度、BASE-0の資格・fallback・anchor duty、120秒chain cadence、現行mainのclock cursor・REAL tick・GHOSTDAGを変更しない。slice到着数や実行中sessionの存在を、heartbeatやBASE-0を止める根拠にしない。

旧algo-11設計本文は削除し、試験結果・未実施項目だけを[v0試験記録](../evidence/palw/implementation-records/rfc-0008-v0-test-record.md)に残す。[ADR-0169](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md)は新設計への参照に置き換える。新しい実装対象は[EXEC統合仕様v1](../design/palw/execution/rfc-0008-implementation-spec.md)である。旧試験結果をEXEC sliceの合格実績として引き継がない。

## 1. mainを基準とする4役

本改定のコード確認基点は`MISAKA-BTC/misakas` main `282355ba9`。現行roundのalgo idは10で、[ADR-0125](../adr/0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)と`palw_execution_lane_v1`のFinal credit / schedule / permitによるfee-only laneである。新しいEXECという呼称だけで、[ADR-0168](../adr/0168-an-execution-block-is-a-third-class-and-the-chain-reaches-it-through-an-anchor.md)の未導入のanchor trailerや着色方式を実装済みと扱わない。

| 役割 | payloadと資格 | chainでの責務 |
| --- | --- | --- |
| REAL | 新しいordinary PALW useful work。短い推論のcomplete claim、または長いclaimを開くroot useful work | 現行の資格・weight・clock規則でchain consensusを担う |
| EXEC | `EXEC_TX`: Final済みcredit由来のTxPermit。`EXEC_SLICE`: 進行中root claimの非重複work | chainにanchorされる高速execution lane。selected parentにならず、weight 0、DAA 0 |
| HEARTBEAT | 現行mainのheartbeat | 現行のclock/livenessを継続する |
| BASE-0 | 現行mainのemergency fallbackと必要なanchor duty | 長期的なREAL不在など、現行条件で復旧を担う |

EXECの全subtypeについて、raw blue work、PALW `safe_weight` / `immature_weight`、consensus blue score、DAA tick / retarget contribution、pruning hierarchy contributionを**追加しない**。EXECのFinal work accountingからfork-choice weightへ戻す経路も作らない。REAL root自身の現行規則で認められたworkだけがREALのweightになる。

EXECのcanonical acceptanceとconsensus BLUEは別の事実である。legacy roundがREDとして格納されることと、将来のRPCでEXEC classを表示することを混同しない。EXECの大量発行が通常blockのk anticone budgetを消費したりREALをREDにしたりしないことを、共通lane実装の受入条件にする。表示を変えるだけではこの条件を満たさない。

## 2. 同じEXEC class、独立した2つのpayload

将来の共通envelopeは概念的に次の形とする。型名は設計名であり、現在のRust APIではない。

```rust
PalwExecV2 {
    version,
    network,
    anchor,
    subtype,
    tx_permit: Option<RoundPermitV1>,
    work_slice: Option<ClaimSliceV1>,
    payload_root,
    executor_bond,
    signature,
}
```

**初期releaseは`EXEC_TX` / `EXEC_SLICE`の排他的subtypeを採用する。** `EXEC_TX`にはTxPermitとtransaction batchを、`EXEC_SLICE`にはWorkSliceとwork material commitmentを載せる。どちらも同じEXEC class・chain非進行laneであり、第5のclaim-backed block classを作らない。

初期releaseでは両fieldが存在するenvelope、両fieldが欠けたenvelope、subtypeとfieldが一致しないenvelopeを拒否する。`EXEC_SLICE`はユーザーtransaction batchを運ばず、WorkSliceの資格だけでtransaction fee収入や取引枠を得ない。fee/bondの資金確保は、chainで受理済みのroot claimまたは既存の資金経路から行う。

将来、1 EXEC blockに両payloadを載せる拡張は可能だが、別のversioned仕様と受入試験を必要とする。両条件を独立に検証した上でblock全体をatomicに受理/拒否する。consensusの「block部分受理」は初期版に入れない。

### 2.1 EXEC_TX: 過去のFinal creditによる高速取引

```text
過去のclaimがFinal → 既存credit / schedule → round permit
→ EXEC_TXでtransaction batch → chainで受理 → fee収入
```

既存permitのspan、round、index、operator/domain、bond、payout、one-use、quota、parity、gas budgetを維持する。WorkSliceがあることをTxPermitの代わりにしない。TxPermit消費は新しいLLM計算・claim reward・work weightを生成しない。

### 2.2 EXEC_SLICE: 現在のclaimに属する新しいwork

```rust
ClaimSliceV1 {
    root_claim_id,
    slice_index,
    class_id,
    canonical_job_id,
    kernel_version,
    plan_root,
    canonical_range,
    predecessor_state_root,
    result_state_root,
    evidence_root,
    da_root,
    executor_bond,
    signature,
}
```

active root claim、認められたexecutor bond、canonical plan/range、直前state、証拠・DA、署名を検査する。rangeはPALW-TIR/kernelの決定的な境界で導出し、credited workはcanonical IR costから計算する。自己申告CCU/PWU、任意の極小slice、slice数、header数からcreditを算出しない。

署名はnetwork/version/subtype、anchor、block payload commitment、root、index、range、前後state、evidence/DA、bondを拘束する。root/plan/class/kernelへのbindingが不完全なsliceを受理しない。rootはchainで受理済みのcanonical claimであり、未受理のroot宣言だけではsliceを発行できない。

## 3. REAL rootと長い推論

```text
短い推論: REAL → complete claim → 現行の検証/challenge → Final

長い推論: REAL (root claim / session open)
           ├─ EXEC_SLICE 0
           ├─ EXEC_SLICE 1
           ├─ EXEC_SLICE 2 ... N
           └─ claim complete → 検証/challenge/DA条件成立 → root claim Final
```

REALはchain-level claim anchor、EXECはそのclaimの内部計算を運ぶlaneとする。rootはclass/job/input、bounded total work、slice plan、初期state、DA、executor authorization、bond/exposure、expiryをcommitする。

**sessionを開く宣言自体を、実行済みuseful workとして扱わない。** root REALは現行REALのticket、実計算、admissionを満たす必要がある。rootに実計算prefixを含める場合、そのrangeを共通work台帳に登録し、後続sliceで再度creditしない。未実行の全session予算からrootのweightや即時rewardを得ない。root-openを現行REALにどう適合させるかは実装gateであり、任意のtx-carried session objectをREALに昇格させる抜け道を設けない。

初期版ではsliceの検証結果をroot claimへ集約し、**root claimがFinalになって初めて、そのclaimのslice work rewardを確定する**。各sliceが独立claimとしてFinal、permit、reward、weightを獲得する旧v0の方式は採用しない。completeは全canonical rangeと最終stateが揃った事実であり、Finalと同義ではない。

## 4. Accounting: permitとworkを分離する

| 台帳 | キー・意味 | 何を発生させるか |
| --- | --- | --- |
| TxPermitUse | 既存`(span, round, permit_index)`。Final creditに基づく取引資格のone-use | 有効transactionの受理とfee。新規work reward/weightは0 |
| WorkSliceUse | `(root_claim_id, slice_index)`とcanonical range。pending / verified / final / voided work | root Final時の一度だけのwork reward。取引permit消費・weight・clockは0 |
| RootWorkBudget | job/rootのcanonical total、root prefix、pending/final ranges、escrow、settlement marker | rootとsliceの重複禁止、claim単位の一回の精算 |

全historyで次のconservationを満たす。

```text
root_credited_work + Σ credited_slice_work <= canonical_claim_work
credited ranges are pairwise disjoint
Σ settled_work_reward <= the claim's funded reward allocation
EXEC fork-choice weight = 0
EXEC DAA contribution = 0
```

rootとsliceに同じworkのrewardを二重払いしない。（2026-10-15補足: 全positive legのroot native Final前無支払、bound kernel feeのhold、受理前Q/R予約、fee-only Roundの`ExecutionCap`、v1 market feeの`R_max`外の扱いは末尾の仕様閉鎖節R8-BUDGETを規範とする。）既存のclaim allocation・per-DAA発行上限・escrow資金制約を維持し、slice blockごとのcoinbase subsidyを追加しない。slice数を増やしてもclaim allocationは増えない。Tx feeは既存の取引会計に属し、work rewardとは別である。

sliceの受理・positive verification・complete・Final・報酬解放を分ける。Final済みcanonical workが将来の既存scheduleへ入る場合も、**root全体の一つのaggregate Final creditから一回だけ**導出し、既存のeligibility / quota / capを維持する。rootと各sliceを別々のFinalとしてsnapshotに投入しない。EXEC_TXが消費したpermitを再生成しない。

root expiry、missing DA、verification backlogではpending workをtimeout/voidの規則で処理し、検証の沈黙をFinalにしない。rootやsliceのfraudは該当するstate依存の後続sliceを無効化する。証拠・義務に基づくconvictionのみでslashし、verifier不在やlocal timeoutをproducer arithmetic fraudにしない。

## 5. Anchor、reorg、livenessの分離

EXECは既存laneと同じchainへのattachmentを持つ。sliceは**root claimのanchor**と**carrierのchain anchor**を別々に拘束し、canonical branch上のactive rootのみを対象とする。reorgでroot/anchorが外れたsliceは新しいbranchのworkとして自動採用しない。再attachmentはversioned再検査を必要とし、同じworkを二度settleしない。

共通EXEC acceptanceは、chainのparent stateから決定的にcovered set・順序・資格を検証してfoldする。v2のanchor closureはconsensus-parent walkから分離し、stale headやslice burstがchain template・merge depthを塞がないようにする。既存v1のmergeset経路と、将来のanchor closureを混ぜず、具体的なwire commitmentと互換性は実装仕様で固定する。

不正なEXEC blockは拒否し、laneで資格を失ったcarrierはそのlaneの受理対象から外す。producerが勝手に送った不正sliceだけでchain blockやrootを無効にしない。chain blockが宣言したanchor commitment自体が不正なら、既存のblock commitment検査に従う。slice fraudはworkの検証・精算を止めるが、独立したEXEC_TXのpermitや確定済み取引をsliceの結果に依存させない。

heartbeatとBASE-0はsliceのpending数、検証結果、header種別、node-local producer観測を読まない。court、DA、verifier、長い推論の全てが停止しても、現行mainと同じ規則でchainとdeadline machineryが進む。bounded queues、reserved chain-validation資源、EXEC_SLICE独立quotaを設け、slice floodでheartbeat、BASE-0、REAL、EXEC_TXをstarveさせない。

例: 20分のsessionから2分ごとに10 slicesを運べる。それは**DAA +10、10倍fork-choice power、10倍rewardを意味しない**。clockは現行REAL/heartbeat等のactive規則で進み、EXECの進行は0のままである。

## 6. 公開検証・裁定・証拠保持

[RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md)、[RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md)、[ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)に従い、active Kernel/plan/suiteをroot・slice・receiptにbindする。通常検証は指定されたencoded/algebraic constraint検査、異常時は有界exact courtへのlocalizationとする。未対応演算にVM代替経路を設けない。

各sliceのinitial/final state、predecessor、非重複range、証拠とboundary間の最初の不整合を、普通の非Panel public bondがproducer秘密状態なしに検証・訴追できる必要がある。[RFC14](0014-panel-independent-fraud-prosecution.md)のdispute、model/material availability、liability、retention gatesを保ち、多数派receiptで有効fraud proofを無効にしない。[RFC15](0015-panel-free-permissionless-verification.md)のPanel=0は別の全gate成立前に有効化しない。

whole-claim誤受理確率はslice/boundaryの検査を合成する。同じ証拠やchallengeの複数carrierを独立検査と数えない。reorg/pruning/IBD後もpublic prosecutionとFinal後の回収に必要なmaterialを保持する。EXECのweightが0でもpending reward、collateral、DA、courtのexposureは0ではない。

**2026-10-08共通challenge protocol:** [RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)を唯一の正本とする。root/class/planは`challenge_policy_id`をbindし、各`WORK_SLICE` subjectはroot、index、canonical range、前後state、evidence/oracle rootsをchallenge前に固定する。future source windowはそのslice statementのcommit後に始める。root-open時に既知になったseedを、後で自由に作れる全slice statementへ再利用しない。whole-root検証も同じpolicyのbinding/coverageに従い、seed式をRFC8側で追加しない。

EXEC_TX、heartbeat、BASE-0、未検証sliceはbeacon entropyにならない。独立検証・Final・G14・source資格を満たした長いroot useful workを将来sourceにする場合も、root work identity単位で一回だけであり、N carrierをN sourceに数えない。beacon不足はpending/既定deadline処理であり、sliceにweight/clockを与えたりheartbeat・BASE-0を抑止したりしない。

deterministic traceのcommitmentだけで物理的な計算時刻を証明できるとは主張しない。precomputation、fork reuse、root/jobの複製、Sybilによる計算credit再利用をwork identityと台帳で防ぎ、その限界を仕様とUIに表示する。

### 6.1 モデル入手への不介入・bond配分と規則E — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

EXEC_TX/EXEC_SLICEのweight/DAA=0、unique-work、root単位一回精算を維持する。
適用modeのclaim固有Final/court条件・暫定creditとsafe settlement・規則Eの全writer/reader/undoを検証する。
model配布不能だけの停止を起動せず、閉鎖時の未検出workを必ず排除できるとは主張しない。

## 7. 実装境界とactivation gates

| 境界 | 新しい実装対象 |
| --- | --- |
| `palw_execution_lane_v1`とversioned EXEC envelope | 同じlaneのTX/SLICE subtype、domain-separated payloadと独立資格 |
| root claim / PALW state / fold | canonical plan、one-use range、root aggregate Final、escrowと一回のsettlement |
| header / GHOSTDAG / difficulty / sync | 全EXEC subtypeがselected-parent/weight/DAAに入らないこと。consensus anticoneからの隔離 |
| chain template / execution acceptance | bounded anchor closure、parent-state verdict、stale-head recovery、atomic carrier acceptance |
| producer / relay / RPC / explorer | root REALとslice scheduler、独立quota、class/subtype/lifecycleの明示 |
| verification / DA / court | public material、boundary checks、positive verificationとobjective conviction |

この表は設計対象であり、現行コードをこの文書だけで変更する指示ではない。新しいversioned EXEC fenceとroot lifecycle/accountingが必要だが、slice clock fence、BASE-0を排除するmerge-admission fence、新しいalgo-11 chain classは導入しない。既存wire v1・schedule・fingerprint・active network presetsはこの改定で変更しない。

activation前に、少なくとも以下を独立に実証する。

1. **共通lane隔離:** TX/SLICEとそのburstがselected parent、blue score/work、PALW weight、DAA/retarget、k anticone、pruning hierarchyを増やさない。
2. **liveness同値:** producer/verifier/DA/courtの停止、RED spam、slice flood、遅い推論でもheartbeat・BASE-0の資格、clock、anchor duty、復旧が基点mainと一致する。
3. **work/reward保存:** prefixとsliceの重複、skip、別root/job/branch replay、duplicate Final、再起動、reorgで二重credit・二重reward・二重permitが起きない。
4. **資格独立:** invalid WorkSliceが有効TxPermitを変えず、invalid TxPermitがwork creditにならない。初期版のmixed payloadをatomicに拒否する。
5. **public prosecution:** slice/boundaryの各不正、DA default、false Validを有界materialから区別し、公開bondで裁定できる。未解決disputeのままFinalにしない。
6. **資源・移行:** pending depth/work/bytes、open roots、closure walk、retention、queuesをboundedにし、activation境界、旧node、IBD、pruning、同時到着順で同じ結果を示す。

旧v0のalgo-11、slice-clock、floor排除の試験はこの受入を証明しない。2026-10-15の仕様閉鎖節が各gateに対応する規範規則と名前付きパラメータ（USER DECISION REQUIRED）を追加した。未解決gateは実装未完了として列挙し、placeholderでactivationを許可しない。供給/検証能力は実測で判断する。

## 8. 観測と段階導入

まずmainのliveness・EXEC_TX基準値を固定し、root/slice accountingとlane隔離をshadow replayする。次にisolated drillでroot REAL → EXEC_SLICE → positive verification → root Final → 一回の精算を実行する。migration、fraud、DA停止、reorg、restart、pruned join、floodを確認した後に、別途readinessに基づくactivationを審査する。このRFCのmergeでlive規則は変わらない。

観測はREAL / EXEC_TX / EXEC_SLICE / HEARTBEAT / BASE-0別のcarrier数、accepted/pending/verified/final/voided work、rootのcanonical/credited work、Tx fee、work reward、clock進行、空slot、queue、DA/court latencyを分ける。EXEC carrier数をselected-chain shareやconsensus BLUE shareへ足さない。

旧proposalのP2観測（8k attempt 72本中BLUE 1、0.19本/120秒slot等）は過去の運用報告であり、本改定で再測定した値ではない。slice化は計算を細かく運べるが、同じ一台の計算能力を増やさない。本RFCの合格条件を「model BLUE share 90%」から、**非重複workを安全に運び、現行livenessと取引laneを維持し、一つのclaim予算内でFinal/報酬を完結すること**へ変更する。

## 9. 関連設計との優先関係

- ADR-0125のFinal credit / round permit / fee-only transaction経路を維持して一般化する。
- ADR-0168のchain非進行、parent hygiene、anchorの方向は参照する。ただし§10.3のroundへの`safe_weight`配分は本RFCのEXECには適用しない。TX/SLICEともweight 0で、rootのweightをEXECへ再配分しない。
- 旧algo-11設計、per-slice Final/weight/reward、slice clock、floor排除の設計本文は削除する。ADR-0169は置換の案内とし、過去の試験結果だけを独立記録に残す。
- ADR-0140/0142/0165のmain上のactive heartbeat・clock・BASE-0構造を維持する。旧RFC8のfuture slice-clock/class-aware floor変更を、この改定の前提にしない。
- ADR-0173 / RFC14の公開prosecutionと、RFC15のdeferred activationは維持する。

本RFCと[EXEC統合仕様v1](../design/palw/execution/rfc-0008-implementation-spec.md)が2026-10-08以降のRFC8実装対象である。既存の実測・旧分岐実装の記録を、現行mainの実装状態や新設計の合格実績として扱わない。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](0015-panel-free-permissionless-verification.md)を適用する。

root/receipt/権利償還と対象slice/rider/batchを同じproducer bond予算へ束縛する。一claimから派生する全block・報酬・Final weightを受理時に予約し、行使・Finalでも検査する。
容量拡大で総権利を増やさず、Final/retired/reversalを含む全writer/readerとundoを統一する。EXEC_TX/EXEC_SLICEのweight/DAA=0と既存発行・work一意性を維持し、未解決claimから不可逆settlementを許可しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

## 改定 — 2026-10-12（モデル配分とブロック生成機会の接続：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3A.5。

ADR-0177 2026-10-12改定のモデル配分（Miner＋Panelの適格claim bondと1.3〜2倍の係数、提案）をブロック生成機会へ接続する場合、coinbase配分だけの変更として済ませず、ブロック機会の帰属・上限・移行を本RFCで明示する。物理blockの総頻度、DAA/難易度、fork choice、Final weightを同時に変更するとは決定しておらず、特にFinal weightへ係数を自動乗算しない。必要な変更は別のversioned review・試験・activationを要する。

<a id="spec-closure-2026-10-15"></a>

## 仕様閉鎖 — 2026-10-15（設計監査の要件索引 R8-* に対する規範文）

**状態と読み方:** 本節は[設計監査の要件索引](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引)のR8-*群について、本RFC本文に欠けていた規範規則・未定義パラメータ・後続設計記録との矛盾を閉じる文書改定である。上位は[PRINCIPLES](../PRINCIPLES.md)とADR-0173/0176/0177/0178であり、衝突時はそれらと後続のユーザー判断を優先する。本節は実装完了・検証PASS・activationを意味しない。公開fenceは全て不在のまま、release高さを変更しない。dormant codeに入っている数値はmechanics/fixture値であり、政策承認ではない。mechanicsの成功やprivate acceptanceを独立review・経済証拠として扱わない。詳細は[EXEC統合仕様v1](../design/palw/execution/rfc-0008-implementation-spec.md) §§12–15と各設計記録に置き、本節はその規範的要約である。

「USER DECISION REQUIRED」は、値の決定にユーザー判断を要する名前付きパラメータを示す。推奨値は既存記録から根拠を示せる場合だけ記す。

### R8-LANE — 排他envelope、carrier、header carriage

1. §2の排他subtype規則を維持する。加えて、同じwork/claim/rootを運ぶcarrierの数、同じsliceの再送、header carriage内の追加認証byteを、consensus position、permit、fork-choice weight、DAA、retarget、乱数source、独立検査の数として数えてはならない（MUST NOT）。
2. **PAV3 header carriage（`palw_monetary_material_v1`配下、休眠）:** 新しいnative attemptはPAV2の署名済みattemptを不変のまま内包し、事前に署名済みのmonetary-material sessionへ結び付ける。規範条件は次のとおり。
   - 選択するsessionは、未結合で、class/artifact/executor bond/operator/consensus導出job policy/受理区間がattemptと一致し、未chargedの**異なるprovider operatorを2以上**持つものの中で、session root鍵が最小のもの（canonical oldest-key）である（MUST）。producerは別のfunded sessionを選んで追加のdrawを得てはならない。producerとconsensusは同じ選択関数を使う。
   - Readyはsession/consent/claim/execution key/manifestと未chargedのprovider roster（鍵順）を拘束し、各providerはReady idへ署名する。providerは2以上8以下（8はcarriage上限65,536 byte内に全署名・sealを収めるための構造上限）。
   - executorはReady id、attempt id、全provider署名をroster順に含むsealへ署名する。sealはheader段階でattempt自身の鍵によりstatelessに検証する（MUST）。これはprovider署名byteを書き換えて一つのsolveから第二のheaderを作るmalleabilityを閉じるためである。provider鍵・executor鍵が登録bondのものかはstatefulなadmissionで検査する。
   - provider署名とsealはLayer-0 ticket tagの外にあり、価格付け・lottery entropyにならない（MUST NOT）。block identityには含まれる。re-sealでticketを得られない。
   - fence未満では、PAV3を新しい規則で判定してbytesを拒否してはならない。旧releaseとの混在で分裂しないよう、新しいheader形式は自身のfenceまでA-2共通規則（decode不能と同じ扱い）に従う。
3. `PalwMaterialSessionMinProviders`（名前付きパラメータ）: 一sessionに要求する異なるprovider operatorの最小数。不変条件 `2 <= min <= PALW_MATERIAL_MAX_PROVIDERS_V3 (8)`。休眠codeは構造上の下限2を実装している。**value: USER DECISION REQUIRED**（推奨: 2を下限として維持。理由: 1 providerではmaterial保持が単一点故障になり、2は実装済みの構造下限である。これ以上の値は実測したprovider不履行率から決める）。

### R8-WORK — 同単位のwork、prefix/suffix scope

[実装仕様 §12](../design/palw/execution/rfc-0008-implementation-spec.md#12-work-design-correction--execution-cost-is-not-lottery-weight-2026-10-10-audit)を規範とする。

1. REALの抽選込みweightとexecution costは別の量である。credited workは、登録IR、一つのwhole-job facts、絶対positionから求める10次元execution cost vectorとし、prefixと各suffixを同じ単位・同じplanから導出する（MUST）。自己申告CCU/PWU、slice数、carrier数からcreditを導出しない。
2. class登録DAAがcost policyのsnapshotを固定する。登録後のcost table変更は既存classへ遡及しない。
3. 宣言された`total_work`/`boundaries`は導出planと一致しなければならず、生成数は宣言costと照合する。prefix bindingでは、jobの最大長と実際の生成長の両方をREAL contextの非零exact decodeと照合する（MUST）。range input commitmentは実prompt由来とする。
4. 内部演算はchecked arithmeticとし、node cost `< 2^54`、各次元 `< 2^120`、10次元合計 `< 2^124` を超える入力を拒否する（§12.5）。共有DAGの走査はvisited setで各構造nodeを一度だけ訪れる。
5. 履歴の再実行（restart/replay/reorg）を新しいuseful workとして数えない。
6. `ExecWorkReleaseResourceBounds`（名前付き）: 最悪graph、root population、256-root restartのlatency/memory上限。**value: USER DECISION REQUIRED**（release閾値。既存記録に推奨値なし）。

### R8-IDENTITY — useful workの一回利用

1. `job_work_id`はclass、REAL job authorization、認証済みinputだけから導出する（MUST）。cuts、output依存の初期state、verification metadata、continuation extent、root id、owner label、bondに依存させてはならない（MUST NOT）。任意のinput hashでkeyを変える経路を拒否する。
2. 一つのauthorizationで複数rootを開けない。記録が無いことを再利用の許可と扱ってはならない（MUST NOT）。新しいauthorizationは正当な同一inputの再実行を許す。
3. 有限authorization（§12.6）: root openingは `max(source_daa, accepted_daa) <= opening_daa < source_daa + L_auth` の間だけ許される。`L_auth`は休眠v1で100,000 DAA固定、job-use履歴は最大100,000行・1 blockあたり最大256件の期限切れ削除で、期限内の行を追い出さない。これらはv1構造profileであり、release memory/latencyや経済admission capの承認ではない。`L_auth`・履歴上限の本番値: **value: USER DECISION REQUIRED**。
4. 元のclockはnative claim受理時に捕捉する（§12.7）。後のheader可用性や呼出側bytesで変更できない。legacy claimは明示的なcarried original-header proof（§12.8）でのみ移行する。source/acceptanceの順序を仮定しない開始条件とする。
5. Remaining（観察、未検証）: 全eligible legacy headerのcoverage/transport可用性、実DAG/multi-process pruned replay。

### R8-BUDGET — 予約、Final前無支払、実debit

1. **Final前無支払:** slice leg、bound kernel fee、Panel fund、EXEC executor fund、riderを含む全てのpositive legは、root claimのnative Final前に支払ってはならない（MUST NOT）。bound kernel claimの計算Finalはreward 0とし、job escrowをconsumer所有のhold（kernel table 31）に保持する。同じjobの別producerのFinalもholdを迂回できない（§13.1）。
2. **root Final精算:** root Finalで一つのallocationのexecutor share内から実際のfeeを支払い、同額のnative issuanceを減らし、未使用escrowをposterへ返す（MUST）。actual debit、payout、root marker、refundを同じdeltaに記録する。void/expiry（ready rootの有限expiryを含む）は全holdを無支払で閉じる。既払prefix/sliceの後付けは拒否する。
3. **受理前予約:** accepted claim、fee、bondの書込みより前に共有Q/R claim budgetを予約する（MUST）。standalone kernel claimはQ/Rだけを予約し、BとFを与えない。round drawは各bondの残存権利から候補を作り、一つでも予約が失敗すればdraw全体を取り消す（原子的）。
4. **fee-only Round（2026-10-11ユーザー判断）:** fee-only Roundは`B_max`に数えず、資本比例の独立`ExecutionCap`で権利割当時に制限・課金する。失敗・未実行で権利を回復させない（MUST NOT）。`ExecutionCap`の資本比率: **value: USER DECISION REQUIRED**。
5. **market fee（2026-10-11ユーザー判断）:** v1ではmarket feeを`R_max`に含めない。ただし期待手数料収入（fee利得）は経済安全性・抑止条件の計算に含める。fee額そのものの上限を証明したとはしない。（設計監査「追加受入条件と政策判断」の「market feeとissuanceの合計を`R_max`に含め、約定前にfee最大額を予約する」という記述は、[readiness matrix](../design/palw/integration/activation-readiness-matrix.md)に記録されたこのユーザー判断と矛盾するため、v1については本項で置き換える。）
6. **予算window `W_budget`（ADR-0176 D4）:** 400 DAAはdrill暫定値である（2026-10-11）。条件は対象profileが`L <= W_budget`とslot上限を満たすこと。長いG14 staged経路へそのまま適用しない。RFC12のwork量`W`とは別物である。本番値: **value: USER DECISION REQUIRED**（推奨: 400 DAA、上記条件付き。理由: ユーザーがdrill用に採用した値であり、それより長い経路には別の値が必要）。
7. **allocation freeze:** retained EXEC rootがleadを名指しした後、riderによるescrow再配分を拒否する（§15.7）。
8. **状態（2026-10-15、lead記録）: closed-as-provisional。** R8-BUDGETの仕様は次の値を**drill暫定（ユーザー判断10-11）、本番値はcensus/実測待ち**として閉じる: 予算window `W_budget` = 400 DAA（対象profileが`L <= W_budget`とslot上限を満たす場合のみ。長いG14 staged経路には適用しない）。fee-only Roundは`B_max`の外で、資本比例の独立`ExecutionCap`に従う（比率の値は未決）。v1のmarket feeは`R_max`の外。presetは変えず、fenceは武装しない。

### R8-WINDOW — 依存claimの責任・material・post-Final

1. **依存担保の床:** bindingの時点で `H_exec = expiry + max(native_window_court, kernel_liability) + kernel_court_deadline + kernel_proof_grace` をchecked arithmeticで依存claim行に固定する（MUST）。計算Final、pre-Final default、root fee精算、cancelでこのhorizonを短縮してはならない（MUST NOT）。支払済み・failed・予約ゼロ・期限切れのworkの後付けを拒否する。restoreは全依存のbase floorを再導出する。
2. **Settled後の有罪:** Settled後の依存claimのconviction/defaultは、rootのnative Final、safe weight、native evidence、未解放vesting、payout全体へ一貫して伝播する（MUST）。kernelは自身のcollateralを処罰し、伝播だけを理由に他actorを追加slashしない。`void_claim`だけで巻戻しを済ませない。
3. **native紛争期限:** native DA/courtの延長は、依存pathの最短の署名済みhorizon内に収まらなければならない。超える延長は拒否する（§15.4）。既存termを越える延長は、新しい署名・新しい資金による明示の同意だけで行う（未実装）。
4. **X3（2026-10-12ユーザー判断）:** claim単位の絶対期限（固定点d0＝claim受理時）を延長しない規則は新しいversioned fenceでのみ適用する。live t12の旧規則は変えない。
5. **workerの同意:** `nonce_v2 = H(slice-job-nonce/v2; nonce_v1 || expiry || H_exec || executor_exposure_sompi || policy_root)` を署名済みkernel claimで拘束し、最初の署名済みsliceでだけ予約する（§15.6）。allow-listは同意ではない。
6. **Panel署名lifetime（inner 66/65、aux 121、`palw_panel_lifetime_v2`）:** producerはadmission前に絶対受理区間・service/liability endpoint・retry/pause予算・最大PWU/金額へ署名し、native birthで実statementを捕捉する。retryは同じendpointを共有し、pauseはcounterをresetしない。expiryは時間内のcourtを中和せず、獲得済み現金を消さない（MUST NOT）。名前付きパラメータ`PalwPanelLifetimePolicyV2 {maximum_service_daa, maximum_pause_daa, maximum_retries, maximum_enrollments, maximum_enrollments_per_producer}`（不変条件: `0 < maximum_service_daa < u64::MAX`、`maximum_pause_daa <= maximum_service_daa`、`0 < per_producer <= maximum_enrollments`、security/economic review scopeが非零）: **value: USER DECISION REQUIRED**。休眠codeのprobe値は政策値ではない。
7. **monetary material covenant（outer 240–242、aux 120、`palw_monetary_material_v1`）:** 全provider・sponsorが完全なroot/依存、capacity manifest、担保、自己資金fee、物理的支払window `W_mat`、payment-relative義務に署名する。liabilityまたはqueueが残る間、holdはMAXとする。materialの解放は**最後の実支払 + `W_mat`**以後であり、release blockが現在のselected chainでfinalizedであることを物理GCの条件とする（MUST）。sparse DAAやqueue滞留でproviderの新しい義務は期限切れにならない。欠落delta、古いtip、未完了import、generation変化、契約の不在はGCを許可しない。名前付きパラメータ`PalwMonetaryMaterialPolicyV1 {max_contracts, max_contracts_per_provider, max_bytes_per_contract, max_bytes_per_provider, minimum_collateral_sompi, minimum_fee_sompi, window_daa (= W_mat), response_window_daa, max_enrollment_daa, cost_report_root}`（不変条件: 全て非零、`max_contracts_per_provider <= max_contracts`、`max_bytes_per_provider >= max_bytes_per_contract`、`cost_report_root`は承認済みreview由来）: **value: USER DECISION REQUIRED**。固定前払feeを未知の支払・回復時間の安全な対価と表示しない（`PaymentAndRecoveryTimeUnknown`）。
8. paid material: native paid契約の`material_until`は署名済みEXEC retention horizon以上でなければ、fee/R移転・root/work-use書込みの前に拒否する（§15.11）。
9. Remaining（観察、未検証）: 実nodeのnative root Final後の公開告発と支払回復、承諾付きhorizon延長、pruned join、workerの最低対価・資本費用。
10. **承諾付きhorizon延長（2026-10-15、休眠実装）:** `palw_horizon_extension_v1`（kernel inner 68、aux 126）。EXECに束縛されたkernel claimの生きている`liability_until`を、worker（claimのproducer）とsponsor（jobのposter）が同一termsへ各自署名して延長する。全員の署名が揃った時点で原子的に適用し、(a) 延長だけ（Final・court窓を短縮しない）、(b) claim固定のhard deadline（claim受理`d0`からの`maximum_claim_horizon_daa`、EXEC rootのnative claimにX3が適用される場合はその`dispute_hard_deadline`との`min`）を越えない、(c) 期限切れhorizonを再開しない、(d) 獲得済み現金を取消さない、(e) EXEC income holdを同じhorizonまで延長する、(f) transferred claimのproviderは自身の署名済みlease（tag 150）が新horizonを既に覆うことで同意する（leaseを書換えない）。名前付きパラメータ`PalwHorizonExtensionPolicyV1 {maximum_extension_daa, maximum_consent_window_daa, maximum_claim_horizon_daa, maximum_open_extensions, require_price_contract, review scopes}`: **value: USER DECISION REQUIRED**。

### R8-SLICE — 重複・再利用・一精算

1. 同一executorの支払はslice毎にfloorせず、executor毎のworkをchecked arithmeticで合計してから一度だけfloorする（MUST）。slice分割の変更で同じexecutorの支払額を変えない。
2. 新しいleg全体は未使用残高で制限し、不足時はexecutor毎に比例配分して整数端数をroot legに残す。advanceがallocation全体を既に超える場合、新しい支払は0。
3. 公開prefixの再利用、同じ計算Final行の再適用、claim/revealの再送で二度精算しない。rootは一度だけ精算する。
4. Remaining: native root Final／精算とその再起動・reorgまでの閉包（validation-only）。

### R8-NEUTRAL — 順序・分割の不変性

1. admission、ticket、allocation、payoutの結果は、operator id、genesisでの順序、class/bondの登録順を入れ替えても変わらない（MUST）。tie-breakは内容由来のcanonical keyで行い、登録順やoperatorの身元を優先しない。
2. 同じ資本をbond、model、claim、root/slice/riderに分割しても、Q/B/R/Fの合計、ticket、モデル配分（ADR-0177 D3の`S_m`）を増やしてはならない（MUST NOT）。
3. 登録operator idを独立した経済主体の証明として扱わない。同じ実質所有者が複数operatorを持つ可能性を残余リスクとして明記する。
4. per-bond ticket上限は`B_max`から導出したものを使い、別の値を定義しない。
5. Remaining: 全callerでのswap/split試験（validation-only）。

### R8-RECOVERY — 世代一致、restart/reorg/pruning

1. budget（carriage tail `0xEF`、delta 190/191）とEXEC（tail `0xEE`、delta 180/181）、reward vesting（`0xF4`）、actor extension（`0xF5`）、job-use履歴（`0xF2`）は同じ世代でcarriage/復元する（MUST）。片方を捨てる統合を禁止する。
2. native clock（F3/table 8）と明示的migration proofはrestore・undoで同じ結果を与える。rootはlocal header lookupに依存しない。
3. material GCは、active profileがcommitted generationを読めない、sinkが読取中に変わる、pruning importが進行中・不安定・読取不能・epoch変化の場合、`Some(u64::MAX)`でholdを保持する（MUST）。読取errorを「義務なし」と解してはならない。
4. pruned joinは、同じ認証済みauthority、source/failure/fee、material commitmentとjournalを完全に受け取らなければならない。snapshotに収めるために履歴のfailureや失敗promiseを落としてはならない（MUST NOT）。
5. Remaining: settled native root、OS crash、migration履歴の全coverage、multi-process P2P/実pruningの証拠（validation-only）。

### R8-ECON — 収支基準

1. [§15.7](../design/palw/execution/rfc-0008-implementation-spec.md#157-worker-price-criterion-and-allocation-freeze)の基準を規範とする: `p_collect_lower * M_net_min >= C_upper`。`p_collect_lower`はnative Finalとrelease時の回収可能性の**同時**事象の証明済み下限であり、独立性の根拠なしに周辺確率を掛けない（二つの周辺下限a, bからは`max(0, a+b-1)`）。資本は各時点で一度だけ数える。
2. 費用・回収floor・funded minimumの証拠が欠ける場合の判定はUNKNOWNであり、肯定判定にしない（MUST NOT）。未実装の上限を実装済みとして比較しない。
3. 名前付きパラメータ `capital_rate`、class別`C_upper`の測定値、`M_net_min`を拘束する最低価格契約: **value: USER DECISION REQUIRED**（既存記録に推奨値なし。最低価格契約は未実装）。
   （2026-10-15補足: 最低価格契約は休眠fence `palw_minimum_price_v1`（kernel inner 69、aux 127）で実装した。workerとposterが同一termsへ署名し、jobのGAP-5 escrowが`M_net_min`を賄い、まだworkerが予約されていない時だけ受理する。`C_upper` = class別測定値 + `capital_rate`×資本時間面積（一度だけ）、`required_M = ⌈C_upper / p_collect_lower⌉`未満の申出、未測定class、`p_collect_lower = 0`（UNKNOWN）、u64を越える見積りを拒否する。契約が支配するjobは契約workerのclaimだけを受理期限内に受理する。名前付きパラメータ`PalwMinimumPricePolicyV1 {capital_rate_ppm, capital_rate_period_daa, p_collect_lower_ppm, class_cost_upper, maximum_contracts, maximum_contract_daa, cost_report_root, economic_review_scope}`: **value: USER DECISION REQUIRED**。精算時の最低額強制（settlement enforcement）は未実装。)
   （2026-10-15 round 3補足（休眠source、未compile・未test）: **精算時の最低額強制**と**期限切れ契約・古い同意のsweep**を実装した。(a) `load_ledger`は署名済み契約ごとに、そのjobを保持する契約workerのclaimへ`funded_minimum_sompi`をpayout floorとしてkernel ledgerの`consumer_minimum_payouts_v1`（consumer注入、row/carriageではない、restoreで空）へ入れる。(b) kernel ledgerはFinal（およびEXECの`finish_exec_reward`）でjob escrowから`paid = min(escrow, posterの残bond, consumer cap)`を計算し、`paid < floor`なら**少なく払わず**、契約結果をneutralとする: producerへのescrow支払いは0、escrowは全額posterへ`ReleaseJobEscrow`で返す（`paid ≥ floor`なら従来どおり）。fenceより下ではfloorが空で、支払いはbyte同一。(c) kernel routeのclosing stepの先頭（そのblockのobject適用後、block DAA）で決定的sweepが走り、aux 127の契約行（とjob index）を次で退役させる: 全署名前に`accept_by_daa`を過ぎた同意＝StaleConsent、`retention_until_daa`超過＝Expired、`accept_by_daa`超過かつ契約workerのclaimなし＝Unused、job escrow消滅（支払・neutral返却・idle返却）＝Settled。aux 126の延長行は未適用で`consent_by_daa`超過（古い同意）、または適用済みで`new_liability_until_daa`超過で退役する。退役はcapacityを解放するだけで、現金・claim・horizonを動かさない。(d) restore fenceは、route ledgerのDAAで既にsweepが退役させたはずのStaleConsent/Expired契約と古い延長行が残っていれば拒否する。式: `floor(claim) = funded_minimum_sompi`（署名済み契約、`job_claims[job]`のproducer = worker）; `paid = min(amount, collateral_poster, cap)`; `paid < floor ⇒ paid := 0, ReleaseJobEscrow(amount)`。新しい値は導入しない。neutral時にworkerへ何も払わない規則の妥当性（契約上の不払い責任の所在）は**USER DECISION REQUIRED**。)
4. Remaining: 需要・cap前後・window飽和時の実測比較（validation-only）。

### ALL-MODEL

§6.1とADR-0177を維持する。追加で、2026-10-12のユーザー判断に従い、モデルbytesを強制するcourt経路はRFC6の2026-10-15節に記す休眠の制限fenceで閉じ、新しいdemand unitはclaim固有のwitness/state/traceに限る。公開検証できない証拠経路（N4）は新fence以後のclaimを新規報酬・weight対象から除外する。

### 本文との関係

§4の会計表と§7のgateは本節で補完される。旧記述を削除せず、衝突する場合は本節と[実装仕様](../design/palw/execution/rfc-0008-implementation-spec.md) §§12–15を優先する。設計監査の旧記述「実際に回収可能なmarket feeとissuanceの合計を`R_max`に含める」は、v1では2026-10-11のユーザー判断（market feeは`R_max`外）で置き換えられたものとして扱う。
