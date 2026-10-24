//! Actual move-only owner/open callback and host ledger. No fake valid model replay.
use super::*;
use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryPoolV1, PalwMemoryReservationKeyV1};
use kaspa_consensus_core::{Hash64, palw_backend::PalwTirOwnStepSourceInfoV1};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
fn key() -> PalwMemoryReservationKeyV1 {
    PalwMemoryReservationKeyV1 { role: "tir-owner-lifetime-test", class_id: Default::default(), job: Default::default() }
}
struct SourceDrop {
    ledger: Arc<PalwMemoryLedgerV1>,
    drops: Arc<AtomicUsize>,
}
impl Drop for SourceDrop {
    fn drop(&mut self) {
        assert_eq!(self.ledger.reserved_bytes(), 64);
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
impl PalwTirOwnStepSourceV1 for SourceDrop {
    fn info_v1(&self) -> &PalwTirOwnStepSourceInfoV1 {
        panic!("lifetime probe has no execution information")
    }
    fn node_v1(&self, _: u8, _: u64) -> Result<Hash64, String> {
        Err("unused".into())
    }
    fn frontier_v1(&self, _: u8, _: u64) -> Result<Vec<Hash64>, String> {
        Err("unused".into())
    }
    fn leaf_hashes_v1(&self, _: u64, _: u32) -> Result<Vec<Hash64>, String> {
        Err("unused".into())
    }
    fn row_node_v1(&self, _: u8, _: u64) -> Result<Hash64, String> {
        Err("unused".into())
    }
    fn row_frontier_v1(&self, _: u8, _: u64) -> Result<Vec<Hash64>, String> {
        Err("unused".into())
    }
}
#[test]
fn tir_owner_replay_owned_failed_open_releases_and_case_retains_source_quota() {
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64), || None);
    let memory = ledger.reserve(key(), 64).unwrap();
    let failed = OwnedTirReplayV2::open_reserved(memory, || {
        assert_eq!(ledger.reserved_bytes(), 64);
        Err("source IO unavailable".into())
    });
    assert!(failed.is_err());
    assert_eq!(ledger.reserved_bytes(), 0);
    let drops = Arc::new(AtomicUsize::new(0));
    let owned = OwnedTirReplayV2::open_reserved(ledger.reserve(key(), 64).unwrap(), || {
        Ok(Arc::new(SourceDrop { ledger: ledger.clone(), drops: drops.clone() }) as Arc<dyn PalwTirOwnStepSourceV1>)
    })
    .unwrap();
    let mut cases = std::collections::HashMap::new();
    cases.insert(1, owned);
    assert!(ledger.reserve(key(), 1).is_err());
    assert!(cases.get(&1).unwrap().as_ref().node_v1(0, 0).is_err());
    cases.clear();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(ledger.reserved_bytes(), 0);
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tir_owner_replay_owned_cancelled_open_keeps_worker_and_returned_source_reserved() {
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64), || None);
    let memory = ledger.reserve(key(), 64).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let worker_ledger = ledger.clone();
    let worker_drops = drops.clone();
    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (release, wait_release) = mpsc::channel();
    let task = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            OwnedTirReplayV2::open_reserved(memory, || {
                assert_eq!(worker_ledger.reserved_bytes(), 64);
                started.send(()).unwrap();
                wait_release.recv().unwrap();
                Ok(Arc::new(SourceDrop { ledger: worker_ledger, drops: worker_drops }) as Arc<dyn PalwTirOwnStepSourceV1>)
            })
        })
        .await
        .unwrap()
    });
    wait_started.await.unwrap();
    task.abort();
    assert!(matches!(task.await,Err(why) if why.is_cancelled()));
    assert_eq!(ledger.reserved_bytes(), 64);
    assert!(ledger.reserve(key(), 1).is_err());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    release.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while ledger.reserved_bytes() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
