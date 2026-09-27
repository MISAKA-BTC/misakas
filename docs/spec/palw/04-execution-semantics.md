# PALW spec — 04. Execution semantics

> **Skeleton (Phase 1, 2026-09-27).** [00-index.md](00-index.md) gives the conventions.

**Purpose.** This chapter defines what "the inference ran, as committed" means, bit for bit. It
covers the one execution family and its integer arithmetic, the operation set and transcendentals,
how a forward is cut into steps and tiles and committed, how held (long) context is represented, and
which kernels are part of consensus. A seat re-executes against this chapter, and the court
adjudicates against it. Runtime backends may do anything below this boundary (chapter 14).

**Principles served:** P3 (the committed execution is the answered one), P5 (backends below the
semantic boundary are free to be faster).

## 4.1 One execution family

- [ ] There is exactly one execution family, and the court is not optional. *Sources:* 0053
  (supersedes 0051), 0026. *Code:* `core/palw_base0.rs`, `core/palw_backend.rs`.
- [ ] The semantic boundary is the catalogue: what is consensus, and what is a backend's choice.
  *Sources:* 0057 D1, D6. *Code:* `core/palw_catalog_coverage.rs`.

## 4.2 BASE-0 arithmetic and its tiers

- [ ] The integer-only arithmetic: representation, the three arithmetic rules, the closed operation
  set, reduction order. The normative body of 0040 moves here whole, or into a sub-chapter
  `04a-base0-arithmetic.md` if it is too long. *Sources:* 0040 A–G. *Code:* `core/palw_base0.rs`,
  `core/palw_base0_ops.rs`, `core/palw_base0_profile.rs`.
- [ ] The residual site: its narrowing and its per-tensor, per-layer gain. *Sources:* 0050.
- [ ] The A16 tier: sixteen-bit activations. *Sources:* 0047. *Code:* `core/palw_base0_a16.rs`.
- [ ] The QWEN36 hybrid operations: router selection, the combine, `IntLn`, the gated delta rule,
  partial rotation. *Sources:* 0052 (as amended). *Code:* `core/palw_qwen36_ops.rs`,
  `core/palw_qwen36_profile.rs`.
- [ ] Canonical transcendentals: exp and log are algorithms, not functions. *Sources:* 0031.
  *Code:* `core/palw_transcendental.rs`.
- [ ] Kimi K3 (armed on testnet-12 from genesis): what its class adds. *Sources:* TODO, name the ADR
  (0133 and 0135 cite it). *Code:* `core/palw_kimi_k3_*.rs`, fence `palw_kimi_k3`.

## 4.3 Kernels are the build

- [ ] The kernel set is a consensus set. A kernel that a network has not armed is not in its identity.
  *Sources:* 0067 D3, 0102 D2–D4. *Code:* `court_catalog_root`, fence `palw_token_lift` (the embedding
  lift, read per token, 0102).
- [ ] A fused row is admitted only if the court can dissect it. *Sources:* 0093 D6/D8. *Code:* fence
  `palw_fused_dissectable`.

## 4.4 Steps, tiles and commitments

- [ ] The step function, pinned at tile granularity (shape profile v3): what a step and a tile are,
  and how `tile_len` is bounded. Accounting never reads `tile_len` (P4, chapter 05). *Sources:* 0030,
  0144 §5. *Code:* `core/palw_step.rs`, `core/palw_step_leg.rs`, `core/palw_legs.rs`.
- [ ] Model-tier step spaces are adjudicable end to end: the commitments a model-tier claim carries.
  *Sources:* 0070 §3–§4.
- [ ] A draw is one forward: the prefill is one pass over the weights. *Sources:* 0117. *Code:* fence
  `palw_prefill_draw`.
- [ ] The inventory root over an artifact's leaves, as a definition. How a node builds it is in
  chapter 14. *Sources:* 0106. *Code:* `core/palw_artifact.rs`.

## 4.5 Held context

- [ ] The context is held off the chain. The chain carries a root, an opening and a logarithm.
  *Sources:* 0103, 0081 (what remains after 0082). *Code:* `core/palw_held_context_v1.rs`.
- [ ] The capture is a fold. The close is flat in the context. *Sources:* 0082 (the capture and fold
  decisions; the court decisions are in chapter 09).
- [ ] The attention history belongs to the class, and the held regime reduces over its own width.
  *Sources:* 0116.
- [ ] The held regime arrives at a height, and a held class carries its own prompt form. *Sources:*
  0118, 0119.
- [ ] Sampled layers, and resuming a segment. *Code:* `core/palw_layer_sample_v3.rs`,
  `core/palw_segment_resume_v1.rs`. *Sources:* TODO, name the ADR in Phase 2.

**Activation.**

| Network | Status |
| --- | --- |
| mainnet | not active (PALW disabled) |
| testnet-12 | from genesis. `palw_token_lift`, `palw_fused_dissectable`, `palw_kimi_k3` and `palw_attn_anchored_root` are armed at DAA 0 by `palw_t12_arm_every_rule_from_genesis`. Confirm `palw_prefill_draw` in Phase 2 |

**Design:** `design/palw/execution.md` and `design/palw/held-context.md`.
