# MISAKA Fact Settlement Layer(FSL)設計書 v0.5 — 経済設計(改訂統合版)

**版:** Draft v0.5 — Economic Design Revised(§31–§43)
**日付:** 2026-06-11
**コード接地版:** [ADR-0022](adr/0022-fsl-economic-design.md)(本文書がその source design)
**位置づけ:** [FSL 設計書 v0.3](misaka-fsl-design-v0.3.md)(§0–§30、真実決定層のコア設計)の経済編。v0.4 経済設計草稿(未コミット作業版)を全面改訂し**置換**する。リポジトリにコミットされる FSL 経済設計は本 v0.5 が初版である。真実決定層(§0–§30)は本書で変更しない。
**改訂理由:** 外部レビュー(総合 7.5/10)の指摘 15 件・修正案 A–G を反映する。最重要修正は coverage と exposure の分離(旧 §35.3 の数値例は K9a と不整合だった)、保険数理(相関リスク・solvency)、MISAKA 価格依存の遮断、cold start の二段階化、template royalty の逓減と責任有界化、法務構造である。対応は §42 に全件記録する。
**監査トレーサビリティ:** 外部レビュー指摘との対応は §42 に全件記録する(真実決定層のレビュー対応は v0.3 §28)。

## 31. 設計原理

### 31.1 拒否する機構(v0.4 から不変)

```
新トークンの発行 / emission を報酬の主軸にすること / stake 加重による真実評価 /
垂直別サブトークン / 裁定への delegation
```

### 31.2 採用する原理

```
P1: 真実決定層と資本配分層の完全分離(分離壁は回帰テストで固定)
P2: 報酬の主軸は実需収入。補助は seed runway + revenue-gate の二段階(§37)
P3: 価格は USD 建てで定義。支払・担保は MISAKA に依存しない多様化(§36)
P4: 信用を収益化する — ただし「保証」の言語規律を厳格化する(P6)
P5: 希少スキル(predicate engineering、再現可能な証拠生産)に報酬を集中
P6(新): 保証の言語規律 — declared_exposure と covered_exposure を混同した
    対外主張を禁止する。FSL が金銭保証するのは covered_exposure までであり、
    それを超える依存は「FSL evidence を参照する統合」であって
    「FSL insured settlement」ではない。
```

## 32. 二層構造と分離壁

```
資本配分層: coverage bounty 市場(§38)、warranty 引受資本(§35)、archival 投資
─────────── 分離壁(E-F1/E-F2 で監査可能に固定) ───────────
真実決定層: Predicate DSL / forensic evidence / entity-credential panel / L0–L4
```

資本は真実判定に入らないが、**coverage agenda(どの世界をよく裁けるようになるか)は決める**。これは排除せず透明化する: bounty sponsor の開示義務(§38.2)を分離壁の一部として扱う。

## 33. 需要側 — 収入源と価格設計

### 33.1 収入源

```
D1 claim 作成料:   template class / risk_tier 別
D2 settlement fee: accepted_exposure(§35.2)に比例。tier-A 2bp / B 5bp / C 12bp
D3 Product A API:  購読課金(§33.2 で顧客分解)
D4 priority / SLA: 短縮 window・優先 panel・専用 RPC
D5 correction warranty premium(§35)— 「保険料」という呼称は §40 の法務構造確定まで
   対外的に使用しない
```

### 33.2 D3 の顧客分解(仮説の検証単位)

D3 は有望だが「最大の成長源」は仮説として扱い、segment 別 willingness-to-pay を Phase 1 で実測する。

```
segments: AI trading agents / market makers / 報道 / リサーチ / コンプライアンス /
          予測市場 indexer / warranty 引受者 / DAO automation
課金対象は「事実そのもの」ではなく次に限定する(無料公開層の維持は信用の前提):
  low-latency feed / bulk evidence graph / parser diff / source conflict alert /
  finality webhook / correction risk score / SLA / commercial warranty 連携
無料層: スコアボード、確定済み fact の単発参照、再計算手順 — 常時無料
```

### 33.3 D2 の exposure 申告規律(過少申告対策)

settlement fee と warranty が declared_exposure に比例する以上、過少申告インセンティブが生じる。対策を統合規約に組み込む。

