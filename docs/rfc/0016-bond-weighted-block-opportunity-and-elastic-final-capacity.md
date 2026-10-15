# RFC-0016 — bond加重のブロック生成機会と弾力的Final容量: 共通W抽選を保ったまま`P_m`をブロック権利へ接続する

> **設計前提:** [PRINCIPLES](../PRINCIPLES.md) §0（2026-10-14、モデルの非公開を経済的に不利にする。不正は排除でなく有界化）を最上位とする。
> [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)（bond単位のQ/B/R/F上限と共通拘束`d + W`）、
> [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)（モデル入手への合意不介入、モデル別配分）、
> [RFC-0015 §8.3–8.5](0015-panel-free-permissionless-verification.md)（2026-10-14確定の`α = 1.5`・30日平均・Miner＋Verifier元本・ブロック機会にも適用）を前提とする。
> 本RFCはそれらを変更しない。2026-10-14決定の「モデル間のブロック生成機会を`P_m`で配る」を、
> [ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)の共通work target抽選と矛盾なく実装する設計と、
> 人気モデルが検証容量不足でFinal待ちになる問題の設計を提案する。

**トークンの表示名は Misaka、ticker は BILI。** 既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Draft, 2026-10-15。設計提案のみ。**本RFCの機構はどれも実装されていない。** 値はすべて未承認で、§12の一覧はユーザー判断を要する。
* **Implementation status:** not implemented。§1の検証は既存コードの読み取り記録であり、build・test・実node試験は行っていない。
* **Source baseline:** `origin/pre` `4fb381a87`（2026-10-14、α = 1.5確定・ModelPosition手数料5 %・attestation gate・closure-uneconomic goal）と、
  統合branch `integ/rfc02-rfc06-12-g14` HEAD `7dd86f2a0`（RFC02＋RFC06/07/08/12＋G14公開conviction＋RFC-0015休眠実装に`4fb381a87`をmerge）。
* **Target property（ユーザー目標）:** 「bondが多く集まるモデルほど多くのブロック生成機会と報酬を獲得し、人気モデルがFinal待ちで不利にならない」。
* **Decision（提案）:** 物理blockはADR-0137の共通`W_work`抽選のまま変えない。その上に**モデル別ブロック権利台帳**を置き、
  `P_m`で各epochの報酬対象ブロック権利とcoinbaseを一度だけ配る。使われなかった権利は上限付きのcarryと、上限`σ`付きのspillで扱い、
  残りは発行しない。新規モデルにはentrant reserveを置く。検証容量は、そのモデルを実際に保有してbondした検証者に比例させる。
  抽選の前にadmission quoteを出し、上限付きの公平なpending queueで受ける。容量不足は明示的に報告する（§5–§7）。
* **Activation:** 提案fence名は`palw_model_block_rights_v1`と`palw_elastic_final_capacity_v1`。設計用の未割当名で、既存Paramsのfieldではない。
  有効化の高さ・wire tag・RPC op番号・署名domainは割り当てない。休眠（全presetで`None`、Some-onlyでhash、arm時はvalidationで拒否）で実装し、
  全RFCを一度に有効化する方針（2026-10-08ユーザー決定）と「経済が閉じるまで報酬・consensus weightの新fenceは武装しない」（2026-10-09）に従う。
* **Not activated by:** 本RFCのmerge、休眠実装のmerge、unit/E2E testのPASS、`palw_model_bond_allocation_v1`の値の承認だけ。
* **Normative language:** MUST / MUST NOT / SHOULDは提案fenceの有効化後の規則である。型名・関数名・fence名は設計用で、現行Rust型の存在を意味しない。

## 0. 日本語での決定（要約）

**レビューの指摘は大筋で正しい。現行コードでは`P_m`はブロック選択に接続されていない。**
ADR-0137の抽選は「1単位の仕事 = 1単位の当選確率」をモデル間で等しくする。このため、どのモデルのブロックが何本出るかは計算量の結果であり、bondは入力にない。
`P_m`を実装するはずの配分engine（`palw_model_bond_allocation_v1`）は休眠中で、armすると拒否される。
しかも休眠実装でも、モデル予算の不足は**報酬額だけ**を削る設計になっている。

本RFCは「ブロック生成機会」を次の二層に分けて扱う。

| 層 | 決めるもの | 規則 | 本RFCでの変更 |
| --- | --- | --- | --- |
| 物理層 | 実際にDAGへ入るモデルblockの本数とcadence、DAA、fork choice、Final weight | ADR-0137の共通`W_work`抽選 `p_m = min(1, CCU_m / W_work)` | **変更しない**（MUST NOT） |
| 権利層 | 報酬対象ブロック権利と、それに付くcoinbase | `P_m`によるepoch予算・carry・spill・entrant reserve | **新設**（`palw_model_block_rights_v1`） |

権利層の予算は報酬の期待値を決め、quoteを通じてminerの計算の移動先を誘導する。
minerが移動できる限り、物理blockの比率も均衡で`P_m`へ近づく。これは誘導であり、強制はしない。
物理層を直接`P_m`へ縛る案（per-model target、slot割当）は、ADR-0137が除いた人気ペナルティとcold start trapを戻すので採らない（§4）。

Final待ちは、Panel経路（現行t12）の容量規則を検証者単位の容量へ改める。対象モデルを保有しreadyな検証者が増えれば、そのモデルの容量が増える。
抽選の前にquoteを出し、受け入れられないclaimに計算を使わせない。満杯時は即拒否でなく、上限付きの公平なqueueで受ける。
OPV（Panel=0）経路には容量待ちはないが、検査率を請求率に比例させる。不足は明示的に報告する（§6）。

## 1. レビュー主張の検証

`pre` = `4fb381a87`、`HEAD` = `7dd86f2a0`。行番号はその時点のfileのもの。判定はCONFIRMED / PARTLY / WRONG。

### 1.1 判定表

