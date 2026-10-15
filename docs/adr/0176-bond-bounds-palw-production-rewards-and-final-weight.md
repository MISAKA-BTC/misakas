# ADR-0176 — Bond bounds PALW claim capacity, blocks, rewards and Final weight

* **Status:** Accepted as a design premise at the user's request, 2026-10-10. Specification details, implementation, economic measurements and activation remain pending.
* **Scope:** All future PALW reward-bearing producer paths, including Attempt, Free Prompt, claim-backed blocks, receipt rights, slices/riders/batches, unpaid escrow and settlement readers. Applies with existing Panels and with the separately gated Panel=0 mode.
* **Complements:** [ADR-0171](0171-probabilistic-constraint-checks-and-court-on-dispute.md), [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md), [RFC14](../rfc/0014-panel-independent-fraud-prosecution.md) and [RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md).
* **Precedence:** For future PALW economics, this decision supersedes directions that increase bond-attributed economic or definitive consensus credit merely by multiplying claims. Earlier implementation records and old-claim rules remain historical. No wire id, Params field, activation DAA, fingerprint or deployment is assigned here.

## 1. PALWの成立に必要な共通前提

**MISAKAは確率的な計算検証と経済的拘束を組み合わせてPALWを実現する。
Bondが採掘速度と報酬総額の上限を決め、モデル計算はその枠を使用するために必要な正当な仕事となる。
不正をしても枠は増えず、公開証拠で反証・客観裁定されれば、報酬を失い、適用規則に従って担保を失う。**

同額の適格な拘束資本、同じDAA期間、同じ共通ruleset/容量倍率に対して、正直なminerと
計算を省略して高速にclaimするminerに認めるclaim・ブロック・報酬・確定weightの上限は同じである。
同じbondから一定期間に得られる機会を計算実行速度から切り離し、偽造の速度優位を追加の採掘枠へ変換させない。
実際に発行・Finalできる件数や支払額が同じになる保証ではない。正当な計算、適用modeの検証、
admission資源とFinal条件は別途必要であり、有罪claimは報酬対象にならない。

経済的な上限は計算正当性の証明ではなく、確率的checkerは実監視・証拠包含・担保回収を保証しない。
承認済み全constraintの検査、公開material、独立public verifier、exact court、実際の徴収可能額をANDで要求する。
この決定をstakeだけで計算を省略して採掘できる規則にしない。

## 2. 規範的決定

### D1 — Bondと共通DAA期間による四つの上限

producer bondを`b`、適格な拘束資本を`C`、合意上の期間を`I`（長さ`W` DAA）、
共通のclaim容量倍率を`rho`として、合意会計は次をMUST満たす。

~~~text
N_claims(b, I)       <= Q_max(C, W, rho)
N_reward_blocks(b,I) <= B_max(C, W)
R_attributed(b, I)   <= R_max(C, W)
FinalWeight(b, I)    <= F_max(C, W)
~~~

同一rulesetの上限はModel ID、class、PWU、実行時間、token数、主張した仕事量、計算速度で増やさない。
`rho`を上げるとclaim容量は増やせるが、`B_max` / `R_max` / `F_max`は増やさない（MUST NOT）。
未決済claim数・暫定weight・最大責任額・node処理予算の別capも維持する。
classごとの計算・検証・損失担保が不足すればadmissionを制限するが、共通の追加経済枠を与えない。
資本増減、期間境界、rolling/epoch集計、支払構成と帰属時計はrelease前に固定し、遅延payoutや繰越で迂回させない。

### D2 — Claim容量拡大と一件の経済的権利を連動させる

claim容量を`m`倍にしても、同額bond・同期間の経済的総影響を増やさない（MUST）。
実装は一件の権利を細分化するか、同じ固定予算を複数claimへ明示的に配分する。
次は同じ大きさの権利を配分した場合の説明例であり、実装定数ではない。

| 項目 | 基準 | claim容量×m |
| --- | --- | --- |
| 発行可能なclaim数 | N | mN |
| 一件の報酬・ブロック権利・weight配分（それぞれ） | A | A/m |
| bond全体の配分上限 | N×A | N×A |

物理blockを`1/m`個として発行しない。端数のブロック権利は合意上の整数/fixed-point台帳で集約し、
定義された閾値と資格を満たした実際のblock生成時にも`B_max`を検査する。
per-claimの切上げ・最小報酬・最小weightで総量を増やさず、残余の扱いを決定的にする。
実際の計算量/PWUの記録を小さく偽装せず、計算の真のstatementと付与する経済的/consensus creditを分離する。
同じ仕事の分割・root/slice・rider/batch・receipt・権利譲渡で二重の権利を作らない。

