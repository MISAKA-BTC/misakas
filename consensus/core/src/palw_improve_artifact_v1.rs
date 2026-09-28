//! **RFC-0004 §6.3: a candidate's artifact reference and the composite artifact root.**
//!
//! Skeleton (step 0) of the candidates lane's module (`rfc4/cand` owns it from the skeleton's sha on):
//! the reference type and the root formula every lane shares. The composite layout, the sub-root
//! openings, param authentication, the composite and family rules and the adapter-section inventory
//! are the lane's (`palw_improve_composite_v1`).

use crate::Hash64;
use borsh::{BorshDeserialize, BorshSerialize};

/// Key of [`palw_improve_composite_artifact_root_v1`].
pub const PALW_IMPROVE_COMPOSITE_ARTIFACT_DOMAIN_V1: &[u8] = b"misaka-palw/improve/composite-artifact/v1";

/// **A candidate's artifact** (RFC-0004 §6.1, §6.3): full weights, or the parent plus an adapter
/// section whose first `p` params are the parent's, byte for byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwTirArtifactRefV1 {
    Single { root: Hash64 },
    Composite { parent_class: Hash64, parent_root: Hash64, adapter_root: Hash64, p: u32 },
}

impl PalwTirArtifactRefV1 {
    /// The artifact root Phase F's class id hashes (`tir_class_id_v1`): the single root, or the
    /// composite root.
    pub fn artifact_root(&self) -> Hash64 {
        match self {
            PalwTirArtifactRefV1::Single { root } => *root,
            PalwTirArtifactRefV1::Composite { parent_class, parent_root, adapter_root, p } => {
                palw_improve_composite_artifact_root_v1(parent_class, parent_root, adapter_root, *p)
            }
        }
    }
}

/// **The composite artifact root** (RFC-0004 §6.3): BLAKE2b-512 keyed by
/// `misaka-palw/improve/composite-artifact/v1` over `parent_class ‖ parent_root ‖ adapter_root ‖
/// le32(p)`, with no version prefix. It binds the parent class, the parent's artifact root, the
/// adapter section's root and `P`, so the parent's tensors are never re-committed.
pub fn palw_improve_composite_artifact_root_v1(parent_class: &Hash64, parent_root: &Hash64, adapter_root: &Hash64, p: u32) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(PALW_IMPROVE_COMPOSITE_ARTIFACT_DOMAIN_V1).to_state();
    state.update(parent_class.as_byte_slice());
    state.update(parent_root.as_byte_slice());
    state.update(adapter_root.as_byte_slice());
    state.update(&p.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_composite_root_binds_every_part() {
        let h = |b| Hash64::from_bytes([b; 64]);
        let base = palw_improve_composite_artifact_root_v1(&h(1), &h(2), &h(3), 10);
        assert_ne!(base, palw_improve_composite_artifact_root_v1(&h(9), &h(2), &h(3), 10));
        assert_ne!(base, palw_improve_composite_artifact_root_v1(&h(1), &h(9), &h(3), 10));
        assert_ne!(base, palw_improve_composite_artifact_root_v1(&h(1), &h(2), &h(9), 10));
        assert_ne!(base, palw_improve_composite_artifact_root_v1(&h(1), &h(2), &h(3), 11));
        let composite = PalwTirArtifactRefV1::Composite { parent_class: h(1), parent_root: h(2), adapter_root: h(3), p: 10 };
        assert_eq!(composite.artifact_root(), base);
        assert_eq!(PalwTirArtifactRefV1::Single { root: h(4) }.artifact_root(), h(4));
    }
}