| # | 主張 | 判定 | 根拠（pre / HEAD） | HEADでの差分 |
| --- | --- | --- | --- | --- |
| (a) | ADR-0137は単価型difficultyを共通`W`に置き換えた。Attemptの当選確率は`p_m = min(1, CCU_m / W)`で、モデル別claim数は入力にない | **CONFIRMED** | `consensus/core/src/palw_admission_v2.rs` pre L1130–1146 / HEAD L1134–1150 `palw_effective_class_target_v1`（非floor classは登録行の`economic_ccu_per_claim`と`W`だけを読み、class targetは読まない）。`palw_work_target_v1.rs` L116–125 `palw_work_ticket_target_v1`。floorはpre L1106 / HEAD L1110 `palw_work_lottery_floor_v1`（single lottery後は`max(W₀, W)`） | HEAD L1095–1103に`native_material_ticket_v3`分岐を追加。同じ`palw_effective_class_target_v1`を読むので規則は同じ。補足: claim数は**ticketの入力ではない**が、§(c)の容量admissionでは入力になる |
| (b) | `W`はepochごとに`max(W₀, W·実blocks/目標blocks)`、1 epochの変化率に上限。全モデル共通。t12 shipped presetはDAA 0から有効 | **CONFIRMED** | `palw_work_target_v1.rs` L94–114 `palw_work_target_step_v1`（`clamp(W/4, W·4)`、`PALW_WORK_TARGET_MAX_FACTOR_V1 = 4` L34、floor `W₀`、floorのclassは数えない）。`config/params.rs` pre L10377–10401 / HEAD L10499–10523 `palw_t12_shipped_params`（「the fences at DAA 0」「the work target is armed from DAA 0」）、HEAD L12303 `palw_t12_arm_every_rule_from_genesis` | 変更なし（行番号のみ移動）。RC preset（t11系）は`palw_work_target`がDAA 7,101、`palw_single_lottery`が7,780（HEAD L12022・L12038 / pre L11900・L11916） |
| (c) | 当選はFinal・報酬と別。Panel容量とinflight上限が新claimを拒否する（`PanelRoomExhausted`、`ClassInflightCapped`）。8kは窓3 span・同時約5、2Mは2,799 span・最大1。人気モデルのFinal処理量は検証資源で決まり、minerを増やしても増えない | **PARTLY** | 拒否経路は正しい: `palw_state_v2/fold_read.rs` `check_class_admits_claim` pre L901 / HEAD L901–1063。`ClassInflightCapped` pre L1002・L1046 / HEAD L1016・L1060。`PanelRoomExhausted` pre L1019・L1034 / HEAD L1033・L1048。窓と上限は`palw_work_target_v1.rs` L205–245 | 次の4点で不正確。(1) 8k行はaudit fence（t12では有効）後、C7（窓 ≥ 1,000 span）でないためlicenceで解放され、**静的上限5ではなくrate roomだけ**で判定される（L222–229）。(2) rate roomは**そのclassのready seat数に比例する**（HEAD L1209–1217・L1266 `model_registry_room_ready_v1`）。保有して準備した検証者が増えれば容量は増える。(3) ただし他の全classの未払replayを自classの供給から引くので、検証者集合が別でもclass間で容量が干渉する（`palw_state_v2.rs` HEAD L13130–13146 `PalwPanelRateV1::read`）。(4) HEADのOPV経路（休眠）には検証容量のadmissionがなく、担保だけを見る（`misaka/palw/kernel/src/ledger.rs` L2116–2129 `opv_claim_capacity`） |
| (d) | RFC-0015は30日平均の（miner＋verifier）資本で`P_m = S̄^1.5 / ΣS̄^1.5`を決め、ブロック機会とcoinbaseの両方に適用（§8.5、pre `18987a94b`）。配分engineは休眠（`palw_bond_budget_v1.rs` L1–24）。既存設計はモデル予算不足時に報酬額だけを制限し、抽選やブロック頻度は制限しない（設計書§3.5）。よってα = 1.5のブロック配分は実際のブロック選択に接続されず、W抽選だけではbondからブロック数は導けない | **CONFIRMED**（追加あり） | RFC-0015 §8.5.5 pre L626 / HEAD L627–674。`palw_bond_budget_v1.rs` L1–15（「dormant, no height」）。モデル予算はRだけを削る: pre L2634 / HEAD L2680 `reservation.reward_sompi.min(self.model_available(&m))`。arm時は拒否: pre L684 / HEAD L711–734。`docs/design/palw/bond-budget-and-model-allocation.md` §3.5 L296–308（「A model shortfall clips **reward only**」）、§3.6 L310–315（未使用分は発行せず、再配分しない） | 追加の乖離: コードの`S_m`は**miner資本だけ**（tag 140の署名済み資本割当）で、epoch snapshotとseasoningを使う（L10、L520–531）。10-14決定の**30日平均もverifier元本も未実装**。HEADは`opv_mechanics_probe_v1`（calibration用、L368–382）を追加したが、配分fenceは引き続き拒否される |
| (e) | リスク: 人気モデルの検証backlog、bond基準のブロック配分と共通W抽選の衝突（配分80 %で計算20 %、または計算が配分を超える）、個別bondのBlock/Reward上限でモデルが大きい`P_m`を使えない | **CONFIRMED**（将来リスクとして） | (c)・(d)の根拠に加え、§1.2のX4（α > 1と線形capの組合せで構造的にstrandingが起きる）とX1（OPV kernel claimは報酬blockを持たない） | — |

### 1.2 レビューが扱っていない事項

| # | 事項 | 根拠 | 本RFCへの影響 |
| --- | --- | --- | --- |
| X1 | **RFC-0015のlead default（ユーザーreview待ち）: kernel経路のclaimは`B = F = 0`を予約し、報酬blockもconsensus weightも作らない** | `docs/design/palw/rfc-0015-implementation-status-2026-10-15.md` L52、L73 | Panel=0では「ブロック生成機会」を付ける物理blockがない。権利層をADR-0176の`B`次元から独立した台帳にし、kernel claimの報酬legに同じ`ψ`を掛ける（§5.9、判断U7） |
| X2 | RFC08 R8-BUDGET: 予算window `W_budget = 400 DAA`はdrill暫定。fee-only Roundは`B_max`外の独立`ExecutionCap`。v1 market feeは`R_max`外 | `docs/rfc/0008-palw-claim-backed-consensus-blocks.md` L292–301 | fee-only Roundとmarket feeは権利層の外に置く（§5.8） |
| X3 | **記号`W`が4つの別の量を指し、値も衝突している**: ADR-0137のwork target（MAC-eq）、ADR-0176の拘束期間`d + W`（RFC-0015 §8.5.5では「7日相当DAA」、コードの許容範囲は280–436 DAA）、RFC08の`W_budget = 400 DAA`、RFC-12のwork `W = 6.33e9 pwu` | `palw_bond_budget_v1.rs` L32–37（`PALW_BOND_BUDGET_W_MIN_DAA_V1 = 280`、`MAX = 436`、150 s/DAA換算）。RFC-0015 HEAD L588・L647。`activation-readiness-matrix.md` L318–320 | 本RFCは`W_work`・`W_hold`・`W_budget`・`W_pwu`と書き分ける。7日相当は150 s/DAA換算で約4,032 DAAとなり、コードの上限436を超える。**ユーザー判断が要る**（U6） |
| X4 | ADR-0176の個別capは資本に線形（`⌊C·b/u⌋`）。一方`P_m`はα = 1.5で超線形 | `palw_bond_budget_v1.rs` L796–813 `palw_bond_budget_caps_v1`、L876–912 `palw_bond_budget_admit_v1`。ECON: 移行後に資本55 %のモデルが報酬81 %（`activation-readiness-matrix.md` §3g） | capの合計が配分とちょうど釣り合う水準では、上位モデルの配分がcapに当たって**構造的に余る**。capのheadroom条件を§5.6で規範化する |
| X5 | ADR-0177 D4: 使い切れない枠の扱いをrelease前に固定し、miner capを超える自動再配分を許さない。D5: coinbase配分の変更だけで物理block頻度・ticket・DAA・難易度・fork choiceを変えない | ADR-0177 L89–122、2026-10-14決定 L238–250 | spillは各bondのcap以下に限る。物理層は変えない（§5.4、§5.7） |
| X6 | M\*-49 verifier pay（休眠）: 検査ごとの料金を各jobのescrowから払い、49 %の中からdrawされた検証者へ先に払う。1 epochあたりclaim数`N = 1`、slot `m = 4`、料金1.36 BILIはすべて暫定定数。drawとattestation objectは未割当 | `consensus/core/src/palw_verifier_pay_v1.rs` L1–35 | 検査予算は請求率に比例せず固定。人気モデルの検査率が薄まる（§6.6） |
| X7 | 検証attestation gate（2026-10-14維持）: 承認されないmodel-epochにはモデル配分を払わない。claim・escrow・Final・weight・slashには触れない | `docs/PRINCIPLES.md` §0-1、`activation-readiness-matrix.md` §3f L248–252・§3k | 権利層の予算はモデル配分そのもの。gateが拒否したmodel-epochは、carry・spill・entrantからも受け取らない（§5.7） |
| X8 | 検証容量はreadyな検証者数に比例する（Panel経路）。F-R（`palw_capacity_verify_room`）では、class roomを使う各bondの取り分がstake比例になる。ただしF-Rは出荷presetで休眠 | `fold_read.rs` HEAD L1185–1242、`palw_verify_capacity_v1.rs` L21–30、`params.rs` HEAD L10769–10790（「testnet-12 ships every capacity fence dormant」） | 弾力化の土台は既にある。足りないのはclass間干渉の除去、C7静的上限の導出化、queue、quoteである（§6） |
| X9 | **admissionと抽選の順序**: block検証ではadmission（L1083–1084）の後に抽選（L1095–1103）を検査する。minerの計算はその前に済んでいる。producerの事前確認`palw_class_admits_claim_v1`はある（`palw_state_v2.rs` HEAD L12217、`palw_producer_v2.rs` L246–259）。ADR-0137 D5も「the producer pre-checks the room (no forward is wasted)」とする | `palw_admission_v2.rs` HEAD L1083–1103。`palw_producer_v2.rs` L253–259（「the tip does not hold this node's attempts still riding side blocks」）。予算で拒否された自attemptはskipされ、carveは留保される（`palw_bond_budget_v1.rs` L686） | 事前確認はtip時点の読み取りで、予約ではない。並行blockや他minerとの競合で、事前確認が通った当選でも拒否され得る（計算の浪費）。quoteに余裕幅を持たせ、queueで受ける（§6.4–6.5） |
| X10 | ADR-0137 D5は「an allocation is a share」として**モデル別配分を明示的に否定**した | ADR-0137 L303–325 | 2026-10-14決定はモデル間配分を再導入する。本RFCは物理層でADR-0137を保ち、配分は権利層に閉じ込める。ADR-0137 D7（shareは読み手の数字）の改定範囲は権利層に限る（§5.7） |
| X11 | `408f2171e`（10-14 14:03）のcommit messageは「Block issuance is not scaled by bond, there is no 30-day average」と書いたが、17分後の`18987a94b`が30日平均とブロック機会への適用を確定した。readiness §3gの「Block issuance … not scaled by bond aggregation」はnetwork全体の量の話で、10-14決定と両立する | `git show 408f2171e`、`18987a94b`、`activation-readiness-matrix.md` L269 | 本RFCはnetwork全体のblock頻度・DAA・難易度・fork choice・Final weightを変えず、モデル間の配分だけを扱う |

