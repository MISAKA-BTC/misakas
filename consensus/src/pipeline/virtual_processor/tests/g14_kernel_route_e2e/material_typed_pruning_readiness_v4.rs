//! Observe the actual asynchronous pruning workflow; a published target is not
//! yet a completed, served snapshot. No point/marker or history is written here.
use super::*;
use crate::model::stores::pruning::PruningStoreReader;
use kaspa_consensus_core::palw_native_resource_v1::NativeStateCarriageV1;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub(super) async fn drain(net: &Net, db: &Path) -> (Hash64, Arc<NativeStateCarriageV1>) {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut previous = String::new();
    loop {
        let vp = net.chain.vp();
        let (point, checkpoint, retention) = {
            let p = vp.pruning_point_store.read();
            (
                p.pruning_point().expect("pruning point read"),
                p.retention_checkpoint().expect("checkpoint read"),
                p.retention_period_root().expect("retention root read"),
            )
        };
        let utxo = vp.pruning_meta_stores.read().utxoset_position().expect("pruning UTXO position read");
        let store = vp.palw_state_v2_store.read();
        let identity = store.pruning_snapshot_key().expect("durable PALW snapshot identity read must not become absence");
        let params = vp.palw_state_params_v2.as_ref().expect("active native params");
        let served = store.load_pruning_snapshot_carriage_cached(params, point);
        let observation = format!(
            "pp={point} utxo={utxo} checkpoint={checkpoint} retention={retention} snapshot={identity:?} loader={}",
            match &served {
                Ok(Some(_)) => "ready".into(),
                Ok(None) => "absent-or-other-point".into(),
                Err(e) => format!("ERROR:{e:?}"),
            }
        );
        if observation != previous {
            progress(format!("event=pruning_readiness {observation}"));
            previous = observation;
        }
        // A payload/typed admission error is not pending publication and cannot
        // be accepted after a sleep. Keep its exact diagnostic in the artifact.
        let served = served.unwrap_or_else(|e| panic!("exact-point PALW snapshot loader failed: {e:?}; {previous}"));
        drop(store);
        let after = {
            let p = vp.pruning_point_store.read();
            (
                p.pruning_point().unwrap(),
                p.retention_checkpoint().unwrap(),
                p.retention_period_root().unwrap(),
                vp.pruning_meta_stores.read().utxoset_position().unwrap(),
            )
        };
        let stable = after == (point, checkpoint, retention, utxo);
        if stable && utxo == point && checkpoint == retention {
            let carriage = served.unwrap_or_else(|| panic!("completed pruning workflow has no exact-point PALW snapshot; {previous}"));
            assert_eq!(identity.map(|(p, _)| p), Some(point));
            assert_eq!(carriage.last_point.as_ref().map(|p| p.block), Some(point));
            // The actual public retained API must serve the same rooted owner;
            // internal inspection alone is not accepted as serving evidence.
            let public = net
                .chain
                .ctx
                .consensus
                .pruning_point_palw_state_shared_v1(point)
                .unwrap_or_else(|| panic!("completed pruning point is not publicly servable; {previous}"));
            // Publication can move once more while the public reader runs.
            // Re-bracket before accepting a coherent final observation.
            let latest = {
                let p = vp.pruning_point_store.read();
                (
                    p.pruning_point().unwrap(),
                    p.retention_checkpoint().unwrap(),
                    p.retention_period_root().unwrap(),
                    vp.pruning_meta_stores.read().utxoset_position().unwrap(),
                )
            };
            if latest == after {
                assert!(Arc::ptr_eq(&carriage, &public));
                return (point, public);
            }
        }
        assert!(free_bytes(db) >= 8 << 30, "actual filesystem free space below explicit local 8GiB long-fixture floor");
        assert!(Instant::now() < deadline, "pruner did not drain to a coherent completed snapshot: {previous}");
        // Foreground mining has stopped. This yields only to the actual native
        // background workflow; success still requires all marker/owner checks.
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
