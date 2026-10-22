// RFC-0032 A / ADR-0199 material intake (lane material-intake, 2026-10-22).
// Verified-half original/world fixture with an installed shared context; the
// storage schedule is the test-only injection (production is typed-missing).

fn material_intake_world(fx: &PalwTirPublicFrameFixtureV1)
    -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, Hash64, Hash64) {
    use crate::palw_court_shared_tir_state_v1 as shared;
    let frame = &fx.frame;
    let p = frame.plan_v1();
    let claim = p.original_claim;
    let (state, params, extras) = world(fx, frame);
    let params = params.with_tir_from_daa(Some(0));
    let state = bind_clock_service(&state, frame, &params, &extras);
    let profile = shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
    let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
    let scope = set.scope_id().unwrap();
    let context = shared::SharedTirContextV1 { version: 1, claim, allocation_id: p.allocation_id, source_id: p.source_id,
        rules_id: p.rules_id, profile_id: profile, scope_id: scope, queries: set.len(), original_accepted_daa: p.terms.accepted_daa,
        hard_daa: p.terms.accepted_daa + 10_000, material_until_daa: p.terms.material_until_daa };
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    b.write_staged_row_v1(Key::CourtSharedContext(claim), Some(Row::CourtSharedContext(Box::new(context)))).unwrap();
    let installed = b.state.clone();
    let mut extras = extras.clone();
    extras.staged_tir_route_active = true;
    (installed, params, extras, claim, scope)
}

fn material_test_schedule() -> crate::palw_court_shared_material_intake_v1::MaterialIntakeScheduleV1 {
    let s = crate::palw_court_shared_material_intake_v1::MaterialIntakeScheduleV1 {
        sompi_per_byte: 1,
        draft_ttl_daa: 100,
        cover: crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
            max_original_leaves: 1_000_000, max_total_edges: 10_000_000, max_total_coordinates: 1_000_000, max_root_memberships: 1_000_000 },
    };
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(Some(s));
    s
}

fn material_signed(kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair, network: Hash64, claim: Hash64, scope: Hash64,
    target: crate::palw_court_shared_material_intake_v1::MaterialTargetV1, who: PalwBondKeyV2,
    action: crate::palw_court_shared_material_intake_v1::MaterialActionV1)
    -> crate::palw_court_shared_material_intake_v1::SignedMaterialIntakeV1 {
    use crate::palw_court_shared_material_intake_v1::*;
    let mut s = SignedMaterialIntakeV1 { version: 1, claim, scope_id: scope, target, submitter: who, action, signature: vec![] };
    s.signature = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, s.signing_message(network).unwrap().as_byte_slice(),
        MATERIAL_INTAKE_CONTEXT_V1, [77; 32]).unwrap().as_ref().to_vec();
    s
}

/// Begin + every part, each in its own block (`start`, `start+1`, ...); optional Finish.
#[allow(clippy::too_many_arguments)]
fn material_upload(b: &mut TransitionBuilder<'_>, frame: &Frame, start: u64, kp: &libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair,
    network: Hash64, claim: Hash64, scope: Hash64, target: crate::palw_court_shared_material_intake_v1::MaterialTargetV1,
    who: PalwBondKeyV2, body: &[u8], parts: Option<usize>, finish: bool) -> (Hash64, u64) {
    use crate::palw_court_shared_material_intake_v1::*;
    use super::super::palw_court_shared_material_intake_fold_v1::{material_intake, MaterialIntakeEffectV1 as E};
    const PART: usize = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1;
    let hash = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(body);
    let mut daa = start;
    let begin = MaterialActionV1::Begin { total_bytes: body.len() as u64, body_hash: hash };
    assert_eq!(material_intake(b, &at(frame, daa), &material_signed(kp, network, claim, scope, target, who, begin)).unwrap(), E::Begun);
    let draft = material_draft_id_v1(claim, target, who, body.len() as u64, hash).unwrap();
    for (i, part) in body.chunks(PART).enumerate().take(parts.unwrap_or(usize::MAX)) {
        daa += 1;
        let action = MaterialActionV1::Part { draft_id: draft, offset: (i * PART) as u64, bytes: part.to_vec() };
        assert_eq!(material_intake(b, &at(frame, daa), &material_signed(kp, network, claim, scope, target, who, action)).unwrap(), E::Part);
    }
    if finish {
        daa += 1;
        let action = MaterialActionV1::Finish { draft_id: draft };
        assert_eq!(material_intake(b, &at(frame, daa), &material_signed(kp, network, claim, scope, target, who, action)).unwrap(),
            E::Completed);
    }
    (draft, daa)
}

