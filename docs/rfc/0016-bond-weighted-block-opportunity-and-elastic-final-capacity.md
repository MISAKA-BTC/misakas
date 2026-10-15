# RFC-0016 — bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する

> **設計前提:** [PRINCIPLES](../PRINCIPLES.md) §0（2026-10-14、モデルの非公開を経済的に不利にする。不正は排除ではなく有界化）を最上位とする。
> [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)（bond単位のQ/B/R/F上限と共通拘束）、
> [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)（モデル入手への合意不介入とモデル別配分）、
> [RFC-0015 §8.3–8.5](0015-panel-free-permissionless-verification.md)（2026-10-14確定: `α = 1.5`、30日平均、Miner＋Verifier元本、ブロック機会にも適用）を前提とし、変更しない。
> 本RFCは、bondの多いモデルが**物理的に**多くのブロック生成機会を得る設計と、人気モデルが検証容量の不足でFinal待ちにならない設計を扱う。

**トークンの表示名は Misaka、ticker は BILI。** 既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Accepted design (V+), values pending measurement — 2026-10-15。物理層の設計としてV+を採用した（第3回のユーザー判断）。
  値（`σ`、`k`ほか）は測定で決める。**有効化は§14のgateで止めたまま**であり、下の3つの「ユーザー判断」表は本番値の承認でも有効化の承認でもない。
* **Implementation status:** **implemented dormant, not activated, unbuilt**（branch `integ/rfc0016-impl`、§16。ADR-0176/0177とRFC-0017のbranchをmerge済み）。
  * P4: `eee1710ce`。
  * V+・弾力的Final容量・quote・report・test source・simulation: §16と
    [rfc-0016-implementation-status-2026-10-15.md](../design/palw/rfc-0016-implementation-status-2026-10-15.md)（行 → file:line）。
  * build・test・実node試験は未実施で、PASSではない。値はすべてplaceholderで、測定記録がないため、validationはどちらのfenceのarmも拒否する。
* **Source baseline:** `origin/pre` `4fb381a87`と、統合branch `integ/rfc02-rfc06-12-g14` HEAD `7dd86f2a0`。
* **Activation:** なし。fenceは`Params::palw_model_block_rights_v1`と`Params::palw_elastic_final_capacity_v1`（休眠）。
  全presetで`None`、Some-onlyでhash、`Some(never())`は`None`へ畳む。armすると拒否される（測定値記録のgateと、未有効化のgate）。
  高さ・wire tag・RPC op・署名domainは割り当てない。
* **Not activated by:** 本RFCやcodeのmerge、test PASS、本表の設計判断。
* **Normative language:** MUST / MUST NOTは、その設計を採る場合の要件である。採用を意味しない。

## ユーザー判断 2026-10-15（第1回: review、拘束）

| # | 判断 |
| --- | --- |
| R1 | RFC-0016はDraftのまま設計を続ける。B+を採用済みとも、元の目標を達成する設計とも書かない |
| R2 | 「物理ブロック生成機会」（共通W抽選の当選と、実際に生成されたblock）と「報酬対象ブロック権利」を分ける。経済的誘導による収束を主張するなら、条件と反例を書く |
| R3 | `α = 1.5`、30日平均、共通拘束（7日相当）は確定。280–436 DAAと400 DAAは選択肢ではなく、直すべきcodeのずれである。個別capを、配分を使い切らせるために引き上げない。使い切れない配分は報告する |
| R4 | Panel=0の前提（claimとblockの対応、B/R/Fの予約、coinbaseとescrowの区別）を先に確定する |
| R5 | 試験を追加する（entrant reserveの予算控除、モデル分割、容量0、容量の二重計上、queue中の予約、quote後の競合、無効claimによる希釈） |
| R6 | 目標`P_m`、物理block、報酬権利、Final、実際の支払、既存miner個人の収入を別々に報告する。目標シェアの増加だけで改善を報告しない。受入は物理・支払の列で測る |

## ユーザー判断 2026-10-15（第2回: 設計決定、本番値・有効化の承認ではない）

| # | 決定 | 本文 |
| --- | --- | --- |
| D-P3 | **Kernel claimにcoinbase subsidy legを作らない。** 利用者払いescrowの報酬は残し、既存のV2 coinbase経路とは分ける | §2.3 |
| D-P1/P2 | **`B = 0`、`F = 0`を維持する。** 通常のKernel claimはtransactionで運ばれ、それだけでは独立の報酬対象ブロック権利もFinal weightも作らない。Q、R、責任担保、carrierの資源上限は残す | §2.2 |
| D-P4 | **clipを禁止する。** coinbase由来のモデル予算が利用者escrowを削る経路（§2.4）を直す。producerの共通R capは適用する。最大報酬予約が足りない場合は**受理前に拒否**し、受理後にclipしない | §2.4、code `eee1710ce` |
| D-P5 | 今回の変更では既存のV2 Panel検証経路を残す。Panel=1への移行には、検証・rotation・責任・Final条件の別途確認が要る。**Kernel経路のPanel=0が、coinbaseを受け取る採掘経路をPanel-freeにすると書いてはならない** | §2.5 |
| D-Q1 | **bondに基づく物理ブロック生成機会を優先する。** 全モデルで1計算あたりの当選確率と報酬が完全に等しいこと（ADR-0137）は絶対条件ではない。物理抽選または事前発行権への接続を設計目標とする。B+だけで物理目標を達成したと扱わない。総発行予算、個別cap、検証条件、時計・heartbeatの保護は維持する | §7・§8 |
| D-H | **拘束期間: Panel=0は7日相当DAA、Panel=1は1日相当DAA。** modeを切り替えても発行能力を再取得できないこと（予算のreset・二重取得の禁止）は必須。件数・予約額・burstは、正直な最小構成の連続REAL生産と、高速偽造側の上限を同時に測って決める | §3 |

## ユーザー判断 2026-10-15（第3回: 設計採用、本番値・有効化の承認ではない）

| # | 決定 | 本文 |
| --- | --- | --- |
| D-V1 | **V+を、RFC-0016の物理層の設計として採用する**（旧V1を決定）。ほかの物理層の手段とB+は「検討した代替案」に下げる | §7・§8・§9 |
| D-σk | `σ`と`k`は**測定で**適切な値に決める。codeでは名前付きのfence定数として置き、測定値記録のgate（未測定・placeholderの値ではarmを拒否）を付ける | §8.6、code `palw_rfc0016_v1::values` |
| D-impl | 指示どおり実装を続ける。実装は採用・有効化を意味しない | §16 |

## 0. 要約

1. **二つの「ブロック生成機会」**
   * 物理ブロック生成機会: 共通`W_work`抽選の当選と、実際に生成されたモデルblock。cadence・DAA・fork choiceに効く。
   * 報酬対象ブロック権利: 台帳上の権利。報酬だけに効く。
   * 現行コードでは、物理blockは計算量だけで決まり、bondは入力にない（§1）。
2. **D-Q1により、物理層をbondへ接続することが設計目標になった。**
   §7で物理層の手段（ticketの傾き、採掘前の抑制、モデル別sub-target、beacon slot、事前発行権）を比較した。
   **採用した設計は「事前発行のモデル別ブロック権利（σ上限付きの途中再発行つき）を、抽選前のquoteとadmissionで要求する」V+である**（§8、D-V1。値は測定待ちで、未有効化）。
   これにより、bondの多いモデルは物理blockの**上限**を事前に保証される。実際に物理blockを生むには、そのモデルを掘る計算が要る。
   **計算のないモデルに物理blockを生ませる規則はない**（反例E1）。
