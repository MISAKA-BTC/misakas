# RFC-0017 — モデル資格: 登録は自由、影響は制限、通常資格は選べない検証者の検査と継続更新で得る

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。
> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)のbond当たりQ/B/R/F上限と共通DAA拘束を適用する。本RFCはそれらの式・値を変えず、その外側に「モデル単位の資格」による上限を追加する。
> **モデル入手への不介入 — [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md):** chainはTorrent/HTTP等の取得可否を判定しない。本RFCの検査は「抽選された検証者が承認したか」だけを読み、未承認の理由が入手不能であっても計算不正のslashにはしない。

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Draft — 2026-10-15。ユーザー設計（2026-10-15指示）の文書化と、休眠fenceの下の実装（**implemented, dormant, unverified**）。build/test/実node試験は未実施でPASSではない。
* **Decision:** 登録は自由・不変のまま（ADR-0175）、新規モデルの経済的・合意上の影響を段階（Registered / Probation / Eligible / Lapsed）で制限する。通常資格への昇格は、モデル・検査対象claim・参加stakeを先に固定し、その後に生じる乱数で全体のverifier stakeから抽選された検証者 `m` 人中 `k` 人の認証済み承認を要する。資格は一度の合格で永久化せず、更新期間ごとに再検査する。subsidy・報酬付きblock権利・Final weightの三つを別々にgateする。
* **Activation:** 休眠fence `palw_model_eligibility_v1`（`Option<ForkActivation>`）。全presetで`None`、flag-dayリストに含めない、fingerprintはSome-only、`Some(never())`は`None`へ畳む、armすると`validate_palw_model_eligibility_v1`が拒否する。A-2一様: fence未満ではkernel inner 74/75はA-2 gateで落ち、fold・gate・RPCは旧規則と同じ値を返す。前提fence: `palw_probabilistic_constraints_v1`（aux行を持つkernel route）と`palw_bond_budget_v1`（F=0のreservationとprincipal共通cap）が同じ高さ以下。
* **Baseline:** 設計根拠は`pre` = `4fb381a87`（2026-10-14のα=1.5確定とattestation gate維持を含む）、実装は統合branch `integ/rfc02-rfc06-12-g14` = `2e74a485b`の上。`pre`の文書commit 4件（`640b7c242`〜`4fb381a87`）はこのbranchに無く、このbranchは`pre`より182 commit進んだcodeを持つ（merge-base `64699e960`）。本文のcode行番号は`2e74a485b`のもので、`pre`で同じ行のもの・`pre`に無いものは§3で明記した（`scripts/ci/check-doc-citations.sh`で両treeを照合）。
* **Not activated by:** 本RFCのmerge、設計値表（§11）、単体試験sourceの存在、simulation driverの出力。
* **Normative language:** MUST / MUST NOTは本fence有効化後の規則。設計値は名前付きの未承認値であり本番承認ではない。
* **RFC-0016:** 本branchの`docs/rfc/`には存在しない（他branchで予約中と扱う）。本RFCは0015の書式に揃えた。

## 0. 原則

**登録は自由。ただし新規モデルの経済的・合意上の影響は制限する。通常資格への移行には、モデル登録者が選べない検証者による検査を要求する。資格は一度の合格で永久化せず継続的に更新する。**

* 「登録から7日後に自動で通常資格」や「最初の100 claimが合格したら通常資格」は不十分である（MUST NOT）。モデル・Miner・Panelを同一攻撃者が握る場合、待機時間は資本の放置だけで、claim合格は自作自演のPanel受理だけで消費できる。どちらも登録者が選べない検証者を一度も通らない。
* 昇格条件は「登録者が選べない検証者が、自分で取得したモデルで検査し承認した」ことでなければならない（MUST）。chainが読むのは抽選・認証済み検証記録・期限・資格条件だけであり、モデルの配布・取得可否は読まない（ADR-0177 D1）。
* 資格は更新期間ごとの再検査で維持する（MUST）。合格後に非公開化・偽造へ転じたモデルは次の更新で承認されず失効する。失効しても既存claimの担保・challenge・settlement責任は消えない（MUST）。

## 1. 脅威モデル

ユーザー仮定: 攻撃者は最大 **300M BILI**（総供給の3 %、購入価格と流動性は範囲外）を持ち、Miner・Panel・verifier・複数モデルへ同じ資金を二重使用せず**最適配分**し、最悪の配置を探索する。

三つの割合を区別して報告する（MUST）。混同して「3 %だから安全」と結論してはならない（MUST NOT）。

