//! **Deterministic Kimi K3 artifact identity.**
//!
//! Same source bytes, same geometry, same tokenizer → same artifact root. The converter never
//! consults host endianness beyond little-endian integers, never writes timestamps, and hashes
//! tensors in **lexicographic name order**. A second conversion that disagrees is a different
//! artifact, not a "close enough" one.

use crate::Hash64;
use crate::palw_kimi_k3_profile::PalwKimiK3GeometryV1;
use crate::palw_kimi_k3_tokenizer_v1::KimiK3TokenizerSpecV1;
use borsh::BorshSerialize;

pub const KIMI_K3_ARTIFACT_DOMAIN: &[u8] = b"misaka-palw/kimi-k3-artifact/v1";
pub const KIMI_K3_ARTIFACT_MAGIC: &[u8; 8] = b"PALWK3V1";

/// What a converter commits before hashing weights.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwKimiK3ArtifactHeaderV1 {
    pub magic: [u8; 8],
    pub geometry: PalwKimiK3GeometryV1,
    pub tokenizer_id: Hash64,
    /// Blake2b-64 of the weight stream (int8 rows, names sorted).
    pub weight_root: Hash64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwKimiK3ArtifactError {
    EmptyWeight,
    GeometryRefused,
}

/// Root of one converted artifact. Pure function of the header's canonical Borsh bytes.
pub fn kimi_k3_artifact_root_v1(header: &PalwKimiK3ArtifactHeaderV1) -> Result<Hash64, PalwKimiK3ArtifactError> {
    if header.magic != *KIMI_K3_ARTIFACT_MAGIC {
        return Err(PalwKimiK3ArtifactError::GeometryRefused);
    }
    let bytes = borsh::to_vec(header).map_err(|_| PalwKimiK3ArtifactError::GeometryRefused)?;
    let mut h = blake2b_simd::Params::new().hash_length(64).key(KIMI_K3_ARTIFACT_DOMAIN).to_state();
    h.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

/// Hash a weight stream. `tensors` must already be sorted by name; unsorted input is refused so
/// two converters cannot "agree" after a silent reorder.
pub fn kimi_k3_weight_root_v1(tensors: &[(String, Vec<u8>)]) -> Result<Hash64, PalwKimiK3ArtifactError> {
    if tensors.is_empty() {
        return Err(PalwKimiK3ArtifactError::EmptyWeight);
    }
    for pair in tensors.windows(2) {
        if pair[0].0 >= pair[1].0 {
            return Err(PalwKimiK3ArtifactError::EmptyWeight);
        }
    }
    let mut h = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kimi-k3-weights/v1").to_state();
    for (name, bytes) in tensors {
        h.update(&(name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        h.update(&(bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Ok(Hash64::from_bytes(out))
}

/// Convert named tensors + geometry + tokenizer into a root. The function a second host repeats.
pub fn kimi_k3_convert_artifact_v1(
    geometry: PalwKimiK3GeometryV1,
    tokenizer: &KimiK3TokenizerSpecV1,
    mut tensors: Vec<(String, Vec<u8>)>,
) -> Result<Hash64, PalwKimiK3ArtifactError> {
    tensors.sort_by(|a, b| a.0.cmp(&b.0));
    tensors.dedup_by(|a, b| a.0 == b.0);
    let weight_root = kimi_k3_weight_root_v1(&tensors)?;
    let tokenizer_id =
        crate::palw_kimi_k3_tokenizer_v1::kimi_k3_tokenizer_id_v1(tokenizer).map_err(|_| PalwKimiK3ArtifactError::GeometryRefused)?;
    kimi_k3_artifact_root_v1(&PalwKimiK3ArtifactHeaderV1 { magic: *KIMI_K3_ARTIFACT_MAGIC, geometry, tokenizer_id, weight_root })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_kimi_k3_profile::KIMI_K3_CARD;
    use crate::palw_kimi_k3_tokenizer_v1::KimiK3TokenizerSpecV1;

    #[test]
    fn the_same_source_is_the_same_root() {
        let tensors = vec![("a".into(), vec![1, 2, 3]), ("b".into(), vec![4])];
        let r1 = kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, tensors.clone()).unwrap();
        let r2 = kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, tensors).unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn tensor_order_does_not_move_the_root() {
        let a = vec![("b".into(), vec![4]), ("a".into(), vec![1, 2, 3])];
        let b = vec![("a".into(), vec![1, 2, 3]), ("b".into(), vec![4])];
        assert_eq!(
            kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, a).unwrap(),
            kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, b).unwrap()
        );
    }

    #[test]
    fn a_different_weight_is_a_different_artifact() {
        let a = vec![("a".into(), vec![1])];
        let b = vec![("a".into(), vec![2])];
        assert_ne!(
            kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, a).unwrap(),
            kimi_k3_convert_artifact_v1(KIMI_K3_CARD, &KimiK3TokenizerSpecV1::CARD, b).unwrap()
        );
    }
}
