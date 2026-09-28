//! **RFC-0002 Phase F, step F1: the `palw_tir_v1` fence** — dormant everywhere, fingerprinted with
//! its value where armed, invisible to the identity until it fires, named by the fork id, and
//! refused by `validate_palw_v2` when it names another build's primitive set, another court, wider
//! ceilings than the format allows, or prerequisites the ruleset lacks.
//!
//! The byte-identity of every shipped ruleset is pinned in `palw_tir_fences_are_dormant.rs`; this
//! file checks what the fence does once it is set.

use kaspa_consensus_core::config::params::{ForkActivation, Params, SIMNET_PARAMS, palw_t12_shipped_params};
use kaspa_consensus_core::fork_id_v1::fork_id_gate_fences_v1;
use kaspa_consensus_core::palw_tir_v1::{
    PALW_T12_TIR_CEILINGS_V1, PALW_T12_TIR_V1_ENTRY, PALW_TIR_COURT_VERSION_V1, PALW_TIR_PRIM_SET_DOMAIN_V1, PalwTirCeilingsV1,
    PalwTirFenceV1, palw_tir_court_root_v1, palw_tir_prim_set_id_v1,
};

/// A height no testnet-12 fence uses.
const AT: u64 = 9_999_991;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn t12_armed(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(at));
    p
}

#[test]
fn testnet_12_leaves_it_dormant_and_the_accessor_agrees() {
    let t12 = palw_t12_shipped_params();
    assert!(t12.palw_tir_v1.is_none(), "no ruleset arms the IR until its flag day is chosen");
    assert!(t12.palw_tir_v1_fence().is_none());
    assert!(!t12.palw_tir_v1_active_at(u64::MAX));
    assert!(t12.palw_fences_v1().contains(&("palw_tir_v1", None)), "the exhaustive fence list names it");
}

#[test]
fn a_scheduled_fence_moves_the_ruleset_and_the_schedule_and_never_the_identity() {
    let base = ids(&palw_t12_shipped_params());
    let armed = t12_armed(ForkActivation::new(AT));
    armed.validate_palw_v2().unwrap_or_else(|e| panic!("testnet-12 can arm it: {e}"));
    let moved = ids(&armed);
    assert_ne!(moved.0, base.0, "the ruleset a node announces names the fence and its value");
    assert_eq!(moved.1, base.1, "two builds that differ only about a FUTURE height stay peers");
    assert_ne!(moved.2, base.2, "the schedule reports it");
    assert!(armed.palw_tir_v1_fence().is_some() && !armed.palw_tir_v1_active_at(AT - 1) && armed.palw_tir_v1_active_at(AT));
}

#[test]
fn never_is_absence_for_the_identity_and_genesis_is_a_rule() {
    let base = palw_t12_shipped_params();
    let never = t12_armed(ForkActivation::never());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "Some(never()) collapses whole in the normaliser");
    never.validate_palw_v2().unwrap_or_else(|e| panic!("a dormant value validates: {e}"));
    let genesis = t12_armed(ForkActivation::always());
    assert_ne!(genesis.consensus_identity_id(), base.consensus_identity_id(), "a fence in force at genesis separates identities");
}

#[test]
fn the_value_is_fingerprinted_and_the_height_alone_is_what_the_identity_normalises() {
    let a = t12_armed(ForkActivation::new(AT));
    let mut b = a.clone();
    b.palw_tir_v1 = Some(PalwTirFenceV1::this_build_v1(
        ForkActivation::new(AT),
        PalwTirCeilingsV1 { max_program_bytes: 64_000, ..PALW_T12_TIR_CEILINGS_V1 },
    ));
    assert_ne!(a.consensus_params_id(), b.consensus_params_id(), "different ceilings are a different ruleset");
    let mut c = a.clone();
    c.palw_tir_v1 = Some(PalwTirFenceV1::this_build_v1(
        ForkActivation::new(AT),
        PalwTirCeilingsV1 { max_cone_work: 1 << 17, ..PALW_T12_TIR_CEILINGS_V1 },
    ));
    assert_ne!(a.consensus_params_id(), c.consensus_params_id(), "the cone-work cap is part of the value");
    assert_eq!(PALW_T12_TIR_CEILINGS_V1.max_cone_work, 1 << 16, "testnet-12's admission work cap");
    assert_ne!(a.consensus_schedule_id(), b.consensus_schedule_id(), "and the schedule report says so");
    assert_eq!(a.consensus_identity_id(), b.consensus_identity_id(), "while the height is in the future they peer");
}

