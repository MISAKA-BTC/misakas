//! Native pruning-carriage import before an ordinary outside verifier starts. The importer has
//! neither a PALW tip nor pre-point delta rows and receives no retained producer capture.
use super::*;
use kaspa_consensus::model::stores::acceptance_data::AcceptanceDataStore;
use kaspa_consensus::model::stores::block_transactions::BlockTransactionsStore;
use kaspa_consensus::model::stores::palw_state_v2::PalwStateTipRecordV2;
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use kaspa_database::prelude::{CachePolicy, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_pruning_carriage_verifier_acquires_automatic_multipart_and_releases_after_independent_replay() {
    fresh_public_service_with_recovery(PublicCase::UnretainedLargeConstraintJob, Some(RecoveryMode::Pruned)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_pruning_carriage_verifier_defaults_when_the_automatic_producer_withholds_material() {
    fresh_public_service_with_recovery(PublicCase::UnretainedWithheldFpV3Binding, Some(RecoveryMode::Pruned)).await;
}

pub(in crate::palw_panel::palw_service_public_e2e) async fn import_public_carriage(
    chain: &mut Chain,
    premine: &[Funding],
    claim: Hash64,
) {
    let point = chain.tc.get_sink();
    chain.tc.virtual_processor().capture_pruning_point_palw_state(point);
    let wire =
        borsh::to_vec(&chain.tc.pruning_point_palw_state(point).expect("the source serves a captured pruning-point state")).unwrap();
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).unwrap();
    let expected = chain.tc.palw_state_v2_store.read().load_tip(&chain.bundle.state).unwrap().unwrap().1.state_root();
    // The native import is authenticated by the selected child, not by the peer's claimed root.
    let child = chain.heartbeat(vec![]).await;
    assert_eq!(child.header.palw_state_root, expected);
    let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _receiver) = async_channel::unbounded();
    let tc = Arc::new(TestConsensus::with_db(db.clone(), &chain.config, sender));
    let mut multiset = MuHash::new();
    tc.append_imported_pruning_point_utxos(premine, &mut multiset).expect("the premine is appended");
    tc.import_pruning_point_utxo_set(chain.config.params.genesis.hash, multiset).unwrap();
    let handles = tc.init();
    let mut fresh = Chain {
        tc,
        handles,
        config: chain.config.clone(),
        bundle: chain.bundle.clone(),
        bonds: chain.bonds.clone(),
        network_domain: chain.network_domain,
        time: chain.time,
        nonce: chain.nonce,
        history: chain.history.clone(),
        _pruned_db: Some(lifetime),
    };
    // Independent prefix validation establishes the same header/UTXO history. The PALW tip and
    // every prefix delta are then actually deleted; none can stand in for the supplied carriage.
    for block in &chain.history[..chain.history.len() - 1] {
        fresh.tc.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("the importer validates the prefix");
    }
    fresh
        .tc
        .validate_and_insert_block(Block::from_header_arc(child.header.clone()))
        .virtual_state_task
        .await
        .expect("the root witness is a normally validated selected-child header");
    assert_eq!(fresh.tc.get_sink(), point);
    {
        let mut store = fresh.tc.palw_state_v2_store.write();
        let blocks: Vec<_> = store.iter_delta_blocks().collect();
        let mut batch = rocksdb::WriteBatch::default();
        for block in blocks {
            store.delete_delta_batch(&mut batch, block).unwrap();
        }
        db.write(batch).unwrap();
        // Same singleton deletion as the consensus crate's pruned-join fixtures, using the
        // public database primitive because its delete_tip_for_tests is crate-test-only.
        let mut tip = CachedDbItem::<PalwStateTipRecordV2>::new(db.clone(), DatabaseStorePrefixes::PalwStateV2Tip.into());
        tip.remove(DirectDbWriter::new(&db)).unwrap();
        *store = store.clone_with_new_cache(CachePolicy::Empty);
        assert!(store.load_tip(&fresh.bundle.state).unwrap().is_none());
        assert_eq!(store.iter_delta_blocks().count(), 0);
    }
    let mut forged = carriage.clone();
    forged.claims.get_mut(&claim).unwrap().execution_root = Hash64::from_bytes([0xED; 64]);
    assert!(
        fresh.tc.import_pruning_point_palw_state(point, forged).is_err(),
        "a changed live claim cannot be installed against the selected-child root"
    );
    assert!(fresh.tc.palw_state_v2_store.read().load_tip(&fresh.bundle.state).unwrap().is_none());
    fresh.tc.import_pruning_point_palw_state(point, carriage).expect("the normal carriage installs against the committed root");
    let (installed_at, installed) = fresh.tc.palw_state_v2_store.read().load_tip(&fresh.bundle.state).unwrap().unwrap();
    assert_eq!(installed_at, point);
    assert_eq!(installed.state_root(), expected);
    assert_eq!(
        borsh::to_vec(&fresh.tc.pruning_point_palw_state(point).expect("an importer can serve the same authenticated carriage"))
            .unwrap(),
        wire
    );
    assert!(matches!(fresh.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert!(fresh.view(claim).public_fp_job.is_none());
    fresh.tc.validate_and_insert_block(child).virtual_state_task.await.expect("the first body folds from the imported state");
    assert_eq!(fresh.tc.get_sink(), chain.tc.get_sink());
    assert_eq!(
        fresh.tc.palw_state_v2_store.read().load_tip(&fresh.bundle.state).unwrap().unwrap().1.state_root(),
        chain.tc.palw_state_v2_store.read().load_tip(&chain.bundle.state).unwrap().unwrap().1.state_root()
    );
    let outsider = fresh.tc.palw_bond_of_pubkey_v2(&card_pubkey(OUTSIDER)).unwrap().0;
    let candidate = fresh.tc.palw_fraud_filer_candidates_v1(outsider).into_iter().find(|c| c.claim_id == claim && !c.seat).unwrap();
    let original = fresh
        .history
        .iter()
        .find(|b| {
            b.transactions.iter().any(|t| {
                t.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
                    && borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3>(&t.payload)
                        .is_ok_and(|p| p.claim_id() == claim)
            })
        })
        .unwrap()
        .hash();
    fresh.tc.block_transactions_store.delete(original).unwrap();
    fresh.tc.acceptance_data_store.delete(candidate.job.accepted_block).unwrap();
    assert!(fresh.tc.get_block(original).is_err());
    assert!(fresh.tc.get_block_acceptance_data(candidate.job.accepted_block).is_err());
    assert!(
        crate::palw_panel::palw_fraud_filer::palw_fraud_filer_fp_payload_v1(
            fresh.tc.as_ref().as_ref(),
            &candidate,
            fresh.network_domain
        )
        .is_err()
    );
    eprintln!("[service-pruned] authenticated native carriage restored an absent tip/delta prefix at DAA {}", fresh.daa());
    std::mem::swap(chain, &mut fresh);
}
