//! **Which `MatMul` a seat checks how, read off the program's dataflow** (RFC-0007 Part II, §II.2) — the analysis itself lives in
//! `misaka_palw_tir::dataflow`, because consensus pins the canonical serving set in the class profile with the same code
//! (`kaspa_consensus_core::palw_mesh_v1`); this module re-exports it under the names the checker has always used.
pub use misaka_palw_tir::dataflow::*;
