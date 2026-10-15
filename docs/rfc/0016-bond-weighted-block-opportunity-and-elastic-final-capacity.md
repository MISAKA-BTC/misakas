# RFC-0016 — bond加重のブロック生成機会と弾力的Final容量: 物理ブロックと報酬対象ブロック権利を分けて設計する

> **設計前提:** [PRINCIPLES](../PRINCIPLES.md) §0（2026-10-14、モデルの非公開を経済的に不利にする。不正は排除でなく有界化）を最上位とする。
> [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)（bond単位のQ/B/R/F上限と共通拘束`d + W_hold`）、
> [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)（モデル入手への合意不介入、モデル別配分）、
> [RFC-0015 §8.3–8.5](0015-panel-free-permissionless-verification.md)（2026-10-14確定: `α = 1.5`、30日平均、Miner＋Verifier元本、ブロック機会にも適用、`W_hold` = 7日相当DAA）を前提とし、変更しない。
> 本RFCは、2026-10-14決定の「モデル間のブロック生成機会を`P_m`で配る」を
> [ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)の共通work target抽選とどう接続できるかを分析する。
> あわせて、人気モデルが検証容量の不足でFinal待ちになる問題を扱う。

**トークンの表示名は Misaka、ticker は BILI。** 既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Draft, 2026-10-15（同日のユーザーreviewで改定）。**設計検討中の文書であり、採用された設計はない。**
  §6の案B+は検討を続ける作業仮説であり、採用されていない。また、案B+は元の目標（§0.1）を物理ブロックについて達成しない（§5）。
* **Implementation status:** not implemented。§1の検証は既存コードの読み取り記録であり、build・test・実node試験は行っていない。
* **Source baseline:** `origin/pre` `4fb381a87`と、統合branch `integ/rfc02-rfc06-12-g14` HEAD `7dd86f2a0`
  （RFC02＋RFC06/07/08/12＋G14公開conviction＋RFC-0015休眠実装に`4fb381a87`をmerge）。
* **Activation:** なし。設計用の未割当fence名として`palw_model_block_rights_v1`と`palw_elastic_final_capacity_v1`を使う。既存Paramsのfieldではない。
  高さ・wire tag・RPC op・署名domainは割り当てない。
* **Not activated by:** 本RFCやその休眠実装のmerge、test PASS、配分値の承認だけ。
* **Normative language:** MUST / MUST NOTは、将来その規則を採る場合の要件として書く。採用を意味しない。型名・関数名・fence名は設計用である。

## ユーザー判断 2026-10-15（本RFCのreview、拘束）

| # | 判断 | 本文への反映 |
| --- | --- | --- |
| R1 | RFC-0016はDraftのまま、案B+の設計を続ける。B+を採用済みとも、元の目標を達成する設計とも書かない。「推奨＝採用」と読める表現を除く | Status、§6冒頭、§12。「推奨」を「作業仮説」に置き換えた |
| R2 | **最優先:** 「物理ブロック生成機会」（共通W抽選の当選と、実際に生成されたblock）と「報酬対象ブロック権利」（台帳上の権利）を分ける。B+は物理抽選を変えないので、bondを増やしてもそのモデルの物理blockは直接には増えない。経済的誘導による収束を主張するなら、収束条件と反例を書く。物理抽選を変えずに元の目標を満たせるか、満たすには物理層で何が要るかを分析する（決定ではない） | §0.1、§5（全体） |
| R3 | 確定事項は再検討しない: `α = 1.5`、30日平均、共通拘束7日相当。280–436 DAAと400 DAAは**7日相当へ直すべきcodeのずれ**であって選択肢ではない。個別capを、モデルが配分を使い切るためだけに引き上げてはならない。使い切れない配分は報告し、cap引き上げで解決しない | §1.2 X3、§6.6、§12（旧U6・U15を削除） |
| R4 | Panel=0について、他の議論より先に、(a) Kernel claimとそれを運ぶblockの対応、(b) B/R/Fを誰がいつ予約するか、(c) coinbase subsidyと利用者払い（escrow）報酬の区別を確定して書く。RFC-0015のlead default（`B = F = 0`）は保留中のdefaultで決定ではない。維持・変更それぞれの帰結を示す | §2（前提節） |
| R5 | 試験表に7項目を合格基準付きで追加する | §9.3 |
| R6 | 計測は、配分目標`P_m`、物理block数、付与された報酬権利、Final数、実際の支払、既存minerごとの個人収入を**別々に**報告する。目標シェアの増加だけを根拠に、実際のblock生成や収入の改善を報告してはならない。受入試験は物理・支払の列で測る | §8、§9 |

## 0. 要約

### 0.1 二つの「ブロック生成機会」

| 用語 | 定義 | 決めるもの | 現行コード |
| --- | --- | --- | --- |
| **物理ブロック生成機会** | 共通`W_work`抽選に当選し、実際にDAGへ入るモデルblock | cadence、DAA、fork choice、Final weightへの寄与 | ADR-0137の抽選。1 forwardの当選確率は`min(1, CCU_m / W_work)`で、bondは入力にない |
| **報酬対象ブロック権利** | 台帳上で報酬（coinbase）の対象として数えるblock 1本分の権利 | そのblockのclaimに払う報酬 | 未実装。休眠中の配分engineはモデル予算の不足時に報酬額だけを削る |

元の目標「bondが多く集まるモデルほど多くのブロック生成機会と報酬を獲得し、人気モデルがFinal待ちで不利にならない」は、この二つを区別していない。
本RFCではこう読む。

* **報酬について:** `P_m`で配る報酬対象ブロック権利とcoinbaseの**目標**は、bondに対して単調に増やせる（§6）。
  ただし実際の支払は、そのモデルで実際に生成されFinalになったclaimがなければ発生しない。
* **物理ブロックについて:** 案B+は物理抽選を変えない。したがって**bondを増やしても、そのモデルの物理blockは直接には増えない**。
  物理blockが`P_m`に近づくのは、§5の条件が全部揃い、minerが計算を移したときだけである。反例は§5.3にある。
  物理抽選を変えずに、物理blockについて元の目標を保証する方法はない（§5.4）。

### 0.2 現状の結論（§1）

