# RFC-0018 — DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。
> **暗号学的な計算証明を使わない — [PRINCIPLES §8](../PRINCIPLES.md)(2026-10-15 ユーザー決定、恒久):** 本RFCはZK・succinct argument・validity proof 等を、必須条件・代替経路・将来・研究のいずれとしても挙げない。使う暗号はハッシュ、署名、Merkle commitment/opening、乱数(beacon)だけであり、計算の正しさは決定論的再計算・サンプリング再実行・代数的スポットチェック・抽選検証者の承認(明示的前提の下)で扱う。
> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)のbond当たりQ/B/R/F上限と共通DAA拘束を適用する。[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)のモデル入手不介入は維持するが、§7.9のとおり **合意creditについては改定** する(ユーザー方針 2026-10-15)。

**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

* **Status:** Draft — 2026-10-15。レビュー検証(§1)、設計(§4–§9)、休眠fenceの下の実装(**implemented, dormant, unverified**、§13)。build/test/実node試験は未実施で、どの項目もPASSではない。
* **Decision:**
  1. 同じ親から同時に生成された異なるモデルのsibling blockは全て保持・mergeされ、各claimはBLUE/REDに関わらず独立に記録・検証・帰属される。共有上限で競合するときの採否は、mergeset集合の関数(色・merge順に依存しない)とする(§4)。
  2. Panel anchorのgenesis operator特権を除去し、bond済み・成熟済みの任意の採用attemptの集約seedで置き換える(§5)。
  3. **`Final`と`SecurityCertified`を別状態にする(Gate C、ユーザー方針で採用)。** 合意creditは`SecurityCertified`のclaimだけが生む。認証は「claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認」+「G14公開告発windowの異議なし経過」であり、**前提に基づく認証** である(§7)。
  4. DNS-BFTは廃止済み(RFC-0012)であり、本RFCは安全性にも収束にも使わない。DNS廃止後の収束はGate Cとrule E v2(資本加重・分割耐性・認証済みのみ)で行う(§6)。
  5. DAG幅(k/BPS/parents/mergeset)は休眠scheduleの仕様とstubだけを置き、値はユーザー判断とする(§8)。
* **Activation:** 5つの新fence(§13.1)。全presetで`None`、flag-dayリストに含めない、fingerprintはSome-only、`Some(never())`は`None`へ畳む、armすると`validate_palw_rfc0018_v1`が拒否する。A-2一様: fence未満では全readerが従来どおりに答え、Gate Cのfence未満で受理されたclaimは旧規則のまま完結する。**有効化は別途のユーザー判断。**
* **Baseline:** 検証の基準は`pre` = `5aa52b886`、live testnet-12 = `rcore/int-12` `0b1c11b87`、実装は`integ/rfc02-rfc06-12-g14` `6b1a01acd`から分岐した`integ/rfc0018-dag`。本文のcode行番号は`integ/rfc0018-dag`のもの(§1の検証表はHEAD `6b1a01acd`時点の行)。
* **Not activated by:** 本RFCのmerge、設計値表(§11)、単体試験sourceの存在、driverの出力。
* **Normative language:** MUST / MUST NOTは各fence有効化後の規則。設計値は名前付きの未承認値であり本番承認ではない。

## ユーザー方針 2026-10-15(本RFCを拘束する)

1. 「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。
2. 認証の柱は抽選検証者の承認とする。これは前提として置く(「正直な抽選verifier stakeの割合が閾値以上」)。暗号学的な計算証明を必須条件に加えない。f = 1/3、f = 0.6、最悪ケースの評価は前提が崩れた場合の影響の報告であり、設計を変える条件ではない。G14の公開告発で反証されたclaimは認証済みでもvoidされ、合意creditは取り消される。
3. MISAKAは暗号学的な計算証明を一切使用しない(PRINCIPLES §8、恒久)。

## 0. 要約と目標状態

**目標状態:** 「多数のモデルごとにminerとPanelが独立して同時稼働し、それぞれの計算がDAGに取り込まれ、特定のPoS側の生成者選出や確定制御に妨げられず、全体として一つの合意に収束する」。

DAG(GHOSTDAG、複数親、REDの取引受理)は作り直さない。現状は (a) 複数親・複数tip・RED受理がすでに機能し、(b) PALWの採否は単一proposer選出ではなく計算由来のticketとbond予算で決まり、(c) RED merged workもPALWでは数える。残る障害は四つである。

| 障害 | 現状 | 本RFC |
| --- | --- | --- |
| sibling間の順序依存 | 共有上限(DAA当たり発行予算、network room、per-bond)で競合したとき、consensus order(blue workとhash)の先頭が勝つ。ADR-0170のseed anchorは「最後に採用されたattempt」で決まる | canonical sibling order(モデル間round robin、§4) |
| Panel anchorの特権 | testnet-12はDAA 750からgenesisの8 operator bondのattemptだけがanchor(lane A)。headerだけで判定し、merged workで拒否されたattemptもanchorになり得る | 許可不要の集約anchor(§5) |
| 未認証claimの合意credit | `Final`のclaimはsourceを問わずfrontierを進め、settlementはattemptの`Final`を数え、rule Eはvoidされたattemptも数える。閉鎖モデルの自作自演でもfinalityが進む。testnet-12の現在の保護はDNS-BFT gateに依存しており、DNS-BFTは廃止済み | Gate C: `SecurityCertified`だけがcreditを生む(§7)、rule E v2(§6) |
| DAG幅 | 120 s / k = 1 / parents 10 / mergeset 180。長時間推論(p50 340 s)はk = 1でほぼ全てRED | 休眠schedule仕様とstub(§8) |

## 1. レビュー主張の検証(STEP 1)

判定: **CONFIRMED** / **PARTLY** / **WRONG**。行は`6b1a01acd`。liveは`0b1c11b87`、preは`5aa52b886`。

