# RFC-0012 — 用途別bond・予算・Court slash

- 文書日付：2026-11-26（Asia/Tokyo）
- 文書区分：**DESIGN_SPECIFICATION**。採択済みの規則を統合する。提案に限る部分は該当節で明示する。
- 判断理由：[ADR-0012](../adr/0012-role-bonds-budgets-and-slashing.md)
- 規範入口：[設計原則](../PRINCIPLES.md)／[RFC一覧](README.md)
- 本文は実装の設計を定める。実装・実測・公開有効化の完了を示すものではない。

## 1. 原資と役割

miner account bond、model登録者責任bond、Panel検証bond、TB1/BFTの役務bond、手数料UTXO、報酬原資、market reserveを別勘定にする。残高があるだけで任意の役務へ拘束してよいことにはしない。

modelと用途を固定したlotを使い、同じ元本の二重担保を禁止する。兼任者でもMININGとVERIFICATIONを分け、責任中の付替え・引出し・他modelへの逃避を認めない。

## 2. 予算予約

受付前に、実Jobと実modelからclaim責任、検証、資料供給、受賞時支払い、未解決case、後続carrierの必要量をnative見積もりで計算する。Studio/Gatewayは同じ計算結果を使用する。

```text
reserved + consumed + closed_unissued <= authorized
```

source、model、bond、署名slotの各scopeで検査する。総bond残高、予約額、その予約から実際に支払いへ使える上限を区別する。高額な受賞に対して支払可能額が不足するclaimを、安価な実行見積もりだけで受理しない。

全額予約不能なら試行前に拒否又は小さい新Jobを提案する。実行後・当選後に勝手に上限を引き上げたり、約束した配分をclipして成功にしたりしない。

## 3. 資本上限と回復

全モデルの係数は1.0。model別の認証された配分・資本上限は、共通policyの平均bondと規範式を用いる。既存の7日相当平均bondの3/2乗を使う版は、その計測区間・正規化・端数・有効時点を固定する。

資本は機会・影響の上限であり、仕事量meterへ再び倍率として掛けない。担保能力の回復、元本解放、発行枠回復、未使用予約の返金は別の遷移とする。

genesis起動に特別な既得mining bondを必須にせず、正規BASE回復・通常のbond登録で参加可能にする。ローカルdevnetの固定テストbondを公開の特権と混同しない。

## 4. 一回消費とreorg

署名済みEXEC slotの使用、quantum/permit、仕事キー、原資消費、報酬出力を同じ受賞遷移へ結合する。一部消費・ゼロ額処理でも同一の使い切りslotを再利用しない。

rollbackは元の親状態と保存deltaへ一致させ、再適用は同じ結果を再現する。別枝で消費された記録を、そのまま現在枝の決済と表示しない。再送は新しい支払いにならない。

## 5. Courtの個別slash

個人の計算不正、署名違反、認証された提供義務違反等は、Courtで確定したoffenceと責任lotに限定して処理する。任意の未応答、BFT不採用、正しい高速計算だけで有罪にしない。

実際に回収したMISAKAをSとすると、選出内外の発見・立証者の**合計**報酬は`E=floor(490*S/1000)`、burnは`S-E`。各役割へ49%ずつ支払う規則ではない。

受取資格、提出元、先行性、重複、同一offenceのsourceを正規planで固定する。不正本人・同一認証identityの自己slash受領を認めないが、未知のsybil共謀を完全排除したとは主張しない。

分割回収では累積Sに対するEの増分だけを配る。同じ回収額から繰り返しfloor報酬を発行しない。未回収の宣言額、失効した未発行予約、BFT恒久廃止資産をSへ足さない。

## 6. BFT廃止との分離

全PanelのVERIFICATION lotは、BFTによるモデル恒久廃止の一括burnから保護する。不正者もこの用途上の保護を持ち、本人の違反は別のCourtで個別slashする。

登録者責任・MINING用途・未確定mining報酬・actual market reserveの廃止効果はRFC-0017に従う。正当に成立した検証役務報酬をモデル廃止だけで消さない。BFTの通常5%と没収原資は別であり、廃止bountyを作らない。

## 7. 受入条件

同じlotの二重拘束、別modelへの転用、資本cap超過、未回収slash報酬、分割回収の二重報酬、zero/partial slot再利用、失効期限後の支払い、reorg後の残高不整合を拒否する。予算不足の理由は不足勘定と必要/利用可能量を保持する。

## 8. claim予約の分解と期間上限

以下は`palw_bond_formula_v2`を選ぶprofileの設計である。既存claimは受理DAAで選ばれた式を保持する。係数・期間・金額の単位はformula policyへ固定し、実測なしの値や本節の例を公開網へ自動適用しない。

