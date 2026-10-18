# ADR-0187 — native 経路の G14:枠なしの公開告発、期限切れ seal の解放、mining claim の Final 後の追及

- **Decision:** Accepted — ユーザー決定 2026-10-17:G-N2 の実装値(Final 後の demand は生涯 8・同時 2、attestor の lock は seat-lock 関係を claim の reserved exposure に当てた額、責任期間は他 lane と同じ式)をそのまま採用する。最終の drill・測定で問題が出た場合は見直す。
- **Scope:** native funded v11・text29 の告発枠(G-N1)、owning39 mining claim の Final 後の追及(G-N2)、担保不足での有罪(G-14 / T-N13)。
- **Implementation:** branch `integ/g14-court-capacity-r2`。各 native lane の既存の休眠 fence の下(全 preset で `None`)。testnet-12 の identity は不変。
- **Verification:** v11 の node 試験 5 件、v39 mining court の fold 試験 4 件(G14 完全性証明 §13)。text29 と v39 の実 job の試験は harness が無く未実行。
- **Activation:** なし。各 native lane は測定記録と有効化の判断を要する。
- **Related:** [G14 完全性証明 §13](../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md)、[ADR-0183](0183-g14-court-capacity-panel-floor-retention-detection.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)。

## 背景と決定

1. **枠なしの公開告発(v11 action 19、text29 action 21)。** 自己完結の filing は、producer の operator 以外の Active bond が、seal・ticket・owner 席なしで出せる。作業料は前払いで、誤りなら没収する。producer の予約は使わない。検証と有罪は、seal の reveal と同じ関数を通る。commit-reveal は無いので、写しが先に入れば finder share はその提出者に渡る。producer は同じように有罪になる。
2. **期限切れ seal の解放(v11 action 20)。** reveal 期限を過ぎ、protected DA を 1 度も強制していない seal は、job の terminal 前に席を返す。未使用の guarantee は返還しない。protected DA を強制した seal は席を保つ。これで job あたりの protected 回答は 2 × 4 のままになる。
3. **mining claim の Final 後の追及(v39 action 80)。**
   - 資料:component の資料を claim の責任期間まで保持する(collection 53 を拒否)。責任期間 = 受理 + 認証の寿命 + receipt の寿命 + prosecution window。
   - 告発:自己完結の filing、または Final 後の demand(claim あたり生涯 8 件・同時 2 件、回答期限は既存の `da_answer`)。
   - 未回答:default は有罪とする。
   - 有罪の効果:producer の reserved exposure の徴収(残高まで)、receipt・safe weight・certification の取消、Reproduced attestor の lock の charge。
   - Gate C:開いている demand は pending である。
4. **attestor の lock。** native mining claim に Reproduced と attest した bond は、seat-lock 関係(ADR-0151 D1/D4)を claim の reserved exposure と quorum k に当てた額を lock する。
5. **veto。** RFC-0018 U-6 は採用しない(`veto = 0`)。

## 採用理由・代替案

- G-N1 の監査の選択肢は 3 つ:期限切れ seal の解放、枠なしの自己完結 filing、枠の価格による増加。前の 2 つを採った。後の 1 つは経済値の変更になる。
- producer の前払い回答上限(`check_native_protected_capacity`)は変えていない。生涯の上限を外す案は、producer の義務を無限にするので採らなかった。
- G-N2 の監査は「negative veto の人数で客観裁定を代用しない」としている。veto 0 で、verified filing と default だけで裁く。

## 値(2026-10-17 ユーザー採用。drill・測定で問題が出れば見直す)

| 値 | 現状 | 根拠 |
|---|---|---|
| Final 後の demand 数(8 / 同時 2) | 採用 | protected seal の 2 × 4 に合わせた。測定していない |
| attestor の lock 額 | 採用(seat-lock 関係 × reserved exposure) | 既存の関係の適用。gain の代わりに exposure を使っている |
| 責任期間 | 採用(既存の値の和) | 新しい値は導入していない |
| 試験の horizon(challenge 300 など) | 試験だけ | probe の値は node 試験に短すぎる |

## 追記(2026-10-18):text29 の料金式の欠陥

- text29 の `paid_restore` の二次の料金式は、block あたりの court work 上限を常に超え、text29 は到達不能だった。同じ単価の線形の式に直した。
- Fund は、paged29 の同時義務が固定の `seal_ttl` の下で物理 service を超えるため、まだ成立しない。値の判断を待つ(G14 完全性証明 §13.5)。

## 追記(2026-10-18、ユーザー決定):carriage の period

- Final27 の予約 scheduler(`NativeCarriageServiceV22::reserved_scheduler_v30`)の carriage period を 2 DAA から 1 DAA にした。物理 service は 2 倍になる。
- `seal_ttl`、締切、責任期間は変えていない。Final27 が有効な所だけに効く(全 preset で休眠)。Final27 の外の probe service は period 2 のまま。
- text29 と v39 は同じ service を使うので、両方の Fund の同時義務の検査に効く。

## 帰結

- 暗号学的な計算証明は使っていない。
- 残る限界:ordinary lane と discovery ticket の先取り(価格だけで抑えている)。text29 と v39 の実 job での試験。node 水準の担保不足(正規の経路では作れない)。

## 追記(2026-10-18、ユーザー決定):v39 の Master・mining の料金は運ぶ bytes で計る

- v39 の Master(65、66、67〜70 の metadata upload)、mining(73、75)、component の Fund の restore 料金から、固定の envelope(65,536 byte、66 の 131,072 byte と component ごとの 32,768 byte、prior component ごとの 16,384 byte、finalized の 32,768 byte)を外した。action が実際に運ぶ・restore する bytes(登録 body、source、book、post)だけを数える。
- block あたりの court work 上限(2^30、通常 2^29)と単価(16,384 / 8,192 work/byte)は変えていない。休眠の `palw_native_execution_v39` / `palw_native_mining_v39` の中だけで効く。
- これで fixture の Master 65 は 97,027,072 work 程度になり、通常の admission の上限に入る(G14 完全性証明 §13.7 の表の前提が変わった)。
- Master の audit(`derive_native_master_final_audit_v39`)の restore extent の再計算も同じ式にした。
