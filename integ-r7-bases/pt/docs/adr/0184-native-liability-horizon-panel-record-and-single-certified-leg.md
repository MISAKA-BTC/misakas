# ADR-0184 — native claim の責任期間、Panel-licensed Final の記録専用 event、認証報酬の単一 leg

- **Decision:** Accepted — ユーザー決定 2026-10-17(ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC08(native claim の責任と手数料)、RFC07(Panel-licensed Final の event)、RFC18(認証済み報酬の支払い)。
- **Implementation:** RFC07 は `integ/validation-r2`(`KernelFinalReadV1::panel_record`、`panel_record_v1`)。RFC08 は 08-A(4 family の admission sourcing)と 08-B の native job v39(`palw_native_liability_horizon_v1`、kernel route aux 144)を `integ/validation-r2` に実装。text v29・funded v11 の保持と post-Final 告発は未実装。RFC18 は変更なし。
- **Verification:** core 単体のみ(保持・解放・返金・post-Final 有罪化、aux 144 codec、fingerprint 不変)。実ノード試験は未実施。
- **Activation:** なし(全 fence dormant、t12 identity 不変)。
- **Related:** [外部監査の状況](../audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md)、ADR-0185。

## 決定

1. **RFC08:** native claim(funded v11・text・job v39・mining v39)の責任期間 = Final の下限 + `window_court`(他 lane の責任期間と同じ式)。producer が commit 時に署名する。手数料はその期間まで保持し、満了で支払う。有罪または default のとき、未払いの手数料は requester へ返金し、担保は slash する。
2. **RFC07:** Panel-licensed Final は、認証済み(rooted な ledger 行から読む)の記録専用 event を出す。beacon source にも credit にも数えない(`event` には入れず、`panel_record` に置く)。
3. **RFC18:** 認証済み報酬の解放は producer への単一 leg のまま。検証者には既存の reporter share と検証手数料で支払う。

## 追記 2026-10-18(ユーザー決定)

- **RFC08 の horizon は導出する。** Final 下限 + `window_court` を、producer が署名した commit 入力(とその時点の ruleset)から決定論的に計算する。wire・object kind は変更しない。producer の署名は式の全入力を覆うので、horizon は署名済み入力で確定する。新しい dormant fence の下で、fence 以後に commit された claim だけが対象。

## 実装記録 2026-10-18(設計変更なし)

- **新しい dormant fence `palw_native_liability_horizon_v1`**(全 preset で `None`、`palw_settlement_once_v1` 以下を要求、state params の mirror は borsh skip で identity・fingerprint 不変)。fence 以後に commit された claim だけが対象。
- **native job v39:** Final で producer への手数料を payout queue に入れず、aux 144(`0x01 ‖ family ‖ job`)に保持する(受取人、額、requester の返金先、horizon = 署名済み deadline + `window_court`)。bond budget の予約も horizon まで開いたまま。closing tick が horizon 経過かつ未決の告発(pending demand・proof owner)がない行を解放し、Final が行うはずだった支払い・native income・予約 close を行う。
- **post-Final の告発:** court(proof owner)・公開 DA request・DA default を、保持中の Final に horizon まで受け付ける(各 clock の上限は horizon)。`NativeJobBookV39::convict_after_final` が Final の settlement を有罪に置き換え、保持中の手数料を requester へ返金し、reserve を pre-Final の有罪と同じく回収し、finder に支払う。証拠は family 既存の court・DA のみ(PRINCIPLES §8)。
- **Master / mining:** 手数料が保持中の component がある Master は close しない(mining claim は component の horizon 後にのみ成立)。
- **kernel 修正(バグ):** paid claim の Final・default が EXEC hold で延長された liability を paid lifetime の値で短縮していた(`max` に修正)。root の署名済み lifetime が paid lifetime より長いと EXEC の一貫性検査で tip が拒否されていた。
- **残り:** text v29・funded v11 の保持と post-Final 告発、mining receipt の直接の vesting(現状は Master の close 待ちで間接的に horizon 後)。
