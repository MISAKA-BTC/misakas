//! Signed courts over authenticated material delivered through the actual node.
use super::*;
use kaspa_consensus_core::palw_court_close_receipt_v1::NativeCourtRulesV1;
use kaspa_consensus_core::palw_court_numerical_transport_v1::{self as transport, NumericalControlRefV1 as Numerical};
use kaspa_consensus_core::palw_court_rooted_numerical_material_v1::with_rooted_numerical_material_v1;
use kaspa_consensus_core::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1 as Shared;
use kaspa_consensus_core::palw_court_shared_tir_bottom_v1 as bottoms;
use kaspa_consensus_core::palw_court_shared_tir_root_v1 as roots;
use kaspa_consensus_core::palw_court_shared_tir_round_v1 as rounds;
use kaspa_consensus_core::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use kaspa_consensus_core::palw_tir_court_v1 as court;
use kaspa_consensus_core::palw_tir_public_carriage_v1::*;

fn native_rules(net: &Net) -> NativeCourtRulesV1 {
    let state = net.chain.tip_state().1;
    net.chain
        .vp()
        .palw_test_resolved_transition_extras_v1(state.last_point().unwrap())
        .native_court_rules_v1
        .expect("the actual node resolves its native court rules")
}
fn court_rules(native: &NativeCourtRulesV1) -> court::PalwTirCourtRulesV1 {
    court::PalwTirCourtRulesV1 {
        max_step_leaf_count: native.step_ladder,
        prompt_form: if native.prompt_ids_merkle {
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1
        } else {
            kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
        },
        limits: kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&native.court),
    }
}
fn native_binding(net: &Net, capture: &FamilyCaptureV2, claim: Hash64) -> PalwTirStepBindingV1 {
    let state = net.chain.tip_state().1;
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&state, claim).unwrap();
    let class_id = frame.original_v1().job.class_id;
    let record = state.tir_class_v1(&class_id).expect("actual registered program");
    let mut class = capture.binding.class.clone();
    class.program = record.program.as_ref().clone();
    class.layout = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&state, class_id)
        .expect("actual registered layout")
        .clone();
    // Resolve rechecks class/artifact identity and reads the job/roots from the
    // actual original. No captured execution row enters the evidence store.
    frame.header_v1().resolve_v1(&class, frame.original_v1().execution_root, native_rules(net).step_ladder).unwrap()
}
fn outside_filer(net: &Net, claim: Hash64) -> usize {
    let state = net.chain.tip_state().1;
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&state, claim).unwrap();
    (0..net.funding.len())
        .find(|card| {
            let who = net.bond(*card);
            who != frame.custody_v1().pledge_v1().producer
                && frame.original_custody_v1().credit_v1(&who).is_none()
                && frame.original_custody_v1().grant_v1(&who).is_none()
                && state.bond(&who).is_some_and(|bond| {
                    matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active)
                        && bond.collateral >= net.bundle.state.min_collateral_sompi()
                })
        })
        .expect("an actual funded non-liable public filer")
}
fn numerical_object(control: Numerical<'_>, scope: Hash64, native: &NativeCourtRulesV1) -> Obj {
    let manifest = transport::describe_numerical_v1(&control, scope, native).unwrap();
    let mut parts = Vec::new();
    transport::write_numerical_parts_v1(&control, &manifest, native, |part| {
        parts.push(borsh::from_slice(&borsh::to_vec(&part)?)?);
        Ok(())
    })
    .unwrap();
    assert!(parts.len() <= kaspa_consensus_core::palw_court_shared_op_carriage_v1::PALW_SHARED_COURT_MAX_PARTS_V1);
    Obj::SharedCourtOpV1 { op: Box::new(Shared::Numerical { manifest, parts }) }
}