* 現行・統合のどちらでも、物理blockの本数は計算量だけで決まる。
* `P_m`は文書上は確定している。コードは休眠のminer-only・epoch snapshot版で、報酬額を削るだけである。
* Panel=0のkernel claimには抽選も物理モデルblockもない。報酬は利用者払いのescrowで、coinbaseは使わない（§2）。
  このため`P_m`の「ブロック機会」と「coinbase」は、Panel=0の経路にはそのままでは適用先がない。
* 検証容量はPanel経路ではready seatに比例する。ただしclass間の干渉、C7の静的上限、事前確認の競合という穴がある。

## 1. レビュー主張の検証

`pre` = `4fb381a87`、`HEAD` = `7dd86f2a0`。行番号はその時点のfileのもの。

### 1.1 判定表

| # | 主張 | 判定 | 根拠（pre / HEAD） | HEADでの差分・補足 |
| --- | --- | --- | --- | --- |
| (a) | ADR-0137は単価型difficultyを共通`W`に置き換えた。`p_m = min(1, CCU_m / W)`で、モデル別claim数は入力にない | **CONFIRMED** | `consensus/core/src/palw_admission_v2.rs` pre L1130–1146 / HEAD L1134–1150 `palw_effective_class_target_v1`。`palw_work_target_v1.rs` L116–125 `palw_work_ticket_target_v1`。floorはpre L1106 / HEAD L1110 | HEAD L1095–1103に`native_material_ticket_v3`分岐を追加。同じ関数を読むので規則は同じ。claim数はticketの入力ではないが、容量admissionの入力ではある |
| (b) | `W`はepochごとに`max(W₀, W·実blocks/目標blocks)`で、変化率に上限がある。全モデル共通。t12 shipped presetはDAA 0から有効 | **CONFIRMED** | `palw_work_target_v1.rs` L94–114（`clamp(W/4, W·4)`、L34）。`config/params.rs` pre L10377–10401 / HEAD L10499–10523 `palw_t12_shipped_params`、HEAD L12303 | 行番号の移動のみ。RC preset（t11系）は7,101と7,780（HEAD L12022・L12038） |
| (c) | Panel容量とinflight上限が新claimを拒否する。8kは窓3・同時約5、2Mは2,799・最大1。人気モデルのFinal処理量は検証資源で決まる | **PARTLY** | `palw_state_v2/fold_read.rs` `check_class_admits_claim` pre L901 / HEAD L901–1063。`ClassInflightCapped` pre L1002・L1046 / HEAD L1016・L1060。`PanelRoomExhausted` pre L1019・L1034 / HEAD L1033・L1048。`palw_work_target_v1.rs` L205–245 | 次の4点で不正確。(1) 8k行はaudit fence後、上限5ではなくrate roomだけで判定される（L222–229）。(2) roomはそのclassのready seat数に比例する（HEAD L1209–1217・L1266）。(3) 他の全classの未払replayを自classの供給から引く（`palw_state_v2.rs` HEAD L13130–13146）。(4) HEADのOPV経路には検証容量のadmissionがなく、担保だけを見る（`misaka/palw/kernel/src/ledger.rs` L2116–2129） |
| (d) | RFC-0015は30日平均の`P_m = S̄^1.5/ΣS̄^1.5`をブロック機会とcoinbaseに適用する。engineは休眠で、既存設計は報酬額だけを制限する。α = 1.5のブロック配分は実際のブロック選択に接続されていない | **CONFIRMED**（追加あり） | RFC-0015 §8.5.5 pre L626 / HEAD L627–674。`palw_bond_budget_v1.rs` L1–15。Rだけのclip: pre L2634 / HEAD L2680。arm時は拒否: pre L684 / HEAD L711–734。設計書`bond-budget-and-model-allocation.md` §3.5 L296–308、§3.6 L310–315 | コードの`S_m`はminer資本だけ（tag 140）で、epoch snapshotとseasoningを使う（L10、L520–531）。30日平均とverifier元本は未実装。kernel経路での誤適用は§2.4 |
| (e) | 人気モデルのbacklog、bond配分と共通W抽選の衝突、個別capで大きい`P_m`を使えない | **CONFIRMED**（将来リスクとして） | (c)・(d)とX1・X4 | 衝突の詳細な分析は§5 |

### 1.2 レビューが扱っていない事項

| # | 事項 | 根拠 | 扱い |
| --- | --- | --- | --- |
| X1 | kernel claimは`B = F = 0`を予約する（RFC-0015のlead default、ユーザーreview待ち）。報酬はjob escrowで、coinbaseではない | `palw_bond_budget_fold_v1.rs` L1501–1545、`misaka/palw/kernel/src/ledger.rs` L147–151 | §2で前提として確定させる |
| X2 | RFC08 R8-BUDGET: fee-only Roundは`B_max`外の独立`ExecutionCap`、v1 market feeは`R_max`外 | `docs/rfc/0008-palw-claim-backed-consensus-blocks.md` L292–301 | 権利層の対象外（§6.8） |
| X3 | **codeのずれ:** 共通拘束`W_hold`は7日相当DAAで確定している（RFC-0015 HEAD L588・L647）。一方コードの許容範囲は280–436 DAA（`palw_bond_budget_v1.rs` L32–37）、RFC08の`W_budget`は400 DAA（drill暫定）である | 同左。`activation-readiness-matrix.md` L318–320 | **選択肢ではない。** 280–436と400は7日相当へ直すべきずれとして記録する（§11.2のC1）。ADR-0137の`W_work`（MAC-eq）とRFC-12の`W_pwu`は別の量で、ずれではない。記号を分けて書く |
| X4 | ADR-0176の個別capは資本に線形、`P_m`はα = 1.5で超線形。上位モデルの配分はcapで使い切れないことがある | `palw_bond_budget_v1.rs` L796–813・L876–912。ECONの集中比1.4–1.8倍（`activation-readiness-matrix.md` §3g） | **capは引き上げない**（R3）。使い切れない分は報告する（§6.6） |
| X5 | ADR-0177 D4（使い切れない枠をcapを超えて再配分しない）、D5（coinbase配分の変更だけで物理block頻度・ticket・DAA・難易度・fork choiceを変えない） | ADR-0177 L89–122・L238–250 | §5.4・§6.7 |
| X6 | M\*-49 verifier pay（休眠）は検査数`N = 1`、slot `m = 4`の固定定数で、請求率に比例しない | `palw_verifier_pay_v1.rs` L1–35 | §7.6 |
| X7 | attestation gate（2026-10-14維持）は、承認されないmodel-epochにモデル配分を払わない。claim・Final・weight・slashには触れない | `PRINCIPLES.md` §0-1、readiness §3f・§3k | §6.7 |
| X8 | F-R（`palw_capacity_verify_room`）ではclass roomの各bondの取り分がstake比例。出荷presetでは休眠 | `palw_verify_capacity_v1.rs` L21–30、`params.rs` HEAD L10769–10790 | §7 |
| X9 | block検証はadmission（HEAD L1083–1084）の後に抽選（L1095–1103）を見る。producerの事前確認はあるが予約ではない | `palw_state_v2.rs` HEAD L12217、`palw_producer_v2.rs` L246–259 | §7.4・§7.5 |
| X10 | ADR-0137 D5は「an allocation is a share」としてモデル別配分を否定した | ADR-0137 L303–325 | 本RFCは物理層でADR-0137を保ったまま、配分を権利層に限ることを検討する |
| X11 | `408f2171e`のcommit message（「Block issuance is not scaled by bond, there is no 30-day average」）は、17分後の`18987a94b`で上書きされた | `git show 408f2171e`、`18987a94b` | 確定事項は`18987a94b`（R3） |

