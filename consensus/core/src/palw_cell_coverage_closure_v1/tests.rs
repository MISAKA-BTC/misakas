//! REQ tests of RFC-0006 R6-CELL round 4's dormant coverage closure (`palw_cell_coverage_closure_v1`). Unverified source
//! (2026-10-15): written, not run.

use super::*;
use crate::Hash64;
use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
use crate::network::{NetworkId, NetworkType};
use crate::palw_cell_coverage_v1::palw_shipped_legacy_profiles_v1;
use crate::palw_state_chunk_map as maps;
use crate::palw_step::{PalwLayerKindV1, kernel_semantics_id_v1};

const FENCE: u64 = 1_000;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

/// testnet-12's bundle state params, as shipped (the closure's mirror `None`).
fn shipped_sp() -> PalwStateParamsV2 {
    match t12().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => panic!("testnet-12 is V2"),
    }
}

/// The shipped state params with the closure armed at [`FENCE`] (a fixture: no shipped ruleset can carry this).
fn armed_sp() -> PalwStateParamsV2 {
    shipped_sp().with_cell_coverage_closure_from_daa(Some(FENCE))
}

/// A shipped profile with BOTH layer kinds (a hybrid), on no map: the long form covers every section, so it is covered — the base
/// every refused fixture below changes one thing of.
fn hybrid_on_no_map() -> PalwShapeProfileV3 {
    let (_, mut profile) = palw_shipped_legacy_profiles_v1()
        .into_iter()
        .find(|(_, p)| {
            let kinds: Vec<PalwLayerKindV1> = (0..p.layer_count).map(|l| p.layer_kind(l)).collect();
            kinds.contains(&PalwLayerKindV1::Attention) && kinds.contains(&PalwLayerKindV1::GatedDeltaNet)
        })
        .expect("a shipped hybrid ladder family (Qwen3.6) constructs");
    profile.state_chunk_map_id = Hash64::default();
    assert_eq!(palw_cell_closure_legacy_refusal_v1(&profile), None, "no map: the long form covers every section");
    profile
}

fn with_map(map: Hash64) -> PalwShapeProfileV3 {
    let mut p = hybrid_on_no_map();
    p.state_chunk_map_id = map;
    p
}

/// `count` extra router nodes (copies of the attention table's first node, re-keyed) in the attention table.
fn with_routers(kernel: &str, count: usize) -> PalwShapeProfileV3 {
    let mut p = hybrid_on_no_map();
    let id = kernel_semantics_id_v1(kernel);
    let existing = p.attn_nodes.iter().filter(|n| n.kernel_semantics_id == id).count();
    for _ in existing..count.max(existing) {
        let mut node = p.attn_nodes[0].clone();
        node.kernel_semantics_id = id;
        p.attn_nodes.push(node);
    }
    p
}

fn unknown_map() -> PalwShapeProfileV3 {
    with_map(kernel_semantics_id_v1("not/a/state/chunk/map"))
}

#[test]
fn req_cell_the_fence_is_dormant_everywhere_and_refused_when_armed() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, t12()] {
        assert_eq!(p.palw_cell_coverage_closure_v1, None);
        assert_eq!(p.palw_cell_coverage_closure_v1_from_daa(), None);
        p.validate_palw_cell_coverage_closure_v1().unwrap();
        assert!(p.palw_fences_v1().contains(&("palw_cell_coverage_closure_v1", None)), "the exhaustive fence list names it");
    }
    assert_eq!(shipped_sp().cell_coverage_closure_from_daa(), None, "testnet-12's mirror is None");
    let mut p = t12();
    p.palw_cell_coverage_closure_v1 = Some(ForkActivation::never());
    p.validate_palw_cell_coverage_closure_v1().unwrap();
    p.palw_cell_coverage_closure_v1 = Some(ForkActivation::new(FENCE));
    assert!(p.validate_palw_cell_coverage_closure_v1().is_err(), "refused when armed: the rule is unverified");
    p.sync_palw_cell_coverage_closure_v1();
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => assert_eq!(bundle.state.cell_coverage_closure_from_daa(), Some(FENCE)),
        _ => panic!("testnet-12 is V2"),
    }
    // It pays and weights nothing: never an N3-gated reward profile.
    assert!(!crate::palw_beacon_bootstrap_v1::PALW_N3_GATED_REWARD_FENCES_V1.contains(&"palw_cell_coverage_closure_v1"));
}

