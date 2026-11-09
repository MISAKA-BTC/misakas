# RFCの現行20本への対応表

## 2026-11-09追加改訂 — 公開資料とモデル未保有の一人検証

先行改訂から継承した旧番号RFC50ファイルとadoption-map50行を保持し、新規0050を追加した51ファイルを現行20冊へ割り当てる。原添付の49ファイルに、同会話で採択された0049と今回0050を重ねた数である。過去のsource-map、section-owners、歴史付録、旧hashは書き換えない。

| 追加記録 | 主所有者 | 関連する所有者 | 状態 |
| --- | --- | --- | --- |
| [0050](../rfc/0050-public-materials-and-one-honest-panel.md) | [20](0020-verification-gated-final-and-adversarial-models.md) | 01〜19の公開入力・資源・選出・会計・保存・bootstrap・native検証との接続 | Accepted design、実装・測定・有効化は別 |

公開資料、cold取得、一人前提、k_min=1、最大虚偽caseの有限終結は0050／0218で採択し、詳細を16〜20と関連冊子へ反映する。旧75／41や旧署名の意味は変更しない。以下は先行改訂時点の対応記録である。

## 2026-11-09追加改訂 — 一人の反証と実行整合性

入力の旧番号RFC49ファイルを保持し、新規採択記録0049を加えた50ファイルを現行20冊へ割り当てる。既存adoption-map49行、source-map、section-owners、歴史付録は維持する。今回入力49件と、以前の入力48件という履歴メタデータを混同しない。

| 追加記録 | 現行主担当 | 関連担当 | 状態 |
| --- | --- | --- | --- |
| [0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) | [20](0020-verification-gated-final-and-adversarial-models.md) | [05](0005-bounded-resources-and-long-context.md) / [06](0006-panels-and-probabilistic-verification.md) / [07](0007-claims-exec-and-answer-accounting.md) / [08](0008-claim-dependencies-and-liability.md) / [10](0010-dag-consensus-and-native-settlement.md) / [12](0012-authenticated-state-storage-and-ibd.md) / [14](0014-court-master-contract.md) / [16](0016-miner-execution-commit-and-material-supply.md) / [17](0017-noninteractive-positive-verification.md) / [18](0018-interactive-negative-court-and-defaults.md) / [19](0019-model-free-node-and-bounded-evidence.md) | Accepted design、実装・測定・有効化は別 |

## 2026-11-08追加改訂 — EXEC・1 BPS

現行の通常編集先は20冊を維持する。旧番号系列は、以前の48ファイルを保持し、
新しい採択記録RFC-0048を加えて49ファイルとなる。番号0042の二つの原文も別パスで保持する。
adoption-mapのv2は既存48行をそのまま残し、新規採択記録のinput hashが存在しないことを明示する。
旧41原文と89収録区間のsource-mapは変更しない。

| 追加記録 | 現行主担当 | 関連担当 | 状態 |
| --- | --- | --- | --- |
| [0048](../rfc/0048-exec-credit-schedule-and-one-bps.md) | [07](0007-claims-exec-and-answer-accounting.md) | [05](0005-bounded-resources-and-long-context.md) / [10](0010-dag-consensus-and-native-settlement.md) / [18](0018-interactive-negative-court-and-defaults.md) / [19](0019-model-free-node-and-bounded-evidence.md) / [20](0020-verification-gated-final-and-adversarial-models.md) | Accepted design、実装・測定・有効化は未完了 |

以下の48ファイルという数は直前の改訂時点を表す。新規0048はこの表の追加行で管理する。

整理日: 2026-11-08 / 入力: docs(2).zip。[現行索引](README.md) / [移行規則](MIGRATION.md)。

**現行の編集先は20本。添付内の旧番号RFCは48ファイルすべてを保持する。**
0042は二つの異なるファイルがあるため、番号だけで同一視しない。
[adoption-map.json](adoption-map.json)が48ファイルの現行担当・新profile条項・入力SHA-256を持つ。
[source-map.json](source-map.json)は2026-10-29の41原文・89収録区間の歴史台帳として変更しない。

