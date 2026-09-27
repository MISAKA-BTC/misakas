# PALW spec — 04. Execution semantics

> **Normative.** This chapter states the rules as they are on each network today. The integer
> arithmetic itself is in [04a-integer-arithmetic.md](04a-integer-arithmetic.md). Reasoning:
> [design/palw/execution.md](../../design/palw/execution.md) and
> [design/palw/held-context.md](../../design/palw/held-context.md). The code is the truth, and
> disagreements are listed in [divergences.md](divergences.md).

**Applies to:** mainnet (not active: PALW disabled) · testnet-12 (from genesis)
**Reconciled with code at:** `55a7be02f` (2026-09-27)
**Principles served:** P3 (the committed execution is the one the user was answered with); P5 (backends
below the semantic boundary are free to be faster).

What "the inference ran, as committed" means, bit for bit. Seats re-execute against this chapter, and
the court adjudicates against it.

## 4.1 One execution family

- **PALW-EX-1.** There MUST be exactly one execution family: the integer CPU family of chapter 04a. The
  court is not optional. `PalwClassTermsV2` does not exist, and there is one panel, the network's. The
  Metal/GGUF family is withdrawn, and its routing family is reserved.
- **PALW-EX-2 (the semantic boundary).** The consensus surface is the catalogue: the ops of 04a and the
  kernels armed on the network. A runtime backend MAY implement them any way it likes below that
  boundary (vectorised, fused up to the committed row, GPU-accelerated), provided its outputs are
  bit-identical (14 §14.4).
- **PALW-EX-3 (exactness).** Within a pinned class, equality is full 64-byte equality. No verdict MAY use
  a tolerance.

**Sources:** ADR-0053 D1–D6, ADR-0026 D3, ADR-0057 D1/D6. **Code:** `core/palw_base0.rs`,
`core/palw_backend.rs`, `core/palw_catalog_coverage.rs`.

## 4.2 The arithmetic

- **PALW-EX-4.** Every committed step MUST be computed exactly as 04a specifies:
  - BASE-0: integer-only, the representation, the three arithmetic rules, the closed op set, free
    reduction order, the two integer transcendentals, and `Rescale`;
  - the A16 activation tier;
  - the residual site's narrowing and per-tensor, per-layer gain;
  - the QWEN36 hybrid operations: router selection with its normative tie rule, a single-accumulator
    combine, `IntLn`, the gated delta rule, and partial rotation;
  - the canonical transcendentals: exp and log as algorithms.
- **PALW-EX-5 (tables are data).** Rotation tables and every other transcendental evaluated at
  registration are artifact data, pinned like weights. A transcendental evaluated at inference is
  normative arithmetic. Every op is total: a shape error is a refusal, never a panic.
- **PALW-EX-6 (Kimi K3).** The Kimi-K3 kernels are armed on testnet-12 (`palw_kimi_k3`). Under
  `palw_offence_attribution`, a class that reaches a Kimi-K3 kernel is refused at admission (09
  PALW-CT-8), so no testnet-12 class executes them.

**Sources:** ADR-0040, ADR-0031, ADR-0047, ADR-0050, ADR-0052 (in 04a); ADR-0152 J-5. **Code:**
`core/palw_base0_ops.rs`, `core/palw_base0_a16.rs`, `core/palw_qwen36_ops.rs`, `core/palw_transcendental.rs`,
`core/palw_kimi_k3_*.rs`.

## 4.3 Kernels are the build

- **PALW-EX-7.** The kernel set is the consensus surface, and it is irreducible. A kernel that a network
  has not armed is not in its identity (`court_catalog_root`). Arming a kernel is a ruleset change.
- **PALW-EX-8 (the embedding lift).** Past `palw_token_lift`, the hybrid's embedding lift is read per token
  (graph-v6), and a graph-v6 registration pins its operand-inventory root.
