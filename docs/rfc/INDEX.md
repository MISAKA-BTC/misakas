# RFC 番号順索引

[入口](README.md) / [トピック別](TOPICS.md) / [番号台帳](../maintenance/numbering.md)。自動生成の探索用索引です。原文の過去のStatusを現在の稼働保証へ昇格させません。

**画像・動画（RFC-0003）とモデル改善（RFC-0004）はmainnet後へ延期・当面無効。** [現行方針](../governance/post-mainnet-scope.md)。

| 番号 | 文書 | 状態の入口 |
| --- | --- | --- |
| RFC-0001 | [PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計](0001-palw-inference-surface-gaps.md) | 原文と後続改定を参照 |
| RFC-0002 | [PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program a…](0002-palw-tensor-ir.md) | 原文と後続改定を参照 |
| RFC-0003 | [PALW Generative Model Classes — one job, determinism and output layer, and the class profi…](0003-palw-generative-model-classes.md) | 画像・動画は延期/無効。共通部分は別 |
| RFC-0004 | [PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR](0004-palw-model-improvement.md) | モデル改善はmainnet後へ延期/無効 |
| RFC-0005 | [PALW VM（不採用）](0005-palw-ml-vm.md) | 原文と後続改定を参照 |
| RFC-0006 | [PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with …](0006-palw-layer-sharded-panels.md) | 原文と後続改定を参照 |
| RFC-0007 | [PALW probabilistic detection — direct checks, authenticated receipts, and deterministic co…](0007-palw-verification-certificates-and-algebraic-checks.md) | 原文と後続改定を参照 |
| RFC-0008 | [PALW EXEC lane — claim-backed work slices and transaction execution](0008-palw-claim-backed-consensus-blocks.md) | 原文と後続改定を参照 |
| RFC-0009 | [PALW Remote Client — node を持たないモデル登録・claim・非保管型の報酬回収](0009-palw-remote-miner.md) | 原文と後続改定を参照 |
| RFC-0010 | [Permissionless PALW Panel binding and claim completion](0010-permissionless-palw-panel-and-claim-completion.md) | 原文と後続改定を参照 |
| RFC-0011 | [Permissionless model onboarding with probabilistic constraint verification and court on di…](0011-permissionless-model-and-long-context-onboarding.md) | 原文と後続改定を参照 |
| RFC-0012 | [PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto](0012-palw-only-consensus-and-native-evm-settlement.md) | 原文と後続改定を参照 |
| RFC-0013 | [Reproducible layouts and resource-bounded model onboarding tools](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | 原文と後続改定を参照 |
| RFC-0014 | [Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](0014-panel-independent-fraud-prosecution.md) | 原文と後続改定を参照 |
| RFC-0015 | [Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](0015-panel-free-permissionless-verification.md) | 原文と後続改定を参照 |
| RFC-0016 | [bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) | 採用設計・測定待ち・休眠 |
| RFC-0017 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)](0017-model-eligibility-examination-and-renewal.md) | 二階層採用・旧審査撤回 |
| RFC-0018 | [DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](0018-dag-parallel-multi-model-consensus.md) | Draft / 個別採択あり |
| RFC-0019 | [年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変](0019-annual-model-upgrade.md) | Draft / 原文参照 |
| RFC-0020 | [長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](0020-long-context-calibration-memory.md) | Draft / 部分実装 |
| RFC-0021 | [設計凍結・例外変更手続きと文書の役割分離](0021-design-freeze-and-documentation-governance.md) | Accepted / 文書管理 |
| RFC-0022 | [初期 mainnet から画像・動画・モデル改善を外す適用範囲](0022-post-mainnet-media-and-model-improvement-scope.md) | Accepted / 延期・無効化方針 |
| RFC-0023 | [未解決 claim の責任を保持し、それに依存しない claim を進める](0023-unresolved-claim-liability-and-independent-progress.md) | 原文と後続改定を参照 |
| RFC-0024 | [ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(単一位置裁定・先読み開示・先行取得)](0024-court-door-3200-to-ctx-2m.md) | 原文と後続改定を参照 |
| RFC-0025 | [モデルが増え、不正に対する court が大量発生しても処理を継続できる客観的 court](0025-scalable-objective-court-many-models-mass-disputes.md) | 原文と後続改定を参照 |
| RFC-0026 | [court の主契約:時計・帰責・責任期間・置換関係の一本化](0026-court-policy-master-contract.md) | 原文と後続改定を参照 |
| RFC-0027 | [Miner Account Bond と Panel 追及報酬: 10万の基礎担保、責任連動の追加拘束、REAL継続性](0027-miner-account-bond-and-panel-incentives.md) | 原文と後続改定を参照 |
| RFC-0028 | [rooted court response eligibility history](0028-rooted-court-response-eligibility-history.md) | 原文と後続改定を参照 |
| RFC-0029 | [owner-independent shared TIR query data](0029-owner-independent-shared-tir-query-data.md) | 原文と後続改定を参照 |
| RFC-0030 | [court終了効果と未解決責任の保存](0030-court-end-effects-and-pending-liability-archive.md) | 原文と後続改定を参照 |
| RFC-0031 | [共有courtのinitial notice境界](0031-shared-court-initial-notice-boundary.md) | 原文と後続改定を参照 |
| RFC-0032 | [共有courtの終端materialと非独占提出](0032-shared-court-terminal-material-and-nonblocking-ingress.md) | 原文と後続改定を参照 |
| RFC-0033 | [D2 readiness rootのdigest opening取得](0033-d2-readiness-root-opening-availability.md) | 原文と後続改定を参照 |
| RFC-0034 | [archived closeの実受付と金融責任の適格解消](0034-archived-close-qualified-financial-discharge.md) | 原文と後続改定を参照 |
| RFC-0035 | [shared court originalの実原資と役割別同意](0035-court-original-funded-contract-and-role-consent.md) | 原文と後続改定を参照 |
| RFC-0036 | [認証付き PALW state map と行単位アクセス](0036-authenticated-palw-state-map-and-row-access.md) | 原文と後続改定を参照 |
| RFC-0037 | [IBD の有界 authenticated row stream と世代番号付き保存](0037-ibd-bounded-row-stream-and-storage-generations.md) | 原文と後続改定を参照 |
| RFC-0038 | [free-prompt の回答トークンを支払済み計算として記録する(FPA1 carriage)](0038-palw-fp-answer-accounting.md) | 原文と後続改定を参照 |
| RFC-0039 | [通常 verifier の fee consent と provider の有限支払・保持契約](0039-finite-fee-consent-and-provider-payment-bounds.md) | 原文と後続改定を参照 |
| RFC-0040 | [再利用 prefix の生成元を署名済み work identity で拘束する](0040-authenticated-prefix-work-dependencies.md) | 原文と後続改定を参照 |
| RFC-0041 | [genesis bond を廃止し、公開の BASE 回復経路から立ち上げる](0041-genesis-bond-free-bootstrap.md) | 原文と後続改定を参照 |
| RFC-0042 | [全モデルの producer を G14 Panel=1 に統一する](0042-all-model-g14-panel-one.md) | 原文と後続改定を参照 |