## 基準日後の7ファイル

| 保存する旧RFC | 現行の主担当 | 新profileの関連先 | 扱い |
| --- | --- | --- | --- |
| [0042-all-model-g14-panel-one.md](../rfc/0042-all-model-g14-panel-one.md) | [06](0006-panels-and-probabilistic-verification.md) | [20](0020-verification-gated-final-and-adversarial-models.md) | モデル保有者内の通常Panelとglobal検証者を分離。旧署名・旧profileは保持。 |
| [0042-court-family-supplier-consent-and-native-reservation.md](../rfc/0042-court-family-supplier-consent-and-native-reservation.md) | [11](0011-bond-budgets-fees-and-payments.md) | [16](0016-miner-execution-commit-and-material-supply.md) / [18](0018-interactive-negative-court-and-defaults.md) | 供給同意・資金予約は共通会計へ。新版originalとqualified defaultへ接続。 |
| [0043-wide-q24-llm-output.md](../rfc/0043-wide-q24-llm-output.md) | [02](0002-tensor-ir-and-shared-query-data.md) | [16](0016-miner-execution-commit-and-material-supply.md) / [17](0017-noninteractive-positive-verification.md) / [19](0019-model-free-node-and-bounded-evidence.md) | 型・丸め・layoutの版を保持し、canonical traceと局所courtへ束縛。 |
| [0044-positive-court-full-coverage.md](../rfc/0044-positive-court-full-coverage.md) | [17](0017-noninteractive-positive-verification.md) | [16](0016-miner-execution-commit-and-material-supply.md) / [19](0019-model-free-node-and-bounded-evidence.md) / [20](0020-verification-gated-final-and-adversarial-models.md) | 旧R2全被覆の保証は保持。新版はPanel全再実行・Node局所検査とglobal承認前提へ変更。 |
| [0045-positive-court-canonical-assignment.md](../rfc/0045-positive-court-canonical-assignment.md) | [11](0011-bond-budgets-fees-and-payments.md) | [06](0006-panels-and-probabilistic-verification.md) / [17](0017-noninteractive-positive-verification.md) / [20](0020-verification-gated-final-and-adversarial-models.md) | 割当・WorkerPaymentの原資と署名済み権利を保持。全体検証承認とは分離。 |
| [0046-positive-native-funded-relay.md](../rfc/0046-positive-native-funded-relay.md) | [11](0011-bond-budgets-fees-and-payments.md) | [16](0016-miner-execution-commit-and-material-supply.md) / [18](0018-interactive-negative-court-and-defaults.md) / [20](0020-verification-gated-final-and-adversarial-models.md) | funded relayの同意・予約・返還を維持。新版の供給と公開反証へ接続。 |
| [0047-batch-court-time-contract.md](../rfc/0047-batch-court-time-contract.md) | [18](0018-interactive-negative-court-and-defaults.md) | [14](0014-court-master-contract.md) / [19](0019-model-free-node-and-bounded-evidence.md) / [20](0020-verification-gated-final-and-adversarial-models.md) | a+3120とa+10000の時計・公開経路を維持。事前認証とcold取得予算を接続。 |

## 新profileの責務の優先順位

16はMiner originalと資料供給、17は非対話positive、18は対話negativeと提供義務違反、
19はモデルを持たないNode、20はglobal選出・肯定認証・成功Finalを所有する。
01〜15の先頭節も更新し、下記の歴史配置だけから新profileの規則を決めない。
旧版を置換する範囲は[20](0020-verification-gated-final-and-adversarial-models.md)に限定する。
旧原文の署名・wire・fence・権利は保持する。

## 2026-10-29の41原文の収録対応（歴史記録）


[統合索引](README.md) / [編集・移行規則](MIGRATION.md)