| # | 主張 | 判定 | 根拠(file:line) | live t12 |
| --- | --- | --- | --- | --- |
| a | `ghostdag(parents)`は複数親を扱う | CONFIRMED | `consensus/src/processes/ghostdag/protocol.rs:303`(selected parent `:244-271`、mergeset順`:317`、k-cluster着色`:334-364`、blue score/work `:366-386`)。round blockは常にRED(`:335`)、ADR-0105のheartbeat透過(`:112-147`) | live(同一。差分はtest整形のみ) |
| b | `pick_virtual_parents`は複数tipを選びtemplateへ渡す | CONFIRMED | `processor.rs:5033-5139`(候補`max_block_parents×3 = 30`、`:4794-4798`、重い半分5+無作為、`:5055-5073`、parents ≤ 10かつmergeset ≤ 180、`:5105`)。template `processor/template.rs:853`。PALW固有の除外: heartbeat幅 ≤ 4(`:5085-5120`)、round lane枠(`:5102`)、merge depth(`:5222-5240`)、lead cap(`processor/palw.rs:10212`)。bond・attemptの性質による除外はない | live(`processor.rs`内の同等code) |
| c | `palw_v2_merged_works`はBLUE/REDのPALW workを集めて検証する。REDはPALWで数えGHOSTDAG blue workには足さない。重複・予算超過は拒否 | PARTLY | 収集は`processor/palw.rs:9197-9251`、BLUE/REDを分けず`acceptance_ordered_mergeset_without_selected_parent`の一列(`:9215`、blue work昇順、DAA 750以降はparents-first)。blue workは`mergeset_blues`だけ(`protocol.rs:368-384`)— CONFIRMED。重複はfoldで拒否(`palw_state_v2.rs` `DuplicateClaim`/`DuplicateExecution`/`DuplicateWork`)、予算超過は**skip**(block不適格ではない、step 4b)。**PALWにRED専用の期限はない**(GHOSTDAGのmerge depthとmergeset上限だけ)。REDの報酬は自分のminerへ(`coinbase.rs:313-351`) | live |
| d | miner資格は計算由来ticket(`class_ticket_v3`)+bond/予算で、単一PoS proposer選出ではない。Panel選出`select_job_panel_v3`は別で、classを検証できるbondから抽選 | PARTLY | ticket: `palw_attempt_v2.rs:581-588`(execution commitmentのhash、`ticket ≤ target`、`palw_admission_v2.rs:925-934`)— CONFIRMED。**`select_job_panel_v3`(`palw_job_panel.rs:137`)にはlive呼出元がない**。liveのPanelは`derive_panel_v2_with_policy_judged_v1`(`palw_panel_v2.rs:1415`)、binder `processor/palw.rs:9472-9517`/`9574-9707` | ticket: live。`select_job_panel_v3`: 未使用 |
| e | DNS stake reorg gateが存在し最初に問われる(`dns_reorg_outcome`) | CONFIRMED(補足付き) | `processor/dns.rs:132`。呼出: `processor.rs:4833`(dead-branch escape)、`:4931`(sink search)、`palw_rule_e.rs:323`。順序: heapはblue work順(`:4848-4851`)→finality→UTXO検証→`dns_reorg_outcome`。内部ではDNS未廃止なら**BFT gate `dns_bft_gate_refusal`が最初**(`dns.rs:150`)、V2の非延長はstrict economic win(`:229-237`)。旧work×stake規則はV2では延長にしか届かない | live(`dns_bft_gate`はDAA 0、strict-winはDAA 750) |
| f | testnet-12ではPanel bindにgenesis 8 operator bondの特権anchorが関与し、RED merged blockで届いても認識される | CONFIRMED | `palw_operator_anchor_v1.rs:102-104`(全genesis bond)、`processor/palw.rs:6543-6562`(`blues ∪ genuine_reds`、selected parent除く)、seedはDAGで最古のoperator attempt(`:6576-6595`)。**追加所見F-6:** `operator_of_v1`(`:169-176`)はheaderだけを見るので、merged workでskipされたoperator attemptもanchorになり得る | live(DAA 750〜、live版は`mergeset_reds`にround blockを含む) |
| g | partition再合流が収束しないことがある。rule E(ADR-0178)とDNS退役(RFC-0012)は実装済み・休眠 | CONFIRMED | rule E `palw_fork_choice_rule_e_v1`は全preset `None`、`PALW_FORK_CHOICE_RULE_E_ARMABLE_V1 = false`(`:99`)。DNS退役`palw_dns_retirement`は全preset `None`かつrule Eなしでは拒否(`:121-129`)。live `0b1c11b87`には両moduleとも存在しない | live: 未実装。HEAD: 休眠 |
| h | 120 s、k = 1、parents 10、mergeset 180 | PARTLY | `params.rs:228-229`(`new_two_minute_bps` = `new_seconds_per_block(120, 1)`)、`:243`(180)、`:254`(120,000 ms)、`:258`(10)。**finality depthは式の360ではなく`window_challenge/2` = 600、pruning depthは74,920**(`with_palw_v2_depths`、`params.rs:4197-4201`)。merge depth 30 block、coinbase maturity 1、DAA窓264。k/BPSを高さで変える仕組みはない(`ForkedParam`はcoinbaseのbps履歴だけ、`:5675-5689`) | live |
| i | `palw_mergeset_view.rs`に同一親sibling BLUE/REDの試験がある | PARTLY | `consensus/src/pipeline/virtual_processor/tests/palw_mergeset_view.rs:9`の**1件だけ**(3 siblingで真のREDとmerged BLUEを作り、round blockが会計を変えないことを確認)。複数モデル・claim帰属は試験していない。**live `0b1c11b87`には存在しない**(`acb7dd644`で追加) | pre/HEAD |

### 1.1 レビューが扱っていなかった点

| 項目 | 所見 |
| --- | --- |
| 多モデルのticketと難易度(共通W) | `palw_work_target`以降、class targetは`MAX·min(1, CCU_m/W)`(`palw_work_target_v1.rs:116-125`)。Wはnetwork共通でepochごとに`model_blocks/expected_blocks`で再調整(`:98-111`)。single lotteryではclass ticketがlottery全体で、attempt blockのblue workはモデルに依らず一定(`protocol.rs:374-382`)。**モデルAのminerはモデルBとticketを競わない**。相互作用は共通Wの再調整、k = 1の着色、共有上限、ADR-0165の`FloorNotIdle`だけ |
| 異なるモデルの同時siblingの採否 | claimは`attempt_id_v2`と`execution_key`で鍵付けされ、モデルが違えば衝突しない。round・parent単位の単一性制約はない。帰属: bondはattemptの`executor_bond`、escrowは`carrying_block`、Panelは`claim_id`(`palw.rs:9502`)。**順序依存はstep 4bの共有上限だけ**: DAA当たり16 carve(`EmissionBudgetExhausted`)、network room、同一bondのissuance/exposure。負けたsiblingのcarveはcoinbaseで差し引かれた後、escrowされずに焼却される(`palw_v2_merged_escrow_withheld`の超集合性) |
| ADR-0170のseed anchor窓とRED | DAA 5,300以降(live)、採用されたmerged attemptはBLUE/REDを問わずseed anchorを記録(`palw_state_v2.rs:14543-14555`)。slotはlast-write-winsなので**merging blockのconsensus orderがseedを決める**(module doc `palw_anchor_window_v1.rs:31-32`が認める)。4bでskipされたREDはanchorにならない。効くのは次のspanから(`palw_anchor_window_v1.rs:83-85`) |
| partition再合流(経済比較器 vs blue work) | 比較器`compare_palw_candidates_v1`(`palw_fork_choice.rs:72-78`)のkeyは(safe frontier, safe weight, live total, hash)。strict-winで2 DAAより深いtieは現職維持(`palw_fork_authority_v2.rs:221`)。heartbeatだけの分割では両側0で恒久分裂し、finality depth(600 blue score)で封印される(record: 約4–6 h〜7–10 h)。ADR-0178 §5も同じ |
| IBD candidates RwLock再入deadlock | `protocol/flows/src/flow_context.rs:622`(parking_lotは非再入)。修正は`ibd/flow.rs:860`の単一read(`:898`)。commit `57235a5b3`(親`0b1c11b87`)と双子`537b8b000`は**HEADに含まれ、live `rcore/int-12`には含まれない**(hotfixは一括release待ち) |
| safe frontier(比較器の第1key) | `palw_state_v2.rs:14660-14688`(本branchでは`:14692-14712`): 最古の未解決claimより下で、**phaseが`Final`の最深claim**。weightを読まず、sourceも見ない。weight 0のFinal(uncertified-weightless、RFC-0017 probation、budgetで0に切られたclaim、free-promptのFinal)も進める。後退しない(`reverse_convicted_final`はweightだけ引く) |
| rule Eの二つの未解決問題(ADR-0178 §10/§11) | (1) participationは**資本ではなく異なるbondの数**(分割すれば増える)。(2) **voidされたattemptも数える**: `PalwRuleESideAccV1::push`がbondを集合に入れるのは`Voided`の判定より前(`palw_fork_choice_rule_e_v1.rs:463-476`、ADR-0178 §10「no void moves participation」)。 |
| testnet-12の現在の保護 | V2の深いreorgはBFT gate(DNS-BFT)とstrict-winに依存する。**DNS-BFTはRFC-0012で廃止済み**(物理除去はWIP、`origin/codex/rfc12-remove-dns-bft` `344a1b882`、pre-reorgで未build、本branchには取り込まない)。したがって閉鎖モデル攻撃は新規則の中で塞がなければならない(§7) |

