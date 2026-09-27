# RFC index

An RFC is a proposal under discussion. [INDEX.md](../INDEX.md) §3 explains how an RFC becomes an ADR
and a Spec change. Take the next number from this table, not from `ls`: an RFC can live on a branch
before it reaches `main`.

| RFC | Title | Status | Where |
| --- | --- | --- | --- |
| 0001 | PALW inference surface gaps: deterministic decode controls, serving and input extensions (FP Job V4) | §A (the FP Job V4 release) Implementation Frozen, 2026-09-27 (G0). §0–§7 (P1–P3) Draft, for a later release | **Reserved here.** The text is `docs/rfc/0001-palw-inference-surface-gaps.md` on branch `rcore/fp-sampler` (`77a474a1c` when this index was written). It reaches `main` with that branch. Do not copy it here |
| 0002 | Canonical ML IR v1 (PALW-TIR): a bounded, deterministic integer tensor IR as the consensus VM, with optional fused kernels; v1 frozen on an architecture corpus | Draft, 2026-09-28 | [0002-palw-tensor-ir.md](0002-palw-tensor-ir.md) (branch `rfc/0002-tensor-ir`) |

**Next free number: RFC-0003.**

## ADRs that read as proposals

These ADRs decide nothing, or were forward-looking designs with nothing built. They keep their ADR
numbers. Reopening one means filing a new RFC that cites it.

| ADR | Why it reads as an RFC |
| --- | --- |
| [0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | "Decides nothing and changes no rule". It asks whether an inference can be the ticket without a hash lottery |
| [0023](../adr/0023-base-three-lane-execution.md) | "Proposed — design freeze … Nothing is implemented". The Base three-lane execution design |