## 2. 前提: Panel=0のclaim・block・予約・資金の対応（R4）

以下は統合HEADの休眠実装から読み取った事実と、その帰結である。値と選択はユーザー判断として§12に残す。

### 2.1 (a) Kernel claimとそれを運ぶblock

1. **運搬:** kernel経路のobjectは「one object, one transaction」で運ばれる。claimを提出する`CommitClaim`も、1つのtransactionとしてblockに入る
   （`palw_kernel_route_fold_v1.rs` L6–12）。そのobjectを適用するのは、それを受理するchain blockのfoldである。`ClaimCommitted`はそこで発生する（L1395–1420）。
   reorgでは受理blockが変わり、予約もそれに従って作り直される。
2. **抽選がない:** kernel claimはATTEMPTのticketを持たず、共通`W_work`抽選に参加しない。claimは利用者が投稿したjobに対して、seal → revealの順で提出される。
   1つのjobに1つのclaimで、`canonical_work_id`で一意である（`rfc-0015-implementation-status-2026-10-15.md` L73）。
3. **物理モデルblockを作らない:** claimを運ぶtransactionはどのblockにも入れる。そのblockはモデルblockではなく、claimのためのblockは生成されない
   （`palw_bond_budget_fold_v1.rs` L1501「Kernel carriers create no reward block or consensus weight」）。
4. **EXEC_SLICE:** RFC-0008のwork-slice blockはkernel claimを参照し、検証を受けることができる。weightとDAAへの寄与は0で、sliceごとのcoinbase subsidyはない（RFC-0008 L134）。
5. **V2 Attempt laneはPanelのまま:** 実装済みのPanel=0は、V2 Attempt route（共通`W_work`抽選、coinbase subsidyの経路）にPanel-freeの経路を与えていない
   （status doc L15–18）。抽選と物理モデルblockは、Panel=0でも**Panel検証のV2側にだけ存在する**。

**帰結:** Panel=0の経路には「物理ブロック生成機会」という量がない。claimの数は利用者のjob投稿数とproducerの能力で決まり、bondも抽選も決めない。

### 2.2 (b) B/R/Fを誰がいつ予約するか

| 予約主体 | 時点 | 内容 | 根拠 |
| --- | --- | --- | --- |
| jobの**投稿者のbond** | `PostJob`時 | `claim_reward`をescrowとして予約し、`job_fee`をburnする。M\*-49の検査料escrowも同時に積む | `ledger.rs` L1584–1617 |
| claimの**producerのbond**（ADR-0176台帳） | `ClaimCommitted`時（受理chain block） | Q = 1、**B = 0**、**R = job escrowの額**、**F = 0**。共通拘束`d + W_hold`はここから始まる。責任担保はkernel自身のreservation（OPV admissionの必要額）で、income holdは`liability_until`まで | `palw_kernel_route_fold_v1.rs` L1395–1413、`palw_bond_budget_fold_v1.rs` L1503–1545・L1395 |
| **モデル台帳** | 同上（配分fenceがある場合だけ） | kernel bindingで認証された唯一のnative classのモデルを`model`として渡し、`preview_claim_v1`がRを`model_available`でclipする | `palw_bond_budget_fold_v1.rs` L1514–1536、`palw_bond_budget_v1.rs` HEAD L2680 |
| verifier | — | claim単位の予約なし。検査料はjob escrowから払う（M\*-49、休眠） | `palw_verifier_pay_v1.rs` L1–12 |

### 2.3 (c) coinbase subsidyと利用者払い報酬の区別

* **kernel claimの報酬は利用者払い（escrow）だけである。** Final時にjobのescrowから一度だけ払い、新規発行はしない
  （`ledger.rs` L147–151「out of the job's escrow, never issued」、GAP-5のユーザー判断option A。fold側は`palw_kernel_route_fold_v1.rs` L22–27「Σ kernel payouts ≤ Σ kernel debits, with no coinbase rule involved」）。
* **coinbase subsidy（`R_PALW`）は、chain blockのworker carveとして積み上がる**（設計書§3.4、`palw_bond_budget_v1.rs` L11）。
  支払先はV2のAttempt/rider claimで、これはPanel経路である。
* RFC-0015 §8.5.5の`R_m = R_PALW · P_m`は**coinbase subsidyの配分**である。Panel=0のkernel claimには、現在これを受け取るlegがない。

### 2.4 発見: 配分fenceがkernel claimの利用者払い報酬をclipしてしまう

休眠コードでは、配分fenceがある場合、kernel claimのR（job escrowの額）が`model_available`でclipされる（§2.2の3行目）。
`model_available`はcoinbase carveから積んだモデル予算である。
つまり、**coinbaseの配分予算が利用者払いのescrowを削る**ことになり、資金の種類が混ざっている。
削られた分のescrowの扱い（producerへの未払いが投稿者へ戻るのか、burnか）も定義されていない。
このままarmしてはならない。§12のP4で判断を求める。

### 2.5 lead default `B = F = 0`の帰結（決定ではない）

