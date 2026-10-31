//! Actual committed-store read failures must not manufacture an economic zero
//! or disqualify a valid block. Small native mechanics, not a reward campaign.
#![allow(warnings)]
use super::*;
use crate::model::stores::{pruning::PruningStoreReader, virtual_state::VirtualStateStoreReader};
use crate::pipeline::virtual_processor::processor::PalwLocalTransitionFaultV1;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_database::{
    create_temp_db,
    prelude::{ConnBuilder, DB, DbKey},
};
use std::{sync::Arc, time::Duration};

struct CorruptReserve {
    db: Arc<DB>,
    key: DbKey,
}
impl CorruptReserve {
    fn at(db: Arc<DB>, block: BlockHash) -> Self {
        let prefix: Vec<u8> = DatabaseStorePrefixes::ReserveBalance.into();
        let key = DbKey::new(&prefix, block);
        assert!(db.get(&key).unwrap().is_none(), "this dormant-economics fixture starts with a genuine absent row");
        db.put(&key, [0xff]).unwrap(); // an actual malformed cold u64 row
        Self { db, key }
    }
}
impl Drop for CorruptReserve {
    fn drop(&mut self) {
        // Repair even on a failed assertion, before native workers shut down.
        self.db.delete(&self.key).unwrap();
    }
}
fn config() -> kaspa_consensus_core::config::Config {
    ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            // Local fixture only: an absent reserve row canonically encodes zero.
            p.dns_params.as_mut().unwrap().pos_v2_activation_daa_score = u64::MAX;
        })
        .build()
}

#[tokio::test]
async fn rfc12_reserve_fault_stops_native_template_and_snapshot_without_zero_fallback() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (tx, _rx) = async_channel::unbounded();
    let mut ctx = TestContext::new(TestConsensus::with_db(db.clone(), &config(), tx));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor();
    assert!(vp.dns_params.is_some());
    let parent = ctx.consensus.get_sink();
    assert_eq!(vp.dns_reserve_balance_for_native_v1(parent).unwrap(), 0);
    let pp = vp.pruning_point_store.read().pruning_point().unwrap();
    vp.capture_pruning_point_overlay_snapshot(pp).unwrap();
    let before = vp.pruning_point_overlay_snapshot();
    let fault = CorruptReserve::at(db, parent);
    assert!(matches!(vp.dns_reserve_balance_for_native_v1(parent), Err(RuleError::LocalNativeStateContextUnavailable(_))));
    assert!(matches!(
        ctx.consensus.build_block_template(
            ctx.miner_data.clone(),
            Box::new(OnetimeTxSelector::new(vec![])),
            TemplateBuildMode::Standard
        ),
        Err(RuleError::LocalNativeStateContextUnavailable(_))
    ));
    assert!(matches!(
        vp.compute_overlay_snapshot(parent, &vp.initial_active_bond_view()),
        Err(RuleError::LocalNativeStateContextUnavailable(_))
    ));
    assert!(matches!(vp.capture_pruning_point_overlay_snapshot(parent), Err(RuleError::LocalNativeStateContextUnavailable(_))));
    assert_eq!(vp.pruning_point_overlay_snapshot(), before, "no incorrect zero snapshot was persisted");
    drop(fault);
    assert_eq!(vp.dns_reserve_balance_for_native_v1(parent).unwrap(), 0);
    ctx.consensus
        .build_block_template(ctx.miner_data.clone(), Box::new(OnetimeTxSelector::new(vec![])), TemplateBuildMode::Standard)
        .unwrap();
}

#[tokio::test]
async fn rfc12_reserve_fault_retries_actual_pending_candidate_without_disqualification() {
    let (_life, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (tx, _rx) = async_channel::unbounded();
    let mut ctx = TestContext::new(TestConsensus::with_db(db.clone(), &config(), tx));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let block = ctx.build_block_template_keeping_time(777).block.to_immutable();
    let vp = ctx.consensus.virtual_processor();
    let parent = ctx.consensus.get_sink();
    // Hold the real virtual gate while header/body workers admit the block.
    // The queued normal worker resumes after repair and release of this guard.
    let virtual_read = vp.virtual_stores.upgradable_read();
    let futures = ctx.consensus.validate_and_insert_block(block.clone());
    let status = tokio::time::timeout(Duration::from_secs(10), futures.block_task).await.unwrap().unwrap();
    assert_eq!(status, BlockStatus::StatusUTXOPendingVerification);
    let fault = CorruptReserve::at(db, parent);
    let previous = virtual_read.state.get().unwrap();
    let pp = vp.pruning_point_store.read().pruning_point().unwrap();
    let finality = vp.virtual_finality_point(&previous.ghostdag_data, pp);
    let mut diff = previous.utxo_diff.clone().to_reversed();
    let mut bonds = vp.initial_active_bond_view();
    assert!(matches!(
        vp.sink_search_algorithm(&virtual_read, &mut diff, &mut bonds, parent, vec![block.hash()], finality, pp),
        Err(PalwLocalTransitionFaultV1::Context(_))
    ));
    assert_eq!(ctx.consensus.block_status(block.hash()), BlockStatus::StatusUTXOPendingVerification);
    assert_eq!(ctx.consensus.get_sink(), parent);
    drop(fault);
    drop(virtual_read);
    let status = tokio::time::timeout(Duration::from_secs(10), futures.virtual_state_task).await.unwrap().unwrap();
    assert_eq!(status, BlockStatus::StatusUTXOValid);
    assert_eq!(ctx.consensus.get_sink(), block.hash(), "the exact original candidate succeeds after its local read is repaired");
}
