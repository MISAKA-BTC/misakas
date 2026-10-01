//! **RFC-0007 Part II: seat-local algebraic verification of a PALW-TIR execution** — a prototype.
//!
//! A panel seat today replays a claim: it recomputes every multiply-accumulate of the job from the
//! weights it holds. This crate checks the same execution without the weights' work, and for the
//! weight matrices without the weights:
//!
//! * the producer serves a **witness** — the exact output of every `MatMul` the seat checks
//!   algebraically, which a TIR execution never commits (its accumulators are `i64`/`i128`,
//!   PALW-TIR-5) — next to the committed rows a capture already serves ([`witness`]);
//! * each served `MatMul` is checked by **Freivalds' algorithm** over a prime field: a weight
//!   product against the seat's secret **sketch** `S = W·v` of the weight, built once per epoch and
//!   `|free|` times smaller than `W` (per expert for a routed MoE weight, the expert being the one
//!   the seat computed itself); an activation × activation product with a fresh vector ([`geom`],
//!   [`sketch`], [`check`]);
//! * every **other** node — narrowings, norms, transcendentals, selection, state — is recomputed
//!   exactly by the reference evaluator, the court's own, from values already established
//!   ([`walk`]);
//! * each node is checked over the fewest primes whose product exceeds the span of its refined
//!   proven interval — one 61-bit Mersenne prime for every `i32` accumulator, two or three for wide
//!   ones ([`field`]); the vectors come from a seat secret that is never serialised ([`secret`]);
//! * the committed rows the seat derives are compared with the claim's, the first difference
//!   being the named leaf an accusation opens in the EXACT court, which is unchanged.
//!
//! **Seat-local.** No consensus object, rule or fence: two seats may check differently, and the
//! court never sees a sketch. What a protocol must add is the producer's duty to serve the witness
//! and its availability (RFC-0007 Part II, §II.7). The crate depends on the IR and the typed
//! backend and on nothing of consensus.
//!
//! [`analysis`] says which `MatMul` is checked how, read off the dataflow alone; [`fixture`] holds
//! the tiny classes the tests run; [`cost`] counts what a check costs on a real program's shapes
//! (the measurement tool, `--features measure`).

pub mod analysis;
pub mod check;
pub mod cost;
pub mod field;
pub mod fixture;
pub mod geom;
pub mod secret;
pub mod sketch;
mod walk;
pub mod witness;

pub use analysis::{TirActActPolicyV1, TirCheckPolicyV1, TirMatMulKindV1, TirSideV1, TirSketchAnalysisV1, TirWeightSourceV1};
pub use check::{TirCheckFailureV1, TirCheckFaultV1, TirCheckReportV1, TirSketchCheckerV1};
pub use field::{TirSketchModulusV1, tir_sketch_moduli_for_span_v1};
pub use secret::{TirSeatSketchSecretV1, TirSketchKeysV1};
pub use sketch::{TirSketchStatsV1, TirSketchStoreV1};
pub use witness::{TirSketchJobV1, TirTamperSiteV1, TirWitnessV1, tir_commit_root_v1, tir_witness_capture_v1, tir_witness_produce_v1};
