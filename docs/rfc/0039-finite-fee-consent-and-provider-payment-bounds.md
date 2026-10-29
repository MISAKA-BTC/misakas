# RFC-0039 — 通常 verifier の fee consent と provider の有限支払・保持契約

Status: **Accepted fee scopes — 通常 verifier fee と provider fee-only 条件を採択** (2026-10-28)。provider の役務を有限化する別案（A/B）は未採択。ユーザーは「この報酬条件を採択して実装」と明示した。採択対象は元の有限 X3 期限までの全額保留、期限内の客観的有罪で元 poster へ全額返金、それ以外は期限後に verifier へ支払い（ServiceHold を含む）、遅い verdict による fee の遡及没収なし、既存 bond liability 維持。実装・検証・武装・配備の完了は意味しない。

## 問題と範囲

全機能を完成させて testnet-12 DAA 17,400 で有効化する目標に対し、SettlementOnce は通常 verifier fee と material provider fee の認証済み有限責任条件を欠く。通常 verifier の draw deadline は本人が同意した責任期限ではない。現行 provider V1 の署名は、全権利終了・最後の実支払 + W・その finality までの event-relative 保持を明示する。現在の署名の意味を有限期限へ変更しない。

この RFC は不足する責任と権利の扱いをレビューする案である。既存の paid-V2 trap、native39 issuance の例外、kernel X3、source producer が署名する既存 beacon terms を一括置換しない。モデル単位審査や計算の暗号学的証明を導入しない。claim の認証・court・Gate C は維持する。

## 1. 通常 verifier fee consent

新しい独立の versioned consent を、既存の署名対象を変更せずに追加する。署名対象は `version, network, ruleset, claim, verifier_bond, draw_slot, poster_bond, fee, liability_until, material_until, forfeiture_policy_digest, nonce, clean=true`。signature を除く canonical bytes と専用 domain を使う。chain、claim、slot、金額、政策の変更は署名不一致となる。consent のない旧オブジェクトを新規則への同意とみなさない。

- `liability_until` は原 claim の受理 DAA に束縛された責任期限を覆う。通常 kernel では採択済み X3 の hard を用い、後の観測時刻で延長しない。paid enrollment はその署名済み lifetime を維持する。
- `material_until` は実際の材料契約の期限で裏付ける。verifier の同意だけで provider の署名済み役務を延長しない。
- 新しい consent 自体を明示的な Clean fee-attestation として扱い、その受理時に一つの draw slot の fee 全額を poster の既存 escrow から専用 held row へ移す。同じ transition で kernel の slot を paid とし、後の slash や旧 attestation がその fee を再び使わないようにする。満額を確保できない consent は状態を変えず拒否する。Gate C・claim Final・credit を作らず、他 claim の保留額と混ぜない。obligation は claim/verifier/slot ごとに一意。
- 提案する標準形は、有限の責任期限まで fee を全額保留し、期限内の客観的な有罪 verdict が Clean attestation に結び付く場合のみ fee 全額を元の poster へ返すこと。それ以外は期限後に verifier の支払権利を解除する。対象 claim が ServiceHold / 未解決でも、この fee の責任期限を延長しない。対象 claim を Final / SecurityCertified にしたり、その credit や producer 報酬を解除する処理ではない。
- Unavailable / 未回答 / service 不足 / 再編による一時的不明を fraud と扱わない。期限後の verdict で解除済み fee を遡及没収しない。既存 signer bond liability は、その既存規則と obligation のまま別に一度だけ処理する。reorg では fee の hold / verdict / release と payout を同じ履歴に巻き戻し、選択 chain 上で期限内だった verdict を再評価する。壁時計では判定しない。
- **採択済み（通常 verifier の条件）:** この「有限期限で解除 / 期限内の有罪 Clean の fee 全額を poster へ返す」組合せは2026-10-28に明示採択された。paid trap の probe 値を承認済み値として流用したものではない。

状態は `AcceptedHeld → Released | Forfeited`。書込み・資金移動・once marker は同じ transition で実行する。期限経過だけで Gate C 認証を作らない。pruning / restart / IBD / reorg でも同じ署名済み条件と terminal 判定を復元する。

## 2. Provider V2 の署名済み上限

V1 契約は変更せず、V2 は少なくとも現行 enrollment 全項目と `service_until, last_admissible_payment_daa, material_until, late_payment_policy, fee_release_policy_digest, queue_reservation_id` を署名対象とする。provider 全員と sponsor が同じ canonical payload に署名する。原 claim、依存集合、EXEC plan、材料 bytes 上限、全 offer/fee/collateral を維持する。未知 version や未知 policy は拒否する。

候補となる期限導出は次のとおりである。`a` は認証された受理 DAA、`H` は既存 consumer の最大責任期限、`Q` は**新たに承認・実測が必要な**物理支払遅延上限、`W` は既存 policy の材料保持 tail。