3. **B+（報酬権利だけ）** とV+以外の物理層の手段は、検討した代替案である。B+は物理目標を達成しない（§9）。
4. **Panel=0のKernel claim**
   * 抽選も物理モデルblockもなく、報酬は利用者払いescrowだけである。
   * `B = F = 0`とcoinbase legなしは決定済みである。
   * `P_m`がKernel claimに配るものはない。
   * coinbaseを受け取る採掘経路は、V2のPanel検証のままである（§2）。
5. **拘束期間**はPanel=0が7日、Panel=1が1日である。調整の対象は、bond・期間あたりQ/B/R/F・予約額・burstの組合せで、1日あたりの発行をそろえる。mode切替で発行能力を取り戻せない（§3）。

## 1. レビュー主張の検証

`pre` = `4fb381a87`、`HEAD` = `7dd86f2a0`。

| # | 主張 | 判定 | 根拠（pre / HEAD） | 補足 |
| --- | --- | --- | --- | --- |
| (a) | 共通`W`、`p_m = min(1, CCU_m/W)`、claim数は入力にない | **CONFIRMED** | `palw_admission_v2.rs` pre L1130–1146 / HEAD L1134–1150。`palw_work_target_v1.rs` L116–125。floorはpre L1106 / HEAD L1110 | HEAD L1095–1103の`native_material_ticket_v3`も同じ関数を読む。claim数は容量admissionの入力である |
| (b) | epochごとの`W`のstep、共通、t12はDAA 0から | **CONFIRMED** | `palw_work_target_v1.rs` L94–114・L34。`params.rs` pre L10377–10401 / HEAD L10499–10523、HEAD L12303 | RC presetは7,101と7,780（HEAD L12022・L12038） |
| (c) | 容量とinflight上限が拒否する。人気モデルは検証資源で律速される | **PARTLY** | `fold_read.rs` pre L901 / HEAD L901–1063。`ClassInflightCapped` HEAD L1016・L1060。`PanelRoomExhausted` HEAD L1033・L1048。`palw_work_target_v1.rs` L205–245 | 8kはrate roomだけで判定される（L222–229）。roomはready seatに比例する（HEAD L1209–1217・L1266）。class間で差し引きがある（`palw_state_v2.rs` HEAD L13130–13146）。OPVは担保だけを見る（`ledger.rs` L2116–2129） |
| (d) | `P_m`は確定、engineは休眠、Rだけを削る | **CONFIRMED**（追加あり） | RFC-0015 §8.5.5 HEAD L627–674。`palw_bond_budget_v1.rs` L1–15、HEAD L2680、L711–734。設計書§3.5–3.6 | codeの`S_m`はminer-onlyのepoch snapshot（L10、L520–531） |
| (e) | backlog、配分と抽選の衝突、capによる制約 | **CONFIRMED**（将来リスク） | 上記とX1・X4 | 分析は§6 |

| # | レビューが扱っていない事項 | 根拠 |
| --- | --- | --- |
| X1 | Kernel claimは`B = F = 0`で、報酬はjob escrow | `palw_bond_budget_fold_v1.rs` L1501–1545、`ledger.rs` L147–151 |
| X2 | R8-BUDGET: fee-only Roundは独立の`ExecutionCap`、v1 market feeは`R_max`外 | RFC-0008 L292–301 |
| X3 | codeの拘束期間（280–436 DAA）とRFC08の400 DAAが、決定（Panel=0は7日、Panel=1は1日）とずれている | `palw_bond_budget_v1.rs` L32–37。§13のC1 |
| X4 | capは資本に線形、`P_m`は超線形 | `palw_bond_budget_v1.rs` L796–813。§9.6 |
| X5 | ADR-0177 D4・D5 | ADR-0177 L89–122・L238–250 |
| X6 | M\*-49の検査数は固定定数 | `palw_verifier_pay_v1.rs` L1–35 |
| X7 | attestation gate | PRINCIPLES §0-1、readiness §3f・§3k |
| X8 | F-Rはstake比例で、出荷では休眠 | `palw_verify_capacity_v1.rs` L21–30、`params.rs` HEAD L10769–10790 |
| X9 | 事前確認はあるが予約ではない | `palw_state_v2.rs` HEAD L12217、`palw_producer_v2.rs` L246–259 |
| X10 | ADR-0137 D5はモデル別配分を否定した。D-Q1でこの絶対性は解除される | ADR-0137 L303–325 |

## 2. Panel=0の前提（決定済み）

### 2.1 Kernel claimとそれを運ぶblock（事実）

* Kernel経路のobjectは「one object, one transaction」で運ばれる（`palw_kernel_route_fold_v1.rs` L6–12）。
  `ClaimCommitted`は、そのtransactionを受理するchain blockのfoldで発生する（L1395–1420）。reorgでは受理blockと予約が作り直される。
* claimは抽選ticketを持たない。利用者が投稿したjobに対してseal → revealで提出され、1つのjobに1つのclaimである（status doc L73）。
* claimのためのblockは生成されない（`palw_bond_budget_fold_v1.rs` L1501）。
* EXEC_SLICEはweightもDAAも0で、sliceごとのsubsidyはない（RFC-0008 L134）。
* 共通`W_work`抽選と物理モデルblockは、V2 Attempt lane（Panel検証）にだけ存在する（status doc L15–18）。

### 2.2 B/R/Fの予約（D-P1/P2で決定）

| 主体 | 時点 | 内容 |
| --- | --- | --- |
| jobの投稿者のbond | `PostJob` | escrow（`claim_reward`）を予約し、`job_fee`をburnする。検査料escrowも積む（`ledger.rs` L1584–1617） |
| producerのbond（ADR-0176台帳） | `ClaimCommitted`（受理chain block） | **Q = 1、B = 0、R = escrowの全額、F = 0**。責任担保と`liability_until`までのincome hold。共通拘束はここから数える |
| モデル台帳 | なし | D-P4により、Kernel claimはモデル予算から取らず、記録もしない |

Kernel claimは、それ自体では報酬対象ブロック権利もFinal weightも作らない。Q・R・責任担保・carrierの資源上限は残る。

### 2.3 資金の区別（D-P3で決定）

* Kernel claimの報酬は利用者払いescrowだけで、新規発行しない（`ledger.rs` L147–151、`palw_kernel_route_fold_v1.rs` L22–27）。
* coinbase subsidy（worker carve）はV2 Attempt/rider claimの経路だけに流れる。両者を混ぜない（MUST NOT）。
* `P_m`の配分（RFC-0015 §8.5.5）はcoinbase subsidyの配分なので、Kernel claimには適用しない。

### 2.4 clipの禁止（D-P4で決定、code `eee1710ce`、休眠・未build）