### 1.3 結論

1. 現行・統合のどちらでも、物理blockの本数は計算量だけで決まる（ADR-0137）。bondは入力ではない。
2. `P_m`は文書上は確定している。しかし実装は休眠のminer-only・epoch snapshot版で、報酬額を削るだけである。
   ブロック機会への接続、30日平均、verifier元本は未実装である。
3. 容量はPanel経路ではready seatに比例するが、class間の干渉、C7の静的上限1、事前確認の競合という3つの穴がある。
   OPV経路には容量待ちはないが、検査予算が固定である。
4. したがって目標性質は**現状では成立していない**。成立には§5（権利層）と§6（容量）の両方が要る。

## 2. 用語と記号

| 記号 | 意味 |
| --- | --- |
| `W_work` | ADR-0137のwork target（MAC-eq）。`palw_work_target_step_v1`で歩む共通値 |
| `W_hold` | ADR-0176の共通拘束期間。発行枠は`d + W_hold`より早く戻らない |
| `W_budget` | RFC08 R8-BUDGETの予算window（drill暫定400 DAA） |
| `N_30` | 30日相当のDAA数（RFC-0015 §8.5.5） |
| `S_m(k)`, `S̄_m(d)`, `P_m(d)` | RFC-0015 §8.5.5の定義どおり。`P_m = S̄_m^1.5 / Σ_j S̄_j^1.5`、分母0なら0 |
| `e`, `E_alloc` | 配分epochとその長さ（DAA） |
| `U` | ブロック権利1本の固定小数単位（既存`PALW_BUDGET_BLOCK_UNIT_V1 = 1,000,000`と同じ単位を推奨） |
| `Pool(e)` | epoch `e`で配る権利の総量（§5.2） |
| `Base_m(e)` | `⌊Pool(e) · P_m(e)⌋`。モデルの基本予算 |
| `Demand_m(e)` | epoch `e`に受理され、close時点で無効化されていないモデル`m`のclaim数 × `U` |
| `ψ_m(e)` | epoch `e`のモデル`m`のclaimが実際に消費する権利の割合（0–1） |
| `σ` | spillで受け取れる上限。`Base_m`に対する比 |
| `κ`, `K` | carryの上限（`Base_m`に対する比）と保持epoch数 |
| `ε` | entrant reserve（`Pool`に対する比） |
| 物理block | 抽選に当選してDAGに入ったモデルblock |
| ブロック権利 | 報酬対象として数えるブロック1本分の権利。coinbaseはこれに付く |

## 3. 目標性質の分解

| ID | 性質 | 形式 |
| --- | --- | --- |
| G1 | bondの単調性 | 他モデルの`S̄`が固定なら、`S̄_m`を増やすと`BlockShareTarget_m`と`CoinbaseShareTarget_m`は減らない（`S̄_m > 0`なら厳密に増える）。両方とも`P_m`に等しい |
| G2 | 計算単位の公平性（ADR-0137） | 物理層では1 forwardの当選確率が`min(1, CCU_m / W_work)`で、モデルによらない。モデル別のtargetを置かない |
| G3 | 実現 | minerが十分なら、epochのブロック権利シェアは`P_m`へ収束する（carryの許容範囲内）。計算不足なら不足分を明示的に報告する |
| G4 | 人気モデルのFinal非劣化 | モデルのFinal処理量は、そのモデルを保有してbondした検証者の容量に比例して伸びる。不足時は理由と量を明示的に報告する。抽選後の容量拒否で計算を浪費させない |
| G5 | 総量不変 | network全体のcadence・DAA・難易度・beacon・fork choice・Final weight・総発行予算を変えない。未使用分は発行しない |
| G6 | 個別capの維持 | 各bondのQ/B/R/F capと`d + W_hold`を超えない。過去claimへ遡及しない |
| G7 | 新規モデル | α = 1.5と30日平均の下でも、適格な新規モデルが機会を失いすぎない（最低限の機会を持つ） |
| G8 | 操作耐性 | 資本の付け替え、短期借入、Sybil検証者、モデル分割、容量の占拠、自己取引で、同額資本の配分を増やせない |

## 4. 代替案

### 4.1 案A: `P_m`で駆動するモデル別sub-target

各モデルが自分の`W_m`を持ち、epochごとに`W_m ← W_m · (realized_m / (P_m · expected))`で歩む。

