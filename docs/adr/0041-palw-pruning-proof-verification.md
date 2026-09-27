# ADR-0041: PALW pruning-proof verification — exhaustive and amortised, not sampled

> **Body moved (2026-09-27).** Normative rules → [spec/palw/13](../spec/palw/13-fork-choice-and-heartbeat.md); the full text as written → [design/palw/archive/0041-palw-pruning-proof-verification.md](../design/palw/archive/0041-palw-pruning-proof-verification.md); the reasoning is summarised in [design/palw/liveness.md](../design/palw/liveness.md).

* Status: Landed. It activates nothing on a shipped preset by itself. **D1 (sampled pruning-proof
  verification) was withdrawn as unsound**, and D1′ (exhaustive and amortised) governs. D2
  (parallelism during IBD) landed, and buys far less than drafted. This is the live ADR-0041, not the
  snapshot's (see README "Number hygiene").
* Date: 2026-08

## Context

Verifying a pruning proof means one inference per proof header. Doing that for every header of a
long proof looked too slow, so sampling was proposed.

## Decision

- **D1 — Sample the PoW checks.** *Withdrawn: unsound in this proof structure.*
- **D1′ — Keep verification exhaustive and amortise the per-header cost:** hold the model resident and
  serve jobs over one context. The overhead was about 97 % of a one-shot verification. → spec 13
  PALW-FC-8.
- **D2 — Parallelise verification during IBD.** Landed. Measured on the fleet host, it helps far less
  than the drafted 8×.

## Consequences

- IBD cost is bounded by amortisation, not by sampling. From DAA 750 on testnet-12, a pruning-proof
  switch also needs a strict economic win (spec 13 PALW-FC-7).

## Links

- Spec: [13 Fork choice and heartbeat](../spec/palw/13-fork-choice-and-heartbeat.md)
- Design: [design/palw/liveness.md](../design/palw/liveness.md)
- Full text as written: [design/palw/archive/0041-palw-pruning-proof-verification.md](../design/palw/archive/0041-palw-pruning-proof-verification.md)