| 割合 | 定義 | 例 |
| --- | --- | --- |
| 総供給比 | 300M / 総供給 | 3 %（ユーザー仮定） |
| 配分対象の実効拘束資本比 | 攻撃者の拘束資本 / 配分で数える全拘束資本（RFC-0015 §8.5.5のMiner＋Verifier元本） | 正直な拘束資本が300Mだけなら **50 %** |
| 抽選されるverifier stake比 `f` | 攻撃者のverifier stake / 抽選母集団のstake | 抽選を「対象モデルを既に保有するverifier」だけから行うと **100 %** になり得る |

* `f`は総供給比ではなく、全体verifier stakeの規模で決まる。ECON文書（readiness §3f: 敵対者上限1/3、`m = 47, k = 24`の暫定値）と同様に、**全体verifier stakeについての別の仮定**を明示しなければ`P_pass`は計算できない（MUST）。本RFCは「正直なverifier stake `V_honest`」を名前付き入力とし、値を決めない（§11）。
* 抽選母集団を対象モデルの保有者に限ると、閉鎖モデルでは攻撃者だけが母集団になる（`f = 100 %`）。したがって母集団はネットワーク全体のverifier stakeでなければならない（MUST、§4C）。
* 攻撃者の最悪配置の探索空間: Miner資本 / verifier stake / Panel / モデル数 / bond分割 / 登録・再登録の時期 / 検査試行の時期 / 公開→非公開の切替 / Panel=1・Panel=0のmode切替。§7の8ケースはこの空間の必須の切り口である。

## 2. α = 1.5 の増幅

RFC-0015 §8.5.5（ユーザー決定2026-10-14、撤回しない）: `P_m = S̄_m^1.5 / Σ_j S̄_j^1.5`。攻撃者が全額を1モデルに集中し、正直な資本97 %が`n`モデルに均等に分散する場合:

~~~text
P_attack = 0.03^1.5 / (0.03^1.5 + 0.97^1.5 / √n)
         ≈ 0.54 %  (n = 1)
         ≈ 5.16 %  (n = 100)
         ≈ 14.68 % (n = 1000)
~~~

（数値は`palw_eligibility_alpha_share_f64`と`scripts/sim/misaka-palw-rfc0017-eligibility-sim.py`の解析部で再計算した。）

* これは**cap・資格gate適用前の配分目標**であり支払額ではない。実際の支払はADR-0176の個別bond cap、総発行予算、本RFCの資格gateで制限される。
* α = 1.5は撤回しない。ただし **「3 %の集中攻撃者 対 多数モデルに分散した正直資本」を有効化前の必須採用試験とする**（MUST、§7-2）。報告は「配分目標」「cap後の実支払」「Final weight」を分ける。

## 3. 既存部品と経路別の適用範囲

### 3.1 既存部品（registry行は`4fb381a87`と`2e74a485b`で同じ行。それ以外は`2e74a485b`の行、pre行を括弧で併記）

| 部品 | 場所 | 何をgateするか |
| --- | --- | --- |
| registry lifecycle `Registered / Prefetching / Probation / ActiveLimited / Active / Held / Candidate / Capped` | `consensus/core/src/palw_model_registry_v1.rs:354` | claimの受理可否（`admits_claims` :398）と受理速度（`admission_permille` :406: Probation 50‰、ActiveLimited 100‰、Active 1000‰） |
| `Candidate`（「登録は資格ではない」） | 同 :379、遷移 :575 | network母集団から抽選したadmission jury（ADR-0147）の過半がready-sealedのときだけ`Prefetching`へ |
| admission jury（抽選検査者） | seed v1 :511 / v2 :533、抽選`palw_admission_jury_v1` `palw_panel_v2.rs:1816`（operatorごと1 ticket）、駆動`transition_builder.rs:5896`（pre 5751） | 乱数は実行laneの`round_seed_anchor`（ADR-0130、`palw_execution_lane_v1.rs:403`）。block hashではなく勝利した推論の実行commitment |
| Probation → ActiveLimited → Active | 同 :610 / :624（`probation_claims: 10`、`stable_epochs: 3`、:94–108） | probe claimの合格件数と安定epoch。**probe claimの合否は自分のPanelでも作れる** |
| ECON round 2の全体attestation gate（`palw_model_verification_gate_v1`、inner 67 / aux 125） | policy `palw_econ_contracts_v1.rs:89`、抽選`palw_model_verification_draw_v1` :169（stake比例・復元抽選）、payout判定 `palw_econ_contracts_fold_v1.rs:236`（`gate_final_reward_v1`、`transition_builder.rs:8977`だけから呼ばれる） | **subsidy（モデル配分のFinal reward）だけ**。claim・escrow・Final・weight・slashには触れない（readiness §3f、pre `activation-readiness-matrix.md:247`） |