| 選択 | Panel=0 claimへの`P_m`の効果 | その他の帰結 |
| --- | --- | --- |
| **維持**（`B = F = 0`） | 物理blockもブロック権利もない。`P_m`が触れられるのはRだけで、そのRは利用者払いなので、§2.4のとおり触れるべきでない。**結果として、Panel=0の経路では`P_m`は何も配らない**（新しいsubsidy legを作る場合を除く） | ADR-0176のB capはkernel claimを数えない。kernel claimのFinalはfork choiceに寄与せず、規則E・RFC-0014 §16.10への影響はない |
| **Bだけ>0**（会計単位、weightなし） | 権利台帳がkernel claimを数えられる。ただしsubsidy legがなければ、権利に払う資金がない | bond B capを消費する。各経路の予約・消費・undoを合わせる必要がある |
| **F>0** | — | OPV FinalがconsensusのFinal weightを持つ。fork choiceの変更で、規則EとRFC-0014 §16.10の再検証が要る |
| **kernel claimへのcoinbase subsidy legを新設** | `P_m`でsubsidyを配れる | 財源（どのchain blockのcarveか）、Final前の無払い、責任hold、escrowとの二重払い防止の設計が要る。総発行は増やさない |

本RFCの後続の議論（§5–§7）は、主にPanel経路のV2 Attempt laneを対象とする。Panel=0への適用は§2.5の判断（P1–P5）が出るまで保留する。

## 3. 用語と記号

| 記号 | 意味 |
| --- | --- |
| `W_work` | ADR-0137のwork target（MAC-eq）。共通抽選の値 |
| `W_hold` | ADR-0176の共通拘束期間。**7日相当DAA（確定）** |
| `W_pwu` | RFC-12のwork（pwu）。別の量 |
| `N_30` | 30日相当のDAA数（確定） |
| `S_m(k)`, `S̄_m(d)`, `P_m(d)` | RFC-0015 §8.5.5の定義（確定）。`P_m = S̄_m^1.5 / Σ_j S̄_j^1.5` |
| `c_m` | モデル`m`の物理blockシェア（実測） |
| `e`, `E_alloc` | 配分epochとその長さ |
| `U` | ブロック権利1本の固定小数単位 |
| `Pool(e)`, `Base_m(e)`, `Demand_m(e)`, `ψ_m(e)` | §6.2–6.3 |
| `σ`, `κ`, `K`, `ε` | spill上限、carry上限、carry保持epoch数、entrant reserve（§6.4–6.5） |

## 4. 目標性質の分解

| ID | 性質 | 層 | 案B+での状態 |
| --- | --- | --- | --- |
| G1t | `S̄_m`を増やすと、`P_m`（ブロック権利とcoinbaseの**目標**）が単調に増える | 権利 | 成立する（定義による） |
| G1p | `S̄_m`を増やすと、そのモデルの**物理block**が増える | 物理 | **保証しない**（§5） |
| G1r | `S̄_m`を増やすと、そのモデルの**実際の支払**が増える | 支払 | 条件付き（§5.2の条件が揃うとき） |
| G2 | 1 forwardの当選確率がモデルによらない（ADR-0137） | 物理 | 保つ |
| G4 | Final処理量がそのモデルの検証者容量に比例して伸び、不足は報告される | 容量 | §7 |
| G5 | cadence・DAA・難易度・fork choice・Final weight・総発行を変えない | 全体 | 保つ |
| G6 | 個別Q/B/R/Fと`d + W_hold`を超えない | bond | 保つ |
| G7 | 新規モデルが機会を失いすぎない | 権利 | §6.5 |
| G8 | 資本の付け替え・Sybil・分割・容量の占拠・自己取引で配分を増やせない | 全体 | §9.2・§9.3 |

## 5. 物理ブロックと報酬権利の分析（R2、最優先）

### 5.1 モデル

* minerは、1単位の計算あたりの期待報酬が最大のモデルを掘ると仮定する。B+の下で、モデル`m`の1単位計算あたりの期待報酬は`ψ_m · E / W_work`である
  （`CCU_m ≤ W_work`のとき、1当選に平均`W_work`の計算が要り、1当選の報酬上限は`E`）。
* `ψ_m < 1`のモデルから`ψ_m = 1`のモデルへ計算が移れば、物理シェア`c_m`は権利予算に近づく。これが「経済的誘導」の全部であり、規則で強制するものではない。
* 物理抽選は`W_work`だけで決まるので、**どのモデルのbondも当選確率を変えない**。物理blockを増やせるのは、そのモデルを掘る計算の増加だけである。

### 5.2 収束の条件

次の**全部**が成り立つとき、飽和領域で`c_m`は`P_m`（`σ`による幅を含む）へ近づく。

| ID | 条件 | 破れる例 |
| --- | --- | --- |
| C1 | 飽和: `W_work > W₀`で、期待cadenceを基準にする予算が拘束している | `W_work = W₀`の領域では予算が余り、全モデルで`ψ = 1`。誘導がない |
| C2 | 移動できる計算がある: minerの機材が移動先モデルを実行できる（VRAM/RAM、2M contextなど）。重みの取得（cold fetch、数十GB）、prefetch、class seating（`palw_class_seated_admits_v1`、`palw_state_v2.rs` HEAD L12233）、warm-upの費用が、期待利益差×期間を下回る | 機材がモデル専用、RAMが足りない、取得に時間がかかる |
| C3 | 情報: `ψ`の見積りを、計算の前にquoteで知ることができる（§7.4） | quoteがなければ、低い`ψ`を後から知ることになる |
| C4 | 移動先に検証容量がある | 移動先の容量が0か満杯なら、queue待ちか拒否になる |
| C5 | 移動先が報酬gate（onboarding/G14、lifecycle ACTIVE、attestation gate）を通っている | gateで拒否されたmodel-epochには払われない |
| C6 | 時間: `P_m`は30日平均でゆっくり動く。予算はepochごとに固定。収束には移動時間とepoch観測の両方が要る | 移動より速い変化（急増）には追いつかない |
| C7 | `σ`が小さい: `σ`が大きいほど`ψ`の差が小さく、誘導が弱い。`σ = ∞`では誘導がない（権利シェア = 物理シェア） | 大きい`σ` |

### 5.3 反例

