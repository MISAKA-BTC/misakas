//! REQ-T6-4 tests: every unit D2 restricts has a replacement route that compels no model bytes, and a full collusion on a new claim
//! (producer + every Panel seat) is still convicted on it by one public bonded verifier outside the Panel. Unverified source
//! (2026-10-15, round 3): written, not run.

use super::*;
use crate::config::params::Params;
use crate::network::{NetworkId, NetworkType};
use crate::palw_checkpoint_court_v1::tests::{HeldFixture, fixture_prompt_ids, held_fixture};
use crate::palw_court_scope_v1::{palw_court_demand_allowed_v1, palw_court_unit_scope_v1};
use crate::palw_mode_v2::PalwConsensusMode;
use crate::palw_ruling_fences_v1::{PALW_D2_RESTRICTED_UNITS_V1, palw_court_model_scope_admits_v1};
use crate::palw_state_v2::PalwStateParamsV2;
use crate::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, PalwStepOutLenV1, PalwStepTableV1, canonical_step_leaf_index};
use crate::palw_step_refute::PalwExecutionStepRefutationV1;

const LADDER: u64 = 1 << 26;
const ARTIFACT_ROOT: u64 = 0xA7;
const FENCE: u64 = 1_000;

fn h64(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(v), index: 0 })
}

/// testnet-12's state params with D2 armed at [`FENCE`] (a fixture: no shipped ruleset carries it).
fn d2_armed_sp() -> PalwStateParamsV2 {
    match Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12)).palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state.with_court_model_scope_from_daa(Some(FENCE)),
        _ => panic!("testnet-12 is V2"),
    }
}

/// A claim record (every other field zero) of the fixture's execution, accepted at `accepted`.
fn claim_record(fx: &HeldFixture, accepted: u64) -> PalwClaimStateV2 {
    use borsh::BorshDeserialize;
    let zeros = [0u8; 8192];
    let mut c = <PalwClaimStateV2 as BorshDeserialize>::deserialize(&mut &zeros[..]).expect("a zero claim decodes");
    c.accepted_daa = accepted;
    c.class_id = fx.class_id();
    c.execution_root = fx.binding.committed_execution_root;
    c.bond = bond(1);
    c
}

fn one_move_claim(fx: &HeldFixture) -> PalwOneMoveClaimV2 {
    PalwOneMoveClaimV2 {
        execution_root: fx.binding.committed_execution_root,
        class_id: fx.class_id(),
        artifact_root: h64(ARTIFACT_ROOT),
    }
}

/// The claim part of `leaf` as the accused serves it on chain past the fence: the committed half, NO artifact openings.
fn claim_part(fx: &HeldFixture, leaf: u64, prompt: bool) -> PalwLeafEvidenceV1 {
    let preimage = fx.preimages.get(&leaf).expect("the committed tile").clone();
    let position = preimage.coord.position;
    PalwLeafEvidenceV1 {
        refutation: PalwExecutionStepRefutationV1 {
            binding: fx.binding.clone(),
            output_opening: fx.opening(leaf),
            output_preimage: preimage,
            inputs: vec![],
            prompt_token_ids: vec![],
            decode_tokens: None,
            kv_checkpoint: None,
        },
        artifact_openings: vec![],
        prompt_ids_opening: prompt
            .then(|| crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&fixture_prompt_ids(20), position).expect("the prompt tile")),
    }
}

/// Layer 1's fused attention site at prefill position 12, with a tile of `count` lanes committed there (the canonical width when
/// `None`): its leaf.
fn fused_fixture(count: Option<u32>) -> (HeldFixture, u64) {
    let fx = held_fixture(true, 20, None);
    let profile = &fx.binding.shape_profile;
    let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
    let slot = profile.global_node_slot(PalwStepTableV1::Attn, 1, fused).expect("a slot");
    let (node, _) = profile.resolve_node_slot(slot).expect("the node");
    let PalwStepOutLenV1::Fixed { elements } = node.out_len else { panic!("a fused site commits a fixed row") };
    let coord = PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: 12, tile_index: 0 };
    let count = count.unwrap_or(elements.min(node.tile_len));
    let fx = fx.with_committed_tile(coord, count, vec![0u8; 4 * count as usize]);
    let leaf = canonical_step_leaf_index(&fx.binding.shape_profile, &fx.binding.job_context, &coord).expect("a leaf");
    (fx, leaf)
}

