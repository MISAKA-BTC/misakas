//! Streaming root adapters for nested consensus engines. These authenticate
//! stored bytes to existing roots; whole-state import remains mandatory.
use crate::palw_native_resource_v1::{NativeDemandLeaseV1, NativeRetainedV1, reserve_native_requests_v1};
use crate::palw_row_stream_v1::PalwCollectionRootStreamV1;
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteHeaderV1};
use misaka_palw_kernel::{hash, rows};
use std::collections::BTreeMap;
use std::io::{self, Write};

pub const KERNEL_LEDGER: &[u8] = b"kernel-ledger-rows/v1";
pub const PANEL: &[u8] = b"panel-state-chunks/v1";
pub const BUDGET_HEADER: &[u8] = b"bond-budget-header-rows/v1";
pub const CHUNK_BYTES: usize = 64 << 10;

#[derive(Debug)]
pub enum NestedError {
    Decode(String),
    Resource(crate::palw_native_resource_v1::NativeCoverageV1),
}
impl From<&str> for NestedError {
    fn from(why: &str) -> Self {
        Self::Decode(why.into())
    }
}
impl From<String> for NestedError {
    fn from(why: String) -> Self {
        Self::Decode(why)
    }
}

pub enum NestedRooter {
    Aux { hash: blake2b_simd::State, declared: u64, seen: u64, previous: Option<((u8, Vec<u8>), Option<NativeDemandLeaseV1>)> },
    Kernel(Kernel),
    Panel { hash: blake2b_simd::State, declared: u64, seen: u64 },
    Budget(Budget),
}
impl NestedRooter {
    pub fn new(label: &[u8], declared: u64) -> Option<Self> {
        if label == crate::palw_native_audit_proof_v32::NATIVE_AUX_LABEL_V32 {
            let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kernel-route/aux-root/v1").to_state();
            hash.update(&declared.to_le_bytes());
            Some(Self::Aux { hash, declared, seen: 0, previous: None })
        } else if label == KERNEL_LEDGER {
            Some(Self::Kernel(Kernel {
                declared,
                seen: 0,
                header: None,
                table: None,
                hash: None,
                rows_left: 0,
                counts: [0; 32],
                roots: [[0; 64]; 32],
                previous: None,
            }))
        } else if label == PANEL {
            let domain = b"misaka-palw/panel-v3/state";
            let mut hash = blake2b_simd::Params::new().hash_length(64).to_state();
            hash.update(&(domain.len() as u32).to_le_bytes());
            hash.update(domain);
            Some(Self::Panel { hash, declared, seen: 0 })
        } else if label == BUDGET_HEADER {
            Some(Self::Budget(Budget { declared, seen: 0, header: None, roots: None, last_group: 0 }))
        } else {
            None
        }
    }
    pub fn row_count(&self) -> u64 {
        match self {
            Self::Aux { seen, .. } => *seen,
            Self::Kernel(k) => k.seen,
            Self::Panel { seen, .. } => *seen,
            Self::Budget(b) => b.seen,
        }
    }
    pub fn row(&mut self, key: &[u8], value: &[u8]) -> Result<(), NestedError> {
        match self {
            Self::Aux { hash, declared, seen, previous } => {
                if *seen >= *declared || key.len() < 5 || u32::from_le_bytes(key[1..5].try_into().unwrap()) as usize != key.len() - 5 {
                    return Err("native aux row framing or count differs".into());
                }
                if previous.as_ref().is_some_and(|((table, last), _)| (*table, last.as_slice()) >= (key[0], &key[5..])) {
                    return Err("native aux rows reordered or duplicate".into());
                }
                let lease = reserve(key.len() - 5)?;
                *previous = Some(((key[0], key[5..].to_vec()), lease));
                hash.update(key);
                hash.update(&(value.len() as u32).to_le_bytes());
                hash.update(value);
                *seen += 1;
                Ok(())
            }
            Self::Kernel(k) => k.row(key, value),
            Self::Panel { hash, declared, seen } => {
                if *seen >= *declared
                    || key != seen.to_le_bytes()
                    || value.is_empty()
                    || value.len() > CHUNK_BYTES
                    || (*seen + 1 < *declared && value.len() != CHUNK_BYTES)
                {
                    return Err("panel chunk sequence or extent differs".into());
                }
                hash.update(value);
                *seen += 1;
                Ok(())
            }
            Self::Budget(b) => b.row(key, value),
        }
    }
    pub fn finish(self) -> Result<Hash64, NestedError> {
        match self {
            Self::Aux { hash, declared, seen, .. } if declared == seen => {
                Ok(Hash64::from_bytes(hash.finalize().as_bytes().try_into().unwrap()))
            }
            Self::Aux { .. } => Err("native aux row count differs".into()),
            Self::Kernel(k) => k.finish(),
            Self::Panel { hash, declared, seen } if declared != 0 && seen == declared => {
                Ok(Hash64::from_bytes(hash.finalize().as_bytes().try_into().unwrap()))
            }
            Self::Panel { .. } => Err("panel chunk count differs".into()),
            Self::Budget(b) => b.finish(),
        }
    }
}
fn reserve(bytes: usize) -> Result<Option<NativeDemandLeaseV1>, NestedError> {
    reserve_native_requests_v1(bytes as u64).map_err(NestedError::Resource)
}
fn decode<T: borsh::BorshDeserialize + borsh::BorshSerialize + crate::palw_native_resource_v1::NativeWireScanV1>(
    bytes: &[u8],
) -> Result<NativeRetainedV1<T>, NestedError> {
    crate::palw_claim_local_v1::decode_claim_local_value_v1(bytes).map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => NestedError::Resource(e),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(e) => NestedError::Decode(format!("nested header: {e:?}")),
    })
}

