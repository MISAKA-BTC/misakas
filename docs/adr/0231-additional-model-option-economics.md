# ADR-0231 — 追加モデル限定MisakaOptionと新しい報酬・売買・移転規則

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](../rfc/0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](0232-reward-buy-and-permanent-liquidity.md)／[移行契約](../governance/model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。旧seedのpre-trade制限と市場の公開LP禁止は、新しい報酬専用の永久流動性操作とは区別する。
<!-- MOE2-ADOPTED-20261120-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0063](../rfc/0063-additional-model-option-economics.md)
- 現行所有先: [RFC-v2-0011](../rfc-v2/0011-bond-budgets-fees-and-payments.md)、[ADR-v2-0021](../adr-v2/0021-positions-and-membership.md)、[ADR-v2-0023](../adr-v2/0023-model-reward-routing-and-owner-leg.md)

## 背景

旧文書には報酬買上げ5%、owner売買手数料1%又は5%、holder間の非譲渡性、core市場の旧残高売却を許す記述が混在していた。今回の明示的変更は、買上げ50%、通常売買のburn5%・owner3%、直接送受信・譲渡、およびMisakaOptionを追加モデルに限定することを要求する。genesis・softfork指定モデルでは対象マイニング報酬の100%をMinerへ帰属させる。

## 決定

1. **論理profile MOE1を採択する。** 具体wire tag、fence、高さは未割当。新しい規範を旧署名や旧runtimeへ自動適用しない。
2. **OptionはADDITIONALだけで、登録と市場生成は原子的。** nativeのmodel ID・artifact root・有効catalogから分類する。MOE1の追加登録成功時には一意のpairと初期virtual/実reserve・在庫が同時に存在する。登録失敗時は両方rollbackし、別の市場作成・外部seed・準備待ちを要求しない。GENESIS/FORK_DESIGNATEDには市場を生成せず、残高・取引・移転・allowance・membershipも有効化しない。
3. **ADDITIONALではB=floor(500R/1000)。** Rは当該認証済みclaimの価格付け後、買上げ・Panel配分前のworker報酬。D=R-Bを元の版付き報酬planで配分する。旧20% Panel poolを採る場合に限り、R=1,000は買上げ500・Panel100・Miner400となる。
4. **GENESIS/FORK_DESIGNATEDではMiner=R。** 買上げ、Rからのowner配当、Panel/Verifier/prefix worker等への控除は0。別原資の検証役務escrowを受理前に確保し、無資金の場合は新受付を止める。検証を省略しない。
5. **通常AMMのBILI grossをburn50‰・owner30‰・残額へ分ける。** owner30‰は既存のcontributor内部分配を含む総額であり、別legを加算しない。netはgから二つのfloorを引き、保存則を保つ。Rからの自動買上げには追加BILI burn0・owner直接支払0を適用し、B全額を実reserveへ入れ、取得Positionをretireする。
6. **直接transfer・受取・譲渡・allowanceを許す。** 通常のnetwork fee以外にAMMの5%/3%を課さない。native残高が正本であり、EVMは同じ原子的遷移、revert、log、認証、allowanceを扱う。Position譲渡でBILI bondや署名責任を移さない。
7. **membershipを新holderへ結合する。** 旧保有者の残高証明や転送によるquota resetを新権利の重複に使えない。保有による現金配当や採掘報酬請求権は与えない。
8. **市場欠落は正常な受付状態にしない。** 登録済みADDITIONALのpair欠落はstate invariant違反又は移行不整合として拒否する。後から準備してopenする経路や、市場なしだからMinerへR全額を戻す旧fallbackを設けない。seed/deepenは既存市場への任意操作である。決済資源・別役務原資・原期限は事前確保し、検証等の成功条件を満たせなければ原期限の未成功終端と元原資精算へ進める。
9. **比率と支払時期を分離する。** 既存tier、発行枠、bond、成熟、hold、export capを維持し、買上げ流出も合算する。100%Minerは即時払いや新規発行を意味しない。
10. **旧core市場・残高は有効化前に清算する。** 既存権利・資金を明示的に処理し、coreのsell-only市場を新profileへ持ち込まない。無同意の没収、未知価格での強制償還、無資金の補填は許可しない。追加モデルの正式fork指定でも同じ完了条件を要求する。

## 優先関係

本採択はADR0087/0089/0091/0095/0101/0114/0124/0177/0179とADR-v2-0021/0023の、対象・配分・fee・非譲渡・core残存市場に関する旧条項をRFC0063の範囲で置換する。原文、過去実装、試験結果、旧fenceは今回の改訂記録へ保全する。旧claimの認証・価格付け・期限・支払責任を遡及変更しない。

RFC0061/0062のTB1/FOW1、全実行検証、反証優先、一回会計、未完成定理に対する有効化停止を維持する。旧文書の「経済値を維持」という一般句から、本採択の50%・5%/3%・100%・追加モデル限定を拒む優先順位は作らない。

## 期待する効果と限界

経済的分類と追加登録・一意市場の不変条件を全consumerで共有し、genesis/forkを追加モデルへ誤分類すること、市場rowを欠いた登録を成功と扱うこと、pairのuser buy停止を利用して旧買上げ義務を消すこと、旧feeや旧非譲渡APIを新profileの別経路として使うことを防ぐ。

相対売買を伴う直接移転にはAMM手数料がかからない。買上げ50%は市場価格や不正の不採算を保証しない。R全額Minerでも、別原資の検証費とMiner自身の運用費がなくなるわけではない。

## 実装と有効化

この改訂は設計採択である。native/EVMの原子的移転、経済snapshot、二重支払防止、資金予約、残高移行、core市場の清算、復旧、負荷・攻撃検証を完了したとは記録しない。RFC0063の全gateと明示activationを満たすまでMOE1を有効化しない。