```
exposure_attestation: 統合者が declared_exposure に署名
coverage 上限:        申告額まで(過少申告は自らの補償上限を削る — 自己矯正的)
ラベル規律:           未申告 exposure に「FSL-settled」ラベル使用不可
監査権:               random audit + indexer による on-chain 市場規模との突合
exposure_registry:    同一 fact に依存する market / derivative / 保険 / DAO 条件の
                      登録簿。unresolved_correlation_risks(v0.3 §12.3)の入力になる
重大な過少申告:        integration score 降格 + ラベル剥奪
```

## 34. 供給側 — 役割別の報酬設計

### 34.1 Template economy(逓減 royalty + 有界責任)

旧 10% 固定は標準化後の rent-seeking と author の無限責任懸念の両方を生む。次に改める。

```
royalty スケジュール(当該 template 経由の D1/D2 に対して):
  incubation(0–12 ヶ月):     10%
  growth(13–24 ヶ月):        5%
  standard(標準化認定後):     1–2%
  deprecated:                  0%
  critical maintenance bounty: 別枠(保守は royalty と独立に発注)
escrow:   royalty の 40% を correction_tail 期間 escrow
clawback: template defect 起因(K2b)の没収は escrow 残高を上限とする(責任の有界化)。
          gross negligence / fraud の立証時のみ追加 slashing(CorrectionCourt 管轄)
fork 規則: template は fork 可能。fork 版は独立 red-team 審査必須。semantic core を
          再利用する fork は original author に縮小 tail royalty(standard 率の 1/2)
quality-adjusted multiplier:
  royalty_multiplier = base × quality_score × maintenance_score × audit_status
  quality_score の入力 = template_metrics(下記)。単純 volume 比例にしない
template_metrics(公開):
  usage_count / void_rate / correction_rate / challenge_rate /
  average_resolution_time / source_conflict_rate /
  post_cutoff_evidence_usage_rate / red_team_findings(open・resolved)
```

「広すぎる template」(汎用化で royalty を最大化し曖昧性を増やす)は challenge_rate / void_rate が multiplier を毀損するため、経済的に不利になる。

### 34.2 Adjudicator — 多指標 track record と fee schedule

overturn は希少かつ遅延するため(K1 ≤ 0.1%/年では個人評価のサンプルが不足)、選出重みは多指標化する。多数派一致率は引き続き不使用。

```
track_record =
    w1 × overturn_involvement(遅行・最重要)
  + w2 × evidence citation completeness
  + w3 × reasoning reproducibility score(第三者再計算の成功率)
  + w4 × COI disclosure clean record
  + w5 × red-team / calibration case 成績(合成事案 — 新規参加者の評価高速化)
  + w6 × response latency
  − w7 × false escalation penalty
新規参加者は calibration case(正解既知の演習事案)で初期 track record を構築できる
— 上位 entity への評価集中(E5)の緩和策を兼ねる。
```

fee は個別入札禁止を維持しつつ、protocol fee schedule に希少性を反映する。

```
fee = base_fee × risk_tier_mult × expertise_scarcity_mult
    × language_jurisdiction_mult × urgency_mult × historical_defect_mult
multiplier は governance が四半期改定(個別 adjudicator の値引き・上乗せは不可)
```

### 34.3 Source onboarding の四階層(公式署名を MVP 必須にしない)

公式 source の ML-DSA 直接署名は ultimate moat だが、初期採用の前提にすると adoption が止まる。evidence 強度の階層として定義する。

```
L0-native-source:    公式 source が FSL 対応キーで直接署名(最強・長期目標)
L0-notarized-source: TLS notary / timestamped archive / WARC + content hash
L1-parser-source:    公開公式資料 + 再現可能 parser(F23 合格)— MVP の主力
L2-human-confirmed:  公式資料は存在するが parser confidence 不足 → panel 確認
階層は evidence の authority 評価と L0 自動確定の許可判定に使う。
MVP の受理基準は L1-parser-source 以上とする。
```

### 34.4 その他の役割(v0.4 から維持)

Proposer(fee share + bond 分配、track record 選出)、parser 運用者(再計算合格が支払条件)、archival(storage fee + challenge)、warranty 引受者(§35、分離壁適用)。

