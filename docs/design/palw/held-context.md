# Held context, long context and context limits — design

> **Not normative.** This document explains why the rules on held context in
> [spec/palw/03](../../spec/palw/03-classes-and-registry.md) §3.7,
> [04](../../spec/palw/04-execution-semantics.md) §4.5 and [09](../../spec/palw/09-court-and-offences.md)
> are what they are.

**Decisions recorded in:** ADR-0081 (superseded in part), 0082, 0084, 0103, 0110, 0112, 0116, 0118,
0119, 0121.
**Last revised:** 2026-09-27

## 1. Problem

A person types thousands of tokens and receives thousands more. The chain must hold one claim whose
verification costs a bounded number of bytes wherever bytes are spent, and a bounded amount of compute
wherever compute cannot be avoided, at contexts up to 2M tokens.

## 2. The design in one paragraph

- The context is **held off the chain**. The chain carries a root, an opening and a logarithm, not the
  capture.
- The capture is a fold, and the close is flat in the context.
- A held class is walked at its own ladder (`2^40`) and commits its prompt as a tiled Merkle root.
- A seat demands the committed leaf it needs.
- A context limit is armed only from public vectors anyone can reproduce.

## Source texts (archived ADR bodies)

- [ADR-0110 — A context limit is activated from reproducible public vectors, not the maintainer's workstation](archive/0110-a-context-limit-is-activated-from-reproducible-public-vectors-not-the-maintainers-workstation.md)
- [ADR-0118 — The held regime arrives at a height, and a held class carries its own prompt form](archive/0118-the-held-regime-arrives-at-a-height-and-a-held-class-carries-its-own-prompt-form.md)
- [ADR-0119 — A held class is walked at the regime's ladder, and the chain records which classes those are](archive/0119-a-held-class-is-walked-at-the-regimes-ladder-and-the-chain-records-which-classes-those-are.md)
- [ADR-0082: The close is flat in the context — attention is refuted by dissection, the capture is a fold, and the answer is what earns](archive/0082-the-close-is-flat-in-the-context.md)
- [ADR-0103 — The context is held off the chain, and the chain carries a root, an opening and a logarithm](archive/0103-the-context-is-held-off-the-chain-and-the-chain-carries-a-root-an-opening-and-a-logarithm.md)
- [ADR-0081: Long context — the input is a state chain](archive/0081-long-context-the-input-is-a-state-chain.md)
- [ADR-0116 — An attention history is the class's, and the held regime reduces over its own width](archive/0116-an-attention-history-is-the-classs-and-the-held-regime-reduces-over-its-own-width.md)
