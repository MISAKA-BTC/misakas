//! CAS is transport only. Every reconstructed proof must still pass the SAME
//! pinned-header/Core32 root verifier. A JSON flag never authorizes a receipt.
use kaspa_consensus_core::palw_state_proof_v1::{PalwCollectionOpeningV1, PalwFactProofV1, PalwStateOpeningV1};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    io::Read,
    path::{Path, PathBuf},
};
pub const CAS_MANIFEST_MAX_BYTES_V1: usize = 160 << 20;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCasObjectV1 {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCasRowV1 {
    pub key: String,
    pub value: NativeCasObjectV1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeCasFactProofV1 {
    pub schema: String,
    pub objects: String,
    pub state: NativeCasObjectV1,
    pub label: String,
    pub rows: Vec<NativeCasRowV1>,
}
fn hexbytes(s: &str, cap: usize) -> Result<Vec<u8>, String> {
    if !s.len().is_multiple_of(2) || s.len() / 2 > cap || !s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
        return Err("receipt CAS canonical bounded hex".into());
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "receipt CAS hex".into())).collect()
}
fn relative(root: &Path, p: &str) -> Result<PathBuf, String> {
    let path = Path::new(p);
    if path.is_absolute() || path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return Err("receipt CAS relative object pool required".into());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let p = root.join(path).canonicalize().map_err(|e| e.to_string())?;
    if !p.starts_with(root) {
        return Err("receipt CAS object escapes evidence root".into());
    }
    Ok(p)
}
fn read_object(pool: &Path, o: &NativeCasObjectV1, cap: usize) -> Result<Vec<u8>, String> {
    if o.sha256.len() != 64 || o.bytes > cap as u64 {
        return Err("receipt CAS object hash/extent".into());
    }
    let expected = hexbytes(&o.sha256, 32)?;
    let file = pool.join(&o.sha256).canonicalize().map_err(|e| e.to_string())?;
    if !file.starts_with(pool) {
        return Err("receipt CAS object symlink escapes pool".into());
    }
    let raw = crate::native_audit_io_v32::read_bounded_file_v32(&file, o.bytes as usize)?;
    if raw.len() != o.bytes as usize || Sha256::digest(&raw).as_slice() != expected.as_slice() {
        return Err("receipt CAS object missing/truncated/mutated".into());
    }
    Ok(raw)
}
pub fn read_native_fact_proof_cas_v1(evidence_root: &Path, path: &Path) -> Result<PalwFactProofV1, String> {
    let compressed = crate::native_audit_io_v32::read_bounded_file_v32(path, CAS_MANIFEST_MAX_BYTES_V1)?;
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(compressed.as_slice())
        .take(CAS_MANIFEST_MAX_BYTES_V1 as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    if raw.len() > CAS_MANIFEST_MAX_BYTES_V1 {
        return Err("receipt CAS expanded manifest cap".into());
    }
    let m: NativeCasFactProofV1 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if m.schema != "misaka.palw.native-fact-proof-cas.v1"
        || !matches!(m.label.as_str(), "kernel-aux-v32" | "pending-payouts-v32")
        || m.rows.len() > 65536
        || m.state.bytes > 4 << 20
    {
        return Err("receipt CAS exact proof label/shape".into());
    }
    let pool = relative(evidence_root, &m.objects)?;
    if !pool.is_dir() {
        return Err("receipt CAS pool unavailable".into());
    }
    let mut extent = m.state.bytes as usize;
    for r in &m.rows {
        if r.key.len() % 2 != 0 || r.key.len() / 2 > 4101 || r.value.bytes > 32 << 20 {
            return Err("receipt CAS original row bound".into());
        }
        extent = extent
            .checked_add(r.key.len() / 2)
            .and_then(|n| n.checked_add(r.value.bytes as usize))
            .filter(|n| *n <= 64 << 20)
            .ok_or("receipt CAS original full proof extent exceeded")?;
    }
    let preimage = read_object(&pool, &m.state, 4 << 20)?;
    let mut rows = Vec::with_capacity(m.rows.len());
    for r in m.rows {
        rows.push((hexbytes(&r.key, 4101)?, read_object(&pool, &r.value, 32 << 20)?));
    }
    Ok(PalwFactProofV1 {
        state: PalwStateOpeningV1 { preimage },
        collection: PalwCollectionOpeningV1 { label: m.label.into_bytes(), rows },
    })
}
