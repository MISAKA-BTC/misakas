# モデル階層と不変な identity

> 採用済み条項の読解用統合版。新規規則・実装検証・本番有効化ではありません。[状態](../../STATUS.md) / [凍結](../../governance/design-freeze.md)。

genesisと有効化された年次カタログで指定されたModel ID + 固定artifact rootは1.0の階層、後から登録したモデルは登録時から審査なしで0.3です。自動昇格・審査・更新・失効・試行手数料はありません。1.0への経路は年次フォーク指定だけです。

既存のModel ID/rootを上書きせず、カタログは追加のみ。registryのActiveなどは運用状態であり、階層とは別です。モデル公開・保持者数・取得可能性・共有状況をモデル単位で審査しません。

全階層でclaimごとの認証・courtを維持します。年次指定は政策的決定で、公開性・独立検証可能性の技術的証明ではありません。モデル改善protocolを延期しても通常の不変登録を廃止しません。

## 出典と適用条件

[ADR-0175](../../adr/0175-registered-models-are-permanently-immutable.md) / [ADR-0179](../../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) / [RFC-0017](../../rfc/0017-model-eligibility-examination-and-renewal.md) / [RFC-0019](../../rfc/0019-annual-model-upgrade.md)。

[パラメーター台帳](../../governance/parameter-register.md) / [mainnet後への延期](../../governance/post-mainnet-scope.md) / [仕様索引](../README.md)。