ECON gateはpreでは文書（readiness §3f/§3k）、code（`palw_econ_contracts_v1.rs`）は統合branchにだけある。

### 3.2 経路別の現状（`2e74a485b`）

`pre`での対応行: PP `palw_v2_unentitled_blues` 3056、`palw_v2_payout_outputs` 3325、`palw_candidate_order_v2` 452、`palw_v2_check_attempt_admission` 7860、`palw_v2_check_receipt_spend` 8098; TB `finalize_claim` 8727（weight 8766–8772、reward 8827、ECON gateなし）; BBF `plan_attempt_budget_v1` 1583; `fold_read.rs` 901/943; `objects.rs` 3117/3226; SV2 18555/18615、20434/20496。`palw_econ_contracts_v1.rs`、`palw_econ_contracts_fold_v1.rs`、`palw_kernel_settlement_v1.rs`、`palw_flat_execution_fold_v11.rs`、`palw_funded_execution_v11.rs`、`palw_panel_free_gate_v1.rs`は`pre`に無い。`palw_model_registry_v1.rs`、`palw_issuance_slots_v1.rs`、`consensus/src/processes/coinbase.rs`、`palw_fork_choice.rs`は両treeで同じ。

略号: SV2 = `consensus/core/src/palw_state_v2.rs`、TB = `…/palw_state_v2/transition_builder.rs`、BBF = `consensus/core/src/palw_bond_budget_fold_v1.rs`、KRF = `consensus/core/src/palw_kernel_route_fold_v1.rs`、PP = `consensus/src/pipeline/virtual_processor/processor/palw.rs`。

| 経路 | (a) admission | (b) 報酬付きblock権利 / B cap | (c) coinbase / subsidy | (d) Final weight / chain choice | registry段階を読むか |
| --- | --- | --- | --- | --- | --- |
| V2 Attempt | `apply_attempt` SV2:20714 → `check_class_admits_claim` `fold_read.rs:901` → `class_lifecycle_refusal` :714。processor側 `palw_v2_check_attempt_admission` PP:8960 | issuance slot `palw_issuance_slots_v1.rs:173`、B: `plan_attempt_budget_v1` BBF:1629（SV2:21016で呼出） | 補助金carveをescrow（SV2:20872）→ Finalで`bond_budget_final_reward_v1` TB:8973 → ECON gate TB:8977 → `palw_v2_payout_outputs` PP:4094 | TB:8898–8920（`palw_claim_safe_contribution_v3` → C7 → `bond_budget_final_weight_v1` BBF:1806）、暫定credit `reserve_for_claim` TB:8244 | admissionのみ。Final weight・coinbaseは段階を読まない |
| Free Prompt | `FreePromptCommitted` `objects.rs:3055`、claim gateは`audit_2026_09_23`以後だけ（:3178 / :3287） | `plan_free_prompt_budget_v1` BBF:1654、receipt spend BBF:1678 | receipt blockのcoinbaseでworker carve（`coinbase.rs:426`）。escrow 0なのでECON gateは届かない | receipt spendで`safe_weight`へ加算 SV2:18884。Final自体はweight無し | fence以後のadmissionだけ |
| Kernel / Panel=0 (OPV) | KRF:943、`opv_gate_v1` KRF:403、予算 `plan_kernel_claim_budget_v1` KRF:1403 | **B = 0、F = 0**（BBF:1541） | 利用者払いescrowのみ（KRF:685/858）。subsidy legなし | なし（`palw_kernel_settlement_v1.rs:22–24`） | 読まない |
| native funded execution | `palw_flat_execution_fold_v11.rs:757`（`checked_class` :94はkernel descriptorの`Active`） | B = R = F = 0（BBF:2119–2131） | requester escrowのみ（:907–1003） | なし（BBF:2063） | 読まない |
| EXEC slices (RFC-0008) | `apply_root_declared_v2` `palw_exec_v2_fold.rs:785`（rootのREAL attemptが経路1を通過済み）、slice :1048 | sliceに独自blockなし（rootの1 block） | rootのFinal reward内で分配 `settle_at_final_v2` :1162（TB:9030–9122） | rootのAttempt Finalだけ | rootを通じて間接的に |

**判明したgap（本RFCで閉じる/記録する）**

