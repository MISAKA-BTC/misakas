//! Actual signed source on the joint genesis, followed by its existing native39
//! work campaign. Public captures supply execution/material, never chain state.
use super::family_legacy_campaign as family;
use super::*;
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use std::path::Path;

#[cfg(feature = "evm")]
#[tokio::test]
#[ignore = "requires actual source capture for this exact funded genesis and explicit archive payer ceilings"]
async fn original_archive_normal_node_signed_tfo_fee_and_native39_v1() {
    original_archive_node_flow_kind_v2(false).await
}
#[cfg(feature = "evm")]
#[tokio::test]
#[ignore = "requires actual source capture and explicit payer ceilings; OAR2 mixed ownership"]
async fn original_archive_normal_node_signed_tfo_fee_and_native39_mixed_v2() {
    original_archive_node_flow_kind_v2(true).await
}
#[cfg(feature = "evm")]
async fn original_archive_node_flow_kind_v2(mixed: bool) {
    let dir = std::env::var("MISAKA_FAMILY_SOURCE_CAPTURE_DIR").expect("actual source producer capture is mandatory");
    let ceiling = |name| -> u64 {
        let n = std::env::var(name)
            .unwrap_or_else(|_| panic!("explicit payer choice {name} is mandatory"))
            .parse()
            .expect("u64 payer ceiling");
        assert!(n > 0);
        n
    };
    let budget = kaspa_consensus_core::palw_state_v2::PalwOriginalArchivePayerBudgetV1 {
        additional_carrier_rent_sompi: ceiling("MISAKA_ARCHIVE_ADDITIONAL_RENT_SOMPI"),
        dedicated_archive_capital_sompi: ceiling("MISAKA_ARCHIVE_CAPITAL_SOMPI"),
    };
    // Separate real histories preserve the positive carrier's input; negative
    // ordinary transactions are genuinely accepted, not rolled back or relabeled.
    for case in [family::ArchiveCarrierCaseV1::RentShort, family::ArchiveCarrierCaseV1::CapitalShort] {
        let parts = crate::pipeline::virtual_processor::tests::family_supplier_node_candidate::family_native39_campaign_config_v2();
        let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (sender, receiver) = async_channel::unbounded();
        let mut negative = Net::over_cfg(parts, |cfg| TestConsensus::with_db(db, cfg, sender));
        negative._keep.push(Box::new(receiver));
        let capture = family::FamilyCaptureV2::read(Path::new(&dir), &negative);
        family::register_family_capture_class_v2(&mut negative, &capture).await;
        assert!(
            family::admit_family_original_archive_case_kind_v2(&mut negative, &capture, None, Some(budget), case, mixed)
                .await
                .is_none()
        );
    }
    let parts = crate::pipeline::virtual_processor::tests::family_supplier_node_candidate::family_native39_campaign_config_v2();
    let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut net = Net::over_cfg(parts, |cfg| TestConsensus::with_db(db.clone(), cfg, sender));
    net._keep.push(Box::new(receiver));
    let capture = family::FamilyCaptureV2::read(Path::new(&dir), &net);
    family::register_family_capture_class_v2(&mut net, &capture).await;
    let admitted = family::admit_family_original_archive_case_kind_v2(
        &mut net,
        &capture,
        None,
        Some(budget),
        family::ArchiveCarrierCaseV1::Positive,
        mixed,
    )
    .await
    .unwrap();
    let actual = admitted.actual_origin.expect("actual executed native birth");
    let state = net.chain.tip_state().1;
    let status =
        kaspa_consensus_core::palw_state_v2::palw_original_archive_status_v1(&state, admitted.claim, actual).unwrap().unwrap();
    assert!(status.constructed_v1() && status.material_ready_v1());
    family::assert_family_winning_carrier_v2(&state, &admitted);
    let root = state.state_root();
    let sink = net.chain.sink();
    drop(state);
    let net = net.restart(db.clone());
    assert_eq!((net.chain.sink(), net.chain.tip_state().1.state_root()), (sink, root));
    assert_eq!(
        kaspa_consensus_core::palw_state_v2::palw_original_archive_status_v1(&net.chain.tip_state().1, admitted.claim, actual)
            .unwrap(),
        Some(status)
    );
    // This is the existing real GateC/PNR9/EVM/reorg/restart work path. Its
    // unrelated legacy/mixed frame ownership is inspected, never inferred from
    // this source's receipt or charged to this source's archive reservation.
    let net = run_native39_node_e2e_on_net_v3(net, db, true, true, std::time::Instant::now()).await;
    let state = net.chain.tip_state().1;
    family::assert_family_winning_carrier_v2(&state, &admitted);
    let f1_count = state
        .kernel_route_state_v1()
        .unwrap()
        .aux
        .keys()
        .filter(|(table, key)| {
            *table == kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1
                && key.len() == 65
                && key.first() == Some(&b'F')
        })
        .count();
    eprintln!(
        "[archive-node] actual source={:?} birth={:?}; retained legacy/mixed F1 candidates={} (not a mixed ownership closure assertion)",
        admitted.claim, actual, f1_count
    );
}