* 修正前: 配分fenceがあると、Kernel claimのR（escrow）が、coinbase由来の`model_available`でclipされていた。
* 修正: `palw_budget_origin_is_user_escrow_v1`（KernelRoute）と`PalwBondBudgetStateV1::preview_claim_for_origin_v1`を追加した。
  * 利用者払いのoriginはモデル予算を無視する。
  * escrowの全額を予約しなければならない。claimごとの上限を下回る場合は`EscrowNotReservable`、共通R capが足りない場合は`Exhausted { Reward }`。どちらも受理前に拒否する。
  * `reserve_claim_v1`とfoldの`plan_bond_budget_v1`はこの関数を使う。escrow claimはモデルを記録しない。
* test source（未実行）:
  * `rfc0016_p4_user_escrow_is_never_clipped_by_the_model_budget`
  * `rfc0016_p4_insufficient_r_refuses_an_escrow_claim_before_acceptance`
  * `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`

### 2.5 coinbase採掘経路とPanel（D-P5で決定）

* 今回の変更では、V2 Attempt lane（coinbase）の既存Panel検証経路を残す。
* **Kernel経路のPanel=0は、coinbaseを受け取る採掘経路をPanel-freeにしない**（MUST NOTで、そう記述しない）。
* coinbase経路の検証modeを変える（Panel=1へ移すことを含む）には、検証・rotation・責任・Final条件を別途確認する必要がある。

## 3. 拘束期間・発行能力の調整とmode切替（D-H、ADR-0176と整合）

実装はADR-0176側（branch `integ/adr-0162-0178`の別agent）が担当する。本RFCは設計上の要件を書くだけで、code作業を重複させない。

### 3.1 調整の対象

調整するのは拘束日数だけではない。次の**組合せ**全体である。

* bond額
* 期間あたりのQ/B/R/F
* 1 claimあたりの予約額
* 短期のburst・rate上限

**主条件（MUST）:**

1. 同じbond・同じmode・同じ期間なら、高速偽造は正直な計算より多くのQ/B/R/Fを得ない。
2. 基準となる正直なminerがcapに実際に到達できる。
   基準miner: Qwen2.5-1.5B A16の固定profile、基準機材、producer最小bond。
3. 需要・機材・検証がそろっているのに、capが低すぎてREAL生産が止まることはない。
4. heartbeatを0にしない。時計・fallbackの規則（ADR-0066/0140/0142/0165）を弱めない。

### 3.2 Panel=0とPanel=1の比較

* 最初の比較では、**1日あたりの最大発行**をそろえる。
  * Panel=1: 1日あたり`q, b, r, f`。
  * Panel=0: 7日あたり`7q, 7b, 7r, 7f`。
* 同じ件数で7倍速く回復させる形にしてはならない（MUST NOT）。
* `W_hold(Panel=0)` = 7日相当DAA、`W_hold(Panel=1)` = 1日相当DAA。どちらも**通常時の最早の再利用時刻**である。

### 3.3 能力の回復と担保の解放は別

* 発行能力の回復（`accepted_daa + W_hold(mode)`）と、担保の解放は別の条件である。
* 未解決のdispute・court・DA・責任期間がある限り、担保は保持する（MUST）。
* 共謀した早期のValid・早期のFinalで、Panel=1の1日を短くできない（MUST NOT）。
* 同時に必要な担保はおおよそ`λ · a · H`である。`λ`はclaimの率、`a`は1 claimあたりの責任担保、`H`は責任期間である。

### 3.4 短期のburst上限

* 週の総量の上限だけでは足りない。短い期間のburst・rate上限を追加する（MUST）。
* 消費した能力を早く戻してはならない。二重に発行してはならない（MUST NOT）。

### 3.5 mode切替

* reservationの最早回復時刻は受理時のmodeの`W_hold`で固定し、modeを切り替えても短縮しない（MUST）。
* mode切替では、消費履歴・hold・残存責任をすべて引き継ぐ（MUST）。window・履歴をresetしてはならない。mode別にwindowを新設して二重取得してはならない（MUST NOT）。
* 1つのbondのwindowは全modeで共有し、capは全modeの予約の合計に適用する。
* test source `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`（`eee1710ce`、未実行）は、現行の単一`W`の下でこの性質を固定する。mode別`W`の実装（ADR-0176側）でもこの性質を保つ。

### 3.6 値の決め方（3段階）

値はここで決めない。次の3段階で導出する。

1. **正直な最小構成の能力を測る。**
   * 基準miner（§3.1）と、各profile・各modeの最小構成で、連続REAL生産を測る。
   * 測る量: claim率、Final率、検証待ち、費用、`W_hold`期間の累積のp50/p99、短窓（1 span、10 span）のburstのp99。cold・warmの両条件で測る。
   * 同じrelease・同じparams・同じ網条件で、高速偽造側（計算省略、carrier・seal遅延・cap・担保・admissionだけが上限）の最大率とburstを、解析と実測の両方で同時に測る。
2. **capと予約額をsweepする。**
   * bond額 × 期間あたりのQ/B/R/F × 1 claimの予約額 × burst上限の組合せを掃引する。
   * 主条件（§3.1）、1日あたりの発行の同等性（§3.2）、`λ · a · H`の担保、を満たす範囲を求める。
3. **敵対的・境界試験を行う。** 選んだ点で、偽造・mode切替・早期Valid共謀・burst・期間境界・reorg/restartの試験を行う。

**制約が両立しない場合は、その衝突を報告する。数値を作り出して合わせてはならない（MUST NOT）。**
測定のcommit、params、機材、網条件、生データ、導出式をevidenceとして残す。UNKNOWNはPASSにしない。

## 4. 用語と記号

| 記号 | 意味 |
| --- | --- |
| `W_work` | ADR-0137のwork target（MAC-eq） |
| `W_hold(mode)` | 拘束期間。Panel=0は7日相当DAA、Panel=1は1日相当DAA（確定） |
| `N_30`、`S̄_m`、`P_m` | RFC-0015 §8.5.5（確定）。`P_m = S̄_m^1.5 / Σ S̄_j^1.5` |
| `c_m` | モデルの物理blockシェア（実測） |
| `N_exp(e)` | epoch `e`の期待モデルblock数（`closed_expected_blocks`、DAA数） |
| `Rt_m(e)` | §8の事前発行権 |
| `σ`、`κ`、`ε`、`τ_m`、`k` | spill上限、carry上限、entrant reserve、ticketの傾きとその上限 |

## 5. 目標と保持する制約

| ID | 内容 | 扱い |
| --- | --- | --- |
| G1p | `S̄_m`が増えると、そのモデルの**物理ブロック生成機会**が増える | **主目標**（D-Q1） |
| G1t | 目標（`P_m`）が単調 | 定義で成立 |
| G1r | 実際の支払が増える | 物理blockとFinalに依存する |
| G2 | 1計算あたりの当選確率と報酬がモデル間で等しい（ADR-0137） | **絶対条件ではない**（D-Q1）。崩す場合はその幅を有界にし、報告する |
| G4 | 人気モデルのFinalが検証者容量に比例し、不足は報告される | §10 |
| K1 | 総発行予算（per-DAA subsidy）を増やさない | 保持（MUST） |
| K2 | 個別Q/B/R/Fと`W_hold`（引き上げない） | 保持（MUST） |
| K3 | 検証条件（Final前に払わない、court、gate） | 保持（MUST） |
| K4 | 時計とheartbeatの保護 | 保持（MUST）。§8.4 |