pub struct Kernel {
    declared: u64,
    seen: u64,
    header: Option<NativeRetainedV1<PalwKernelRouteHeaderV1>>,
    table: Option<u8>,
    hash: Option<blake2b_simd::State>,
    rows_left: u64,
    counts: [u64; 32],
    roots: [[u8; 64]; 32],
    previous: Option<((Vec<u8>, u8, u32), Option<NativeDemandLeaseV1>)>,
}
impl Kernel {
    fn close(&mut self) -> Result<(), NestedError> {
        if self.rows_left != 0 {
            return Err("kernel table row count differs".into());
        }
        if let Some(hash) = self.hash.take() {
            self.roots[self.table.unwrap() as usize] = hash::finish(hash);
        }
        self.previous = None;
        Ok(())
    }
    fn row(&mut self, key: &[u8], value: &[u8]) -> Result<(), NestedError> {
        if self.seen >= self.declared {
            return Err("kernel row count exceeded".into());
        }
        self.seen += 1;
        if key == [0] {
            if self.header.is_some() || self.seen != 1 {
                return Err("kernel header order differs".into());
            }
            self.header = Some(decode(value)?);
            return Ok(());
        }
        if self.header.is_none() {
            return Err("kernel header missing".into());
        }
        if key.len() == 2 && key[0] == 1 {
            self.close()?;
            let table = key[1];
            if table >= 32 || self.table.map_or(table != 0, |last| table != last + 1) || value.len() != 8 {
                return Err("kernel table header order differs".into());
            }
            self.table = Some(table);
            let count = u64::from_le_bytes(value.try_into().unwrap());
            if count > self.declared - self.seen {
                return Err("kernel table extent differs".into());
            }
            self.counts[table as usize] = count;
            self.rows_left = count;
            let mut h = hash::keyed(&rows::collection_domain(table));
            h.update(value);
            self.hash = Some(h);
            return Ok(());
        }
        if key.len() < 2 || key[0] != 2 || Some(key[1]) != self.table || self.rows_left == 0 {
            return Err("kernel row outside its table".into());
        }
        let raw_key = &key[2..];
        let lease = reserve(raw_key.len().checked_mul(3).ok_or("kernel sort key overflow")?)?;
        let order = rows::sort_key(key[1], raw_key);
        if self.previous.as_ref().is_some_and(|(previous, _)| previous >= &order) {
            return Err("kernel rows reordered or duplicate".into());
        }
        self.previous = Some((order, lease));
        let mut leaf = hash::keyed(misaka_palw_kernel::state::LEDGER_LEAF_DOMAIN_V1);
        leaf.update(&((4 + raw_key.len() + value.len()) as u64).to_le_bytes());
        leaf.update(&(raw_key.len() as u32).to_le_bytes());
        leaf.update(raw_key);
        leaf.update(value);
        self.hash.as_mut().unwrap().update(&hash::finish(leaf));
        self.rows_left -= 1;
        Ok(())
    }
    fn finish(mut self) -> Result<Hash64, NestedError> {
        self.close()?;
        if self.table != Some(31) || self.seen != self.declared {
            return Err("kernel table inventory incomplete".into());
        }
        let h = self.header.ok_or("kernel header missing")?;
        let _lease = reserve(crate::palw_native_resource_v1::btree_node_envelope_bytes_v1::<u8, (u64, [u8; 64])>() * 32)?;
        let tables: BTreeMap<_, _> = (0u8..32).map(|t| (t, (self.counts[t as usize], self.roots[t as usize]))).collect();
        Ok(Hash64::from_bytes(rows::root_from_table_digests_v1(&h.policy, h.config_root, h.scalars, h.opv.as_ref(), &tables)))
    }
}

