//! New streaming39 fullaux CAS transport. JSON labels/booleans are never
//! authority: Core authenticates every original row and lazy reread against the
//! independently pinned inner state. Old proof32 format/caps remain unchanged.
use kaspa_consensus_core::{Hash64, palw_native_audit_proof_v39::*, palw_state_proof_v1::PalwStateOpeningV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CasObjectV39 {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuxStreamHeaderV39 {
    pub schema: String,
    pub state: CasObjectV39,
    pub total_rows: usize,
    pub objects: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuxStreamRowV39 {
    pub table: u8,
    pub key: String,
    pub value: CasObjectV39,
}
#[derive(Clone)]
pub struct NativeCasAuxReaderV39 {
    pool: PathBuf,
    index: Arc<BTreeMap<(u8, Vec<u8>), CasObjectV39>>,
}
fn relative(root: &Path, name: &str) -> Result<PathBuf, String> {
    let p = Path::new(name);
    if p.is_absolute() || p.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err("audit39 CAS path must be a normal relative path".into());
    }
    let path = root.join(p);
    let real = path.canonicalize().map_err(|e| e.to_string())?;
    if !real.starts_with(root) {
        return Err("audit39 CAS symlink escapes evidence root".into());
    }
    Ok(real)
}
fn hex(s: &str) -> Result<Vec<u8>, String> {
    if s.len() % 2 != 0 || s.len() > 8192 || s.bytes().any(|c| !c.is_ascii_digit() && !(b'a'..=b'f').contains(&c)) {
        return Err("audit39 canonical lowercase byte hex".into());
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "audit39 byte hex".into())).collect()
}
fn object(pool: &Path, o: &CasObjectV39, cap: usize) -> Result<Vec<u8>, String> {
    if o.sha256.len() != 64 || hex(&o.sha256)?.len() != 32 || o.bytes > cap as u64 {
        return Err("audit39 CAS object original extent/hash".into());
    }
    let path = relative(pool, &o.sha256)?;
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if f.metadata().map_err(|e| e.to_string())?.len() != o.bytes {
        return Err("audit39 CAS exact original object length".into());
    }
    let mut raw = Vec::with_capacity(o.bytes as usize);
    std::io::Read::by_ref(&mut f).take(o.bytes + 1).read_to_end(&mut raw).map_err(|e| e.to_string())?;
    if raw.len() != o.bytes as usize || crate::runtime_pack::commit::hex(&Sha256::digest(&raw)) != o.sha256 {
        return Err("audit39 CAS original object changed".into());
    }
    Ok(raw)
}
impl NativeAuditRowReaderV39 for NativeCasAuxReaderV39 {
    fn read(&self, table: u8, key: &[u8], maximum: usize) -> Result<Option<Vec<u8>>, String> {
        self.index.get(&(table, key.to_vec())).map(|o| object(&self.pool, o, maximum)).transpose()
    }
}
fn line(r: &mut impl BufRead, maximum: usize) -> Result<Option<Vec<u8>>, String> {
    let mut out = Vec::new();
    let n = r.take(maximum as u64 + 1).read_until(b'\n', &mut out).map_err(|e| e.to_string())?;
    if n == 0 {
        return Ok(None);
    }
    if n > maximum || out.last() != Some(&b'\n') {
        return Err("audit39 bounded complete JSONL line required".into());
    }
    out.pop();
    Ok(Some(out))
}
/// Verify all original rows once. Only key/hash/size metadata is retained; page
/// bodies are reloaded and rehashed on demand by the private checked Core view.
pub fn read_native_aux_stream_v39(
    evidence_root: &Path,
    index_path: &Path,
    pinned_inner: Hash64,
) -> Result<VerifiedNativeAuxRowsV39<NativeCasAuxReaderV39>, String> {
    let root = evidence_root.canonicalize().map_err(|e| e.to_string())?;
    let path = index_path.canonicalize().map_err(|e| e.to_string())?;
    if !path.starts_with(&root) {
        return Err("audit39 stream index escapes original evidence root".into());
    }
    let mut input = BufReader::new(std::fs::File::open(path).map_err(|e| e.to_string())?);
    let first = line(&mut input, 4096)?.ok_or("audit39 full index header missing")?;
    let h: AuxStreamHeaderV39 = serde_json::from_slice(&first).map_err(|e| e.to_string())?;
    if h.schema != "misaka.palw.native-aux-stream.v39" || h.total_rows > NATIVE_AUX_ROWS_MAX_V39 {
        return Err("audit39 owning stream header/count".into());
    }
    let pool = relative(&root, &h.objects)?;
    if !pool.is_dir() {
        return Err("audit39 CAS object directory absent".into());
    }
    let state = PalwStateOpeningV1 { preimage: object(&pool, &h.state, 4 << 20)? };
    let mut index = BTreeMap::new();
    let mut metadata = 0usize;
    let mut bytes = 0u64;
    let mut prior = None;
    while let Some(raw) = line(&mut input, 16384)? {
        if index.len() >= h.total_rows {
            return Err("audit39 extra original index rows".into());
        }
        let r: AuxStreamRowV39 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let key = (r.table, hex(&r.key)?);
        if prior.as_ref().is_some_and(|p| p >= &key)
            || r.value.bytes > NATIVE_AUX_ROW_BYTES_MAX_V39 as u64
            || r.value.sha256.len() != 64
            || hex(&r.value.sha256)?.len() != 32
        {
            return Err("audit39 index sorted original key/hash/extent".into());
        }
        metadata = metadata
            .checked_add(key.1.len() + 256)
            .filter(|n| *n <= NATIVE_AUX_INDEX_BYTES_MAX_V39)
            .ok_or("audit39 index metadata reservation exhausted")?;
        bytes = bytes
            .checked_add(r.value.bytes + key.1.len() as u64 + 9)
            .filter(|n| *n <= NATIVE_AUX_STREAM_BYTES_MAX_V39)
            .ok_or("audit39 original full stream byte reservation exhausted")?;
        prior = Some(key.clone());
        index.insert(key, r.value);
    }
    if index.len() != h.total_rows {
        return Err("audit39 complete original index truncated".into());
    }
    let reader = NativeCasAuxReaderV39 { pool, index: Arc::new(index) };
    let rows = reader.index.iter().map(|((table, key), o)| {
        let value = object(&reader.pool, o, NATIVE_AUX_ROW_BYTES_MAX_V39)?;
        let key = borsh::to_vec(&(*table, key)).map_err(|e| e.to_string())?;
        Ok((key, value))
    });
    verify_native_aux_stream_v39(&state, pinned_inner, h.total_rows, rows, reader.clone())
}
