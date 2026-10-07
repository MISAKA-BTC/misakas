//! **ADR-0172 — model extensibility through versioned kernels, not a universal VM.**
//!
//! This crate is the reference implementation of the route RFC-0005 §§K.0–K.8, RFC-0011 §16 and
//! RFC-0004 §0 specify, for the one semantics MISAKA already has: PALW-TIR v1.
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
//! * [`field`], [`challenge`] — GF(2^127 − 1) and the post-commit challenge stream (bind first, then
//!   draw; no modulo bias).
//! * [`trace`] — an honest producer's trace and its evidence commitment (every node value of every
//!   position, committed before any challenge exists).
//! * [`verify`] — the K2 reference verifier: Freivalds for `MatMul`, exact recompute for the cheap
//!   families, authenticated state continuity by wiring, and [`verify::KernelFaultProofV1`]: a fault
//!   localized to one primitive instance (one scalar for a `MatMul`) that **any** node re-checks from
//!   public material alone (ADR-0173 D1–D3) — no producer secret, no seat sketch, no Panel vote.
//! * [`assurance`] — RFC-0004 §0's labels for evaluation evidence and the promotion error budget.
//!
//! # What this is not
//!
//! * **Not consensus.** No fence, object tag, delta, carriage tail, wRPC op or DB prefix. The built-in
//!   descriptor is `Implemented`, never `Active`, in [`descriptor::builtin_schedule_v1`]: a registration
//!   checked against it returns `KERNEL_NOT_ACTIVE`. A drill may pass its own schedule.
//! * **Not a VM.** There is no interpreter, ISA, syscall or uploaded checker. An unknown primitive,
//!   checker or family id is refused, never treated as success.
//! * **Not a soundness review.** The error bound is the Freivalds/union-bound arithmetic of this suite
//!   under its stated assumptions (post-commit unbiased beacon, BLAKE2b-512 binding). The independent
//!   review, the beacon construction and the Panel/DA analyses RFC-0011 §15.3–§15.4 require are open.
//! * **Flat commitments.** A node value is committed by one BLAKE2b-512 over its canonical bytes, so a
//!   fault proof opens the whole tensors of one relation. Chunked (Merkle) openings would shrink the
//!   court's bytes; their cost is reported, not hidden ([`plan::PlanBudgetsV1`]).

pub mod assurance;
pub mod challenge;
pub mod check;
pub mod descriptor;
pub mod family;
pub mod field;
pub mod hash;
pub mod outcome;
pub mod plan;
pub mod trace;
pub mod verify;

pub use check::{PlanAcceptanceV1, check_plan_v1};
pub use descriptor::{
    KernelDescriptorV1, KernelScheduleV1, KernelStatusV1, ModelKernelBindingV1, builtin_schedule_v1, k2_tir_v1_descriptor,
};
pub use family::{CheckerIdV1, ConstraintFamilyV1, CourtIdV1};
pub use hash::Digest;
pub use outcome::{CoverageBucketV1, RegistrationOutcomeV1};
pub use plan::{VerificationPlanV1, plan_for_tir_program_v1};
pub use verify::{ClaimVerdictV1, KernelFaultProofV1, verify_claim_v1, verify_fault_proof_v1};
