//! Allocation-free scan of the existing fixed-int, little-endian bincode
//! BlockBody -> transparent Arc<Vec<Transaction>> codec. This is a local DB
//! projection, not a network decoder or a transaction-ID recomputation.
//! Keep actual production-codec parity tests beside the store on schema changes.
use std::{fmt, mem::size_of};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CodecError {
    pub offset: usize,
    pub reason: &'static str,
}
impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "transaction ID projection at byte {}: {}", self.offset, self.reason)
    }
}
struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], CodecError> {
        let end = self.offset.checked_add(len).ok_or_else(|| self.error("length overflow"))?;
        let bytes = self.bytes.get(self.offset..end).ok_or_else(|| self.error("truncated field"))?;
        self.offset = end;
        Ok(bytes)
    }
    fn error(&self, reason: &'static str) -> CodecError {
        CodecError { offset: self.offset, reason }
    }
    fn length(&mut self) -> Result<usize, CodecError> {
        let value = u64::from_le_bytes(self.take(8)?.try_into().unwrap());
        usize::try_from(value).map_err(|_| self.error("length does not fit this target"))
    }
    fn count(&mut self, minimum_item_bytes: usize) -> Result<usize, CodecError> {
        let len = self.length()?;
        if len > (self.bytes.len() - self.offset) / minimum_item_bytes {
            return Err(self.error("collection length exceeds remaining frame"));
        }
        Ok(len)
    }
    fn byte_vector(&mut self) -> Result<&'a [u8], CodecError> {
        let len = self.length()?;
        self.take(len)
    }
    fn hash(&mut self) -> Result<&'a [u8; 64], CodecError> {
        if self.length()? != 64 {
            return Err(self.error("Hash64 length is not 64"));
        }
        Ok(self.take(64)?.try_into().unwrap())
    }
}

/// Validate every field without constructing inputs, outputs, scripts or payload.
/// Hash64 uses serialize_bytes (8-byte count + 64 bytes); SubnetworkId uses a
/// fixed 20-byte tuple; ScriptPublicKey is u16 followed by its byte sequence.
/// TransactionMass and the cached ID are both persisted after payload.
pub fn scan_ids<'a>(bytes: &'a [u8], mut id: impl FnMut(&'a [u8; 64])) -> Result<usize, CodecError> {
    let mut row = Cursor { bytes, offset: 0 };
    let transactions = row.count(142)?;
    for _ in 0..transactions {
        row.take(2)?; // version
        let inputs = row.count(93)?;
        for _ in 0..inputs {
            row.hash()?;
            row.take(4)?; // previous outpoint index
            row.byte_vector()?; // signature script stays borrowed
            row.take(8 + 1)?; // sequence + sig op count
        }
        let outputs = row.count(18)?;
        for _ in 0..outputs {
            row.take(8 + 2)?; // value + ScriptPublicKey version
            row.byte_vector()?;
        }
        row.take(8 + 20 + 8)?; // lock time + subnetwork + gas
        row.byte_vector()?; // payload stays borrowed
        row.take(8)?; // mass (serde AtomicU64)
        id(row.hash()?);
    }
    // CachedDbAccess uses bincode 1.3.3's deserialize function, which sets
    // fixint + allow_trailing_bytes (unlike DefaultOptions::new()). Preserve
    // that accepted DB-row behavior; this projection adds no validity rule.
    Ok(transactions)
}