#[test]
fn material_intake_partial_bodies_coexist_first_complete_wins_and_reorg_reverts() {
    use crate::palw_court_shared_material_intake_v1::*;
    use super::super::palw_court_shared_material_intake_fold_v1::{material_intake, MaterialIntakeEffectV1 as E};
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
    let frame = &fx.frame;
    let (mut state, params, extras, claim, scope) = material_intake_world(&fx);
    let producer = frame.custody_v1().pledge_v1().producer;
    let (pk, ok) = (libcrux_ml_dsa::ml_dsa_87::generate_key_pair([71; 32]), libcrux_ml_dsa::ml_dsa_87::generate_key_pair([72; 32]));
    // Fixture key setup, not a native rotation.
    state.bonds.get_mut(&producer).unwrap().pubkey = pk.verification_key.as_ref().to_vec();
    state.bonds.get_mut(&outsider()).unwrap().pubkey = ok.verification_key.as_ref().to_vec();
    let schedule = material_test_schedule();
    let net = extras.tir_public_network_domain_v1.unwrap();
    let leaves = required_parameter_leaves_v1(&state, claim, schedule.cover).unwrap();
    let leaf = *leaves.first().expect("history original reads parameters");
    let body = borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap();
    let target = MaterialTargetV1::Parameter { leaf };
    let before = state.state_root();
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);

    // A colluding first submitter opens a draft and stalls after one part (if any).
    let stalled_parts = if body.len() > crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 { Some(1) } else { Some(0) };
    let (stalled, _) = material_upload(&mut b, frame, 1, &pk, net, claim, scope, target, producer, &body, stalled_parts, false);
    // An independent party's partial body coexists with it (distinct key, no slot).
    let (honest, last) = material_upload(&mut b, frame, 3, &ok, net, claim, scope, target, outsider(), &body, None, false);
    assert_ne!(stalled, honest);
    assert!(matches!(b.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, stalled)), Some(Row::CourtMaterialDraft(_))));
    assert!(matches!(b.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, honest)), Some(Row::CourtMaterialDraft(_))));
    let pre_finish_root = b.state.state_root();
    let pre_finish_entries = b.entries.len();
    // The independent submitter completes first: its body becomes canonical.
    let finish = material_signed(&ok, net, claim, scope, target, outsider(), MaterialActionV1::Finish { draft_id: honest });
    assert_eq!(material_intake(&mut b, &at(frame, last + 1), &finish).unwrap(), E::Completed);
    let target_id = target.id_v1(claim).unwrap();
    let Some(Row::CourtMaterial(row)) = b.state.staged_court_row_v1(&Key::CourtMaterial(claim, target_id)).cloned() else { panic!() };
    assert_eq!(row.completed_by, outsider());
    assert_eq!(row.bytes, body);
    assert!(b.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, honest)).is_none(), "winner's draft is released");
    let Some(Row::CourtMaterialLedger(l)) = b.state.staged_court_row_v1(&Key::CourtMaterialLedger(claim, outsider())) else { panic!() };
    assert_eq!(l.reserved_bytes, 0);
    assert_eq!(l.charged_sompi, body.len() as u64 * schedule.sompi_per_byte);
    // The stalled draft gets no further storage and is released at its Finish.
    if body.len() > crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 {
        let part = MaterialActionV1::Part { draft_id: stalled,
            offset: crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1 as u64,
            bytes: body[crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1..].chunks(
                crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).next().unwrap().to_vec() };
        let r = b.state.state_root();
        assert!(material_intake(&mut b, &at(frame, last + 2), &material_signed(&pk, net, claim, scope, target, producer, part)).is_err());
        assert_eq!(b.state.state_root(), r);
    }
    let late = material_signed(&pk, net, claim, scope, target, producer, MaterialActionV1::Finish { draft_id: stalled });
    assert_eq!(material_intake(&mut b, &at(frame, last + 2), &late).unwrap(), E::AlreadyComplete);
    assert!(b.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, stalled)).is_none());
    let Some(Row::CourtMaterialLedger(pl)) = b.state.staged_court_row_v1(&Key::CourtMaterialLedger(claim, producer)) else { panic!() };
    assert_eq!(pl.reserved_bytes, 0);
    // A new Begin for a complete target changes nothing.
    let r = b.state.state_root();
    let again = material_signed(&pk, net, claim, scope, target, producer,
        MaterialActionV1::Begin { total_bytes: body.len() as u64, body_hash: Hash64::from_bytes([5; 64]) });
    assert_eq!(material_intake(&mut b, &at(frame, last + 3), &again).unwrap(), E::AlreadyComplete);
    assert_eq!(b.state.state_root(), r);
    // Canonical material is immutable and cannot be retyped through the writer.
    let mut changed = (*row).clone();
    changed.completed_by = producer;
    assert!(b.write_staged_row_v1(Key::CourtMaterial(claim, target_id), Some(Row::CourtMaterial(Box::new(changed).into()))).is_err());

    // Reorg: reverting the block's deltas restores the pre-intake root exactly;
    // re-applying restores the canonical material.
    let canonical = b.state.state_root();
    let mut undone = b.state.clone();
    for entry in b.entries.iter().rev() { apply_delta_entry(&mut undone, entry, true).unwrap(); }
    assert_eq!(undone.state_root(), before);
    assert!(undone.staged_court_row_v1(&Key::CourtMaterial(claim, target_id)).is_none());
    for entry in &b.entries[..pre_finish_entries] { apply_delta_entry(&mut undone, entry, false).unwrap(); }
    assert_eq!(undone.state_root(), pre_finish_root, "both partial drafts coexist again before the winning Finish");
    for entry in &b.entries[pre_finish_entries..] { apply_delta_entry(&mut undone, entry, false).unwrap(); }
    assert_eq!(undone.state_root(), canonical);

    // Restart: the carriage round-trips and the raw canonical body is re-authenticated.
    let mut accepted = b.state.clone();
    accepted.set_last_point_for_tests(Some(at(frame, last + 3)));
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&accepted)).unwrap();
    let fresh = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&params, Some(accepted.state_root())).unwrap();
    assert_eq!(fresh.state_root(), accepted.state_root());
    let Some(Row::CourtMaterial(loaded)) = fresh.staged_court_row_v1(&Key::CourtMaterial(claim, target_id)) else { panic!() };
    loaded.check_context(&fresh).unwrap();
    let mut corrupt = fresh.clone();
    let Some(Row::CourtMaterial(m)) = corrupt.staged_courts.get_mut(&Key::CourtMaterial(claim, target_id)) else { panic!() };
    let last_byte = m.bytes.len() - 1;
    m.bytes[last_byte] ^= 1;
    m.digest = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&m.bytes);
    let Some(Row::CourtMaterial(m)) = corrupt.staged_court_row_v1(&Key::CourtMaterial(claim, target_id)) else { panic!() };
    assert!(m.check_context(&corrupt).is_err(), "persisted raw material is re-authenticated, not a sealed flag");
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(None);
}