## 2. 主要な所見

* **F-1(sibling順序依存):** 異なるモデルのsibling claimはいずれも受理され得るが、共有上限で競合するとGHOSTDAG順の先頭が勝ち、負けたsiblingのcarveは焼却される。決定的だがモデル間で公平ではなく、merge順の偶然が帰属を決める。
* **F-2(ADR-0170のlast-write-wins):** 同一blockの複数REDのうちseed anchorになるのはconsensus orderの最後。
* **F-3(PALWのRED期限なし):** REDのPALW workはGHOSTDAGのmerge depth(30 block ≈ 1 h)以内にmergeされれば数えられる。それより遅いREDは`ViolatingBoundedMergeDepth`で失われる。
* **F-4(未認証claimの合意credit):** `Final`はPanel licenceとDAA sweepだけで到達し、safe frontier・safe weight・settlement depth・settled anchor・rule E participationを進める。Panelを自分で満たせる閉鎖モデルでは、これが外部の誰にも検査されないまま合意creditになる。
* **F-5(frontierとsettlementの不一致):** frontierは`Final`のclaimをsourceを問わず採り、settlementはattemptの`Final`だけを数える。free-promptの`Final`だけが存在すると、frontierは進むのにsettlement depthは0になる。
* **F-6(operator anchorのheader判定):** merged workで拒否されたoperator attemptもanchorになり得る。
* **F-7(rule Eの分割とvoid):** §1.1のとおり。
* **F-8(DNS-BFT依存):** live testnet-12の深いreorg保護は廃止済みの機構に依存している。
* **F-9(長時間推論とk = 1):** 推論p50 340 sではλ = 2·340/120 = 5.7となり、k = 1のRED率は推定97.7 %(§8)。live実測(2026-10-03)は72 attemptのうち71がRED。

## 3. liveと休眠の対応

| 機構 | live testnet-12(`0b1c11b87`) | pre / HEAD | 本RFC |
| --- | --- | --- | --- |
| GHOSTDAG・複数親・RED取引受理 | live | 同じ | 変更なし |
| merged work(BLUE/RED)のPALW受理 | live | 同じ | §4のcanonical順序(休眠) |
| lane A operator anchor | live(DAA 750) | 同じ | §5で置換(休眠) |
| ADR-0170 anchor窓 | live(DAA 5,300) | 同じ | §4でcanonical化(休眠) |
| DNS-BFT gate | live(DAA 0)| 同じ。RFC-0012で廃止済み、物理除去はWIP | 依存しない |
| strict economic win | live(DAA 750) | 同じ | Gate Cがkeyの中身を認証済みに限る(休眠) |
| rule E v1 | なし | 休眠・arm不可 | rule E v2(休眠) |
| DNS退役(RFC-0012) | なし | 休眠・rule Eなしでは拒否 | Gate C・rule E v2と組で収束(休眠) |
| `SecurityCertified` | なし | なし | §7(休眠) |
| IBD deadlock hotfix | なし | あり | — |

## 4. 設計変更1: 多モデル同時生成とRED取り込み(`palw_dag_sibling_attribution_v1`)

### 4.1 不変条件

* **I-S1(保持):** 有効なsibling blockは色に関わらず全て保持され、merge depth以内に後続blockのmergesetへ入る(GHOSTDAGの既存性質)。MUST NOT: PALWの規則でsiblingをparent候補から外す。
* **I-S2(独立記録):** 各claimは自分のbond・class・carrying blockで記録される(`attempt_id_v2`、`execution_key`、`accepted_block`)。Panelは`claim_id`で鍵付けされ、seedはclaim idを含む(`palw_panel_draw_seed_v1`)。siblingのPanel・escrow・報酬を混同しない(MUST)。
* **I-S3(順序非依存):** 一つのaccepting blockが採用するmerged attemptの集合は、mergesetの**集合**の関数でなければならない(MUST)。色・blue work・merge順・到着順に依存しない。
* **I-S4(モデル間公平):** 共有上限で競合したとき、各モデルのsiblingが1件ずつ採用されてから、どのモデルの2件目も採用される(round robin)。
* **I-S5(seed anchor):** ADR-0170のseed anchorは、同じspanで採用されたattempt(自身・merged、BLUE/RED)のうち`H(domain ‖ execution key)`が最小のもの。last-write-winsではない(`record_round_seed_anchor_of`、`transition_builder.rs`)。
* **I-S6(selected parentのsibling):** siblingのうちGHOSTDAGがselected parentに選んだものは、自分のblockの遷移で先に採用される(merger のmergesetに入らない)。どれが選ばれるかはGHOSTDAGの決定的な規則(blue work、次にblock hash)で決まり、到着順にもmerge順にも依存しない。block hashを動かすには勝ちticket(推論1回)がもう一度要る。共有上限で負けるのは、このselected parentを除いたmergeset内のsiblingで、それはcanonical順で決まる。

### 4.2 規則(fence以後、accepting blockのDAAで判定)

step 4bの前に、mergesetのattempt workだけを並べ替える。非attempt work(receipt spend等)は元の位置に残す。順序は、モデル(class)ごとに`H(domain ‖ execution key)`の昇順、モデル同士は各モデル先頭の優先度順、1巡につき1モデル1件のround robin(`palw_sibling_canonical_order_v1`、`consensus/core/src/palw_rfc0018_v1.rs:283`、fold `palw_state_v2.rs:14481-14496`)。優先度はsibling自身の推論で決まり、accepting minerは動かせない。siblingのminerが動かすには別の勝ちticket(推論1回)が要る。

### 4.3 mergeset上限とREDの期限

* mergeset 180、parents 10、heartbeat ≤ 4/mergesetは変えない。round robinはこの上限の内側で働く。
* REDのPALW workの期限はGHOSTDAGのmerge depth(testnet-12で30 block)である。これを超えるREDは`ViolatingBoundedMergeDepth`で失われる。推論時間 + 伝播はmerge depthの時間(約60分)より十分短くなければならない(SHOULD、§8で計測)。
* coinbaseのcarve差し引きは採用集合の超集合のまま(`palw_v2_merged_escrow_withheld`)。負けたsiblingのcarveは焼却。本RFCは「どれが負けるか」をmerge順の偶然からround robinへ変えるだけで、焼却の規則は変えない。

### 4.4 計測と受入試験

