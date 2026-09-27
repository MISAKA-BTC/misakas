# ADR-0037: PALW off the block-critical path — an asynchronous, budgeted job state machine over a permanent hash floor

> **Body moved (2026-09-27).** Normative rules → [spec/palw/07](../spec/palw/07-claim-lifecycle.md), [spec/palw/08](../spec/palw/08-verification.md), [spec/palw/10](../spec/palw/10-collateral-and-economics.md); the full text as written → [design/palw/archive/0037-palw-async-job-state-machine.md](../design/palw/archive/0037-palw-async-job-state-machine.md); the reasoning is summarised in [design/palw/lifecycle.md](../design/palw/lifecycle.md).

* Status: **Superseded in part by ADR-0038 (same day).** Decision 1, which made PALW never
  block-critical over a permanent hash floor, is reversed. Decisions 2–9 were carried into ADR-0038
  Decision G and, through the V2 ruleset (ADR-0042), are what spec 07, 08 and 10 state today.
* Date: 2026-08-17

## Context

After the Ollama path was shown forgeable, PALW needed a verification design that could not stall
the chain. The first answer put PALW off the block-critical path: hash PoW would order blocks, and
PALW would be an asynchronous, budgeted job state machine that credits work later.

## Decision

- **D1 — Three layers; PALW is never block-critical on a value-bearing network.** *Reversed by
  ADR-0038:* PALW is the consensus work.
- **D2 — Jobs are state, not history.** One state machine with explicit phases and deadlines. → spec
  07 §7.1.
- **D3 — Identity and signatures are fully bound** and verified at every consumer entry. → spec 02, 07.
- **D4 — The panel is selected at a future anchor, on a real snapshot, with a dual deadline.** → spec
  07 §7.3, 08.
- **D5 — Sampled verification is the fast path, never the final ruling.** → spec 08 PALW-VF-12, -15.
- **D6 — A two-tier hardware taxonomy.** Classes qualify by calibration, not by self-declaration.
- **D7 — Mint is a carve of the scheduled subsidy, never an append.** → spec 10 §10.8.
- **D8 — `P_check` and self-declared capacity are out of the safety argument.**
- **D9 — An on-chain class registry.** A freeze halts credit, not the chain, and there is no per-job
  override. → spec 03.
- **D10 — Reconciliation, and the 10 BPS question** (deferred).

## Consequences

- The invariants and the activation stages of the full text became release-blocking review checks
  for the V2 lineage.
- D1's reversal is why production is PALW-only, with a near-weightless clock lane (ADR-0039, 0060,
  0066).

## Links

- Spec: [07 Claim lifecycle](../spec/palw/07-claim-lifecycle.md) · [08 Verification](../spec/palw/08-verification.md) · [10 Collateral and economics](../spec/palw/10-collateral-and-economics.md)
- Design: [design/palw/lifecycle.md](../design/palw/lifecycle.md)
- Full text as written: [design/palw/archive/0037-palw-async-job-state-machine.md](../design/palw/archive/0037-palw-async-job-state-machine.md)
