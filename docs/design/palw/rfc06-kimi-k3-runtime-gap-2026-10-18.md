# RFC0006 Kimi K3 runtime boundary (source review, 2026-10-18)

Status: implementation gap; no build, test, node run, or measurement performed.
Kimi-linear TIR is a different profile and cannot supply this consumer.

The repository's K3 definitions comprise `palw_kimi_k3_profile.rs`,
`palw_kimi_k3_ops.rs`, `palw_kimi_k3_artifact_v1.rs`, and the fenced court
dispatch. Base0 and the SDK have no K3 artifact reader, mapped holding, execution
backend, or producer capture. The artifact helper hashes named byte vectors;
it does not define a complete tensor file layout, calibrated parameter
inventory, loader, or actual executable architecture. The foundation module
explicitly describes a weightless derivation, not an actual K3 run.

The existing profile cannot be connected by silently copying the Q36 engine:

- MLA K/V writes have width `kv_lora_rank + qk_rope_head_dim` (576 on the card),
  while `kimi_k3_mla_fused` consumes already decompressed, position-major,
  head-major K and V rows (`heads * d_qk` and `heads * d_v`). The profile has no
  explicit decompression graph connecting those representations. Filling
  absent matrices, their orientation, positional rules, or parameter scales
  from assumptions would invent an architecture.
- The old KDA court dispatch allocates a zero state per call using the entire
  opened K/V row lengths and reads only the first decay/beta entry. A real
  per-head recurrent runtime needs persistent authenticated per-head state,
  its transition and checkpoint relation, and the matching court consumer.
  Replacing this dispatch in place would reinterpret its old kernel ID.
- The profile's routed parameter names do not specify a full trainable expert
  architecture and converter. A root over arbitrary named bytes does not
  establish that they are the model's actual tensor inventory.

A new runtime requires authoritative architecture and artifact definitions
before implementation: concrete tensor names/shapes/dtypes/orientations,
quantization/calibration and tokenizer rules, latent expansion and position
semantics, and exact KDA head/state transition. The source must identify which
K3 version and weight revision it describes. No such complete producer contract
was found in the repository during this review. External authoritative input
is being requested by the coordinating agent.

After those inputs exist, an explicit new profile/kernel/state-map version
must connect the actual loader and bounded inventory to SDK holding resolution,
producer, independent checker, retained capture, native per-reader routing,
parameter openings, state checkpoints, DA, and direct court adjudication.
Every buffer and simultaneously live owner must be reserved before allocation.
The old profile, artifact-root domain, wire IDs and historical court meanings
remain unchanged. The public activation refusal remains in place until the
whole new contract and consolidated acceptance pass are complete.

Neither data membership nor matching replay supplies selected-verifier
approval, model review, credit, or a claim of cryptographic computation proof.
This gap is not marked complete by adding a guard, a helper, or another
family's executable backend.
