# ADR-0238 — モデル別仕事量とFOW1の報酬・有限EXEC生成権の接続

- 日付: 2026-11-26（Asia/Tokyo）
- 状態: **PROPOSED_DESIGN_NOT_ACTIVATED**
- 規範案: [RFC0070](../rfc/0070-model-work-accounting-and-exec-credit.md)
- 背景: ユーザーはモデル登録時の計算量推定、モデル別の公平な報酬とEXEC量への反映、および1 BPS/10分の台帳確定目標を求めた。詳細式の採択・native実装・公開有効化は別工程である。

## 判断案

1. 既存CanonicalWork/CCU/TIR Workを共通meterとして拡張する。全モデルの一回のpromptを同額にしない。規範計算量と実際の機器費用、検証費用、Node資源上界を分離する。
2. モデル採用にWorkProfileIdを束縛する。BFTは計算量推定を確認するが恣意的倍率を設定できない。AUTOとBFTで同一meter、既存MAD2の採否条件を維持する。BFTの実機測定資料は採否の補助であり、数学的な費用証明や一律のベンチマーク合格を要求しない。
3. FOW1の一試行一Ticketと実行前固定targetを維持し、初版は共通target＋当選原資の実認定仕事量比例方式を選ぶ。最大原資を事前予約し、事後target変更・予算clip・仕事量倍率の二重適用を禁止する。固定jobのtarget比例案はこの版に混ぜない。
4. MOE3の全モデル1.0・一意市場・BFT管理、共通発行上限とmodel資本上限を維持。資本shareを上限、仕事量を上限内の支払基準にする。共通sourceの実認定仕事予算をMOE3 §4.3の按分重みとして使う限定改定を提案する。
5. MOE3の50/20/20/5/5、整数残差、結合按分、実予算比LP、小数8桁、BMR1の個別Court実回収49/51と恒久廃止100%burnを変更しない。全検証bond保護と個別責任も維持する。
6. 正規当選・Final・独立Gate C・成熟等を満たす認定仕事から、別purposeで有限EXEC creditを一回発行する。落選仕事の別入口、追加抽選、無認証weightを作らない。scheduleを事前freezeし、独立TB1 SCHEDULEを使う。
7. 1 BPSは全kindの共通外側carrier上限・目標とする。10分は通常健全時のsettled frontier進行の目標であり、個別claimの期限を一律600秒に置換しない。120秒計画epoch・DAA・発行scheduleを自動変更しない。
8. 全consumerの合意gate、原期限、旧履歴、責任、reorg、pruning・IBDを維持。QF1 REVIEW_HOLDや公開fenceを本文で解除しない。

## 選択肢と理由

| 選択肢 | 判断 |
|---|---|
| 1 prompt一律報酬 | 不採用。重い計算を避ける動機を作る |
| 総parameter数による倍率 | 不採用。MoE稼働expert、context、Stop、量子化を反映できない |
| BFTがモデルごとに単価を裁量決定 | 不採用。恣意的な発行・報酬誘導と異機種不一致の危険 |
| 事後的に実出力長でtarget変更 | 不採用。FOW1固定targetを破る |
| 固定jobの事前target比例 | 将来別版候補。可変長jobでは本版へ混在させない |
| 共通target＋認定仕事比例の当選原資 | 初版提案。発行・bond cap、全額事前予約が必要 |
| 仕事量をtarget・原資双方に掛ける | 不採用。期待値の二重加重 |

## 残る設計・実装課題

共通価格・targetのepoch更新則、Q_exec、資本・役務・source closeの予約サイズ、正規meterの演算表、branch facts codec、bounded Courtのcoverage、署名/wire/fenceは別途決定し、未設定時は有効化しない。5%検証枠の採算、GPU/低RAMの性能、Qwen2BとFlash Nextの正確なmodel IDと実測、1 BPS/600秒の達成可能性は未検証。期待配分は換金額・物理費用利益の保証ではない。採択は[設計凍結の変更手続き](../governance/design-freeze.md)に従う。