// ---- the inventory -------------------------------------------------------------------------------------------------------------

/// **REQ-T6-4, the inventory**: each of the four restricted units — exactly the ARMED t12 units that compel model bytes — has a
/// replacement route; every unit any route moves is admissible under the D2 rule (a compellable unit is claim-specific); every model
/// byte on a route comes from a verifier's own copy or is already public; and a court route ends in an existing adjudicator.
#[test]
fn every_restricted_unit_has_a_replacement_route_that_compels_no_model_bytes() {
    use PalwD2ReplacementUnitV1 as U;
    assert_eq!(
        PALW_D2_REPLACEMENT_ROUTES_V1.iter().map(|r| r.restricted).collect::<Vec<_>>(),
        PALW_D2_RESTRICTED_UNITS_V1.to_vec(),
        "one route per restricted unit, in the fence's order"
    );
    for kind in PALW_D2_RESTRICTED_UNITS_V1 {
        // The restricted unit itself: compelled model bytes on an armed route (why D2 closes it).
        assert!(palw_court_demand_allowed_v1(kind).is_err(), "{kind:?}");
        assert!(palw_court_unit_scope_v1(kind).route.armed_on_testnet_12(), "{kind:?}");
        let route = palw_d2_replacement_route_v1(kind).expect("a replacement route");
        assert_eq!(route.restricted, kind);
        assert!(!route.units.is_empty(), "{kind:?}");
        for unit in route.units {
            assert!(palw_d2_replacement_unit_admissible_v1(*unit), "{kind:?}: {unit:?} compels model bytes");
            let scope = palw_d2_replacement_unit_scope_v1(*unit);
            if scope.material.is_model_bytes() {
                assert!(
                    matches!(scope.supplier, CourtSupplierV1::VerifierOwnCopy | CourtSupplierV1::Public),
                    "{kind:?}: {unit:?}'s model bytes are not a verifier's own copy"
                );
            }
        }
        // Each court route reads model bytes from the CHALLENGER (its rows, or the operands it published at opening).
        if route.verdict != PalwD2VerdictRouteV1::ReadinessDigest {
            assert!(route.units.contains(&U::AccuserLeafRows), "{kind:?}: the terminal reads the accuser's rows");
        }
        if matches!(route.verdict, PalwD2VerdictRouteV1::AttnDissection | PalwD2VerdictRouteV1::ConeDissection) {
            let published = route.units.iter().position(|u| *u == U::ChallengerPublishedOperands).expect("published at opening");
            let relayed = route.units.iter().position(|u| *u == U::RelayedRootOperands).expect("relayed into the root claim");
            assert!(published < relayed, "{kind:?}: the operands are public before the accused's turn");
        }
    }
    // Every replacement unit, exhaustively.
    for unit in [
        U::HeldLeafClaimPart,
        U::AccuserLeafRows,
        U::RootClaimCoordinates,
        U::ChallengerPublishedOperands,
        U::RelayedRootOperands,
        U::ReadinessDigest,
        U::ReadinessRefutationLeaf,
    ] {
        assert!(palw_d2_replacement_unit_admissible_v1(unit), "{unit:?}");
    }
    // What the accused can be made to supply on these routes is claim material only.
    for unit in [U::HeldLeafClaimPart, U::RootClaimCoordinates] {
        assert!(palw_d2_replacement_unit_scope_v1(unit).material.is_claim_specific(), "{unit:?}");
    }
    // Units D2 does not restrict have no replacement (they are not closed).
    for kind in [CourtUnitKindV1::HeldStepRange, CourtUnitKindV1::AccuserArtifactOpenings, CourtUnitKindV1::RCoreEvent] {
        assert!(palw_d2_replacement_route_v1(kind).is_none(), "{kind:?}");
    }
    // The fold carries the routes: the named-leaf opening asks the published operands, each opening object hands its own.
    let sessions = include_str!("../palw_state_v2/da_sessions.rs");
    assert!(sessions.contains("crate::palw_court_model_scope_routes_v1::palw_d2_named_leaf_operands_admit_v1("));
    let state = include_str!("../palw_state_v2.rs");
    assert!(state.contains("crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::Attn {"));
    let objects = include_str!("../palw_state_v2/objects.rs");
    assert!(objects.contains("crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::TirParams(&refutation.params)"));
    assert!(objects.contains("crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::GenParams(&close.params)"));
    let held_close = include_str!("../palw_held_close_fold_v1.rs");
    assert!(held_close.contains("crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1::None,"));
}

