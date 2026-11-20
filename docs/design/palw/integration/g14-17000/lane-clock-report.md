# lane CLOCK 報告(2026-10-26、後任)

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

本書の旧source・型・抽選式・実装状況は旧profileの記録として読む。FOW1/TB1の新規実装には0061/0062の型・purpose・固定点・失敗終端を適用し、旧記述をfallback経路として実装しない。

[TB1の正本](../../../../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../../../../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../../../../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../../../../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

branch `claude/g14-clock2`(`d8aded6de` の上)。fence はどれも未武装、preset は未変更。

## 方式(B-1)
前任の選択を尊重: **X3(`palw_claim_hard_deadline_v1`)を kernel route に広げる**。`palw_court_clock_v2` は legacy V2 dispute record 用で `palw_legacy_public_filer_v1` を前提とするため slice に入れない。
- X3 の高さ以降に commit された kernel claim は、Final 時・pre-Final default 時の `liability_until` を `committed + 3,120 + court_deadline + proof_grace` 以上に保つ(`misaka_palw_kernel::claim_clock`、consumer が load ごとに注入、root・row 外)。notice a+3,120(inclusive)内の告発は裁かれ、horizon は a+10,000 を超えない。reservation・保留 Final 報酬・signer lock は horizon に従う。
- fence 未満・注入なしは historical horizon と byte 同一。
- slice の X3 entry は probe(a+300)ではなく `palw_x3_decided_policy_v1`(Δ 512/512/512/120/360、c_retain 36,058 sompi/DAA、ユーザー承認の目安 2026-10-22、実測で置換)を入れる。

## slice の fence 一覧(`PALW_T12_KERNEL_SLICE_FENCES_V1`)
audit_1004 / gen_range_twin / model_court_window / receipt_spend_v4(int-13、15,500 のまま)、probabilistic_constraints、kernel_route_panel_one、**provider_court_v1(新、M-1 の告発者・claim ごと 4 件上限)**、**bond_budget_v1(新、統括指示: settlement_once の前提。lane VAL の承認済み V3 目安 policy)**、claim_hard_deadline_v1(決定 policy)、settlement_once、Gate C、g14_route_required。full list にも bond_budget を settlement_once の前に追加(LIST 38)。

## M-1 費用の不等式
`runs × dismissal_fee × max(check+challenge+liability, 3,150) > collateral + reward + 外部利得`。interim で 64 × 0.1 MSK × 3,150 = 20,160 MSK > 1,505 MSK(13.4 倍)。前提: DAA あたり 1 block。台帳 `docs/governance/parameter-register.md` に記載。

## 試験
- `palw_t12_full_activation_16000` 9/9 通過。新規 `the_kernel_slice_puts_the_kernel_route_on_x3s_clock`: slice 一覧そのものを 17,000 で武装し、X3 決定 policy・provider court・bond budget の高さ、clock floor、承認済み 500 MSK での 64 run 不等式、bond budget の validation 通過、全体 validation が前提不足ではなく arming record 不在(`PALW_KERNEL_ROUTE_ARMING_RECORDS_V1` 空)だけで拒否されることを確認。
- censorship clock 単体試験 通過。
- e2e `g14_clock_*`(2 本)は **失敗**: `seats()` の interim assignment が無い(`g14_kernel_route_e2e.rs:840`)。既存の `g14_f03k2_panel_one_t05_*` 3 本も同じ箇所で失敗するため、統合 head の Panel=1 world fixture の問題(本 lane の変更は clock 未注入時 no-op)。

## 残件
- Panel=1 world fixture の修復と `g14_clock_*` の再実行。正直・嘘(Final 前後)・沈黙・64 run 飽和下の外部告発の e2e は未完。
- M-5: e2e harness を slice 一覧そのもの・Gate C 75/41・beacon k=3/D=200 で駆動する部分は未着手(unit 側のみ)。
- F02: actor/turn ごとの duty 時計、`inclusion_delay_daa` の claim 単位適用は未実装。
- reward liability horizon fence は slice に未追加。