- `service_until >= H`。
- `last_admissible_payment_daa = service_until + Q` (checked arithmetic)。
- `material_until >= last_admissible_payment_daa + W`。
- すべての実依存 claim の hard と material 期限が署名済み上限内に収まるものだけを受け入れる。
- queue reservation は仕事を受ける前に確保し、全ての native payment consumer が同じ予約を課金する。平均 throughput の見積りを最悪遅延の保証にしない。

**finality は別条件である。** DAA 上限を設定しても、その時刻までの finality は保証されない。finality 未成立で安全に材料を捨てること、未解決の支払を確定済みにすることは、本案から導かれない。以下のどちらを採るかと、その finality 条件を決める必要がある。

## 3. 遅れた支払の選択肢

| 案 | 期限時に未払いの権利 | 利点 | 費用・未解決条件 |
| --- | --- | --- | --- |
| A: 権利を維持し、事前資金と有限予約で保管を引き継ぐ | 権利を消さず、支払 escrow と材料 custody を明示された次の契約へ移す。元 provider の責任を本人の署名外で延長しない | 正当な報酬を期限だけで失わせない | 次の provider の明示同意、移管の実 bytes と認証、独立 reserve、移管完了前の旧契約責任、移管不能・finality 停止時の処理が必要。有限資金で無期限保持は保証できない |
| B: 期限で未消費の支払枠を取消し、sponsor に一度だけ返す | **新 V2 で事前同意した**未払い権利のみ期限時に消し、対応 escrow を返す。支払済み fee と既存 V1 権利は取消さない | 有限期間・有限資金の契約にしやすい | 正当に働いても遅延で支払を失う可能性がある。consumer/provider 双方の明示同意、ネットワーク遅延と当事者違反の区別、実払済みと refund の競合排除が必要。finality 前に取消しを不可逆とみなせない |

A は現時点で完成した採択案ではない。引継ぎ失敗時に署名済み上限を超えて旧 provider を拘束することも、自動的に B の権利取消しへ切り替えることも認めない。事前資金・受入枠・最後の backup の署名済み期限内に義務を完了する条件と、完了できない場合の具体的な経済状態が必要である。方向承認だけで本番へ進めない。

B を選ぶ場合は、期限で失われる新 V2 の未払い権利を明示し、取消しと refund を同じ transition で行う。どちらの案も既存の honest/network progress 前提の下で評価する。無条件の wall-clock 進行や finality 保証を新たに要求するものではない。通常の reorg 保護を維持し、finality 前の変更は取り消せる状態として扱う。稼働中の V1 entitlement を B に移さない。

**判断を分離する:** 通常 verifier の有限 fee 契約は独立してレビュー・採択できる。provider は Q の測定と、A の終端失敗時または B の権利取消しの具体的扱いが閉じるまで未採択とする。

## 4. 旧 cohort と新 wire

新 fence より前に受理した claim/fee/provider contract は元の規則を保つ。新 consent と V2 enrollment は fence 前に新効果を持たない。旧 wire tag の意味を変えず、carriage、署名 context、SDK builder、検証、state codec と restore が version を識別する。新 tag/aux prefix の番号はレジストリと照合して実装時に割り当てる。本 draft は番号を予約・再利用しない。

新 cohort でも、既存 V1 provider の同意が新しい期限を覆わなければ admission を拒否する。自動更新、暗黙の同意、TTL の無断延長は行わない。新 consent を受け取れない場合に旧 permissive path へ fallback しない。

## 5. 実装と検証の完了条件

1. 未決政策と値を ADR に採択し、正確な wire/署名仕様をレビューする。
2. builder → signature → admission → held state → verdict → release/refund → physical queue の全経路を接続し、資金保存と一回性を検査する。
3. 誤署名、別 network/claim/slot、金額置換、旧 cohort、境界 DAA、overflow、支払/refund 競合、重複 consumer、容量不足、provider 未同意を検査する。
4. 正当な支払、期限内有罪、混雑、停止、遅れた finality と recovery を複数 node で実行し、restart/reorg/pruning/IBD 後の権利と会計一致を示す。
5. queue/保持 byte・CPU/RAM・帯域・最悪遅延の測定と独立 review を pin する。実装・試験・測定の完了後にのみ blocker と readiness を更新する。

参照: [設計凍結手続き](../governance/design-freeze.md)、[既存の未 source 調査 §4.6](../design/palw/integration/rfc-0006-0007-0008-0012-remaining-work-2026-10-15.md)、[全機能 readiness](../design/palw/integration/g14-17000/full-activation-17400-readiness-2026-10-28.md)。


## 6. Independent provider fee-only contract (Accepted, implementation pending)