* G-1: 登録段階は**admissionだけ**に効き、coinbase（c）とFinal weight（d）はregistry段階を読まない。`Probation`のclaimも`Active`と同じweightを得る。
* G-2: Probation → Active の昇格条件（probe合格件数・安定epoch）は自分のPanelで満たせる。§0の「待機」「最初のN件合格」と同じ弱点。
* G-3: ECON gateはsubsidyだけを止め、block権利とFinal weightを止めない（§6）。
* G-4: 検査試行の総数を数えるものが無い。jury（ADR-0147）は監査周期ごと1回に制限するが、再登録・別class idでの再挑戦の累計は数えない（§5）。
* G-5: Kernel/OPVとnative fundedは既にB = F = 0・利用者払いのみなので、資格gateの対象外で良い（user-paid test jobに相当、§4A）。

## 4. 設計

### A. 登録直後（Registered）

* モデルは存在する（不変・登録自由、ADR-0175）。利用者払いのtest job（kernel/OPV、native funded、escrowのみの経路）は完了・精算できる（MUST）。
* 登録・自分のPanelのReady・自分のclaimのFinalは、通常のcoinbase配分・報酬付きblock権利・Final weightを**一切**生まない（MUST NOT）。
* 検査報酬のための新規発行はしない（MUST NOT）。検査費用は利用者・登録者・検証者の既存escrow/手数料の範囲（GAP-E3）。

### B. Probation（試用）

* 影響は `min(principal共通cap, 段階別モデルcap, network全体のprobation予算)` 以下（MUST）。
  * principal共通cap = ADR-0176のbond `B` cap（本RFCは編集しない。予算planの拒否として働く）。
  * 段階別モデルcap = lineage（重みの同一性 = artifact root）ごとのepoch上限。
  * network全体のprobation予算 = epochごとの**固定値**。モデル数・principal数に比例して増えない（MUST）。
* 初期提案: **probation claimのFinalはFinal weightを持たない**。「job/service完了」と「合意上のcreditを作るFinal」を分離する（§6の表）。
* 資本分割・再登録をしても消費履歴と検査試行回数は残る（MUST）: 段階別capと試行回数はlineage（artifact root）に付き、class idを変えても共有される。principal単位の累計も記録する。完全に別の重み（摂動した重み）は別lineageであり、それはnetwork固定予算と試行予算で抑える（Sybilで増えない部分だけを保証とする）。
* RegisteredからProbationへの移行は既存registryを再利用する: registry lifecycleが`admits_claims()`（ADR-0147のnetwork抽選juryがCandidateを通し、登録者以外のready seatが揃った状態）になった時点。

### C. 通常資格への昇格（commit-then-draw）

1. **先に固定する（MUST）:** inner 74 `CommitEligibilityExamV1`でモデル（class id）、検査対象claim集合のroot・件数、目的（昇格/更新）、lineageの試行番号を記録する。参加stakeは**commit時点のDAAで切る**（それ以後に作られたbondは抽選されない）。
2. **後から乱数（MUST）:** commit時点で記録済みのseed anchorより**厳密に後**のanchor（`round_seed_anchor`、ADR-0130の実行lane。再抽選には勝利する推論が要る）を使い、`seed = H(exam id ‖ anchor span ‖ anchor execution key)`。
3. **抽選（MUST）:** 全体verifier母集団（Active、commit前`verifier_maturity`以上に登録、最小担保、committer自身のoperatorは除外）から**stake比例・復元抽選で`m`位置**（R7-ECONの`palw_model_verification_draw_v1`を再利用。bond分割は期待位置数を変えない）。検査claimも同じseedで選ぶ。
4. **承認（MUST）:** 抽選位置のうち`Reproduced`（登録rootで認証した自分のcopyで、登録specどおり再現した）を出した位置が`k`以上で合格。登録者は検査者を指名できず、不利な抽選を捨てても試行は消費済み。
5. 正直なverifierがモデルを入手できない場合は`Unavailable`となり、肯定承認は出ない。これは**「昇格条件未達」であり計算不正のslashではない**（MUST NOT slash）。
6. chainが判定するのは抽選・認証済み検証記録・期限・資格条件だけ（MUST）。transcriptの中身は判定しない。
7. **Sybil:** 署名数は独立性の証明にならない。安全性は抽選と資源比（`f`）の仮定で評価する。追加仮定として「**抽選された検証者のうち一定割合が正直に検査する**」を置く（これはBFTのblock合意ではない）。

### D. 通常運用（Eligible / Lapsed）