fn assert_refused_root_carrier(net: &mut Net, card: usize, control: &roots::SignedSharedTirRootV1, native: &NativeCourtRulesV1) {
    let before = net.chain.tip_state().1.state_root();
    let funding = net.funding[card].clone();
    let object = numerical_object(Numerical::Root(control), control.scope_id, native);
    let carrier = net.carrier(card, &object);
    // The outer transaction is genuinely payer-signed; the inner control is
    // refused for its actual numerical signature/scope. No block is fabricated.
    assert!(net.mempool(&carrier).is_err(), "native mempool must reject invalid numerical authentication");
    net.funding[card] = funding;
    assert_eq!(net.chain.tip_state().1.state_root(), before);
}

/// Producer-signed root/rounds and an outside filer's actual native bottom.
/// No-mismatch is local evidence, never selected-verifier authentication.
pub(crate) async fn complete_honest_family_court_v2(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    admitted: &AdmittedFamilyOriginalV2,
) -> Hash64 {
    let claim = admitted.claim;
    let binding = native_binding(net, capture, claim);
    let native = native_rules(net);
    let rules = court_rules(&native);
    let state = net.chain.tip_state().1;
    let scope = kaspa_consensus_core::palw_court_shared_tir_state_v1::derive_shared_tir_context_v1(&state, claim).unwrap().scope_id;
    let producer = net.bond(capture.card);
    let filer_card = outside_filer(net, claim);
    let initial_claim = state.claim(&claim).unwrap().clone();
    let initial_slashed = state.bond(&producer).unwrap().slashed;
    assert!(
        kaspa_consensus_core::palw_security_certification_v1::palw_claim_certified_daa_v1(&state, &claim).is_none(),
        "the joint court precedes the separate selected-verifier approval step"
    );
    let (leaf, mut root) = with_rooted_numerical_material_v1(&state, claim, |store| {
        (0..binding.step_leaf_count).find_map(|leaf| {
            let root = court::build_tir_root_claim_v1(&binding, leaf, store, &rules).ok()?;
            court::check_tir_root_claim_v1(&root, leaf, &rules).ok()?;
            Some((leaf, root))
        })
    })
    .unwrap()
    .expect("actual program and rooted material must contain a numerical dissection site");
    kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
    let mut root = roots::SignedSharedTirRootV1 {
        version: 1,
        claim,
        scope_id: scope,
        policy_id: roots::shared_tir_root_policy_id_v1(&native).unwrap(),
        leaf,
        arity: native.court.dissection_arity(),
        signer: producer,
        root: Box::new(root),
        signature: Vec::new(),
    };
    root.signature = sign(capture.card, root.signing_message(net.domain).unwrap().as_byte_slice(), roots::SHARED_TIR_ROOT_CONTEXT_V1);
    let mut unsigned = root.clone();
    unsigned.signature[0] ^= 1;
    assert_refused_root_carrier(net, capture.card, &unsigned, &native);
    let mut foreign_scope = root.clone();
    foreign_scope.scope_id = Hash64::from_u64_word(0xfa4200);
    assert_ne!(foreign_scope.scope_id, scope);
    foreign_scope.signature =
        sign(capture.card, foreign_scope.signing_message(net.domain).unwrap().as_byte_slice(), roots::SHARED_TIR_ROOT_CONTEXT_V1);
    assert_refused_root_carrier(net, capture.card, &foreign_scope, &native);
    net.send(vec![(capture.card, numerical_object(Numerical::Root(&root), scope, &native))]).await;
    assert!(matches!(net.chain.tip_state().1.staged_court_row_v1(&Key::CourtSharedRoot(claim, leaf)), Some(Row::CourtSharedRoot(_))));
    let mut path = Vec::new();
    for _ in 0..rounds::SHARED_TIR_MAX_PATH_V1 + 2 {
        let state = net.chain.tip_state().1;
        let native = native_rules(net);
        let rules = court_rules(&native);
        let prepared = rounds::prepare_shared_tir_round_v1(&state, claim, leaf, &path).unwrap();
        if prepared.phase.terminal_range().is_some() {
            let mut evidence = with_rooted_numerical_material_v1(&state, claim, |store| {
                court::build_tir_dissect_bottom_v1(&binding, &prepared.phase, store, &rules)
            })
            .unwrap()
            .unwrap();
            kaspa_consensus_core::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut evidence.binding);
            let mut bottom = bottoms::SignedSharedTirBottomV1 {
                version: 1,
                claim,
                leaf,
                root_id: prepared.root_id,
                path: path.clone(),
                parent_id: prepared.parent_id,
                filer: net.bond(filer_card),
                bottom: Box::new(evidence),
                signature: Vec::new(),
            };
            bottom.signature =
                sign(filer_card, bottom.signing_message(net.domain).unwrap().as_byte_slice(), bottoms::SHARED_TIR_BOTTOM_CONTEXT_V1);
            let id = bottom.id().unwrap();
            net.send(vec![(filer_card, numerical_object(Numerical::Bottom(&bottom), scope, &native))]).await;
            let state = net.chain.tip_state().1;
            let Some(Row::CourtSharedBottom(row)) = state.staged_court_row_v1(&Key::CourtSharedBottom(id)) else {
                panic!("actual signed bottom carrier must persist native adjudication");
            };
            let native = native_rules(net);
            let checked = bottoms::check_shared_tir_bottom_v1(&state, &net.bundle.state, &native, &row.control).unwrap();
            assert_eq!(checked.outcome(), &bottoms::SharedTirBottomOutcomeV1::NoMismatchFound);
            let current = state.claim(&claim).expect("actual original remains retained through its court door");
            assert_eq!(
                (
                    current.class_id,
                    current.bond,
                    current.execution_root,
                    current.trace_root,
                    current.output_root,
                    current.job_identity
                ),
                (
                    initial_claim.class_id,
                    initial_claim.bond,
                    initial_claim.execution_root,
                    initial_claim.trace_root,
                    initial_claim.output_root,
                    initial_claim.job_identity
                ),
                "numerical court retains the exact original source and roots"
            );
            // Ordinary DAA-driven phase changes can occur between carriers.
            // A local no-mismatch has neither a fraud verdict nor approval authority.
            assert!(!matches!(
                current.phase,
                kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Voided {
                    reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtFraud
                        | kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtHeldVerdict
                        | kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::CourtDefault,
                    ..
                }
            ));
            assert!(kaspa_consensus_core::palw_security_certification_v1::palw_claim_certified_daa_v1(&state, &claim).is_none());
            assert_eq!(state.bond(&producer).unwrap().slashed, initial_slashed);
            assert_family_winning_carrier_v2(&state, admitted);
            return id;
        }
        let round = with_rooted_numerical_material_v1(&state, claim, |store| {
            court::build_tir_dissect_round_v1(&binding, &prepared.phase, binding.class.layout.h_tile, store, &rules)
        })
        .unwrap()
        .unwrap();
        let mut control = rounds::SignedSharedTirRoundV1 {
            version: 1,
            claim,
            leaf,
            root_id: prepared.root_id,
            path: path.clone(),
            parent_id: prepared.parent_id,
            signer: producer,
            round: Box::new(round),
            signature: Vec::new(),
        };
        control.signature =
            sign(capture.card, control.signing_message(net.domain).unwrap().as_byte_slice(), rounds::SHARED_TIR_ROUND_CONTEXT_V1);
        net.send(vec![(capture.card, numerical_object(Numerical::Round(&control), scope, &native))]).await;
        path.push(0);
    }
    panic!("native numerical geometry did not terminate within its existing bound");
}