- **PALW-EX-9 (fused rows).** A fused row MUST be one the court can dissect. Admission refuses a fused
  class otherwise (`palw_fused_dissectable`).

**Sources:** ADR-0067 D3, ADR-0102 D1–D4, ADR-0093 D6.

## 4.4 Steps, tiles and commitments

- **PALW-EX-10 (the step space).** A job's execution is a sequence of committed steps over its graph's
  step space.
  - Each step is committed at tile granularity.
  - The shape profile (v3) binds everything a step's arithmetic depends on (`shape_profile_id`).
  - The step leg is the execution commitment's per-step record.
  - `tile_len` is bounded by the profile and never feeds accounting (05 PALW-WK-3).
- **PALW-EX-11 (what a claim commits).**
  - For every claim: the execution root over its steps, the full-logits trace root, and the output
    root.
  - For a model tier, additionally the commitments its step spaces need to be adjudicable end to end.
  - The prompt ids ride as a Merkle root (tiled for held classes).
- **PALW-EX-12 (a draw is one forward).** The prefill MUST be one pass over the weights, layer by layer.
  Past `palw_prefill_draw`, an attempt's job is the canonical job without its decode calls, plus one
  decode step. A held material answers the whole job its block asked for.
- **PALW-EX-13 (the inventory root).** An artifact's inventory root is the Merkle root over its leaves,
  built as a stream (14 §14.4). The layout rules and the openings are fixed by the class. A readiness
  proof opens this tree (08 PALW-VF-27).

**Sources:** ADR-0030 §1–§5, ADR-0070 D2–D5, ADR-0082 D5, ADR-0117 D1–D4, ADR-0106. **Code:**
`core/palw_step.rs`, `core/palw_step_leg.rs`, `core/palw_legs.rs`, `core/palw_artifact.rs`,
`core/palw_prompt_ids_v1.rs`.

## 4.5 Held context

- **PALW-EX-14 (held off the chain).** A long context MUST be held off the chain. The chain carries a
  root, an opening and a logarithm, never the capture:
  - the court opens at a named leaf, and nobody walks the leaf ladder;
  - a seat's unit is an interval of positions over the whole job, prefill included;
  - the state chunk map (v4) is append-only, with the checkpoint root as a frontier;
  - the ids never ride above one standard transaction.
- **PALW-EX-15 (the capture is a fold).** The executor folds each leaf hash as it is produced, so its
  capture is bounded in memory whatever the context. The executor prices a job against the ruleset's
  ladder. A seat recomputes the cache from the prompt it holds and never fetches it.
- **PALW-EX-16 (attention history).** The longest attention history an op reduces over MUST be the
  class's (`palw_attn_history_bound_v1`): `2^21` positions for a held class, and `2^18` otherwise. A held
  context past its bound is refused at admission.
- **PALW-EX-17 (the held regime).** A held class commits its prompt as a tiled Merkle root and is walked
  at the held ladder, `2^40` leaves (03 PALW-CL-22).
- **PALW-EX-18 (resuming a segment).** Verification V2 segments resume from committed checkpoints
  (`palw_segment_resume_v1.rs`). Sampled layers for the S3 sampler are drawn by
  `palw_layer_sample_v3` from the stored seed (08 §8.1).

**Sources:** ADR-0103 D1–D9, ADR-0082 D7–D9, ADR-0116 D1–D5, ADR-0118, ADR-0119, ADR-0081 D3 (the one
surviving part: `prompt_token_ids_hash` as a Merkle root), ADR-0133 §11.1. **Code:**
`core/palw_held_context_v1.rs`, `core/palw_state_chunk_map.rs`, `core/palw_segment_resume_v1.rs`,
`core/palw_layer_sample_v3.rs`.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis: the held regime, the prompt-ids Merkle root, `palw_token_lift`, `palw_fused_dissectable`, `palw_kimi_k3`, `palw_prefill_draw`, `palw_attn_anchored_root` |