* 定期再検査が報酬資格・生成資格を更新する。一度の合格は永久でない（MUST）。`next_renewal_daa`までに更新検査に合格しなければ、猶予後`Lapsed`（Registeredと同じ権利）。
* 資格喪失後も既存claimの担保・challenge・settlement責任は残る（MUST）。
* **claim単位の責任とモデル全体の資格判断を分ける（MUST）:** 検証者のattestationはモデル単位の判定（`Reproduced / Unavailable / NotReproducible`）と、検査claimの不一致リスト（各claimの既存courtへ送る告発材料）を別に持つ。不一致リストはモデルの判定に数えない。したがって**一人のMinerの意図的な不正で、公開モデルの正直なMinerが止まることはない**。

## 5. ゲートの数学

独立なstake比例抽選で、攻撃者の抽選stake比が`f`のとき、攻撃者だけで合格する確率:

~~~text
P_pass = Σ_{i=k..m} C(m,i) f^i (1−f)^(m−i)

f = 3 %、4-of-7:  P_pass ≈ 0.00264 %（2.636e-5）/ 試行
10,000回の独立試行で少なくとも1回合格:  1 − (1 − P_pass)^10000 ≈ 23.2 %
~~~

* したがって、大量登録・再申請・長期運用を通じた**総試行数を有界にし数える**（MUST）。`redraw-until-lucky`を防ぐ（MUST）。実装: lineage生涯上限、principal/epoch上限、**network/epoch固定上限**、lineage cooldown、commit時点で試行を消費（撤回・再登録で返らない）。network上限`A`があれば攻撃者の総合格確率は `1 − (1 − P_pass)^A ≤ A·P_pass` で抑えられる。
* 次は**別の要因**であり`P_pass`に含まれない: 乱数のbias（anchorの再抽選費用）、検査者への攻撃（妨害・買収）、確認せず承認（lazy verifier）、疎な不正の見逃し（検査claim数が有限）。
* `P_pass`は計算のsoundnessでも有罪確率でもない（MUST NOT 同一視）。
* network固定上限はgriefing面を持つ: 攻撃者が試行枠を先に消費すると正直な新モデルの待ち時間が伸びる（§7で「正直モデルの待ち時間」として必ず報告する）。試行あたりの手数料は未接続（GAP-E3）。

## 6. 三つの帰結を別々にgateする

subsidyだけを止めるのは不十分である（G-3）。三つを別々にgateする（MUST）。

| 帰結 | 中身 | 本RFCのhook |
| --- | --- | --- |
| coinbase | subsidy資格と支払（モデル配分`R_m`、補助金carveの報酬） | Registered/Lapsedはclaim gateで報酬付きclaimを受けない。モデル配分の入力`S̄_m`は`palw_eligibility_allocation_capital_v1`でRegistered/Lapsedを0（配分呼出への接続はGAP-E1） |
| 報酬付きblock | 発行権・admission・ADR-0176 `B` capの消費 | claim gate（REQ-ELIG-1）、Probationは`min(principal B cap, モデル段階cap, network予算)`（REQ-ELIG-2） |
| chain choice | 暫定credit（immature）とFinal weight（`F`） | Probation claimはADR-0176予約を`F = 0`で立てる（`PalwBudgetPlanV1::without_final_weight_v1`）。全再導出（`bond_budget_weight_or`、`retire_claim`、整合性検査）が同じ0を読む |

**どのFinalがjob完了で、どのFinalが合意creditを作るか（MUST明示）**

| Final | job完了・escrow精算 | 合意credit（Final weight） |
| --- | --- | --- |
| Kernel/OPV claim、native funded execution | はい | いいえ（B = F = 0、既存規則） |
| EXEC slice | はい（root内の分配） | いいえ（rootのAttempt Finalだけ） |
| V2 Attempt / Free Prompt（Eligibleまたはfence未満・genesis） | はい | はい（既存規則） |
| V2 Attempt / Free Prompt（Probation、初期提案） | はい | **いいえ**（予約F = 0、pin `'C'`） |
| V2 Attempt / Free Prompt（Registered / Lapsed） | 受理されない（利用者払い経路を使う） | — |

## 7. 受入シナリオ — 必須の300M攻撃

各ケースで次を**別々に**報告する（MUST）。「安全/危険」だけのラベルで報告してはならない（MUST NOT）。

* 最大報酬流出（R_PALWに対する攻撃者の割合、期間平均）
* 攻撃者の採用block比
* Final weight比
* 正直モデルの待ち時間
* 再検査（昇格・更新）の成功確率