/// Net::send's generic carrier pays only FEE. A paid witness opener must
/// additionally burn its exact terminal rent from a genuine ordinary input.
async fn send_paid_witness(net: &mut Net, card: usize, control: PalwTirPublicControlV1) {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let rent = control.declared_rent_sompi;
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 {
        version: PALW_LIFECYCLE_TX_VERSION_V2,
        object: Obj::TirPublicControlV1 { control: Box::new(control) },
    })
    .unwrap();
    let outpoint = net.funding[card].0;
    let entry = net.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).expect("actual filer funding UTXO");
    let fee = FEE.checked_add(rent).unwrap();
    let change = entry.amount.checked_sub(fee).expect("filer actually owns the complete rent and normal fee");
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, Vec::new(), 0, 1)],
        vec![TransactionOutput::new(change, card_payout_spk(card))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, card, net.config.params.storage_mass_parameter);
    tx.finalize();
    net.mempool(&tx).expect("actual signatures, fee, rent and exact witness scope pass mempool");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|accepted| accepted.id() == tx.id()));
    net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let next = TransactionOutpoint::new(tx.id(), 0);
    let actual = net.chain.ctx.consensus.get_virtual_utxo_entry(next).expect("actual accepted rent carrier change");
    assert_eq!(actual.amount, change);
    net.funding[card] = (next, actual);
}

