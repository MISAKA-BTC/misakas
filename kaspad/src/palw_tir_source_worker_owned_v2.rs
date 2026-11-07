//! Native Source ownership through accepted-read planning and blocking work.
//! An incomplete worker hands back to the public shared-material path. It does
//! not attribute a slash, and a missing private response is not `PartyDefault`.
use kaspa_consensus_core::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};

/// Why fetch, replay, or localization did not finish. Every arm leaves public
/// submission open. None of them is a panel verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicWorkerIncompleteV2 {
    /// The blocking worker stopped, panicked, or was cancelled.
    Stopped,
    /// No chain-accepted rooted body was readable. An off-chain copy is not a substitute.
    WithheldBody,
    /// A rooted record does not reproduce the accepted body.
    TamperedRecord,
    /// A rooted chunk names a different signer than the accepted body.
    DoubleSignature,
}
impl PublicWorkerIncompleteV2 {
    /// Missing private work does not authorize the native party-default charge.
    pub(super) fn attributes_party_default_v2(self) -> bool {
        match self {
            Self::Stopped | Self::WithheldBody | Self::TamperedRecord | Self::DoubleSignature => false,
        }
    }
    pub(super) fn leaves_public_submission_v2(self) -> bool {
        match self {
            Self::Stopped | Self::WithheldBody | Self::TamperedRecord | Self::DoubleSignature => true,
        }
    }
}

/// `Completed` is local evidence for the existing public filer. `Remains` is
/// not queued and is not a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicWorkerHandoffV2<T> {
    Completed(T),
    Remains(PublicWorkerIncompleteV2),
}

pub(super) fn classify_worker_incomplete_v2(why: &str) -> PublicWorkerIncompleteV2 {
    let why = why.to_ascii_lowercase();
    if why.contains("double sign") || (why.contains("substitut") && why.contains("signer")) {
        return PublicWorkerIncompleteV2::DoubleSignature;
    }
    if why.contains("checksum")
        || why.contains("tamper")
        || why.contains("does not reproduce")
        || why.contains("substitut")
        || why.contains("more chunks")
    {
        return PublicWorkerIncompleteV2::TamperedRecord;
    }
    if why.contains("withheld") || why.contains("lost a rooted") || why.contains("no rooted") || why.contains("missing body") {
        return PublicWorkerIncompleteV2::WithheldBody;
    }
    PublicWorkerIncompleteV2::Stopped
}

/// `accepted` is the chain read of a rooted body. `None` is withheld. A private
/// transcript is not an argument here and cannot complete the handoff.
pub(super) fn rooted_chunk_handoff_v2<T>(accepted: Option<T>) -> PublicWorkerHandoffV2<T> {
    match accepted {
        Some(body) => PublicWorkerHandoffV2::Completed(body),
        None => PublicWorkerHandoffV2::Remains(PublicWorkerIncompleteV2::WithheldBody),
    }
}

pub(super) fn handoff_join_v2<T>(joined: Result<T, tokio::task::JoinError>) -> PublicWorkerHandoffV2<T> {
    match joined {
        Ok(value) => PublicWorkerHandoffV2::Completed(value),
        Err(_) => PublicWorkerHandoffV2::Remains(PublicWorkerIncompleteV2::Stopped),
    }
}

pub(super) fn handoff_worker_result_v2<T>(joined: Result<Result<T, String>, tokio::task::JoinError>) -> PublicWorkerHandoffV2<T> {
    match joined {
        Ok(Ok(value)) => PublicWorkerHandoffV2::Completed(value),
        Ok(Err(why)) => PublicWorkerHandoffV2::Remains(classify_worker_incomplete_v2(&why)),
        Err(_) => PublicWorkerHandoffV2::Remains(PublicWorkerIncompleteV2::Stopped),
    }
}
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