| ID | 試験 | 合格条件(記録する値) | 実装 |
| --- | --- | --- | --- |
| T-1a | 3モデル・3 miner・別Panel、同じ親からsibling(in-process、k = 1) | 3 blockともmergerの過去にある(selected parent/BLUE/RED)。3 claimとも自分のbond・class・carrying blockで記録 | `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs` |
| T-1b | 同じblock群を別nodeへ、3 siblingの到着順を全6通り | sink、selected parent、blues、reds、PALW state root(採用claim・消費した上限・escrow)、safe frontier、ADR-0170 seed anchorが一致 | 同上 |
| T-1c | sibling fence armで同じ2試験 | 同上 | 同上 |
| T-1d | canonical順は集合の関数、round robin、非attempt slot不変 | 回転・逆順で同じ列。3モデルの先頭3件は3モデル | `palw_rfc0018_v1/tests.rs` |
| T-1e | 実process: 3 producer nodeを分離して同じbaseから3モデルを同時採掘、再接続 | 各siblingの色、claim記録、帰属、Panelがclaimごとに別 | `scripts/drills/rfc0018_dag_drill.py siblings` |

## 5. 設計変更2: Panel anchorの特権除去(`palw_panel_anchor_permissionless_v1`)

### 5.1 候補の評価

| 候補 | 許可不要 | grinding耐性 | RED認識 | 評価 |
| --- | --- | --- | --- | --- |
| lane A(現状、operatorのattempt) | ✗(genesis 8 bond) | operatorを信頼 | ✓ | 置換対象 |
| ADR-0130 `round_seed_anchor`(execution laneの勝利推論) | ✓ | 1回の再抽選 = 1回の勝ち推論 | ADR-0170以降✓ | 単一sourceの最後の貢献者が選べる |
| ADR-0170窓(span単位の最新anchor) | ✓ | 同上 | ✓ | last-write-wins(F-2) |
| merged-attemptの集約(採用) | ✓ | 下の上界 | ✓ | 正直な貢献を1件でも含めば予測不能 |
| beacon(RFC07 Part VI/N3) | ✓ | 設計上最良 | — | bootstrapが未成立(N3)。成立後の置換先として残す |

### 5.2 規則(fence以後、anchor blockのDAAで判定。lane Aより優先)

* **貢献(確定したfold出力):** anchor block Xのselected parentの状態にある**attempt claim**(foldが採用したもの)で、Xより下のchain blockが受理したもの。carrying blockはBLUEでもREDでもよい。phaseに依らない(採用されたものは後でvoidされても採用された)。認識は採用の1 chain block後になる(`palw_permissionless_contributions_from_state_v1`、`palw_rfc0018_v1.rs`)。headerだけのoperator判定(F-6)はなくなる。
* **bondごとに1件:** 各bondの最早`(DAA, block)`の貢献だけを使う。同じbondの後の勝ちは再抽選にもweightにもならない。
* **集約seedとproducer除外:** slot以降の、**claimのproducer以外のbond**の貢献をbond順に並べ、`H(domain ‖ slot ‖ Σ(bond, execution))`(`palw_permissionless_anchor_seed_v1`)。そのようなbondが1つもなければ、Xはそのclaimをanchorしない(`palw_permissionless_anchors_claim_v1`)。この述語を、processorのwalk(`palw_v2_anchor_fact_of_candidate_for_v1`)、binding導出、foldのstep 4c(`palw_void_claims_unbound_in_their_anchor_block_v1`)の三者が同じ確定集合に対して問う。anchorされないclaimはXでbindもvoidもされず、自分のanchorを待つ。
* **到達範囲:** 最新の貢献のDAA(anchor自身のDAAで上限)。draw時刻は最早の貢献のDAA(lane Aと同じ扱い)。
* **旧claim:** anchor blockのDAAがfence未満なら従来(lane AまたはF1)。

### 5.3 grinding上界

攻撃者が窓内の`b`個のbondを支配し、bond `i`が窓内に`q_i`件の採用attemptを持てるとき、攻撃者が選べるseedの数は高々`Π(1 + q_i)`(各bondで「出さない」か自分の勝ちのどれか)。正直な貢献の実行commitmentは公開前に予測できず、hashの内側にある。`q_i`はADR-0176のbond当たりclaim予算で上限があり、1件ごとに勝ち推論1回の費用がかかる。Panel捕獲確率はこの因数倍を超えない(報告値、§10 T-2b)。

### 5.4 未解決

* GAP-A1(header近似)とGAP-A2(2 bond最小での代替)は2026-10-15に解消した(確定fold出力からの貢献、claimごとのproducer除外)。残るのは、認識が採用より1 chain block遅れることだけである。

## 6. 設計変更3: DNS廃止後の収束 — rule E v2(`palw_rule_e_participation_v2`)

### 6.1 規則(fence以後、現職のDAAで判定。rule E v1を同じ高さ以下で要する)

* **participation v2** = 共通過去`F`の登録で、排他過去に**voidされておらずGate Cで認証済み**のattemptを1件以上持つ異なるbondの、`F`での担保の合計(`PalwRuleEParticipationV2Acc`、`palw_rfc0018_v1.rs:219`、accumulator `palw_fork_choice_rule_e_v1.rs:520`)。
* **順序の修正:** bondは、Voided/未認証の判定を通った後でだけparticipation集合に入る(v2)。Gate Cの下では、未認証のattemptはrecord段階で`attempt = false`になるので、v1の集合にも入らない(`palw_fork_choice_rule_e_v1.rs:357-383`)。
* **even split v2** = 共通資本の1/3(切上げ、最小1)。
* **順序:** participationが数える間(`W_p`以上)は資本、その後に経済key(frontier, safe, live)。それぞれ認証済みのみ(Gate C)。
* **creditなし同士(§6.4):** 両側とも認証済みcreditが一切ない(資本0、frontier 0、safe 0、live 0)ならGHOSTDAG順で決める(`palw_rule_e_decide_v2`、`:804`、processor `palw_rule_e.rs:218-226`)。IBDはtieで現職を保つ(従来どおり)。

### 6.2 性質

* **分割耐性:** 資本に線形で、bond当たりの上限も下限もないので、資本を`n`個のbondへ分けても合計は分けない場合を超えない(全片が認証済みattemptを持つとき等しい)。23,076分割の検算はT-0。
* **bond式v2との整合:** creditは`F`での担保`C`で、bond式v2の`u·C`と予算engineの`F_max(C, W)`が読むのと同じ`C`。予約や`F`以後の入金は数えない。
* **void・未認証の除外:** voidされた(G14でconvictされたものを含む)attemptと未認証のattemptは数えない。
* **leaf v2:** clientのleafは資本を運ばない(GAP-R4)。v2を宣言していないnetworkではnodeも資本を0として扱い、clientのpairと一致する。

### 6.3 Gate Cとの組

DNS退役後の収束は**Gate Cとrule E v2の組**で行う。fork choiceの全key(participation、frontier、safe、live)は認証済みclaimだけから作られ、未認証claimは誰に対してもcreditを持たない。

### 6.4 creditなし区間の扱い

両側がcreditを持たない区間(heartbeatだけ、または未認証claimだけ)は、PALWのfinalityが一切報告されていない区間である(§7.8)。GHOSTDAG順で決めても、報告済みのfinalityは覆らない。これでheartbeatだけの分割(ADR-0178 §5)も収束する。finality depthを超えた分割は従来どおり封印される。

### 6.5 受入試験