旧番号は履歴識別子として保持する。主な移行先が一つでも、混在する条項を責務ごとに分けたものは補助移行先を示す。完全な行範囲は[機械可読対応表](source-map.json)にある。

| 旧RFC | 主移行先 | 本文の収録先すべて | 配置・状態の注意 |
| --- | --- | --- | --- |
| [0001](../rfc/0001-palw-inference-surface-gaps.md) | [01](0001-inference-and-generative-jobs.md) | [01](0001-inference-and-generative-jobs.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0002](../rfc/0002-palw-tensor-ir.md) | [02](0002-tensor-ir-and-shared-query-data.md) | [02](0002-tensor-ir-and-shared-query-data.md)、[03](0003-model-onboarding-and-artifacts.md)、[14](0014-court-master-contract.md) | TIRの中心は02。onboarding節は03、Court専用条項は14。 |
| [0003](../rfc/0003-palw-generative-model-classes.md) | [01](0001-inference-and-generative-jobs.md) | [01](0001-inference-and-generative-jobs.md)、[15](0015-freeze-deferred-rejected-and-governance.md) | 共通job・text等は01。画像生成・動画生成profileは15の延期範囲。 |
| [0004](../rfc/0004-palw-model-improvement.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | モデル改善protocolは15のmainnet後延期。既存の登録・変換とは分離。 |
| [0005](../rfc/0005-palw-ml-vm.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | 不採用。TIR／versioned kernelは02に残す。 |
| [0006](../rfc/0006-palw-layer-sharded-panels.md) | [06](0006-panels-and-probabilistic-verification.md) | [06](0006-panels-and-probabilistic-verification.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0007](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md) | [06](0006-panels-and-probabilistic-verification.md) | [06](0006-panels-and-probabilistic-verification.md)、[14](0014-court-master-contract.md) | 通常検査は06。exact courtへのescalationは14。計算validity proofの禁止は15。 |
| [0008](../rfc/0008-palw-claim-backed-consensus-blocks.md) | [07](0007-claims-exec-and-answer-accounting.md) | [07](0007-claims-exec-and-answer-accounting.md)、[14](0014-court-master-contract.md) | EXEC／仕事量は07。裁定・証拠保持の専用条項は14。 |
| [0009](../rfc/0009-palw-remote-miner.md) | [09](0009-remote-client-and-sdk.md) | [09](0009-remote-client-and-sdk.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0010](../rfc/0010-permissionless-palw-panel-and-claim-completion.md) | [06](0006-panels-and-probabilistic-verification.md) | [06](0006-panels-and-probabilistic-verification.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0011](../rfc/0011-permissionless-model-and-long-context-onboarding.md) | [03](0003-model-onboarding-and-artifacts.md) | [03](0003-model-onboarding-and-artifacts.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) | [10](0010-dag-consensus-and-native-settlement.md) | [10](0010-dag-consensus-and-native-settlement.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0013](../rfc/0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | [03](0003-model-onboarding-and-artifacts.md) | [03](0003-model-onboarding-and-artifacts.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0014](../rfc/0014-panel-independent-fraud-prosecution.md) | [14](0014-court-master-contract.md) | [11](0011-bond-budgets-fees-and-payments.md)、[14](0014-court-master-contract.md) | Courtは14。共通bond・配分の条項は11。 |
| [0015](../rfc/0015-panel-free-permissionless-verification.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | [11](0011-bond-budgets-fees-and-payments.md)、[15](0015-freeze-deferred-rejected-and-governance.md) | Panel=0は15で休眠管理。採用済みの共通bond上限・配分は11へ分離。 |
| [0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) | [11](0011-bond-budgets-fees-and-payments.md) | [11](0011-bond-budgets-fees-and-payments.md)、[15](0015-freeze-deferred-rejected-and-governance.md) | 共通会計・V+は11。Panel=0前提は15で管理。 |
| [0017](../rfc/0017-model-eligibility-examination-and-renewal.md) | [04](0004-model-tiers-and-annual-catalog.md) | [04](0004-model-tiers-and-annual-catalog.md)、[15](0015-freeze-deferred-rejected-and-governance.md) | 現行二階層は04。廃止したモデル審査は15。 |
| [0018](../rfc/0018-dag-parallel-multi-model-consensus.md) | [10](0010-dag-consensus-and-native-settlement.md) | [10](0010-dag-consensus-and-native-settlement.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0019](../rfc/0019-annual-model-upgrade.md) | [04](0004-model-tiers-and-annual-catalog.md) | [04](0004-model-tiers-and-annual-catalog.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0020](../rfc/0020-long-context-calibration-memory.md) | [05](0005-bounded-resources-and-long-context.md) | [05](0005-bounded-resources-and-long-context.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0021](../rfc/0021-design-freeze-and-documentation-governance.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | 設計凍結・重大問題時の変更手続きを15へ。 |
| [0022](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | 初期mainnetの除外・無効化・再開条件を15へ。 |
| [0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md) | [08](0008-claim-dependencies-and-liability.md) | [08](0008-claim-dependencies-and-liability.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0024](../rfc/0024-court-door-3200-to-ctx-2m.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0025](../rfc/0025-scalable-objective-court-many-models-mass-disputes.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0026](../rfc/0026-court-policy-master-contract.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0027](../rfc/0027-miner-account-bond-and-panel-incentives.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | [15](0015-freeze-deferred-rejected-and-governance.md) | 未採択案として15へ。永久不採用や現行bond制度の停止とはしない。 |
| [0028](../rfc/0028-rooted-court-response-eligibility-history.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0029](../rfc/0029-owner-independent-shared-tir-query-data.md) | [02](0002-tensor-ir-and-shared-query-data.md) | [02](0002-tensor-ir-and-shared-query-data.md) | 全文の主題を02へ統合。courtの時計・帰責は14から参照する。 |
| [0030](../rfc/0030-court-end-effects-and-pending-liability-archive.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0031](../rfc/0031-shared-court-initial-notice-boundary.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0032](../rfc/0032-shared-court-terminal-material-and-nonblocking-ingress.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0033](../rfc/0033-d2-readiness-root-opening-availability.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0034](../rfc/0034-archived-close-qualified-financial-discharge.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0035](../rfc/0035-court-original-funded-contract-and-role-consent.md) | [14](0014-court-master-contract.md) | [14](0014-court-master-contract.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0036](../rfc/0036-authenticated-palw-state-map-and-row-access.md) | [12](0012-authenticated-state-storage-and-ibd.md) | [12](0012-authenticated-state-storage-and-ibd.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0037](../rfc/0037-ibd-bounded-row-stream-and-storage-generations.md) | [12](0012-authenticated-state-storage-and-ibd.md) | [12](0012-authenticated-state-storage-and-ibd.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0038](../rfc/0038-palw-fp-answer-accounting.md) | [07](0007-claims-exec-and-answer-accounting.md) | [07](0007-claims-exec-and-answer-accounting.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0039](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md) | [11](0011-bond-budgets-fees-and-payments.md) | [11](0011-bond-budgets-fees-and-payments.md)、[15](0015-freeze-deferred-rejected-and-governance.md) | 採択済みfeeは11。未採択の役務有限化A/Bは15へ。 |
| [0040](../rfc/0040-authenticated-prefix-work-dependencies.md) | [08](0008-claim-dependencies-and-liability.md) | [08](0008-claim-dependencies-and-liability.md) | 元条項の採択・未採択・実装記録を保ち、主題別に統合。 |
| [0041](../rfc/0041-genesis-bond-free-bootstrap.md) | [13](0013-permissionless-bootstrap-and-release.md) | [13](0013-permissionless-bootstrap-and-release.md) | 新ネットワークのゼロbond起動を13へ。既存chainのgenesisは変更しない。 |