## 35. 保証経済 — Correction Warranty(旧「保険」の再定義)

### 35.1 Exposure と Coverage の完全分離(最重要修正)

```
declared_exposure:       統合者が申告・署名した依存決済総額(§33.3)
covered_exposure:        購入された warranty の補償上限
accepted_exposure_cap:   min(covered_exposure, security_cap(risk_tier),
                             aggregate_limit 残枠(§35.4))
uncovered_exposure:      declared_exposure − covered_exposure(ゼロでない場合、
                         下流 UI への表示義務)
規律(新 K9a–K9c):
K9a: 「FSL-settled(insured)」ラベルは declared_exposure ≤ accepted_exposure_cap
     の場合のみ使用可能
K9b: uncovered_exposure は下流ユーザーに表示されなければならない
K9c: cap 超過の市場は「FSL evidence 参照」としてのみ統合可能であり、
     insured settlement を名乗れない
改番対応表(v0.3 §12.2 の旧 K9a–K9d との完全対応 — 編集注: 旧 K9d の衝突回避のため
K9f まで採番する):
  旧 K9a(downstream_exposure ≤ 公開補償能力)→ 新 K9a–K9c +
      accepted_exposure_cap(§35.1 の四分解)に置換・吸収
  旧 K9b(attack_cost_lower_bound ≥ β × exposure)→ K9d に改番して維持
  旧 K9c(bond + slashable stake ≥ γ × exposure)→ K9e に改番して維持
  旧 K9d(K9d/K9e 不充足 claim の受理拒否 / multi-oracle 強制)→ K9f に改番して維持
```

### 35.2 Warranty の期間構造(correction tail 基準)

premium の期間は market duration ではなく訂正リスクの tail である。

```
coverage_term:
  settlement_window: claim 作成 → settled_final
  correction_tail:   settled_final → correction 申立期限(template 別、
                     初期値: filing 系 180d / 公式記録系 90d / 高争点 365d)
premium = rate(risk_tier) × covered_exposure × tail_year_fraction × tail_multiplier
rate 初期値(年率換算): tier-A 10bp / tier-B 40bp / tier-C 120bp
tail_multiplier: correction tail 中の後発証拠発生率の実績で改定
```

### 35.3 Unit economics(修正版 — 両ケースを明示)

```
Case 1: declared $10M / covered $1M(tier-B、tail 180d)
  accepted_exposure_cap = $1M
  → insured settlement としては $1M までしか受けない。残 $9M は uncovered として
    表示義務(K9b)。premium = $1M × 40bp × 0.5 = $2,000
  D2 は accepted exposure 基準: $1M × 5bp = $500
Case 2: declared $10M / covered $10M(tier-B、tail 180d)
  premium = $10M × 40bp × 0.5 = $20,000
  D2 = $10M × 5bp = $5,000、粗収入 $25,000 + D3
  控除: 期待補償原価(K1 × cap)、tail reserve 積立、引受資本コスト、
        相関 group の資本賦課(§35.4)
旧 v0.4 の「$10M exposure に premium $4,000」という例は誤りであり廃止する。
```

### 35.4 保険数理 — 相関リスクと支払能力(solvency)

FSL の最大損失は独立な 1 件誤りではなく、**同一設計欠陥による束の誤裁定**である(parser バグで同 template の 300 claim が連鎖 correction、timezone 欠陥で選挙市場群が同時訂正、等)。per-fact cap だけでは止められない。

```
risk_correlation_group(全 fact に付与):
  template_id / parser_id / source_type / source_operator /
  jurisdiction / event_type / template_author
aggregate_exposure_limit(引受の受理判定に使用):
  per_fact / per_template / per_parser / per_source /
  per_jurisdiction / per_epoch
capital_metrics(常時公開、E8–E13):
  available_capital / capital_at_risk / solvency_ratio /
  probable_maximum_loss(PML: 最悪単一 correlation group の全損) /
  stress_loss_99 / stress_loss_99_5 / reinsurance_capacity /
  correction_tail_reserve
受理規則: covered_exposure の追加引受は、当該 correlation group の
  aggregate limit と solvency_ratio 下限(初期 2.0)を同時に満たす場合のみ
tranching: 引受資本は first-loss / senior に分離可。外部再保険は §40 の
  法務構造確定後に接続
```

