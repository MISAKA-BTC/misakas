# K2-TIR-v1 — the first versioned kernel (ADR-0172 / RFC-0005 §§K.0–K.8 / RFC-0011 §16 / RFC-0004 §0)

Status: reference implementation in `misaka-palw-kernel`, **dormant**. No fence, object tag, delta, carriage tail, wRPC op or DB prefix.
The shipped schedule lists the descriptor as `Implemented`; every registration checked against it returns `KERNEL_NOT_ACTIVE`.
This is not a soundness review, not a beacon design and not an activation proposal.

## What is built

| RFC item | Where | Notes |
| --- | --- | --- |
| `KernelDescriptorV1` (RFC05 §K.2) | `descriptor.rs` | Sub-ids are crate spellings (u32), not allocated wire ids. Families listed with ONE checker and ONE court each; soundness policy (target 128 bits, 2 repetitions, binding 256 bits) and resource ceilings are descriptor-wide, never plan-selected. Any change is a new digest. |
| `ModelKernelBindingV1` | `descriptor.rs` | `class_binding_id` under its own domain; legacy class ids are never rehashed. A different context is a different id (no "exact duplicate"). |
| Release states (ADR-0172 §3) | `KernelScheduleV1` | `Proposed → Implemented → LockedIn{daa} → Active{since} → Deprecated{stop_new}`. Unknown digest = never active. |
| Constraint families (RFC05 §K.3) | `family.rs` | structure, exact-arithmetic, dense-matrix, quant-range, nonlinear, selection, recurrent-state, media-pipeline. Every TIR v1 primitive has one family; none is media. |
| `VerificationPlanV1` (data, bounded grammar) | `plan.rs` | One relation per scheduled node (prim, family, checker, court, repetitions, occurrences, dims), one boundary per state, declared budgets and error bits. No code. |
| Deterministic plan checker (RFC05 §K.4 step 2) | `check.rs` | Re-derives everything from the program and descriptor: active descriptor; program validates and is in the primitive set; every node covered exactly once with the kernel's checker (no weaker checker, no fewer repetitions); the GF(2^127−1) alias bound for every `MatMul`; one boundary per state; budgets equal to the derived ones and within ceilings; the whole-claim error **derived** (`t·126 − ⌈log2 R⌉`, union with the binding term) ≥ target, and the declaration ≤ derived. |
| Registration outcomes (RFC11 §16.2) | `outcome.rs` | `ELIGIBLE_AT`, `FRONTEND_REQUIRED`, `KERNEL_EXTENSION_REQUIRED` (family, relation, required, available), `KERNEL_NOT_ACTIVE`, `BOUNDS_EXCEEDED`, `INCOMPLETE_COVERAGE`, `PLAN_FORGED`, `EXTERNAL_BLOCKER`. |
| Coverage buckets (RFC11 §16.4) | `outcome.rs` | Only `supported_active_kernel` (eligible AND on-chain registration evidence) counts as success; eligibility alone is `untested`. |
| Bind first, then draw (RFC11 §15.3) | `challenge.rs` | Seed = H(network, claim, class binding, plan root, evidence root, beacon); per-instance labelled streams; unbiased 127-bit sampling. **The beacon itself is not supplied.** |
| Evidence and wiring | `trace.rs` | Evidence = every node value of every occurrence of every position (flat BLAKE2b-512 per tensor). Inputs are wired, never stated: a `Fixed` state's entry value is the previous position's committed `StateWrite` output (zeros at 0); a `Hist` window's prior rows are the earlier positions' committed appended rows. |
| K2 checker (RFC11 §15.2 rows) | `verify.rs` | `MatMul`: Freivalds over GF(2^127−1), fresh public vector per batch slice and repetition. Other families: exact recompute from authenticated inputs (RFC11 §15.1 "small cheap constraints may be checked exactly"). Every opened output: dtype, shape, range. |
| Localization + terminal court | `verify.rs` | A `MatMul` mismatch recomputes the failing row and names one scalar; `verify_fault_proof_v1` re-authenticates the openings from **public** material (evidence, param commitments, tokens) and recomputes one scalar (k multiply-adds) or one instance. No producer state, no seat secret, no vote (ADR-0173 D1–D3). |
| DA vs fraud | `verify.rs` | A value not served, or served but not the committed one, is `Unavailable` — never a pass, never a conviction. |
| Assurance labels (RFC04 §0, §2.2) | `assurance.rs` | V/B/J/T + `ExactFold / ExactCourt / Legacy / Probabilistic{descriptor, bits}`; promotion's union bound; `None` (report the dependency) for Legacy or non-V results. |

## Tests (`cargo test -p misaka-palw-kernel`)

Unit: field arithmetic against double-and-add, sign mapping; descriptor identity, schedule standing; family table; challenge binding;
assurance union bound. End-to-end (`tests/k2_e2e.rs`) on RFC07 Part II's dense GQA + SwiGLU + top-2-of-4 MoE + history fixture:

* the tracer equals the reference evaluator's logits;
* shipped schedule → `KERNEL_NOT_ACTIVE`; armed schedule → `ELIGIBLE_AT` with ≥ 128 derived bits; `wide_v1`'s `i128` product and a descriptor
  without recurrent-state → `KERNEL_EXTENSION_REQUIRED` naming the family;
* plan forgeries refused: an omitted relation, a doubled one, a lower repetition, a swapped checker, an overclaimed error, an understated court
  budget, another program, too many positions, a court above the ceiling;
* an honest claim passes; one false scalar in a `MatMul` is found, localized to `(slice, i, j)`, convicted by a fresh court, and the same
  accusation against the honest claim is dismissed; a forged opening is not authentic; **every** scalar of one product, bumped alone, is caught;
* one false value in TopK, Gather, Div, Add and HistAppend is recomputed and convicted; a forged history window at a later position is caught at
  that position (its prior rows are re-read from the committed earlier rows);
* served ≠ committed and withheld material → `Unavailable`; a challenge not bound to this evidence → refused.

## What is NOT done (open, in the order the RFCs gate them)

1. **Soundness review** of the composition and of the alias bound (RFC11 §15.4, ADR-0172 §6). The finite fault corpus is not a proof.
2. **The beacon** and its withholding/grinding analysis (RFC11 §15.3); a multi-round/IOP transcript is not needed for direct Freivalds but is
   for any GKR aggregation, which is not built.
3. **Cost**: direct Freivalds reads `X`, `W`, `Y` (O(mk + kn + mn) per repetition) — for single-token GEMV this is no cheaper than recompute
   (RFC11 §15.2). The plan reports `verifier_work_per_position`, `evidence_bytes_per_position`, `artifact_bytes` and the worst court honestly;
   no large-model or 9B-8k measurement is made here.
4. **Commitments are flat**: a court opens whole tensors. Chunked/Merkle openings and their pricing are a commitment-suite extension.
5. **Media pipelines** (RFC-0003 v2 programs) have no family here → `KERNEL_EXTENSION_REQUIRED`. Multi-modulus dense relations for `i128`
   accumulators likewise.
6. **ADR-0173 / RFC-0014 gate**: the fault proof is objective and public, but authenticated public *availability* of every node value (who serves
   it, retention, demand/default) is the DA path of RFC-0009 stage B and RFC-0014, not built here. Reward/weight activation of any profile
   under this kernel stays closed until that gate passes.
7. **Consensus wiring**: no fence (`palw_probabilistic_constraints_v1` is proposed in RFC11 §15.7 and not added), no receipt type, no tally.
   Census integration reports the static outcome only (`palw-class census`), counted under RFC11 §16.4 buckets.
