# ADR-0183 — paid claim の lock 乱数:後続 seal の commitment、最初の lock のみ固定 seed、承認済み paid claim を beacon source に

- **Decision:** Accepted — ユーザー決定 2026-10-17(F1・F2・bootstrap、ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC-0007 paid claim(`palw_claim_challenge_v2`)の checker 抽選の乱数源と beacon source の範囲。PALW 合意方式・既存数値は変更しない。
- **Implementation:** `integ/validation-r2`(`8c5124c6a`、`7962446f3`)。`PalwClaimChallengePolicyV2::bootstrap_seed`(dormant、probe と全 preset で `None`)、`misaka_palw_challenge::paid_opt_in::collect_paid_seal_reveal_seed_v6`、`paid_claim_seal_anchor_v2` / `paid_claim_bootstrap_anchor_v2`、`palw_exec_v2_credit_approval_v1`。
- **Verification:** 単体試験のみ(challenge 6/6、core 41/41 と bootstrap 3/3)。実ノード試験・drill は未実施。
- **Activation:** なし。fence は全 preset で `None`、t12 identity 不変。
- **Related:** RFC-0007、[検証記録](../audit/palw/design-reviews/consolidated-validation-2026-10-15.md) §1・§4・§5、PRINCIPLES §8(乱数としての commitment / VRF は可、計算証明は不使用)。

## 背景

RFC07 adapter 以降、beacon event を出せるのは選ばれた checker が承認した paid claim の Final だけで、paid claim の lock は Final を持つ source を要した。新しいチェーンでは最初の event を作れず、lock 後の source も無限に後続の承認を要した(検証記録 F1)。

## 決定

1. paid claim の lock 乱数は、対象 claim の commit より後に seal された claim の commitment(seal 時に内容と salt が固定、seal 期間後に公開)から作る。source は Final を要しない。承認済み Final は報酬・credit 側でのみ要求する。
2. seal 由来の seed が得られず、承認済み paid Final もまだ無い最初の lock に限り、genesis / 年次フォークで pin した固定 seed を使う。固定 seed で lock した claim は抽選が予測可能なため EXEC credit を得ない(event は出す)。
3. 承認済み paid claim を beacon source に含め、paid job の opt-in source 登録を許す(同じ policy 下のみ)。

4. onboarding / conformance beacon の窓(`PALW_ONBOARDING_POLICY_WINDOW_DAA_V1`)を、paid source claim の署名済み寿命(source 窓を含む)以上に広げる。120 → 1,024 DAA(暫定値)。120 では paid source の Final が窓に入らず beacon が UNAVAILABLE になった(検証記録 §6)。ユーザー決定 2026-10-17。

## grinding が効かない理由

対象は全 source の seal より先に commit する。seal は内容と salt を固定し、公開は seal 期間後。source 集合は seal 期間の終わりで閉じ、対象自身の producer・base job・相互 source 組は除外される。残る手段は reveal の保留だけで、それは attempt 全体の veto(deposit 没収)になり、結果の選択にはならない。前提:混ぜた source のうち少なくとも 1 つが正直。

## 帰結

pin する seed の値は network ごとのユーザー決定。fence 有効化は ADR-0181 の手続きに従う。