* G1は物理層で直接成立する。
* **G2を壊す。** bondに対して計算が多いモデルは`W_m`が上がり、1 forwardあたりの当選確率が下がる。ADR-0137 §3.4が除いた「人気・重さに罰を与え、それを恒久化する」cap trapと同じ形である。
* **G7を壊す。** `S̄ = 0`の新規モデルは`P = 0`になり、targetが無限大に近づく。ADR-0137 §3.1のcold start trapが戻る。
* 30日平均のlagと×4/÷4のclampが合わさり、ADR-0137 §3.2のsawtoothと同型の振動を生む。
* bondの多いモデルに計算がなければ、そのcadenceは空く（floorが埋める）。liveness上は問題ないが、報酬は失われる。
* class DAA状態の復活が要り、fork choiceに触れるupgradeになる。

**不採用。**

### 4.2 案B: 共通W抽選＋モデル別ブロック権利予算

物理層はADR-0137のまま。権利層で、各epochの報酬対象ブロック権利を`P_m`で配る。
予算を超えた当選も物理blockとして有効で、権利はcarry・spill・0の順に決まる。

* G2を保つ（物理層は無変更）。G5を保つ（物理量は無変更、権利は発行しない）。
* G1は権利層で成立する。
* G3は経済的な誘導で達成する。予算を超えたモデルでは1当選あたりの期待報酬が下がるので、minerはbondの多いモデルへ移るか、そのモデルへbondを足す。
  これは「bond集積 = 利用者集積」（2026-10-10ユーザー判断）と同じ方向である。
* spillを計算比例で全部配ると、権利シェアは計算シェアと一致し、`P_m`は効かなくなる（§5.4で証明）。spillには上限`σ`が要る。

### 4.3 案C: beaconでslotをモデルへ割り当て、slot内で共通W

* 割り当てられたモデルにminerがいなければslotが空く。minerはモデルごとに特化していて（数GBの重み、2M context）、slotごとに切り替えられない。
* beaconのgrinding・bootstrap問題（RFC-0007 Part VI、OPVB）を、block validityの中心に持ち込む。
* slot割当がblock validityとfork choiceに入る。
* bondが少なく計算が多いモデルは大半のslotで待機するので、人気ペナルティが最大になる。

**不採用。**

### 4.4 案D: Bに上限付きの抽選tiltを加えるhybrid

案Bに加え、ticketを`T_m = MAX · min(1, CCU_m · τ_m / W_work)`、`τ_m ∈ [1/k, k]`とする。
`τ_m`は権利層の`ψ_m`から決める。物理blockを直接`P_m`へ寄せられるが、計算単位の公平性（G2）を最大`k²`倍まで崩し、案Aの罰を有界に戻す。

**既定では採らない**（`k = 1`）。minerが移動できず、Bの誘導だけでは物理シェアが長く乖離する場合の選択肢として、判断U17に残す。

### 4.5 比較

| 性質 | A | B（推奨） | C | D（k > 1） |
| --- | --- | --- | --- | --- |
| G1 bond単調（targetとcoinbase） | 物理で成立 | 権利で成立 | 物理で成立 | 両方で成立 |
| G2 計算単位の公平性 | 破る | **保つ** | slot内のみ | 有界に破る |
| G5 cadence・fork choice不変 | 破る（class DAA） | **保つ** | 破る | ticketに触れる |
| G7 新規モデル | cold start trap | entrant reserve | slotなしで停止 | reserve＋tilt |
| 人気（計算）ペナルティ | 恒久化し得る | 報酬側で有界（`σ`） | 最大 | 有界 |
| 実装の侵襲 | 合意の中核 | 権利台帳と報酬経路 | 合意の中核 | 中核＋台帳 |

## 5. 推奨設計: 共通W抽選＋モデル別ブロック権利台帳（案B+）

### 5.1 二層の規範

1. 物理層: `palw_effective_class_target_v1`、`palw_work_target_step_v1`、cadence、DAA、fork choice、Final weightは本fenceで変えない（MUST NOT）。
   予算を超えた当選も、既存規則で有効な物理blockである。
2. 権利層: モデル`m`のclaimが受け取る報酬は`ψ_m(e) ×（そのclaim自身のcarve）`である。
   1 claimあたりの報酬（ADR-0137 D4、Upgrade Cの`min(escrow, attempted · rate)`）はモデルによらず等しいので、
   **ブロック権利シェアとcoinbaseシェアは同じ`P_m`から一度だけ決まる**（RFC-0015 §8.5.5「同じ比率を一度だけ使う」）。
   1 block当たりの報酬に`P_m`を二度掛けしてはならない（MUST NOT）。
3. 権利台帳はADR-0176のbond単位`B`次元とは別のモデル単位台帳である。`B`の予約・消費・`d + W_hold`は既存どおりで、`ψ ≤ 1`なので個別capを超えない（G6）。

### 5.2 予算の基準量（Pool）

`Pool(e)`は次の2案から選ぶ（判断U1）。推奨はR1。

* **R1（推奨）: 期待cadence基準。** `Pool(e) = expected_model_blocks(e) × U`。`expected_model_blocks`は`W_work`が使う値と同じで、
  attempt laneのcadence slice（`closed_expected_blocks`、DAA数でclockではない）である。epoch開始時に決まり、quoteを決定的に出せる。
  * `W_work = W₀`の領域（計算供給が少なく、モデルblockがcadenceを埋めない。ADR-0137 §8）では予算が需要を上回り、ほぼ全モデルが`ψ = 1`になる。
    希少性がないときにcadenceを捨てず、どのモデルも飢えさせない。
  * 計算が飽和して`W_work > W₀`になると予算が拘束し、`P_m`が効く。
  * 無報酬blockを積んでも`Pool`は増えない（grief耐性）。
* **R2: 前epoch実現量基準。** `Pool(e) = realized_model_blocks(e−1) × U`。`P_m`が常に拘束するが、希少性がないときにも報酬を留保する。

どちらの場合も、支払はclaim自身のcarveの`ψ`倍以下であり、network予算（claimごとのescrow）を超えない。新しいmintはない（G5、ADR-0177 D4）。

### 5.3 epoch予算とclose時の比例消費

epoch `e`の開始時（境界blockのfold）に、各モデルについて次を固定する（MUST）。`P_m(e)`はepochの開始時点の値で、epoch中は動かさない。

~~~text
Base_m(e)    = floor(Pool(e) * A_m / sum_j A_j)     A_m = floor(S̄_m^1.5)（既存palw_model_available_v1と同じ512-bit整数算術）
Entrant_m(e) = §5.5
Budget_m(e)  = Base_m(e) + Carry_m(e) + Entrant_m(e)
~~~

claimの受理時は、従来どおりclaimの報酬・blockの最大値を予約する（ADR-0176 §2.5、RFC-0015 §8.3.2）。
**権利の消費はepoch closeでまとめて決める**（MUST）。

~~~text
Demand_m(e) = U * (epoch e に受理され、close時点で無効化されていない m の claim 数)
Used_m      = min(Budget_m, Demand_m)
Unused_m    = Budget_m - Used_m
Excess_m    = Demand_m - Used_m
SpillIn_m   = §5.4
psi_m(e)    = (Used_m + SpillIn_m) / Demand_m      （Demand_m = 0 なら未定義で、支払もない）
~~~

* epoch内の先着順を使わない（MUST NOT）。同じepochの同じモデルのclaimは同じ`ψ`を受け取る。epoch開始直後の殺到を生まない。
* claimの支払は`max(Final, epoch close)`まで保持する。Finalがcloseより早い場合も、`ψ`が決まるまで払わない。
* close後に有罪・voidとなったclaimの`ψ · U`は再配分しない（遡及付与の禁止）。留保する。
* `ψ`は予約済みの最大値以下の消費であり、「reserve ≥ consume、残りはmintしない」という既存の消費規則（`palw_bond_budget_v1.rs` L8）と合う。

