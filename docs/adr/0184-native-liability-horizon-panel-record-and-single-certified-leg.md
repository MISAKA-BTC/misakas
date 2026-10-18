# ADR-0184 — native claim の責任期間、Panel-licensed Final の記録専用 event、認証報酬の単一 leg

- **Decision:** Accepted — ユーザー決定 2026-10-17(ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC08(native claim の責任と手数料)、RFC07(Panel-licensed Final の event)、RFC18(認証済み報酬の支払い)。
- **Implementation:** RFC07 は `integ/validation-r2`(`KernelFinalReadV1::panel_record`)。RFC08 は未実装(本 ADR が仕様)。RFC18 は変更なし。
- **Verification:** 未実施。
- **Activation:** なし(全 fence dormant、t12 identity 不変)。
- **Related:** [外部監査の状況](../audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md)、ADR-0185。

## 決定

1. **RFC08:** native claim(funded v11・text・job v39・mining v39)の責任期間 = Final の下限 + `window_court`(他 lane の責任期間と同じ式)。producer が commit 時に署名する。手数料はその期間まで保持し、満了で支払う。有罪または default のとき、未払いの手数料は requester へ返金し、担保は slash する。
2. **RFC07:** Panel-licensed Final は、認証済み(rooted な ledger 行から読む)の記録専用 event を出す。beacon source にも credit にも数えない(`event` には入れず、`panel_record` に置く)。
3. **RFC18:** 認証済み報酬の解放は producer への単一 leg のまま。検証者には既存の reporter share と検証手数料で支払う。

## 追記 2026-10-18(ユーザー決定)

- **RFC08 の horizon は導出する。** Final 下限 + `window_court` を、producer が署名した commit 入力(とその時点の ruleset)から決定論的に計算する。wire・object kind は変更しない。producer の署名は式の全入力を覆うので、horizon は署名済み入力で確定する。新しい dormant fence の下で、fence 以後に commit された claim だけが対象。