| ID | 状況 | 物理block | 報酬権利・支払 | 元の目標 |
| --- | --- | --- | --- | --- |
| E1 | **minerがモデルを移れない**（機材・モデル専用設定、モデルサイズとRAM、cold fetch、seating）。Aはbondが多いが、Aを実行できる機材が少ない | Aの物理シェアはAを実行できる計算のシェアのまま。bondを増やしても変わらない | Aは`ψ_A = 1`で予算が余る。余りはcarry → σ上限付きspill → 留保。**ネットワーク全体の実支払が減る** | 物理・支払とも満たさない |
| E2 | **需要の急増**: Bの利用が急増し、計算がBへ集まる。Bのbondは少ない | Bの物理blockは抽選で急増する。共通`W_work`も上がり、**他モデルの1計算あたりの物理blockが減る** | `P_B`は30日平均で遅れて動く。`ψ_B`は下がり、Bのminerの1計算あたりの報酬が落ちる。利用者がBへbondを入れても、`S̄`へは30日かけてしか反映されない | Bは「人気なのに不利」になる。bondの多いAの物理blockも減る |
| E3 | **資本が多く計算が少ない**（Aがbond 80 %、計算20 %） | Aの物理シェアは20 %のまま | α = 1.5で`P_A`は80 %より大きい。使われない約60 %以上がcarry・spill（`σ`上限）・留保に流れる。Aのbond保有者が得るのは20 %分だけで、残りの多くは誰にも払われない | 目標（予算）は上がるが、物理・支払は上がらない |
| E4 | **計算が多く資本が少ない**（Bがbond 20 %、計算80 %） | Bの物理シェアは80 %。Bの当選で`W_work`が上がり、Aの1計算あたりの物理blockが減る | `ψ_B ≈ (Base_B + spill_B) / Demand_B`で、Bのminerは安く払われる。Aは`ψ_A = 1`でも、物理blockは増えない | Aのbondは物理blockを増やさない |
| E5 | **無報酬の当選で`W_work`を吊り上げる**（griefing） | 安いモデルで当選を積むと`W_work`が上がり、bondの多いモデルの物理blockが減る | 期待cadence基準の`Pool`は増えない。攻撃者は計算費用を払うだけ | bondの多いモデルの物理機会を、計算で削れる |
| E6 | **`W₀`領域**（計算供給が少ない） | 物理blockは計算どおり | 予算が余り、全モデルで`ψ = 1`。`P_m`は支払にも物理にも効かない | 目標シェアだけが動き、実体は変わらない |

### 5.4 物理抽選を変えずに元の目標を満たせるか

* **物理blockについては満たせない。** 物理blockは当選したforwardからしか生まれず、当選確率はbondを読まない。
  B+ができるのは報酬の目標を`P_m`で決めることと、計算の移動を誘うことだけである。E1–E6のとおり、誘導は保証にならない。
* **報酬について**は、§5.2の条件が全部揃えば支払は`P_m`へ近づく。揃わなければ、目標だけが上がって実支払は上がらない（E1・E3・E6）。
* **どの規則を採っても、計算のないモデルに物理blockを生ませることはできない。** 物理層で取り得る手段は、計算があることを前提にした次のものに限られる（分析であり、決定ではない）。

| 物理層の手段 | 効果 | 代償 |
| --- | --- | --- |
| P-i モデル別ticketの傾き（案D、`τ_m ∈ [1/k, k]`を`P_m`と`c_m`の比から決める） | bondが多いモデルは1計算あたりの当選が増え、他のモデルは減る | G2（計算単位の公平性）を最大`k²`倍まで崩す。ADR-0137 §3.4の人気ペナルティが有界に戻る |
| P-ii `P_m`を超えたモデルのadmissionを抽選の前に絞る（quoteで拒否） | 超過モデルの物理blockが減り、`W_work`が下がる。残りのモデルの1計算あたりの当選が増える（bondの多いモデルにも間接的に効く） | 計算の多いモデル（急増中の人気モデルを含む）を直接罰する（E2の悪化）。cadenceに空きができ、floorが埋める |
| P-iii モデル別のsub-target（案A） | 物理シェアを`P_m`へ直接制御する | cold start trap（`P = 0`の新規モデル）、30日平均の遅れによる振動、class DAA状態の復活、fork choiceに触れるupgrade |
| P-iv beaconによるslot割当（案C） | slotの割合を`P_m`にする | minerのいないslotが空く。beaconのgrinding、block validityの変更 |

いずれも、**元の目標の後半（人気モデルが不利にならない）と、計算単位の公平性（ADR-0137）の少なくとも一方を犠牲にする**。
物理blockの目標と人気モデルの非劣化は、計算とbondが一致しない状況（E2–E4）では同時に満たせない。どちらを優先するかはユーザーの判断である（§12のQ1）。

## 6. 案B+（作業仮説、未採用）: 共通W抽選＋モデル別報酬権利台帳

**この節は検討を続ける作業仮説であり、採用されていない。B+は物理抽選を変えないので、物理blockについて元の目標を達成しない（§5.4）。**

### 6.1 二層

1. 物理層は変えない（`palw_effective_class_target_v1`、`palw_work_target_step_v1`、cadence、DAA、fork choice、Final weight）。
2. モデル`m`のclaimが受け取る報酬は、`ψ_m(e) × claim自身のcarve`である。1 claimあたりの報酬はモデルによらず等しい（ADR-0137 D4）。
   したがって権利シェアとcoinbaseシェアは、同じ`P_m`から一度だけ決まる。`P_m`を二度掛けしない。
3. 権利台帳は、ADR-0176のbond単位のB次元とは別のモデル単位の台帳である。`ψ ≤ 1`なので個別capを超えない。

### 6.2 予算の基準量`Pool(e)`

* **R1:** 期待cadence（`closed_expected_blocks`、DAA数）× `U`。epoch開始時に決まり、grief耐性がある。`W₀`領域では拘束しない（§5.3 E6）。
* **R2:** 前epochの実現量 × `U`。常に拘束するが、希少性がないときも報酬を留保する。

どちらでも、支払はclaim自身のcarveの`ψ`倍以下であり、新しいmintはない（§12のQ2）。

### 6.3 epoch予算とclose時の比例消費

~~~text
Base_m(e)    = floor((Pool(e) - Entrant(e)) * A_m / sum_j A_j)    A_m = floor(S̄_m^1.5)
Budget_m(e)  = Base_m(e) + Carry_m(e) + Entrant_m(e)
Demand_m(e)  = U * (epoch e に受理され、close時点で有効な m の claim 数)   （無効化・有罪のclaimは数えない）
psi_m(e)     = min(1, (min(Budget_m, Demand_m) + SpillIn_m) / Demand_m)
~~~