ユーザー指示の「ADR-0172 heartbeat/fallback」は、このrepoでは次の規則を指すものと読む。
ADR-0066/0140（独立したheartbeat lane。ADR-0140 C1–C5: 難易度を締めない、他の何かが時計を進めている間は時計を進めない、fork-choice weightを取らない、稼がない・生産を奪わない、非常時に確実に始動する）、
ADR-0142（時計はcursorで、heartbeatがslotを消費する）、ADR-0165 rev5（BASE-0はidle時だけのbonded floor、REAL attemptがslot tickを運べる。FALLBACK-V1統合は撤回）。
現在のADR-0172はversioned kernelのADRである。

## 6. 物理ブロックと報酬権利の分析（背景）

### 6.1 経済的誘導による収束の条件（物理層を変えない場合）

次の全部が成り立つ場合に限り、報酬側の差によって物理シェア`c_m`が`P_m`へ近づく。

* C1: networkが飽和している。
* C2: 計算が移動できる（機材、RAM、cold fetch、seatingの費用が、期待利益差を下回る）。
* C3: 計算の前にquoteで見積りが分かる。
* C4: 移動先に検証容量がある。
* C5: 移動先がgateを通っている。
* C6: 30日平均と移動に要する時間がある。
* C7: spill上限`σ`が小さい。

### 6.2 反例

| ID | 状況 | 物理層を変えない場合の結果 |
| --- | --- | --- |
| E1 | minerがモデルを移れない（機材、モデル専用設定、RAM、cold fetch、seating） | bondの多いモデルの物理blockは変わらない。予算は余り、総支払が減る |
| E2 | 低bondモデルの需要が急増する | そのモデルの物理blockが急増し、`W_work`が上がって他モデルの物理blockが減る。30日平均は遅れて動く |
| E3 | 資本が多く計算が少ない | 物理blockは計算どおり。予算は大半が使われない |
| E4 | 計算が多く資本が少ない | 低bondモデルの当選が`W_work`を上げ、bondの多いモデルの物理blockを薄める |
| E5 | 無報酬の当選で`W_work`を吊り上げる | bondの多いモデルの物理blockが減る |
| E6 | `W₀`の領域（計算供給が少ない） | 予算が拘束せず、何も変わらない |

**結論:** 物理層を変えなければG1pは保証されない。これがD-Q1の理由である。

## 7. 物理層の設計空間（D-Q1）— V+以外は検討した代替案（D-V1）

### 7.1 手段

| ID | 手段 | 仕組み |
| --- | --- | --- |
| P-i | モデル別ticketの有界な傾き | `T_m = MAX · min(1, CCU_m · τ_m / W_work)`。`τ_m ∈ [1/k, k]`を`P_m / c_m`から決め、計算加重平均1に正規化する |
| P-ii | 採掘前の抑制 | 物理シェアが`(1+σ)P_m`を超えたモデルに、quoteとadmissionで新しい当選を受理しない |
| P-iii | モデル別sub-target | モデルごとの`W_m`を、`P_m · N_exp`へ向けてepochごとに調整する |
| P-iv | beacon slot割当 | 各slotのモデルを`P_m`で抽選し、slot内は共通`W_work`で決める |
| P-v | 事前発行権 | epoch開始前に、モデル別のブロック権利`Rt_m = ⌊P_m · N_exp⌋`を発行する。モデルblockは、そのモデルの権利を1本消費する場合だけ受理する。quoteは残りの権利を示し、権利のないモデルには計算をさせない |

### 7.2 保持する制約に対する評価

| 手段 | K1 総発行 | K2 個別cap | K3 検証 | K4 時計・heartbeat | G2の崩れ |
| --- | --- | --- | --- | --- | --- |
| P-i | 飽和時は保つ。`W₀`領域では正規化が要る（ないと計算あたりの発行率が上がる） | 保つ | 保つ | 保つ（slotの供給量は不変、tickの担い手だけが変わる） | 最大`k²`倍、有界 |
| P-ii | 保つ（余ったcadenceはfloor・heartbeatが埋める） | 保つ | 保つ | floorとheartbeatの始動（ADR-0140 C5）の試験が要る | 超過モデルは0（不連続） |
| P-iii | 保つ | 保つ | 保つ | class DAA状態の復活。floorのhardening（ADR-0137 §3.5）の再発リスク | 無制限 |
| P-iv | 保つ | 保つ | 保つ | slotの空き、beaconのgrinding、block validityの変更 | slot外は0 |
| P-v | 保つ（権利 ≤ `N_exp`、未使用分は発行しない） | 保つ（権利の消費はproducer bondのBの予約の中） | 保つ | 未使用の権利の分をfloor・heartbeatが埋める。ADR-0140 C1–C5の試験が要る | 権利を持つモデル間では等しい。使い切ったモデルは0 |

### 7.3 反例に対する評価

| 手段 | E1 移れない | E2 急増 | E3 資本≫計算 | E4 計算≫資本 | E5 吊り上げ | E6 `W₀` |
| --- | --- | --- | --- | --- | --- | --- |
| P-i | 解けない（計算なし） | 罰は`k`倍まで | 1計算あたりの当選が最大`k`倍 | 低bondモデルの当選が最大`1/k`倍。bondの多いモデルの物理blockは増える | 低bondモデルでの吊り上げを弱める | 正規化があれば中立 |
| P-ii | 解けない | 最悪（急増した人気モデルを止める） | 間接的（`W_work`が下がる） | bondの多いモデルの物理blockが増える | 止まる | 飽和時だけ効かせる |
| P-iii | 解けない（cold start trapもある） | 遅れて罰する | 効く | 効く | 効く | 振動する |
| P-iv | slotが空く | 罰する | slotが空く | 効く | 効く | slotが空く |
| P-v | 解けない。権利は余り、floorが埋める（発行しない） | 権利で頭打ち。σ再発行で緩和 | 権利で物理blockの**上限**を保証。計算があれば到達する | `W_work`が下がり、bondの多いモデルが権利まで到達する | 権利を超えられない | 権利が需要を上回り、制約しない |

**どの手段もE1を解けない。** 計算のないモデルは物理blockを生めない。
物理層でできるのは、bondの多いモデルが**計算を持っているときに**、その計算が当選しやすくなるか、他のモデルの当選を抑えることだけである。

## 8. 採用設計 V+: 事前発行権 ＋ σ上限付きの途中再発行 ＋ 抽選前のquote（D-V1で採用、値は測定待ち）

### 8.1 規則案

1. **発行:** epoch `e`の開始ブロックのfoldで、各モデルに`Rt_m(e) = ⌊(N_exp(e) − Ent(e)) · A_m / ΣA_j⌋`を発行する（MUST）。
   `A_m = ⌊S̄_m^1.5⌋`。`Ent(e) = ε · N_exp(e)`はentrant reserveで、固定総額から先に差し引く。新規発行ではない。
   `Σ Rt ≤ N_exp`（K1）。発行はDAA数で決め、時計を読まない。
2. **消費:** モデル`m`の当選（V2 Attempt）は、admission時に`m`の残り権利を1本消費する場合だけ、モデルblockとして受理する（MUST）。
   消費はproducer bondのB/R予約（ADR-0176）の中で行い、個別capを超えない（K2）。
   権利がないモデルの当選は受理しない。quoteで事前に知らせるので、計算を使わせない。