| ID | 試験 | 合格条件 | 実装 |
| --- | --- | --- | --- |
| T-3a | v2のorderはv2 fence未満でv1と同一、反対称 | 全組合せ | `palw_rfc0018_v1/tests.rs` |
| T-3b | 分割中立・bondは1回・void/未認証/非共通は0 | 資本の等式 | 同上 |
| T-3c | 実process: partition、再接続、新規IBD node。linkごとに配送遅延を変え到着順を変える | sink、sinkのmerge読み(selected parent/blues/reds)、採用claim集合、PALW経済状態、fork choiceの立場が全nodeで一致 | `rfc0018_dag_drill.py partition` |
| T-3d | DNS退役+rule E v2+Gate Cのlocal calibration devnetで同じ | 同上+未認証claimのcredit 0 | `rfc0018_dag_drill.py retired` |

## 7. Gate C — `SecurityCertified`(`palw_independently_work_certified_v1`、ユーザー方針により採用)

### 7.1 二つの状態

| 状態 | 意味 | 何が起きるか |
| --- | --- | --- |
| `Final` | claimの手続きが完了 | 利用者払いescrowの経済的settlementは既存規則どおり |
| `SecurityCertified` | 合意creditを持ってよい | safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来(coinbase)報酬legに寄与 |

不変条件(fence以後、claimの受理DAAで判定):

* **I-C1:** `SecurityCertified`でないclaimは、上の7つのどれにも寄与しない(MUST NOT)。
* **I-C2:** `Final`、Panelのlicence、Panelの過半、モデル保有者の集合、登録者が満たせる集合のいずれも、単独で認証にならない(MUST NOT)。
* **I-C3:** 認証済みclaimが後でG14の公開告発で反証されたら、voidされ、合意creditは取り消される(MUST)。safe weightは`reverse_convicted_final`で引かれ、participation・settlement depthから消える。
* **I-C4:** DNS-BFTおよびDNS finality anchorに安全性・収束を依存しない(MUST NOT)。
* **I-C5:** 何も認証されない間も、DAG、通常取引、heartbeat/BASE-0の時計、DAAは進む。止まるのはfinality creditだけ(MUST)。

### 7.2 認証述語(前提に基づく認証)

1. **commit:** 支配されたclaimの`Final`で、claim・producer・母集団の切断点を固定する(`commit_at_final_v1`、`consensus/core/src/palw_security_certification_fold_v1.rs:45`)。
2. **draw:** commitより厳密に後のexecution laneのseed anchor(ADR-0130)から作るseedで、**global pool**(Active、commitの`verifier_maturity`前に登録、最小担保以上、producerのoperatorを除く)から`m`位置を担保比例・復元抽選する(`palw_certification_pool_admits_v1`、`palw_security_certification_v1.rs:280`、RFC-0017/R7-ECONの抽選を再利用)。**モデル保有の有無で母集団を絞らない**。
3. **attest:** 抽選されたbondは期限内に、そのclaimを再実行して判定を署名する(RFC-0017のinner 75を、exam idの代わりにcertification idで使う、`try_attest_claim_certification_v1`、fold `:111`)。
4. **decide:** `k`位置以上が`Reproduced`(mismatchなし)で、claimがまだ`Final`、未解決の告発がなく、**G14公開告発window**(`prosecution_window_daa`)が`Final`から経過したら`Certified`(`palw_certification_verdict_v1`、`:320`)。期限までに`k`未満なら`NotCertified`、母集団不足は`NoPopulation`、`certify_by`までに抽選できなければ`Expired`、`Final`でなくなれば`Withdrawn`。**1 claimにつき1回、再試行なし、classごとに数える**(attemptの有界化と計数)。network全体のepoch当たり抽選数にも上限がある。
5. **公開check経路:** 決定論的な公開検査(G14完了classの公開material)が固定されたclassは、告発window経過で`PublicCheckCertified`。このbinaryでは固定classは0件(`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1 = &[]`)。

**前提(明示):** 認証は抽選検証者の署名付き承認に依る。**「抽選されたverifier stakeのうち正直な割合が閾値(`k/m`)以上である」ことを前提とする。** 署名は再計算の証明ではない。これは無条件の数学的証明ではなく、前提に基づく認証である。RPCはこれを`securityCertificationPremise`として返す(§7.8)。

閉鎖モデルは、正直な抽選検証者が入手も再実行もできないので、正直な位置から肯定承認を得られない。攻撃者が`m`位置中`k`以上を引いたときだけ認証される。

### 7.3 前提が崩れた場合の影響(報告値、設計の変更条件ではない)

攻撃者のverifier stake比`f`(抽選される母集団に占める割合)ごとの、閉鎖モデルの1 claimが認証される確率`P(X ≥ k)`、`X ~ Bin(m, f)`(`palw_certification_false_pass_f64`):

| `f` | 7-of-5 | 47-of-32(設計値) | 23,076 claimでの期待認証数(47/32) |
| --- | --- | --- | --- |
| 0.03 | 4.85e-7 | 8.9e-38 | ≈ 0 |
| 0.25 | 1.29e-2 | 6.4e-10 | 1.5e-5 |
| 1/3 | 4.53e-2 | 1.19e-6 | 0.027 |
| 0.6 | 0.420 | 0.163 | ≈ 3,762 |
| 1(最悪) | 1 | 1 | 23,076 |

* 7-of-5では`f = 1/3`で23,076 claimのうち約1,045件が認証される。claim数に比例して前提失敗の影響が積み上がるので、設計値は`m = 47, k = 32`とした(未承認、U-5)。費用は認証されるclaimごとに`m`回の再実行である。
* `f = 0.6`や最悪ケースでは、本設計は合意creditを守れない。これは前提の外側であり、設計を変える条件ではない(ユーザー方針2)。
* veto(否定`v`位置で拒否)は追加案U-6であり、採用設計ではない(設計値`v = 0`)。

### 7.4 合意creditの加算点(一箇所)

認証が決まったとき、sweep(`sweep_security_certification_v1`、fold `:234`、kernel routeの終了tick `palw_kernel_route_fold_v1.rs:1671`)が、再導出と同じ式でcreditを加える(`credit_certified_v1`、`:196`): attemptはFinalのsafe weight(canonical基準、F-Wの上限、予算の付与)とsettled anchor 1件、free-promptはそれまでのspendのweight。発行由来の報酬legは`Final`で保留され(`hold_issuance_reward_at_final_v1`、`:91`)、認証時に支払われ、失敗時は名付けられない(焼却、新規発行なし)。

### 7.5 frontierとsettlementの単一規則

fence以後、frontierとsettlement depthの両方で、合意creditを生むのは**認証済みattemptの`Final`だけ**とする(`palw_consensus_credit_anchor_v1`、`palw_security_certification_v1.rs:367`)。free-promptの合意creditは認証後のspendのweightだけで、frontierもsettlement depthも動かさない。「free-promptの`Final`だけが存在する」場合は、frontierなし・depth 0(F-5の解消、T-0c)。fence未満は従来(frontierは任意source、settlementはattempt)。

### 7.5a frontierの巻き戻し(conviction後の再計算)

fence以後、safe frontierは毎block**再計算**する: 解決済みprefix内の、frontier資格のある`Final`の最深claim(認証済みattempt。fence未満で受理されたclaimは従来規則)、ただし**floor**を下回らない(`palw_gate_c_frontier_v1`)。