#[test]
fn material_intake_rooted_store_builds_identical_evidence_to_borrowed_path() {
    use crate::palw_court_shared_material_intake_v1::*;
    use crate::palw_tir_court_v1::{PalwTirEvidenceStoreV1, PalwTirCourtRulesV1, build_tir_cone_refutation_v1};
    use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as MaterialStore, PromptMaterialV1};
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
    let frame = &fx.frame;
    let (mut state, params, extras, claim, scope) = material_intake_world(&fx);
    let who = outsider();
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([73; 32]);
    state.bonds.get_mut(&who).unwrap().pubkey = kp.verification_key.as_ref().to_vec();
    let schedule = material_test_schedule();
    let net = extras.tir_public_network_domain_v1.unwrap();
    let binding = &fx.execution.binding;
    let court = PalwTirCourtRulesV1 { max_step_leaf_count: fx.rules.maximum_leaves, prompt_form: fx.rules.prompt_form,
        limits: crate::palw_court_v2::palw_tir_court_limits_v1(&fx.rules.court) };
    let (early_leaf, early) = (0..binding.step_leaf_count).find_map(|i|
        build_tir_cone_refutation_v1(binding, i, &fx.execution, &court).ok()
            .filter(|p| !p.prompt_token_ids.is_empty()).map(|p| (i, p))).expect("fixture has a prompt-reading cone");
    let opened: Vec<u32> = early.params.single().map(|p| p.opened.iter().map(|(i, _)| *i).collect()).unwrap_or_default();
    let leaves = required_parameter_leaves_v1(&state, claim, schedule.cover).unwrap();
    assert!(opened.iter().all(|l| leaves.binary_search(l).is_ok()), "the derived union covers the actual read set");

    // Borrowed reference path (caller-held, authenticated material).
    let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
    let units: Vec<_> = (0..binding.step_leaf_count).map(|index| source.authenticate_v1(&Q::StepLeaf { index },
        &R::StepLeaf { preimage: fx.execution.step_leaf(index).unwrap(), opening: fx.execution.step_opening(index).unwrap() }, None).unwrap())
        .collect();
    let borrowed_params: Vec<_> = opened.iter().map(|i| fx.execution.param_opening(*i).unwrap()).collect();
    let borrowed = MaterialStore::new_with_prompt(&state, claim, scope, &units, &borrowed_params, None,
        PromptMaterialV1::Flat(&fx.execution.prompt)).unwrap();
    let reference = build_tir_cone_refutation_v1(binding, early_leaf, &borrowed, &court).unwrap();
    assert_eq!(borsh::to_vec(&reference).unwrap(), borsh::to_vec(&early).unwrap());

    // Rooted path: the same material submitted through intake, then read back.
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let empty = RootedMaterialV1::load_v1(&b.state, claim).unwrap();
    let missing = empty.store_v1(&b.state, claim, scope).unwrap();
    assert!(build_tir_cone_refutation_v1(binding, early_leaf, &missing, &court).is_err(), "absent material is never a fallback");
    let profile = crate::palw_court_shared_tir_state_v1::context(&b.state, claim).unwrap().profile_id;
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
    let mut daa = 1;
    for index in 0..binding.step_leaf_count {
        let ordinal = (0..set.len()).find(|&o| set.request_at(o).unwrap() == Q::StepLeaf { index }).unwrap();
        let body = borsh::to_vec(&R::StepLeaf { preimage: fx.execution.step_leaf(index).unwrap(),
            opening: fx.execution.step_opening(index).unwrap() }).unwrap();
        daa = material_upload(&mut b, frame, daa, &kp, net, claim, scope, MaterialTargetV1::Unit { ordinal }, who, &body, None, true).1 + 1;
    }
    for leaf in &opened {
        let body = borsh::to_vec(&fx.execution.param_opening(*leaf).unwrap()).unwrap();
        daa = material_upload(&mut b, frame, daa, &kp, net, claim, scope, MaterialTargetV1::Parameter { leaf: *leaf }, who, &body, None, true).1 + 1;
    }
    match fx.rules.prompt_form {
        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat => {
            let body = borsh::to_vec(&fx.execution.prompt).unwrap();
            material_upload(&mut b, frame, daa, &kp, net, claim, scope, MaterialTargetV1::PromptFlat, who, &body, None, true);
        }
        crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1 => panic!("history fixture is Flat"),
    }
    let rooted = RootedMaterialV1::load_v1(&b.state, claim).unwrap();
    assert_eq!(rooted.units_len_v1() as u64, binding.step_leaf_count);
    assert_eq!(rooted.params_len_v1(), opened.len());
    let store = rooted.store_v1(&b.state, claim, scope).unwrap();
    let replay = build_tir_cone_refutation_v1(binding, early_leaf, &store, &court).unwrap();
    assert_eq!(borsh::to_vec(&replay).unwrap(), borsh::to_vec(&reference).unwrap(), "rooted store builds the borrowed path's bytes");
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(None);
}

