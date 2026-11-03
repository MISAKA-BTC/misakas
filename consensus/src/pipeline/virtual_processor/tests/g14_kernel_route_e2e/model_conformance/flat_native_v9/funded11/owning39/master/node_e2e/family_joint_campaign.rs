//! One actual signed A/C/native-B history. Public producer captures contain
//! material, never a state snapshot installed as consensus authority.
use super::family_legacy_campaign as family;
use super::*;
use kaspa_consensus_core::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1;
use kaspa_consensus_core::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, palw_claim_liability_frame_key_v1};
use kaspa_consensus_core::palw_state_v2::{PalwClaimLocalSettlementV1, PalwClaimStateV2, PalwStateCarriageV2};

fn source_export_v3(state: &PalwChainStateV2, source: &family::AdmittedFamilyOriginalV2) -> Option<PalwPrefixExportReservationV1> {
    let route = state.kernel_route_state_v1()?;
    let origin = source.actual_origin.expect("source export checks require an authenticated actual birth");
    let mut found = None;
    for ((table, key), raw) in &route.aux {
        if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'E') {
            continue;
        }
        let row = kaspa_consensus_core::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || {
            borsh::from_slice::<PalwPrefixExportReservationV1>(raw)
        })
        .unwrap();
        assert_eq!(key.len(), 65);
        assert_eq!(&key[1..], row.budget_id.as_byte_slice());
        if row.source_claim == source.claim
            && (row.point.block, row.point.daa_score, row.point.blue_score) == (origin.block, origin.daa_score, origin.blue_score)
        {
            assert!(found.replace(*row).is_none(), "one E reservation per actual source birth");
        }
    }
    found
}

pub(super) fn assert_no_source_export_v3(state: &PalwChainStateV2, source: &family::AdmittedFamilyOriginalV2) {
    assert!(source_export_v3(state, source).is_none(), "closed actual source birth must not retain an E reservation");
}

fn assert_open_prefix_history_v3(
    state: &PalwChainStateV2,
    a: &family::AdmittedFamilyOriginalV2,
    c: &family::AdmittedFamilyOriginalV2,
) {
    assert!(state.claim(&c.claim).is_some(), "the intermediate consumer is genuinely live");
    assert!(source_export_v3(state, a).is_some(), "a live consumer retains its actual source E");
    let route = state.kernel_route_state_v1().unwrap();
    for admitted in [a, c] {
        assert!(
            route.aux.contains_key(&(
                PALW_CLAIM_LOCAL_TABLE_V1,
                palw_claim_liability_frame_key_v1(admitted.actual_origin.expect("open history requires an authenticated birth").block)
                    .to_vec()
            )),
            "a genuinely open consumer retains the actual A/C birth frames"
        );
    }
}

fn material_clock_v3(net: &Net, state: &PalwChainStateV2, id: Hash64, now: u64) -> (u64, bool) {
    let mut until = state.claim(&id).map_or(0, |claim| claim.trace_retention_daa);
    let mut held = state.court_service_holds_claim_v1(&id) || state.open_courts_of(&id) > 0;
    for end in [
        state.material_income_hold_until_v1(&net.bundle.state, id),
        state.staged_claim_contract_v1(&id).map(|c| c.terms.material_until_daa),
        state.staged_tir_source_until_v1(id),
        state.exec_v2_liability_until_v1(&net.bundle.state, &id).unwrap(),
    ]
    .into_iter()
    .flatten()
    {
        until = until.max(end);
        held |= now <= end;
    }
    for clock in state.court_duty_clocks_for_claim_v1(id) {
        until = until.max(clock.response_deadline_daa);
        held |= now < clock.response_deadline_daa;
    }
    for (_, row) in state.family_supplier_rows_v1().unwrap().iter().filter(|(_, row)| row.consent.terms.claim == id) {
        until = until.max(row.consent.terms.material_until_daa);
        held |= now <= row.consent.terms.material_until_daa;
    }
    (until, held)
}

