//! Readonly full-size public Master32 output witnesses. The frozen Core decoder
//! cap stays unchanged; descriptor preflight bounds i128 expansion BEFORE any
//! Tensor allocation. A SHA cache shares identical public outputs across jobs.
use misaka_palw_tir::{DType, Tensor};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, path::Path, sync::Arc};
pub const MASTER_WITNESS_TYPED_LIMIT_V1: usize = 2usize << 30;
pub const MASTER_WITNESS_PEAK_LIMIT_V1: usize = (2usize << 30) + (256 << 20);
#[derive(Clone)]
pub struct CheckedDecodedMasterOutputsV1 {
    values: Arc<Vec<Vec<Tensor>>>,
    certificates: Option<Arc<Vec<Vec<misaka_palw_kernel::seg::NodeOpeningV1>>>>,
    typed_bytes: usize,
    raw_bytes: usize,
    sha256: [u8; 32],
}
impl CheckedDecodedMasterOutputsV1 {
    pub fn values(&self) -> &[Vec<Tensor>] {
        self.values.as_slice()
    }
    pub fn shared_values(&self) -> Arc<Vec<Vec<Tensor>>> {
        self.values.clone()
    }
    pub fn shared_certificates(&self) -> Option<Arc<Vec<Vec<misaka_palw_kernel::seg::NodeOpeningV1>>>> {
        self.certificates.clone()
    }
    pub fn typed_bytes(&self) -> usize {
        self.typed_bytes
    }
    pub fn raw_bytes(&self) -> usize {
        self.raw_bytes
    }
    pub fn sha256(&self) -> [u8; 32] {
        self.sha256
    }
}
fn take<'a>(raw: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], String> {
    let end = at.checked_add(n).ok_or("master witness cursor overflow")?;
    let v = raw.get(*at..end).ok_or("master witness truncated descriptor/body")?;
    *at = end;
    Ok(v)
}
fn number(raw: &[u8], at: &mut usize) -> Result<usize, String> {
    Ok(u32::from_le_bytes(take(raw, at, 4)?.try_into().unwrap()) as usize)
}
/// Includes Tensor/Vec/shape ownership, not merely the serialized raw bytes.
pub fn preflight_master_outputs_v1(raw: &[u8]) -> Result<usize, String> {
    use kaspa_consensus_core::palw_native_audit_proof_v32::NATIVE_MASTER_OUTPUTS_MAX_BYTES_V32;
    if raw.len() > NATIVE_MASTER_OUTPUTS_MAX_BYTES_V32 {
        return Err("master witness frozen raw bound exceeded".into());
    }
    let mut at = 0usize;
    let stages = number(raw, &mut at)?;
    if stages == 0 || stages > 64 {
        return Err("master witness stage count".into());
    }
    let mut typed = stages.checked_mul(std::mem::size_of::<Vec<Tensor>>()).ok_or("master witness vector overflow")?;
    for _ in 0..stages {
        let count = number(raw, &mut at)?;
        if count > 65536 || count > raw.len().saturating_sub(at) / 6 {
            return Err("master witness tensor count".into());
        }
        typed = typed
            .checked_add(count.checked_mul(std::mem::size_of::<Tensor>()).ok_or("master witness tensor metadata overflow")?)
            .ok_or("master witness metadata overflow")?;
        for _ in 0..count {
            let tag = take(raw, &mut at, 1)?[0];
            let dtype = *DType::ALL.get(tag as usize).ok_or("master witness dtype")?;
            let rank = take(raw, &mut at, 1)?[0] as usize;
            if rank > 8 {
                return Err("master witness rank".into());
            }
            let mut elements = 1usize;
            for _ in 0..rank {
                let dim = number(raw, &mut at)?;
                if dim == 0 {
                    return Err("master witness zero dimension".into());
                }
                elements = elements.checked_mul(dim).ok_or("master witness shape product overflow")?;
            }
            if elements as u64 > misaka_palw_tir::types::MAX_ELEMENTS {
                return Err("master witness frozen tensor extent exceeded".into());
            }
            let bytes = number(raw, &mut at)?;
            if elements.checked_mul(dtype.width()) != Some(bytes) {
                return Err("master witness exact dtype/shape/body extent differs".into());
            }
            typed = typed
                .checked_add(elements.checked_mul(std::mem::size_of::<i128>()).ok_or("master witness i128 expansion overflow")?)
                .and_then(|n| n.checked_add(rank * std::mem::size_of::<usize>()))
                .ok_or("master witness typed extent overflow")?;
            if typed > MASTER_WITNESS_TYPED_LIMIT_V1 || typed.checked_add(raw.len()).is_none_or(|n| n > MASTER_WITNESS_PEAK_LIMIT_V1) {
                return Err("master witness bounded typed/raw peak exceeded before allocation".into());
            }
            take(raw, &mut at, bytes)?;
        }
    }
    if at != raw.len() {
        return Err("master witness trailing bytes".into());
    }
    Ok(typed)
}
#[derive(Default)]
pub struct NativeMasterWitnessCacheV1 {
    objects: BTreeMap<[u8; 32], CheckedDecodedMasterOutputsV1>,
    typed: usize,
}
impl NativeMasterWitnessCacheV1 {
    pub fn load(&mut self, path: &Path) -> Result<CheckedDecodedMasterOutputsV1, String> {
        self.load_inner(path, false)
    }
    pub fn load_transient(&mut self, path: &Path) -> Result<CheckedDecodedMasterOutputsV1, String> {
        self.load_inner(path, true)
    }
    fn load_inner(&mut self, path: &Path, evict_unused: bool) -> Result<CheckedDecodedMasterOutputsV1, String> {
        let raw = crate::native_audit_io_v32::read_bounded_file_v32(
            path,
            kaspa_consensus_core::palw_native_audit_proof_v32::NATIVE_MASTER_OUTPUTS_MAX_BYTES_V32,
        )?;
        let digest: [u8; 32] = Sha256::digest(&raw).into();
        if let Some(existing) = self.objects.get(&digest) {
            return Ok(existing.clone());
        }
        if evict_unused {
            self.objects.retain(|_, object| Arc::strong_count(&object.values) > 1);
            self.typed = self.objects.values().map(|v| v.typed_bytes).sum();
        }
        let certified = raw.starts_with(&kaspa_consensus_core::palw_flat_stage_v28::NativeMasterOutputCertificatesV32::MAGIC);
        let typed = if certified {
            let c = kaspa_consensus_core::palw_flat_stage_v28::NativeMasterOutputCertificatesV32::from_bytes(&raw)?;
            preflight_master_outputs_v1(&c.values)?
                .checked_add(raw.len().checked_mul(4).ok_or("master certificate metadata expansion overflow")?)
                .ok_or("master certificate typed sum overflow")?
        } else {
            preflight_master_outputs_v1(&raw)?
        };
        let next = self
            .typed
            .checked_add(typed)
            .filter(|n| *n <= MASTER_WITNESS_TYPED_LIMIT_V1)
            .ok_or("master witness aggregate typed reservation exceeded before decode")?;
        if next.checked_add(raw.len()).is_none_or(|n| n > MASTER_WITNESS_PEAK_LIMIT_V1) {
            return Err("master witness aggregate decode peak exceeded".into());
        }
        let (values, certificates) = kaspa_consensus_core::palw_flat_stage_v28::decode_native_master_certificates_v32(&raw)?;
        let checked = CheckedDecodedMasterOutputsV1 {
            values: Arc::new(values),
            certificates: certificates.map(Arc::new),
            typed_bytes: typed,
            raw_bytes: raw.len(),
            sha256: digest,
        };
        self.typed = next;
        self.objects.insert(digest, checked.clone());
        Ok(checked)
    }
    pub fn retained_typed_bytes(&self) -> usize {
        self.typed
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tensor(tag: u8, shape: &[u32], bytes: u32) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend(1u32.to_le_bytes());
        raw.extend(1u32.to_le_bytes());
        raw.push(tag);
        raw.push(shape.len() as u8);
        for d in shape {
            raw.extend(d.to_le_bytes());
        }
        raw.extend(bytes.to_le_bytes());
        raw.resize(raw.len() + bytes as usize, 0);
        raw
    }
    #[test]
    fn descriptors_bound_exact_typed_expansion_before_decode() {
        let raw = tensor(DType::I32.tag(), &[4, 8], 128);
        assert!(preflight_master_outputs_v1(&raw).unwrap() >= 32 * 16);
        assert!(preflight_master_outputs_v1(&tensor(DType::I32.tag(), &[4, 8], 127)).is_err());
        assert!(preflight_master_outputs_v1(&tensor(DType::I8.tag(), &[0], 0)).is_err());
        let mut trailing = raw;
        trailing.push(0);
        assert!(preflight_master_outputs_v1(&trailing).is_err());
    }
}
