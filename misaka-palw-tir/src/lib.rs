//! **PALW-TIR v1 — the PALW Canonical Tensor IR (RFC-0002), and its reference evaluator.**
//!
//! **An IR, not a VM.** A program is a finite static DAG, a static layer schedule and a scan over
//! positions: no program counter, branch, jump, loop or call, and every cost is known at
//! registration. The *reference evaluator* ([`interp::Interpreter`] — the type keeps its first
//! name) walks that graph; it is the meaning of "correct" for the court.
//!
//! A class program is data: a static DAG over a closed set of twenty-five integer primitives
//! ([`prim::Prim`]), carried as a [`program::TirProgramV1`] whose canonical Borsh bytes are its
//! identity. The court will know these primitives and nothing about models.
//!
//! The normative text is `docs/spec/palw/04b-tensor-ir.md`; the corpus it was derived from is
//! `docs/design/palw/tir/corpus-v1.md`. This crate is the FIRST implementation: slow and obviously
//! correct — straightforward loops, every value an `i128`, no SIMD, no reordering — and total:
//! every malformed program and every out-of-range value is an [`error::TirError`], never a panic.
//! A second implementation is written from the spec alone; the golden vectors under
//! `consensus-vectors/tir-v1/` are what the two must both reproduce.
//!
//! * [`types`], [`prim`], [`program`] — the IR and its wire form.
//! * [`validate`] — structural normal form and shape/type inference (the part of admission the
//!   evaluator needs to be total; ranges, costs and cones are Gate 2).
//! * [`arith`], [`eval`] — the primitives' integer semantics.
//! * [`interp`] — one position step, a multi-position run, and cone evaluation.
//! * [`builder`] — a small program builder and the first composite templates (the seed of
//!   `tir_library_v1`).
//!
//! This is a leaf crate: it depends on nothing in this repository, so `kaspa-consensus-core` can
//! depend on it for admission and the court. Identity hashing stays with the caller.

pub mod arith;
pub mod builder;
pub mod error;
pub mod eval;
pub mod interp;
pub mod prim;
pub mod program;
pub mod tensor;
pub mod types;
pub mod validate;

pub use error::{TirError, TirErrorKind, TirResult};
pub use interp::{ConeEnv, Interpreter, MapParams, ParamSource, RunState, StepOutput};
pub use prim::{Cmp, Prim, Rounding};
pub use program::{Block, ConstDecl, Node, ParamDecl, Ref, Schedule, StateDecl, StateKind, TirProgramV1};
pub use tensor::Tensor;
pub use types::{DType, Dim, TensorType};