/// Run the numerical carriers, then independently find and file an actual
/// original-domain contradiction from rooted operands. A numerical statement
/// and terminal status are never fabricated to force a conviction.
pub(crate) async fn adjudicate_forged_family_claim_v2(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    admitted: &AdmittedFamilyOriginalV2,
) -> Hash64 {
    complete_honest_family_court_v2(net, capture, admitted).await;
    let state = net.chain.tip_state().1;
    let binding = native_binding(net, capture, admitted.claim);
    let native = native_rules(net);
    let rules = court_rules(&native);
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&state, admitted.claim).unwrap();
    let maximum = frame.plan_v1().terminal.maximum_witness_bytes;
    let evidence = with_rooted_numerical_material_v1(&state, admitted.claim, |store| {
        (0..binding.step_leaf_count).find_map(|leaf| {
            let refutation = court::build_tir_cone_refutation_v1(&binding, leaf, store, &rules).ok()?;
            let mut proof = kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) };
            proof.tir_strip_program_v1();
            match kaspa_consensus_core::palw_tir_objective_v1::adjudicate_original_v1(
                &state,
                admitted.claim,
                &proof,
                &native.court,
                native.step_ladder,
                rules.prompt_form,
                maximum,
            )
            .ok()?
            {
                kaspa_consensus_core::palw_tir_objective_v1::PalwTirObjectiveOutcomeV1::Convicts(_) => {
                    Some(PalwTirPublicConvictionEvidenceV1::Arithmetic { proof: Box::new(proof) })
                }
                _ => None,
            }
        })
    })
    .unwrap()
    .expect("forged source must contain an independently adjudicable actual contradiction");
    let filer_card = outside_filer(net, admitted.claim);
    let filer = net.bond(filer_card);
    let producer = frame.custody_v1().pledge_v1().producer;
    let before_slash = state.bond(&producer).unwrap().slashed;
    let plan = frame.plan_v1();
    let (allocation_id, source_id, rent) = (plan.allocation_id, plan.source_id, frame.conviction_rent_v1());
    let extras = net.chain.vp().palw_test_resolved_transition_extras_v1(state.last_point().unwrap());
    kaspa_consensus_core::palw_state_v2::palw_tir_public_staged_conviction_preflight_v2(
        &state,
        &net.bundle.state,
        &extras,
        net.daa() + 2,
        filer,
        admitted.claim,
        &evidence,
    )
    .expect("actual native filing predicate accepts this exact contradiction and literal filer");
    let bytes = borsh::to_vec(&evidence).unwrap();
    assert!(bytes.len() as u64 <= maximum, "original terminal witness allowance is unchanged");
    let digest = palw_tir_public_witness_digest_v1(&bytes);
    for (i, bytes_part) in bytes.chunks(PALW_TIR_PUBLIC_WITNESS_PART_BYTES_V1).enumerate() {
        let mut control = PalwTirPublicControlV1 {
            version: 1,
            claim: admitted.claim,
            allocation_id,
            source_id,
            target: PalwTirPublicControlTargetV1::ConvictionWitness,
            expected_turn: 0,
            declared_rent_sompi: if i == 0 { rent } else { 0 },
            signer: filer,
            action: PalwTirPublicControlActionV1::WitnessPart(PalwTirPublicWitnessPartV1 {
                total_bytes: bytes.len() as u64,
                digest,
                offset: (i * PALW_TIR_PUBLIC_WITNESS_PART_BYTES_V1) as u64,
                bytes: bytes_part.to_vec(),
            }),
            signature: Vec::new(),
        };
        control.signature =
            sign(filer_card, control.signing_message_v1(net.domain).unwrap().as_byte_slice(), PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1);
        send_paid_witness(net, filer_card, control).await;
    }
    let after = net.chain.tip_state().1;
    assert!(
        matches!(after.claim(&admitted.claim).unwrap().phase, kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Voided { .. }),
        "only the actual last signed witness part and native adjudicator may void the original"
    );
    assert!(after.bond(&producer).unwrap().slashed > before_slash, "conviction must collect actual producer principal");
    assert_family_winning_carrier_v2(&after, admitted);
    let extras = net.chain.vp().palw_test_resolved_transition_extras_v1(after.last_point().unwrap());
    assert!(
        kaspa_consensus_core::palw_state_v2::palw_tir_public_staged_conviction_preflight_v2(
            &after,
            &net.bundle.state,
            &extras,
            net.daa() + 2,
            filer,
            admitted.claim,
            &evidence,
        )
        .is_err(),
        "the same adjudication cannot collect the producer principal twice"
    );
    digest
}