| # | ケース | 必須の観測 | 試験source |
| --- | --- | --- | --- |
| 1 | 全額を1つの閉鎖モデルへ集中 | Registered中は流出0、Probation中は段階cap以下・weight 0、昇格は`P_pass(f)`だけ | `palw_model_eligibility_v1/tests.rs` `req_elig_1_*`, `req_elig_2_*`; driver S1 |
| 2 | 正直資本が多数モデルへ分散（n = 1/100/1000） | α増幅の配分目標とcap後実支払を分けて | `alpha_one_point_five_amplification_*`; driver S2 |
| 3 | 300Mを多数モデル・bondへ分割 | network probation予算は固定、試行はnetwork上限内、αの劣加法性 | `req_elig_2_the_network_probation_budget_is_fixed_*`, `s7_2_3_4_union_bound_*`, `req_elig_3_draws_are_stake_proportional_and_sybil_neutral`; driver S3 |
| 4 | 正直なverifier stakeが小さい（総供給3 % ≠ verifier 3 %） | `f`を変えて`P_pass`を報告 | `s7_2_3_4_union_bound_*`; driver S4 |
| 5 | 最初は公開・正直、昇格後に非公開・偽造 | 次回更新で承認されず失効、露出窓 = 更新期間 + 猶予 | `s7_5_closed_after_promotion_lapses_at_the_next_renewal`; driver S5 |
| 6 | 抽選を見てから撤回・再登録 | 試行はcommit時に消費、lineageで継承、同番号の再送拒否 | `s7_6_*`, `req_elig_4_*`; driver S6 |
| 7 | 検証者・証拠の妨害 | 正直モデルの待ち時間、`Unavailable`はslashしない | `req_elig_3_k_positive_positions_*`; driver S7 |
| 8 | 1日/7日mode切替を30日・90日 | 切替でprobation予算・試行を再取得しない | `s7_8_mode_switching_re_acquires_nothing`; driver S8 |

**simulation driver:** `scripts/sim/misaka-palw-rfc0017-eligibility-sim.py`。全てのmodel parameterは名前付き定数（`ATTACKER_BILI`、`ALPHA`、§11の設計値、`HONEST_CHECK_RATE`、`ATTACKER_ATTEMPT_SHARE`、`BOND_CAP_MULTIPLE`、`HOLD_DAYS`、scenarioごとの`honest_verifier_bili`等）。出力は上の5指標をscenario×期間（30/90日）ごとに別列で出す。driverはchainの実装を実行せず規則をmodel化したものであり、出力は検証でも値の承認でもない。driverの既知の単純化（30日平均の立ち上がり遅れを無視＝攻撃者に有利、ADR-0176 capは`BOND_CAP_MULTIPLE`で代用、乱数biasと検査者攻撃は未model化）はdocstringに列挙した。

## 8. 実装（dormant）

### 8.1 fenceと割り当て

| 種類 | 割り当て | 確認 |
| --- | --- | --- |
| fence | `Params::palw_model_eligibility_v1: Option<ForkActivation>`（`config/params.rs`）、mirror `PalwStateParamsV2::model_eligibility_v1`（`#[borsh(skip)]`） | 全preset `None`、identity: collapse・pin list・fingerprint 2箇所Some-only、`fork_id_v1.rs`のprobe名、`validation.rs`で拒否 |
| kernel inner kind | **74** `CommitEligibilityExamV1`、**75** `AttestEligibilityExamV1` | 使用中63–71・73をgrep、72は未割当のまま、76以上も未使用 |
| kernel route aux table | **131**（65-byte key: `'M'`model、`'L'`lineage、`'P'`principal、`'N'`network、`'E'`exam、`'C'`probation pin） | 使用中〜130をgrep。scanner armは`scripts/drills/gen_rfc06_native_wire.py`と`gen_rfc06_native_inventory.py`へ追加し、生成器を再実行（`wire_kernel_aux.rs`、`wire_generated.rs`、`generated.rs`） |
| RPC | 新opなし。`RpcPalwModelLifecycle`へ追記（wire version 3、proto `RpcPalwModelLifecycle`内のfield 39–45） | ops ≤ 244、`messages.proto` ≤ 1307は不変 |
| object tag / p2p / funded action | 割り当てなし | — |

### 8.2 module

* `consensus/core/src/palw_model_eligibility_v1.rs` — 段階・権利・policy・行・鍵・effective stage（A-2）・probation予算・試行・commit-then-draw・verdict・更新/失効・status・解析関数・fence（`Params`）。
* `consensus/core/src/palw_model_eligibility_fold_v1.rs` — `palw_state_v2`の子module。inner 74/75のfold、sweep（anchor→抽選→判定→結果、失効、古いexam・pinの退役）、各hook。
* `misaka/palw/kernel/src/model_eligibility_v1.rs` — inner 74/75のwire型（kernel crate）。ledgerは両kindを拒否（consumer専有）。
* 再利用: registry lifecycle（`admits_claims`）、`round_seed_anchor`（ADR-0130/0147）、R7-ECONの抽選関数、ADR-0176の予算plan。