3. **抽選:** 権利を持つモデルの間では、共通`W_work`抽選のまま（G2は権利を持つモデル間で保たれる）。
4. **途中再発行（σ）:** epochの割合`φ`を過ぎると、需要の超過したモデルは自分の権利を超えて、解放された権利を使える。
   * 上限: そのepochの合計が`Rt_m · (1 + σ)`以下（MUST）。
   * 他のモデルの保護: 各モデルの残りの見込み`min(未使用, ⌈used · remaining / elapsed · pace margin⌉)`は使えない。
   * 全体: epochの合計`Σ Rt`は超えない（MUST）。
   * 実装の形（2026-10-15）: 状態を持たない形にした。checkpointでのsnapshotを置かず、epochのカウンタとDAAの位置だけから決まる。
     超過したモデルの間の配分は`A_m`加重ではなく、抽選の順（先着）である。`A_m`加重には新しい状態が要るため、加重にするかは測定後の判断とする（V2）。
   * 残りは失効し、そのcadenceはfloor（BASE-0）とheartbeatが埋める。発行はしない。
   * `σ`はE2（急増）の罰とcadenceの損失を緩める調整値である（`σ = ∞`ならADR-0137に近く、`σ = 0`なら厳密な`P_m`）。
5. **新規モデル:** entrant reserveの権利を線形・上限付きで配る（登録から`N_30`未満、報酬gateを通過、`S_m ≥ S_entry`）。
6. **Final・支払:** 権利付きのモデルblockのclaimは、既存の検証条件（K3）でFinalになり、claim自身のcarveの全額を受け取る（`ψ`は不要）。
   有罪・voidの場合は払わず、権利は戻らない（`W_hold`と同じ扱い）。
7. **容量との結合:** quoteは、残りの権利と検証容量（§10）の両方を見て`Admit`を返す。

### 8.2 G1pの達成範囲

* bondの多いモデルは、物理blockの上限（`Rt_m`、再発行を含む）を事前に保証される。そのモデルの計算が上限まで当選できる限り、物理シェアは`P_m`に到達する（E3・E4で効く）。
* 計算が足りない場合（E1）、権利は失効し、目標は達成されない。この不足を`unused_rights_m`として報告する（§11）。

### 8.3 主なリスク

* 権利の失効が多いと、モデルblockの総数が`N_exp`を下回り、`W_work`は`W₀`へ下がる。floorとheartbeatが埋めるcadenceが増える。
* 権利の最後の1本をめぐる競合（並行block）。quoteの余裕幅`h`とqueue（§10）で吸収し、浪費を報告する。
* epoch内では早い当選が権利を取る。抽選の速さで決まるので、先着の殺到は抽選のrateで制限される。
* 権利はfork-choice weightではない。権利の残数を時計・DAA・bitsの入力にしない（MUST NOT）。

### 8.4 時計とheartbeatの保護（K4）

* heartbeat lane（ADR-0066/0140）は権利を消費せず、権利の影響を受けない（MUST）。ADR-0140 C1–C5を保つ。
  * 難易度を締めない。
  * 他の何かが時計を進めている間は時計を進めない。
  * fork-choice weightを取らない。
  * 稼がない。
  * 全モデルの権利が尽きても非常時に始動する。
* 時計のcursor（ADR-0142）: slotの供給と消費の規則は変えない。権利はslot tickを増やさない（ADR-0165 B: REAL attemptが運ぶtickの規則も変えない）。
* floor（ADR-0165 A″）: 権利が尽きたモデルの空いたcadenceでfloorがidleになり、BASE-0が採掘できる。floorは無報酬の残余のままである（ADR-0137 D3）。
* DAAと`bits`: 権利はDAA・`bits`・難易度・`W_work`の調整式（`palw_work_target_step_v1`）を変えない。`W_work`の入力（実現したモデルblock数）は既存のとおり。
* 必須試験: 全モデルの権利を0にしたnetworkで、heartbeatとfloorが目標cadenceを維持し、DAAが崩れない（§12.4のH1–H4）。

### 8.5 その他の手段の位置付け（検討した代替案）

* **`k`の正確な意味:** V+の補助としての、モデル別ticketの有界な傾き（P-i）`τ_m = clamp(P_m / c_m, 1/k, k)`の上限である。
  `c_m`はモデルの実現した物理シェアである。
  * `k = 1`（`tilt_k_permille = 1000`）で無効になる。これが既定である。
  * 権利を持つモデルの間に残る計算の偏り（同じ権利数のモデル間の当選速度）だけを調整する。
  * 計算単位の公平性（G2）を最大`k²`倍まで崩す。
  * 抽選への組込みは未実装である（純関数`palw_rights_tilt_permille_v1`のみ）。値の測定と別途の判断が要る。
* P-ii、P-iii、P-ivは、§7の評価から採らない（検討した代替案）。

### 8.6 `σ`と`k`の導出（D-σk）

**何を制御するか**

* `σ`: `φ`を過ぎたあと、需要の超過したモデルが使える再発行権利の上限（`Rt_m · σ`）。
  * 大きいほどADR-0137（計算比例）に近づき、E2（急増）の罰と未使用権利の失効が減る。
  * 小さいほど`P_m`に厳密になる。
* `k`: §8.5の補助ticket傾きの上限。
  * 大きいほど、bondの多いモデルの1計算あたりの当選を増やせる。
  * その分G2を崩し、人気（計算）モデルを罰する。
* 同時に決める従属の値: `φ`（再発行の開始割合）と`pace margin`（他モデルの保護の強さ）。

**測定ハーネス**

* モデル: 基準のQwen2.5-1.5B A16の固定profileと、CCUの異なる追加モデル（少なくとも重いものと軽いもの）。
* 機材: 基準機材。
* 構成:
  * 正直な最小構成（producer最小bond、1 miner）。
  * 複数miner構成（同一モデルの複数miner、モデル間を移動できるminerとできないminerの混在）。
* シナリオ: E1–E6（移れないminer、需要急増、資本≫計算、計算≫資本、`W_work`の吊り上げ、`W₀`領域）と、敵対的試験A1–A12。
* 測る量: §11の全列。特に、モデル別の物理block数・実際の支払・既存minerの個人収入・未使用権利・queue待ち・heartbeat依存のDAA。
* 道具:
  * `scripts/sim/palw-rfc0016-vplus-sim.py`: 測定する領域を選ぶためのモデルで、測定ではない。
  * 実node・実機の測定: 未作成（evidence-only）。

**目的**

* bondが多く計算を持つモデルについて、物理blockと支払が`P_m`を追う度合いを最大にする。
* 失効する権利（unminted）を有界にする。
* 未使用の権利を使った妨害（griefing）ができない。
* heartbeat・時計の規則は変えない（測るだけ）。
* 人気モデル（急増）を停止させない。

**掃引範囲（初期案、測定の前提で変えてよい）**

* `σ` ∈ {0, 0.1, 0.25, 0.5, 1, 2}
* `k` ∈ {1, 1.25, 1.5, 2}
* `φ` ∈ {0.25, 0.5, 0.75}
* `pace margin` ∈ {1.0, 1.2, 1.5}

**選択規則**

