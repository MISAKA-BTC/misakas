# MISAKA Base 三つの EVM lane — 不採用理由

Status: **Withdrawn / 不採用。**

PQ settlement・現行 Ethereum 互換・並列 proof EVM を三つの lane として追加する案は採用しない。ユーザーがローカルで行う推論を検証可能にする目的に対し、独立した実行環境と互換性・証明・運用の維持が過大になるため。判断は [ADR-0023](adr/0023-base-three-lane-execution.md) と [ADR-0144](adr/0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) に従う。既存の selected-parent EVM は [ADR-0020](adr/0020-selected-parent-evm-lane.md) の別機能である。
