# design / palw / verification

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

本書の旧source・型・抽選式・実装状況は旧profileの記録として読む。FOW1/TB1の新規実装には0061/0062の型・purpose・固定点・失敗終端を適用し、旧記述をfallback経路として実装しない。

[TB1の正本](../../../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../../../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../../../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../../../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

文書の適用版・対象・採用状態を確認してください。配置は採用・有効化ではありません。

[総合入口](../../../README.md) / [状態](../../../STATUS.md)。

## 資料

- [beacon-source-opt-in-reservation.md](beacon-source-opt-in-reservation.md)
- [court-model-scope-v1.md](court-model-scope-v1.md)
- [da16-transport-and-provider-court.md](da16-transport-and-provider-court.md)
- [legacy-route-g14-filer.md](legacy-route-g14-filer.md)
- [legacy-route-g14-held-da.md](legacy-route-g14-held-da.md)
- [misaka-palw-model-adjudicability-guide-v0.1-ja.md](misaka-palw-model-adjudicability-guide-v0.1-ja.md)
- [misaka-palw-slash-protocol-design-v0.1.md](misaka-palw-slash-protocol-design-v0.1.md)
- [opv-beacon-bootstrap.md](opv-beacon-bootstrap.md)
- [pesg-b-detection-bounds.md](pesg-b-detection-bounds.md)
- [rfc-0007-panel-frozen-source-subject-2026-10-11.md](rfc-0007-panel-frozen-source-subject-2026-10-11.md)
- [rfc06-memory-routing-court-v2-source-plan-2026-10-15.md](rfc06-memory-routing-court-v2-source-plan-2026-10-15.md)
- [rfc07-panel-paid-consumer-2026-10-13.md](rfc07-panel-paid-consumer-2026-10-13.md)
- [rfc08-panel-signed-lifetime-v2-implementation-2026-10-14.md](rfc08-panel-signed-lifetime-v2-implementation-2026-10-14.md)
- [shard-rfc6-10.md](shard-rfc6-10.md)
- [t12-int10-court-window-review.md](t12-int10-court-window-review.md)
- [t12-panel-backlog-1001.md](t12-panel-backlog-1001.md)
- [t12-panel-backlog-1003.md](t12-panel-backlog-1003.md)