### 35.5 Flywheel(維持、表現修正)

品質(K1)→ loss ratio 低下 → 引受資本流入 → coverage capacity 増 → accepted_exposure_cap 上昇 → 高額統合受理 → fee 増。「市場が値付けする FSL の品質」として premium rate・decline rate・外部資本比率を公開する。

## 36. 通貨・担保政策 — MISAKA 依存の遮断

### 36.1 問題(レビュー指摘 4)

MISAKA 急落 → adjudicator の USD 建て担保不足 → margin call 大量発生 → 選出停止続出 → panel capacity 低下 → resolution 遅延 → K10/K11 悪化。これは担保問題ではなく **真実決定の供給能力ショック** である。

### 36.2 改訂

```
fee 支払:
  stablecoin 支払いを正式に許可(MISAKA 保有を統合者に強制しない)
  protocol は stablecoin 収入の一定割合(初期 10%)で MISAKA を market buy → burn
  (MISAKA への価値接続は buy & burn / stake 需要 / gas で維持)
stake 担保:
  haircut 付き basket: MISAKA(haircut 50%)/ approved stablecoin(5%)/
  tokenized T-bill 等の承認 RWA(10%)/ warranty LP share(30%)
  slashable_value_usd = Σ amount × (1 − haircut) × oracle_price
panel continuity reserve:
  margin call が epoch 内に閾値超で発生した場合、treasury reserve が
  一時的に担保を補完し panel capacity を維持(事後に当事者へ求償)。
  「価格ショックで裁定が止まらない」ことを K10/K11 の前提条件にする
```

### 36.3 換算価格の操作耐性(レビュー指摘 5)

「DEX 経由の決定的換算」を廃止し、manipulation-resistant pricing に置換する。

```
price_source:  primary = multi-venue TWAP(≥ 30 分)
               backup = oracle feed / fallback = 前 epoch median
               emergency = stablecoin-only settlement(換算停止)
constraints:   max_slippage_bps / min_liquidity_depth / max_epoch_conversion /
               conversion timing の randomization /
               circuit breaker(乖離 > x% で換算停止 + emergency mode)
margin call・burn・treasury 配分のトリガー価格はすべて本 price_source を使用する。
換算価格の安全性は oracle そのものである、という認識を仕様に明記する。
```

## 37. ブートストラップ — 二段階(seed runway + revenue gate)

旧式 `subsidy ≤ μ × fee_revenue` は fee_revenue=0 の初期に補助もゼロになり cold start を殺す(レビュー指摘 6)。二段階に改める。

```
Phase S(bootstrap、epoch ≤ N、N 初期値 = 18 ヶ月):
  subsidy(e) ≤ BOOTSTRAP_SEED_CAP(e)
  BOOTSTRAP_SEED の規律:
    - 事前固定の USD 建て予算(emission ではなく treasury の有限 runway)
    - 公開支出 dashboard(配分先・成果物・検収結果)
    - 配分は coverage bounty(§38)経由のみ(裁量配分の排除)
    - 期限到来後の未使用分は treasury にロック(供給側に流さない)
Phase R(revenue gate、epoch > N):
  subsidy(e+1) ≤ min(SUBSIDY_CAP(e+1), μ × fee_revenue(e))
  μ 初期値 3.0、12 ヶ月毎に半減、Phase R 開始から 36 ヶ月で sunset
Phase S → R の移行条件: shadow program の §19.3 成功条件充足、または N 到来の早い方
```

## 38. 資本配分 — coverage bounty 市場

### 38.1 構造(v0.4 から維持)

需要側が未カバーの事実カテゴリに bounty を置き、template 開発・source onboarding・parser 整備・初期 proposer 流動性を資金づける。bounty は裁定・KPI・selection に一切影響しない(E-F1)。

### 38.2 Sponsor 開示義務(レビュー指摘 11/G)

資本は真実判定に入らないが coverage agenda を決める。agenda の透明化を義務とする。

