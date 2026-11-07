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

/// Complete39 root authority: a streamed original aux image AND its exact
/// pinned child/selected-parent state envelope under the auditor's own Params.
/// This is not a native execution, payout, chain-selection or A6 authority.
pub struct AnchoredNativeAuxStreamV39 {
    anchor: kaspa_consensus_core::palw_native_state_anchor_v39::CheckedNativeStateAnchorV39,
    auxiliary: VerifiedNativeAuxRowsV39<NativeCasAuxReaderV39>,
}
impl AnchoredNativeAuxStreamV39 {
    pub fn anchor(&self) -> &kaspa_consensus_core::palw_native_state_anchor_v39::CheckedNativeStateAnchorV39 {
        &self.anchor
    }
    pub fn auxiliary(&self) -> &VerifiedNativeAuxRowsV39<NativeCasAuxReaderV39> {
        &self.auxiliary
    }
}
pub fn read_native_aux_stream_anchored_v39(
    evidence_root: &Path,
    index_path: &Path,
    child_header_path: &Path,
    parent_header_path: &Path,
    envelope_path: &Path,
    pinned_child: Hash64,
    pinned_parent: Hash64,
    parameters: &kaspa_consensus_core::config::params::Params,
) -> Result<AnchoredNativeAuxStreamV39, String> {
    let root = evidence_root.canonicalize().map_err(|e| e.to_string())?;
    let bounded = |path: &Path, cap: usize| -> Result<Vec<u8>, String> {
        let real = path.canonicalize().map_err(|e| e.to_string())?;
        if !real.starts_with(&root) {
            return Err("audit39 original anchor path escapes evidence root".into());
        }
        crate::native_audit_io_v32::read_bounded_file_v32(&real, cap)
    };
    let child: kaspa_consensus_core::header::Header =
        serde_json::from_slice(&bounded(child_header_path, 4 << 20)?).map_err(|e| e.to_string())?;
    let parent: kaspa_consensus_core::header::Header =
        serde_json::from_slice(&bounded(parent_header_path, 4 << 20)?).map_err(|e| e.to_string())?;
    let envelope =
        kaspa_consensus_core::palw_native_state_anchor_v39::NativeStateEnvelopeV39::from_bytes(&bounded(envelope_path, 1030)?)?;
    let index = index_path.canonicalize().map_err(|e| e.to_string())?;
    if !index.starts_with(&root) {
        return Err("audit39 original anchor index escapes evidence root".into());
    }
    let mut file = BufReader::new(std::fs::File::open(&index).map_err(|e| e.to_string())?);
    let first = line(&mut file, 4096)?.ok_or("audit39 original state index missing")?;
    let header: AuxStreamHeaderV39 = serde_json::from_slice(&first).map_err(|e| e.to_string())?;
    if header.schema != "misaka.palw.native-aux-stream.v39" {
        return Err("audit39 original owning index schema".into());
    }
    let pool = relative(&root, &header.objects)?;
    let state = PalwStateOpeningV1 { preimage: object(&pool, &header.state, 4 << 20)? };
    let anchor = kaspa_consensus_core::palw_native_state_anchor_v39::check_native_state_anchor_v39(
        &child,
        pinned_child,
        &parent,
        pinned_parent,
        parameters,
        &state,
        &envelope,
    )?;
    let auxiliary = read_native_aux_stream_v39(&root, &index, anchor.inner())?;
    if auxiliary.state_root() != anchor.inner() {
        return Err("audit39 original envelope and full aux anchors differ".into());
    }
    Ok(AnchoredNativeAuxStreamV39 { anchor, auxiliary })
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayoutStreamRowV39 {
    pub key: String,
    pub value: CasObjectV39,
}
/// The common pending-payout collection is data identity only. Its original32
/// format/caps are unchanged; wrapping it under a39 state anchor grants none of
/// the old32 execution or Final authorities.
pub struct AnchoredNativePayoutStreamV39 {
    proof: kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1,
    inner: Hash64,
}
impl AnchoredNativePayoutStreamV39 {
    pub fn state_root(&self) -> Hash64 {
        self.inner
    }
    pub fn verified_rows(&self) -> Result<kaspa_consensus_core::palw_native_audit_proof_v32::VerifiedNativePayoutRowsV32<'_>, String> {
        kaspa_consensus_core::palw_native_audit_proof_v32::verify_native_payout_rows_v32(&self.proof, self.inner)
    }
}
pub fn read_native_payout_stream_anchored_v39(
    evidence_root: &Path,
    index_path: &Path,
    original: &AnchoredNativeAuxStreamV39,
) -> Result<AnchoredNativePayoutStreamV39, String> {
    use kaspa_consensus_core::{
        palw_native_audit_proof_v32 as common,
        palw_state_proof_v1::{PalwCollectionOpeningV1, PalwFactProofV1},
    };
    let root = evidence_root.canonicalize().map_err(|e| e.to_string())?;
    let index = index_path.canonicalize().map_err(|e| e.to_string())?;
    if !index.starts_with(&root) {
        return Err("audit39 original payout index escapes evidence root".into());
    }
    let mut file = BufReader::new(std::fs::File::open(index).map_err(|e| e.to_string())?);
    let header: AuxStreamHeaderV39 =
        serde_json::from_slice(&line(&mut file, 4096)?.ok_or("audit39 original payout header missing")?).map_err(|e| e.to_string())?;
    if header.schema != "misaka.palw.native-payout-stream.v39" || header.total_rows > common::NATIVE_AUX_PROOF_MAX_ROWS_V32 {
        return Err("audit39 original common payout schema/unchanged row cap".into());
    }
    let pool = relative(&root, &header.objects)?;
    let state = PalwStateOpeningV1 { preimage: object(&pool, &header.state, 4 << 20)? };
    if state.root() != original.anchor().inner() {
        return Err("audit39 payouts not same original39 pinned state".into());
    }
    let mut rows = Vec::new();
    let mut bytes = state.preimage.len();
    let mut last = None;
    while let Some(raw) = line(&mut file, 4096)? {
        if rows.len() >= header.total_rows {
            return Err("audit39 extra original payout rows".into());
        }
        let row: PayoutStreamRowV39 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
        let key = hex(&row.key)?;
        if key.len() != 64 || row.value.bytes != 72 || last.as_ref().is_some_and(|p| p >= &key) {
            return Err("audit39 common payout exact original key/value/order".into());
        }
        let value = object(&pool, &row.value, 72)?;
        bytes = bytes
            .checked_add(136)
            .filter(|n| *n <= common::NATIVE_AUX_PROOF_MAX_BYTES_V32)
            .ok_or("audit39 common payout unchanged byte cap")?;
        last = Some(key.clone());
        rows.push((key, value));
    }
    if rows.len() != header.total_rows {
        return Err("audit39 original payout collection truncated".into());
    }
    let proof =
        PalwFactProofV1 { state, collection: PalwCollectionOpeningV1 { label: common::NATIVE_PAYOUT_LABEL_V32.to_vec(), rows } };
    common::verify_native_payout_rows_v32(&proof, original.anchor().inner())?;
    Ok(AnchoredNativePayoutStreamV39 { proof, inner: original.anchor().inner() })
}