// ---- G14 under D2: the full collusion ------------------------------------------------------------------------------------------

/// **REQ-T6-4 / G14: producer + every Panel seat collude on a claim accepted past `palw_court_model_scope_v1`; one public bonded
/// verifier outside the Panel convicts it objectively, from public authenticated material only, with no model byte taken from the
/// accused.** The Panel licensed the lie (its votes are no input to any verdict below); the producer will not serve its weight rows
/// (D2 closes the `StepLeaf` demand that compelled them). The verifier demands the claim part (public once answered; a withheld one
/// is the DA default), supplies its OWN rows, and files the one move.
#[test]
fn req_t6_4_full_collusion_on_a_new_claim_is_convicted_by_the_replacement_route() {
    let sp = d2_armed_sp();
    let verifier = bond(0x0E7);

    // (a) A non-fused leaf: the producer committed a malformed tile (a lie at the leaf's own root).
    let honest = held_fixture(true, 20, None);
    let (&leaf, preimage) = honest.preimages.iter().next().expect("a cache-write tile");
    let lying = held_fixture(true, 20, None).with_committed_tile(preimage.coord, 1, vec![0; 4]);
    let new_claim = claim_record(&lying, FENCE);
    // The old route is closed for this claim (D2), and stays open for a claim accepted below the fence.
    assert!(palw_court_model_scope_admits_v1(&sp, &new_claim, CourtUnitKindV1::HeldStepLeaf).is_err());
    assert_eq!(palw_court_model_scope_admits_v1(&sp, &claim_record(&lying, FENCE - 1), CourtUnitKindV1::HeldStepLeaf), Ok(()));
    // The replacement: the claim part (no rows) + the verifier's rows (none needed at this leaf) → the one move convicts.
    let part = claim_part(&lying, leaf, false);
    assert_eq!(
        palw_d2_one_move_on_claim_part_v1(&part, &[], &one_move_claim(&lying), LADDER),
        Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        "the producer is convicted on public material, by a bond that is no seat ({verifier:?})"
    );

    // (b) A fused attention site: a malformed committed tile convicts in one move too (the structural pass at the named leaf).
    let (fused, fused_leaf) = fused_fixture(Some(1));
    assert_eq!(
        palw_d2_one_move_on_claim_part_v1(&claim_part(&fused, fused_leaf, true), &[], &one_move_claim(&fused), LADDER),
        Ok(PalwShardCourtVerdictV1::ExecutorGuilty)
    );

    // (c) An honest fused tile defers to the dissection — whose root claim the accused files from PUBLIC operands: the opening must
    // publish them, so an opening that publishes nothing (or bytes that are not the registered model's) does not open it.
    let (fair, fair_leaf) = fused_fixture(None);
    assert_eq!(
        palw_d2_one_move_on_claim_part_v1(&claim_part(&fair, fair_leaf, true), &[], &one_move_claim(&fair), LADDER),
        Ok(PalwShardCourtVerdictV1::NeedsDissection)
    );
    let fair_claim = claim_record(&fair, FENCE);
    assert_eq!(
        palw_d2_named_leaf_operands_admit_v1(&fair_claim, fair_leaf, &PalwD2OpeningOperandsV1::None, h64(ARTIFACT_ROOT), None, LADDER),
        Err(PalwD2RouteErrorV1::NoChallengerOperands)
    );
    let forged = vec![PalwArtifactOpeningV1 {
        operand: PalwArtifactOperandV1 { tensor_name: "blk.1.attn_softmax".into(), layer: Some(1), row_start: 0, bytes: vec![1] },
        leaf_index: 0,
        leaf_count: 2,
        path: vec![h64(3)],
    }];
    assert!(matches!(
        palw_d2_named_leaf_operands_admit_v1(
            &fair_claim,
            fair_leaf,
            &PalwD2OpeningOperandsV1::Attn { binding: &fair.binding, openings: &forged },
            h64(ARTIFACT_ROOT),
            None,
            LADDER,
        ),
        Err(PalwD2RouteErrorV1::Operands(_))
    ));

    // (d) The court never takes model bytes from the accused: a claim part carrying rows is refused on this route.
    let mut with_rows = claim_part(&lying, leaf, false);
    with_rows.artifact_openings = forged.clone();
    assert_eq!(
        palw_d2_one_move_on_claim_part_v1(&with_rows, &[], &one_move_claim(&lying), LADDER),
        Err(PalwD2RouteErrorV1::ClaimPartCarriesModelBytes(1))
    );

    // (e) An honest claim is not convicted: the accuser's one move on the honest cache-write leaf does not convict.
    let fair_part = claim_part(&honest, leaf, false);
    assert_ne!(
        palw_d2_one_move_on_claim_part_v1(&fair_part, &[], &one_move_claim(&honest), LADDER),
        Ok(PalwShardCourtVerdictV1::ExecutorGuilty)
    );
}