```
bounty_sponsor_disclosure:
  sponsor entity(credential 連結)/ 関連市場 / 関連 warranty exposure /
  下流統合の利害(当該カテゴリで market を作る予定の有無)/
  template author・parser 運用者・source 交渉者との関係(COI)
sponsor と請負者の COI は bounty 検収の red-team 審査で重点確認項目とする。
bounty 残高分布・sponsor 開示は公開ロードマップとして常時公開。
```

## 39. 経済 KPI と failure modes

### 39.1 KPI(E1–E13)

```
E1  subsidy / fee_revenue 比(Phase R で単調減少、sunset で 0)
E2  fee revenue / settled fact(tier 別)
E3  warranty loss ratio ≤ 30%
E4  引受資本残高と外部資本比率
E5  供給側報酬の entity 集中度(上位 10 < 40%)
E6  template royalty defect clawback 率 ≤ 5%
E7  burn 累計 / stake ロック量
E8  solvency ratio = available capital / risk-weighted covered exposure(≥ 2.0)
E9  aggregate exposure 集中度(template / parser / source / jurisdiction 別)
E10 PML(parser・template defect ストレス下の最大損失)
E11 coverage decline rate(tier / category 別 — 引受拒否率は品質情報)
E12 外部再保険 capacity
E13 correction tail reserve 充足率
```

### 39.2 Failure modes(改訂)

```
FM1 需要不足:          Phase S 終了 + revenue gate により自動縮退。D3 無料層 +
                       evidence graph が最小存続核
FM2 逆選択:            rate 細分化 + per-group aggregate limit + 引受者の個別 decline 権
FM3 fee 底辺競争:      schedule 制(個別入札禁止)+ 希少性 multiplier
FM4 補助終了後の離脱:   emission 延命をしない(規律として明記)
FM5 MISAKA 急落:       担保 basket + haircut + continuity reserve(§36)
FM6 相関事故:          aggregate limit + PML 監視 + tail reserve(§35.4)。
                       発生時は当該 correlation group の新規受理停止 + 一斉 re-verify
FM7 換算価格操作:       §36.3 の circuit breaker + emergency stablecoin settlement
FM8 過少申告:          §33.3 の attestation / 監査 / ラベル剥奪
```

## 40. 法務構造(レビュー指摘 15)

「保険」「premium」「tranching」の呼称は法域により保険業・デリバティブ規制を誘発する。製品を四分割し、MVP は warranty として開始する。

```
protocol_backstop:        プロトコルレベルの限定補償(MVP。対外呼称は
                          "limited correction warranty")
parametric_warranty:      API/SLA 製品保証(D3/D4 に付帯)
commercial_coverage:      規制対応 wrapper — licensed partner / captive / mutual
                          経由で提供(Phase 3)
third_party_underwriting: 法人格を持つ外部資本 pool(commercial coverage の裏側)
法域評価は運営文書として整備(設計書範囲外)。本設計書内の「保険」「premium」の
語は §35 の内部用語であり、対外表記は上記分類に従う。
```

## 41. Test plan(経済、改訂)

```
E-F1:  分離壁(bounty / 引受 / stake 超過分が真実決定の計算に不使用)
E-F2:  引受者の CorrectionCourt 不干渉
E-F3:  Phase S/R の遷移と revenue gate の自動収縮
E-F4:  royalty clawback が escrow 残高に有界
E-F5:  K9a–K9c — accepted_exposure_cap 超過でラベル使用不可、uncovered 表示義務
E-F6:  換算の操作耐性 — TWAP/fallback/circuit breaker の発動系統
E-F7:  担保 basket の haircut 計算と margin call、continuity reserve の発動
E-F8:  track record 多指標合成(多数派一致率の不使用を含む回帰)
E-F9:  aggregate_exposure_limit — correlation group 枠超過の引受拒否
E-F10: solvency_ratio 下限割れ時の新規受理停止
E-F11: 相関事故シミュレーション(parser defect で同 template 束の correction)
       → FM6 の停止・re-verify・tail reserve 取崩しの一連動作
E-F12: exposure 過少申告の検出(indexer 突合)→ ラベル剥奪
E-F13: royalty 逓減スケジュールと fork 時の tail royalty 計算
E-F14: bounty sponsor 開示の必須化(未開示 bounty の設置不能)
E-F15: calibration case による新規 adjudicator の初期 track record 構築
```