### 8.3 hook（呼出箇所）

| 場所 | hook | 規則 |
| --- | --- | --- |
| `fold_read.rs` `check_class_admits_claim`（Attempt / FreePrompt） | `eligibility_claim_refusal_v1` | Registered/Lapsedは`ELIG_NOT_ELIGIBLE` |
| `palw_state_v2.rs` `apply_attempt`（予算planの直前/claim書込後） | `eligibility_probation_preview_v1` / `_commit_v1` | Probationは段階cap・network予算で全量受理または拒否（`ELIG_PROBATION_BUDGET`）、F = 0で予約、pin |
| `objects.rs` FreePromptCommitted（同上） | 同上 | `quanta`ブロック分 |
| `palw_bond_budget_fold_v1.rs` `PalwBudgetPlanV1` | `without_final_weight_v1`（追加のみ、engine規則は不変） | 予約F = 0 |
| `transition_builder.rs` `finalize_claim` | `eligibility_final_weight_v1` | pin退役（weightは予約F = 0で既に0） |
| `palw_kernel_route_fold_v1.rs` dispatch / closing tick | inner 74/75、`sweep_model_eligibility_v1` | — |
| processor `palw.rs` A-2 gate | `PalwKernelInnerFenceV1::ModelEligibilityV1` | fence未満は落とす |
| モデル配分（ADR-0177、`palw_bond_budget_v1`の`roll_epoch`） | `palw_eligibility_allocation_capital_v1`を提供 | **未接続（GAP-E1）**: 当該fileは別laneが改定中のため編集しない |
| RPC `GetPalwModelRegistry` | `palw_eligibility_status_of_v1` → row fields | stage、次回更新DAA、三権利、合格数、lineage試行数 |

## 9. 未解決のgap

* **GAP-E1（coinbase配分の分母）:** Registered/Lapsedの資本を配分の分母・分子から外す呼出（`roll_epoch`の入力）を未接続。ADR-0176/0177 lane（`integ/adr-0162-0178`）が`palw_bond_budget_v1.rs`を改定中のため、提供関数`palw_eligibility_allocation_capital_v1`を合流後に呼ぶ。未接続の間、駐車資本による他モデルの希釈が残る（支払自体はclaim gateで止まる）。
* **GAP-E2（Panel=0とkernel route）:** OPV/kernel claimはB = F = 0なので対象外とした。将来kernel claimに報酬legが付くなら同じhookが要る。
* **GAP-E3（試行手数料）:** policyに手数料はない。network試行枠のgriefing（§5）への費用は未設計。新規発行はしない。
* **GAP-E4（evaluation lane）:** RFC-0004のevaluation claim（`PalwGatedClaimV1::Evaluation`）はgateしていない。
* **GAP-E5（claims_rootの検証）:** chainは検査claim集合の中身を判定しない（rootと件数だけ）。検証者が当該claimを取得できなければ`Unavailable`になる。
* **GAP-E6（genesisとpre-fence）:** genesis classは対象外、pre-fence classは更新1期間だけ猶予（U-5/U-6）。

## 10. 必須試験とactivation checklist

~~~text
ACTIVATION_ALLOWED
    = 本RFCの値（§11）のユーザー承認
      AND GAP-E1 接続（配分の入力が資格を読む）
      AND 単体試験 PASS（palw_model_eligibility_v1/tests.rs、kernel wire、RPC wire v3）
      AND A-2 一様試験 PASS（t12_a2u_new_kinds_uniform: fence遥か上で全block同一）
      AND §7 の8ケースを real node・独立driverで5指標ごとに報告（ラベルではなく数値）
      AND reorg / restart / IBD / carriage（aux 131）で同じ状態
      AND 前提fence（palw_probabilistic_constraints_v1、palw_bond_budget_v1）が同じ高さ以下
      AND 別途調整したnetwork schedule
~~~

本RFCの単体試験sourceは書いたが、build・実行はしていない（PASSではない）。

## 11. 本RFCで決めない値

次は**設計判断の名前付き値であり、本番承認ではない**。validationは全てのactivationを拒否する。