/// The IR / pipeline openings: what is carried authenticates (or the finalize authenticates it — R-D2-COVER), and a pipeline
/// accusation names a pipeline class.
#[test]
fn a_cone_opening_publishes_authenticated_params_or_states_it_reads_none() {
    let (fx, leaf) = fused_fixture(None);
    let c = claim_record(&fx, FENCE);
    let root = h64(ARTIFACT_ROOT);
    let none = crate::palw_tir_court_v1::PalwTirParamOpeningV1::None;
    assert_eq!(palw_d2_named_leaf_operands_admit_v1(&c, leaf, &PalwD2OpeningOperandsV1::TirParams(&none), root, None, LADDER), Ok(()));
    let bogus = crate::palw_tir_court_v1::PalwTirParamOpeningV1::Single(crate::palw_artifact::PalwArtifactMultiproofV1 {
        leaf_count: 4,
        opened: vec![(1, PalwArtifactOperandV1 { tensor_name: "w".into(), layer: None, row_start: 0, bytes: vec![7; 8] })],
        siblings: vec![h64(9)],
    });
    assert!(matches!(
        palw_d2_named_leaf_operands_admit_v1(&c, leaf, &PalwD2OpeningOperandsV1::TirParams(&bogus), root, None, LADDER),
        Err(PalwD2RouteErrorV1::Operands(_))
    ));
    assert!(matches!(
        palw_d2_named_leaf_operands_admit_v1(&c, leaf, &PalwD2OpeningOperandsV1::GenParams(&[]), root, None, LADDER),
        Err(PalwD2RouteErrorV1::Relay(_))
    ));
    assert_eq!(
        palw_d2_named_leaf_operands_admit_v1(&c, leaf, &PalwD2OpeningOperandsV1::GenParams(&[]), root, Some(root), LADDER),
        Ok(())
    );
    assert_eq!(palw_d2_named_leaf_operands_admit_v1(&c, leaf, &PalwD2OpeningOperandsV1::CarriedUnchecked, root, None, LADDER), Ok(()));
}

// ---- the relay keeps the accused's signature -----------------------------------------------------------------------------------

