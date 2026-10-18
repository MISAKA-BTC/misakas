# ADR-0183 — G14 裁定容量・Panel 下限・裁定保持期間・検出方針(ユーザー決定の記録)

- **Decision:** Accepted — ユーザー決定 2026-10-16〜17 の実装記録(ADR-0181 の手続きによる記録)。
- **Scope:** G14 の裁定容量、Panel の最低担保、court を要する class の trace 保持期間、検出方針の claim gate。
- **Implementation:** branch `integ/g14-court-capacity-r2`(dormant fence。全 preset で None、t12 の identity は不変)。
- **Verification:** targeted test(`g14_court_capacity`、`g13_panel_liability`、`g14_detection_gate`、node `lg14a_legacy_filer_e2e`、`g14_kernel_route_e2e`)。全体 suite は未実行。
- **Activation:** なし。各 fence は測定記録と別途の有効化決定を要する。
- **Related:** [G14 完全性証明 §11](../audit/palw/design-reviews/g14-completeness-proof-2026-10-16.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)。

## 背景と決定

1. **裁定容量(`palw_court_capacity_v1`)。** 1 手の予算は別枠にする。court を要する claim には、最初の 1 行分の予約 pool を claim の寿命中 1 回だけ割り当てる。受理数は hard-deadline bucket ごとに M = ⌊reserve/2⌋ 件で上限を設け、超えた claim は skip 可能な `CourtClaimAdmissionFull` で受理しない。H より 1 service window 以上前に受理されたのに裁定されなかった行は無報酬とする(Final 後は Final を取り消し、vesting を burn する)。
2. **Panel の最低担保。** G-13 が有効な間は、最低 seat 担保を 240,000 MSK とする(測定した健全時の最小値 238,119 MSK に基づく)。draw、bind、room、activation、seating はすべて同じ下限関数を読む。
3. **trace 保持期間。** court を要する(fused-attention)V2 Attempt は、licence から 4W + W_disclose 以上の保持を約束する(t12 では 14,568 DAA)。約束しない Attempt は報酬対象にしない。
4. **検出方針(`palw_detection_policy_v1`)。** claim gate は、その claim の実際の保持期間に対する裁定予算を読む。公開検証されていない kernel claim の種類(pipeline、typed Composite/Retrieval)は受理しない。

## 採用理由・代替案

いずれもユーザー決定である。値は測定記録から導出した。決定にない値は作っていない。

## 帰結

暗号学的な計算証明は使っていない。ピーク時の Panel 容量は未解決である(ピーク時に必要な担保は 1,696,051 MSK)。
