# V+ — 物理 block の生成機会

> 採用済み条項の読解用統合版。新規規則・実装検証・本番有効化ではありません。[状態](../../STATUS.md) / [凍結](../../governance/design-freeze.md)。

RFC-0016はモデル別事前発行権、上限付き途中再発行、抽選前quote/admissionのV+を採用しています。権利は生成機会を制約し、実際の生成には計算が必要です。残り権利と検証容量を見て、個別bond予算内で権利を消費します。

RFC-0017による改定では階層係数を含む配分を使い、tier fence下で旧entrant reserve/Probation予算を適用せずε=0、登録tier全体のκに従います。旧審査・昇格を復活させません。

再発行を含む権利会計が総予算・個別cap・階層総枠を守るかは別途試験事項です。生成権利は合意weightや支払の無条件保証ではありません。測定値・有効化は別承認です。

## 出典と適用条件

[RFC-0016](../../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) / [RFC-0017](../../rfc/0017-model-eligibility-examination-and-renewal.md) / [ADR-0179](../../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) / [RFC-0018](../../rfc/0018-dag-parallel-multi-model-consensus.md)。

[パラメーター台帳](../../governance/parameter-register.md) / [mainnet後への延期](../../governance/post-mainnet-scope.md) / [仕様索引](../README.md)。
