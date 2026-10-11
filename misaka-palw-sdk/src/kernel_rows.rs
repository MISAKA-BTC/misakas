//! Bounded collection of op-211 pages. This verifies a consistent complete
//! inventory before the existing route-root verifier runs. Caller-supplied roots
//! still need authentication against the caller's trusted chain view.
use kaspa_hashes::Hash64;

pub type KernelRowV1 = (u8, Vec<u8>, Vec<u8>);

/// Local tooling limits, not consensus population or soundness approval.
#[derive(Clone, Copy)]
pub struct KernelRowsLimitsV1 {
    pub maximum_rows: u64,
    pub maximum_bytes: usize,
}
impl Default for KernelRowsLimitsV1 {
    fn default() -> Self {
        Self { maximum_rows: 1_000_000, maximum_bytes: 384 << 20 }
    }
}

/// The roots, header and advertised inventory must stay fixed across all pages.
#[derive(Clone, PartialEq, Eq)]
pub struct KernelRowsSnapshotV1 {
    pub tip_daa: u64,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
    pub header: Vec<u8>,
    pub total_rows: u64,
}

pub struct KernelRowsCollectorV1 {
    limits: KernelRowsLimitsV1,
    snapshot: Option<KernelRowsSnapshotV1>,
    rows: Vec<KernelRowV1>,
    cursor: Option<(u8, Vec<u8>)>,
    bytes: usize,
    complete: bool,
}
impl KernelRowsCollectorV1 {
    pub fn new(limits: KernelRowsLimitsV1) -> Self {
        Self { limits, snapshot: None, rows: Vec::new(), cursor: None, bytes: 0, complete: false }
    }

    /// Errors invalidate the read: discard the collector and start again.
    pub fn append(
        &mut self,
        snapshot: KernelRowsSnapshotV1,
        rows: Vec<KernelRowV1>,
        next: Option<(u8, Vec<u8>)>,
    ) -> Result<(), String> {
        if self.complete || snapshot.total_rows > self.limits.maximum_rows {
            return Err("kernel inventory exceeds the row limit or continues after completion".into());
        }
        if self.snapshot.as_ref().is_some_and(|first| first != &snapshot) {
            return Err("kernel route snapshot changed between pages; read again".into());
        }
        let new_count = (self.rows.len() as u64).checked_add(rows.len() as u64).ok_or("kernel row count overflow")?;
        if new_count > snapshot.total_rows || (next.is_some() && (rows.is_empty() || new_count >= snapshot.total_rows)) {
            return Err("kernel page cannot advance within its advertised inventory".into());
        }
        let mut previous = self.cursor.as_ref().map(|(table, key)| (*table, key.as_slice()));
        let mut bytes = self.bytes;
        if self.snapshot.is_none() {
            bytes = bytes.checked_add(snapshot.header.len()).ok_or("kernel read size overflow")?;
        }
        for (table, key, row) in &rows {
            let current = (*table, key.as_slice());
            if previous.is_some_and(|old| current <= old) {
                return Err("kernel rows repeat or move behind the requested cursor".into());
            }
            previous = Some(current);
            // Include the per-row allocations as well as payload bytes.
            bytes = bytes
                .checked_add(key.len())
                .and_then(|n| n.checked_add(row.len()))
                .and_then(|n| n.checked_add(std::mem::size_of::<KernelRowV1>()))
                .ok_or("kernel read size overflow")?;
        }
        if bytes > self.limits.maximum_bytes {
            return Err("kernel inventory exceeds the tooling memory limit".into());
        }
        if let Some(cursor) = &next {
            if previous != Some((cursor.0, cursor.1.as_slice())) {
                return Err("kernel continuation cursor is not the last served row".into());
            }
        } else if new_count != snapshot.total_rows {
            return Err("kernel final page is missing advertised rows".into());
        }
        self.bytes = bytes;
        self.cursor = next;
        self.complete = self.cursor.is_none();
        self.snapshot.get_or_insert(snapshot);
        self.rows.extend(rows);
        Ok(())
    }

    pub fn cursor(&self) -> Option<&(u8, Vec<u8>)> {
        self.cursor.as_ref()
    }

    pub fn finish(self) -> Result<(KernelRowsSnapshotV1, Vec<KernelRowV1>), String> {
        if !self.complete {
            return Err("kernel inventory is incomplete".into());
        }
        Ok((self.snapshot.ok_or("kernel inventory was never read")?, self.rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(total_rows: u64) -> KernelRowsSnapshotV1 {
        KernelRowsSnapshotV1 {
            tip_daa: 10,
            ledger_root: Hash64::from_bytes([1; 64]),
            aux_root: Hash64::from_bytes([2; 64]),
            header: vec![3],
            total_rows,
        }
    }
    fn row(table: u8) -> KernelRowV1 {
        (table, vec![0], vec![table])
    }
    fn collector() -> KernelRowsCollectorV1 {
        KernelRowsCollectorV1::new(KernelRowsLimitsV1::default())
    }

    #[test]
    fn kernel_pages_finish_one_complete_ordered_inventory() {
        let mut read = collector();
        read.append(snapshot(2), vec![row(1)], Some((1, vec![0]))).unwrap();
        assert_eq!(read.cursor(), Some(&(1, vec![0])));
        read.append(snapshot(2), vec![row(32)], None).unwrap();
        let (observed, rows) = read.finish().unwrap();
        assert_eq!(observed.total_rows, 2);
        assert_eq!(rows, vec![row(1), row(32)]);
        let mut empty = collector();
        empty.append(snapshot(0), vec![], None).unwrap();
        assert!(empty.finish().unwrap().1.is_empty());
    }

    #[test]
    fn kernel_pages_refuse_stalled_repeated_missing_and_substituted_inventory() {
        let mut read = collector();
        assert!(read.append(snapshot(2), vec![], Some((0, vec![]))).is_err());
        assert!(read.append(snapshot(2), vec![row(1)], Some((9, vec![0]))).is_err());
        read.append(snapshot(2), vec![row(1)], Some((1, vec![0]))).unwrap();
        assert!(read.append(snapshot(2), vec![row(1)], None).is_err());
        assert!(read.append(snapshot(2), vec![], None).is_err());
        for changed in 0..5 {
            let mut altered = snapshot(2);
            match changed {
                0 => altered.tip_daa += 1,
                1 => altered.ledger_root = Hash64::default(),
                2 => altered.aux_root = Hash64::default(),
                3 => altered.header.push(4),
                _ => altered.total_rows = 3,
            }
            assert!(read.append(altered, vec![row(32)], None).is_err());
        }
        assert!(read.finish().is_err());
    }

    #[test]
    fn kernel_pages_refuse_resource_overrun_before_retaining_rows() {
        let mut read = KernelRowsCollectorV1::new(KernelRowsLimitsV1 { maximum_rows: 1, maximum_bytes: 100 });
        assert!(read.append(snapshot(2), vec![row(1)], Some((1, vec![0]))).is_err());
        let mut large = row(1);
        large.2 = vec![0; 101];
        assert!(read.append(snapshot(1), vec![large], None).is_err());
        assert!(read.rows.is_empty());
        read.append(snapshot(1), vec![row(1)], None).unwrap();
        assert!(read.append(snapshot(1), vec![], None).is_err());
    }
}
