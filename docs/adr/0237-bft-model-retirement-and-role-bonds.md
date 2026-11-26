# ADR-0237 — モデル廃止と個人責任の分離・検証bond保護・49%発見報酬

- 日付: 2026-11-26（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 承認根拠: ユーザーが提示した恒久廃止・検証bond一律保護の本文と、最終coinbase50/20/20/5/5、slash49%配分の追加指示。
- 規範: [RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)
- 関連: [RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[移行](../governance/model-retirement-transition.md)

## 決定

1. BFTのモデル恒久廃止を設計採択する。genesisを含む全モデルに適用し、通常の可逆停止とは別の終端にする。
2. 廃止時は対象poolの現存real MISAKA、登録者責任bond、対象モデル採掘bond、未確定採掘報酬の対象残額を100%burn又は未発行取消する。
3. **全Panel/Gate Cの検証bondをBFT一括burnから保護する。** BFTに正常な検証者を個別判別させない。不正な検証者の個別責任はCourtで裁く。
4. 採掘/検証/登録者bondはmodelと用途のlotで分離し、兼任による一括保護・一括没収、二重担保、責任中の用途変更を禁止する。他model・未割当資金を巻き込まない。
5. 正当に成立した検証報酬のMISAKA支払を維持する。係争中の検証資金も専用予約に保ち、未確定採掘報酬の一括burnへ混ぜない。
6. BFT継続報酬は最終指定の通常coinbase5%。途中の10%は採用しない。BFTの廃止成功報酬・没収資産受取・burn比例報酬は0。
7. 個別Court slashの実回収MISAKAの49%を当該不正の発見・立証に寄与したPanel/Gate Cへ合わせて配り、残り51%をburnする。未決/却下case、未回収債権、BFTモデル廃止burnには適用しない。
8. 被告の同じcanonical責任identity・既知aliasによる受給、同一offence/資金源の二重請求を拒否する。分割回収は累積Sに対する49%差分で処理し、資格・配分planを固定する。
9. 恒久LPの通常引出し禁止を維持し、正規廃止のburn専用例外だけを加える。pool内POSITIONは凍結、既確定holder数量は保存するが同poolで換金できなくなる。
10. 新wire/domain/lot/migration/nativeは別途実装・検証して有効化する。今回の文書により実資産のburnを実行しない。

## 置換する範囲

MAD2の「BFTは採用・可逆停止だけ」、MOE2の「永久LPは処分不能」、旧burn-pauseの単なるACTIVE/PAUSEDだけでは新しい恒久廃止を表現できないため、BMR1に限って明示改定する。旧停止票を恒久廃止票へ流用しない。

旧Courtのslash配分と競合する範囲は、新49/51のpurpose・offence版へ限定して置換する。元の不正認定、反証、責任上限、徴収可能範囲は維持し、旧署名済み契約へ遡及再計算しない。

## 判断理由とレビュー

検証bondを全て対象外にすれば、BFTに正常なPanelを見分ける別の判定系が不要になる。用途lotを残すことで、MinerがPanelを兼任するだけで採掘担保を逃がすことも防ぐ。実回収原資からの49%を発見/立証に結び付け、告発だけで新規発行を得る経路は作らない。

独立レビューに基づき、履歴上のLP累計ではなく現在reserveを対象にし、通貨burn・POSITION retire・未発行取消を別計数する。case freezeは認証と有限期限を必要とし、却下・期限切れ後に独立責任だけを残す。

BFTの判断品質・故障前提、本人性/Sybil、委任資本・不正利益、旧lot移行、資産処分の実装は未検証の課題である。49%の設定だけで不正や共謀が常に不採算になるとはしない。