* すべての合格条件を満たす領域から選ぶ。合格条件は、上の目的のそれぞれについて事前に決めた閾値（V6）である。
* 満たす領域がなければ、衝突する制約を報告し、値を作らない（MUST NOT）。
* 選んだ値は`PalwRfc0016MeasuredRecordV1`（evidenceとindependent reviewのdigest）として`PALW_RFC0016_MEASURED_RECORDS_V1`に固定する。
  それまでvalidationはarmを拒否する。

## 9. B+（報酬権利だけ）— 検討した代替案（D-V1）

* B+は物理抽選を変えず、`P_m`で報酬権利と支払を配る（epoch closeの`ψ`、carry、σ上限付きspill、entrant reserve）。
* **B+だけでは物理目標を達成しない**（D-Q1）。V+との比較基準として残す。
* 規則案: `Base_m = ⌊(Pool − Ent) · A_m / ΣA⌋`、`ψ_m = min(1, (min(Budget, Demand) + SpillIn) / Demand)`。
  無効・有罪のclaimは`Demand`から除き、正直な`ψ`を薄めない。
* 個別capは引き上げない。使い切れない配分（`stranded_by_bond_caps_m`）は報告する（R3、MUST NOT）。
* attestation gateで拒否されたmodel-epochは、どこからも受け取らない。

## 10. 弾力的Final容量（Panel経路）

* **検証者単位の容量:** `Cap_m = Σ_v c_v · x_{v,m}`。1人の検証者の配分の合計は`c_v`以下で、二重計上しない。Hall条件を満たす配分だけを容量として数える。
* **C7の導出上限:** `cap_m = max(1, ⌊sigma_cap · Cap_m · window_m / claim_replay_m⌋)`。`Cap_m = 0`なら0とし、受理しない。
* **quote:** `admit / queue / refuse`、残りの権利（V+）、bondの残り、容量、余裕幅`h`を返す。side blockの需要も数える。
* **queue:** 上限付き。bondの取り分は資本比例。窓はqueueを出た時点から数え、queue中はFinalにならない。予約は1 claim分まで。`T_queue`を過ぎたらslashなしでvoidとする。
* **OPV:** 容量待ちはない。検査数を請求率から導出し、`checked_share_m`を報告する。
* **検証者の誘因:** 取り分を10–30 %の範囲内で傾ける。
* **不足の報告:** `capacity_short_m`と不足量を返す。黙ってFinalを遅らせない（MUST NOT）。

## 11. 計測と報告

| 列 | 内容 |
| --- | --- |
| (1) 目標 | `S̄_m`、`P_m`、発行した権利`Rt_m` |
| (2) 物理block | 当選し受理されたモデルblock数、物理シェア`c_m`、権利不足で受理されなかった当選、`unused_rights_m` |
| (3) 報酬権利 | 消費した権利、再発行の入出、失効、`stranded_by_bond_caps_m` |
| (4) Final | Final数、void、有罪、`checked_share_m` |
| (5) 支払 | モデル別の支払。coinbase subsidyと利用者escrowを分ける |
| (6) 個人収入 | 既存minerごとのepochあたり支払と、計算あたり支払 |
| (7) 容量 | `Cap_m`、検証者数、queue、`capacity_short_m`と不足量 |
| (8) 時計 | heartbeat・floorが埋めたslot数、DAAの偏差 |

* 目標（列1）の増加だけで、物理（列2）や収入（列5・6）の改善を報告してはならない（MUST NOT）。
* 受入は列2・列5・列6で判定する（MUST）。UNKNOWNはPASSにしない。

## 12. 受入試験

すべての試験で§11の全列を記録する。

### 12.1 ユーザー指定

| ID | ケース | 合格基準 |
| --- | --- | --- |
| T1 | 同じCCU、Aの計算がBの3倍 | 物理（列2）・支払（列5・6）を測る。V+では、Aの物理blockは`Rt_A`（再発行を含む）を上限に増える |
| T2 | Aのbondだけを増やす（Aに十分な計算がある場合と、ない場合の両方） | V+: 計算がある場合、Aの物理block（列2）が増える。ない場合は`unused_rights_A`として報告する。B+: 物理は不変と報告する |
| T3 | 全モデルで急増 | `W_work`が上がる。権利は`P_m`のまま。モデル別のhardeningはない |
| T4 | Aだけが急増 | V+: Aは権利と再発行の上限まで。不足は報告する。容量不足はqueueに入り、報告される |
| T5 | 検証者を追加 | `Cap`が増え、queueが減り、Final数が増える |
| T6 | 既存bondがcap | capを超えない。使い切れない分は報告し、capは引き上げない |
| T7 | 新規モデルの参入 | entrantの権利付与と、物理block・支払を測る。下限（判断V6）以上 |

### 12.2 R5の追加試験

| ID | 試験 | 合格基準 |
| --- | --- | --- |
| N1 | entrant reserveの予算控除 | `Σ Rt + Σ Ent ≤ N_exp`。reserveの有無で総発行の上限が同じ |
| N2 | ほぼ同一のモデルを複数登録する分割 | `P`の合計は分割前以下。entrantの総額は一定。capの合計は不変。支払と物理blockの合計が分割前を超えない |
| N3 | 容量0での受理 | `Cap = 0`のモデルは受理しないか`capacity_short`を返す。quoteは`Refuse` |
| N4 | 検証者容量の二重計上 | 検証者ごとの配分の合計が`c_v`以下 |
| N5 | queue中の最大予約 | 1 claim分を超えない。queueの合計がモデル予算・cap・権利を超えない |
| N6 | quote後の競合 | `h`の範囲内ではqueueが吸収する。超えた分は報告し、浪費率を記録する |
| N7 | 無効・有罪claimによる希釈 | 正直なclaimの支払（V+）と`ψ`（B+）が下がらない |

### 12.3 第2回の決定に対応する試験

| ID | 試験 | 合格基準 | 状態 |
| --- | --- | --- | --- |
| P4a | escrowをclipしない | モデル予算が不足しても、Kernel claimのRはescrowの全額。モデルを記録せず、モデル予算は不変 | test source `rfc0016_p4_user_escrow_is_never_clipped_by_the_model_budget`（未実行） |
| P4b | Rの不足は受理前に拒否 | 共通R capの不足は`Exhausted { Reward }`、claimごとの上限の不足は`EscrowNotReservable`。何も書かない | test source `rfc0016_p4_insufficient_r_refuses_an_escrow_claim_before_acceptance`（未実行） |
| M1 | Panel=0とPanel=1の切替で発行能力を再取得しない | 両方向で、先のreservationが`d + W`まで共有windowに残り、早期Finalでも戻らない | test source `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`（未実行。mode別`W`の実装後に拡張する） |
| M2 | mode別の`W_hold` | Panel=0のreservationは7日相当、Panel=1は1日相当で、受理時のmodeで固定される | 未実装（§13のC1） |
| M3 | 1日あたりの発行の同等性 | 同じbondで、Panel=0の7日の上限（`7q, 7b, 7r, 7f`）とPanel=1の1日の上限（`q, b, r, f`）の比が、1日あたりで同じになる | ADR-0176側 |
| M4 | 能力の回復と担保の解放 | 共謀した早期のValid・Finalでも、Panel=1の1日（Panel=0の7日）より前に能力が戻らない。未解決のdisputeがあれば担保を保持する | ADR-0176側 |
| M5 | 短期のburst上限 | 週の総量の範囲内でも、短窓の上限を超えない。消費した能力が早く戻らず、二重発行がない | ADR-0176側 |
| M6 | 基準minerがcapに到達 | Qwen2.5-1.5B A16の固定profile・基準機材・producer最小bondで、連続REAL生産がcapまで到達し、需要・機材・検証があるのに停止しない。heartbeatは0にならない | 測定（§3.6） |
| M7 | 偽造は正直より多くを得ない | 同じbond・mode・期間で、高速偽造のQ/B/R/Fが正直な計算以下 | 測定・敵対試験（§3.6） |
| K1a | Kernel claimにcoinbaseが流れない | どの経路でもKernel claimへのcoinbase支払が0 | 未作成 |