* **floor:** fenceが効いた最初のblockで、そのときのfrontierで初期化する(有効化でfrontierが戻らない)。資格のある`Final`のclaimが**退役**したとき、その位置まで上げる。退役したclaimはもうconvictされない(`reverse_convicted_final`は退役済みを扱わない)。kernel-route aux 136の`'F'` row(`note_retirement_frontier_floor_v1`、`ensure_frontier_floor_v1`)。
* **巻き戻し:** 認証済みclaimが後でconvict(void)されると、そのclaimはfrontierでなくなり、frontierは残る資格claimの最深、またはfloorへ戻る。deltaの`Frontier`エントリが任意の移動を記録するので、reorgは正確に戻し、IBDはcarriageでfloor rowを受け取る。
* **pruning:** pruning上限は、進行の上限(§7.7)と**巻き戻し上限**(convictionで再びfrontierになり得る最も浅い位置 = stateに残る資格`Final`の最小受理blue score、`palw_frontier_rollback_bound_v1`)の小さい方を超えない。再びfrontierになり得る点をpruneすることはない。資格claimは退役で必ずstateから消えるので、上限は有界に進む。

### 7.6 寿命

認証rowはclaimの退役まで残り、退役後にsweepで消える。`claim_retirement_daa ≥ lifetime_daa()`(`certify_by + attest window`と告発windowの大きい方)でなければならない(MUST、activation checklist)。満たさないと`Final`のclaimが未認証のまま退役する(creditなしで整合はするが、正直なclaimも認証されない)。

### 7.7 pruning progress

fence以後、frontierは認証済みclaimでしか動かないので、pruningはfrontierを待たない。上限は、最古の未解決claimと、認証が未決の最古の`Final` claimの、受理blue scoreの小さい方の1つ下(`palw_pruning_progress_ceiling_v1`、`:379`、processor `palw.rs:719-750`)。open claim・court・未決の認証が読む履歴は消さない(MUST)。未決の認証は`lifetime_daa`で必ず決まるので、pruningの停止は有界である。

### 7.8 settlementの意味とRPC

DNS-BFTがないので、settlementの`safe`/`settled`はPALWの認証済みcreditだけを意味する。未認証claimだけのchainは**PALW settlement finalityなし**(`settled = false`、`depth = 0`)と報告し、`uncertifiedFinalAnchors`(認証されていない`Final` attemptの数)と`securityCertificationPremise`(前提の文言)を返す(`GetPalwSettlementResponse`、wire version 4、gRPC field 13/14)。claimごとの認証状態は`GetPalwClaims`のrow(`securityCertification`、wire version 6、gRPC field 39)で返す。

**leaf-v2 client経路:** rule Eのrecord(`PalwRuleEClaimRecordV1`)は真のphaseと**認証状態**(`certification`: NotGoverned / Certified / Uncertified)を持ち、leaf-v2 windowにcommitされる。Gate Cの適用(未認証はparticipationなし・frontierなし、FPの`Final`はfrontierなし)はnodeとclientが共有するaccumulatorで行う。clientは`palw_rule_e_certification_summary_v1`で、未認証`Final`の数と前提の文言を得る(`palw_fork_choice_rule_e_leaf_v2.rs`)。資本(rule E v2のparticipation)はまだleafに載らない(GAP-R4)。RFC-0012のnative settlementも、fence以後は`Final`遷移ではなく認証遷移を成熟workの証拠とする(`palw_native_settlement_v1.rs:161`、`:338`、`:404`)。

### 7.9 ADR-0177の変更(明示)

ADR-0177は「モデルの入手不能だけでは報酬もweightも止めない」とした。**合意creditについてこれを変更する:** モデルを入手できない検証者は再実行できないので、そのモデルのclaimは認証されず、合意creditを持たない。すでに認証されたworkの報酬は取り消さない(告発による反証を除く)。入手不能を理由にslashしない点は変わらない。

### 7.10 genesisモデルと70B

genesis登録は認める。**genesisモデルにも認証の免除はない**。70Bモデルは、独立した検証(抽選検証者の再実行)が成立するまで合意creditを持たない。

### 7.11 受入試験 — 必須の最上位試験「300M閉鎖モデル/自前Panel攻撃」(T-0)

**合格条件: 不正な合意finalityが一切ない。** 6つの量を**別々に**報告する。

| ID | ケース | safe frontier移動 | safe weight | settlement depth | rule E participation比 | settled anchor | 報酬流出 | 実装 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| T-0a | 300Mを13,000 BILIの最小bond 23,076個に分割、閉鎖モデル、自前Panel、未決/veto/expire | 0 | 0(FP分を除き) | 0 | 0 | 0 | 0 | `consensus/core/src/palw_rfc0018_v1/attack_tests.rs` |
| T-0b | 1/7のattemptをvoid | 同上 | 同上 | 同上 | 0(順序修正) | 同上 | 同上 | 同上 |
| T-0c | free-promptの`Final`だけ(認証済みでも) | 0 | spend分のみ | 0 | — | 0 | — | 同上、`tests.rs` |
| T-0d | reorg(両方向)とIBDのclaim-set経路 | 正直側が現職でも挑戦者でも勝つ | | | | | | `attack_tests.rs` |
| T-0e | 分割の中立性(仮に認証されても23,076片 = 1 bond) | | | | 等しい | | | 同上 |
| T-0f | 実process: 定常、reorg(分離と再接続)、restart、pruning progress、新規IBD | 各phaseで上の6量が0、premiseがRPCに出る、pruning pointが進む | | | | | | `scripts/drills/rfc0018_dag_drill.py attack` |
| T-0g | 前提失敗時の影響(f = 1/3、0.6、1)を報告 | 報告のみ | | | | | | `attack_tests.rs` |

その他のGate C試験: identity(fence未満・出荷network)、`Final ≠ SecurityCertified`、Panel licenceは認証ではない、失敗outcomeは全て0、voidは常に未認証、provisionalは0で一定、verdict(m-of-k、告発window、未解決告発、期限、expire、withdraw)、母集団がglobalでproducerを除く(`palw_rfc0018_v1/tests.rs`、`palw_security_certification_v1.rs`内の単体試験)。

## 8. DAG幅(`palw_dag_width_schedule_v1`、仕様とstub)

### 8.1 現状

120 s、k = 1、parents 10、mergeset 180、merge depth 30 block、finality 600、heartbeat ≤ 4/mergeset。heartbeatはADR-0105で透過(bonded blockの着色を邪魔しない)。

### 8.2 RED率の推定(Poisson、λ = 2·d/T、`palw_dag_width_red_permille_v1`)

| 実効遅延 d | k = 1 | k = 2 | k = 3 | k = 4 | k = 8 |
| --- | --- | --- | --- | --- | --- |
| 2 s(伝播) | 0.1 % | 0.0 % | 0.0 % | 0.0 % | 0.0 % |
| 30 s | 9.0 % | 1.4 % | 0.2 % | 0.0 % | 0.0 % |
| 120 s | 59.4 % | 32.3 % | 14.3 % | 5.3 % | 0.0 % |
| 340 s(8k p50) | 97.7 % | 92.1 % | 81.6 % | 66.8 % | 12.0 % |
| 418 s(8k p95) | 99.2 % | 97.0 % | 91.6 % | 82.4 % | 26.7 % |

T = 60 sでは340 sの推論はk = 8でも79.6 %がRED。推論が長いworkは、kを広げてもBLUEにはならない。

### 8.3 分析

