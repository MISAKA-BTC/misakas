# 平均元本・階層係数・登録総枠

> 採用済み条項の読解用統合版。新規規則・実装検証・本番有効化ではありません。[状態](../../STATUS.md) / [凍結](../../governance/design-freeze.md)。

Minerとclaimに実際に拘束され検証責任を負うVerifierの適格元本を、資金単位で重複せず集計します。待機bond、登録料、AMM/Position、未払予定報酬を追加元本としません。

基準の設計形は30日相当DAAの平均 `S̄_m` に対する `A_m = c_m · S̄_m^1.5`、総枠適用前の相対配分 `P_m = A_m / Σ A_j`。係数を含む同じ配分を物理block機会とcoinbaseに使い、受理blockの報酬へ二重に掛けません。

登録tierの合計がκを超えると比例縮小し、縮小分は発行せず、指定階層にも再配分しません。縮小後に再正規化して制限分を復活させる意味には読みません。整数算術・分母ゼロ・端数は出典に従います。

配分目標と実支払を分けます。資本が増えても個別bond上限・総発行は増やしません。α、平均期間、係数、総枠の既存値は今回変更していません。

## 出典と適用条件

[ADR-0177](../../adr/0177-model-bond-allocation-without-availability-consensus.md) / [ADR-0179](../../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) / [RFC-0016](../../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) / [RFC-0017](../../rfc/0017-model-eligibility-examination-and-renewal.md)。

[パラメーター台帳](../../governance/parameter-register.md) / [mainnet後への延期](../../governance/post-mainnet-scope.md) / [仕様索引](../README.md)。