pub struct Budget {
    declared: u64,
    seen: u64,
    header: Option<NativeRetainedV1<crate::palw_bond_budget_v1::PalwBondBudgetHeaderV1>>,
    roots: Option<[PalwCollectionRootStreamV1; 3]>,
    last_group: u8,
}
impl Budget {
    fn row(&mut self, key: &[u8], value: &[u8]) -> Result<(), NestedError> {
        if self.seen >= self.declared {
            return Err("budget header row count exceeded".into());
        }
        self.seen += 1;
        if key == [0] {
            if self.seen != 1 || value.len() < 24 {
                return Err("budget header order differs".into());
            }
            let counts: [u64; 3] = std::array::from_fn(|i| u64::from_le_bytes(value[i * 8..i * 8 + 8].try_into().unwrap()));
            self.header = Some(decode(&value[24..])?);
            self.roots = Some([
                PalwCollectionRootStreamV1::new(b"bond_budget_reporter_income/v1", counts[0]),
                PalwCollectionRootStreamV1::new(b"bond_budget_material_income/v1", counts[1]),
                PalwCollectionRootStreamV1::new(b"bond_budget_consumer_contract/v1", counts[2]),
            ]);
            return Ok(());
        }
        if key.is_empty() || !(1..=3).contains(&key[0]) || key[0] < self.last_group {
            return Err("budget income group order differs".into());
        }
        self.last_group = key[0];
        self.roots.as_mut().ok_or("budget header absent")?[(key[0] - 1) as usize]
            .row(&key[1..], value)
            .map_err(|e| NestedError::Decode(e.to_string()))
    }
    fn finish(self) -> Result<Hash64, NestedError> {
        if self.seen != self.declared {
            return Err("budget header row count differs".into());
        }
        let [a, b, c] = self.roots.ok_or("budget income inventory absent")?;
        let h = self.header.ok_or("budget header absent")?;
        Ok(crate::palw_bond_budget_v1::palw_bond_budget_header_root_v1(
            &h,
            a.finish().map_err(|e| e.to_string())?,
            b.finish().map_err(|e| e.to_string())?,
            c.finish().map_err(|e| e.to_string())?,
        ))
    }
}