### D3 — Final後まで一貫した予約・消費・帰属会計

claim受理時にclaim枠と、派生し得るblock・報酬・Final weightの最大配分をMUST予約する。
未払escrow・将来請求権も含め、権利行使・block生成・支払/成熟・Finalで同じ予約と総上限を再検査する。
予約を消費済みcreditへ移す際は二重計上しない。一claimから複数block/支払が派生しても同じbond予算を消費する。

暫定weightだけをbondで抑え、Final時に未予約のfull weightを加算する経路を新規則へ持ち込まない（MUST NOT）。
canonical work/PWU、暫定・Final・retired weight、fork choice、DAAへの接続、RPC/EVM/bridge等の
全writer/readerとundo/reorg/IBDで同じversioned配分を読む。純粋な運搬・slice・再送で追加weightや乱数source数を作らない。
既存EXEC_TX/EXEC_SLICEのweight/DAAは0のままであり、独立のreview/upgradeなしにclockを変更しない。
適用modeのclaim固有証拠・court/Final条件を満たさないworkから不可逆な利益を先取りさせない。
モデル未取得だけでFinal/weightを止めず、旧取得保証はADR177の条件付き検証へ改定する。

### D4 — 共通の最早回復時刻と責任担保を分離

受理DAA `d`で消費した枠は、共通の`reuse_not_before = d + W`まで回復させない（MUST NOT）。
早期Valid/audit/Final、取消、void/conviction、再送・別sessionでclaim枠や失効rewardの配分を早く返さない。
同額bond・同期間では計算省略によって枠の回転率を上げられない。

発行枠の時計と、challenge/court/DA/retention/liabilityの責任終了を別に検査する。
未解決責任を担保する資本・未払報酬を保持し、`d + W`到達で同じ担保を新旧claimの損失に重複使用しない。
slash後の減った資本、既存責任控除後のheadroomを次のadmissionに反映する。
bond分割、退出/再登録、鍵変更、担保移転で消費履歴と時計を消さず、同額資本の集約上限を増やさない。

### D5 — 小さいbondで実用的なclaim数を扱うための予約設計

正直なminerに数十万BILIの担保を機械的に要求する構造を避ける方向で、
一claimの経済的権利と必要予約を細分化し、検証・court・DA費用と最大回収責任を満たす配分を設計する。
admission数を増やしても未払の全rewardや共有担保を架空の追加資金として数えない。
予約額の引下げはescrow規則、公開検証能力、徴収可能担保、共通DAA拘束とMUST一体評価する。
拘束期間を延ばすだけで検証能力・徴収可能額の不足を満たしたと扱わない。
350 BILI、rho=10,000、100件/7日は未採用の検討例であり、現行最小bondや発行量を変更しない。

### D6 — 正しい計算を促す経済評価

同じ発行機会の上限を確認した後、実効検出・裁定・徴収確率`p`、失効する報酬`R_risk`、
攻撃者への還流を控除した徴収可能損失`L_collectible_net`、節約計算費用`C_saved`を評価する。
単純化した同じ機会数の比較では`p * (R_risk + L_collectible_net) > C_saved`を要求する。
同時claim間の検出相関・共有担保枯渇・自己共謀reporter bounty・fork-choice/外部市場利得は別途計上する。
現行49% reporter規則を引き継ぐ評価でgross Slashをそのまま攻撃者の損失としない。
実効`p=0`なら費用節約による不正優位は残る。capだけでPALW安全性が証明されたとは宣言しない。

## 3. 現行コードとの違い

現在のpre/t12にはρで[担保予約の一部](../../consensus/core/src/palw_weight_cap_v1.rs)と
[slot/token容量](../../consensus/core/src/palw_issuance_slots_v1.rs)を変える仕組みがある。
[ADR-0167](0167-the-x1000-capacity-package-a-fixed-per-daa-reward-budget-riders-and-a-lower-only-breaker.md)の
×1000 packageはDAA全体のemission budget、rider分配、breakerも含むが、この決定の全上限ではない。
モデルclassの[escrow割引](../../consensus/core/src/palw_audit_door_v1.rs)には対象制限があり、
現在の[Final weight](../../consensus/core/src/palw_weight_cap_v1.rs)はρだけで一律に縮小されない。
[t12確認記録](../design/palw/t12-bond-reuse-audit-2026-10-10.md)は現行コード観測として保持し、
class/work依存の拘束期間という旧提案は本決定の共通期間へ改定する。
これらの既存機能をD1–D6達成の証拠に流用しない。