* epoch内の先着順は使わない。同じepochの同じモデルのclaimは同じ`ψ`を受け取る。
* 支払は`max(Final, epoch close)`まで保持する。close後に無効・有罪となったclaimの分は、再配分せずに留保する。
* `ψ`の計算時に無効・有罪のclaimを`Demand`から除き、正直なclaimの`ψ`を薄めない。close後に判明したものは留保に回すので、正直なclaimの支払は減らない（§9.3のN7）。

### 6.4 未使用予算: carry、上限付きspill、留保

* spillを計算比例で全部配ると、権利シェアが物理シェアに一致し、`P_m`は効かなくなる（飽和時は`ΣUnused = ΣExcess`）。そのためspillには上限`σ`が要る。
* carryは`min(Unused_m, κ · Base_m)`を同じモデルが`K` epoch以内に使える。期限切れの分はspillへ回る。
* spillは`Excess_m > 0`のモデルへ、`A_m`の重みで、各`σ · Base_m`を上限に、model id順のwater-fillingで配る。
* 残りは留保し、発行しない（ADR-0177 D4）。

### 6.5 新規モデル

* 30日平均の欠損日は0として数える（確定した30日平均に従う。近道は作らない）。
* entrant reserveは`ε · Pool`を**固定総額の中から先に差し引く**。新規発行ではない（MUST）。
  配分は線形で、上限付き、適格なentrantだけに行う（登録から`N_30`未満、報酬gateを通過、attestation gateで拒否されていない、`S_m ≥ S_entry`）。
  使われなかった分はspillへ回る。
* reserveが増やすのは**報酬権利の目標**だけで、新規モデルの物理blockは計算でしか増えない（§5）。

### 6.6 個別capと使い切れない配分（R3）

* 個別Q/B/R/Fと`d + W_hold`は変えない。**モデルが配分を使い切れるようにするために、個別capを引き上げたり、cap policyを選び直したりしてはならない**（MUST NOT）。
* モデルのbondがcapに当たってclaimを出せない分は`Unused`となり、carry・spill・留保へ流れる。
  その量を`stranded_by_bond_caps_m`として**報告する。capの引き上げで解決はしない。**
* α > 1と線形capの組合せ（X4）や、verifier元本の多いモデルでは、使い切れない配分が構造的に生じ得る。これは報告対象であり、修正対象ではない。

### 6.7 既存決定との整合

| 決定 | 整合 |
| --- | --- |
| ADR-0137 | 物理層は変えない。D7（shareは読み手の数字）を改めるのは権利層だけ |
| ADR-0176 | bond単位の予約、cap、`d + W_hold`、残存責任は変えない |
| ADR-0177 D1・D4・D5 | 入手性を読まない。未使用分を発行しない。spillはcap以下。物理block頻度・ticket・DAA・fork choiceを変えない |
| RFC-0015 §8.5.5 | `P_m`を権利とcoinbaseへ一度だけ適用する（物理blockへの適用は§5.4のとおり未達）。登録者legは置かない |
| PRINCIPLES §0 | attestation gateで拒否されたmodel-epochは、`Base`・carry・spill・entrantのどれからも受け取らない |

### 6.8 対象外

fee-only Round（`ExecutionCap`）、v1 market fee、取引fee、ModelPosition手数料、floor class、Panel=0のkernel claim（§2.5の判断まで）は対象外とする。

## 7. 弾力的Final容量（Panel経路）

### 7.1 現状

* class容量 = readyなseat × replay速度 − 他の全classの未払replay。
* C7（2M行）は静的上限1。F-Rとρ clampは休眠。
* producerの事前確認はあるが予約ではなく、競合が起きる。
* OPV経路には容量待ちがなく、検査数は固定定数で決まる。

### 7.2 検証者単位の容量会計

* 各ready検証者`v`は予算`c_v`とready集合`M_v`を持つ。`Cap_m = Σ_v c_v · x_{v,m}`とし、`x`は需要比例の決定的なwater-fillingで求める。
* **1人の検証者の容量は、全モデルで合計して`c_v`以下である（MUST）。** 複数モデル・複数classで二重に数えない（§9.3のN4）。
* 検証者集合が交わらないモデルは、互いの容量を減らさない。
* 任意の検証者部分集合について`Σ load ≤ Σ c_v`（Hall条件）を満たすときだけ、容量として数える。

### 7.3 C7上限の導出

`cap_m = max(1, floor(sigma_cap · Cap_m · window_m / claim_replay_m))`。ただし`Cap_m = 0`なら0とし、新claimを受理しない（§9.3のN3）。

### 7.4 抽選前のadmission quote

読み取り専用。`admit / queue / refuse`、`ψ`の見積り、bondの残り、容量、余裕幅`h`を返す。
`Admit`はqueueの空きが`h`以上のときだけ返す。自nodeのside blockのattemptも需要に数える。

### 7.5 上限付きの公平なpending queue

* classごとに長さ`L_queue`。bondごとの取り分は資本比例。
* 検証窓とdeadlineはqueueを出た時点から数える。queue中はFinalにならず、報酬・Final weightもない。
* 受理時に予約するQ・責任担保・escrowは**1 claim分の最大値だけ**で、queue中に予約が増えることはない。
  権利台帳の`Demand`には、queueを出て受理が確定した時点で数える（§9.3のN5）。
* `T_queue`を過ぎたclaimはslashなしでvoidとし、Qの消費は戻さない。
* 予算超過のclaimは容量に空きがあれば受け入れるが、権利のあるclaimを押し出さない。

### 7.6 OPV経路

容量待ちはない。検査数`N`と`m`を請求率から導出し、`checked_share_m`を報告する。Panel=0への適用は§2.5の判断の後に行う。

### 7.7 検証者の誘因

* verifier元本は`S̄_m`に入る。
* 検証者取り分を、利用率に応じてADR-0132 Cの10–30 %の範囲内で傾ける。
* 総額は変えない。

### 7.8 不足の報告

`capacity_short_m`と不足量をRPCで返す。不足を黙ってFinal遅延にしてはならない（MUST NOT）。

## 8. 計測と報告（R6）

### 8.1 別々に報告する列