This alternative leaves V1 service, service collateral, worker entitlements, replacement duties, physical-payment tails and finality unchanged. Only the provider's own prepaid fee becomes a separate finite liability tranche. It is not a NoLiableClaim exemption, nor a finite deadline for the underlying service.

The provider and original sponsor sign a new version binding network, ruleset, original enrollment/consent ID, exact fee context and provider slot, original sponsor/refund recipient, prepaid amount, absolute fee deadline H, policy digest and nonce. The deadline is an explicit new financial term, not a fabricated original claim X3 horizon. This binding works before root birth: enrollment identity, fee slot and escrow already exist, and no unknown claim acceptance time is substituted. No already queued or physically paid V1 fee may be migrated retrospectively. New versions and old rights remain distinguishable.

Proposed complete money rule: reserve the whole fee in its own held row through H. An objective material-provider default actually processed on the selected chain at or before H returns the whole held fee to the original sponsor. Otherwise release the whole fee to the provider after H, including when root/service/worker payments remain unresolved or the root never materializes. A later default does not claw back this fee, while existing service bond sanctions continue. The fee is compensation for the original enrollment's service undertaking; this proposal does not condition its existence on eventual claim birth. Reorg rolls back default, held amount, release and payout together.

The exact default authority is the existing monetary-material fold transition: an unanswered PalwMonetaryMaterialChallengeV1 past its reply deadline triggers PalwSettlementLegV1::MaterialProviderDefault and sets provider.charged. It is not a producer Convicted or ProducerDefault event, nor an unrelated ProviderLiableDefault event. A new durable row must bind fee context, provider, original enrollment generation and the actual default processing DAA; the existing charged boolean lacks that timestamp. Replacement must not attribute a previous provider's default or fee to its successor.

The fee cutoff is the actual default processing DAA, not challenge opening time. A challenge still awaiting its reply deadline at H does not postpone the fee release. Such a later default still charges the service bond but does not recover the fee. This late-discovery limitation is a NEW proposed financial decision, explicitly requiring adoption. A challenge whose reply deadline is H can first default at H+1, so it is outside the refundable interval. Existing provider fee queueing currently precedes default processing in the tick; implementation must evaluate and record all decisive timely defaults before evaluating fee release.

The concrete proposed deadline is H = the original enrollment terms.enrollment_until_daa. The new sponsor/provider signatures explicitly adopt that existing admission deadline as the fee-only refundable interval; the old V1 signature alone does not. Require enrollment acceptance DAA < H < u64::MAX and retain the existing max_enrollment_daa validation. No new numeric duration is proposed. A root born after H cannot reopen the fee refund interval, even though the service and service-bond obligations continue. A replacement does not move an earlier enrollment fee deadline or change its recipient. This mapping is a new financial meaning requiring explicit adoption; it is not an implication of the old enrollment window.

Under the combined settlement predicate, R is the exact prepaid fee, K and collateral are the dedicated held-fee tranche, the fee's signed hard deadline/lock/retention are H, and fee evidence is retained through H. The original event-relative service bond remains a separate liability and cannot be counted again as this fee tranche. Material evidence for a fee default must remain available through H under the original healthy covenant; failure to establish that evidence retention at admission refuses the new fee contract. This explicitly narrows the NEW fee contract's clawback obligation; it does not reinterpret the old V1 payment obligation or remove its lifetime restrictions.

Required review/tests: consent identity and signatures; negative old-cohort migration; boundary H/H+1; open-but-unresolved challenge at H; replacements; conservation and exactly-once refund/payout; no-root enrollment; queue pressure and rollback; unchanged old service/bond/entitlements. Physical queue and long-lived service resource bounds remain independent implementation requirements.

Review (2026-10-28): independent review found the fee-only rule implementable with the stated late-default limitation. Required ordering is default recording before fee release; both parties sign the new fee condition; actual payout queues and their material tails remain tracked; old fees never migrate implicitly. This review preceded the user adoption recorded below.

## Provider fee-only 採択記録 — 2026-10-28

ユーザー回答: **「この条件を採択して実装」**。採択対象は RFC-0039 §6 の新しい provider/支払者双方の署名付き fee 契約である。H は元の `enrollment_until_daa`。報酬全額を H まで保留し、対象 provider の材料提供義務違反が選択 chain 上で H 以内に確定したときのみ元の支払者へ全額返金する。それ以外は H 後に provider へ支払い、H 以前に告発されていても違反確定が H 後なら fee を遡及回収しない。claim 未成立・告発未解決でも同じ。既存の材料保持義務、service bond、worker の報酬権利は継続し、旧契約には遡及しない。

これは有限役務・未払い worker 権利の取消し・custody 移管案 A/B の採択ではない。実装完了、試験合格、測定値承認、有効化、公開配備の宣言でもない。今回の完了範囲は全項目の実装完了で、DAA 17,400 は fence 方針として維持する。
