# ADR-0024: Verified LLM Token-Weighted BFT for DNS finality

**Status:** Retired — VLT validator 投票重み・compute overlay は廃止済み。

VLT による validator 投票重みは再実装しない。PALW は validator 投票に依存せず、VLT 重みは [ADR-0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) で削除、compute committee/beacon は [ADR-0134](0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) で廃止されたため。

旧実装は履歴であり、将来の実装計画ではない。