### 5.4 未使用予算: carry、上限付きspill、留保

**計算比例のspillを全部配ると`P_m`は無効になる。** `Σ_m Budget_m = Σ_m Demand_m`（飽和時）なら、`Σ Unused = Σ Excess`である。
未使用分を超過モデルへ上限なしで全部渡すと、各モデルは`Demand_m`を全額受け取り、権利シェアは計算シェアに一致する。
したがってspillには上限が必要であり、`σ`はADR-0137（計算比例、`σ = ∞`）と厳密な`P_m`（`σ = 0`）の間を補間する1つの調整値になる。

1. **carry:** `Carry_m(e+1) = min(Unused_m(e), κ · Base_m(e))`。同じモデルだけが`K` epoch以内に使える。期限を過ぎた分はその時点のspill poolへ移る。
   目的は抽選の分散（小さいモデルほど大きい）の吸収である。
2. **spill:** `Spill(e) = Σ_m (Unused_m − carry_m)＋期限切れcarry`。`Excess_m > 0`のモデルへ配る。
   * 受け手の重みは`A_m`（同じα曲線）とする。bondの多い超過モデルほど多く受け取る。
   * 各受け手の上限は`min(Excess_m, σ · Base_m + SpillFloor_m)`（判断U3。`SpillFloor_m`は既定0）。
   * model id順の決定的なwater-fillingで配る（MUST）。
3. **留保:** 残りは既存の「withheld, never minted」経路へ送る（ADR-0177 D4、設計書§3.6）。

例（数値は**説明用で採用値ではない**）: 同じCCU、同じ`S̄`の2モデル（`P_A = P_B = 0.5`）で、Aの計算がBの3倍（0.75 : 0.25）、R1で飽和、`κ = 0`、`σ = 0.25`とする。
Bの未使用0.25 PoolのうちAは`σ · 0.5 = 0.125`を受け取る。権利はA 0.625、B 0.25、留保0.125となり、`ψ_A = 0.83`、`ψ_B = 1`。
Aの物理シェア（0.5 → 0.75）と権利シェアはどちらも上がる。一方、Aの1当選あたりの報酬はBより低いので、計算はBへ、bondはAへ移る誘因が生じる。
`σ = 0.5`なら`ψ_A = 1`となり、ADR-0137の結果と一致する。

### 5.5 新規モデル（G7）

α = 1.5は小さいモデルを強く削る。資本シェア10 %のモデルの`P`は3.57 %（線形なら10 %）、1 %なら0.10 %である。さらに30日平均が立ち上がりを遅らせる。

1. **30日平均の欠損日は0として数える（MUST）。** 登録からの経過日数で割る近道は短期借入に弱いので採らない。
   全モデルに共通する0日は比率を変えないので、既存モデル間の`P`は歪まない。
2. **entrant reserve:** 各epochで`ε · Pool`をα配分の前に取り分ける。適格なentrantの間で**線形**（`α = 1`）に、現在のseasoned locked `S_m`比で配る。
   1モデルあたりの上限を置く。適格条件は、登録から`N_30`未満、onboarding/G14の報酬gate（`palw_reward_gate_v1`）を通過、
   attestation gateで拒否されていない、`S_m ≥ S_entry`の4つ（MUST）。使われなかった`ε`はspill poolへ入る。
3. 線形配分はモデル分割に中立である（分割しても合計は増えない）。`ε`の総量は登録数によらず固定である（G8）。
4. ADR-0137 §10の「新規モデルは最初のforwardから計算シェアで当選する」は物理層でそのまま成り立つ。新規モデルが失うのは権利層の取り分だけで、その下限がreserveである。

### 5.6 個別bond capとモデル配分（G6、stranding）

1. claimの権利は、そのclaimを出したbondの予約（ADR-0176）の範囲内でしか消費しない。`ψ ≤ 1`なので、モデル配分が個別capを増やすことはない（MUST NOT）。
2. モデルのbondが全員capに達してclaimを出せない場合、そのモデルの予算は`Unused`となり、§5.4のcarry・spill・留保へ流れる。capを超えて同じbondへ回してはならない（MUST NOT、ADR-0177 D4）。
   このとき`stranded_by_bond_caps_m`として報告する（§7）。
3. **capのheadroom条件（SHOULD、policyの検証項目）:** α > 1では上位モデルの`P_m`が資本シェアを上回る。
   一方capは資本に線形である。モデルのminer capの合計が`W_hold`期間の`Base_m`を下回ると、配分は構造的に余る。policyは次を満たすように`block_units_per_unit`を選ぶ。

   ~~~text
   b_per_unit * S_m_miner / u  >=  P_m * Pool_per_W_hold        （想定する資本分布の全モデルについて）
   必要な倍率 ≈ max_m P_m / (S_m_miner / Σ S_miner)            （ECONの1.4〜1.8倍が目安）
   ~~~

4. verifier元本は`S̄_m`に入るが、minerのcapは増やさない（RFC-0015 §8.5.5）。verifier元本の比率が高いモデルはstrandingしやすい。これは仕様どおりで、報告対象である。

### 5.7 既存決定との整合

| 決定 | 整合のさせ方 |
| --- | --- |
| ADR-0137 D1–D5 | 物理層は無変更。D7「shareは読み手の数字」は**権利層についてだけ**、2026-10-14ユーザー決定により改める。D5の「no per-class allocation」は検証容量の規則として残り、本RFC §6の容量はモデル別配分ではなく検証者単位の会計である |
| ADR-0176 | bond単位のQ/B/R/F、予約、`d + W_hold`、残存責任はそのまま。権利台帳はモデル単位の追加制約である |
| ADR-0177 D1 | 入手性を判定しない。権利・容量・quoteのどれもモデル配布の有無を読まない（MUST NOT） |
| ADR-0177 D4・D5 | 未使用分は発行しない。spillは個別cap以下。物理block頻度・ticket・DAA・難易度・fork choiceは変えない |
| RFC-0015 §8.3・8.4 | 物理blockを`1/m`に分けず、権利は固定小数の台帳で扱う。端数切上げで総量を増やさない |
| RFC-0015 §8.5.5 | `P_m`をブロック権利とcoinbaseの両方へ一度だけ適用する。登録者legは置かない |
| PRINCIPLES §0（closure-uneconomic） | bondは共有されたモデルに集まり、`P_m`と権利が増える。attestation gateで拒否されたmodel-epochは、`Base`・carry・spill・entrantのどれからも権利を受け取らない（MUST）。その予算は留保する（spillへは回さない。判断U16）。claim・escrow・Final・weight・slashには触れない |

### 5.8 権利層の対象外

fee-only Round（R8-BUDGETの独立`ExecutionCap`）、v1 market fee（`R_max`外）、取引fee、ModelPosition売買手数料（ADR-0114）は権利層の外に置く。
floor class（ADR-0137 D3、無報酬のliveness）も対象外である。

### 5.9 Panel=0（kernel経路）への適用

lead defaultではkernel claimは`B = F = 0`を予約する（X1）。権利台帳はADR-0176の`B`から独立しているので、
kernel claimの報酬leg（settlement predicateで`settled`になる額）に同じ`ψ_m(e)`を掛けて適用する（判断U7、推奨）。`F = 0`は変えない。
EXEC slice経由のV2 REAL workも、slice先のkernel claimのモデルで数える。