## 4. 実装・受入・移行

詳細設計を[RFC15 §8](../rfc/0015-panel-free-permissionless-verification.md)と関連RFCへ接続する。
全reward-bearing mode/profileで共有会計・整数精度・帰属・最大同時責任・回復時計・経済測定を完成させる。
旧claimは受理時rulesetのまま完了し、新規則はversioned codec/state/fingerprintと別の協調upgradeで適用する。
旧claimの残存責任を新予算へ移す境界を定義し、upgradeを満額枠の再取得経路にしない。
Panel=0、確率的検査、ADR177の元本/coinbase会計・経済評価・claim固有court scopeのgateも省略しない。
旧Torrent/FPR gateは撤回し、model予算の変動で個別bond capを増やさない。

| 必須受入試験 | 確認する性質 |
| --- | --- |
| 同額資本・同期間・同倍率、正直な計算と高速偽造、異なるclass/work量 | claim上限が共通。ブロック・報酬・Final weightの総上限を偽造で増やせない |
| 容量×1/×100/×1000、端数・最小credit・期間境界 | claim数を増やしてもB/R/Fの上限が不変。丸めや繰越で増えない |
| 一claimから複数receipt/block、Free Prompt、slice/rider/batch、権利移転 | 受理予約と全支払/Finalの両方で同じ帰属予算を消費 |
| 早期Final、void、再送、d+W到達、未解決courtと担保枯渇 | 早期枠回復と担保重複使用がなく、徴収可能額を維持 |
| bond分割・再登録・鍵変更・担保移転、混在ruleset | 同額資本の総上限が増えず、残存責任と消費履歴を引き継ぐ |
| Final/retired/reversal、fork choice/DAA、RPC/EVM、restart/reorg/IBD | 全writer/readerが同じ予算とversioned weightを復元・取消 |
| Panel/miner共謀、外部検証、実際の回収と自己共謀bounty | 公開反証・有界裁定・報酬失効・net担保損失まで成立 |

測定項目はbond額当たりのclaim/DAA、報酬対象block/DAA、報酬額/DAA、Final credit/DAA、
責任期間・最大同時露出、検査/court資源、実効検出/徴収率と計算費用である。
有限の文書検査や旧test PASSを新mode・新経済保証の完成証拠にしない。

## Model-bond allocationとの接続 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)のmodel配分は有効な拘束miner元本に連動する集合予算である。
公開参加が増えても既存bondのQ/B/R/F上限と共通DAA拘束は不変とし、支払時はnetwork/model/bondの
全予算を同時に検査する。model配布・取得可否は合意条件から外し、正しいmodelを取得したverifierの
検証能力と閉鎖時の実効検出率を区別する。倍率式と公開優位は未立証であり、元本だけの支払を認めない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

## 改定 — 2026-10-12（共通原則の正本化、Bond/time台帳の具体化：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §1・§3・§3A。

