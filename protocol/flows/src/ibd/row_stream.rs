//! Receive telemetry for the authenticated RFC-0037 streaming import path.
//! The negotiated streaming receiver performs validation and ordinary atomic import;
//! this process-local diagnostic slot never grants import authority.
use super::IbdFlow;
#[cfg(test)]
use crate::palw_snapshot_chunks::Snapshot;
#[cfg(test)]
use kaspa_consensus_core::palw_row_stream_v1::PalwRowStreamErrorV1;
use kaspa_consensus_core::{BlockHash, Hash64};
#[cfg(test)]
use kaspa_p2p_lib::common::ProtocolError;

#[cfg(test)]
fn row_stream_error(err: PalwRowStreamErrorV1) -> ProtocolError {
    match err {
        PalwRowStreamErrorV1::NativeResource(unavailable) => ProtocolError::LocalNativeCapacityUnavailable(unavailable),
        PalwRowStreamErrorV1::ByteBudget(_) | PalwRowStreamErrorV1::Sink(_) => {
            ProtocolError::LocalNativeResourceUnavailable(err.to_string())
        }
        other => ProtocolError::OtherOwned(format!("PALW row stream generation refused: {other}")),
    }
}

// One bounded diagnostic slot, never a state-import authority or storage marker.
// It lets node integration checks distinguish an authenticated codec2 receive
// from a negotiated109 connection which used the explicit107 fallback.
static LAST_AUTHENTICATED_ROW_SNAPSHOT_V1: std::sync::Mutex<Option<(u64, BlockHash, Hash64, u64)>> = std::sync::Mutex::new(None);

impl IbdFlow {
    /// Process-local receive telemetry: sequence, pruning point, authenticated
    /// state root, complete collection count. This records staging validation;
    /// ordinary atomic import and stable-generation publication are separate.
    pub fn last_authenticated_row_snapshot_v1() -> Option<(u64, BlockHash, Hash64, u64)> {
        *LAST_AUTHENTICATED_ROW_SNAPSHOT_V1.lock().unwrap()
    }

    pub(super) fn record_authenticated_row_snapshot_v1(point: BlockHash, root: Hash64, collections: u64) {
        let mut last = LAST_AUTHENTICATED_ROW_SNAPSHOT_V1.lock().unwrap();
        let sequence = last.as_ref().map_or(1, |receipt| receipt.0.saturating_add(1));
        *last = Some((sequence, point, root, collections));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_snapshot_native_refusal_keeps_the_typed_local_capacity_cause() {
        let capacity = kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<Snapshot>(
            "row stream test",
            "local reservation refused",
        );
        assert!(matches!(
            row_stream_error(PalwRowStreamErrorV1::NativeResource(capacity)),
            ProtocolError::LocalNativeCapacityUnavailable(_)
        ));
        assert!(matches!(row_stream_error(PalwRowStreamErrorV1::TruncatedGeneration), ProtocolError::OtherOwned(_)));
    }
}
