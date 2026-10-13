//! Source witness25 over the ORIGINAL real-PC and external R inventory.
//! Enlarging the paid filing envelope does not replace PC's original tiled
//! row/column framing, omit a parameter namespace, or shrink source context.
use crate::palw_model_artifact_v2::decode_model_artifact_proof_v2;
use crate::palw_onboarding_v1::ArtifactMismatchProofV1;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    hash::{Digest, object_id},
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::program::TirProgramV1;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeSourceProofBoundV25 {
    pub version: u16,
    pub program: Digest,
    pub real_parameter: Digest,
    pub pc_instances: u32,
    pub maximum_bytes: u32,
    pub work: u64,
    pub scratch: u64,
}
impl NativeSourceProofBoundV25 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-source-full-court-bound/v25", self)
    }
}
pub fn derive_native_source_proof_bound_v25(
    program: &TirProgramV1,
    pc: &ParamCommitmentsV1,
) -> Result<NativeSourceProofBoundV25, String> {
    // Validate ALL original real names/layers; a cheap caller count cannot
    // reduce the proof-map or source restoration obligation.
    let expected = program
        .params
        .iter()
        .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
        .ok_or("source25 original real-PC instance count overflow")?;
    if expected != pc.by_instance.len() {
        return Err("source25 original real-PC namespace count differs".into());
    }
    for (j, p) in program.params.iter().enumerate() {
        if p.per_layer {
            for layer in 0..program.schedule.layers.len() {
                if !pc.by_instance.contains_key(&(j as u16, Some(layer as u16))) {
                    return Err("source25 original layer missing".into());
                }
            }
        } else if !pc.by_instance.contains_key(&(j as u16, None)) {
            return Err("source25 original parameter missing".into());
        }
    }
    let pc_bytes = borsh::to_vec(pc).map_err(|e| e.to_string())?.len();
    let maximum_bytes = pc_bytes
        .checked_add(128 << 10)
        .and_then(|n| n.checked_add(7))
        .filter(|n| *n <= 16 << 20)
        .ok_or("source25 exact PC-map plus original4096-element tile exceeds16MiB")?;
    let work = crate::palw_model_artifact_v2::model_artifact_work_v2(program.encode().len(), maximum_bytes)
        .ok_or("source25 full metadata tariff overflow")?;
    let scratch = (maximum_bytes as u64)
        .checked_mul(4)
        .and_then(|n| n.checked_add((program.encode().len() as u64) * 8))
        .and_then(|n| n.checked_add(8 << 20))
        .ok_or("source25 complete decoder/verifier scratch overflow")?;
    Ok(NativeSourceProofBoundV25 {
        version: 25,
        program: misaka_palw_kernel::public::program_root_v1(&program.encode()),
        real_parameter: pc.root(),
        pc_instances: u32::try_from(expected).map_err(|_| "source25 count overflow")?,
        maximum_bytes: maximum_bytes as u32,
        work,
        scratch,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowedNativeFilingV25<'a> {
    pub kind: u8,
    pub body: &'a [u8],
}
pub fn borrowed_native_filing_v25(raw: &[u8], original_maximum_bytes: u32) -> Option<BorrowedNativeFilingV25<'_>> {
    if raw.len() < 7 || raw.len() > original_maximum_bytes as usize || raw[..2] != 25u16.to_le_bytes() || raw[2] > 2 {
        return None;
    }
    let n = u32::from_le_bytes(raw[3..7].try_into().ok()?) as usize;
    if n != raw.len() - 7 {
        return None;
    }
    Some(BorrowedNativeFilingV25 { kind: raw[2], body: &raw[7..] })
}
pub fn decode_native_source_fault_v25(raw: &[u8], original: &NativeSourceProofBoundV25) -> Result<ArtifactMismatchProofV1, String> {
    if original.version != 25 || original.maximum_bytes == 0 || original.maximum_bytes > 16 << 20 || original.pc_instances == 0 {
        return Err("source25 lacks ORIGINAL native checked paid bound".into());
    }
    let f = borrowed_native_filing_v25(raw, original.maximum_bytes)
        .filter(|f| f.kind == 1)
        .ok_or("source25 new bounded source envelope grammar")?;
    // The original bounded tile/R opening grammar is deliberately reused. It
    // opens original leaves up to4096 elements; no64-value prefix is relabeled
    // as an authenticated original PC tile.
    decode_model_artifact_proof_v2(f.body, original.pc_instances, original.maximum_bytes - 7).map_err(|e| e.to_string())
}
