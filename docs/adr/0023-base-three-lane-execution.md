# ADR-0023: MISAKA Base + Three Execution Lanes (PQ-EVM / ETH-compat / Proof-verified Parallel EVM)

**Status:** 不採用（将来の実装対象外）。

EVM の三つの実行 lane を PALW work に広げる案は実装しない。[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) の「利用者が元々行うローカル推論に報酬を払う」範囲を超えて、別の実行・報酬面を増やすため。既存の [selected-parent EVM lane](0020-selected-parent-evm-lane.md) は独立した現行設計として維持する。
