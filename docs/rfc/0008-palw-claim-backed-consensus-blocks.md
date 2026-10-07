# RFC-0008: PALW EXEC lane — claim-backed work slices and transaction execution

Status: Revised Draft, 2026-10-08 — design only; no runtime change, activation height, fingerprint or testnet rule change

Original proposal: 2026-10-03. Stable filename retained for existing links.

> **設計の優先規則:** heartbeat・BASE-0を弱めず、現行mainのliveness構造を正とする。claim-backed sliceは既存ROUND/EXEC laneへ統合する。[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)の公開prosecution要件も維持する。文書の改定は実装・activationを意味しない。

## 0. 要旨と旧提案の置換

**既存EXEC laneを、Final済みclaimから得たtransaction permitだけでなく、進行中claimのauthenticated work sliceも運べるPALW execution laneへ拡張する。** block classは増やさず、REAL / EXEC / HEARTBEAT / BASE-0の4役とする。同じlaneを使っても、round creditとslice work creditは別の意味・別の台帳を持つ。

旧RFC8の「sliceを新しいalgo-11のselected-parent / consensus BLUE / DAA候補にする」「sliceでchain clockを進める」「floorのmerge admissionを変更してモデルBLUE shareを上げる」という方針は撤回する。本改定はheartbeatの条件・重み・頻度、BASE-0の資格・fallback・anchor duty、120秒chain cadence、現行mainのclock cursor・REAL tick・GHOSTDAGを変更しない。slice到着数や実行中sessionの存在を、heartbeatやBASE-0を止める根拠にしない。

旧algo-11設計本文は削除し、試験結果・未実施項目だけを[v0試験記録](../design/palw/rfc-0008-v0-test-record.md)に残す。[ADR-0169](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md)は新設計への参照に置き換える。新しい実装対象は[EXEC統合仕様v1](../design/palw/rfc-0008-implementation-spec.md)である。旧試験結果をEXEC sliceの合格実績として引き継がない。

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

rootとsliceに同じworkのrewardを二重払いしない。既存のclaim allocation・per-DAA発行上限・escrow資金制約を維持し、slice blockごとのcoinbase subsidyを追加しない。slice数を増やしてもclaim allocationは増えない。Tx feeは既存の取引会計に属し、work rewardとは別である。

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

deterministic traceのcommitmentだけで物理的な計算時刻を証明できるとは主張しない。precomputation、fork reuse、root/jobの複製、Sybilによる計算credit再利用をwork identityと台帳で防ぎ、その限界を仕様とUIに表示する。

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

旧v0のalgo-11、slice-clock、floor排除の試験はこの受入を証明しない。未解決gateは実装未完了として列挙し、placeholderでactivationを許可しない。供給/検証能力は実測で判断する。

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

本RFCと[EXEC統合仕様v1](../design/palw/rfc-0008-implementation-spec.md)が2026-10-08以降のRFC8実装対象である。既存の実測・旧分岐実装の記録を、現行mainの実装状態や新設計の合格実績として扱わない。
