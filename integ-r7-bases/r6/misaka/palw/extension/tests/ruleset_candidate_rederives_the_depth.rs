//! **A ruleset candidate prints what the arming build prints, depths included** (ADR-0152 §4-quater
//! P-1; adopted from the second review's probe of `feat/t12-class-verify-deadline`).
//!
//! Past `palw_class_verify_deadline` a V2 network's pruning depth is the D_cap claim lattice (74,920
//! on testnet-12, against 12,002 without the fence), derived from the fences rather than carried. A
//! candidate that set the fence by name and printed the params as they were would describe a build
//! that does not exist: a params id the arming build never prints, and a depth that `validate_palw_v2`
//! refuses (K18). `ruleset_candidate::verify` therefore re-derives the depths after arming its fences
//! ([`with_derived_depths_v1`]); these tests pin that it lands on the arming build.
//!
//! Run: `cargo test -p misaka-palw-extension --test ruleset_candidate_rederives_the_depth -- --nocapture`

use kaspa_consensus_core::config::params::{ForkActivation, Params, palw_t12_shipped_params, palw_v2_pruning_depth_v1};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use misaka_palw_extension::kinds::ruleset_candidate::{set_fence_by_name, with_derived_depths_v1, would_print_v1};
use misaka_palw_extension::{PalwExtensionClassificationV1 as C, PalwExtensionDepthV1 as D, PalwExtensionEnvV1, verify_extension_v1};

fn bundle(p: &Params) -> kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2 {
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else {
        panic!("a ConsensusV2 network")
    };
    bundle.clone()
}

/// testnet-12 as a build without the fence derives it: the fence off, its mirror synced, the depth
/// re-derived without it.
fn t12_without_the_fence() -> Params {
    let mut without = palw_t12_shipped_params();
    without.palw_class_verify_deadline = None;
    without.sync_palw_class_verify_deadline();
    let b = bundle(&without);
    without.blockrate.pruning_depth = palw_v2_pruning_depth_v1(&without.blockrate, &b, without.palw_da_court, None);
    without
}

/// **The genesis candidate for the deadline fence, on testnet-12 without it, is testnet-12.** Setting
/// the fence by name and re-deriving the depths — exactly the steps `verify` takes — prints the
/// shipped testnet-12's params, identity and schedule ids, carries its 74,920-DAA depth and
/// validates. Without the re-derivation (the review's finding) the same candidate kept 12,002,
/// printed another params id and was refused K18 although the arming build validates.
#[test]
fn a_genesis_candidate_for_the_deadline_fence_prints_the_arming_build() {
    let armed = palw_t12_shipped_params();
    let without = t12_without_the_fence();
    assert_eq!(without.pruning_depth(), 12_002, "the premise: the depth before P-1");
    without.validate_palw_v2().expect("the fence-off build validates");

    let mut set = without.clone();
    set_fence_by_name(&mut set, "palw_class_verify_deadline", ForkActivation::always()).expect("genesis is accepted");
    let candidate = with_derived_depths_v1(set.clone());
    println!("candidate depth {} / arming build {}", candidate.pruning_depth(), armed.pruning_depth());
    assert_eq!(candidate.pruning_depth(), 74_920, "the D_cap lattice");
    assert_eq!(candidate.finality_depth(), armed.finality_depth());
    let (printed, real) = (would_print_v1(&candidate), would_print_v1(&armed));
    assert_eq!(printed.params_id, real.params_id, "the ruleset the arming build announces");
    assert_eq!(printed.identity_id, real.identity_id, "the identity it peers under");
    assert_eq!(printed.schedule_id, real.schedule_id, "the schedule it logs");
    candidate.validate_palw_v2().expect("and validates, as the arming build does");

    // The counterfactual the fix closes: the fence set, the depth carried.
    assert_eq!(set.pruning_depth(), 12_002);
    assert_ne!(would_print_v1(&set).params_id, real.params_id, "a params id no build prints");
    let refused = set.validate_palw_v2().expect_err("a depth under the lattice");
    println!("without the re-derivation: {refused:?}");
}

/// Re-deriving is a no-op where nothing moved: the shipped testnet-12 (already at 74,920) and a V2
/// network without the fence keep their ids.
#[test]
fn rederiving_the_depths_moves_nothing_that_did_not_move() {
    let armed = palw_t12_shipped_params();
    assert_eq!(would_print_v1(&with_derived_depths_v1(armed.clone())).params_id, would_print_v1(&armed).params_id);
    let without = t12_without_the_fence();
    assert_eq!(would_print_v1(&with_derived_depths_v1(without.clone())).params_id, would_print_v1(&without).params_id);
    let t11: Params = NetworkId::with_suffix(NetworkType::Testnet, 11).into();
    assert_eq!(would_print_v1(&with_derived_depths_v1(t11.clone())).params_id, would_print_v1(&t11).params_id);
}

/// **Through the verifier**: a testnet-11 candidate arming the deadline fence at genesis records the
/// arming build's params id WITH its depths re-derived — the value `with_derived_depths_v1` gives over
/// the fence set by name.
#[test]
fn the_verifier_records_the_rederived_arming_build() {
    let t11 = NetworkId::with_suffix(NetworkType::Testnet, 11);
    let params: Params = t11.into();
    let manifest = format!(
        r#"{{
  "manifest": "misaka-palw/extension-manifest/v1",
  "kind": "ruleset-candidate",
  "name": "the class-verify deadline at genesis",
  "network": "testnet-11",
  "requires": {{ "fences": {{ "palw_class_verify_deadline": "genesis" }} }},
  "declares": {{ "object_id": "{}" }},
  "admission": {{ "object": "none" }}
}}"#,
        "0".repeat(128)
    );
    let dir = tempfile::tempdir().unwrap();
    let report = verify_extension_v1(manifest.as_bytes(), dir.path(), &PalwExtensionEnvV1::genesis(t11), D::Full)
        .unwrap_or_else(|e| panic!("boundary refusal: {e}"));
    assert!(matches!(report.classification, C::RulesetChange { .. }), "{:?}", report.classification);
    let mut set = params.clone();
    set_fence_by_name(&mut set, "palw_class_verify_deadline", ForkActivation::always()).expect("genesis is accepted");
    let derived = with_derived_depths_v1(set.clone());
    println!(
        "testnet-11: depth {} -> fence set {} -> re-derived {}",
        params.pruning_depth(),
        set.pruning_depth(),
        derived.pruning_depth()
    );
    assert_eq!(
        report.recomputed.get("arming.params_id").map(String::as_str),
        Some(derived.consensus_params_id().to_string().as_str()),
        "the verifier prints the re-derived arming build"
    );
}