## 6. 弾力的Final容量

### 6.1 現状（§1のまとめ）

* Panel経路: class容量 = readyなseat × seatのreplay速度 − 他の全classの未払replay。C7（2M行）は静的上限`max_inflight_claims = 1`。F-Rでは測定速度とρ clampを使う（休眠）。
* 抽選後の拒否: producerの事前確認はあるが予約ではない。並行blockで競合する。
* OPV経路: 容量待ちはなく、challenge windowが経過すればFinalになる。検査の厚さはverifier payの固定定数（`N = 1`、`m = 4`）で決まる。

### 6.2 検証者単位の容量会計（Panel経路）

* 各ready検証者`v`は、1 spanあたりのreplay予算`c_v`を持つ。基準は既存の`reference_work_per_span × utilization`、F-R後は測定値である。
  `v`は、possession proofでreadyなモデルの集合`M_v`を持つ。
* モデル`m`の容量は`Cap_m = Σ_v c_v · x_{v,m}`とする。`x_{v,m}`は`v`の予算を`M_v`の需要（未払replay）へ比例配分した重みで、
  model id順の決定的なwater-fillingで求める（MUST）。`|M_v|`には既存の`max_models_per_bond`型の上限を置き、計算量を有界にする。
* 検証者集合が交わらないモデル同士は互いの容量を減らさない。現行の「他classの全term」の差し引きを置き換える。
* 安全側の条件として、任意の検証者部分集合について`Σ_m load_m ≤ Σ_v c_v`（Hall条件）を満たす配分だけを容量として数える（MUST）。

### 6.3 C7静的上限の導出化

`max_inflight_claims`の固定値（2M行の1）を、測定容量からの導出値に置き換える。

~~~text
cap_m = max(1, floor(sigma_cap * Cap_m * window_m / claim_replay_m))
~~~

`sigma_cap ≤ 1`は安全係数（判断U11）。Final前のhold（ADR-0152 T-2(b)）はそのまま残す。ρ clamp（段階的安全計画）も残す。

### 6.4 抽選前のadmission quote

producerが計算を始める前に、合意状態から決定的に導いたquoteを読めるようにする。quoteは読み取り専用で、objectではない。

~~~text
Quote(m, bond, d) = {
  admit:        Admit | Queue(position_estimate) | Refuse(reason),
  psi_estimate: Budget_m / max(Demand_m_so_far_extrapolated, 1) の見積り,
  bond_headroom: Q/B/R/F の残り,
  capacity:     Cap_m, owed_m, queue_len_m, margin_h,
  valid_until_daa
}
~~~

* `Admit`は、queueの空きが余裕幅`h`以上あるときだけ返す（MUST）。`h`はDAGの並行度（並行blockで同時に当選し得る数）から決める（判断U14）。
* 既存の`class_admission_refusal`と`bond_class_share`（`palw_producer_v2.rs` L246–259）を拡張して載せる。
* 自nodeのside blockに乗っているattemptも自分の需要として数える（`palw_producer_v2.rs` L253–259の既知の穴）。

### 6.5 上限付きの公平なpending queue

満杯のclassで、quoteの後に競合で当選したclaimを即拒否する代わりに、上限付きのqueueで受ける。

1. **受理:** queueはclassごとに長さ`L_queue_m`まで。bondごとの取り分は、そのclassのqueueを使うbondの資本に比例する（F-Rのstake比例と同じ形）。上限を超えたら`QueueFull`で拒否する。
2. **期限保存（MUST）:** 検証窓・deadline・challenge windowは、queueを出た時点から数える。queueにいる間はFinalにならず、報酬・Final weightもない。
   Finalは常に検査の後である。DA保持義務は受理時から始まる。
3. **担保と時計:** 受理時にQ・責任担保・escrowを予約する。`d + W_hold`は受理DAAから数える（早期回復なし）。
   責任期間は`queue滞在 + window`だけ延びるが、`T_queue`で有界である。
4. **期限切れ:** `T_queue`以内に出られなかったclaimは、slashなしでvoidにする。Qの消費は戻さない（spam抑止）。
5. **順序:** 受理DAA順（同じなら決定的なtie-break）で、bondごとのround-robinとする。権利の有無で順序を変えない（判断U8で別案あり）。
6. **予算を超えたclaim:** `ψ`の見積りが0でも、容量に空きがあれば受理する（既定）。権利のあるclaimを押し出してはならない（MUST NOT）。

### 6.6 OPV（Panel=0）経路の検査予算

* OPVには容量待ちがないので、「Final待ち」の不利はない。不利は検査の厚さ（`checked_share_m`）に移る。
* verifier payの`N`（epochあたりclaim数）と`m`（slot数）を固定定数から、モデルの請求率に比例する導出値へ改める（判断U13）。
  検査料は各jobのescrowから払うので、請求が増えれば財源も増える。総発行は増えない。
* `checked_share_m`を報告し、目標を下回ったら`coverage_short`を出す（§7）。

### 6.7 検証者を人気モデルへ呼び込む報酬

* verifier元本は`S̄_m`に入るので（RFC-0015 §8.5.5）、人気モデルを検証すること自体がそのモデルの`P_m`を押し上げる。
* Panel経路の検証者取り分（ADR-0132 Cの`clamp(…, 10 %, 30 %)`）の範囲内で、利用率の高いモデルほど検証者側へ寄せる。
  `share_m = clamp(base + γ · (util_m − u*), 10 %, 30 %)`（判断U12）。総額は変えず、miner側との配分だけを変える。

### 6.8 不足の明示的報告

`capacity_short_m`は、queue占有率が閾値を超えたとき、quoteが`Refuse`/`QueueFull`を返したとき、`checked_share_m`が目標未満のときに立てる。
不足量（必要な追加検証者の容量）と一緒にRPCで返す（MUST）。不足を黙ってFinal遅延にしてはならない（MUST NOT）。

## 7. Telemetry / RPC

モデル・epochごとの4段funnelを、合意状態または正規のblock履歴から導く。

| 段 | 指標 | 出所 |
| --- | --- | --- |
| 1 当選 | `attempt_wins_m`（抽選を通ったモデルblock数。step 4でskipされたown attemptを含む）、`skipped_own_attempts_m` | block履歴（reader index）。off-chainのforward数は観測できないので数えない |
| 2 受理 | `claims_admitted_m`、`claims_queued_m`、`refused_capacity_m{PanelRoomExhausted, ClassInflightCapped, QueueFull}`、`refused_budget_m{bond Q/B/R/F}`、`queue_wait_daa_m{p50, p95}` | fold（合意）＋reader |
| 3 Final | `finals_m`、`voided_queue_expiry_m`、`convicted_m`、`checked_share_m`（OPV） | 合意状態 |
| 4 支払 | `rights_granted_units_m`、`psi_m`、`coinbase_paid_sompi_m`、`spill_in_m`、`spill_out_m`、`carry_m`、`withheld_m`、`stranded_by_bond_caps_m`、`entrant_grant_m` | 権利台帳（合意、journaled delta） |
| 目標 | `S̄_m`、`P_m`、`BlockShareTarget_m = CoinbaseShareTarget_m = P_m`、実現した物理シェアと権利シェア | 合意＋reader |
| 容量 | `Cap_m`、ready検証者数、`owed_m`、`capacity_short_m`と不足量 | 合意 |

