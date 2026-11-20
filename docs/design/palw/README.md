# PALW 詳細設計

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — TB1乱数とFOW1コア採掘値

**ADOPTED_DESIGN_NOT_ACTIVATED**。今回のユーザー指示により、新規PALW採掘の方式をFOW1＋TB1へ統一する。コア推論の採掘値zと補助receiptを区別し、登録系列の非衝突定理・複数試行費用条件・完全検査対応を採掘適格性に要求する。未完成のQwen定理や参照fixtureを本番合格としない。

本書の旧source・型・抽選式・実装状況は旧profileの記録として読む。FOW1/TB1の新規実装には0061/0062の型・purpose・固定点・失敗終端を適用し、旧記述をfallback経路として実装しない。

[TB1の正本](../../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1の正本](../../rfc/0062-palw-function-output-and-work-admissibility.md) / [用途・置換・移行](../../governance/palw-tb1-fow1-transition.md) / [全旧記述の適用台帳](../../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。新profileでは本節とこれら後続規範を優先する。以下の旧方式・旧Status・原文収録は、その版の履歴又は本改定で明示的に継承する範囲であり、新規受付のfallbackではない。旧受理claimの署名・責任・原期限は保持する。設計採択、native実装、定理完成、公開activationを区別する。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

| 分野 | 資料 |
| --- | --- |
| 経済・bond | [economics](economics/README.md) |
| 検証・court・beacon | [verification](verification/README.md) |
| EXEC・資源・remote | [execution](execution/README.md) |
| モデル・kernel・onboarding | [models](models/README.md) |
| fork choice・finality・IBD | [consensus](consensus/README.md) |
| 統合状況・readiness | [integration](integration/README.md) |
| 利用者ツール(CLI・RPC) | [tooling](tooling/README.md) |
| genesis・IR・kernelの詳細 | [genesis8](genesis8/) / [tir](tir/) / [vm](vm/) |

画像・動画経路とモデル改善は **mainnet後へ延期・当面無効**です。旧「全RFC完成」の記録を理由に再び初期mainnetの必須対象に戻しません。[正式方針](../../governance/post-mainnet-scope.md)。
