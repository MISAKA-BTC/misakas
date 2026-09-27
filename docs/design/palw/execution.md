# Execution semantics, kernels and runtimes — design

> **Not normative.** This document explains why the rules in
> [spec/palw/04-execution-semantics.md](../../spec/palw/04-execution-semantics.md) and
> [04a](../../spec/palw/04a-integer-arithmetic.md) are what they are.

**Decisions recorded in:** ADR-0026 D3, 0030, 0031, 0040, 0047, 0050, 0052, 0053, 0057, 0067, 0102,
0117.
**Last revised:** 2026-09-27

## 1. Problem

A lie must be convictable on any CPU, so the arithmetic must be exactly reproducible everywhere. It
must not depend on libm, float reduction order or a vendor's kernels. At the same time miners must be
free to run faster: P5 says efficiency is rewarded.

## 2. The design in one paragraph

- Every class runs one integer arithmetic (BASE-0, its A16 tier and the QWEN36 ops), whose reduction
  order is free.
- Transcendentals are either algorithms or registration-time data.
- The kernel set a network has armed is its consensus surface, and nothing else is.
- Below that surface any backend may be as fast as it can, provided it is bit-identical.
- The tolerant GPU family (ADR-0051) was tried, and withdrawn once its motive expired and its safety
  mechanisms turned out never to have existed (ADR-0053).

## Source texts (archived ADR bodies)

- [ADR-0040: `PALW-BASE-0` — the integer-only arithmetic normative specification](archive/0040-palw-base-0-integer-arithmetic.md)
- [ADR-0030: The PALW step function, pinned at tile granularity — shape profile v3](archive/0030-palw-step-function-shape-profile.md)
- [ADR-0053: One execution family — Family M is withdrawn, and the court is not optional](archive/0053-palw-one-execution-family.md)
- [ADR-0102 — The embedding lift is read per token, and a kernel a network has not armed is not in its identity](archive/0102-the-embedding-lift-is-read-per-token-and-an-unarmed-kernel-is-not-in-the-identity.md)
- [ADR-0117 — A draw is one forward](archive/0117-a-draw-is-one-forward.md)
- [ADR-0052: `PALW-QWEN36` — the integer arithmetic for Qwen3.6's hybrid graph](archive/0052-palw-qwen36-hybrid-class.md)
- [ADR-0050: The BASE-0 residual site — the narrowing that was never declared, and the amplification that was](archive/0050-palw-base0-residual-site.md)
- [ADR-0057: BASE-0 runtime acceleration — backends below the semantic boundary](archive/0057-palw-base0-runtime-acceleration.md)