#[test]
fn material_intake_refuses_corrupt_parts_overbudget_and_dormant_fence() {
    use crate::palw_court_shared_material_intake_v1::*;
    use super::super::palw_court_shared_material_intake_fold_v1::{material_intake, MaterialIntakeEffectV1 as E};
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
    let frame = &fx.frame;
    let (mut state, params, extras, claim, scope) = material_intake_world(&fx);
    let who = outsider();
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([74; 32]);
    state.bonds.get_mut(&who).unwrap().pubkey = kp.verification_key.as_ref().to_vec();
    let net = extras.tir_public_network_domain_v1.unwrap();
    let schedule = material_test_schedule();
    let leaves = required_parameter_leaves_v1(&state, claim, schedule.cover).unwrap();
    let leaf = leaves[0];
    let body = borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap();
    let target = MaterialTargetV1::Parameter { leaf };
    let sign = |t, a| material_signed(&kp, net, claim, scope, t, who, a);
    let before = state.state_root();

    // Dormant: no fence, no schedule, no context -> refused, nothing written.
    let mut dormant_extras = extras.clone();
    dormant_extras.staged_tir_route_active = false;
    let mut d = TransitionBuilder::new(&state, &params, false, false, false, false, &dormant_extras);
    let begin = MaterialActionV1::Begin { total_bytes: body.len() as u64,
        body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) };
    assert!(material_intake(&mut d, &at(frame, 1), &sign(target, begin.clone())).is_err());
    let mut s = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    set_material_intake_test_schedule_v1(None);
    assert!(material_intake(&mut s, &at(frame, 1), &sign(target, begin.clone())).is_err(), "typed-missing schedule refuses");
    assert_eq!(s.state.state_root(), before);
    let schedule = material_test_schedule();

    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    // Forged signature and a non-eligible signer are refused.
    let mut forged = sign(target, begin.clone());
    forged.signature[0] ^= 1;
    assert!(material_intake(&mut b, &at(frame, 1), &forged).is_err());
    // Declared size above the target's native width is refused (never caller authority).
    let over = MaterialActionV1::Begin { total_bytes: body.len() as u64 + 1, body_hash: Hash64::from_bytes([1; 64]) };
    assert!(material_intake(&mut b, &at(frame, 1), &sign(target, over)).is_err());
    // A coordinate that no numerical root reads is refused.
    if let Some(outside) = (0u32..).find(|l| leaves.binary_search(l).is_err()).filter(|l| *l < 1 << 20) {
        let action = MaterialActionV1::Begin { total_bytes: 1, body_hash: Hash64::from_bytes([2; 64]) };
        assert!(material_intake(&mut b, &at(frame, 1), &sign(MaterialTargetV1::Parameter { leaf: outside }, action)).is_err());
    }
    assert_eq!(b.state.state_root(), before);

    // Corrupt part under the honest checksum: Finish refused, state rolled back.
    let mut corrupt = body.clone();
    corrupt[0] ^= 1;
    let honest_hash = crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
    assert_eq!(material_intake(&mut b, &at(frame, 2), &sign(target,
        MaterialActionV1::Begin { total_bytes: body.len() as u64, body_hash: honest_hash })).unwrap(), E::Begun);
    let draft = material_draft_id_v1(claim, target, who, body.len() as u64, honest_hash).unwrap();
    const PART: usize = crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1;
    for (i, part) in corrupt.chunks(PART).enumerate() {
        assert_eq!(material_intake(&mut b, &at(frame, 3), &sign(target,
            MaterialActionV1::Part { draft_id: draft, offset: (i * PART) as u64, bytes: part.to_vec() })).unwrap(), E::Part);
    }
    // Non-canonical offset / retry with other bytes are refused.
    assert!(material_intake(&mut b, &at(frame, 3), &sign(target,
        MaterialActionV1::Part { draft_id: draft, offset: 0, bytes: body[..body.len().min(PART)].to_vec() })).is_err());
    let r = b.state.state_root();
    assert!(material_intake(&mut b, &at(frame, 4), &sign(target, MaterialActionV1::Finish { draft_id: draft })).is_err());
    assert_eq!(b.state.state_root(), r);
    // Self-consistent checksum over corrupt bytes: authentication refuses it.
    let (bad_draft, last) = material_upload(&mut b, frame, 5, &kp, net, claim, scope, target, who, &corrupt, None, false);
    let r = b.state.state_root();
    assert!(material_intake(&mut b, &at(frame, last + 1), &sign(target, MaterialActionV1::Finish { draft_id: bad_draft })).is_err());
    assert_eq!(b.state.state_root(), r);
    assert!(!target_complete_v1(&b.state, claim, target).unwrap());

    // Per-claim budget: one submitter declaring every unit at its native bound
    // repeatedly is refused once the derived requirement is reserved.
    let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
    let profile = crate::palw_court_shared_tir_state_v1::context(&b.state, claim).unwrap().profile_id;
    let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
    let mut refusal = None;
    'passes: for pass in 0..3u8 {
        for ordinal in 0..set.len() {
            let q = set.request_at(ordinal).unwrap();
            if matches!(q, crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids { .. }) { continue; }
            let Ok((bound, _)) = crate::palw_tir_public_owned_v1::request_body_bound_v1(&source, q) else { continue };
            let action = MaterialActionV1::Begin { total_bytes: bound, body_hash: Hash64::from_bytes([100 + pass; 64]) };
            if let Err(e) = material_intake(&mut b, &at(frame, 50), &sign(MaterialTargetV1::Unit { ordinal }, action)) {
                refusal = Some(format!("{e:?}"));
                break 'passes;
            }
        }
    }
    assert!(refusal.expect("budget must bind").contains("per-claim native budget"));
    let _ = schedule;
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(None);
}

