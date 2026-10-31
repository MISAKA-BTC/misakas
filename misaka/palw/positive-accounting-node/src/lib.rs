//! Node-side A5 adapters. The accounting leaf stays independent of consensus
//! and execution crates, avoiding consensus -> accounting -> executor cycles.
//! Native ingress/activation remains owned by A0 and is not implied by these
//! locally checked terminals or transaction preflight results.

pub mod dues;
pub mod native;
pub mod rows;
#[cfg(feature = "tir-checker")]
pub mod tir;
