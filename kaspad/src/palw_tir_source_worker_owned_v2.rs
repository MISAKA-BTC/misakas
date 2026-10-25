//! Native Source ownership through accepted-read planning and blocking work.
use kaspa_consensus_core::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};
/// The accepted-state callback has a fixed String transport error type. Keep
/// Source refusal separately typed and stop this planning pass with Ok(()).
/// Neither local capacity nor malformed material supplies a ready work owner.
pub(crate) fn defer_source_read_v2<T>(
    refusal: &mut Option<NativeDecodeErrorV1<String>>,
    read: Result<NativeRetainedV1<T>, NativeDecodeErrorV1<String>>,
) -> Option<NativeRetainedV1<T>> {
    match read {
        Ok(owner) => Some(owner),
        Err(why) => {
            *refusal = Some(why);
            None
        }
    }
}
/// The entire owner enters the blocking closure. Cancelling the async waiter
/// cannot release its ticket while queued/running blocking work still borrows it.
pub(crate) async fn run_owned_source_v2<T: Send + 'static, R: Send + 'static>(
    source: NativeRetainedV1<T>,
    work: impl FnOnce(&T) -> R + Send + 'static,
) -> Result<R, tokio::task::JoinError> {
    tokio::task::spawn_blocking(move || work(&source)).await
}
#[cfg(test)]
#[path = "palw_tir_source_worker_owned_tests_v2.rs"]
mod tests;
/// Tests infer exact admission through the public clone operation, rather than
/// exporting Core's private quote/lease internals or assuming a payload tariff.
#[cfg(test)]
pub(crate) fn test_clone_requests_v2(bytes: &Vec<u8>, limit: u64) -> u64 {
    use kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1;
    let (mut low, mut high) = (0, limit);
    while low < high {
        let middle = low + (high - low) / 2;
        let blocker = reserve_native_requests_v1(limit - middle).unwrap();
        let owner = NativeRetainedV1::try_from_clone_v1(bytes);
        let admitted = owner.is_ok();
        drop(owner);
        drop(blocker);
        if admitted { high = middle } else { low = middle + 1 }
    }
    assert!(low > 0 && low < limit);
    low
}
