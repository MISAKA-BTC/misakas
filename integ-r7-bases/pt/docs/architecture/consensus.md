# 合意・DAG・認証の責任分界

[ADR-0180](../adr/0180-palw-only-consensus-no-bft-validators.md) は PALW のみの合意と BFT validator 不使用の採用理由を記録します。DNS退役・legacy reader・移行は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) を参照します。

[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md) の `Final` と `SecurityCertified` を区別し、claim ごとの認証と固定のBFT validator集合を同一視しません。Rule Eは [ADR-0178](../adr/0178-fork-choice-heals-a-partition-on-bonded-participation.md)、物理生成機会は [RFC-0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)、予算は [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)。

延期した画像・動画・改善が共通経路から開かないことと、残すテキスト・認証・会計の回帰は別途確認します。[延期方針](../governance/post-mainnet-scope.md) / [状態](../STATUS.md)。
