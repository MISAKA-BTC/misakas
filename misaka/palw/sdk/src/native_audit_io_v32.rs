//! Bounded offline inputs for an independent native32 auditor. These wire
//! values are untrusted until the pinned-header and original-row verifier runs.
use kaspa_consensus_core::{
    header::Header,
    palw_native_audit_v27::NativeExecutionAuditRecordV27,
    palw_state_proof_v1::{PalwCollectionOpeningV1, PalwFactProofV1, PalwStateOpeningV1},
};
use std::{io::Read, path::Path};
const MAX_PROOF_BYTES: usize = 64 << 20;
pub fn read_bounded_file_v32(path: &Path, cap: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > cap as u64 {
        return Err("native32 input file exceeds bounded extent".into());
    }
    let mut raw = Vec::new();
    raw.try_reserve_exact(size as usize).map_err(|_| "native32 input allocation")?;
    file.take(cap as u64 + 1).read_to_end(&mut raw).map_err(|e| e.to_string())?;
    if raw.len() > cap {
        return Err("native32 input grew past bounded extent".into());
    }
    Ok(raw)
}
pub fn read_header_v32(path: &Path) -> Result<Header, String> {
    // JSON has no attacker-controlled encoded Vec capacity; the total bytes
    // cap bounds its concrete arrays before deserialization. Unknown nesting
    // retains serde_json's default recursion limit.
    let raw = read_bounded_file_v32(path, 64 << 10)?;
    let mut header: Header = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let runs: Vec<(u8, Vec<kaspa_consensus_core::Hash64>)> =
        borsh::from_slice(&borsh::to_vec(&header.parents_by_level).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    // Derive-based JSON deserialization bypasses the canonical run constructor.
    // Revalidate before hashing can expand a hostile zero/nonincreasing run.
    header.parents_by_level = runs.try_into().map_err(|e| format!("native32 parent runs: {e}"))?;
    if header.parents_by_level.expanded_len() > 255
        || header.palw_commitment.len() > 32 << 10
        || header.parents_by_level.get(0).is_some_and(|v| v.len() > 1024)
    {
        return Err("native32 header concrete arrays exceed profile".into());
    }
    Ok(header)
}
pub fn read_native_record_v32(path: &Path) -> Result<NativeExecutionAuditRecordV27, String> {
    NativeExecutionAuditRecordV27::from_bytes(&read_bounded_file_v32(path, (2 << 20) + (64 << 10))?)
}
/// Proof wire32 is [u16=32, Vec(state preimage), Vec(exact new label),
/// u32 rowcount, repeated Vec(key)/Vec(value)]. Prefixes and aggregate extent
/// are checked over borrowed slices before reserving any row or payload Vec.
pub fn encode_native_fact_proof_v32(proof: &PalwFactProofV1) -> Result<Vec<u8>, String> {
    use kaspa_consensus_core::palw_native_audit_proof_v32::{NATIVE_AUX_LABEL_V32, NATIVE_PAYOUT_LABEL_V32};
    if proof.state.preimage.len() > 4 << 20
        || proof.collection.rows.len() > 65536
        || ![NATIVE_AUX_LABEL_V32, NATIVE_PAYOUT_LABEL_V32].contains(&proof.collection.label.as_slice())
    {
        return Err("native32 original proof collection/extent".into());
    }
    let total = proof
        .collection
        .rows
        .iter()
        .try_fold(14usize + proof.state.preimage.len() + proof.collection.label.len(), |n, (k, v)| {
            if k.len() > 4101 || v.len() > 32 << 20 {
                return None;
            }
            n.checked_add(8)?.checked_add(k.len())?.checked_add(v.len())
        })
        .ok_or("native32 original proof aggregate overflow")?;
    if total > MAX_PROOF_BYTES {
        return Err("native32 original proof aggregate cap".into());
    }
    borsh::to_vec(&(32u16, &proof.state.preimage, &proof.collection.label, &proof.collection.rows)).map_err(|e| e.to_string())
}
pub fn decode_native_fact_proof_v32(raw: &[u8]) -> Result<PalwFactProofV1, String> {
    use kaspa_consensus_core::palw_native_audit_proof_v32::{NATIVE_AUX_LABEL_V32, NATIVE_PAYOUT_LABEL_V32};
    fn vector<'a>(raw: &'a [u8], at: &mut usize, cap: usize) -> Result<&'a [u8], String> {
        let n = u32::from_le_bytes(
            raw.get(*at..at.checked_add(4).ok_or("native32 offset")?).ok_or("native32 truncated count")?.try_into().unwrap(),
        ) as usize;
        *at = at.checked_add(4).ok_or("native32 offset")?;
        if n > cap {
            return Err("native32 nested vector count cap".into());
        }
        let end = at.checked_add(n).ok_or("native32 extent overflow")?;
        let value = raw.get(*at..end).ok_or("native32 truncated nested vector")?;
        *at = end;
        Ok(value)
    }
    if raw.len() > MAX_PROOF_BYTES || raw.get(..2) != Some(32u16.to_le_bytes().as_slice()) {
        return Err("native32 proof version/aggregate cap".into());
    }
    let mut at = 2;
    let state = vector(raw, &mut at, 4 << 20)?;
    let label = vector(raw, &mut at, 32)?;
    if label != NATIVE_AUX_LABEL_V32 && label != NATIVE_PAYOUT_LABEL_V32 {
        return Err("native32 exact proof label".into());
    }
    let n = u32::from_le_bytes(raw.get(at..at + 4).ok_or("native32 row count absent")?.try_into().unwrap()) as usize;
    at += 4;
    if n > 65536 || n > raw.len().saturating_sub(at) / 8 {
        return Err("native32 row count exceeds available wire".into());
    }
    let start = at;
    // Full structural pass before allocations; a late malformed count cannot
    // first force copies of 64MiB or reserve millions of empty rows.
    for _ in 0..n {
        vector(raw, &mut at, 4101)?;
        vector(raw, &mut at, 32 << 20)?;
    }
    if at != raw.len() {
        return Err("native32 proof trailing bytes".into());
    }
    at = start;
    let mut rows = Vec::new();
    rows.try_reserve_exact(n).map_err(|_| "native32 proof row allocation")?;
    for _ in 0..n {
        rows.push((vector(raw, &mut at, 4101)?.to_vec(), vector(raw, &mut at, 32 << 20)?.to_vec()));
    }
    Ok(PalwFactProofV1 {
        state: PalwStateOpeningV1 { preimage: state.to_vec() },
        collection: PalwCollectionOpeningV1 { label: label.to_vec(), rows },
    })
}
pub fn read_native_fact_proof_v32(path: &Path) -> Result<PalwFactProofV1, String> {
    decode_native_fact_proof_v32(&read_bounded_file_v32(path, MAX_PROOF_BYTES)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn proof_wire_rejects_hostile_counts_and_frozen_labels_before_rows() {
        let proof = PalwFactProofV1 {
            state: PalwStateOpeningV1 { preimage: vec![2, 0] },
            collection: PalwCollectionOpeningV1 {
                label: kaspa_consensus_core::palw_native_audit_proof_v32::NATIVE_AUX_LABEL_V32.to_vec(),
                rows: vec![(borsh::to_vec(&(7u8, vec![1u8])).unwrap(), vec![2; 32])],
            },
        };
        let raw = encode_native_fact_proof_v32(&proof).unwrap();
        assert_eq!(decode_native_fact_proof_v32(&raw).unwrap(), proof);
        let mut hostile = raw.clone();
        let count_at = 2 + 4 + proof.state.preimage.len() + 4 + proof.collection.label.len();
        hostile[count_at..count_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_native_fact_proof_v32(&hostile).is_err());
        let mut hostile = raw;
        hostile[count_at + 4..count_at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_native_fact_proof_v32(&hostile).is_err());
        let old =
            PalwFactProofV1 { state: proof.state, collection: PalwCollectionOpeningV1 { label: b"claims".to_vec(), rows: vec![] } };
        assert!(encode_native_fact_proof_v32(&old).is_err());
    }
}