/// The independently owned IDs and the reservation are dropped in that order.
/// The reservation factory runs after validation but before the first ID Vec
/// allocation. The pinned source bytes must remain unchanged for both passes.
pub struct ProjectedIds<T, L> {
    pub ids: Vec<T>,
    pub lease: L,
}
#[derive(Debug)]
pub enum ProjectionError<E> {
    Codec(CodecError),
    Capacity(E),
    Allocation(std::collections::TryReserveError),
}
pub fn project_ids<T, L, E>(
    bytes: &[u8],
    reserve: impl FnOnce(u64) -> Result<L, E>,
    mut make_id: impl FnMut([u8; 64]) -> T,
) -> Result<ProjectedIds<T, L>, ProjectionError<E>> {
    let count = scan_ids(bytes, |_| {}).map_err(ProjectionError::Codec)?;
    let amount = id_reservation_bytes::<T>(count)
        .ok_or_else(|| ProjectionError::Codec(CodecError { offset: 0, reason: "ID allocation overflow" }))?;
    let mut owner = ProjectedIds { ids: Vec::new(), lease: reserve(amount).map_err(ProjectionError::Capacity)? };
    owner.ids.try_reserve_exact(count).map_err(ProjectionError::Allocation)?;
    scan_ids(bytes, |id| owner.ids.push(make_id(*id))).map_err(ProjectionError::Codec)?;
    Ok(owner)
}
pub fn id_reservation_bytes<T>(count: usize) -> Option<u64> {
    u64::try_from(count).ok()?.checked_mul(size_of::<T>() as u64)?.checked_add(128)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_core::memory_budget::{MemoryBudget, MemoryClass};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    // Fixed codec fixture only for malformed-frame and admission/lifetime tests.
    // Actual Transaction/BlockBody bincode parity is tested at the store seam.
    fn empty_transactions(count: u64) -> Vec<u8> {
        let mut bytes = count.to_le_bytes().to_vec();
        for n in 0..count {
            bytes.extend(0u16.to_le_bytes());
            bytes.extend(0u64.to_le_bytes()); // inputs
            bytes.extend(0u64.to_le_bytes()); // outputs
            bytes.extend(0u64.to_le_bytes()); // lock time
            bytes.extend([0; 20]);
            bytes.extend(0u64.to_le_bytes()); // gas
            bytes.extend(0u64.to_le_bytes()); // payload
            bytes.extend(17u64.to_le_bytes()); // mass
            bytes.extend(64u64.to_le_bytes());
            bytes.extend([n as u8; 64]);
        }
        bytes
    }
    #[test]
    fn malformed_frames_are_refused_before_reservation_or_output_construction() {
        let good = empty_transactions(2);
        for size in 0..good.len() {
            let reserved = AtomicUsize::new(0);
            let result = project_ids(
                &good[..size],
                |_| {
                    reserved.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, ()>(())
                },
                |id| id,
            );
            assert!(matches!(result, Err(ProjectionError::Codec(_))), "truncated at {size}");
            assert_eq!(reserved.load(Ordering::SeqCst), 0);
        }
        let mut bad = good.clone();
        bad[..8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(scan_ids(&bad, |_| {}).is_err());
        let mut bad = good.clone();
        bad[10..18].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(scan_ids(&bad, |_| {}).is_err());
        for offset in [18usize, 62, 78] {
            let mut bad = good.clone();
            // First transaction: outputs count, payload length, Hash64 length.
            bad[offset..offset + 8].copy_from_slice(&u64::MAX.to_le_bytes());
            assert!(scan_ids(&bad, |_| {}).is_err());
        }
    }
    #[test]
    fn trailing_bytes_follow_the_existing_bincode_function_policy() {
        let mut bytes = empty_transactions(2);
        bytes.extend([0xff; 1024]);
        let mut ids = Vec::new();
        assert_eq!(scan_ids(&bytes, |id| ids.push(*id)).unwrap(), 2);
        assert_eq!(ids, vec![[0; 64], [1; 64]]);
    }
    #[test]
    fn pressure_refuses_before_materialization_and_release_allows_progress() {
        let budget = MemoryBudget::new(4096, [4096; 4]);
        let occupied = budget.try_reserve(MemoryClass::Transport, 4096 - 512).unwrap();
        let made = AtomicUsize::new(0);
        let bytes = empty_transactions(8);
        let result = project_ids(
            &bytes,
            |n| budget.try_reserve(MemoryClass::Snapshot, n),
            |id| {
                made.fetch_add(1, Ordering::SeqCst);
                id
            },
        );
        assert!(matches!(result, Err(ProjectionError::Capacity(_))));
        assert_eq!(made.load(Ordering::SeqCst), 0);
        assert!(budget.try_reserve(MemoryClass::Transport, 64).is_ok(), "small control request still progresses");
        drop(occupied);
        let ids = project_ids(&bytes, |n| budget.try_reserve(MemoryClass::Snapshot, n), |id| id).unwrap();
        ids.lease.materialized();
        assert_eq!(ids.ids.len(), 8);
        assert_eq!(budget.snapshot().reserved_bytes, 8 * 64 + 128);
        assert_eq!(budget.snapshot().pending_bytes, 0);
        drop(ids);
        assert_eq!(budget.snapshot().reserved_bytes, 0);
    }
    #[test]
    fn more_than_twenty_thousand_ids_has_no_artificial_page_limit() {
        let bytes = empty_transactions(20_001);
        let budget = MemoryBudget::new(4 << 20, [4 << 20; 4]);
        let ids = project_ids(&bytes, |n| budget.try_reserve(MemoryClass::Snapshot, n), |id| id).unwrap();
        assert_eq!(ids.ids.len(), 20_001);
        assert_eq!(ids.ids.last().unwrap(), &[20_000u64 as u8; 64]);
    }
    #[test]
    fn panic_and_normal_drop_keep_ticket_until_all_projected_data_is_gone() {
        struct Id {
            _value: [u8; 64],
            bank: Arc<MemoryBudget>,
            dropped: Arc<AtomicUsize>,
        }
        impl Drop for Id {
            fn drop(&mut self) {
                assert!(self.bank.snapshot().reserved_bytes > 0, "data destructor must still be charged");
                self.dropped.fetch_add(1, Ordering::SeqCst);
            }
        }
        let budget = MemoryBudget::new(4096, [4096; 4]);
        let dropped = Arc::new(AtomicUsize::new(0));
        let bytes = empty_transactions(3);
        let run = |panic_after_first: bool| {
            let mut made = 0;
            project_ids(
                &bytes,
                |n| budget.try_reserve(MemoryClass::Snapshot, n),
                |value| {
                    if panic_after_first && made == 1 {
                        panic!("simulated consumer failure");
                    }
                    made += 1;
                    Id { _value: value, bank: Arc::clone(&budget), dropped: Arc::clone(&dropped) }
                },
            )
        };
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(true))).is_err());
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
        assert_eq!(budget.snapshot().reserved_bytes, 0);
        drop(run(false).unwrap());
        assert_eq!(dropped.load(Ordering::SeqCst), 4);
        assert_eq!(budget.snapshot().reserved_bytes, 0);
    }
}
