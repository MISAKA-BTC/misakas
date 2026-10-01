//! **PALW-TIR v1 on a node: a typed execution backend** (RFC-0002 Phase F, step F9).
//!
//! The reference evaluator (`misaka-palw-tir`) is the meaning of "correct": every element an
//! `i128`, every param cloned, every primitive a straightforward loop. That is what a court should
//! run on one disputed tile and what no node can run on a 1.5B model. This crate computes the SAME
//! function — byte-identical at every node, every commit point and every error — over native
//! storage:
//!
//! * **Typed.** A tensor of dtype `i8`/`i16`/`i32`/`i64`/`idx` is a `Vec` or slice of that native
//!   type ([`elem`]); `i128` storage exists only where a node declares `i128` (spec 04b §2.1).
//! * **Zero-copy params** ([`params`]): borrowed slices, reinterpreted in place from a mapped
//!   little-endian artifact.
//! * **Views, not copies** ([`layout`]): `Reshape` of a contiguous value, `Transpose`, `Slice`,
//!   `Broadcast`, a scalar-index `Gather` (an embedding row) and `HistAppend`'s window are strided
//!   layouts over their input's storage.
//! * **State without per-step copies** ([`exec`]): `Fixed` states are double-buffered; `Hist`
//!   states are append-only row buffers whose window is one contiguous run.
//! * **Proved-away checks** ([`ranges`], [`plan`]): the interval rules of spec 04b §7 decide per
//!   node whether the reference's runtime checks (PALW-TIR-23/24, divisors, indices) can fire; where
//!   they cannot, the kernel runs in wrapping native arithmetic, which is then exact. Where they
//!   can, the kernel checks exactly what the reference checks, so success, failure and the failure
//!   class agree. An executor plans each occurrence once more with the ACTUAL ranges of the params
//!   it holds ([`plan::TirPlan::refine`]) — sound for its whole life, since its params never change —
//!   so an A16 narrowing that must be typed `i128` for arbitrary weights computes, and is stored, in
//!   `i64` for the weights at hand.
//! * **Reordering only where it is free** ([`kernels::matmul`]): an exact sum's value and its
//!   success are order-independent (the order-free rule, PALW-TIR-24), so `MatMul` and `ReduceSum`
//!   vectorise and thread; every lossy site (`Div`, `Clamp`, the transcendentals, selection,
//!   state) computes each element exactly as spec 04b §6 defines it.
//! * **Weights within a budget** ([`tiers`], [`rows`], `node::residency`; ADR-0112 for IR classes,
//!   `docs/design/palw/tir/runtime-residency.md`): the program's dataflow tells the params every
//!   forward reads whole (pinned) from those a route selects rows of (routed) and those an input
//!   selects rows of (gathered); a residency serves the last two as the rows a `Gather` names, read
//!   through the file descriptor, and computes the same bytes.
//!
//! Node software only: `kaspa-consensus-core` never depends on this crate. The court runs the
//! reference; the tests hold the two equal (golden vectors, whole programs, random programs,
//! range-extreme inputs).

pub mod cone;
pub mod elem;
pub mod exec;
pub mod fused;
pub mod kernels;
pub mod layout;
#[cfg(feature = "node")]
pub mod node;
pub mod params;
pub mod plan;
pub mod ranges;
pub mod rows;
pub mod scalar;
pub mod tiers;

pub use cone::eval_cone;
pub use elem::{Buf, Elem, Slice};
pub use exec::{NoSink, NodeValue, StepSink, TirExecutor};
pub use params::{ParamData, TirParams};
pub use plan::TirPlan;
pub use rows::{TirRowCountsV1, TirRowSourceV1, TirRowsInMemoryV1};
pub use tiers::{TirResidencyArithmeticV1, TirTierRulesV1, TirTierV1, TirTiersV1};
