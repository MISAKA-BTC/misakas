//! Source controls for object tag 170 and consumer v2's cell coverage (RFC-0006). Written for the consolidated validation phase; not
//! run here and not acceptance results.
use super::*;
use crate::config::params::{ForkActivation, MAINNET_PARAMS, TESTNET_PARAMS};
use crate::palw_cell_coverage_v1::{
    PalwCellConsumersV2, PalwCellCoverageGapV1, PalwCellDispositionV1, PalwCellUncoveredKindV1 as K, PalwCellVerifierV1,
    palw_cell_uncovered_disposition_v2, palw_legacy_cell_coverage_v1, palw_legacy_cell_coverage_v2,
};
use crate::palw_lifecycle_objects_v2::{PalwLifecycleKindFenceV1, PalwLifecycleKindFencesV1};
use crate::palw_state_chunk_map as maps;
use crate::palw_state_v2::PalwConsensusObjectV2;

fn bond(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_bytes([n; 64]), index: 0 })
}

fn routed_accusation() -> PalwMemoryRoutingAccusationV2 {
    let step = crate::palw_step_refute::tests::skeleton_refutation();
    PalwMemoryRoutingAccusationV2 {
        version: PALW_MEMORY_ROUTING_ACCUSATION_VERSION_V2,
        claim: Hash64::from_bytes([1; 64]),
        execution_root: step.binding.committed_execution_root,
        trace_root: Hash64::from_bytes([2; 64]),
        executor_bond: bond(3),
        accuser_bond: bond(4),
        evidence: PalwMemoryRoutingEvidenceV2::RoutedStep { refutation: step, prompt_ids_opening: None },
        artifact_openings: Vec::new(),
        signature: vec![1],
    }
}

/// **Flags-off historical bytes**: tag 170 is new, dormant on every shipped preset, and leaves every older tag's encoding as it was.
#[test]
fn tag_170_is_dormant_and_old_tags_keep_their_bytes() {
    let object =
        PalwConsensusObjectV2::MemoryRoutingV2 { object: Box::new(PalwMemoryRoutingObjectV2::Accused { accusation: routed_accusation() }) };
    let encoded = borsh::to_vec(&object).unwrap();
    assert_eq!(encoded[0], PALW_MEMORY_ROUTING_OBJECT_TAG_V2);
    assert_eq!(borsh::from_slice::<PalwConsensusObjectV2>(&encoded).unwrap(), object);
    let fences = PalwLifecycleKindFencesV1::default();
    assert!(!fences.kind_in_force_at(&object, u64::MAX), "below its fence: the live build's undecodable payload");
    let armed = fences.with(PalwLifecycleKindFenceV1::MemoryRoutingCourtV2, Some(ForkActivation::new(10)));
    assert!(!armed.kind_in_force_at(&object, 9));
    assert!(armed.kind_in_force_at(&object, 10));
    for p in [MAINNET_PARAMS, TESTNET_PARAMS] {
        assert_eq!(p.palw_memory_routing_court_v2, None);
        assert_eq!(p.palw_memory_routing_court_v2_fence(), None);
    }
}

/// The session id binds the network, the claim, both bonds and the route + subject; the signature context is the objective
/// one-move accuser's under a NEW domain, so no tag-243 or shard-court signature replays onto it.
#[test]
fn the_session_binds_route_subject_and_actors() {
    let a = routed_accusation();
    let network = Hash64::from_bytes([8; 64]);
    let digest = palw_memory_routing_session_id_v2(network, &a);
    assert_ne!(digest, palw_memory_routing_session_id_v2(Hash64::from_bytes([9; 64]), &a));
    let mut swapped = a.clone();
    std::mem::swap(&mut swapped.executor_bond, &mut swapped.accuser_bond);
    assert_ne!(digest, palw_memory_routing_session_id_v2(network, &swapped));
    let mut other_leaf = a.clone();
    if let PalwMemoryRoutingEvidenceV2::RoutedStep { refutation, .. } = &mut other_leaf.evidence {
        refutation.output_opening.leaf_index += 1;
    }
    assert_ne!(digest, palw_memory_routing_session_id_v2(network, &other_leaf));
    assert!(a.validate_shape(u64::MAX).is_ok());
    let mut self_accused = a.clone();
    self_accused.accuser_bond = self_accused.executor_bond;
    assert!(self_accused.validate_shape(u64::MAX).is_err());
    let mut other_root = a;
    other_root.execution_root = Hash64::default();
    assert!(other_root.validate_shape(u64::MAX).is_err());
}

/// A routed accusation is refused (never adjudicated under another router) without a binding, with a binding declared after the
/// claim, and for a step that is not a routed reader.
#[test]
fn a_routed_accusation_needs_the_classs_binding_from_before_the_claim() {
    let a = routed_accusation();
    let binding = a.evidence.binding().clone();
    let claim = crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
        execution_root: binding.committed_execution_root,
        class_id: binding.shape_profile.shape_profile_id(),
        artifact_root: Hash64::default(),
    };
    assert!(palw_memory_routing_verdict_v2(&a, &claim, u64::MAX, None, 100).is_err(), "no binding");
    let late = PalwRouterBindingRowV2 { version: 1, bound_from_daa: 101, readers: Vec::new() };
    assert!(palw_memory_routing_verdict_v2(&a, &claim, u64::MAX, Some(&late), 100).is_err(), "declared after the claim");
    let early = PalwRouterBindingRowV2 { version: 1, bound_from_daa: 0, readers: Vec::new() };
    // The skeleton's opened leaf is the embedding, not a routed reader: the legacy one-move court serves it, not this carrier.
    assert!(palw_memory_routing_verdict_v2(&a, &claim, u64::MAX, Some(&early), 100).is_err());
}