| 名前 | 設計値（未承認） | 意味 |
| --- | --- | --- |
| `PALW_ELIG_EXAM_M_DESIGN_V1` / `_K_` | 7 / 4 | 抽選位置数と必要承認数（kは過半） |
| `PALW_ELIG_EXAM_CLAIMS_DESIGN_V1` | 4 | 1検査で再実行するclaim数 |
| `PALW_ELIG_ANCHOR_DELAY_SPANS_DESIGN_V1` | 1 | commit後、何span後のanchorから抽選できるか |
| `PALW_ELIG_ATTEST_WINDOW_DAYS_DESIGN_V1` | 2日 | 抽選後の承認期限 |
| `PALW_ELIG_PROBATION_MIN_DAYS_DESIGN_V1` / `_MIN_CLAIMS_` | 7日 / 16件 | 最初の昇格commitまでの最短期間と最少claim数 |
| `PALW_ELIG_PROBATION_EPOCH_DAYS_DESIGN_V1` | 1日 | probation予算epoch |
| `PALW_ELIG_PROBATION_NETWORK_PERMILLE_DESIGN_V1` / `_MODEL_` | 20‰ / 2‰ | network固定予算 / lineage段階cap（epochのblock機会比） |
| `PALW_ELIG_ATTEMPTS_PER_LINEAGE_DESIGN_V1` | 24 | lineage生涯試行 |
| `PALW_ELIG_ATTEMPTS_PER_PRINCIPAL_EPOCH_DESIGN_V1` / `_NETWORK_EPOCH_` | 4 / 64 | principal・network のepoch試行上限 |
| `PALW_ELIG_ATTEMPT_EPOCH_DAYS_DESIGN_V1` / `_COOLDOWN_DAYS_` | 30日 / 3日 | 試行epochとlineage cooldown |
| `PALW_ELIG_RENEWAL_EPOCH_DAYS_DESIGN_V1` / `_WINDOW_` / `_LAPSE_GRACE_` | 30日 / 7日 / 2日 | 更新期間・更新窓・失効猶予 |
| `PALW_ELIG_VERIFIER_MATURITY_DAYS_DESIGN_V1` | 7日 | 母集団の成熟期間 |
| `probation_final_weight` | false | 初期提案（Probation FinalはFinal weightなし） |
| `day_daa` | networkのblock時間から導出 | 日数→DAA換算 |
| 正直なverifier stake `V_honest`、正直検査率 | 決めない | §1の別仮定。driverでは名前付き入力 |
| 試行手数料・検査者報酬 | 決めない | 新規発行なし（GAP-E3） |

未確定の値を任意の件数・日数・倍率で埋めて安全性の根拠にしない。

## 12. ユーザー判断が必要な事項

* **U-1** §11の設計値（m/k、予算、試行上限、更新期間）。
* **U-2** Probation FinalのFinal weightを0とする初期提案の確定。
* **U-3** 全体verifier stakeの前提（`V_honest`の下限、またはECONの1/3上限との統合）とm/kの選び方（ECON gateの47/24と統一するか）。
* **U-4** 試行枠griefingへの対策（手数料・burn・lineage優先）と、新規発行をしない検査者報酬の財源。
* **U-5** pre-fenceの登録モデルの扱い（本実装は更新1期間の猶予後に失効、更新検査で継続）。
* **U-6** genesis classを対象外とすること。
* **U-7** ECON round 2 gate（subsidyのみ）との関係: 併存させるか、本RFCの更新検査で置き換えるか。
* **U-8** GAP-E1の接続方法（ADR-0177 laneの`roll_epoch`へ資格の入力を渡す）。

## 13. 参照

* [PRINCIPLES](../PRINCIPLES.md)、[ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)、[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)
* [RFC-0015 §8.5.5](0015-panel-free-permissionless-verification.md)（α = 1.5、30日平均、Miner＋Verifier元本）
* [activation-readiness-matrix §3f/§3g/§3k](../design/palw/activation-readiness-matrix.md)（ECON round 2 gate）
* [PALW確率・経済安全性ゲート](../design/palw/probabilistic-economic-security-gate.md)
* code（`2e74a485b`、pre `4fb381a87`で同じ行は§3.1に明記）: `consensus/core/src/palw_model_registry_v1.rs`、`consensus/core/src/palw_panel_v2.rs`、`consensus/core/src/palw_econ_contracts_v1.rs`、`consensus/core/src/palw_econ_contracts_fold_v1.rs`、`consensus/core/src/palw_bond_budget_fold_v1.rs`、`consensus/core/src/palw_state_v2.rs`、`consensus/core/src/palw_state_v2/transition_builder.rs`、`consensus/core/src/palw_kernel_route_fold_v1.rs`、`consensus/core/src/palw_kernel_settlement_v1.rs`、`consensus/src/pipeline/virtual_processor/processor/palw.rs`
