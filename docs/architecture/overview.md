# MISAKA の構成 — 採用済み設計の入口

> **2026-11-08追加採択:** EXEC creditは[RFC07](../rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](../rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](../maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](../maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

**このページは設計の構成であり、全機能が稼働済みとの主張ではありません。** [状態](../STATUS.md) / [設計凍結](../governance/design-freeze.md)。

## 2026-11-08 採択の新しい claim 版

**Node はモデル本体を取得・保持せず、全推論・GPU・モデル配布先への問い合わせを必要としません。** Miner が実行 trace と必要資料の供給責任を固定し、独立選出 Panel が重い検証を担当します。Node は承認条件と、認証付きの必要な重み断片を含む上限付き局所証拠を検査します。通常 Node が Panel を兼務することは必須ではありません。

モデルが事前共有されているとは仮定せず、モデル内 Miner／Panel の全結託と資料秘匿を脅威に含めます。正常時は非対話 positive、不一致時は negative の対話で局所化し、必要資料・肯定検証が成立しなければ成功 Final を認めません。新版は結果固定後に検証を始め、必須局所 positive と独立肯定承認の成立で `CertificationReady`、公開異議・依存条件等の成立後に Final へ進みます。global pool の独立選出と正直な検証者の前提は残ります。

規範入口は [RFC-v2-0020](../rfc-v2/0020-verification-gated-final-and-adversarial-models.md) と [公開裁定の方針 v0.2](../design/palw/consensus/public-adjudication-policy-v0.2.md)、採択記録は [ADR-0215](../adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) です。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の共通構成と旧版の記録を保持し、新版の設計を旧署名済み claim へ遡及適用しません。

## 共通構成の入口

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