* **PALWの取り込みはREDでも成立する**(§1 c)。RED率が問題になるのは (i) GHOSTDAGのblue workに入らないこと(PALWはblue workをfork choiceの主keyにしない)、(ii) merge depth内にmergeされる必要、(iii) ADR-0165のfloor状態機械がREDでNormalを延長しない点である。
* **throughput:** chain block 1つでmergeset 180、parents 10。多モデルの同時tipが10を超えると、複数blockにわたってmergeされる。merged attemptの採用は、DAA当たり16 carveの発行予算が先に効く(§4のround robinで公平化)。
* **安全余裕:** kを上げるとblue集合が広がり、私的に作ったblockもBLUEになりやすい。PALWではfork choiceの主keyは認証済みcredit(Gate C)なので、kの拡大がfinalityを直接与えることはない。それでもGHOSTDAGの順序の安全性(DAA、merge depth、pruning)はkとBPSに依存する。
* **heartbeat:** BPSを変えるとheartbeatの間隔、DAA窓、PALWの全DAA単位の窓(bind、receipt、challenge、court、retirement)が変わる。DAAで表した値の協調した再導出が要る。

### 8.4 stubの理由

GHOSTDAGの`k`、parentsの上限、mergesetの上限は、GHOSTDAG managerとheader pipelineのconstructorのscalarである。block時間はPALWの全DAA窓を伸縮させる。高さで切り替えるには、selected parentのDAAで鍵付けしたblock単位のreader(crescendoの`ForkedParam`の形)を、header処理、GHOSTDAG、difficulty、pruningの全経路に通す必要があり、すべてlive経路に触れる。本RFCはfence(`PalwDagWidthScheduleFenceV1`、`palw_rfc0018_v1.rs:439`)、構造検査、id(fingerprint)、RED推定関数だけを置き、readerは置かない。validationはarmを拒否する。

## 9. 他RFC・ADRとの関係

* **RFC-0016 V+:** モデルのblock権利の消費は、同じmergesetのsibling間で**merge順に依存してはならない**(MUST)。§4のcanonical順は、権利の消費をstep 4bの採用順と同じ集合関数にする。V+の権利は合意creditではなく生成機会であり、Gate Cとは独立(権利を消費しても、認証されるまでcreditはない)。
* **RFC-0017(U-2、P0):** `Probation`で受理されたclaim(probation pinを持つclaim)は、m-of-kの承認を得ても`SecurityCertified`にならない。この禁止は単一の述語`palw_independently_work_certified_v1`の中で`palw_eligibility_bars_security_credit_v1`を問うので、全てのGate C読み取り箇所(frontier、settlement depth、rule E participation、settled anchor、provisional/final weight、coinbase credit leg)が自動的に従う。pinは`Final`後も残り、claimがstateを離れるかvoidされるまで凍結される。certification sweepはそのclaimに検証者を抽選しない。Registered/Lapsedはclaim gateで拒否されるのでclaimが存在しない。モデルが`Eligible`になった後に受理されたclaimは通常どおり認証される(試験 `rfc18_u2_a_probation_claim_never_earns_security_credit_and_an_eligible_one_does`)。
* **RFC-0017:** 資格(Registered/Probation/Eligible/Lapsed)はモデル単位の生成・報酬の制限で、Gate Cはclaim単位の合意creditの制限。両方を満たしたものだけがcreditを持つ。Gate Cは同じ抽選・承認objectを再利用するので、`palw_model_eligibility_v1`がGate Cのfence以下で必要(validation)。
* **ADR-0176:** participation v2のcreditは担保`C`、予算engineの`F_max(C, W)`と同じ`C`。Gate Cは予算の予約を変えない(予約は受理時、creditは認証時)。
* **ADR-0178:** rule E v2が§10/§11の二つの未解決問題を閉じる。v1の意味は変えない。
* **RFC-0012:** DNS退役後のsettlementは認証遷移を証拠とする(§7.8)。

## 10. 受入試験の一覧

T-0(§7.11、必須の最上位)、T-1(§4.4)、T-2: permissionless anchor(seedはclaimのproducer以外のbondを要し、未採用を無視し、貢献は確定fold出力(REDのcarrierを含む)で、bondごとの最初の貢献の関数で、順序に依存しない — `palw_rfc0018_v1/tests.rs`。T-2b: grinding上界`Π(1+q_i)`の実測報告 — 未作成)、T-3(§6.5)、T-4: DAG幅推定の単調性とschedule構造検査、T-5: DNS退役calibration devnet(`retired`)。

## 11. 本RFCで決めない値

| 名前 | 設計値(未承認) | 意味 |
| --- | --- | --- |
| `PALW_CERT_M_DESIGN_V1` / `_K_` | 47 / 32 | 抽選位置数と必要肯定数 |
| `PALW_CERT_VETO_DESIGN_V1` | 0(なし) | veto案(U-6) |
| `PALW_CERT_ANCHOR_DELAY_SPANS_DESIGN_V1` | 1 | commit後のseed anchor遅延 |
| `PALW_CERT_ATTEST_WINDOW_DAYS_DESIGN_V1` | 2日 | 承認期限 |
| `PALW_CERT_PROSECUTION_WINDOW_DAYS_DESIGN_V1` | 7日 | G14公開告発window |
| `PALW_CERT_CERTIFY_BY_DAYS_DESIGN_V1` | 3日 | 抽選の期限 |
| `PALW_CERT_VERIFIER_MATURITY_DAYS_DESIGN_V1` | 7日 | 母集団の成熟 |
| `PALW_CERT_DRAWS_PER_EPOCH_DESIGN_V1` / `_EPOCH_DAYS_` | 4,096 / 1日 | network全体の抽選上限 |
| `PALW_RFC0018_ANCHOR_MIN_DISTINCT_BONDS_DESIGN_V1` | 2 | 集約anchorの最小bond数 |
| `PalwDagWidthScheduleV1` | なし(probeはk 3、120 s、16、360で構造試験用) | DAG幅 |
| `W_p`(rule E) | 20 DAA(現行) | participationが数え始める深さ |

## 12. ユーザー判断が必要な事項

* **U-1** 各fenceの有効化と順序(Gate C ≥ RFC-0017 eligibility・kernel route、rule E v2 ≥ rule E、DNS退役 ≥ rule E)。
* **U-2** 発行由来報酬legの保留を、producerへの1 legで支払うか(本実装)、Panel share・buyback・vestingへ分けるか(GAP-C4)。
* **U-3** free-promptの認証前spendのcoinbase carve(GAP-C3、本実装はweightだけ止める)。
* **U-4** DAG幅scheduleの値と、block単位readerを実装するか。
* **U-5** `m`/`k`と、認証1件あたり`m`回の再実行費用の負担。
* **U-6** veto案の採否。
* **U-7** creditなし同士のtieをGHOSTDAG順にする規則(§6.4)の承認。
* **U-8** 公開check経路に固定するclassと、その計測記録。

## 13. 実装(休眠、未build、未試験)

### 13.1 fenceと割り当て