async fn close_prefix_history_v3(
    net: &mut Net,
    a: &family::AdmittedFamilyOriginalV2,
    c: &family::AdmittedFamilyOriginalV2,
    original_c: &PalwClaimStateV2,
    export: PalwPrefixExportReservationV1,
) {
    let state = net.chain.tip_state().1;
    let a_clock = material_clock_v3(net, &state, a.claim, net.daa());
    let c_clock = material_clock_v3(net, &state, c.claim, net.daa());
    let deadline = export
        .material_until
        .max(a_clock.0)
        .max(c_clock.0)
        .checked_add(net.bundle.state.claim_retirement_daa())
        .and_then(|d| d.checked_add(2))
        .unwrap();
    // The drain bound is proportional to actual retained rows, not an invented
    // consensus timeout or a changed duty. Every iteration is a normal block.
    let carriage = PalwStateCarriageV2::from_state(&state);
    let drain = carriage
        .staged_courts
        .len()
        .checked_add(state.kernel_route_state_v1().map_or(0, |r| r.aux.len()))
        .and_then(|n| n.checked_add(64))
        .unwrap();
    drop(carriage);
    drop(state);
    eprintln!("[family-prefix-gc] actual finite target {deadline}, drain at most {drain} normal blocks");
    net.beat_to(deadline).await;
    for _ in 0..drain {
        let state = net.chain.tip_state().1;
        let now = net.daa();
        let a_clock = material_clock_v3(net, &state, a.claim, now);
        let c_clock = material_clock_v3(net, &state, c.claim, now);
        if state.claim(&a.claim).is_none()
            && state.claim(&c.claim).is_none()
            && !a_clock.1
            && !c_clock.1
            && source_export_v3(&state, a).is_none()
        {
            assert_eq!(
                kaspa_consensus_core::palw_state_v2::palw_claim_local_recorded_settlement_v1(
                    &state,
                    &net.bundle.state,
                    c.claim,
                    original_c,
                    now
                )
                .unwrap(),
                PalwClaimLocalSettlementV1::Denied,
                "closed consumer keeps denial after its actual source conviction"
            );
            assert_no_source_export_v3(&state, a);
            eprintln!("[family-prefix-gc] closed source+consumer/material at actual DAA {now}; matching E removed");
            return;
        }
        if state.claim(&c.claim).is_some() {
            assert!(source_export_v3(&state, a).is_some(), "live consumer still retains source E");
        }
        assert!(
            a_clock.0 <= deadline && c_clock.0 <= deadline,
            "a new finite duty appeared; do not assert expiry or silently extend the campaign clock"
        );
        drop(state);
        net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    }
    let state = net.chain.tip_state().1;
    panic!(
        "actual finite prefix closure failed: daa={} A_live={} C_live={} A_material={:?} C_material={:?} E={:?}",
        net.daa(),
        state.claim(&a.claim).is_some(),
        state.claim(&c.claim).is_some(),
        material_clock_v3(net, &state, a.claim, net.daa()),
        material_clock_v3(net, &state, c.claim, net.daa()),
        source_export_v3(&state, a)
    );
}
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use std::path::Path;