/// **The accused signs coordinates and claim values only**, so a root-claim object filled with the challenger's published operands
/// carries the accused's signature unchanged: the attention message is `(session, root)` — the head, lanes, history and (m*, S*, V*)
/// — and the IR message `(version, elements, totals)`, neither touching an operand.
#[test]
fn the_root_claim_signatures_cover_coordinates_only_so_a_relay_keeps_them() {
    let court = include_str!("../palw_court_v2.rs");
    assert!(court.contains(
        "pub fn palw_attn_root_claim_message_v1(session_id: &Hash64, root: &crate::palw_attn_dissect::PalwAttnRootClaimV1) -> Vec<u8> {"
    ));
    assert!(court.contains("let message = palw_attn_root_claim_message_v1(session_id, root);"));
    let tir = include_str!("../palw_tir_dissect_v1.rs");
    assert!(tir.contains("m.extend(borsh::to_vec(&(root.version, &root.elements, &root.totals)).expect(\"borsh\"));"));

    // The IR relay, end to end on a skeleton root claim: the params change, the signed message does not.
    use borsh::BorshDeserialize;
    let zeros = [0u8; 16384];
    let signed = <crate::palw_tir_dissect_v1::PalwTirRootClaimV1 as BorshDeserialize>::deserialize(&mut &zeros[..])
        .expect("a skeleton root claim decodes");
    let published = crate::palw_tir_court_v1::PalwTirParamOpeningV1::Single(crate::palw_artifact::PalwArtifactMultiproofV1 {
        leaf_count: 1,
        opened: vec![(0, PalwArtifactOperandV1 { tensor_name: "w".into(), layer: None, row_start: 0, bytes: vec![1, 2] })],
        siblings: vec![],
    });
    let relayed = palw_d2_tir_root_claim_relayed_v1(&signed, &published);
    assert_eq!(relayed.finalize.params, published);
    let session = h64(0x5E55);
    assert_eq!(
        crate::palw_tir_dissect_v1::palw_tir_root_claim_message_v1(&session, &signed),
        crate::palw_tir_dissect_v1::palw_tir_root_claim_message_v1(&session, &relayed),
        "the accused's signature covers the relayed object"
    );
}

// ---- readiness by digest -------------------------------------------------------------------------------------------------------

/// **SeatReadinessLeaves' route**: a seat publishes a keyed digest per drawn leaf (no bytes); a verifier holding the registered model
/// refutes a wrong digest with its own authenticated opening, and an honest digest holds. Bytes that are not the registered leaf's,
/// or a leaf the draw did not name, refute nothing.
#[test]
fn a_readiness_digest_is_refuted_only_by_the_registered_bytes() {
    use crate::palw_artifact::{artifact_leaf_v1, artifact_root_v1, open_artifact_leaves_v1};
    let leaves: Vec<PalwArtifactOperandV1> = (0..3u8)
        .map(|i| PalwArtifactOperandV1 { tensor_name: format!("t{i}"), layer: None, row_start: 0, bytes: vec![i; 8] })
        .collect();
    let root = artifact_root_v1(&leaves.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    let roots = RegisteredModelRootsV1 { artifact_root: Some(root), artifact_leaf_count: Some(3), ..Default::default() };
    let opened = open_artifact_leaves_v1(&leaves, &[2]).unwrap();
    let (beacon, class, seat, span, drawn) = (h64(0xBEAC), h64(0xC1A5), bond(7), 3u64, [0u32, 2]);
    let input =
        |leaf: PalwArtifactOperandV1| PalwD2ReadinessDigestInputV1 { beacon, class_id: class, seat, span, leaf_index: 2, leaf };
    let honest = palw_d2_readiness_digest_v1(&input(leaves[2].clone()));
    let refute = |digest: Hash64, opening: &PalwArtifactOpeningV1| {
        palw_d2_readiness_refute_v1(&roots, beacon, class, seat, span, &drawn, digest, opening)
    };
    assert_eq!(refute(honest, &opened[0]), Ok(PalwD2ReadinessVerdictV1::DigestHolds));
    let mut wrong = leaves[2].clone();
    wrong.bytes[0] ^= 1;
    assert_eq!(refute(palw_d2_readiness_digest_v1(&input(wrong)), &opened[0]), Ok(PalwD2ReadinessVerdictV1::SeatRefuted));
    // The digest binds the draw: another beacon's digest of the same bytes is refuted.
    let stale = palw_d2_readiness_digest_v1(&PalwD2ReadinessDigestInputV1 { beacon: h64(1), ..input(leaves[2].clone()) });
    assert_eq!(refute(stale, &opened[0]), Ok(PalwD2ReadinessVerdictV1::SeatRefuted));
    // A forged opening refutes nothing; an undrawn leaf is not the seat's to answer.
    let mut forged = opened[0].clone();
    forged.operand.bytes[0] ^= 1;
    assert!(refute(honest, &forged).is_err());
    let undrawn = open_artifact_leaves_v1(&leaves, &[1]).unwrap();
    assert!(refute(honest, &undrawn[0]).is_err());
}