* 予算・carry・spill・`ψ`・queueは合意値なので、権利台帳のmodel row（既存`PalwModelBudgetRowV1`の拡張）に載せる。その他はreaderで導く。
* RPC opは未割当である。設計済みで未実装のops 250–253（設計書§8.4）と既存のop 185/186の拡張を候補とする。

## 8. 受入試験

### 8.1 ユーザー指定の試験

| ID | ケース | 必須結果 |
| --- | --- | --- |
| T1 | 同じCCUの2モデル。AのminerとCPUがBの3倍、bondは同じ | Aの実際のブロックシェア（物理）が上がる（ADR-0137）。権利シェアも`P_A`から上がり、上がり幅は`σ`で決まる。`ψ_A`と`ψ_B`を報告する |
| T2 | Aのbondだけを増やす | `BlockShareTarget_A`と`CoinbaseShareTarget_A`がともに上がり、両者は常に等しい（`P_A`）。物理層のticketは変わらない |
| T3 | 全モデルでclaimが同時に急増 | 共通`W_work`が上がる。モデル別のtarget・hardeningはない（ticket比 = CCU比が不変）。`P_m`と予算は変わらない |
| T4 | Aだけが急増 | AのFinal率は`Cap_A`まで比例して伸びる。超えた分はqueueに入り、`capacity_short_A`と不足量が報告される。抽選後の容量拒否で計算を浪費しない（`Admit` quoteの後の拒否は余裕幅`h`の範囲内だけ）。他モデルの容量は、検証者が交わらない限り減らない |
| T5 | Aの検証者を追加 | `Cap_A`が増え、backlog（`owed_A`とqueue長）が減り、Final数が増える。2M行の上限が導出値に従って1から増える |
| T6 | 既存bondがcap | どのbondのQ/B/R/Fも超えない。モデル予算の余りはcarry・spill・留保へ流れ、`stranded_by_bond_caps_A`が報告される。capのheadroom条件（§5.6）の違反をpolicy検証が拒否する |
| T7 | 新規モデルの参入 | α = 1.5でも、entrant reserveと物理層の当選によって最低限の機会が残る。参入後`N_30`の間の権利シェアが下限（判断U18）を下回らない。既存モデル間の`P`の比は変わらない |

### 8.2 敵対的試験

| ID | 攻撃 | 必須結果 |
| --- | --- | --- |
| A1 | epoch境界をまたぐ資本の付け替え、30日平均への往復操作 | 静的に置いた場合より`Σ`配分が増えない。減少はすぐ反映、増加はseasoning後に平均へ入る |
| A2 | 短期借入で数日だけ`S_m`を積む | `S̄`への寄与は「日数/30」以下。entrant reserveは借入日の`S_m`でなくseasonedな値を読む |
| A3 | Sybil検証者（元本の分割、待機bond） | claimに拘束されていない元本は数えない。分割しても`S_m`は変わらない。容量はpossession proofとreadinessを持つ検証者だけを数える |
| A4 | モデルの分割（同じ資本を複数モデルへ） | α ≥ 1の超加法性で`P`の合計が減る。entrant reserveは線形で、総量が固定 |
| A5 | 容量の占拠（安いclaimで人気モデルのqueueを埋める） | bondのqueue取り分は資本比例。1件ごとに抽選の`W_work`計算、Q、責任担保が要る。権利のあるclaimを押し出さない |
| A6 | 自己取引（producer = verifier、自分のclaimを自分で検査） | 同じ元本をminerとverifierで二重に数えない。自己検証はattestation gateと独立性規則（`palw_admission_independence`）で数えない |
| A7 | 無報酬blockで`W_work`を吊り上げる | 1 blockごとに`W_work`の計算費用がかかる。R1の`Pool`は期待cadence基準なので予算を水増しできない |
| A8 | reorg / restart / IBD | 予算・carry・spill・`ψ`・queue・容量配分を正規状態から決定的に復元する。epoch closeをundo/redoしても二重支払・時計の初期化がない |
| A9 | attestation gateで拒否されたmodel-epoch | `Base`・carry・spill・entrantのいずれからも権利を受け取らない。claim・Final・weight・slashは無変更 |
| A10 | quote後の競合（並行blockで最後の容量を取り合う） | 余裕幅`h`の範囲内ではqueueが吸収する。浪費は`h`を超えた並行度の分に限られ、`skipped_own_attempts`として報告される |
| A11 | 検査しないverifier（bondだけ置く） | 権利・容量・`S̄`にはclaimに拘束された責任のある元本だけを数える。lazy verifierの残余はRFC-0015 §8.5.5の未決事項として報告する |
| A12 | entrant reserveの刈り取り（多数の新規モデル登録） | reserveの総量は固定で、線形・上限付き。報酬gateと`S_entry`を満たさない登録は受け取れない |

### 8.3 不変条件（property test）

* `Σ_m (Used_m + SpillIn_m) + Σ withheld + Σ carry_out = Σ_m Budget_m`（保存）。新しいmintはない。
* すべてのclaimで`ψ ≤ 1`。すべてのbondでQ/B/R/F ≤ cap。
* 物理層の関数（`palw_effective_class_target_v1`、`palw_work_target_step_v1`）の出力は、fenceの前後でbit単位で同じ。
* `σ = ∞`、`κ = 0`、`ε = 0`のとき権利シェアは計算シェアに一致する（ADR-0137）。`σ = 0`のとき、権利は`Base_m`を超えない。

## 9. 実装対応表（提案、未実装）

| 対象 | 変更 |
| --- | --- |
| `consensus/core/src/palw_bond_budget_v1.rs` | `PalwModelBudgetRowV1`に権利予算・需要・carry（期限付き）・spill・`ψ`を追加。`palw_model_available_v1`と同じ整数算術で`Base_m`を計算する`palw_model_rights_base_v1`。epoch close `close_model_rights_epoch_v1`（water-filling spill、留保）。30日平均（DAA ring、欠損日0）。verifier元本の算入（claimに拘束された責任元本、資金単位で重複排除）。entrant reserve。`preview_claim_v1` L2680のRだけのclipを、close時の`ψ`消費へ移す |
| `palw_state_v2`の`palw_bond_budget_fold_v1` | epoch境界でのclose、delta（新番号は未割当）、undo |
| `consensus/core/src/palw_admission_v2.rs` | **変更なし**（案Dを選ぶ場合だけticketのtilt） |
| `consensus/core/src/palw_work_target_v1.rs` | stepは変更なし。`palw_seat_capacity_waterfill_v1`（§6.2）と`palw_derived_inflight_cap_v1`（§6.3）を追加 |
| `consensus/core/src/palw_state_v2/fold_read.rs` | fence後は`check_class_admits_claim`で拒否の代わりにqueueへ入れる。`panel_rate_v1`を検証者単位の容量へ。C7の静的上限を導出値へ |
| `consensus/core/src/palw_state_v2.rs` | `palw_class_admits_claim_v1`の隣に読み取り専用の`palw_admission_quote_v1`。`PalwPanelRateV1`の置き換え |
| `consensus/core/src/palw_producer_v2.rs` | producer factsへquote（`psi_estimate`、queue、余裕幅）。side blockのattemptを需要に数える |
| `consensus/core/src/palw_kernel_route_fold_v1.rs`、`misaka/palw/kernel/src/ledger.rs` | kernel claimの報酬legへ`ψ`（判断U7）。OPVの`checked_share_m` |
| `consensus/core/src/palw_verifier_pay_v1.rs` | `N`と`m`を請求率から導出（判断U13） |
| RPC / CLI / web | §7の指標（op番号は未割当） |
| `config/params.rs`ほか | fence `palw_model_block_rights_v1`（`palw_bond_budget_v1`と`palw_model_bond_allocation_v1`がその高さ以下で有効であることを要求）と`palw_elastic_final_capacity_v1`。どちらも全presetで`None`、Some-onlyでhash、`Some(never())`は畳み、arm時はvalidationで拒否 |
| 新object（検証者の容量attestationなど） | A-2: 自分のfenceまでは「decodeできないものと同じ扱い（判定せず通す）」とする。fence前に段階のどこかで拒否しない |

