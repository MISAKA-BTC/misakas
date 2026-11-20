# モデル階層と不変な identity

<!-- MOE2-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MOE2の30%購入・20%永久流動性・50%残余

**ADOPTED_DESIGN_NOT_ACTIVATED**。新経済profileの正本は[RFC0064](../../rfc/0064-reward-buy-and-permanent-liquidity.md)／[ADR0232](../../adr/0232-reward-buy-and-permanent-liquidity.md)／[移行契約](../../governance/model-option-economics-transition.md)。ADDITIONALの対象worker報酬Rを `B=floor(300R/1000)`、`A=floor(200R/1000)`、`D=R-B-A` に分け、Bで購入したPositionの必要分をAと同じ市場へ追加し、余剰だけをburnする。全工程とDの支払を同じ原子的決済に結合する。

永久LPはnativeの非償還記録であり、withdraw・transfer・owner/管理者引出し・LP手数料請求を持たない。実資産は通常swapに使い、Aを旧locked seed下限へ加算しない。登録時の一意市場生成、coreの市場なし・Miner=R、通常AMM5%/3%・直接移転、TB1/FOW1は維持する。永久LP記録済みmodel IDのcore指定は禁止する。

旧MOE1の「50%を全額購入し、購入Positionを全retireする」は旧cohortの規則であり、MOE2の新規経路へ適用しない。以下の旧採択・原条文・測定はその版の履歴として保存する。
<!-- MOE2-ADOPTED-20261120-END -->


<!-- MOE1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 採択 — 追加モデル限定MisakaOption経済MOE1

**ADOPTED_DESIGN_NOT_ACTIVATED**。有効カタログによるGENESIS/FORK_DESIGNATED/ADDITIONALを市場と支払に接続する。二階層係数自体を変更しない。

追加モデル登録と一意な市場の作成を同じ原子遷移とし、通常の登録済み・市場未作成状態やseed待ちを設けない。genesis・有効softfork指定モデルは市場を作らず、価格付け・共通上限処理後の対象worker報酬RをMinerへ100%配分する。追加モデルのみ50%買上げ、通常AMMはburn5%・owner総額3%・net残差、直接送受信はAMM手数料なしとする。

[RFC0063](../../rfc/0063-additional-model-option-economics.md) / [ADR0231](../../adr/0231-additional-model-option-economics.md) / [適用と移行](../../governance/model-option-economics-transition.md)。新経済profileではこの後続採択を優先する。旧率・旧署名・原文引用・過去の実装試験はその版の記録として保持し、新規経路へfallbackしない。TB1/FOW1の正当性・乱数・Court・適格性条件を免除しない。
<!-- MOE1-ADOPTED-20261120-END -->

> 採用済み条項の読解用統合版。新規規則・実装検証・本番有効化ではありません。[状態](../../STATUS.md) / [凍結](../../governance/design-freeze.md)。

genesisと有効化された年次カタログで指定されたModel ID + 固定artifact rootは1.0の階層、後から登録したモデルは登録時から審査なしで0.3です。自動昇格・審査・更新・失効・試行手数料はありません。1.0への経路は年次フォーク指定だけです。

既存のModel ID/rootを上書きせず、カタログは追加のみ。registryのActiveなどは運用状態であり、階層とは別です。モデル公開・保持者数・取得可能性・共有状況をモデル単位で審査しません。

全階層でclaimごとの認証・courtを維持します。年次指定は政策的決定で、公開性・独立検証可能性の技術的証明ではありません。モデル改善protocolを延期しても通常の不変登録を廃止しません。

## 出典と適用条件

[ADR-0175](../../adr/0175-registered-models-are-permanently-immutable.md) / [ADR-0179](../../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) / [RFC-0017](../../rfc/0017-model-eligibility-examination-and-renewal.md) / [RFC-0019](../../rfc/0019-annual-model-upgrade.md)。

[パラメーター台帳](../../governance/parameter-register.md) / [mainnet後への延期](../../governance/post-mainnet-scope.md) / [仕様索引](../README.md)。