#[test]
fn the_fork_id_gate_names_it_at_its_height() {
    let armed = t12_armed(ForkActivation::new(AT));
    assert!(fork_id_gate_fences_v1(&armed).contains(&AT), "past its height an un-upgraded node is refused");
    assert!(!fork_id_gate_fences_v1(&palw_t12_shipped_params()).contains(&AT));
    assert!(armed.palw_fences_v1().contains(&("palw_tir_v1", Some(ForkActivation::new(AT)))));
}

#[test]
fn validate_refuses_every_value_this_build_cannot_run() {
    let ok = t12_armed(ForkActivation::new(AT));
    assert!(ok.validate_palw_tir_v1().is_ok());
    let with = |edit: &dyn Fn(&mut PalwTirFenceV1)| {
        let mut p = ok.clone();
        let mut fence = p.palw_tir_v1.unwrap();
        edit(&mut fence);
        p.palw_tir_v1 = Some(fence);
        p.validate_palw_tir_v1()
    };
    assert!(with(&|f| f.prim_set_id = kaspa_consensus_core::Hash64::from_bytes([7u8; 64])).is_err(), "another primitive set");
    assert!(with(&|f| f.court_version = PALW_TIR_COURT_VERSION_V1 + 1).is_err(), "another court");
    assert!(with(&|f| f.ceilings.max_program_bytes = 262_145).is_err(), "past the decoder's byte cap");
    assert!(with(&|f| f.ceilings.max_program_bytes = 0).is_err(), "a zero byte cap admits nothing");
    assert!(with(&|f| f.ceilings.max_context = (1 << 21) + 1).is_err(), "past history_bound");
    assert!(with(&|f| f.ceilings.max_unrolled_nodes = 525_313).is_err(), "past the format's occurrences");
    assert!(with(&|f| f.ceilings.max_macs_per_position = 0).is_err(), "a zero cost ceiling");
    assert!(with(&|f| f.ceilings.max_cone_work = (1 << 20) + 1).is_err(), "past admission's own work cap");
    assert!(with(&|f| f.ceilings.max_cone_work = 0).is_err(), "a zero work cap admits nothing");
    assert!(with(&|f| f.activation = ForkActivation::never()).is_ok(), "a dormant value is never refused");

    let mut no_audit = ok.clone();
    no_audit.palw_audit_2026_09_11 = None;
    assert!(no_audit.validate_palw_tir_v1().is_err(), "without A-2 an older build fails the block that carries an IR object");
    let mut no_court = ok.clone();
    no_court.palw_kary_court = None;
    assert!(no_court.validate_palw_tir_v1().is_err(), "IR disputes include history dissections");
    let mut late_court = ok.clone();
    late_court.palw_kary_court = Some(ForkActivation::new(AT + 1));
    assert!(late_court.validate_palw_tir_v1().is_err(), "the court must be in force at or below the fence");

    let mut not_v2 = SIMNET_PARAMS;
    assert!(!matches!(not_v2.palw_consensus_mode, kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    not_v2.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    assert!(not_v2.validate_palw_tir_v1().is_err(), "a ConsensusV2 rule");
    assert!(not_v2.palw_tir_v1_fence().is_none(), "and the accessor never answers off V2");
}

#[test]
fn the_ids_are_keyed_hashes_of_what_they_name() {
    let descriptor = misaka_palw_tir::prim::prim_set_descriptor_v1();
    let expected = blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_PRIM_SET_DOMAIN_V1).hash(&descriptor);
    assert_eq!(palw_tir_prim_set_id_v1().as_byte_slice(), expected.as_bytes(), "prim_set_id = H_key(descriptor)");
    assert_eq!(
        palw_tir_prim_set_id_v1().as_byte_slice(),
        &misaka_palw_tir::prim::PRIM_SET_ID_V1[..],
        "the network's id is the one normal form requires every program to declare (NF-1)"
    );
    let id = palw_tir_prim_set_id_v1();
    assert_ne!(palw_tir_court_root_v1(&id, 1), palw_tir_court_root_v1(&id, 2), "the court version is inside the root");
    assert_ne!(
        palw_tir_court_root_v1(&id, 1),
        kaspa_consensus_core::palw_catalog_coverage::palw_court_catalog_root_v1(),
        "and the IR root is not the bundle's root"
    );
}

#[test]
fn the_flag_day_entry_sets_the_field_it_names() {
    let mut p = palw_t12_shipped_params();
    (PALW_T12_TIR_V1_ENTRY.set)(&mut p, Some(ForkActivation::new(AT)));
    assert_eq!(p.palw_tir_v1, Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT))));
    assert!(p.palw_fences_v1().iter().any(|(name, _)| *name == PALW_T12_TIR_V1_ENTRY.name), "the entry's name is a fence's");
    (PALW_T12_TIR_V1_ENTRY.set)(&mut p, None);
    assert_eq!(ids(&p), ids(&palw_t12_shipped_params()), "setting it back is the shipped ruleset, byte for byte");
}
