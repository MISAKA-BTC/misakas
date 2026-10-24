//! Node-local storage and its host quota are one move-only lifetime.
use crate::palw_memory_ledger::{PalwMemoryLedgerV1, PalwMemoryReservationKeyV1, PalwMemoryReservationV1};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

pub(super) struct CustodianOwnedV2<T> {
    // Rust drops fields in declaration order: storage dies before its quota.
    pub(super) value: T,
    _memory: PalwMemoryReservationV1,
}
impl<T> CustodianOwnedV2<T> {
    pub(super) fn with_reservation(value: T, memory: PalwMemoryReservationV1) -> Self {
        Self { value, _memory: memory }
    }
    pub(super) fn try_build(
        ledger: &Arc<PalwMemoryLedgerV1>,
        key: PalwMemoryReservationKeyV1,
        bytes: u64,
        build: impl FnOnce() -> Result<T, String>,
    ) -> Result<Self, String> {
        let memory = ledger.reserve(key, bytes).map_err(|e| e.to_string())?;
        let value = build()?;
        Ok(Self::with_reservation(value, memory))
    }
}
impl<T> Deref for CustodianOwnedV2<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T> DerefMut for CustodianOwnedV2<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

#[cfg(test)]
#[path = "palw_tir_custodian_owned_tests_v2.rs"]
mod tests;