const EVIDENCE_4: [[u8; 32]; 4] = [[1; 32], [2; 32], [3; 32], [4; 32]];

fn closure_record(genesis: Hash64, at: u64) -> PalwCellClosureArmingRecordV1 {
    PalwCellClosureArmingRecordV1 {
        version: PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORD_VERSION_V1,
        genesis,
        activation_daa: at,
        source_commit: [9; 20],
        evidence: &EVIDENCE_4,
        review: [0xAA; 32],
    }
}

/// **U11 (lane rfcgap-a): the closure is refused by RECORD, not by name.** The binary pins no record, so an armed real network is
/// still refused — now naming the missing arm-readiness record; a well-formed record for exactly this network and height admits it
/// (the mirror is still checked first); another height, another genesis or a malformed record does not.
#[test]
fn req_cell_the_closure_is_refused_until_a_record_for_this_network_and_height_is_pinned() {
    use crate::palw_g14_gates_v1::PalwG14ArmingRefusalV1 as R;
    assert!(PALW_CELL_COVERAGE_CLOSURE_ARMING_RECORDS_V1.is_empty(), "no record is pinned in this binary");
    assert_eq!(EVIDENCE_4.len(), PALW_CELL_COVERAGE_CLOSURE_ARMING_REQUIREMENTS_V1.len());
    assert!(PALW_CELL_COVERAGE_CLOSURE_ARMING_REQUIREMENTS_V1.iter().any(|r| r.starts_with("drill")), "a fence-crossing drill");
    let mut p = t12();
    p.palw_cell_coverage_closure_v1 = Some(ForkActivation::new(FENCE));
    // An unsynced mirror is refused before the record is asked.
    let Err(PalwModeV2Error::Invalid(why)) = p.validate_palw_cell_coverage_closure_in_v1(&[closure_record(p.genesis.hash, FENCE)])
    else {
        panic!("an unsynced mirror passed")
    };
    assert!(why.contains("mirror"), "{why}");
    p.sync_palw_cell_coverage_closure_v1();
    let Err(PalwModeV2Error::Invalid(why)) = p.validate_palw_cell_coverage_closure_v1() else { panic!("armed without a record") };
    assert!(why.contains("palw_cell_coverage_closure_v1") && why.contains("no arm-readiness record"), "{why}");
    assert!(!why.contains("not built"), "the rule is built; the refusal names the record: {why}");
    let g = p.genesis.hash;
    p.validate_palw_cell_coverage_closure_in_v1(&[closure_record(g, FENCE)]).expect("the exact record admits");
    assert!(p.validate_palw_cell_coverage_closure_in_v1(&[closure_record(g, FENCE + 1)]).is_err(), "another height");
    assert!(p.validate_palw_cell_coverage_closure_in_v1(&[closure_record(Hash64::from_bytes([5; 64]), FENCE)]).is_err(), "another net");
    // Malformed records refuse by reason.
    let mut r = closure_record(g, FENCE);
    r.version = 2;
    assert_eq!(palw_cell_closure_arming_record_form_v1(&r), Err(R::Version));
    let mut r = closure_record(g, FENCE);
    r.source_commit = [0; 20];
    assert_eq!(palw_cell_closure_arming_record_form_v1(&r), Err(R::SourceCommit));
    let mut r = closure_record(g, FENCE);
    r.evidence = &EVIDENCE_4[..3];
    assert_eq!(palw_cell_closure_arming_record_form_v1(&r), Err(R::EvidenceCount { expected: 4, got: 3 }));
    let mut r = closure_record(g, FENCE);
    r.review = [0; 32];
    assert_eq!(palw_cell_closure_arming_record_form_v1(&r), Err(R::Review));
    const REPEATED: [[u8; 32]; 4] = [[1; 32], [2; 32], [2; 32], [4; 32]];
    let mut r = closure_record(g, FENCE);
    r.evidence = &REPEATED;
    assert_eq!(palw_cell_closure_arming_record_form_v1(&r), Err(R::Evidence(2)));
    assert!(p.validate_palw_cell_coverage_closure_in_v1(&[r]).is_err(), "a malformed record never arms");
    // The whole V2 validation still refuses the armed closure on the shipped list (the node's path).
    assert!(p.validate_palw_v2().is_err());
}

