//! **ADR-0172 — model extensibility through versioned kernels, not a universal VM.**
//!
//! This crate is the reference implementation of the route RFC-0005 §§K.0–K.8, RFC-0011 §§15–16 and
//! RFC-0004 §0 specify, for the semantics MISAKA already has: PALW-TIR v1 programs and RFC-0003 pipelines of TIR v2 programs.
//! Three descriptors of one kernel line: K2-TIR-v1 (every TIR v1 family up to an `i64` accumulator), K2-TIR-v2 (the
//! multi-modulus dense relation for `i128` accumulators) and K2-TIR-v3 (the media-pipeline family). The completion matrix is
//! `docs/design/palw/kernel-k2-tir-v1.md`.
//!
//! ```text
//!   pinned model → active kernel + declarative VerificationPlan → committed node values →
//!   post-commit public challenges (small checks) → Pass  |  Fault (a public fault proof)  |  Unavailable
//! ```
//!
//! * [`descriptor`] — `KernelDescriptorV1` (RFC-0005 §K.2), the class binding `ModelKernelBindingV1`, and
//!   the binary's schedule of descriptors (`Proposed → Implemented → LockedIn → Active → Deprecated`).
//!   Registration selects only an `Active` descriptor; a code hash never authorizes anything.
//! * [`family`] — the reusable constraint families (§K.3), never one kernel per model brand.
//! * [`plan`] — `VerificationPlanV1`: **data** in a bounded typed grammar (relations over named
//!   primitives, checker ids, dimensions, state boundaries, declared budgets). No code, no plugin.
//! * [`check`] — the deterministic plan checker: coverage of every node, the descriptor's own checker
//!   per family, the integer-to-field alias bound, state boundaries, resource limits, and the
//!   whole-claim error **derived** from the suite (a declaration can never raise it).
//! * [`outcome`] — RFC-0011 §16.2's structured outcomes and §16.4's coverage buckets.
//! * [`field`], [`challenge`] — the Mersenne fields (GF(2^127 − 1), and the multi-modulus set of K2-TIR-v2) and the post-commit
//!   challenge stream (bind first, then draw; no modulo bias).
//! * [`beacon`] — RFC-0011 §15.3's anchor and beacon policy: a finalized window of blocks strictly after the commitments, the
//!   inclusion cutoff, counted reorg rebinds, and grinding as attempts in §15.4's network bound.
//! * [`trace`] — an honest producer's trace and its commitments (every node value of every position, committed before any
//!   challenge exists), the wiring every input is authenticated by, and the state root entering any position.
//! * [`evidence`] — RFC-0011 §15.3's `VerificationEvidenceV1`: network, ruleset, class, program, artifact, plan, job input, initial
//!   and final state, trace, output, context, a segment directory with derived boundary roots, and the suite's parameters.
//! * [`verify`] — the K2 reference verifier: Freivalds for `MatMul`, exact recompute for the cheap
//!   families, authenticated state continuity by wiring, and [`verify::KernelFaultProofV1`]: a fault
//!   localized to one primitive instance (one scalar for a `MatMul`) that **any** node re-checks from
//!   public material alone (ADR-0173 D1–D3) — no producer secret, no seat sketch, no Panel vote.
//! * [`receipt`] — RFC-0007 §V.6's `PalwConstraintReceiptV1` (one seat, one scope, one verdict), the fold's structural admission
//!   of it (scheme, evidence, challenge, scope, assignment, recomputed soundness, deadline) and the per-segment coverage tally.
//! * [`lifecycle`] — RFC-0011 §15.5's claim states (pass needs complete coverage; Final needs the window, retention and no open
//!   dispute; DA outcomes apart from convictions), the dispute budget any public bond files against, and §15.4's network bound.
//! * [`improve`] — RFC-0004 §0 on this route: the epoch's pinned kernel policy, candidate admission under a pinned kernel of the
//!   parent's family, assurance-labelled evaluation results, and §7.5's integer promotion rule kept apart from the computational error.
//! * [`assurance`] — RFC-0004 §0's labels for evaluation evidence and the promotion error budget.
//! * [`pipeline`] — RFC-0005 §K.3's media-pipeline family (K2-TIR-v3): each pipeline stage a claim over its v1 view with
//!   committed inputs, every edge (job values, canonical images, earlier stages' rows and finals, `R`) recomputed exactly, an
//!   edge court, pipeline plans and evidence.
//! * [`public`] — the 2026-10-07 amendments' measure: a fresh non-seat verifier built from a claim's published **bytes**, fault
//!   proofs and the court as byte-level operations, the withholding path (demand → served | producer default), RFC-0015 §1.1's
//!   G14 criteria per profile, and the reward gate that stays closed until they are complete and the material is public.
//!
//! # What this is not
//!
//! * **Not consensus.** No object tag, delta, carriage tail, wRPC op or DB prefix; the consensus fence
//!   `palw_probabilistic_constraints_v1` exists dormant with no height and is refused when armed (RFC-0011 §15.7). The built-in
//!   descriptors are `Implemented`, never `Active`, in [`descriptor::builtin_schedule_v1`]: a registration
//!   checked against them returns `KERNEL_NOT_ACTIVE`. A drill may pass its own schedule.
//! * **Not a VM.** There is no interpreter, ISA, syscall or uploaded checker. An unknown primitive,
//!   checker or family id is refused, never treated as success.
//! * **Not a soundness review.** The error bound is the Freivalds/union-bound arithmetic of this suite
//!   under its stated assumptions (post-commit unbiased beacon, BLAKE2b-512 binding). The independent
//!   review of the composition and of the beacon (see [`beacon`]) and the Panel/DA analyses RFC-0011 §15.3–§15.4 require are open.
//! * **Row/column Merkle commitments** ([`merkle`]). A `MatMul` scalar court opens one row of `X`, one column of `W` and one
//!   row of `Y` with their paths — `O(k + n)` bytes. Every other court still opens the whole instance it recomputes (one
//!   primitive's inputs and output); their worst cost is declared and bounded, not hidden ([`plan::PlanBudgetsV1`]).

pub mod assurance;
pub mod beacon;
pub mod challenge;
pub mod check;
pub mod descriptor;
pub mod evidence;
pub mod family;
pub mod field;
pub mod gate;
pub mod hash;
pub mod improve;
pub mod job;
pub mod ledger;
pub mod lifecycle;
pub mod merkle;
pub mod outcome;
pub mod pipeline;
pub mod plan;
pub mod public;
pub mod receipt;
pub mod trace;
pub mod verify;

pub use check::{PlanAcceptanceV1, check_plan_v1};
pub use descriptor::{
    KernelDescriptorV1, KernelScheduleV1, KernelStatusV1, ModelKernelBindingV1, builtin_schedule_v1, k2_tir_v1_descriptor,
    k2_tir_v2_descriptor, k2_tir_v3_descriptor,
};
pub use evidence::{VerificationEvidenceV1, build_evidence_v1};
pub use family::{CheckerIdV1, ConstraintFamilyV1, CourtIdV1};
pub use hash::Digest;
pub use outcome::{CoverageBucketV1, CoverageEvidenceV1, RegistrationOutcomeV1};
pub use plan::{VerificationPlanV1, plan_for_tir_program_v1};
pub use verify::{
    ClaimVerdictV1, KernelFaultProofV1, ScopeV1, ScopeVerdictV1, verify_claim_v1, verify_fault_proof_v1, verify_scope_v1,
};
