//! Replay search source and the reservation that paid for its live storage.
use crate::palw_memory_ledger::PalwMemoryReservationV1;
use kaspa_consensus_core::palw_backend::PalwTirOwnStepSourceV1;
use std::{ops::Deref, sync::Arc};

/// Move-only: callers can borrow the source, but cannot clone an unguarded Arc.
pub(super) struct OwnedTirReplayV2 {
    source: Arc<dyn PalwTirOwnStepSourceV1>,
    // Declared after source so it drops after the last locally owned source.
    _memory: PalwMemoryReservationV1,
}
impl OwnedTirReplayV2 {
    pub(super) fn open_reserved(
        memory: PalwMemoryReservationV1,
        open: impl FnOnce() -> Result<Arc<dyn PalwTirOwnStepSourceV1>, String>,
    ) -> Result<Self, String> {
        // Held on this blocking worker's stack throughout open, including errors.
        let source = open()?;
        Ok(Self { source, _memory: memory })
    }
    pub(super) fn as_ref(&self) -> &dyn PalwTirOwnStepSourceV1 {
        self.source.as_ref()
    }
}
impl Deref for OwnedTirReplayV2 {
    type Target = dyn PalwTirOwnStepSourceV1;
    fn deref(&self) -> &Self::Target {
        self.source.as_ref()
    }
}
#[cfg(test)]
#[path = "palw_tir_owner_replay_owned_tests_v2.rs"]
mod tests;