```text
Σ_open L + existing_liability + accusation_reserve + unresolved_liability + L_new <= u*C
earliest_recovery = accepted_daa + W_mode

r_c = floor(r_day / (q_day*rho))
f_c = floor(f_day / (q_day*rho))
R_c = min(R_ask, r_c)  // 事前の削減許可がある種別・能力recordだけ
R_c = R_ask           // 上の条件を満たさない場合
F_c = min(F_ask, f_c)

L_weight = max(ceil(f_c*v_F), ceil(PWU*S*A*lambda_pwu/1000))
L_right = R_c
gain = R_c + ceil(f_c*v_F)
L_coll = max(loss_floor_type, ceil(gain*kappa_type/1000))
L_penalty = max(0, L_coll - L_right - ceil(f_c*v_F))
L_other = court_cost_type + da_retention_cost_type
L = L_weight + L_right + L_penalty + L_other
L >= L_coll >= gain
```

`kappa_type >= 1000`とし、`L_weight`と`loss_floor_type`を`rho`で割らない。利用者資金によるkernel/native役務では`L_right=0`、`L_penalty=L_coll`とし、利用者escrowをproducerの責任担保へ数えない。実行種別ごとの費用を使用し、通常Attemptの価格をすべてのFreePrompt・Kernel・NativeFundedへ転用しない。

Q、B、R、Fの期間上限は同じ資本・責任viewで検査する。Bは後続carrierへの帰属、Rは未払を含む。既存profileの`u=500/1000`、Panel=1の1日相当、Panel=0の7日相当は当該profileに限定する。日数はそのnetworkのDAA換算を使う。旧Panel=0という会計種別を、新規claimの検証省略経路として追加しない。

weight/right/penaltyの保持終端は`terminal + H_L`、otherは`max(accepted_daa + retention_hold_type, 全関連Court/DA sessionの終了)`とする。期間枠は`accepted_daa + W_mode`より早く回復させない。licence、早期Final、void、case終結だけで枠を前倒し回復しない。未解決責任はclose後も同時担保の合計へ含める。

Floor又は事前に固定した監査能力recordが削減予約を許す場合でも、最大権利と徴収可能損失を整合させる。MWA1の新しい共通target・当選時支払契約はRFC-0013の提案区分に従い、上式の削減許可を当選後のclipへ利用しない。

rule Eのparticipationは対応する同じ担保Cから計算し、同じ元本を複数accountや役割へ分割して上限を増やさない。未認証claimは担保上限内でも合意creditを持たない。

RPCの予約分解は`type, mode, weight, right, penalty, other, total, collectableLoss, gain, *Hold, capacityRecoveryDaa, rights`を返す。元profileに分解がないclaimでは未提供を明示し、ゼロ予約と区別する。

## 9. account単位の最低担保【提案】

別account担保を採る案では、既存のclaim予約と区別した責任合計を使う。

```text
required = max(100000 BILI, sum(residual_account_liability))
additional = max(0, required - current_account_locked)
```

最低額を各claimへ重ねて加えず、同時に残るclaimの責任を合計する。当選確率を掛けた期待損失で最大責任を割り引かない。既存の用途別lotと同じ元本を二重に算入しない。手数料escrow、期間枠、検証者bondもこの最低額で置き換えない。

不足時は新規受付を止め、受理済みclaimの反証・資料返答・責任解消は継続する。本節は提案であり、最低額を全networkの有効値として扱わない。仮に採用しても個別slashの分配は§5、モデル廃止はRFC-0017に従い、旧案の追加bountyを併設しない。

## 10. 後発立証者への実回収

Minerの計算違反と署名Panelの虚偽承認は別のoffenceとして、署名・検査範囲・元義務・対象lot・回収上限を確定する。責任期間と係争中の引出し・付替えを遮断する。適格な先行申立、証拠commitment、提出者と受取先の署名束縛を受付世代の正規planで検査し、自己受領、コピーによる受取先変更、同じ違反からの二重回収を防ぐ。帰属が争われても有効な反証を裁定する。

各回収で実bond減額とlot消費、累計回収額、発見・立証者の合計枠の増分、残差burn、決済済み識別子を対応付ける。各原子的な状態遷移をcrash後に再開でき、reorgで裁定と対応会計を同じ枝へ戻せることを残高と支出可能な出力で検証する。

### 10.1 Panelの承認鍵と責任元本の対応

承認鍵と検証bondの管理鍵が異なる場合、新profileの承認受付時に、両者が同じ責任を引き受けたことを検査する。署名対象はnetwork・規則版、元claimと実行commitment、model・profile、drawと承認の検査範囲、承認者、bondのoutpoint・lot番号・回収上限・元の責任期限を一意に定める。受付時の規則から一意に導ける項目は、版を固定したcommitmentで束縛できる。

Nodeは選択親の元本所有、用途、割当済み残高、未拘束枠を照合し、他の責任と同じ元本を二重に使わない。承認を有効にする時点までに当該責任の拘束を成立させ、以降は同じ受付記録を参照する。裁定時に現在の受取先から責任者を逆引きしない。役務報酬の支払先を承認した旧 `TB1PAY01` は支払指図に限る。旧claimとその元本へ新しい没収権を遡及させない。

公開判定では、正当な署名者と拘束lotの組だけが回収対象になることを確認する。同じactorの別lot、同じbondの別claim、支払指図だけがあるbondは対象外とする。これらの場合も元claimへの有効な反証とMiner側の個別裁定は継続する。
