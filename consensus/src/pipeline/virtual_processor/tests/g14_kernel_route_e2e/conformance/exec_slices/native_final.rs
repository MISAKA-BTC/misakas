//! Native root settlement uses real signed readiness, the chain's Panel draw and
//! its receipt assembler. The surrounding REAL admission and trace lottery seam
//! remains: this verifies settlement plumbing, not an honest native trace witness.

use super::*;
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, palw_artifact_multiproof_v1};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_READINESS_V2_BUDGET_BYTES_V1, PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, palw_readiness_v2_challenge_seed_v1,
    palw_readiness_v2_draw_v1, palw_seat_readiness_message_v2,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V3_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3, palw_receipt_message_v3,
};
use kaspa_consensus_core::palw_state_v2::{PALW_BOND_CAPABILITY_V2_MLDSA87_CONTEXT, palw_bond_capability_message_v2};
use kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_inventory_operands_v1;
use kaspa_consensus_core::palw_verification_v2::palw_segment_assignment_v2;
use kaspa_consensus_core::palw_work_slice_v2::PalwExecV2RewardFundV1;

/// Prove the actual native inventory at this span. No cached readiness row or
/// lifecycle/Panel/Final setter is used. Each proof is authenticated at acceptance.
async fn ready_native_seats(cw: &mut Cw) {
    use crate::pipeline::virtual_processor::tests::g14_registration_e2e::Tensors;
    let tensors = cw.f.params.tensors.iter().map(|(key, tensor)| (*key, tensor.to_le_bytes())).collect();
    let operands = palw_tir_inventory_operands_v1(&cw.f.program, &Tensors(tensors)).expect("native inventory");
    let leaves: Vec<_> = operands.iter().map(artifact_leaf_v1).collect();
    let state = cw.net.chain.tip_state().1;
    let capabilities: Vec<_> = (0..cw.net.chain.bonds.len())
        .map(|card| {
            let bond = cw.net.bond(card);
            let mut capable_classes = state.bond(&bond).unwrap().capable_classes.clone();
            capable_classes.insert(cw.v2_class);
            let message = palw_bond_capability_message_v2(cw.net.domain, &bond, &capable_classes);
            (
                card,
                Obj::BondCapabilityDeclared {
                    bond,
                    capable_classes,
                    signature: sign(card, message.as_byte_slice(), PALW_BOND_CAPABILITY_V2_MLDSA87_CONTEXT, 0x91),
                },
            )
        })
        .collect();
    cw.net.send(capabilities).await;
    let fold = cw.net.chain.vp().palw_model_registry_fold_at(cw.net.daa()).expect("native registry");
    let span = kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1(cw.net.daa(), fold.span_daa);
    let mut proofs = Vec::new();
    for card in 0..cw.net.chain.bonds.len() {
        let bond = cw.net.bond(card);
        let bytes = borsh::to_vec(&bond).unwrap();
        let seed = palw_readiness_v2_challenge_seed_v1(&cw.v2_class, &bytes, span);
        let draw = palw_readiness_v2_draw_v1(&seed, leaves.len() as u32);
        let mut opened = Vec::new();
        let mut total = 0;
        for index in draw {
            let operand = operands[index as usize].clone();
            total += operand.bytes.len();
            opened.push((index, operand));
            if total >= PALW_READINESS_V2_BUDGET_BYTES_V1 {
                break;
            }
        }
        let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("real artifact multiproof");
        let message = palw_seat_readiness_message_v2(cw.net.domain, &bytes, &cw.v2_class, span, &proof);
        proofs.push((
            card,
            Obj::SeatReadinessProvedV2 {
                bond,
                class_id: cw.v2_class,
                span,
                proof: Box::new(proof),
                signature: sign(card, message.as_byte_slice(), PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, 0x92),
            },
        ));
    }
    cw.net.send(proofs).await;
    let state = cw.net.chain.tip_state().1;
    for card in 0..cw.net.chain.bonds.len() {
        let row = state.seat_readiness(&cw.net.bond(card), &cw.v2_class).expect("accepted readiness proof");
        assert_eq!((row.proved_span, row.proof_version), (span, 2));
        assert!(fold.readiness_row_is_fresh(row, cw.net.daa()));
        assert!(state.bond(&cw.net.bond(card)).unwrap().capable_classes.contains(&cw.v2_class));
    }
}

