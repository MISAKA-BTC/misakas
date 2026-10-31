# RFC v1 → v2 全41本の対応表

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
