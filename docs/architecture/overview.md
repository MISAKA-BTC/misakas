# MISAKA の構成 — 採用済み設計の入口

**このページは設計の構成であり、全機能が稼働済みとの主張ではありません。** [状態](../STATUS.md) / [設計凍結](../governance/design-freeze.md)。

利用者・アプリケーション → CLI / RPC → ノード → PALW の仕事・claim・検証・裁定・会計 → DAG 上の合意・決済、という責任の分離で読みます。

| 論点 | 読む場所 |
| --- | --- |
| モデル identity・二階層 | [モデル階層](../spec/palw/model-tiers.md) |
| 平均元本・階層係数・登録総枠 | [配分](../spec/palw/model-allocation.md) |
| 個別元本の Q/B/R/F・拘束・責任 | [bond 予算](../spec/palw/bond-budgets.md) |
| 物理 block 機会の V+ | [ブロック機会](../spec/palw/block-opportunity.md) |
| claim の認証と court | [claim 認証](../spec/palw/claim-certification.md) |
| DAG・Rule E・BFT validator 不使用 | [合意](consensus.md) |
| EVM・PQ・インターフェース | [EVM](evm.md) / [PQ](pq.md) / [リファレンス](../reference/README.md) |

## 初期 mainnet と後段機能

RFC-0003 の画像・動画経路と RFC-0004 のモデル改善は、**mainnet 後へ延期・当面無効**。共通のテキスト・TIR・検証・登録・会計は保持します。[正式な範囲](../governance/post-mainnet-scope.md)

[整理前の詳細な構成記録](../history/documentation/2026-10-17/architecture/overview.md)は、その時点の図・コード参照・決定対応を保存したものです。旧図の BFT validator を将来設計へ再導入せず、文書だけで既存 network の停止・債務解除も指示しません。
