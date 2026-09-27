# Model lines and the market — design

> **Not normative.** This document explains why the rules in
> [spec/palw/15-model-lines-and-market.md](../../spec/palw/15-model-lines-and-market.md) are what they
> are.

**Decisions recorded in:** ADR-0087, 0088, 0089, 0090, 0091, 0094, 0095, 0101, 0114, 0120, and
[ADR-0154](../../adr/0154-testnet-12-flag-day-daa-750.md) (the model sink).
**Last revised:** 2026-09-27

## 1. Problem

The people behind a model need a way to back it and a way to be rewarded for building it. That must
not become a security, a transferable token, or a lever on the unit price of work (P7). And it must
not turn PALW into a remote-inference marketplace (P1).

## 2. The design in one paragraph

- A line (a class, an owner and a name) publishes versions.
- Its store is a constant-product pair, opened only by a real, permanently locked seed.
- Whole positions are bought from the curve and sold back to it, never transferred. Every move burns
  5 % and pays the owner 5 %.
- 5 % of every Final's worker escrow buys positions the chain keeps forever. No holder is ever paid.
- A position is a membership: product access the line declares, and never income.
- The EVM sees the market through read precompiles and a writer the fold applies after the block.

## Source texts (archived ADR bodies)

- [ADR-0089 — the fold is the truth; the EVM is its window and its hand](archive/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
- [ADR-0088 — the class keeps its graph; a line keeps its owner, and the owner keeps publishing](archive/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
- [ADR-0087 — a position is bought from the curve and sold back to it](archive/0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md)
- [ADR-0091 — The reward buys the pair, and no holder is paid](archive/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
- [ADR-0095 — A position is a membership, not an income](archive/0095-a-position-is-a-membership-not-an-income.md)
- [ADR-0090 — The pair is seeded with real MSK, locked for good, and a position is whole](archive/0090-the-pair-is-seeded-with-real-msk-locked-for-good-and-a-position-is-whole.md)
- [ADR-0094 — A seed is paid in as many transactions as it takes](archive/0094-a-seed-is-paid-in-as-many-transactions-as-it-takes.md)
- [ADR-0101 — A membership is proven by the chain and served by anyone, and a Position never moves between holders](archive/0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md)