| 列 | 意味 | 出所 |
| --- | --- | --- |
| (1) 配分目標 | `S̄_m`、`P_m`（= ブロック権利目標 = coinbase目標） | 合意状態 |
| (2) 物理block | 抽選を通ったモデルblock数（skipされたown attemptは別列）、物理シェア`c_m` | block履歴 |
| (3) 報酬権利 | 付与した権利単位、`ψ_m`、spill入出、carry、留保、`stranded_by_bond_caps_m`、entrant付与 | 権利台帳 |
| (4) Final | Final数、queue期限切れvoid、有罪、`checked_share_m` | 合意状態 |
| (5) 実際の支払 | モデル別の支払額（sompi）と、資金別の内訳（coinbase subsidy / 利用者escrow） | 合意状態 |
| (6) 個人収入 | 既存minerごと（bondごと）の、epochあたり支払額と計算あたり支払額 | 合意状態＋reader |
| (7) 容量 | `Cap_m`、ready検証者数、owed、queue長と待ち時間、`capacity_short_m`と不足量、容量拒否の理由別件数 | 合意状態＋reader |

### 8.2 規範

* 目標シェア（列1）の増加だけを根拠に、実際のblock生成（列2）や収入（列5・列6）が改善したと報告してはならない（MUST NOT）。
* 受入試験の合否は、物理（列2）と支払（列5・列6）の列で判定する（MUST）。目標の列は参考値として併記する。
* 不明な値はUNKNOWNとし、PASSとして扱わない。

## 9. 受入試験

各試験は§8.1の全列を記録する。合否は物理・支払の列で判定する。

### 9.1 ユーザー指定の試験

| ID | ケース | 合格基準 |
| --- | --- | --- |
| T1 | 同じCCUの2モデル。AのminerとCPUがBの3倍 | Aの物理block数（列2）が上がることを測る。Aの支払（列5）と、既存Aminerの計算あたり収入（列6）を報告する。権利シェアの上昇は`σ`で決まる量として記録する |
| T2 | Aのbondだけを増やす | 目標（列1）が上がる。**物理（列2）と支払（列5・6）の変化を測り、変化がなければ「目標のみ上昇」と報告する**（B+では物理が変わらないのが期待結果） |
| T3 | 全モデルでclaimが急増 | `W_work`が上がる。モデル別のhardeningはない（ticket比 = CCU比）。全列を記録する |
| T4 | Aだけが急増 | AのFinal数（列4）が`Cap_A`まで伸びる。超えた分はqueueに入り、`capacity_short_A`と不足量が出る。他モデルの物理block（列2）の減少（`W_work`経由、E2）を測って報告する |
| T5 | Aの検証者を追加 | `Cap_A`が増え、queue長と待ち時間が減り、Final数（列4）が増える |
| T6 | 既存bondがcap | どのbondもcapを超えない。配分の余りは`stranded_by_bond_caps`として報告され、cap引き上げは起きない |
| T7 | 新規モデルの参入 | 参入後`N_30`の間の、新規モデルの権利付与（列3）と支払（列5）が下限（Q6）以上。物理block（列2）は別途報告する |

### 9.2 敵対的試験

| ID | 攻撃 | 合格基準 |
| --- | --- | --- |
| A1 | epoch境界や30日平均を狙った資本の付け替え | 静的に置いた場合より配分の合計が増えない |
| A2 | 短期借入 | `S̄`への寄与は「日数/30」以下 |
| A3 | Sybil検証者 | claimに拘束された責任元本だけを数える。分割しても`S_m`は不変 |
| A5 | 容量の占拠 | bondの取り分は資本比例。権利のあるclaimを押し出さない |
| A6 | 自己取引 | 同じ元本を二重に数えない。自己検証はgateに数えない |
| A7 | `W_work`の吊り上げ（E5） | `Pool`は増えない。bondの多いモデルの物理blockの減少を測って報告する |
| A8 | reorg / restart / IBD | 権利・carry・spill・`ψ`・queue・容量を決定的に復元する。二重支払がない |
| A9 | attestation gateで拒否 | 権利をどこからも受け取らない |
| A11 | 検査しないverifier | 責任のある元本だけを数える。残余は報告する |

### 9.3 追加試験（R5）

| ID | 試験 | 合格基準 |
| --- | --- | --- |
| N1 | entrant reserveの予算控除 | すべてのepochで`Σ_m Base_m + Σ Entrant_m ≤ Pool(e)`。reserveは固定総額から引かれ、総支払が追加されない（reserveの有無で総支払の上限が同じ） |
| N2 | モデル分割（ほぼ同一のモデルを複数登録）でcap回避・α利得・entrant reserve取得を狙う | 分割後の`P`合計は分割前以下（α ≥ 1の超加法性）。entrant reserveの総額は登録数によらず一定。bond単位のcap合計は分割しても不変。分割後の支払合計（列5）が分割前を超えない |
| N3 | 容量0での受理 | ready検証者が0（`Cap_m = 0`）のモデルでは、新claimを受理しないか、queueに入れずに`capacity_short`を明示的に返す。quoteは`Refuse`を返す |
| N4 | 検証者容量の二重計上（モデル・class間） | 各検証者の容量配分の合計が`c_v`以下。複数モデルにreadyな検証者を全モデルに満額で数えない。Hall条件の違反は容量として数えない |
| N5 | queue中の最大権利予約 | queue中のclaimは1 claim分を超えて予約しない。queueの全claimを合わせても、モデル予算とbond capを超える権利を予約しない。queue中のclaimは`Demand`に入らない |
| N6 | quote後の競合 | quoteが`Admit`を返した後、並行するblockやclaimが容量を消費した場合、余裕幅`h`の範囲内ではqueueが吸収する。`h`を超えた分の拒否は`skipped_own_attempts`として報告される。計算の浪費率を記録する |
| N7 | 無効・有罪claimによるepoch報酬の希釈 | close前に無効・有罪となったclaimは`Demand`から除かれ、正直なclaimの`ψ`が下がらない。close後に判明した分は留保され、正直なclaimの支払は変わらない。無効claimを大量に出しても正直なclaimの`ψ`は不変 |

### 9.4 不変条件

* 保存: `Σ 付与 + Σ 留保 + Σ carry残 = Σ Budget`。新規mintはない。
* `ψ ≤ 1`、全bondでQ/B/R/F ≤ cap。
* 物理層の関数の出力は、fenceの前後でbit単位で同じ。
* `σ = ∞`、`κ = 0`、`ε = 0`なら、権利シェアは物理シェアに一致する。

