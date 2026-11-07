# Codex への引き継ぎ 3(2026-10-18)— RFC08 の残り・RFC09・RFC10・RFC12

検証の lane(`integ/validation-r2`)から移す作業です。この lane は x8 の試験の書き直しまでで止め、残りは Codex が引き継ぎます。

最初に [codex-handover-2026-10-18.md](codex-handover-2026-10-18.md) の §0(必ず読むもの)、§3(触らない範囲)、§4(規則)を読んでください。規則は同じです。

## 1. 作業の基準点

- **base**:`origin/integ/validation-r2` の最新。x8 の書き直しが push されてから始めてください。検証の lane の最終報告が届いたら、ユーザーから開始の合図があります。
- **作業 branch**:`codex/rfc08-09-10-12`。
- **push の範囲**:push してよいのは自分の branch だけです。main・pre・`integ/*` には push しないでください。
- **必ず読むもの**
  - `docs/audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md`(各項目の現状とコードの位置)
  - `docs/audit/palw/design-reviews/consolidated-validation-2026-10-15.md` §6〜§8
  - `docs/adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md`、`docs/adr/0185-*.md`
  - `docs/design/palw/rfc-completion-audit-2026-10-17/report_ja.md` の RFC08・09・10・12 の節

## 2. 決定済みのこと(ユーザー判断、変更しない)

- **RFC08 の責任期限**:Final の下限 + `window_court`。producer が署名した commit の入力から決定的に導きます。wire と object の種類は変えません。受付の判定(08-A)は実装済みです。
- **fee**:責任期限まで保留してから支払います。有罪か default なら、未払いの fee を依頼者に返金し、担保を slash します。
- **RFC07**:PanelLicensed の Final は記録用の event だけを出し、beacon の source にも credit にも数えません。
- **RFC18**:認証済みの報酬は、producer への 1 本の支払いのままにします。
- **beacon**:seal-reveal 方式です。後から seal された source の commitment を使い、source の Final は待ちません。

## 3. タスク(この順で)

1. **RFC08 08-B:fee の保留と精算**
   - Final の時点で fee を責任期限まで保留し、期限が来たら解放します。有罪か default なら、返金と slash を行います。
   - native の各系統(funded v11、text v29、job v39、mining v39)の終端の状態を、Final 後も責任期限までは告発できる形にします。
   - `settle_once_liable_v1` の零額の終端マーカーで債務を閉じてしまう経路(`palw_monetary_material_fold_v1.rs` の 788 行付近、`palw_kernel_route_fold_v1.rs` の 953 行付近と 1064 行付近)を、保留・解放・没収・返金の状態遷移に置き換えます。
2. **RFC08 08-C:試験用の迂回口を使わない EXEC の試験**
   - `processor.rs` の 293〜305 行付近にある cfg(test) の hook を使わずに、通常の登録 → family の認証 → readiness → bond share を用意します。
   - 正直な claim が Final になり支払われること、偽った root や prefix が実際に slash されることを確かめます。
3. **RFC09:remote miner**
   - op 203 が V2/V3 の committed leaf の opening を返すようにします(reader は `palw_verify_committed_leaf_v2`)。
   - remote 側(`misaka/palw/remote/src/l2.rs` の 940 行付近、`miner.rs` の 510〜564 行付近)を V2/V3 に移し、full node と同じ root reader を使うようにします。
   - 試験:Rule E と Gate C の下で実際に 2 つに分かれた fork について、remote と full node の選択が一致すること。証拠が欠けているか改ざんされている場合は署名しないこと。
4. **RFC10:統合した状態で最後まで通す**
   - 承認済みの paid source を使い、Panel を手で注入せずに、discovery → 自動 binding → 交代 → Final まで通します。
   - blackout・離脱・再抽選・DA の係争中でも、正直な claim が永久に止まらないこと、偽った claim が中立の終了で処罰を逃れないことを確かめます。
5. **RFC12:受入試験に要る機能**
   - `scripts/drills/misaka-rfc12-acceptance.py` の 62〜78 行が挙げる、ノード側の 4 つの機能を実装します。
     - block ごとの DNS reserve の残高を読む RPC
     - import の WAL barrier の間で正確に crash させる failpoint
     - 認証済みの snapshot から paid request を作る道具
     - outpoint 単位の UTXO を読む RPC
   - 署名付きの deposit-refund CLI も実装します(985〜1000 行)。
   - そのうえで受入スクリプトを流し、二重の credit・二重の返金・消失を outpoint と状態遷移で判定します。
   - `evm` feature 付きのビルドは、ディスクの空きが 30 GB 以上のときだけ行ってください。

## 4. 報告

タスクごとに、実装したもの、試験名と pass の件数、残りとその理由を、上の 2 つの status 文書に日付付きで追記してください。試験で示していないことを「完了」と書かないでください。
