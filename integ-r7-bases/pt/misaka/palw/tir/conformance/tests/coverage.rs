//! **The coverage claim, checked against the live catalogue**: the kernels this crate conforms are
//! EXACTLY the integer part of `catalogued_kernel_ids_v1()` plus `fenced_kernel_ids_v1()`, so a
//! kernel added to the court without a segment and a test fails here, by name.

use std::collections::BTreeSet;

use kaspa_consensus_core::palw_step::kernel_semantics_id_v1;
use kaspa_consensus_core::palw_step_refute::{catalogued_kernel_ids_v1, fenced_kernel_ids_v1, kimi_fenced_kernel_ids_v1};
use misaka_palw_tir_conformance::{CONFORMED_V1, FLOAT_V1, KIMI_OUT_OF_SCOPE};

fn ids(descs: &[&str]) -> BTreeSet<kaspa_consensus_core::Hash64> {
    descs.iter().map(|d| kernel_semantics_id_v1(d)).collect()
}

#[test]
fn every_integer_catalog_kernel_and_the_fenced_lift_are_conformed() {
    let catalog = catalogued_kernel_ids_v1();
    let float = ids(FLOAT_V1);
    assert_eq!(float.len(), 7, "seven float kernels");
    assert!(float.is_subset(&catalog), "the float kernels are catalogued");
    let integer: BTreeSet<_> = catalog.difference(&float).copied().collect();
    assert_eq!(integer.len(), 38, "thirty-eight integer kernels in the catalogue");
    let fenced = fenced_kernel_ids_v1();
    let conformed = ids(CONFORMED_V1);
    assert_eq!(conformed.len(), CONFORMED_V1.len(), "no kernel listed twice");
    let want: BTreeSet<_> = integer.union(&fenced).copied().collect();
    let missing: Vec<&str> = CONFORMED_V1.iter().filter(|d| !want.contains(&kernel_semantics_id_v1(d))).copied().collect();
    assert!(missing.is_empty(), "conformed but not an integer or fenced kernel: {missing:?}");
    assert_eq!(conformed, want, "the conformed set is exactly the integer catalogue plus the fenced RequantizeByToken");
}

/// The Kimi K3 arms are recorded as defective and out of scope: fenced, not catalogued, and not
/// claimed.
#[test]
fn the_kimi_arms_are_recorded_out_of_scope() {
    let kimi = ids(KIMI_OUT_OF_SCOPE);
    assert_eq!(kimi, kimi_fenced_kernel_ids_v1(), "every fenced Kimi arm is recorded");
    assert!(kimi.is_disjoint(&catalogued_kernel_ids_v1()));
    assert!(kimi.is_disjoint(&ids(CONFORMED_V1)), "a defective arm is never claimed as conformant");
}