/// **REQ-CELL-1**: an unknown map is refused at registration at/after the fence (keyed on the registration's own DAA), admitted
/// below it; a covered profile is admitted either side.
#[test]
fn req_cell_1_an_unknown_map_is_refused_at_registration_past_the_fence() {
    let unknown = unknown_map();
    let covered = hybrid_on_no_map();
    let armed = armed_sp();
    assert_eq!(palw_cell_closure_registration_refusal_v1(&armed, FENCE, &unknown), Some(PalwCellClosureRefusalV1::UnknownProfileKind));
    assert_eq!(
        palw_cell_closure_registration_refusal_v1(&armed, FENCE + 7, &unknown),
        Some(PalwCellClosureRefusalV1::UnknownProfileKind)
    );
    assert_eq!(palw_cell_closure_registration_refusal_v1(&armed, FENCE - 1, &unknown), None, "below the fence");
    assert_eq!(palw_cell_closure_registration_refusal_v1(&armed, FENCE, &covered), None, "covered");
    assert!(!PalwCellClosureRefusalV1::UnknownProfileKind.why().is_empty());
}

/// **REQ-CELL-1, per kind**: every uncovered kind the round-3 checker named is refused at admission past the fence — none is
/// silently admitted, and each is named by its own kind in the checker's gaps.
#[test]
fn req_cell_1_every_uncovered_kind_is_refused_at_admission() {
    use crate::palw_cell_coverage_v1::{PalwCellCoverageGapV1, palw_legacy_cell_coverage_v1};
    use crate::palw_step_refute::{KDESC_KIMI_ROUTER_TOPK, KDESC_Q36_ROUTER_TOPK};
    use PalwCellUncoveredKindV1 as K;
    let has_kind = |p: &PalwShapeProfileV3, kind: K| {
        palw_legacy_cell_coverage_v1(p)
            .unwrap_err()
            .iter()
            .any(|g| matches!(g, PalwCellCoverageGapV1::Uncovered { kind: k, .. } if *k == kind))
    };
    let armed = armed_sp();
    let cases: Vec<(&str, PalwShapeProfileV3, K)> = vec![
        ("hybrid v1 map", with_map(maps::hybrid_state_chunk_map_id_v1()), K::HybridRecurrenceUnenumerated),
        ("hybrid v2 map", with_map(maps::hybrid_state_chunk_map_id_v2()), K::HybridRecurrenceUnenumerated),
        ("solo gdn v1 map", with_map(maps::gdn_state_chunk_map_id_v1()), K::SoloRecurrenceMap),
        ("solo gdn v2 map", with_map(maps::gdn_state_chunk_map_id_v2()), K::SoloRecurrenceMap),
        ("solo gdn v3 map", with_map(maps::gdn_state_chunk_map_id_v3()), K::SoloRecurrenceMap),
        ("an attention-only map under GDN layers", with_map(maps::tiled_kv_state_chunk_map_id_v3()), K::MapSectionNotSpelled),
        ("two q36 routers in one table", with_routers(KDESC_Q36_ROUTER_TOPK, 2), K::MultiRouterTable),
        ("a Kimi router", with_routers(KDESC_KIMI_ROUTER_TOPK, 1), K::KimiRouter),
    ];
    for (name, profile, kind) in &cases {
        assert!(has_kind(profile, *kind), "{name}: the checker names {kind:?}");
        assert!(
            palw_cell_closure_registration_refusal_v1(&armed, FENCE, profile).is_some(),
            "{name}: refused at registration past the fence"
        );
        assert_eq!(palw_cell_closure_registration_refusal_v1(&armed, FENCE - 1, profile), None, "{name}: admitted below it");
    }
    // The solo map and the routers are the FIRST gap of a profile with no other defect, so the refusal names them exactly.
    assert_eq!(
        palw_cell_closure_legacy_refusal_v1(&with_map(maps::gdn_state_chunk_map_id_v2())),
        Some(PalwCellClosureRefusalV1::Uncovered(K::SoloRecurrenceMap))
    );
    assert_eq!(
        palw_cell_closure_legacy_refusal_v1(&with_routers(KDESC_Q36_ROUTER_TOPK, 2)),
        Some(PalwCellClosureRefusalV1::Uncovered(K::MultiRouterTable))
    );
    assert_eq!(
        palw_cell_closure_legacy_refusal_v1(&with_routers(KDESC_KIMI_ROUTER_TOPK, 1)),
        Some(PalwCellClosureRefusalV1::Uncovered(K::KimiRouter))
    );
}