#[test]
fn material_intake_incomplete_drafts_expire_and_release_reservation() {
    use crate::palw_court_shared_material_intake_v1::*;
    use super::super::palw_court_shared_material_intake_fold_v1::{material_intake, expire_material_drafts};
    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
    let fx = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
    let frame = &fx.frame;
    let (mut state, params, extras, claim, scope) = material_intake_world(&fx);
    let who = outsider();
    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([75; 32]);
    state.bonds.get_mut(&who).unwrap().pubkey = kp.verification_key.as_ref().to_vec();
    let net = extras.tir_public_network_domain_v1.unwrap();
    let schedule = material_test_schedule();
    let leaf = required_parameter_leaves_v1(&state, claim, schedule.cover).unwrap()[0];
    let body = borsh::to_vec(&fx.execution.param_opening(leaf).unwrap()).unwrap();
    let target = MaterialTargetV1::Parameter { leaf };
    let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
    let (draft, last) = material_upload(&mut b, frame, 1, &kp, net, claim, scope, target, who, &body, Some(1), false);
    let Some(Row::CourtMaterialDraft(d)) = b.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, draft)).cloned() else { panic!() };
    assert_eq!(d.expires_daa, (at(frame, 1).daa_score + schedule.draft_ttl_daa).min(d.material_until_daa));
    let Some(Row::CourtMaterialLedger(l)) = b.state.staged_court_row_v1(&Key::CourtMaterialLedger(claim, who)) else { panic!() };
    assert_eq!(l.reserved_bytes, body.len() as u64);
    // Before expiry nothing is released; past it, a further part is refused.
    assert_eq!(expire_material_drafts(&mut b, &at(frame, last), claim).unwrap(), 0);
    let expiry_offset = schedule.draft_ttl_daa + 1;
    let r = b.state.state_root();
    let finish = material_signed(&kp, net, claim, scope, target, who, MaterialActionV1::Finish { draft_id: draft });
    assert!(material_intake(&mut b, &at(frame, expiry_offset), &finish).is_err());
    assert_eq!(b.state.state_root(), r);
    // The material-horizon sweep releases the expired draft, its parts and its reservation.
    let mut swept = TransitionBuilder::new(&b.state, &params, false, false, false, false, &extras);
    super::super::palw_staged_court_fold_v1::sweep(&mut swept, &at(frame, expiry_offset)).unwrap();
    assert!(swept.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, draft)).is_none());
    assert!(swept.state.staged_court_row_v1(&Key::CourtMaterialPart(draft, 0)).is_none());
    let Some(Row::CourtMaterialLedger(l)) = swept.state.staged_court_row_v1(&Key::CourtMaterialLedger(claim, who)) else { panic!() };
    assert_eq!(l.reserved_bytes, 0);
    // The explicit per-claim expiry releases the same draft/parts/reservation.
    assert_eq!(expire_material_drafts(&mut b, &at(frame, expiry_offset), claim).unwrap(), 1);
    assert!(b.state.staged_court_row_v1(&Key::CourtMaterialDraft(claim, draft)).is_none());
    assert!(b.state.staged_court_row_v1(&Key::CourtMaterialPart(draft, 0)).is_none());
    let Some(Row::CourtMaterialLedger(l)) = b.state.staged_court_row_v1(&Key::CourtMaterialLedger(claim, who)) else { panic!() };
    assert_eq!(l.reserved_bytes, 0);
    crate::palw_court_shared_material_intake_v1::set_material_intake_test_schedule_v1(None);
}