/// Anchor on an actual attempt, then carry the exact seats' signed native V3
/// receipts. Replaying a receipt signs no new financial right.
async fn license_native_root(cw: &mut Cw, root: Hash64) -> Obj {
    ready_native_seats(cw).await;
    cw.net.chain.attempt_at_the_anchor_slot(root, REGISTRANT).await;
    let state = cw.net.chain.tip_state().1;
    let panel = state.panel(&root).expect("chain-derived native Panel").clone();
    assert!(matches!(state.claim(&root).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }));
    assert_eq!(panel.seats.len(), cw.net.bundle.panel.seat_count() as usize);
    assert!(panel.seats.iter().all(|seat| seat.bond != cw.net.bond(ROOT_CARD)));
    let assignment = palw_segment_assignment_v2(panel.anchor, root, panel.seats.len() as u16);
    let signed_daa = cw.net.chain.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<_> = panel
        .seats
        .iter()
        .enumerate()
        .map(|(index, seat)| {
            let card = cw.net.chain.bonds.iter().position(|bond| *bond == seat.bond).expect("native seat key");
            let segments = assignment.mask_of(index as u16);
            let message = palw_receipt_message_v3(cw.net.domain, root, PalwReceiptVerdictV2::Valid, signed_daa, segments);
            PalwSeatReceiptV3 {
                receipt: PalwSeatReceiptV2 {
                    claim: root,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: seat.bond,
                    signed_daa,
                    signature: sign(card, message.as_byte_slice(), PALW_RECEIPT_V3_MLDSA87_CONTEXT, 0x93),
                },
                segments,
            }
        })
        .collect();
    let licence =
        cw.net.chain.vp().palw_v2_receipt_coverage_assemble_impl(root, &receipts).expect("native assembler licenses coverage");
    assert!(matches!(&licence, Obj::ReceiptLicensedV2 { claim, .. } if *claim == root));
    cw.net.send(vec![(REGISTRANT, licence.clone())]).await;
    assert!(matches!(cw.net.chain.tip_state().1.claim(&root).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    licence
}

async fn finalize_native_root(cw: &mut Cw, s: &Session) -> (u64, Obj) {
    let licence = license_native_root(cw, s.real.claim).await;
    let end = cw.net.daa() + cw.net.bundle.state.window_challenge() + 1_000;
    while !matches!(cw.net.chain.tip_state().1.claim(&s.real.claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }) {
        assert!(cw.net.daa() < end, "native Final must be reachable before the signed root expiry");
        cw.net.beat_to(cw.net.daa() + 1).await;
    }
    let state = cw.net.chain.tip_state().1;
    let PalwClaimPhaseV2::Final { final_daa } = state.claim(&s.real.claim).unwrap().phase else { unreachable!() };
    assert!(matches!(state.exec_v2_root_v1(&s.real.claim).unwrap().phase,
        PalwWorkRootPhaseV2::Settled { settled_daa } if settled_daa == final_daa));
    (final_daa, licence)
}

#[tokio::test]
async fn x8_g14_native_root_final_funds_each_leg_once_and_public_conviction_cancels_all_unpaid_funds() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut cw = active_world_over(VerificationModeV1::OptimisticPublicVerification, |config| {
        TestConsensus::with_db(db.clone(), config, sender)
    })
    .await;
    cw.net._keep.push(Box::new(receiver));
    // Signed worker terms are fixed before any computation. The native trace
    // commitment remains the module's seam; only the prefix kernel matrix lies.
    let mut s = opened_with_lifetime(&mut cw, true, |_| false, |_, _, _| Vec::new(), 1_024).await;
    for (index, slice) in s.slices.iter().enumerate() {
        s.lane.push(carry_slice(&mut cw, s.real.claim, index as u32, &slice.claim).await);
    }
    let poster_before = cw.net.collateral(REGISTRANT);
    let before_final = cw.net.ledger();
    let fee_maxima: std::collections::BTreeMap<_, _> = std::iter::once(&s.prefix)
        .chain(s.slices.iter())
        .map(|claim| (claim.claim.id, before_final.job_escrows[&claim.job.id()].amount))
        .collect();
    let (final_daa, licence) = finalize_native_root(&mut cw, &s).await;
    let state = cw.net.chain.tip_state().1;
    let receipt = state.exec_v2_reward_vesting_v1(&s.real.claim).expect("funded independent receipt").clone();
    let native = state.vesting_row(&s.real.claim).expect("forced native vesting").clone();
    assert_eq!(receipt.final_daa, final_daa);
    assert!(receipt.liability_until >= native.final_daa + cw.net.bundle.state.window_court());
    assert!(native.expiry_daa > receipt.liability_until);
    let ledger = cw.net.ledger();
    eprintln!(
        "[x8-native-final] funded allocation={}, native producer={}, escrow={}, receipt={:?}, root={:?}",
        receipt.allocation_sompi,
        native.producer.amount,
        state.claim(&s.real.claim).unwrap().escrowed_reward,
        receipt.legs,
        state.exec_v2_root_v1(&s.real.claim)
    );
    let fee_total: u64 = std::iter::once(&s.prefix)
        .chain(s.slices.iter())
        .map(|claim| {
            assert!(!ledger.exec_reward_holds.contains_key(&claim.claim.id));
            assert!(!ledger.job_escrows.contains_key(&claim.job.id()));
            let row = &ledger.claims[&claim.claim.id];
            let native_root = state.exec_v2_root_v1(&s.real.claim).unwrap();
            let fee = if claim.claim.id == s.prefix.claim.id {
                match native_root.prefix {
                    PalwWorkPrefixStageV2::Verified { route_reward, .. } => route_reward,
                    _ => unreachable!(),
                }
            } else {
                let index = s.slices.iter().position(|slice| slice.claim.id == claim.claim.id).unwrap();
                match state.exec_v2_slice_v1(&s.real.claim, index as u32).unwrap().stage {
                    PalwWorkSliceStageV2::Verified { route_reward, .. } => route_reward,
                    _ => unreachable!(),
                }
            };
            assert_eq!(row.rewarded, fee > 0, "a resolved capped worker right records only actually paid money");
            assert!(fee <= fee_maxima[&claim.claim.id], "root settlement never debits more than its job's actual held escrow");
            fee
        })
        .sum();
    let fees_held: u64 =
        receipt.legs.iter().filter(|((_, fund), _)| *fund == PalwExecV2RewardFundV1::KernelFee).map(|(_, leg)| leg.amount).sum();
    assert!(fee_total > 0, "fixture must exercise actual funded fees");
    assert_eq!(fees_held, fee_total);
    assert_eq!(poster_before - cw.net.collateral(REGISTRANT), fee_total, "every fee has the actual poster debit");
    assert_eq!(receipt.total_sompi_u128() + u128::from(native.producer.amount), u128::from(receipt.allocation_sompi));
    let counters = state.vesting_counters();
    let final_root = state.state_root();
    let final_sink = cw.net.chain.sink();
    let final_utxos = Net::utxos_of(&cw.net.chain);
    let final_route = cw.net.api();
    // Stop Consensus and reopen the exact physical database. Compare stored
    // funds and the complete UTXO set before a new block can recompute them.
    cw.net = cw.net.restart(db.clone());
    let reopened = cw.net.chain.tip_state().1;
    assert_eq!(cw.net.chain.sink(), final_sink);
    assert_eq!(reopened.state_root(), final_root);
    assert_eq!(reopened.exec_v2_reward_vesting_v1(&s.real.claim), Some(&receipt));
    assert_eq!(reopened.vesting_row(&s.real.claim), Some(&native));
    assert_eq!(reopened.vesting_counters(), counters);
    assert_eq!(cw.net.api(), final_route);
    assert_eq!(Net::utxos_of(&cw.net.chain), final_utxos);
    assert_eq!(cw.net.ledger().to_rows(), ledger.to_rows());
    cw.net.send(vec![(REGISTRANT, licence)]).await;
    let repeated = cw.net.chain.tip_state().1;
    assert_eq!(repeated.exec_v2_reward_vesting_v1(&s.real.claim), Some(&receipt));
    assert_eq!(repeated.vesting_counters(), counters, "replayed native licence funds no second allocation");
    assert_eq!(cw.net.collateral(REGISTRANT), poster_before - fee_total);
    let replay = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&replay, "native funded-Final replay");

    let api = cw.net.api().unwrap();
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x94);
    let proof = match fresh.check(s.prefix.claim.id, &s.prefix.claim.published(&cw.cand, &[]), &cw.cand.params) {
        OutsiderFindingV1::Prosecute(proof) => proof,
        other => panic!("public post-native-Final proof must find prefix lie: {other:?}"),
    };
    let objection = cw.net.route(ACCUSER, &K::FileProof { accuser: cw.net.kid(ACCUSER), claim: s.prefix.claim.id, proof });
    cw.net.send(vec![(ACCUSER, objection.clone())]).await;
    let failed = cw.net.chain.tip_state().1;
    assert!(cw.net.ledger().claims[&s.prefix.claim.id].convicted);
    assert!(matches!(failed.exec_v2_root_v1(&s.real.claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
    assert!(failed.vesting_row(&s.real.claim).is_none());
    assert!(failed.exec_v2_reward_vesting_v1(&s.real.claim).is_none());
    let failed_counters = failed.vesting_counters();
    let held_total = receipt.total_sompi_u128()
        + u128::from(native.producer.amount)
        + u128::from(native.reserve)
        + native.seats.iter().map(|(_, leg)| u128::from(leg.amount)).sum::<u128>();
    assert_eq!(failed_counters.created, counters.created);
    assert_eq!(failed_counters.moved, counters.moved, "no composite leg paid before conviction");
    assert_eq!(failed_counters.burned - counters.burned, held_total);
    let failed_root = failed.state_root();
    let failed_sink = cw.net.chain.sink();
    let failed_utxos = Net::utxos_of(&cw.net.chain);
    let failed_route = cw.net.api();
    cw.net = cw.net.restart(db);
    assert_eq!(cw.net.chain.sink(), failed_sink);
    assert_eq!(cw.net.chain.tip_state().1.state_root(), failed_root);
    assert_eq!(cw.net.chain.tip_state().1.vesting_counters(), failed_counters);
    assert!(cw.net.chain.tip_state().1.exec_v2_reward_vesting_v1(&s.real.claim).is_none());
    assert!(cw.net.chain.tip_state().1.vesting_row(&s.real.claim).is_none());
    assert_eq!(cw.net.api(), failed_route);
    assert_eq!(Net::utxos_of(&cw.net.chain), failed_utxos);
    cw.net.send(vec![(ACCUSER, objection)]).await;
    assert_eq!(cw.net.chain.tip_state().1.vesting_counters(), failed_counters, "duplicate proof burns no second fund");
    let replay = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&replay, "post-native-Final public conviction replay");
}

/// Actual Final money is journaled through a real shallow selected-chain
/// rollback, not by calling revert_delta directly. Public proof reapplication
/// after that rollback must burn the restored funds once, with no second fee.
#[tokio::test]
async fn x8_g14_native_final_funds_recover_after_actual_reorg_and_public_proof_reapplies_once() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut cw = active_world_over(VerificationModeV1::OptimisticPublicVerification, |config| {
        TestConsensus::with_db(db.clone(), config, sender)
    })
    .await;
    cw.net._keep.push(Box::new(receiver));
    let mut s = opened_with_lifetime(&mut cw, true, |_| false, |_, _, _| Vec::new(), 1_024).await;
    for (index, slice) in s.slices.iter().enumerate() {
        s.lane.push(carry_slice(&mut cw, s.real.claim, index as u32, &slice.claim).await);
    }
    finalize_native_root(&mut cw, &s).await;
    let funded = cw.net.chain.tip_state().1;
    let native = funded.vesting_row(&s.real.claim).unwrap().clone();
    let receipt = funded.exec_v2_reward_vesting_v1(&s.real.claim).unwrap().clone();
    assert!(receipt.legs.values().any(|leg| leg.amount > 0), "the reorg must carry actual money");
    let counters = funded.vesting_counters();
    let funded_ledger = cw.net.ledger().to_rows();
    let poster = cw.net.collateral(REGISTRANT);
    let root_money = (cw.net.collateral(ROOT_CARD), cw.net.slashed(ROOT_CARD));
    let fork = cw.net.chain.sink();
    let fork_daa = cw.net.daa();
    let prefix_len = chain_blocks(&cw.net.chain, fork).len();
    let healthy = replay_with_lane(&cw.net, &s.lane).await;
    let mut healthy = cw.net.on_chain(healthy);
    healthy.chain.ctx.simulated_time = chain_blocks(&healthy.chain, fork).last().unwrap().header.timestamp;
    let api = cw.net.api().unwrap();
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x95);
    let OutsiderFindingV1::Prosecute(proof) =
        fresh.check(s.prefix.claim.id, &s.prefix.claim.published(&cw.cand, &[]), &cw.cand.params)
    else {
        panic!("real post-Final prefix proof")
    };
    let objection = cw.net.route(ACCUSER, &K::FileProof { accuser: cw.net.kid(ACCUSER), claim: s.prefix.claim.id, proof });
    cw.net.send(vec![(ACCUSER, objection.clone())]).await;
    let convicted_tip = cw.net.chain.sink();
    let convicted_blocks: Vec<_> = chain_blocks(&cw.net.chain, convicted_tip).into_iter().skip(prefix_len).collect();
    assert!(
        cw.net.daa() - fork_daa <= kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1,
        "the actual incumbent proof branch must be shallow enough to undo"
    );
    assert!(cw.net.ledger().claims[&s.prefix.claim.id].convicted);
    assert!(cw.net.chain.tip_state().1.vesting_row(&s.real.claim).is_none());
    let failed_root = cw.net.chain.tip_state().1.state_root();
    cw.net = cw.net.restart(db.clone());
    assert_eq!(cw.net.chain.tip_state().1.state_root(), failed_root, "reopen the actual failed incumbent");
    let target_len = chain_blocks(&cw.net.chain, convicted_tip).len();
    while chain_blocks(&healthy.chain, healthy.chain.sink()).len() <= target_len {
        healthy.chain.heartbeat(healthy.ttpb(), Vec::new()).await;
    }
    for block in chain_blocks(&healthy.chain, healthy.chain.sink()).into_iter().skip(prefix_len) {
        arrive(&cw.net.chain, block, "the actual funded Final branch returning after public conviction").await;
    }
    assert_eq!(cw.net.chain.sink(), healthy.chain.sink(), "the physical-db node really changes selected chain");
    for block in convicted_blocks {
        arrive(&healthy.chain, block, "retain the competing proof tip before virtual UTXO comparison").await;
    }
    assert_eq!(cw.net.chain.ctx.consensus.get_virtual_parents(), healthy.chain.ctx.consensus.get_virtual_parents());
    assert_eq!(Net::utxos_of(&cw.net.chain), Net::utxos_of(&healthy.chain));
    cw.net.assert_same(&healthy.chain, "actual native Final financial recovery");
    let restored = cw.net.chain.tip_state().1;
    assert_eq!(restored.vesting_row(&s.real.claim), Some(&native));
    assert_eq!(restored.exec_v2_reward_vesting_v1(&s.real.claim), Some(&receipt));
    assert_eq!(restored.vesting_counters(), counters, "the old burn is undone; no new allocation is created");
    assert_eq!(cw.net.collateral(REGISTRANT), poster, "the once-paid poster fee is neither refunded nor charged again");
    assert_eq!((cw.net.collateral(ROOT_CARD), cw.net.slashed(ROOT_CARD)), root_money);
    for claim in std::iter::once(&s.prefix).chain(s.slices.iter()) {
        let key = (misaka_palw_kernel::rows::TABLE_CLAIMS_V1, borsh::to_vec(&claim.claim.id).unwrap());
        assert_eq!(cw.net.ledger().to_rows().get(&key), funded_ledger.get(&key), "actual kernel Final/fee right restores exactly");
    }
    let recovered_root = restored.state_root();
    let recovered_sink = cw.net.chain.sink();
    let recovered_utxos = Net::utxos_of(&cw.net.chain);
    cw.net = cw.net.restart(db);
    assert_eq!(cw.net.chain.sink(), recovered_sink);
    assert_eq!(cw.net.chain.tip_state().1.state_root(), recovered_root);
    assert_eq!(Net::utxos_of(&cw.net.chain), recovered_utxos);
    // Actor change chains follow the winning branch, exactly as an external
    // wallet observes its unspent funding after rollback.
    cw.net.funding = healthy.funding;
    cw.net.send(vec![(ACCUSER, objection)]).await;
    let failed_again = cw.net.chain.tip_state().1;
    assert!(cw.net.ledger().claims[&s.prefix.claim.id].convicted);
    assert!(failed_again.vesting_row(&s.real.claim).is_none());
    assert!(failed_again.exec_v2_reward_vesting_v1(&s.real.claim).is_none());
    assert_eq!(failed_again.vesting_counters().created, counters.created);
    assert_eq!(failed_again.vesting_counters().moved, counters.moved);
    assert_eq!(cw.net.collateral(REGISTRANT), poster);
}