/// Existing tag-255 paid material intake for actual missing numerical inputs.
/// Capture is a public transport source only; every Finish authenticates against
/// the retained original. This neither changes nor completes any source duty.
pub(crate) async fn supply_missing_family_numerical_material_v2(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    admitted: &AdmittedFamilyOriginalV2,
) {
    use kaspa_consensus_core::palw_court_shared_material_intake_v1 as intake;
    use kaspa_consensus_core::palw_court_shared_material_intake_v1::{MaterialActionV1 as Action, MaterialTargetV1 as Target};
    use kaspa_consensus_core::palw_step_leg::*;
    use kaspa_consensus_core::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
    let claim = admitted.claim;
    let producer = net.bond(capture.card);
    let ctx = capture.binding.job_context.context_hash();
    let row_hashes: Vec<_> = capture
        .rows
        .iter()
        .enumerate()
        .map(|(row, lanes)| kaspa_consensus_core::palw_step_refute::tiled_logits_row_root_v1(&ctx, row as u32, lanes).unwrap())
        .collect();
    let rows_root = step_merkle_root_v1(&row_hashes).unwrap();
    let response = |query: Q| -> R {
        match query {
            Q::StepNode { level, index } => {
                let (frontier, siblings) =
                    kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&capture.hashes, level, index).unwrap();
                R::StepNode { frontier, siblings }
            }
            Q::StepLeaf { index } => R::StepLeaf {
                preimage: capture.preimages[index as usize].clone(),
                opening: step_opening_v1(&capture.hashes, index).unwrap(),
            },
            Q::StepRun { first, count } => R::StepRun {
                preimages: capture.preimages[first as usize..first as usize + count as usize].to_vec(),
                range: PalwStepRangeOpeningV1 {
                    first_leaf_index: first,
                    leaf_hashes: capture.hashes[first as usize..first as usize + count as usize].to_vec(),
                    siblings: step_merkle_range_siblings_v1(
                        &capture.hashes,
                        usize::try_from(first).unwrap(),
                        usize::try_from(count).unwrap(),
                    )
                    .unwrap(),
                },
            },
            Q::RowsNode { level, index } => {
                let (frontier, siblings) =
                    kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&row_hashes, level, index).unwrap();
                R::RowsNode { frontier, siblings }
            }
            Q::TileNode { row, level, index } => {
                let hashes: Vec<_> = capture.rows[row as usize]
                    .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                    .enumerate()
                    .map(|(tile, lanes)| {
                        kaspa_consensus_core::palw_step_refute::tiled_logits_tile_leaf_v1(&ctx, row, tile as u32, lanes)
                    })
                    .collect();
                let (frontier, siblings) =
                    kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&hashes, level, index).unwrap();
                R::TileNode {
                    row_root: row_hashes[row as usize],
                    row_opening: step_opening_v1(&row_hashes, u64::from(row)).unwrap(),
                    frontier,
                    siblings,
                }
            }
            Q::TraceTile { row, tile } => {
                let lanes = capture.rows[row as usize]
                    .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                    .nth(tile as usize)
                    .unwrap()
                    .to_vec();
                let hashes: Vec<_> = capture.rows[row as usize]
                    .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                    .enumerate()
                    .map(|(tile, lanes)| {
                        kaspa_consensus_core::palw_step_refute::tiled_logits_tile_leaf_v1(&ctx, row, tile as u32, lanes)
                    })
                    .collect();
                R::TraceTile {
                    row_root: row_hashes[row as usize],
                    row_opening: step_opening_v1(&row_hashes, u64::from(row)).unwrap(),
                    lanes,
                    opening: step_opening_v1(&hashes, u64::from(tile)).unwrap(),
                }
            }
            Q::Ids { offset } => R::Ids {
                rows_root,
                ids: capture.generated[offset as usize
                    ..capture.generated.len().min(
                        offset as usize + kaspa_consensus_core::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize,
                    )]
                    .to_vec(),
            },
        }
    };

    let (scope, missing) = {
        let state = net.chain.tip_state().1;
        let context = kaspa_consensus_core::palw_court_shared_tir_state_v1::derive_shared_tir_context_v1(&state, claim).unwrap();
        let Some(Row::CourtSharedContext(installed)) = state.staged_court_row_v1(&Key::CourtSharedContext(claim)) else {
            panic!("paid intake requires the actual installed native context")
        };
        assert_eq!(installed.as_ref(), &context);
        let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&state, claim).unwrap();
        assert_eq!(frame.custody_v1().pledge_v1().producer, producer);
        let class = frame.original_v1().job.class_id;
        let record = state.tir_class_v1(&class).unwrap();
        let layout = kaspa_consensus_core::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&state, class).unwrap();
        let source = kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            record,
            layout,
            frame.header_v1(),
            frame.original_v1().execution_root,
            frame.original_v1().trace_root,
            frame.descriptor_v1().maximum_leaves,
        )
        .unwrap();
        assert_eq!(source.source_id_v1(), context.source_id);
        let queries =
            kaspa_consensus_core::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, context.profile_id).unwrap();
        assert_eq!((queries.scope_id().unwrap(), queries.len()), (context.scope_id, context.queries));
        let mut missing = Vec::new();
        if !matches!(state.staged_court_row_v1(&Key::CourtSharedIds(claim)), Some(Row::CourtSharedIds(_))) {
            let mut body = Vec::new();
            for offset in (0..u32::try_from(capture.generated.len()).unwrap())
                .step_by(kaspa_consensus_core::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize)
            {
                body.extend_from_slice(&borsh::to_vec(&response(Q::Ids { offset })).unwrap());
            }
            assert!(
                body.len() as u64
                    <= kaspa_consensus_core::palw_tir_public_owned_v1::request_body_bound_v1(&source, Q::Ids { offset: 0 }).unwrap().0
            );
            missing.push((Target::FullIds, body));
        }
        for ordinal in 0..queries.len() {
            let query = queries.request_at(ordinal).unwrap();
            if matches!(query, Q::Ids { .. }) {
                continue;
            }
            let target = Target::Unit { ordinal };
            if matches!(
                state.staged_court_row_v1(&Key::CourtMaterial(claim, target.id_v1(claim).unwrap())),
                Some(Row::CourtMaterial(_))
            ) {
                continue;
            }
            if matches!(state.staged_court_row_v1(&Key::CourtSharedQuery(queries.request_id(ordinal).unwrap())), Some(Row::CourtSharedQuery(q)) if q.body.is_some())
            {
                continue;
            }
            let body = borsh::to_vec(&response(query)).unwrap();
            assert!(
                body.len() as u64 <= kaspa_consensus_core::palw_tir_public_owned_v1::request_body_bound_v1(&source, query).unwrap().0
            );
            missing.push((target, body));
        }
        (context.scope_id, missing)
    };
    let schedule = intake::material_intake_schedule_v1().unwrap();
    assert!(schedule.sompi_per_byte > 0, "existing paid schedule must actually debit the signer");
    for (target, body) in missing {
        let before = net.chain.tip_state().1;
        let before_collateral = before.bond(&producer).unwrap().collateral;
        let before_slashed = before.bond(&producer).unwrap().slashed;
        let before_charged = match before.staged_court_row_v1(&Key::CourtMaterialLedger(claim, producer)) {
            Some(Row::CourtMaterialLedger(row)) => row.charged_sompi,
            _ => 0,
        };
        let bytes = u64::try_from(body.len()).unwrap();
        let fee = bytes.checked_mul(schedule.sompi_per_byte).unwrap();
        let hash = kaspa_consensus_core::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body);
        let draft = intake::material_draft_id_v1(claim, target, producer, bytes, hash).unwrap();
        let mut actions = vec![Action::Begin { total_bytes: bytes, body_hash: hash }];
        for (part, bytes) in
            body.chunks(kaspa_consensus_core::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate()
        {
            actions.push(Action::Part {
                draft_id: draft,
                offset: (part * kaspa_consensus_core::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,
                bytes: bytes.to_vec(),
            });
        }
        actions.push(Action::Finish { draft_id: draft });
        for action in actions {
            let mut signed = intake::SignedMaterialIntakeV1 {
                version: 1,
                claim,
                scope_id: scope,
                target,
                submitter: producer,
                action,
                signature: vec![],
            };
            signed.signature =
                sign(capture.card, signed.signing_message(net.domain).unwrap().as_byte_slice(), intake::MATERIAL_INTAKE_CONTEXT_V1);
            net.send(vec![(capture.card, Obj::MaterialIntakeV1 { intake: Box::new(signed) })]).await;
        }
        let after = net.chain.tip_state().1;
        assert_eq!(after.bond(&producer).unwrap().collateral, before_collateral.checked_sub(fee).unwrap());
        assert_eq!(after.bond(&producer).unwrap().slashed, before_slashed.checked_add(fee).unwrap());
        let Some(Row::CourtMaterialLedger(ledger)) = after.staged_court_row_v1(&Key::CourtMaterialLedger(claim, producer)) else {
            panic!("real paid intake ledger")
        };
        assert_eq!(ledger.charged_sompi, before_charged.checked_add(fee).unwrap());
        assert_eq!(ledger.reserved_bytes, 0);
        assert!(after.staged_court_row_v1(&Key::CourtMaterialDraft(claim, draft)).is_none());
        match target {
            Target::FullIds => assert!(matches!(after.staged_court_row_v1(&Key::CourtSharedIds(claim)), Some(Row::CourtSharedIds(_)))),
            Target::Unit { .. } => assert!(
                matches!(after.staged_court_row_v1(&Key::CourtMaterial(claim, target.id_v1(claim).unwrap())), Some(Row::CourtMaterial(row)) if row.bytes == body)
            ),
            _ => unreachable!(),
        }
    }
    let state = net.chain.tip_state().1;
    let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
    assert_eq!(restored.state_root(), state.state_root());
    let binding = native_binding(net, capture, claim);
    with_rooted_numerical_material_v1(&restored, claim, |store| {
        assert!(store.decode_pin().is_some(), "real paid FullIds reach the rooted owner");
        for index in 0..binding.step_leaf_count {
            assert!(store.step_leaf(index).is_some(), "real paid Unit leaf {index} reaches rooted owner");
            assert!(store.step_opening(index).is_some(), "real paid Unit opening {index} reaches rooted owner");
        }
    })
    .unwrap();
}