| 種類 | 割り当て | 確認 |
| --- | --- | --- |
| fence | `palw_dag_sibling_attribution_v1`、`palw_panel_anchor_permissionless_v1`、`palw_rule_e_participation_v2`、`palw_independently_work_certified_v1`(`Option<ForkActivation>`)、`palw_dag_width_schedule_v1`(`Option<PalwDagWidthScheduleFenceV1>`)— `config/params.rs:3186-3206` | 全preset `None`(4 preset literal)、`identity.rs`: collapse・pin list・fingerprint 2箇所Some-only・visit・destructure 3箇所、`fork_id_v1.rs`のprobe、`validation.rs`、mirror `PalwStateParamsV2::rfc0018_v1`(`#[borsh(skip)]`、`state_params.rs:689-697`)、sync `Params::sync_palw_rfc0018_v1` |
| kernel-route aux table | **136**(65-byte key: `'S'`認証row、`'C'`class計数、`'N'`network epoch) | 使用中〜135をgrep。scanner armは`gen_rfc06_native_wire.py`と`gen_rfc06_native_inventory.py`へ追加し、生成器を再実行してrustfmt(`wire_kernel_aux.rs`、`wire_generated.rs`、`generated.rs`) |
| kernel inner kind | 新規なし。RFC-0017のinner 75を、certification idを名指す承認として再利用 | — |
| RPC | 新opなし。`GetPalwSettlementResponse` wire version 4(`uncertifiedFinalAnchors`、`securityCertificationPremise`)、`RpcPalwClaimRow` wire version 6(`securityCertification`)。gRPCはmessage内のfield番号(settlement 13/14、claim row 39)を追加 | RPC op番号 ≤ 244/255、RpcRequest/Response oneofの番号 ≤ 1307は不変 |
| object tag / p2p / funded action | 割り当てなし | — |

### 13.2 module

* `consensus/core/src/palw_rfc0018_v1.rs` — fence、mirror、Gate Cの述語と各site helper、rule E v2のaccumulator、canonical sibling順、permissionless anchor、DAG幅stub、validation。単体試験`palw_rfc0018_v1/tests.rs`、最上位試験`palw_rfc0018_v1/attack_tests.rs`。
* `consensus/core/src/palw_security_certification_v1.rs` — 認証のpolicy、row、鍵、母集団、抽選、verdict、frontier/settlementの単一規則、pruning progress、前提失敗の影響関数、RPCの前提文言。
* `consensus/core/src/palw_security_certification_fold_v1.rs` — `palw_state_v2`の子module: commit、報酬保留、承認、sweep、credit加算。

### 13.3 Gate Cの読み取り箇所(全て、fence未満は従来どおり)

| 合意credit | 箇所 |
| --- | --- |
| participation(rule E v1/v2) | `palw_fork_choice_rule_e_v1.rs:357-383`(record: 未認証は`attempt = false`、未認証`Final`は`Voided`扱い、認証済みFPの`Final`はfrontierなし)、accumulator `:520`、v2 `palw_rfc0018_v1.rs:219` |
| provisional weight(bounded immature/live total) | `transition_builder.rs:8286`(reserve)、`:8311`(release)、`:9311`(void hold)、`chain_state.rs:3367`(整合性の再導出)、`palw_weight_cap_v1.rs:250`(F-W staged)、`:503`(`palw_bounded_immature_v2`)、rule E record `palw_fork_choice_rule_e_v1.rs:420`、予算engine `:296` |
| final weight | `transition_builder.rs:8934`(`finalize_claim`)、`:5060`(`reverse_convicted_final`)、`:9456`(`retire_claim`)、`chain_state.rs:3318`(整合性)、`palw_state_v2.rs:18974`(receipt spend)、`chain_state.rs:3334`(FPの整合性)、認証時の加算 fold `:196` |
| safe frontier | `palw_state_v2.rs:14692-14712`(`palw_gate_c_advances_frontier_v1`) |
| settlement depth | `palw_settlement_v1.rs:89-104`、native settlement `palw_native_settlement_v1.rs:161`、`:338`、`:404` |
| settled anchor | `transition_builder.rs:7545`、`:7744`、`:8630`、`:9194`(fence以後はどれも数えない)、認証時 fold `:196` |
| coinbase(発行由来報酬leg) | `transition_builder.rs:9000`(`Final`で保留)、認証時の支払い fold `:196` |
| commit / 承認 / sweep | `transition_builder.rs:8895`、`palw_kernel_route_fold_v1.rs:1079`、`:1671` |
| pruning | `processor/palw.rs:719-750` |
| RPC | `rpc/service/src/service.rs:2120-2128` |

### 13.4 driver

`scripts/drills/rfc0018_dag_drill.py`: `siblings`(T-1e)、`partition`(T-3c)、`retired`(T-3d)、`attack`(T-0f)。plan固定、node・app dirは各自専有、P2Pはdriver所有のloopback link(方向ごとの配送遅延で到着順を変える)、全判定は安定したsinkでの公開wRPC。driverは何もarmしない。fenceを出さないbinaryではBLOCKED(出荷presetでは常にこれ)。statusは`OBSERVED_CONSISTENT`/`BLOCKED`/`FAIL`。

### 13.5 compile上のリスク(未build)

* 新fieldを持つstruct literal(`PalwRuleESideV1`、`PalwRuleEPairV1`、`PalwSettlementV1`、`GetPalwSettlementResponse`、`NativeDeltaEvidenceV1`、`SkippedEvidenceV1`、`PalwChainAnchorV1`)の取りこぼし。grepで既知の構築箇所は更新済み。
* `palw_security_certification_fold_v1`の即時呼出closure(`try_attest_claim_certification_v1`)の借用、`commit_at_final_v1`での`ensure_route_header`呼出(合成した`PalwBlockContextV2`)。
* processorのanchor walkの新しい変数と、`palw_rule_e.rs`のclosure型注釈。
* `PalwRfc0018MirrorV1`の`Default`(内部policyの`Default`に依存)。
* T-1のE2E試験は2M/8k行を`Active`にplantしてattemptを作る。heldのclassではproducer factsが拒否する可能性がある。

## 14. 有効化gate

~~~text
ACTIVATION_ALLOWED(各fence)
    = ユーザーの有効化判断(U-1)と §11 の値の承認
      AND 単体試験・最上位試験 T-0(attack_tests.rs)を validation が実行して合格
      AND T-0f(attack)を local calibration devnet で実行し、全phaseで 6 量が 0
      AND T-1e / T-3c / T-3d を real node で実行し、収束を記録
      AND claim_retirement_daa ≥ certification lifetime_daa(§7.6)
      AND Gate C ≥ palw_model_eligibility_v1・palw_probabilistic_constraints_v1、rule E v2 ≥ rule E、DNS退役 ≥ rule E
      AND A-2 一様試験(fence 遥か上で全block同一)
      AND 前提失敗の影響(§7.3)をユーザーへ報告済み
~~~

## 15. 参照

* [PRINCIPLES](../PRINCIPLES.md)(§0、§8)、[ADR-0058](../adr/README.md)、[ADR-0105](../adr/README.md)、[ADR-0130](../adr/README.md)、[ADR-0170](../adr/0170-the-seed-anchor-is-a-window-not-a-span.md)、[ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)、[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)、[ADR-0178](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md)
* [RFC-0012](0012-palw-only-consensus-and-native-evm-settlement.md)、[RFC-0016](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)、[RFC-0017](0017-model-eligibility-examination-and-renewal.md)
* [ユーザー判断 2026-10-15 §6](../design/palw/user-decisions-2026-10-15-adr-rfc16-rfc17.md)
* [実装状況](../design/palw/rfc-0018-implementation-status-2026-10-15.md)
