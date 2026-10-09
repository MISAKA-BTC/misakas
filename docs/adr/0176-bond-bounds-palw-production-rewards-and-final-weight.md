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