/// Framed row serialization directly to a staging writer.
pub fn write_row<K: borsh::BorshSerialize, V: borsh::BorshSerialize>(mut out: &mut dyn Write, key: &K, value: &V) -> io::Result<()> {
    let k = u32::try_from(borsh::object_length(key)?).map_err(|_| io::Error::other("nested row key exceeds u32"))?;
    let v = u32::try_from(borsh::object_length(value)?).map_err(|_| io::Error::other("nested row value exceeds u32"))?;
    out.write_all(&k.to_le_bytes())?;
    key.serialize(&mut out)?;
    out.write_all(&v.to_le_bytes())?;
    value.serialize(&mut out)
}
/// Raw framed row without an extra Borsh Vec length prefix.
pub fn write_raw_row(out: &mut dyn Write, key: &[u8], value: &[u8]) -> io::Result<()> {
    out.write_all(&(key.len() as u32).to_le_bytes())?;
    out.write_all(key)?;
    out.write_all(&(value.len() as u32).to_le_bytes())?;
    out.write_all(value)
}

pub struct RawBytes<'a>(pub &'a [u8]);
impl borsh::BorshSerialize for RawBytes<'_> {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(self.0)
    }
}

pub fn write_kernel_rows(route: &crate::palw_kernel_route_v1::PalwKernelRouteStateV1, out: &mut dyn Write) -> io::Result<()> {
    write_row(out, &[0u8], &route.header)?;
    for table in 0u8..32 {
        let count = route.rows.keys().filter(|(t, _)| *t == table).count();
        let max_key = route.rows.keys().filter(|(t, _)| *t == table).map(|(_, key)| key.len()).max().unwrap_or(0);
        let demand = count
            .checked_mul(std::mem::size_of::<(&(u8, Vec<u8>), &Vec<u8>)>())
            .and_then(|n| n.checked_add(max_key.checked_mul(8)?))
            .ok_or_else(|| io::Error::other("kernel row sort overflow"))?;
        let _lease =
            reserve_native_requests_v1(demand as u64).map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e.to_string()))?;
        let mut sorted = Vec::with_capacity(count);
        sorted.extend(route.rows.iter().filter(|((t, _), _)| *t == table));
        sorted.sort_unstable_by_key(|((_, key), _)| rows::sort_key(table, key));
        write_row(out, &[1u8, table], &(count as u64))?;
        for ((_, key), value) in sorted {
            out.write_all(&((2 + key.len()) as u32).to_le_bytes())?;
            out.write_all(&[2u8, table])?;
            out.write_all(key)?;
            out.write_all(&(value.len() as u32).to_le_bytes())?;
            out.write_all(value)?;
        }
    }
    Ok(())
}