### 12.4 時計・heartbeat（V+）

| ID | 試験 | 合格基準 |
| --- | --- | --- |
| H1 | 全モデルの権利が0 | heartbeatとfloorが目標cadenceを維持し、DAAが崩れない（ADR-0140 C5） |
| H2 | 権利の失効が多い | heartbeatが難易度を締めず、weightを取らず、稼がない（ADR-0140 C1・C3・C4） |
| H3 | 権利とslot tick | 権利の消費がslot tickを増やさない。cursorの規則が不変（ADR-0142、ADR-0165 B） |
| H4 | reorg・restart | 権利の発行・消費・再発行を決定的に復元する |

### 12.5 敵対的試験

資本の付け替え（A1）、短期借入（A2）、Sybil検証者（A3）、容量の占拠（A5）、自己取引（A6）、`W_work`の吊り上げ（A7）、reorg（A8）、attestation gate（A9）、検査しないverifier（A11）。
合格基準は第1回版と同じで、物理・支払の列で判定する。

## 13. codeは決定に従う（選択肢ではない）

| ID | 是正 | 状態 |
| --- | --- | --- |
| C1 | 拘束期間の定数をmode別にする: Panel=0は7日相当DAA、Panel=1は1日相当DAA。1日あたりの発行を同等にする（§3.2）。`PALW_BOND_BUDGET_W_MIN/MAX_DAA_V1`（280–436）とRFC08の`W_budget = 400`を合わせる。mode切替で履歴・hold・責任を引き継ぎ、windowを共有する（§3.5）。短期burst上限（§3.4） | ADR-0176側の実装（`integ/adr-0162-0178`）が担当。本branchでは重複しない |
| C2 | `S_m`を、30日平均のMiner＋claimに拘束されたVerifier元本にする | 未実施 |
| C3 | α = 1.5を確定値にする（`alpha_halves = 3`。interimの表記を直す） | 未実施 |
| C4 | escrowをclipしない（D-P4） | `eee1710ce`（休眠、未build） |
| C5 | Rの不足を受理前に拒否する（D-P4） | `eee1710ce`（休眠、未build） |

## 14. 有効化gate

~~~text
ACTIVATION_ALLOWED
    = V+ が採用されている（D-V1、済）
      AND §14.1 の値が §3.6 の3段階導出に基づいて決まっている
      AND §13 の C1–C5 が完了している
      AND §12 の全試験が物理・支払の列で独立review付きPASS
      AND 実node E2E（V2 Panel経路、Kernel経路、mode切替、reorg/restart/IBD）PASS
      AND ADR-0177 の二段評価を再計算
      AND 全RFCを一度に有効化する協調upgradeに入っている
~~~

### 14.1 本RFCで決めない値と判断

| ID | 値・判断 |
| --- | --- |
| V1 | **決定済み（D-V1）: V+を採用。** `k`の値と、P-iを抽選へ組み込むかは§8.6の測定の後 |
| V2 | `σ`、`φ`、pace margin（§8.6の測定で決める）。再発行を`A_m`加重にするか（新しい状態が要る） |
| V3 | `ε`、`S_entry`、1モデル上限 |
| V4 | bond額 × 期間あたりQ/B/R/F × 1 claimの予約額 × burst・rate上限の組合せ（§3.6の3段階で導出） |
| V5 | epochの長さ（`N_30`と`W_hold`は確定済み） |
| V6 | 受入閾値（新規モデルの下限、T4の劣化許容幅） |
| V7 | queue（`L_queue`、取り分、`T_queue`）、quoteの`h` |
| V8 | 容量attestationの形式、`sigma_cap`、計算量の上限 |
| V9 | 検証者取り分の傾き、verifier payの`N`・`m`の導出、`checked_share`の目標 |
| V10 | attestation gateで留保した分の行き先（B+） |
| V11 | coinbase経路をPanel=1へ移すか（D-P5: 別途確認） |

確定事項（`α = 1.5`、30日平均、`W_hold` 7日（Panel=0）・1日（Panel=1）、`B = F = 0`、Kernel claimへのcoinbase legなし、escrowのclip禁止）は判断対象ではない。
個別capを引き上げて配分を使い切らせる選択肢は設けない。

### 14.2 ユーザー判断 2026-10-15（V2〜V11）の実装状況

数値は校正・受入試験の出発点で本番値ではない。休眠・未build・未試験（PASSではない）。行番号は[実装状況](../design/palw/rfc-0016-implementation-status-2026-10-15.md) §8。

| ID | 状態 |
| --- | --- |
| V1 | 決定済み（D-V1）→ 実装済み（§16） |
| V2 | 2026-10-15決定 → 実装: 校正初期値 σ=0.25・φ=0.5・pace 1.2・k=1（`PALW_RFC0016_CALIBRATION_START_V1`）、`A_m`欠損時は再発行なし（`ReissueUnavailable`、先着なし）。本番値は測定待ち |
| V3 | 2026-10-15決定 → 実装: entrant reserve ≤ 20‰、RFC-0017 Probationと一つのpool（明示的netting）、Probationモデル上限2‰、`S_entry` = 適格な最低producer bond |
| V4 | 測定待ち |
| V5 | 2026-10-15決定 → 実装: 1 epoch = 720 DAA、30日窓 = 21,600 DAA（校正）、DAAと実時間のずれをRPCとharnessで報告（「24 h」と表示しない） |
| V6 | 2026-10-15決定 → 測定・選択toolに実装（不変条件違反0、正直cap利用率≥80 %、V+なし比REAL劣化≤5 %暫定）。測定は未実施 |
| V7 | 2026-10-15決定 → 実装: bond資本比例のqueue取り分、quoteは権利でない（決定論的な再検査）、TTL超過は中立終了。`L_queue`・`T_queue`・`h`は測定待ち |
| V8 | 2026-10-15決定 → 実装: 署名対象を固定した容量attestationの型・検査、verifierのモデル間二重計上なし、`sigma_cap` ≤ 1、計算量・メモリ・DA・court負荷の一体評価。attestationのcarriage（object種別）は未割当 |
| V9 | 2026-10-15決定 → 実装: 10〜30 %・基準20 %、`checked_share`と期限内完了率を別計測。傾き・`N/m`は測定待ち |
| V10 | 2026-10-15決定 → 実装（構造上そうなっていることを試験sourceで固定）: 留保分は他へ再配分せず未発行、後日追加発行しない |
| V11 | 2026-10-15決定 → 実装: coinbaseのPanel移行なし（`PALW_RFC0016_COINBASE_PANEL_MIGRATION_V1 = false`と試験） |