- **§1の正本:** 本ADRの§1を共通原則の正本とし、RFC索引と関連RFC（RFC-0010/0014/0015）から参照する。同額の適格拘束元本・同一期間・同一ruleset・容量倍率に対して、正直に計算するminerと計算省略で高速にclaimを出すminerに与える上限を同一にする。計算省略、即時Valid、再提出、取消、失効、鍵・bond・modelの分割や移動で、同じ元本の枠を増加・早期回復させない（MUST NOT）。枠を確保しただけでは支払わず、正当なclaim・検証証拠・裁定・Final条件を満たした範囲でだけ報酬・確定creditを付与する。Panel人数や計算速度を増やしても同じproducer元本の絶対上限は増えない。
- **D1/D3/D4補足:** 長さW DAAのrolling会計を基準案とする。claim受理時に、そのclaimが派生させ得るclaim枠・ブロック権・報酬・Final weightの最大権利を同一の元本・予約へ原子的に帰属させる。受理DAA `d` の消費済み枠を `d + W` より前に再利用しない（Valid、Final、PanelUnavailable、取消、court終了は早期回復理由ではない）。失効額を新claimの予算へ早期に戻さない。採掘枠の回復、資金の送金可能性、court/retention責任からの解放を別々に検査し、同じ元本を新しいbond/鍵へ移して消費履歴を消さない。rhoを増やしても総量は増えず、端数・最小支払・複数leg・root/slice/rider/batchで増幅させない。network/model/bondの三層予算を受理・ブロック権行使・支払・Finalおよび全reader/undoで共通に検査する。期間内総量capと短時間のburst capを区別する。
- **D6の受入条件:** `p_effective · (R_at_risk + L_collectible_net) > C_saved`。`p_effective`は検査、局所化、証拠包含、客観裁定、徴収の完了まで含む。共謀reporterへのbounty還流、担保の使い回し、同時claimの相関、外部利得、Panel報酬や検証費の省略益を評価に含める。総没収見込額を実在する回収可能資本より大きく見積もらない。
- **モデル配分との関係:** ADR-0177の2026-10-12改定（Miner＋Panelのclaim bond合算と1.3〜2倍の配分係数、提案）は、本ADRの個別Q/B/R/F絶対上限を後から乗算で増やさない。「各bondの絶対上限自体を1.3〜2倍へ変更する」案は別案であり、採用していない。
- **2026-10-11の裁定との関係:** fee-only RoundはB_maxに数えず独立のExecutionCapで制限、market feeはv1でR_maxに含めない、予算window W = 400 DAAはdrill暫定（activation-readiness-matrix §3i）。

## 改定 — 2026-10-15（ユーザー方針 2026-10-15: 共通拘束期間・上限の設定方法・測定手順）

**状態:** 方針は確定（ユーザー）。コードは休眠実装・未build・未検証（`palw_bond_budget_v1` V3 policy、全presetで`None`、有効化は拒否）。数値は本番定数ではない。

- **主条件:** 同額の適格拘束元本・同じ検証mode・同じ期間で、計算を省略して高速にclaimする者が正直な計算より多いclaim枠・報酬ブロック・報酬・Final weightを得てはならない。**かつ**、参照用の正直なminerが実用的に使える上限にする。「上限が等しい」ことと「正直なminerが上限に届く」ことは別であり両方を試験する。需要・設備・検証がある間に上限不足だけでREAL生産が止まらないことも試験する。
- **共通拘束期間（設計基準）:** Panel=1は1日相当DAA、Panel=0は7日相当DAA（1日 = 86,400 s / 120 s の時計slot = 720 DAA、名目換算。実測が遅いDAAでは実時間で1日より長くなり、短くはならない）。
- **1日当たり最大発行の均等化:** 同じbondの1日当たり基準上限`q, b, r, f`に対し、Panel=1は1日で`(q, b, r, f)`、Panel=0は7日で`(7q, 7b, 7r, 7f)`。同じ件数を7倍速く回復させる方式は採らない。実装: V3 policyの単位当たりrateを「1日当たり」と読み、`daily(C) = ⌊C·rate/u⌋`、mode `M`の期間上限 = `daily(C) · days(M)`。
- **一つの台帳、三つの見方:** 長期view（engineのwindow、全reservationを`d + 7日`まで保持、全claimを`7·daily`で検査）、日view（各claimのshadow行、`d + 1日`で解放、Panel=1 claimを`daily`で検査）、burst view（shadow行、`d + W_burst`で解放、全claimに追加の制限）。modeを切り替えても同じ元本の消費履歴・拘束・責任を引き継ぐ（Panel=0で消費→Panel=1で満額を再取得、またはその逆は不可）。受理済みclaimの期間・予約・責任は受理時に固定し、後のmode変更で短縮しない。
- **枠の回復 ≠ 担保の解放:** 1日 / 7日は最早の通常再利用時刻であり、全額返却の保証時刻ではない。有効なchallengeや開示/courtが続く間は必要な担保を保持する（`H_L` / `liability_until`）。Panel=1の1日は早期（共謀）Valid署名で短縮しない。
- **burst:** 週の総量だけでは初日に全量を使い6日以上休む使い方が残るため、短期のrate/burst上限を**追加の制限としてのみ**置く（消費済み7日枠を早く返さず、短期bucketと長期台帳の両方から権利を発行しない）。burst許容量はDAG並列度と実測した提出パターンから導く名前付き定数。
- **担保規模:** 同時に予約される元本 ≈ `λ·a·H`（1日の受理数 × 1 claimの責任額 × 実際の平均保持時間）＋余裕＋他の責任。保持が長いほど同時担保が増えるため、1 claimの権利・予約・報酬を一体で調整する（D5）。架空担保・二重予約は不可。
- **heartbeat:** heartbeat 0を目標にせず、数値改善のためにheartbeatの時計・fallback規則を弱めない。floor状態を動かすREALは、検査を通過しclaimを書いたAttempt laneのblock（`palw_real_share_v1`）であり、riderは同じに数えない。各段（計算→抽選当選→claim受理→採用REAL block→検証/Final→清算→bond枠回復）を分けて測り、計算と提出が可能なのにbond枠不足だけでREALが止まった時間、heartbeat依存時間、回復後の採用を記録する。heartbeatだけの期間からREALへ戻る回復は独立の必須試験とする。
- **導出手順:** [cap derivation and measurement plan](../design/palw/adr-0176-cap-derivation-and-measurement-plan-2026-10-15.md)（Stage 1 実測 → Stage 2 sweep → Stage 3 敵対/境界試験）。該当範囲が無ければ数値を作らず、衝突する制約を報告する。現行コード値や「100件/7日」等の過去例を正式値にしない。
- **実装の所在:** `consensus/core/src/palw_bond_budget_v1/hold.rs`（`PalwBondHoldPolicyV1`、`PalwHoldModeV1`、日/burst view、`hold_admit_v1`）、`palw_bond_budget_fold_v1.rs`（planがmodeを運ぶ）、`palw_bond_cap_sweep_v1.rs`（Stage 2/3 simulator）、`scripts/drills/adr0176_stage1_measure.py`（Stage 1 driver）、試験source `consensus/core/tests/adr0176_mode_hold_v1.rs` / `adr0176_cap_sweep.rs`（未実行）。

