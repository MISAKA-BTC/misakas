# ADR-0207 — 通常 verifier fee と有限 provider 契約の判断

Status: **Accepted fee scopes — 通常 verifier fee と provider fee-only 条件を採択** (2026-10-28)。provider の役務を有限化する別案（A/B）は未採択。ユーザーは「この報酬条件を採択して実装」と明示した。採択対象は元の有限 X3 期限までの全額保留、期限内の客観的有罪で元 poster へ全額返金、それ以外は期限後に verifier へ支払い（ServiceHold を含む）、遅い verdict による fee の遡及没収なし、既存 bond liability 維持。実装・検証・武装・配備の完了は意味しない。

## 背景

全機能を完成させて DAA 17,400 で有効化するユーザー目標がある。SettlementOnce の通常 verifier fee は責任期限の同意を欠き、material provider V1 は event-relative な契約で有限の期限を持たない。[RFC-0039](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md) に署名対象、資金移動、互換性、選択肢と試験を記す。

## 採択済みの判断と未決の判断

1. **通常 verifier（採択済み）:** fee 全額を元 claim の有限責任期限まで保留し、Clean の対象が期限内に客観的に有罪となった場合のみ fee 全額を元 poster へ返す条件を採択した。それ以外は期限後に verifier へ解除する。ServiceHold / 未解決の claim を Final や認証済みにせず、fee だけの有限責任を閉じる。既存 bond liability は二重処理せず既存 obligation として維持し、期限後の verdict で解除済み fee を遡及没収しない。reorg では選択 chain の履歴に hold / verdict / payout をまとめて戻して再評価する。
2. **provider の遅れた支払:** A「権利を維持し、事前資金・予約・新 provider の同意で custody を引き継ぐ」か、B「新契約で明示同意した期限に未払い権利を取消し、sponsor に返す」か。A は正当な権利を保つが、有限資金で無期限停滞を保証できない。B は有限化しやすいが、正当な仕事の支払喪失を伴い得る。
3. **必要な値と停止条件:** 物理支払上限 Q、保持移管資金・予約上限、finality 停止時の契約終了/継続条件。既存の X3 hard と W は流用可能な境界であり、Q や finality 保証を代替しない。

provider A は終端の引継ぎ失敗時の扱いが未決で、現時点で完成した採択案ではない。方向承認だけで署名済み cap の延長や B への自動切替をしない。B を選ぶ場合は消える権利と新 cohort を明示する。通常 verifier の案は provider と分けて判断できる。通常 verifier 条件と末尾の provider fee-only 条件は採択済み。provider 役務の有限化 A/B は未採択。現行 V1 の署名や既存 entitlement を変更しない。既存の honest/network progress 前提を維持し、無条件の wall-clock / finality 保証を要求しない。

## 適用条件

[docs/AGENTS.md](../AGENTS.md) は「重大な問題の場合のみ、新規 RFC と新規 ADR の両方、レビュー、設計責任者の明示的採択によって変更する」と定める。新しい fee 没収・期限切れ refund・責任移管は既存署名契約の意味に関わるため、本 draft をレビューして明示的に採択する必要がある。全機能有効化という目標だけを、未指定の金融条件をエージェントが選ぶ同意と扱わない。

kernel X3 の既採択規則への接続と、既存の署名済み beacon source terms の正確な読取りは別途継続する。本 ADR の判断待ちを理由に、独立した実装修正や試験を止めない。


## Independent provider fee-only decision (Accepted, implementation pending)

RFC-0039 section 6 proposes a new sponsor/provider-signed finite fee tranche bound to the exact original enrollment and fee slot. It keeps V1 service, service bond and worker entitlements unchanged. Timely objectively processed material-provider default refunds the full fee to the original sponsor; otherwise the provider receives it after the signed absolute deadline, with no later fee clawback. Pending challenges do not extend that fee deadline; later service sanctions remain.

This is separate from the accepted ordinary-verifier decision. The concrete proposed H equals the original signed enrollment_until_daa, with acceptance DAA < H < u64::MAX and the existing max_enrollment_daa cap. Sponsor and provider explicitly sign this new fee-only meaning. A root born after H or a challenge opened before H whose default is processed after H does not reopen the fee refund interval. Independent review found this implementable with default-before-release ordering and actual physical-payout/material-tail tracking. The user subsequently adopted this exact fee-only condition as recorded below; implementation remains to be completed and verified.

## Provider fee-only 採択記録 — 2026-10-28

ユーザー回答: **「この条件を採択して実装」**。採択対象は RFC-0039 §6 の新しい provider/支払者双方の署名付き fee 契約である。H は元の `enrollment_until_daa`。報酬全額を H まで保留し、対象 provider の材料提供義務違反が選択 chain 上で H 以内に確定したときのみ元の支払者へ全額返金する。それ以外は H 後に provider へ支払い、H 以前に告発されていても違反確定が H 後なら fee を遡及回収しない。claim 未成立・告発未解決でも同じ。既存の材料保持義務、service bond、worker の報酬権利は継続し、旧契約には遡及しない。

これは有限役務・未払い worker 権利の取消し・custody 移管案 A/B の採択ではない。実装完了、試験合格、測定値承認、有効化、公開配備の宣言でもない。今回の完了範囲は全項目の実装完了で、DAA 17,400 は fence 方針として維持する。
