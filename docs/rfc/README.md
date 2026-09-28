# RFC index

An RFC is a proposal under discussion. [INDEX.md](../INDEX.md) §3 explains how an RFC becomes an ADR
and a Spec change. Take the next number from this table, not from `ls`: an RFC can live on a branch
before it reaches `main`.

| RFC | Title | Status | Where |
| --- | --- | --- | --- |
| 0001 | PALW inference surface gaps: deterministic decode controls, serving and input extensions (FP Job V4) | §A (the FP Job V4 release) Implementation Frozen, 2026-09-27 (G0). §0–§7 (P1–P3) Draft, for a later release | **Reserved here.** The text is `docs/rfc/0001-palw-inference-surface-gaps.md` on branch `rcore/fp-sampler` (`77a474a1c` when this index was written). It reaches `main` with that branch. Do not copy it here |
| 0002 | PALW Canonical Tensor IR v1 (PALW-TIR): a bounded, deterministic integer tensor program as the consensus meaning of a class, with a reference evaluator and optional fused kernels; v1 frozen on an architecture corpus | Draft, 2026-09-28 | [0002-palw-tensor-ir.md](0002-palw-tensor-ir.md) (branch `rfc/0002-tensor-ir`) |
| 0003 | PALW Generative Model Classes: one job, determinism and output layer (deterministic randomness R, canonical tensors and why no execution profile, canonical outputs), and the class profiles on top of it (image generation in detail; text, embedding, multimodal input, audio, video) | Draft, 2026-09-28 | [0003-palw-generative-model-classes.md](0003-palw-generative-model-classes.md) (branch `rfc/0003-image-generation`) |
| 0004 | PALW Bounded ML VM (PALW-BVM): a control-flow-only layer over PALW-TIR (IF, bounded FOR, non-recursive CALL, typed registers, calls into TIR segments) for what the tensor IR cannot express; a consensus-enforced ladder TIR → AOT expansion → BVM; a one-step control court with a descent into the TIR court; a measurement gate fixed before any code | Draft, 2026-09-28 — design only; not to be built until its §1 gate opens (closed today: no VM-attributable coverage gap) | [0004-palw-bounded-ml-vm.md](0004-palw-bounded-ml-vm.md) (branch `rfc/0004-0005-vm`) |
| 0005 | PALW Turing-complete ML VM (PALW-GVM) and Verified Model Improvement. **Part I**: general control, memory and gas over PALW-TIR precompiles — rung A = the EVM lane as an asynchronous orchestrator of PALW jobs; rung B = an off-chain RV32IM VM adjudicated by bisection to one instruction and a one-step proof, with a descent into the TIR court at every model call. **Part II**: a market that makes registered models stronger by distillation and RL — teaching artifacts admitted only on verified outcomes (EXEC, EXACT, CRITIC), candidates promoted only by a paired win on hidden, future items, rewards labelled by what they rest on; training itself is never claimed verified | Draft, 2026-09-29 — design only; Part I rung A when contracts ask for model calls, rung B only after PALW-TIR is mainnet-safe and workflow demand is measured; Part II P0 (mathematics, no VM) once `palw_tir_v1` and rung A run on a testnet | [0005-palw-turing-complete-ml-vm.md](0005-palw-turing-complete-ml-vm.md) (branch `rfc/0004-0005-vm`) |

**Next free number: RFC-0006.**

## ADRs that read as proposals

These ADRs decide nothing, or were forward-looking designs with nothing built. They keep their ADR
numbers. Reopening one means filing a new RFC that cites it.

| ADR | Why it reads as an RFC |
| --- | --- |
| [0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | "Decides nothing and changes no rule". It asks whether an inference can be the ticket without a hash lottery |
| [0023](../adr/0023-base-three-lane-execution.md) | "Proposed — design freeze … Nothing is implemented". The Base three-lane execution design |