## 改定 — 2026-10-15（ユーザー判断 2026-10-15: bond式の再設計）

**状態:** 判断は確定（ユーザー）。コードは新しい休眠fence **`palw_bond_formula_v2`** の背後に実装（全presetで`None`、Some-onlyで両fingerprintに入り、`Some(never())`は畳まれ、有効化は拒否）。各claimの受理DAAで判定するため、testnet-12で稼働中の式（`L_existing + L_new ≤ u·C`, `u = 0.5`, `L = ⌈PWU_exposure·S·A/ρ⌉ + m_c`）は変わらない（A-2一様）。値は名前付きfence定数であり本番承認値ではない。

**ユーザーが確認した現状（pre `4fb381a87`と2026-10-10監査JSON、合成bondの価格照会。実測ではない）:** Qwen A16 8kで`0.02471601 + 3,200.84650080 = 3,200.87121681` BILI（約99.9992 %がescrow）、A16 2Mで`59.74294207 + 3,200.84650080`。escrow割引`⌈E/ρ⌉`はFloor classだけ（model classのGPU監査能力が未測定のため意図的）。licence形成がFinal前にescrow予約とissuance slotを解放するため、共謀Validで同じ2 slotを回転できる。`u`を100 %にしても13,000 BILIで2件→4件、`ρ`を上げても0.0247の項しか縮まない。1日/7日の拘束で1件約3,200 BILIを保つと30件/日の例に約192,052 / 約1,344,366 BILIが要り、13,000 BILI・`u=0.5`で30件/日に収めるには1件約216.67 / 約30.95 BILI以下が要る（逆算であり提案値ではない）。

**新しい式（受理DAA `d`、claim種別`t`、mode `M`、bond元本`C`、V3 budget policy、formula policy）:**