/// **REQ-CELL-1b**: the claim gate refuses a claim accepted at/after the fence of a class whose graph the fold holds and is
/// uncoverable; where the fold does not hold the graph it defers to the court (REQ-CELL-2), never admits by default there.
#[test]
fn req_cell_1b_the_claim_gate_refuses_an_uncoverable_held_class() {
    let armed = armed_sp();
    let unknown = unknown_map();
    let covered = hybrid_on_no_map();
    assert!(palw_cell_closure_claim_refusal_v1(&armed, FENCE, Some(&unknown)).is_some());
    assert_eq!(palw_cell_closure_claim_refusal_v1(&armed, FENCE - 1, Some(&unknown)), None, "accepted before the fence: old rule");
    assert_eq!(palw_cell_closure_claim_refusal_v1(&armed, FENCE, Some(&covered)), None);
    assert_eq!(palw_cell_closure_claim_refusal_v1(&armed, FENCE, None), None, "graph not held: the court's (REQ-CELL-2)");
    assert!(palw_cell_closure_court_convicts_v1(armed.cell_coverage_closure_from_daa(), FENCE, &unknown), "…which convicts");
}

/// **REQ-CELL-2**: the court's closure predicate convicts an uncoverable class for a claim accepted at/after the fence, and only then.
#[test]
fn req_cell_2_the_court_never_acquits_an_unknown_map_past_the_fence() {
    let unknown = unknown_map();
    let covered = hybrid_on_no_map();
    assert!(palw_cell_closure_court_convicts_v1(Some(FENCE), FENCE, &unknown));
    assert!(palw_cell_closure_court_convicts_v1(Some(FENCE), u64::MAX, &unknown));
    assert!(!palw_cell_closure_court_convicts_v1(Some(FENCE), FENCE - 1, &unknown), "a claim accepted before it keeps its ruleset");
    assert!(!palw_cell_closure_court_convicts_v1(None, u64::MAX, &unknown), "unarmed: the live court");
    assert!(!palw_cell_closure_court_convicts_v1(Some(FENCE), FENCE, &covered), "a covered class is judged on its merits");
}

/// **REQ-CELL-3**: below the fence — on every shipped network — nothing here refuses or convicts anything, for every shipped profile
/// and every refused fixture.
#[test]
fn req_cell_3_below_the_fence_every_predicate_is_the_old_rule() {
    let shipped = shipped_sp();
    let mut profiles: Vec<PalwShapeProfileV3> = palw_shipped_legacy_profiles_v1().into_iter().map(|(_, p)| p).collect();
    profiles.push(unknown_map());
    profiles.push(with_map(maps::hybrid_state_chunk_map_id_v1()));
    for p in &profiles {
        for daa in [0, FENCE, u64::MAX] {
            assert!(!palw_cell_closure_governs_v1(&shipped, daa));
            assert_eq!(palw_cell_closure_registration_refusal_v1(&shipped, daa, p), None);
            assert_eq!(palw_cell_closure_claim_refusal_v1(&shipped, daa, Some(p)), None);
            assert!(!palw_cell_closure_court_convicts_v1(shipped.cell_coverage_closure_from_daa(), daa, p));
        }
    }
}