## 42. 外部レビュー対応表

| 指摘 | v0.5 での解決 |
| --- | --- |
| 1. coverage と exposure のズレ(最重要) | §35.1: 四分解(declared/covered/accepted_cap/uncovered)+ 新 K9a–K9c。§35.3 を Case 1/2 の正しい数値に修正し旧例を廃止 |
| 2. premium 期間が fact settlement と不整合 | §35.2: coverage_term を settlement_window + correction_tail で定義、premium は tail_year_fraction × tail_multiplier |
| 3. loss ratio だけでは保険として不足 | §35.4: correlation group / aggregate limit / capital_metrics(PML・stress・solvency)、E8–E13 追加 |
| 4. MISAKA 価格依存 | §36.2: stablecoin 支払 + buy&burn、haircut 付き担保 basket、panel continuity reserve |
| 5. DEX 決定的換算の危険 | §36.3: multi-venue TWAP + fallback 系統 + circuit breaker。「換算価格の安全性は oracle そのもの」を明記 |
| 6. revenue gate が cold start を殺す | §37: Phase S(固定 USD seed runway、emission なし、公開 dashboard、期限後ロック)→ Phase R の二段階 |
| 7. royalty 10% 固定が強すぎる / 無限責任 | §34.1: 10→5→1–2→0 の逓減 + maintenance bounty 別枠 + escrow 40% + clawback の escrow 有界化 + fork 規則 |
| 8. 広すぎる template への誘因 | §34.1: quality-adjusted multiplier + template_metrics 公開(challenge/void 率が royalty を毀損) |
| 9. overturn 単独の track record は遅すぎる | §34.2: 多指標合成 + calibration case(新規参加者の評価高速化、E5 集中緩和) |
| 10. fee 固定が専門性偏在を反映しない | §34.2: schedule 制を維持しつつ expertise/language/urgency multiplier を protocol 側で反映 |
| 11. bounty の coverage agenda 形成 | §38.2: sponsor 開示義務(関連市場・warranty exposure・請負者 COI) |
| 12. 公式署名を初期前提にしない | §34.3: source 四階層(native/notarized/parser/human)。MVP 主力は L1-parser-source |
| 13. D3 を仮説として扱う | §33.2: segment 分解と課金対象の限定(鮮度・機械可読性・量・保証・SLA) |
| 14. exposure 過少申告 | §33.3: attestation / 監査権 / indexer 突合 / exposure_registry / ラベル剥奪 |
| 15. 保険の法務リスク | §40: 四分割製品構造。MVP は limited correction warranty 呼称 |

## 43. 最終設計判断(v0.5)

1. FSL トークンは発行しない(不変)。ただし支払・担保は MISAKA 単独依存から haircut 付き basket + stablecoin に多様化し、価値接続は buy & burn・stake 需要・gas で維持する。
2. FSL が金銭保証するのは covered_exposure までである。declared と covered の混同を禁止し(P6)、uncovered の表示義務と insured ラベルの使用条件(K9a–K9c)を統合規約に課す。
3. warranty の価格は correction tail を基準とし、相関リスクは correlation group の aggregate limit と solvency 規律(E8–E13)で資本管理する。per-fact cap 単独の保証主張を行わない。
4. ブートストラップは固定 USD seed runway(emission なし・公開 dashboard・期限付き)と revenue gate の二段階とし、cold start と分配ゲーム防止を両立する。
5. template royalty は逓減 + escrow 有界責任 + quality-adjusted とし、標準化後の rent-seeking と author の無限責任懸念を同時に解消する。
6. 裁定の track record は多指標合成(多数派一致率は不使用)とし、calibration case で新規参加者の参入障壁を下げる。
7. 換算価格は manipulation-resistant pricing を必須とし、価格ショック時も panel continuity reserve で真実決定の供給能力を維持する。
8. 対外的な製品呼称は法務構造(§40)に従い、MVP は limited correction warranty として開始する。
9. 需要が立たない場合は emission で延命せず縮退する(不変)。