```text
(a) 同時担保   Σ_open L + 既存責任lock + 告発予約 + 未決責任 + L_new ≤ u · C        u = 500‰（暫定の出発点、最初に調整しない）
(b) 期間会計   Q / B（後のcarrierへの帰属を含む）/ R（未払を含む）/ F を共通拘束W_M（Panel=1 1日、Panel=0 7日、1日当たり均等化）で集計
(c) 枠の回復   earliest_recovery = d + W_M を受理時に固定。早期（共謀）Valid・licence形成・Final・void・court結果で前倒ししない。
               担保の解放は別条件（未解決のdispute/courtが担保を保持）

1 claimの権利（D2: 期間総量をclaimへ分割）
  r_c = ⌊r_day / (q_day · ρ)⌋,  f_c = ⌊f_day / (q_day · ρ)⌋
  R_c = min(R_ask, r_c)（削減許可時。Floor class、または固定された実測監査能力recordがあるclassだけ）、それ以外は R_ask
  F_c = min(F_ask, f_c)
予約の分解（各部分に固有の保持時間）
  L_weight  = max(⌈f_c · v_F⌉, ⌈PWU·S·A · λ_pwu / 1000⌉)          ρで割らない
  L_right   = R_c                                                   （kernel / nativeは0: 利用者escrowはproducer担保ではない）
  gain      = R_c + ⌈f_c · v_F⌉                                     （claimの最大帰属権利）
  L_coll    = max(loss_floor_t, ⌈gain · κ_t / 1000⌉)                 ρに依存しない下限、κ_t ≥ 1000
  L_penalty = max(0, L_coll − L_right − ⌈f_c · v_F⌉)                （kernel / nativeはL_coll全額）
  L_other   = court_cost_t + da_retention_cost_t
  L         = L_weight + L_right + L_penalty + L_other ≥ L_coll ≥ gain
保持: weight・right・penalty = terminal + H_L、other = max(d + retention_hold_t, そのclaimの全court/DA sessionの終了)、枠 = d + W_M
```

- **licence形成での解放を撤廃:** fence以後に受理されたclaimは、licenceでescrow予約を解放せず（`palw_rcore_supplementary_flips_v1`）、issuance slotは`d + W_M`まで保持する（`palw_issuance_holds_slot_v1`）。Final・終了後も、budget engineの責任view（`close_keeping_liability_v1`）が同時担保検査で責任終了まで数える。
- **分解の公開:** RPC `getPalwClaims`のclaim row v5 `reservationSplit`（JSON: `type`, `mode`, `weight`, `right`, `penalty`, `other`, `total`, `collectableLoss`, `gain`, 各`*Hold`, `capacityRecoveryDaa`, `rights`; proto field 38）。fence未満のclaimは空。
- **Floor限定のescrow割引の置換:** model classは、1 claimの期間権利が上の分割で有界であり、かつclassの検証/監査能力が測定済み（`PALW_BOND_FORMULA_MEASURED_CAPACITY_PINNED_V2`に固定されたrecord、現在は空）の場合だけ削減予約を受ける。監査backlogの懸念は価格ではなく能力で扱う。
- **Panel=0 / Kernel claimの別会計:** 利用者払いescrow（coinbase legなし、model予算で削らない）、producerの責任担保（利用者の前払いで消えない）、producerの期間R上限を分ける（`palw_kernel_accounting_v2`）。約3,200 BILIの自作Attempt見積りはFree Prompt・Kernel claimの共通価格ではなく、種別ごとに価格を付ける（`PalwClaimTypePricingV2`: Attempt / FreePrompt / KernelPanelOne / KernelPanelZero / NativeFunded）。
- **ρと損失:** 予約を小さくしても不正利得は小さくならない。徴収すべき損失の下限（`loss_floor_t`）はρで縮まず、PWUのslash exposureもρで割らない。報酬・weightを据え置いたまま予約だけを1/1000にすることはしない。
- **測定・報告:** claim種別ごとに、予約をweight / escrow権利 / その他責任へ分け実際の保持時間とともに、同時担保`≈ λ·a·H`、四つの合格条件（担保会計、期間上限、実用性、REAL継続）を報告する（上記plan）。
- **実装の所在:** `consensus/core/src/palw_bond_formula_v2.rs`、`palw_bond_budget_v1/hold.rs`（責任view）、`palw_state_v2/state_params.rs`（`claim_escrow_term_v2`）、`palw_weight_cap_v1.rs`（weight部分とG scale）、`palw_issuance_slots_v1.rs`、`palw_state_v2.rs`（licence flip）、`palw_bond_budget_fold_v1.rs`（budgetの責任額、close）、`palw_producer_v2.rs` / RPC（分解の公開）。試験source `consensus/core/tests/adr0176_bond_formula_v2.rs`（未実行）。

## ユーザー方針 2026-10-15 による改定

* bond当たりのQ/B/R/F上限は維持する。上限は**認証済みclaimのcreditの上限**であり、未認証claimのcreditは上限以前に0である。予約は受理時(変更なし)、合意creditの付与は認証時。
* rule E v2のparticipationは`F`での担保`C`(bond式v2の`u·C`と予算engineの`F_max(C, W)`が読むのと同じ`C`)で数え、分割で増えない([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §6)。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。