pub fn write_borsh_chunks<T: borsh::BorshSerialize>(value: &T, out: &mut dyn Write) -> io::Result<()> {
    let _lease =
        reserve_native_requests_v1(CHUNK_BYTES as u64).map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e.to_string()))?;
    struct Chunks<'a> {
        out: &'a mut dyn Write,
        buffer: Vec<u8>,
        index: u64,
    }
    impl Chunks<'_> {
        fn emit(&mut self) -> io::Result<()> {
            if !self.buffer.is_empty() {
                write_raw_row(self.out, &self.index.to_le_bytes(), &self.buffer)?;
                self.buffer.clear();
                self.index += 1;
            }
            Ok(())
        }
    }
    impl Write for Chunks<'_> {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let mut rest = bytes;
            while !rest.is_empty() {
                let n = rest.len().min(CHUNK_BYTES - self.buffer.len());
                self.buffer.extend_from_slice(&rest[..n]);
                rest = &rest[n..];
                if self.buffer.len() == CHUNK_BYTES {
                    self.emit()?;
                }
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.emit()
        }
    }
    let mut chunks = Chunks { out, buffer: Vec::with_capacity(CHUNK_BYTES), index: 0 };
    value.serialize(&mut chunks)?;
    chunks.emit()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn feed(rooter: &mut NestedRooter, mut wire: &[u8]) {
        while !wire.is_empty() {
            let k = u32::from_le_bytes(wire[..4].try_into().unwrap()) as usize;
            let key = &wire[4..4 + k];
            wire = &wire[4 + k..];
            let v = u32::from_le_bytes(wire[..4].try_into().unwrap()) as usize;
            let value = &wire[4..4 + v];
            wire = &wire[4 + v..];
            rooter.row(key, value).unwrap();
        }
    }
    #[test]
    fn rfc37_nested_kernel_stream_matches_independent_ledger_and_rejects_numeric_reordering() {
        use crate::palw_kernel_route_v1::*;
        let policy = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let mut route = PalwKernelRouteStateV1::new(policy, None, rows::LedgerScalarsV1::default());
        let independent = route.ledger().unwrap().root();
        let mut wire = Vec::new();
        write_kernel_rows(&route, &mut wire).unwrap();
        let mut receiver = NestedRooter::new(KERNEL_LEDGER, 33).unwrap();
        feed(&mut receiver, &wire);
        assert_eq!(receiver.finish().unwrap().as_bytes(), independent);
        // Native key byte ordering differs from numeric position ordering.
        for position in [1u32, 255, 256, 65536] {
            route.rows.insert((rows::TABLE_DEMANDS_V1, borsh::to_vec(&([7u8; 64], 1u8, position)).unwrap()), vec![position as u8]);
        }
        wire.clear();
        write_kernel_rows(&route, &mut wire).unwrap();
        let mut receiver = NestedRooter::new(KERNEL_LEDGER, 37).unwrap();
        feed(&mut receiver, &wire);
        assert_eq!(receiver.finish().unwrap(), route.ledger_root());
        let mut truncated = NestedRooter::new(KERNEL_LEDGER, 38).unwrap();
        feed(&mut truncated, &wire);
        assert!(truncated.finish().is_err());
    }
    #[test]
    fn rfc37_nested_panel_chunks_preserve_exact_frozen_digest_and_reject_missing_chunks() {
        let value = vec![19u8; CHUNK_BYTES * 2 + 1];
        let encoded = borsh::to_vec(&value).unwrap();
        let mut wire = Vec::new();
        write_borsh_chunks(&value, &mut wire).unwrap();
        let mut receiver = NestedRooter::new(PANEL, 3).unwrap();
        feed(&mut receiver, &wire);
        let domain = b"misaka-palw/panel-v3/state";
        let mut hash = blake2b_simd::Params::new().hash_length(64).to_state();
        hash.update(&(domain.len() as u32).to_le_bytes());
        hash.update(domain);
        hash.update(&encoded);
        assert_eq!(receiver.finish().unwrap().as_bytes(), hash.finalize().as_bytes());
        let mut missing = NestedRooter::new(PANEL, 3).unwrap();
        missing.row(&0u64.to_le_bytes(), &encoded[..CHUNK_BYTES]).unwrap();
        assert!(missing.finish().is_err());
    }
    #[test]
    fn rfc37_native_aux_stream_matches_existing_root_without_collection_buffer() {
        let mut rows = Vec::new();
        for index in 0..5u8 {
            rows.push((borsh::to_vec(&(index, vec![index; index as usize])).unwrap(), vec![index; 3]));
        }
        let expected = crate::palw_native_audit_proof_v32::native_aux_opening_root_v32(&rows).unwrap();
        let mut receiver = NestedRooter::new(crate::palw_native_audit_proof_v32::NATIVE_AUX_LABEL_V32, 5).unwrap();
        for (key, value) in &rows {
            receiver.row(key, value).unwrap();
        }
        assert_eq!(receiver.finish().unwrap(), expected);
    }
}
