//! Source controls for object tag 244 and consumer v2's cell coverage (RFC-0006). Written for the consolidated validation phase; not
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

/// **Flags-off historical bytes**: tag 244 is new, dormant on every shipped preset, and leaves every older tag's encoding as it was.
#[test]
fn tag_244_is_dormant_and_old_tags_keep_their_bytes() {
    let object = PalwConsensusObjectV2::MemoryRoutingV2 {
        object: Box::new(PalwMemoryRoutingObjectV2::Accused { accusation: routed_accusation() }),
    };
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
            (0..p.layer_count).any(|l| p.layer_kind(l) == L::Attention)
                && (0..p.layer_count).any(|l| p.layer_kind(l) == L::GatedDeltaNet)
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

/// **RFC06-4 — one closure predicate on every blame path.** Below consumer v2 each path keeps its historical answer (`round4`:
/// the round-4 `bool` exactly; the kind-4 / terminal paths `Off`); past it every path gets the same `MemoryRoutingV2` mode, a
/// binding declared after the claim is not the claim's, and the structural refusal equals the court's (REQ-CELL-2) and the claim
/// gate's (REQ-CELL-1b) answers for the same profile and binding.
#[test]
fn every_blame_path_judges_the_claim_like_the_claim_gate_and_the_court() {
    use crate::palw_cell_coverage_closure_v1 as c;
    use crate::palw_router_binding_v2::{PALW_ROUTER_BINDING_VERSION_V2, PalwRouterBindingRowV2};
    let none = c::PalwClosureMirrorsV1::default();
    let round4 = c::PalwClosureMirrorsV1 { cell_closure_from_daa: Some(0), memory_routing_from_daa: None };
    let v2 = c::PalwClosureMirrorsV1 { cell_closure_from_daa: Some(0), memory_routing_from_daa: Some(10) };
    assert_eq!(c::PalwCellClosureModeV1::for_blame_path(none, 5, None, true), c::PalwCellClosureModeV1::Off);
    assert_eq!(c::PalwCellClosureModeV1::for_blame_path(round4, 5, None, true), c::PalwCellClosureModeV1::Legacy);
    assert_eq!(c::PalwCellClosureModeV1::for_blame_path(round4, 5, None, false), c::PalwCellClosureModeV1::Off);
    // Accepted before consumer v2's height: still each path's historical mode.
    assert_eq!(c::PalwCellClosureModeV1::for_blame_path(v2, 9, None, true), c::PalwCellClosureModeV1::Legacy);
    assert_eq!(c::PalwCellClosureModeV1::for_blame_path(v2, 9, None, false), c::PalwCellClosureModeV1::Off);
    let early = PalwRouterBindingRowV2 { version: PALW_ROUTER_BINDING_VERSION_V2, bound_from_daa: 12, readers: Vec::new() };
    let late = PalwRouterBindingRowV2 { version: PALW_ROUTER_BINDING_VERSION_V2, bound_from_daa: 21, readers: Vec::new() };
    for path_round4 in [true, false] {
        assert_eq!(
            c::PalwCellClosureModeV1::for_blame_path(v2, 20, Some(&early), path_round4),
            c::PalwCellClosureModeV1::MemoryRoutingV2 { router_binding: Some(&early) },
            "past consumer v2 every path is the same mode"
        );
        assert_eq!(
            c::PalwCellClosureModeV1::for_blame_path(v2, 20, Some(&late), path_round4),
            c::PalwCellClosureModeV1::MemoryRoutingV2 { router_binding: None },
            "a binding declared after the claim is not the claim's"
        );
    }
    let params =
        crate::palw_state_v2::PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_bytes([1; 64]), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_cell_coverage_closure_from_daa(Some(0))
            .with_memory_routing_court_from_daa(Some(10));
    let q35 = crate::palw_qwen36_profile::qwen36_profile_v1(crate::palw_qwen36_profile::QWEN35_2B).expect("the 3.5-2B row");
    let base = shipped_hybrid();
    let mut composed = base.clone();
    composed.state_chunk_map_id = maps::hybrid_composed_state_chunk_map_id_v2();
    for p in [&base, &composed, &q35] {
        for binding in [None, Some(&early)] {
            let mode = c::PalwCellClosureModeV1::for_blame_path(v2, 20, binding, false);
            assert_eq!(mode.refuses_profile(p), c::palw_cell_closure_court_convicts_v2(Some(0), Some(10), 20, p, binding));
            assert_eq!(mode.refuses_profile(p), c::palw_cell_closure_claim_refusal_v2(&params, 20, Some(p), binding).is_some());
        }
    }
    // The routed arithmetic reads the bound router only under consumer v2 with a binding.
    assert_eq!(c::PalwCellClosureModeV1::Legacy.bound_router_slot(&q35, 0), None);
    assert_eq!(c::PalwCellClosureModeV1::MemoryRoutingV2 { router_binding: None }.bound_router_slot(&q35, 0), None);
}

/// **Genesis family pin — id 8, Qwen3.6-35B-A3B on the legacy Qwen3.6 integer runtime** (the one genesis GDN family on the legacy
/// step court; ids 2, 3 and 6 are TIR programs, judged by the native TIR courts). Its registrable row is graph-v8: profile version
/// 3 on the held composition whose recurrence half spells the 16 key / 32 value heads (v5), per-position cadence. Past consumer
/// v2 it is covered with NO router binding (one q36 router per layer table, the legacy routing appendix), its recurrence geometry
/// derives under the key-head layout, and its recurrence leaves enter the anchored court's replay gate.
#[test]
fn the_genesis_qwen36_moe_row_is_covered_by_consumer_v2_without_a_binding() {
    use crate::palw_cell_coverage_closure_v1 as c;
    let p = crate::palw_context_ladder::palw_qwen36_context_row_profile_v8(8192).expect("the 8k graph-v8 row projects");
    assert!(maps::palw_profile_is_held_v4(&p) && maps::palw_profile_spells_key_heads_v1(&p), "held v5, key heads spelled");
    let claim_time = PalwCellConsumersV2 { router_binding: None, registration: false };
    assert!(c::palw_cell_closure_refusal_v2(&p, claim_time).is_none(), "covered at the claim gate with no binding");
    assert!(palw_legacy_cell_coverage_v2(&p, claim_time).is_ok());
    let geometry = crate::palw_gdn_state_map_v2::PalwGdnStateGeometryV2::of(&p).expect("the key-head recurrence layout derives");
    assert!(geometry.key_heads < geometry.heads, "16 key heads grouped over 32 value heads");
    assert!(
        crate::palw_context_ladder::palw_checkpoint_leaf_carries_recurrence_v1(
            &p,
            crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4
        ),
        "a recurrence checkpoint every history tile — the held route's demand unit"
    );
}
