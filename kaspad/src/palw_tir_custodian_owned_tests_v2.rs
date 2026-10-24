//! Ownership controls use the actual storage wrapper and host ledger, not a model of either.
use super::*;
use crate::palw_memory_ledger::PalwMemoryPoolV1;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
fn key() -> PalwMemoryReservationKeyV1 {
    PalwMemoryReservationKeyV1 { role: "tir-custodian-owned-test", class_id: Default::default(), job: Default::default() }
}
#[test]
fn tir_custodian_owned_refuses_before_open_and_releases_failed_open() {
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64), || None);
    let calls = AtomicUsize::new(0);
    let refusal = CustodianOwnedV2::try_build(&ledger, key(), 65, || {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(vec![0u8; 65])
    });
    assert!(refusal.is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(ledger.reserved_bytes(), 0);
    let failure: Result<CustodianOwnedV2<Vec<u8>>, String> = CustodianOwnedV2::try_build(&ledger, key(), 64, || {
        assert_eq!(ledger.reserved_bytes(), 64);
        Err("backend IO failed".into())
    });
    assert!(failure.is_err());
    assert_eq!(ledger.reserved_bytes(), 0);
    let held = CustodianOwnedV2::try_build(&ledger, key(), 64, || {
        assert_eq!(ledger.reserved_bytes(), 64);
        Ok(vec![0u8; 64])
    })
    .unwrap();
    assert_eq!(held.len(), 64);
    assert!(ledger.reserve(key(), 1).is_err());
    drop(held);
    assert_eq!(ledger.reserved_bytes(), 0);
}
struct DropProbe {
    ledger: Arc<PalwMemoryLedgerV1>,
    drops: Arc<AtomicUsize>,
}
impl Drop for DropProbe {
    fn drop(&mut self) {
        assert_eq!(self.ledger.reserved_bytes(), 64, "storage drops before its reservation");
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tir_custodian_owned_cancelled_worker_keeps_taken_storage_and_output_quota() {
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(64), || None);
    let drops = Arc::new(AtomicUsize::new(0));
    let owner =
        CustodianOwnedV2::try_build(&ledger, key(), 64, || Ok(DropProbe { ledger: ledger.clone(), drops: drops.clone() })).unwrap();
    let mut book = std::collections::HashMap::new();
    book.insert(1, owner);
    let taken = book.remove(&1).unwrap();
    let (started, wait_started) = tokio::sync::oneshot::channel();
    let (release, wait_release) = mpsc::channel();
    let task = tokio::spawn(async move {
        tokio::task::spawn_blocking(move || {
            started.send(()).unwrap();
            wait_release.recv().unwrap();
            // The worker's output remains the same production owner, too.
            taken
        })
        .await
        .unwrap()
    });
    wait_started.await.unwrap();
    task.abort();
    assert!(matches!(task.await,Err(why) if why.is_cancelled()));
    drop(book);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert_eq!(ledger.reserved_bytes(), 64);
    assert!(ledger.reserve(key(), 1).is_err(), "cancelling await cannot admit another allocation");
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

struct UnusedCaptureReader;
impl kaspa_consensus_core::palw_backend::PalwTirPublicCaptureReaderV1 for UnusedCaptureReader {
    fn prepared_descriptor_v1(&self) -> Option<&kaspa_consensus_core::palw_backend::PalwTirPublicPreparedDescriptorV1> {
        None
    }
    fn prepared_need_v1(&self) -> Result<kaspa_consensus_core::palw_backend::PalwTirPublicPreparedNeedV1, String> {
        Err("unused".into())
    }
    fn begin_prepared_v1(
        &self,
        _: &std::path::Path,
        _: u64,
        _: u64,
    ) -> Result<Box<dyn kaspa_consensus_core::palw_backend::PalwTirPublicPreparedWriterV1>, String> {
        Err("unused".into())
    }
    fn slice_quote_v1(
        &self,
        _: &kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicRequestV1,
    ) -> Result<kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1, String> {
        Err("unused".into())
    }
    fn progress_v1(&self) -> kaspa_consensus_core::palw_backend::PalwTirPublicCaptureProgressV1 {
        Default::default()
    }
    fn poll_quoted_v1(
        &mut self,
        _: &kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicRequestV1,
        _: &kaspa_consensus_core::palw_backend::PalwTirPublicCaptureSliceV1,
    ) -> Result<Option<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicResponseV1>, String> {
        Err("unused".into())
    }
    fn poll_v1(
        &mut self,
        _: &kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicRequestV1,
    ) -> Result<Option<kaspa_consensus_core::palw_tir_public_source_v1::PalwTirPublicResponseV1>, String> {
        Err("unused".into())
    }
}
#[test]
fn tir_custodian_owned_capture_zeroquota_never_calls_reader_open() {
    let ledger = PalwMemoryLedgerV1::new(PalwMemoryPoolV1::Host, Some(0), || None);
    let opened = AtomicUsize::new(0);
    let result: Result<CustodianOwnedV2<Box<dyn kaspa_consensus_core::palw_backend::PalwTirPublicCaptureReaderV1>>, String> =
        CustodianOwnedV2::try_build(&ledger, key(), 64, || {
            opened.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(UnusedCaptureReader) as Box<dyn kaspa_consensus_core::palw_backend::PalwTirPublicCaptureReaderV1>)
        });
    assert!(result.is_err());
    assert_eq!(opened.load(Ordering::SeqCst), 0);
    assert_eq!(ledger.reserved_bytes(), 0);
}