## 10. 実装対応表（B+を採る場合の候補、未実装）

| 対象 | 変更候補 |
| --- | --- |
| `palw_bond_budget_v1.rs` | モデルrowに権利予算・carry・spill・`ψ`を追加。epoch close。30日平均。verifier元本。entrant reserve。L2680のRだけのclipを置き換える。kernel claimのescrowをclipしない（§2.4、P4次第） |
| `palw_bond_budget_fold_v1.rs` | epoch closeのdeltaとundo。`plan_kernel_claim_budget_v1`のモデルclip（L1514–1536）の扱い（P4） |
| `palw_admission_v2.rs`、`palw_work_target_v1.rs`の`palw_work_target_step_v1` | 変更しない（§5.4のP-iを選ぶ場合を除く） |
| `palw_work_target_v1.rs` | 検証者単位の容量、C7の導出上限 |
| `fold_read.rs`、`palw_state_v2.rs` | queue、容量の置き換え、quote |
| `palw_producer_v2.rs` | quoteの項目、side blockの需要 |
| `palw_verifier_pay_v1.rs` | `N`・`m`の導出 |
| Params | 休眠fence（全presetで`None`、Some-onlyでhash、arm時は拒否）。新objectはA-2に従う（自分のfenceまでは判定せず通す） |

## 11. 有効化gateとcodeの是正

### 11.1 gate

~~~text
ACTIVATION_ALLOWED
    = §2.5 の判断（P1–P5）が出ている
      AND §5.4 の判断（Q1: 物理層を変えるか）が出ている
      AND B+ または別案がユーザーに採用されている
      AND §12 の値が決まっている
      AND §11.2 のcode是正が完了している
      AND §9 の全試験が、物理・支払の列で独立review付きPASS
      AND 実node E2EとADR-0177の二段評価の再計算
      AND 全RFCを一度に有効化する協調upgradeに入っている
~~~

### 11.2 codeは決定に従う（選択肢ではない）

| ID | 是正 |
| --- | --- |
| C1 | `W_hold`を7日相当DAAへ直す。`PALW_BOND_BUDGET_W_MIN/MAX_DAA_V1`（280–436）と、RFC08の`W_budget = 400`のdrill暫定値は、確定した7日相当に合わせる |
| C2 | `S_m`を、30日平均（`N_30`）のMiner＋claimに拘束されたVerifier元本へ直す。現行はminer-onlyのepoch snapshotとseasoning |
| C3 | 配分曲線はα = 1.5で固定する（コードの`alpha_halves = 3`。interimの表記を確定値へ直す） |

## 12. 本RFCで決めない値と判断

### 12.1 Panel=0前提（§2）

| ID | 判断 |
| --- | --- |
| P1 | kernel claimの`B`: 0のまま / 会計単位として>0 |
| P2 | kernel claimの`F`: 0のまま / >0（fork choiceの変更） |
| P3 | kernel claimへのcoinbase subsidy legを作るか。作るなら財源と保持規則 |
| P4 | 配分fenceがkernel claimの利用者払いescrowをclipしてよいか（§2.4）。作業仮説: clipしない |
| P5 | Panel=0でも、coinbaseを受け取るV2 Attempt laneをPanel経路のまま残すか |

### 12.2 物理層（§5）

| ID | 判断 |
| --- | --- |
| Q1 | 物理blockの目標と人気モデルの非劣化のどちらを優先するか。物理層の手段（P-i〜P-iv）を採るか、採らないか |

### 12.3 B+の値（採用時のみ）

| ID | 値 |
| --- | --- |
| Q2 | `Pool`の基準（R1 / R2） |
| Q3 | `σ`、spillの重み、`κ`、`K` |
| Q4 | `ε`、`S_entry`、1モデル上限 |
| Q5 | `E_alloc`（`N_30`・`W_hold`は確定済み） |
| Q6 | 受入閾値: 新規モデルの最低権利・支払、T4の劣化許容幅 |
| Q7 | 予算超過claimの受理とqueueの優先度 |
| Q8 | queueの`L_queue`、取り分、`T_queue` |
| Q9 | 容量attestationの形式、`sigma_cap`、計算量の上限 |
| Q10 | 検証者取り分の傾き`γ`、`u*` |
| Q11 | verifier payの`N`・`m`の導出、`checked_share`の目標 |
| Q12 | quoteの余裕幅`h` |
| Q13 | attestation gateで留保した予算の行き先 |
| Q14 | 支払を`max(Final, epoch close)`まで保持してよいか |

確定事項（`α = 1.5`、30日平均、`W_hold` = 7日相当）は判断対象ではない。個別capの引き上げで使い切れない配分を解消する選択肢は設けない。

## 13. 参照

* [PRINCIPLES](../PRINCIPLES.md) §0
* [ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)
* [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)
* [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)
* [RFC-0008](0008-palw-claim-backed-consensus-blocks.md) R8-BUDGET、L134
* [RFC-0015](0015-panel-free-permissionless-verification.md) §8.3–8.5.5
* [Bond budget / model allocation設計](../design/palw/bond-budget-and-model-allocation.md) §3.4–3.6
* [RFC-0015 implementation status](../design/palw/rfc-0015-implementation-status-2026-10-15.md) L15–18・§1b
* [Activation readiness matrix](../design/palw/activation-readiness-matrix.md) §3f・§3g・§3k
* コード: [`palw_admission_v2.rs`](../../consensus/core/src/palw_admission_v2.rs)、[`palw_work_target_v1.rs`](../../consensus/core/src/palw_work_target_v1.rs)、
  [`fold_read.rs`](../../consensus/core/src/palw_state_v2/fold_read.rs)、[`palw_bond_budget_v1.rs`](../../consensus/core/src/palw_bond_budget_v1.rs)、
  [`palw_bond_budget_fold_v1.rs`](../../consensus/core/src/palw_bond_budget_fold_v1.rs)、[`palw_kernel_route_fold_v1.rs`](../../consensus/core/src/palw_kernel_route_fold_v1.rs)、
  [`ledger.rs`](../../misaka/palw/kernel/src/ledger.rs)、[`palw_verifier_pay_v1.rs`](../../consensus/core/src/palw_verifier_pay_v1.rs)、
  [`palw_producer_v2.rs`](../../consensus/core/src/palw_producer_v2.rs)、[`config/params.rs`](../../consensus/core/src/config/params.rs)
