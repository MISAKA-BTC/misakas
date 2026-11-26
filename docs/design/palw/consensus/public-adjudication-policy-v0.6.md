# PALW公開裁定の方針 v0.6 — MAS1の採掘適格性と限定BFT停止

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0065](../../../rfc/0007-bft-model-admission.md) / [ADR0233](../../../rfc/0007-bft-model-admission.md)
- 継承: [v0.5](public-adjudication-policy-v0.5.md)のFOW1/TB1、[MOE2](../../../rfc/0016-position-market-and-rewards.md)、[MAS1移行](../../../governance/model-admission-safety-transition.md)

## 1. モデル登録から採掘まで

一般登録と採掘資格を分離する。追加モデルの登録は市場作成と原子的に成立し、未評価モデルの採掘資格はUNASSESSEDから始まる。公開kernel/graph解析、実用性G-USE、FOW1のG-EXEC/G-COLL/G-COSTをすべて満たさない限り採掘を許さない。

FAILは拒否、UNKNOWNはREVIEW_HOLDとしてBFT reviewへ送る。BFTの応答待ち・timeout・未知checkerを仮のPASSへ変換しない。全重みの未認証属性、モデルの評判、有限の非衝突test、安い外付けIDを費用定理と扱わない。

## 2. 検査の有用性と非報酬実行

採掘値zは回答yを作るコア推論へ結合する。回答に不要な重い枝、追加hash puzzle、巨大trace書出しだけをLLMの有用仕事として価格付けしない。正しいcache・batchは分析と会計に含める。

モデルとsuiteをfreezeしてから独立TB1-MODEL_AUDITでtest instanceを導出し、評価結果提出後に別TB1-PANELで再実行担当を選ぶ。評価はNON_REWARDING_MODEL_EVALUATIONであり、採掘権利・合意weight・EXEC credit・MOE2買付を生まない。Nodeは認証出力と評価terminalから有界なscoreを判定する。有限testは一般の計算困難性を証明しない。

## 3. BFT停止と公開反証の区別

客観的な局所反例は従来どおりNodeが検査でき、BFTを待たない。自動確定できない危険には、事前確定したrosterのn=3f+1、q=2f+1署名によるモデル/ruleの安全停止を使う。claim Panelに一人の正直者がいることからBFTの故障前提は導かない。

この証明は停止権限の証明である。Nodeがモデルの計算量下界を確認したという意味ではなく、停止担当へfork choice・台帳finality・reorg・claim算術真偽・既確定資産の変更権を与えない。旧DNS-BFTを復活させない。

## 4. 成功との順序と解除

MAS1対象の同batchでは、認証済み停止・反例・rule失効を成功sweep、Final、mint、credit/weight/permit、未消費権利による受賞ブロックより前に反映する。停止中も元期限・資料・Court・返還を維持する。過去の確定効果を停止証明だけで巻き戻さない。

解除は全gateのfreshなPASSと、現在の停止集合・直前record・検査世代に束縛されたq署名を必要とする。burn Resume、owner変更、core指定、旧証明、timeoutで安全停止を消せない。同batchの新停止は解除に優先する。

この解除はartifact/profile単位である。TheoremRule versionの停止は個別モデルのPASSで一括解除せず、別途採択する新rule versionと各descriptorの再検査で復帰する。世代変更だけで開いているrisk case又は適用済み停止を消さない。

## 5. 安全性の限界とactivation

解析可能な短絡の拒否、不明な新規候補の成功阻止、認証された停止の適用は設計する。未知の短絡を全て発見できること、一般QwenのG-COST、有限testの品質一般化、誤停止なし、停止前の被害ゼロを主張しない。

具体rule・検査器・suite・rosterと故障仮定、非報酬評価、TB1の追加purpose、保護容量、全consumer、restart/reorg/IBD、旧pending drainを検証してから明示activationする。文書採択だけで公開設定・配備・支払を変更しない。
