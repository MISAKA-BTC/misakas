# パラメーター台帳 — 今回の数値変更は0件

初期台帳は RFC-0016 / 0017 と ADR-0176 / 0177 / 0179 の主要値です。他領域の全変数を網羅したとはしません。未掲載値は出典・条件・承認を登録してから調整します。これは node 設定ではありません。[機械可読版](parameters.json)

| キー / 変数 | 基準値 | 単位 | 状態・出典 |
| --- | --- | --- | --- |
| `alpha` / α | 1.5 | 無次元 | 確定設計値。今回変更なし。[出典](../adr/0177-model-bond-allocation-without-availability-consensus.md) |
| `average` / N_30 | 30日相当 DAA | DAA | 確定設計値。今回変更なし。[出典](../rfc/0015-panel-free-permissionless-verification.md) |
| `hold_panel0` / W_hold(Panel=0) | 7日相当 DAA | DAA | 設計基準。今回変更なし。[出典](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) |
| `hold_panel1` / W_hold(Panel=1) | 1日相当 DAA | DAA | 後続改定の設計基準。今回変更なし。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `tier_approved` / c_m 指定階層 | 1.0 | 係数 | 現在の設計値。今回変更なし。[出典](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| `tier_registered` / c_m 登録階層 | 0.3 | 係数 | 固定設計値。今回変更なし。[出典](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| `registered_tier_cap` / κ_registered | 300‰（設計値） | 総枠比率 | 未承認という原文状態を保持。[出典](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| `sigma` / σ | 未確定 | 倍率 | 測定で決定。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `ticket_tilt` / k（補助 ticket） | 未確定 | 無次元 | 測定・導入範囲確認。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `reissue_start` / φ / pace margin | 未確定 | 割合 / 無次元 | 測定で決定。[出典](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| `bond_caps` / Q/B/R/F・予約・burst | 導出対象 | 各予算の単位 | 組合せ評価・本番値未確定。[出典](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) |
| `entrant` / ε | 0（tier fence下） | 総枠比率 | 旧審査廃止に伴う決定。[出典](../rfc/0017-model-eligibility-examination-and-renewal.md) |

## 調整しても外さない条件

**α:** 超線形配分という構造を廃止する変更は通常調整ではない。

**N_30:** 立上り・短期投入・退出後の履歴を評価。

**W_hold(Panel=0):** 共通履歴・残存責任・burst を維持。

**W_hold(Panel=1):** 一律7日と混同せず、最早再利用と担保解放を分離。

**c_m 指定階層:** 二階層・identity 不変を維持。

**c_m 登録階層:** 自動昇格なし。制度の廃止は設計変更。

**κ_registered:** 縮小分不発行・再配分なし。上限無効化は通常調整でない。

**σ:** 総権利・個別cap・登録tier上限を迂回しない。

**k（補助 ticket）:** DAGや認証の k と区別。導入自体を数値のみで承認しない。

**φ / pace margin:** 時計・他モデルの残り権利を保護。

**Q/B/R/F・予約・burst:** 同額元本・同mode・同期間の共通上限と責任を維持。

**ε:** 旧審査用枠の復活は設計変更。

RFC-0016 の旧 carry 上限 κ と RFC-0017 の登録総枠 κ は別の量です。台帳上で区別し、コード名を今回変更しません。

調整には旧新値、単位、対象、根拠、影響、試験、承認、適用条件を残します。既存の固定値も自動的に自由変更可能とはしません。[凍結規程](design-freeze.md)

画像・動画・モデル改善の `DISABLED` は [リリース方針](post-mainnet-scope.md) であり、単なる変数調整として自由に true にできる設定ではありません。