/// Explicit inputs are mandatory because each producer capture commits this
/// candidate's real genesis and funded executor outpoint. No skipped leg is a
/// successful run. Both adjudication helpers submit actual signed native court
/// carriers and assert the native adjudicator's result.
#[cfg(feature = "evm")]
#[tokio::test]
#[ignore = "requires actual source/consumer producer captures for this exact funded genesis"]
async fn rfc0012_joint_signed_prefix_family_native39_evm_cold_pruned_import() {
    let source = std::env::var("MISAKA_FAMILY_SOURCE_CAPTURE_DIR").expect("actual source A producer capture directory is mandatory");
    let consumer =
        std::env::var("MISAKA_FAMILY_CONSUMER_CAPTURE_DIR").expect("actual consumer C producer capture directory is mandatory");
    for (dir, kind) in [(&source, "faulty-ordinary-source"), (&consumer, "prefix-consumer")] {
        assert_eq!(
            std::fs::read_to_string(Path::new(dir).join("fixture-kind.txt")).unwrap(),
            kind,
            "the joint caller requires genuine faulty A and independently executed prefix consumer C captures"
        );
        for name in ["original.tfo2", "state.borsh", "original.object.borsh", "raw-tokenizer.json", "producer-execution.borsh"] {
            assert!(Path::new(dir).join(name).is_file(), "missing actual producer capture {dir}/{name}");
        }
    }
    let parts = crate::pipeline::virtual_processor::tests::family_supplier_node_candidate::family_native39_campaign_config_v2();
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(parts, |cfg| TestConsensus::with_db(db.clone(), cfg, sender));
    net._keep.push(Box::new(receiver));
    let (a_capture, a, c_capture, c) =
        family::admit_source_and_prefix_consumer_v2(&mut net, Path::new(&source), Path::new(&consumer)).await;
    assert!(a.carrier.payload.starts_with(b"FPR2") && c.carrier.payload.starts_with(b"FPR2"));
    let initial = net.chain.tip_state().1;
    assert_open_prefix_history_v3(&initial, &a, &c);
    let export = source_export_v3(&initial, &a).unwrap();
    let original_c = initial.claim(&c.claim).unwrap().clone();
    let c_bond = original_c.bond;
    drop(initial);
    family::supply_family_materials_v2(&mut net, &a_capture, &a).await;
    family::supply_family_materials_v2(&mut net, &c_capture, &c).await;

    // Consumer pays while liability is open. Source retains its genuine unpaid
    // schedule until closure, where return must discharge it in the same spend.
    family::settle_family_escrow_fee_v2(&mut net, &c_capture, &c).await;
    family::complete_honest_family_court_v2(&mut net, &c_capture, &c).await;
    let c_slashed = net.chain.tip_state().1.bond(&c_bond).unwrap().slashed;
    family::adjudicate_forged_family_claim_v2(&mut net, &a_capture, &a).await;
    let after_fault = net.chain.tip_state().1;
    assert_eq!(
        kaspa_consensus_core::palw_state_v2::palw_claim_local_settlement_v1(&after_fault, &net.bundle.state, c.claim, net.daa())
            .unwrap(),
        PalwClaimLocalSettlementV1::Denied,
        "later real source conviction denies its consumer's credit"
    );
    assert_eq!(
        after_fault.bond(&c_bond).unwrap().slashed,
        c_slashed,
        "dependency denial does not additionally slash the consumer producer"
    );
    if after_fault.claim(&c.claim).is_some() {
        assert_open_prefix_history_v3(&after_fault, &a, &c);
    }
    drop(after_fault);

    let state = net.chain.tip_state().1;
    let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
    for admitted in [&a, &c] {
        family::assert_family_winning_carrier_v2(&state, admitted);
        family::assert_family_winning_carrier_v2(&restored, admitted);
    }
    drop(restored);
    drop(state);

    // Native B starts on this same signed public-family history. Its complete
    // Gate C, unreduced budget, PNR9, EVM, reorg and restart path is unchanged.
    // Independent-unresolved-A evidence is the separate stage2 native-A case;
    // here the original court must finish within its actual finite door first.
    let mut net = run_native39_node_e2e_on_net_v3(net, db.clone(), true, true, std::time::Instant::now()).await;
    close_prefix_history_v3(&mut net, &a, &c, &original_c, export).await;
    family::refund_family_escrow_reorg_v3(&mut net, &a_capture, &a, db.clone()).await;
    family::refund_family_escrow_v2(&mut net, &c_capture, &c).await;
    assert_eq!(net.chain.tip_state().1.bond(&c_bond).unwrap().slashed, c_slashed);
    let (sink, state) = net.chain.tip_state();
    assert_no_source_export_v3(&state, &a);
    for admitted in [&a, &c] {
        family::assert_family_winning_carrier_v2(&state, admitted);
    }
    let before = state.state_root();
    drop(state);
    let net = net.restart(db);
    assert_eq!((net.chain.sink(), net.chain.tip_state().1.state_root()), (sink, before));
    assert_no_source_export_v3(&net.chain.tip_state().1, &a);
    stage2_pruned::exercise_with_closed_family_carriers(&net, None, &[&a, &c]).await;
}
