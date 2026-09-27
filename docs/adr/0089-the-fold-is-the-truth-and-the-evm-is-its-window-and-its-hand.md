# ADR-0089 — the fold is the truth; the EVM is its window and its hand

> **Body moved (2026-09-27).** Normative rules → [spec/palw/15](../spec/palw/15-model-lines-and-market.md); the full text as written → [design/palw/archive/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md](../design/palw/archive/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md); the reasoning is summarised in [design/palw/market.md](../design/palw/market.md).

* Status: Proposed 2026-09-05, **implemented the same day** behind `palw_model_evm`. It amends ADR-0087
  D3/D8 and notes that ADR-0020's "EVM is opt-in" is stale.
* Date: 2026-09-05

## Context

The operator asked that Model Positions be usable from the EVM, without being a mere copy of ERC-20,
and with the fold remaining the only truth.

## Decision

- **D1 — Four system addresses in MISAKA's own range,** and a facade family.
- **D2 — A read returns the fold at the EVM block's selected parent.**
- **D3 — The MRC-20 facade:** ERC-20's read half, with the curve in place of its transfer half.
- **D4 — There is no `Transfer`,** because there is no transfer.
- **D5 — `ModelWriter`** has CoreWriter's shape.
- **D6 — The hand moves after the block,** in the fold, and settles one block later.
- **D7 — Two holder namespaces,** and neither reaches the other.
- **D8 — Supply,** stated with the new pools in it.
- **D9 — The fence** `palw_model_evm`.
- **D10 — What HyperEVM has that this does not take.**
- **D11 — Lane 1 is the same primitives with post-quantum keys.**
- **D12 — What ships beside the node** (`contracts/misaka-model/`).

→ spec 15 §15.5 and `spec/evm`.

## Consequences

- The EVM is a window and a hand, never a second source of truth.

## Links

- Spec: [15 Model lines and market](../spec/palw/15-model-lines-and-market.md)
- Design: [design/palw/market.md](../design/palw/market.md)
- Full text as written: [design/palw/archive/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md](../design/palw/archive/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md)
