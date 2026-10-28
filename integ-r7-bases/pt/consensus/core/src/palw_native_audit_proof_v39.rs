//! Streaming full original aux authentication for owning39. Old audit32 caps
//! remain unchanged. A checked index authenticates lazy rereads by row hash;
//! every row is included in the SAME original aux-root/v1 preimage once.
use crate::{Hash64, palw_state_proof_v1::PalwStateOpeningV1, palw_state_v2::PALW_STATE_V2_VERSION};
use misaka_palw_kernel::hash::{Digest, object_id};
use std::collections::BTreeMap;
pub const NATIVE_AUX_ROWS_MAX_V39: usize = 262144;
pub const NATIVE_AUX_STREAM_BYTES_MAX_V39: u64 = 8 << 30;
pub const NATIVE_AUX_INDEX_BYTES_MAX_V39: usize = 128 << 20;
pub const NATIVE_AUX_ROW_BYTES_MAX_V39: usize = 32 << 20;
pub trait NativeAuditRowReaderV39 {
    fn read(&self, table: u8, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>, String>;
}
struct RowIndexV39 {
    bytes: usize,
    digest: Digest,
}
pub struct VerifiedNativeAuxRowsV39<R> {
    state: Hash64,
    root: Hash64,
    ledger: Hash64,
    index: BTreeMap<(u8, Vec<u8>), RowIndexV39>,
    reader: R,
    stream_bytes: u64,
}
impl<R: NativeAuditRowReaderV39> VerifiedNativeAuxRowsV39<R> {
    pub fn state_root(&self) -> Hash64 {
        self.state
    }
    pub fn root(&self) -> Hash64 {
        self.root
    }
    pub fn ledger_root(&self) -> Hash64 {
        self.ledger
    }
    pub fn stream_bytes(&self) -> u64 {
        self.stream_bytes
    }
    pub fn contains(&self, table: u8, key: &[u8]) -> bool {
        self.index.contains_key(&(table, key.to_vec()))
    }
    pub fn keys(&self) -> impl Iterator<Item = (u8, &[u8], usize)> {
        self.index.iter().map(|((t, k), v)| (*t, k.as_slice(), v.bytes))
    }
    pub fn row(&self, table: u8, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>, String> {
        let Some(expected) = self.index.get(&(table, key.to_vec())) else { return Ok(None) };
        if expected.bytes > maximum {
            return Err("audit39 requested original row exceeds caller's typed bound".into());
        }
        let raw = self.reader.read(table, key, expected.bytes)?.ok_or("audit39 original authenticated row disappeared")?;
        if raw.len() != expected.bytes || object_id(b"misaka-palw/audit/immutable-row-bytes/v39", &raw) != expected.digest {
            return Err("audit39 original row changed after full stream authentication".into());
        }
        Ok(Some(raw))
    }
}
pub fn verify_native_aux_stream_v39<R: NativeAuditRowReaderV39>(
    state: &PalwStateOpeningV1,
    pinned_inner: Hash64,
    total_rows: usize,
    rows: impl IntoIterator<Item = Result<(Vec<u8>, Vec<u8>), String>>,
    reader: R,
) -> Result<VerifiedNativeAuxRowsV39<R>, String> {
    if state.preimage.len() > 4 << 20
        || state.root() != pinned_inner
        || state.preimage.get(..2) != Some(PALW_STATE_V2_VERSION.to_le_bytes().as_slice())
        || total_rows > NATIVE_AUX_ROWS_MAX_V39
    {
        return Err("audit39 original state/rowcount/profile differs".into());
    }
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kernel-route/aux-root/v1").to_state();
    h.update(&(total_rows as u64).to_le_bytes());
    let (mut index, mut prior, mut bytes, mut index_bytes) = (BTreeMap::new(), None, 0u64, 0usize);
    for row in rows {
        if index.len() >= total_rows {
            return Err("audit39 full transcript has extra original rows".into());
        }
        let (k, v) = row?;
        if k.len() < 5
            || k.len() > 4101
            || v.len() > NATIVE_AUX_ROW_BYTES_MAX_V39
            || u32::from_le_bytes(k[1..5].try_into().unwrap()) as usize != k.len() - 5
        {
            return Err("audit39 original row exact bounded grammar".into());
        }
        let key = (k[0], k[5..].to_vec());
        if prior.as_ref().is_some_and(|p| p >= &key) {
            return Err("audit39 full original rows reordered/duplicated".into());
        }
        bytes = bytes
            .checked_add((k.len() + v.len() + 4) as u64)
            .filter(|n| *n <= NATIVE_AUX_STREAM_BYTES_MAX_V39)
            .ok_or("audit39 original complete byte reservation exhausted")?;
        index_bytes = index_bytes
            .checked_add(key.1.len() + 160)
            .filter(|n| *n <= NATIVE_AUX_INDEX_BYTES_MAX_V39)
            .ok_or("audit39 original metadata reservation exhausted")?;
        h.update(&k);
        h.update(&(v.len() as u32).to_le_bytes());
        h.update(&v);
        let row = RowIndexV39 { bytes: v.len(), digest: object_id(b"misaka-palw/audit/immutable-row-bytes/v39", &v) };
        prior = Some(key.clone());
        index.insert(key, row);
    }
    if index.len() != total_rows {
        return Err("audit39 original complete aux transcript truncated".into());
    }
    let root = Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap());
    let at = crate::palw_state_v2::PalwChainStateV2::kernel_route_offset_of_preimage_v1(&state.preimage)?
        .ok_or("audit39 original kernel commitment absent")?
        + b"kernel-route/v1".len();
    let commitment = state.preimage.get(at..at + 128).ok_or("audit39 truncated original aux commitment")?;
    if commitment[64..] != root.as_bytes() {
        return Err("audit39 all original rows do not open pinned aux root".into());
    }
    Ok(VerifiedNativeAuxRowsV39 {
        state: pinned_inner,
        root,
        ledger: Hash64::from_bytes(commitment[..64].try_into().unwrap()),
        index,
        reader,
        stream_bytes: bytes,
    })
}