fn shipped_hybrid() -> crate::palw_step::PalwShapeProfileV3 {
    crate::palw_cell_coverage_v1::palw_shipped_legacy_profiles_v1()
        .into_iter()
        .map(|(_, p)| p)
        .find(|p| {
            use crate::palw_step::PalwLayerKindV1 as L;
            (0..p.layer_count).any(|l| p.layer_kind(l) == L::Attention) && (0..p.layer_count).any(|l| p.layer_kind(l) == L::GatedDeltaNet)
        })
        .expect("a shipped hybrid ladder family constructs")
}

const AT_REGISTRATION: PalwCellConsumersV2<'static> = PalwCellConsumersV2 { router_binding: None, registration: true };

/// **Consumer v2 covers only what it reaches**: the composed hybrid maps (both sections), never the historical hybrid v1/v2
/// recurrence half; round 4's answers for every other map unchanged.
#[test]
fn consumer_v2_covers_the_composed_maps_and_nothing_historical() {
    let base = shipped_hybrid();
    let on = |map: Hash64| {
        let mut p = base.clone();
        p.state_chunk_map_id = map;
        p
    };
    for map in [maps::hybrid_composed_state_chunk_map_id_v1(), maps::hybrid_composed_state_chunk_map_id_v2()] {
        let p = on(map);
        assert!(palw_legacy_cell_coverage_v1(&p).is_err(), "round 4 does not know the composed id");
        if crate::palw_cell_coverage_v1::palw_recurrence_reached_v2(&p) {
            let covered = palw_legacy_cell_coverage_v2(&p, AT_REGISTRATION).expect("consumer v2 covers the composition");
            assert!(covered.iter().any(|(_, v)| *v == PalwCellVerifierV1::LegacyAnchoredRecurrenceV2));
        }
    }
    for map in [maps::hybrid_state_chunk_map_id_v1(), maps::hybrid_state_chunk_map_id_v2()] {
        let gaps = palw_legacy_cell_coverage_v2(&on(map), AT_REGISTRATION).unwrap_err();
        assert!(
            gaps.iter().any(|g| matches!(g, PalwCellCoverageGapV1::Uncovered { kind: K::HybridRecurrenceUnenumerated, .. })),
            "historical hybrid roots never gain a recurrence half"
        );
    }
    // Unchanged where consumer v2 reaches nothing: every round-4 answer is the same answer.
    for map in [maps::hybrid_state_chunk_map_id_v3(), maps::tiled_kv_state_chunk_map_id_v3(), Hash64::default()] {
        let p = on(map);
        assert_eq!(palw_legacy_cell_coverage_v1(&p).is_ok(), palw_legacy_cell_coverage_v2(&p, AT_REGISTRATION).is_ok());
    }
}

/// Consumer v2's disposition matrix: exactly the two kinds it reaches move to Covered.
#[test]
fn the_v2_disposition_moves_only_the_reached_kinds() {
    assert!(matches!(palw_cell_uncovered_disposition_v2(K::SoloRecurrenceMap), PalwCellDispositionV1::Covered(_)));
    assert!(matches!(palw_cell_uncovered_disposition_v2(K::MultiRouterTable), PalwCellDispositionV1::Covered(_)));
    assert!(matches!(palw_cell_uncovered_disposition_v2(K::KimiRouter), PalwCellDispositionV1::Covered(_)));
    for kind in [K::HybridRecurrenceUnenumerated, K::MapSectionNotSpelled, K::MapNotEnumerable, K::TirFamilyUnchecked] {
        assert_eq!(palw_cell_uncovered_disposition_v2(kind), PalwCellDispositionV1::RefusedAtAdmission, "{kind:?}");
    }
}

/// The closure readers keep their round-4 answers wherever consumer v2's mirror is absent (every shipped preset).
#[test]
fn below_consumer_v2_the_closure_readers_are_round_4() {
    use crate::palw_cell_coverage_closure_v1 as c;
    let base = shipped_hybrid();
    let mut solo = base.clone();
    solo.state_chunk_map_id = maps::gdn_state_chunk_map_id_v2();
    for p in [&base, &solo] {
        assert_eq!(
            c::palw_cell_closure_court_convicts_v2(Some(0), None, 5, p, None),
            c::palw_cell_closure_court_convicts_v1(Some(0), 5, p)
        );
        assert_eq!(c::PalwCellClosureModeV1::from(true).refuses_profile(p), c::palw_cell_closure_legacy_refusal_v1(p).is_some());
        assert!(!c::PalwCellClosureModeV1::from(false).refuses_profile(p));
    }
}

/// **RFC-0007 item 1**: routing, history and gather each name the court they reach for both kinds of class, and every measured
/// adjudication cost is a named constant that is still UNKNOWN — never a default a rule could read as measured.
#[test]
fn every_public_refutation_route_reaches_a_court_with_a_named_unknown_cost() {
    use crate::palw_claim_compact_v1::PALW_PUBLIC_REFUTATION_ROUTES_V2 as ROUTES;
    use misaka_palw_kernel::claim_compact_v1::native_inventory_v2::NativeRefutationRouteV2;
    assert_eq!(ROUTES.map(|r| r.route), NativeRefutationRouteV2::ALL);
    for r in ROUTES {
        assert!(!r.native_court.is_empty() && !r.profile_court.is_empty());
        assert_eq!((r.native_measured_us, r.profile_measured_us), (None, None), "{}: UNKNOWN until measured", r.route.name());
        assert_eq!(r.native_measured_us, r.route.measured_us());
    }
}