## 16. 実装（休眠、未有効化、未build）

実装は採用・有効化を意味しない。行ごとのfile:lineと試験IDの対応は、[実装状況](../design/palw/rfc-0016-implementation-status-2026-10-15.md)にある。
すべての行は、実装済み、test source作成済み、evidence-onlyのどれかである。

### 16.1 lead decision（2026-10-15、ユーザーreview待ち）

| # | 内容 |
| --- | --- |
| L1 | blockへの`P_m`の適用は1回だけで、V+の権利が行う。V+をarmするとき、ADR-0177 V3の`block_opportunity`は必ずoffにする（validationが拒否する）。V+がreleaseのブロック機会の仕組みである。coinbase側の`P_m`は既存のcoinbase配分が1回だけ適用する（ADR laneのC4もこれで解決する）。単一適用のtest sourceがある |
| L2 | 途中再発行（V2）は`A_m`加重で行う。新しいrooted状態（aux 132）を持ち、reorg・IBDで一貫する。~~`A_m`が得られないときだけ、先着をfallbackとして残す~~ → **ユーザー判断 2026-10-15（V2）で置換: `A_m`が無ければ再発行しない（先着で権利を与えない）** |
| L3 | RPCはop 255、proto 1330–1331を使う（確定済み） |

### 16.2 実装の範囲

* **fence:** `palw_model_block_rights_v1`と`palw_elastic_final_capacity_v1`（Params、identity、validation、fork-id probe、V2 mirror）。
  * どちらもkernel route（`palw_probabilistic_constraints_v1`）が同じ高さ以下にあることを要求する。
  * 孤立calibration networkだけは、buildに入れた`MISAKA_RFC0016_CALIBRATION_VALUES`でarmできる（σ/k測定用）。
* **aux table 132–135**（131はRFC-0017が使う）: 再発行snapshot、pending queue、queue membership、telemetry。
  native scannerのarmは、generatorと生成fileの両方にある。
* **V+:**
  * 発行は`⌊(N_exp − Ent) · A_m / ΣA⌋`で、entrant reserveは固定総額から先に差し引く。
  * 消費は、既存の`epoch_counters`をそのまま使う。
  * admissionは、自分の権利か、`φ`後の`A_m`加重snapshotか（ない場合は先着σのfallback）で決まる。epoch総額は超えない。
  * quoteは、権利・容量・実際のqueue長を示す。
  * RFC-0017のeligibilityをmerge済みで、次のように読む。
    * 発行は、probation外のblock rightを持つモデルだけに行う。
    * Probationのclassは、RFC-0017のprobation grantを権利とするので、V+の検査を通す。
    * Registered/Lapsedは、RFC-0017自身のgateが拒否する。
* **弾力的容量:**
  * 検証者単位のwater-fillingで、容量0の場合は受理しない。
  * C7上限は、容量から導出する。
  * **queueはclaimのlifecycleに組み込み済み**:
    * 待機中はbind期限がない。
    * 待機中は`PanelBound`が効かない。
    * FIFOでdequeueし、そのとき`rebound_daa = now`とする。これでbind窓とanchor slotが、queueを出た時点から始まる。
    * TTLを過ぎるとBindTimeoutでvoidとする。producerへの課金はない。
    * 1 claimが予約する権利は1本だけである。
    * bondごとの取り分は資本比例で、受理前に拒否する。
  * OPV経路の検査容量gate。
  * M\*-49の検証者取り分の傾き（10–30 %の範囲内）。
* **計測:**
  * consensus-derived: 目標、物理block、権利、Final、coinbase支払、既存minerの個人収入、escrow支払、queue待ち、REAL gap、heartbeat依存DAA。
  * node-local（そう表示する）: 拒否件数。
* **RPC op 255 `GetPalwRfc0016Report`**（proto 1330/1331）: JSONで返す。目標と結果を分け、node-localの観測を区別する。
* **ツール:**
  * `scripts/sim/palw-rfc0016-vplus-sim.py`: モデルであり、測定ではない。
  * `scripts/drills/rfc0016_sigma_k_measure.py`: 実node測定のharness。
  * `scripts/sim/palw-rfc0016-report.py`: 目標の増加だけでは改善と表示しない。
* **test source（未実行）:**
  * `consensus/core/tests/rfc0016_v1.rs`
  * `consensus/src/pipeline/virtual_processor/tests/rfc0016_e2e.rs`（H1–H4、A8）
  * 各moduleのunit test
* **evidence-only:**
  * σ/kの測定値と測定値記録。
  * V+のcalibration測定。ADR laneの配分fenceがcalibrationでarm可能になることが前提である。
  * A11（検査しないverifier）。
  * M6/M7の実測。

## 15. 参照

* [PRINCIPLES](../PRINCIPLES.md) §0
* ADR: [0066](../adr/0066-the-heartbeat-lane-out-of-header-bits-and-a-committed-liveness-table.md)、[0137](../adr/0137-a-block-buys-one-unit-of-work-from-any-model-and-a-share-is-a-result-not-an-input.md)、
  [0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md)、[0142](../adr/0142-the-consensus-clock-is-a-cursor-a-heartbeat-consumes-a-slot.md)、
  [0165](../adr/0165-the-floor-is-a-reserve-and-the-work-carries-the-clock.md)、[0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、
  [0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)
* RFC: [0008](0008-palw-claim-backed-consensus-blocks.md) R8-BUDGET・L134、[0015](0015-panel-free-permissionless-verification.md) §8.3–8.5.5
* 設計: [bond-budget-and-model-allocation](../design/palw/bond-budget-and-model-allocation.md) §3.4–3.6、
  [rfc-0015-implementation-status-2026-10-15](../design/palw/rfc-0015-implementation-status-2026-10-15.md)、
  [activation-readiness-matrix](../design/palw/activation-readiness-matrix.md) §3f・§3g・§3k
* code: [`palw_admission_v2.rs`](../../consensus/core/src/palw_admission_v2.rs)、[`palw_work_target_v1.rs`](../../consensus/core/src/palw_work_target_v1.rs)、
  [`fold_read.rs`](../../consensus/core/src/palw_state_v2/fold_read.rs)、[`palw_bond_budget_v1.rs`](../../consensus/core/src/palw_bond_budget_v1.rs)、
  [`palw_bond_budget_fold_v1.rs`](../../consensus/core/src/palw_bond_budget_fold_v1.rs)、[`palw_kernel_route_fold_v1.rs`](../../consensus/core/src/palw_kernel_route_fold_v1.rs)、
  [`ledger.rs`](../../misaka/palw/kernel/src/ledger.rs)、[`palw_verifier_pay_v1.rs`](../../consensus/core/src/palw_verifier_pay_v1.rs)、
  [`palw_producer_v2.rs`](../../consensus/core/src/palw_producer_v2.rs)、[`config/params.rs`](../../consensus/core/src/config/params.rs)

## ユーザー方針 2026-10-15 による改定

* V+のblock権利は生成機会であり合意creditではない。権利を消費したclaimも、`SecurityCertified`になるまで合意creditを持たない。
* 同じmergesetのsibling間で、権利の消費はmerge順に依存してはならない([RFC-0018](0018-dag-parallel-multi-model-consensus.md) §4のcanonical順と同じ集合関数)。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。