### 9.1 移行

* fence前に受理されたclaimは旧規則で完了する。権利台帳は空のcarryで始める。
* 30日平均は配分fenceの有効化からの履歴で計算し、欠損日は0とする（全モデル共通なので比率は歪まない）。
* 旧`palw_model_bond_allocation_v1`の「Rだけ」のclipは、同じ高さで本RFCのclose時消費に置き換える。二重にclipしてはならない（MUST NOT）。

## 10. 有効化gate

~~~text
ACTIVATION_ALLOWED(palw_model_block_rights_v1, palw_elastic_final_capacity_v1)
    = palw_bond_budget_v1 と palw_model_bond_allocation_v1 の policy がユーザー承認済みで有効（同じ高さ以下）
      AND §12 の値（U1–U19）がユーザー決定済み
      AND W記号の衝突（X3）が解消済み
      AND §8.1 T1–T7、§8.2 A1–A12、§8.3 の不変条件が独立review付きでPASS
      AND 実node E2E（Panel経路とOPV経路、reorg/restart/IBDを含む）PASS
      AND ADR-0177 の二段評価（モデル全体と所有者個人）を本設計で再計算
      AND 全RFCを一度に有効化する協調upgradeのscheduleに入っている
~~~

UNKNOWNはPASSにしない。本RFCのmerge、休眠実装のmerge、unit testのPASSだけでは有効化しない。

## 11. 未解決のリスク

* minerが特定モデルに固定されて動けない場合、Bの誘導は遅く、物理シェアは長く`P_m`から乖離する。そのモデルの`ψ`は低いまま残る。案Dの選択肢はこのためにある。
* `Pool`をR1にすると、計算が飽和していない領域では`P_m`がほとんど効かない。これは意図した動作だが、10-14決定の「`E[block share] = P_m`」を文字どおりには満たさない。判断U1で確認する。
* 検証者単位のwater-fillingはnode資源（検証者数 × モデル数）を使う。上限と計測が要る。
* lazy verifier（bondだけ置いて検査しない）は、RFC-0015 §8.5.5の未決事項のままである。
* `p = 0`（誰もモデルを持たない）は、PRINCIPLES §0-3のとおり残余リスクで、bond capで有界化する。本RFCはこれを解決しない。

## 12. 本RFCで決めない値（ユーザー判断）

| ID | 値・判断 | 推奨・選択肢 |
| --- | --- | --- |
| U1 | `Pool`の基準 | R1（期待cadence、推奨）/ R2（前epochの実現量） |
| U2 | carry `κ`と保持epoch `K` | 抽選分散の吸収に足りる最小値。具体値は実測後 |
| U3 | spill上限`σ`、受け手の重み、`SpillFloor_m` | 重みは`A_m`（推奨）/ 計算比例 / `P_m`。`σ`は0（厳密`P_m`）から∞（ADR-0137）の間 |
| U4 | entrant reserve `ε`、`S_entry`、1モデル上限、適格期間 | 適格期間は`N_30`を推奨 |
| U5 | 配分epoch `E_alloc`と、`N_30`・`W_hold`との関係 | — |
| U6 | **`W_hold`の値**: RFC-0015 §8.5.5の7日相当（150 s/DAAで約4,032 DAA）とコードの280–436 DAA、RFC08の`W_budget = 400`の不一致 | どれを正とするか。記号の分離（`W_work` / `W_hold` / `W_budget` / `W_pwu`）も承認を求める |
| U7 | kernel経路（`B = F = 0`）への`ψ`の適用 | 報酬legへ`ψ`を掛ける（推奨）/ kernel経路は権利層の対象外 |
| U8 | 予算超過claim（`ψ`の見積り0）の受理と、queueの優先度 | 空きがあれば受理し、権利claimを押し出さない（推奨）/ 権利claimを優先する / 受理しない |
| U9 | queue: `L_queue_m`、bondの取り分、`T_queue`、期限切れ時のQ消費 | — |
| U10 | 検証者の容量attestationの形式と測定元、計算量の上限 | — |
| U11 | C7の導出上限の`sigma_cap`と最小値 | 最小1を推奨 |
| U12 | 検証者取り分の傾き`γ`と`u*`（10–30 %の範囲内） | — |
| U13 | verifier payの`N`・`m`の導出式、`checked_share`の目標 | — |
| U14 | quoteの余裕幅`h` | DAGの並行度の実測から |
| U15 | capのheadroom倍率（§5.6） | ECONの集中比1.4–1.8倍を下限の目安に |
| U16 | attestation gateで留保した予算の行き先 | 留保（推奨）/ spill |
| U17 | 案Dのtilt上限`k` | 1（無効、推奨） |
| U18 | 受入閾値: 新規モデルの最低権利シェア、T4のFinal率劣化の許容幅、T1の上昇幅 | — |
| U19 | 支払の保持（`max(Final, epoch close)`）の許容 | — |

これらが決まるまで本RFCはDraftであり、値を任意の件数・日数・倍率で埋めない。

## 13. 参照

* [PRINCIPLES](../PRINCIPLES.md) §0
* [ADR-0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md) D1–D7、§3、§10
* [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) D1・D4・D5、2026-10-14決定
* [RFC-0008](0008-palw-claim-backed-consensus-blocks.md) R8-BUDGET、[RFC-0015](0015-panel-free-permissionless-verification.md) §8.3–8.5.5
* [Bond budget / model allocation設計](../design/palw/bond-budget-and-model-allocation.md) §3.4–3.6、§8.4
* [RFC-0015 implementation status 2026-10-15](../design/palw/rfc-0015-implementation-status-2026-10-15.md) §1b
* [Activation readiness matrix](../design/palw/activation-readiness-matrix.md) §3f・§3g・§3k
* コード: [`palw_admission_v2.rs`](../../consensus/core/src/palw_admission_v2.rs)、[`palw_work_target_v1.rs`](../../consensus/core/src/palw_work_target_v1.rs)、
  [`palw_state_v2/fold_read.rs`](../../consensus/core/src/palw_state_v2/fold_read.rs)、[`palw_bond_budget_v1.rs`](../../consensus/core/src/palw_bond_budget_v1.rs)、
  [`palw_verifier_pay_v1.rs`](../../consensus/core/src/palw_verifier_pay_v1.rs)、[`palw_verify_capacity_v1.rs`](../../consensus/core/src/palw_verify_capacity_v1.rs)、
  [`palw_producer_v2.rs`](../../consensus/core/src/palw_producer_v2.rs)、[`config/params.rs`](../../consensus/core/src/config/params.rs)、
  [`misaka/palw/kernel/src/ledger.rs`](../../misaka/palw/kernel/src/ledger.rs)
