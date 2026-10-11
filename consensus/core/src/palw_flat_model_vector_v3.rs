//! Explicit flat-v5 candidate statements and immutable admitted resource scopes.
//! Non-reward, separate from frozen v2 vectors. Public hash-unit DA and execution
//! registration/Final remain separate work; this is not conformance certification.
use crate::palw_model_vector_v2::{
    MODEL_VECTORS_PER_BOND_V2, ModelVectorPostV2, checked_vector_view_with_trace_root, decode_model_vector_post_v2,
};
use crate::{Hash64, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{flat_court5::FlatSegContextV5, flat_scope5::FlatResourceScopeV5, hash::Digest, seg_ledger::SegClaimViewV1};
use std::io::{Cursor, Error, ErrorKind};
pub const FLAT_MODEL_VECTOR_TABLE_V3: u8 = 57;
pub const FLAT_MODEL_VECTOR_BOND_TABLE_V3: u8 = 58;
pub const FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3: usize = crate::palw_kernel_route_v1::PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 - 134;
pub const FLAT_MODEL_VECTOR_MAX_PROOF_BYTES_V3: usize = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatModelVectorPostV3 {
    pub version: u16,
    pub profile: Digest,
    pub scope_root: Digest,
    /// Frozen typed job/evidence carrier, internally version 2, under an explicit
    /// new commitment profile. Its grammar and interpretation are unchanged for v2.
    pub trace: ModelVectorPostV2,
}
impl FlatModelVectorPostV3 {
    pub fn trace_root(&self, class: Digest, binding: Digest) -> Digest {
        misaka_palw_kernel::hash::object_id(
            b"misaka-palw/flat-model-vector/typed-trace/v3",
            &(self.version, self.profile, self.scope_root, class, binding, self.trace.trace_root(class, binding)),
        )
    }
    pub fn id(&self, class: Digest, binding: Digest) -> Hash64 {
        vector_id(self.trace_root(class, binding))
    }
}
pub(crate) fn vector_id(root: Digest) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/flat-model-vector/statement/v3", &root))
}
pub fn decode_flat_model_vector_post_v3(bytes: &[u8]) -> std::io::Result<FlatModelVectorPostV3> {
    let bad = || Error::new(ErrorKind::InvalidData, "invalid bounded flat-model-vector v3 statement");
    if bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 || bytes.len() < 130 {
        return Err(bad());
    }
    let mut r = Cursor::new(bytes);
    let version = u16::deserialize_reader(&mut r)?;
    if version != 3 {
        return Err(bad());
    }
    let profile = Digest::deserialize_reader(&mut r)?;
    let scope_root = Digest::deserialize_reader(&mut r)?;
    let trace = decode_model_vector_post_v2(&bytes[130..])?;
    Ok(FlatModelVectorPostV3 { version, profile, scope_root, trace })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatModelVectorHeaderV3 {
    pub trace_root: Digest,
    pub class: Hash64,
    pub binding: Hash64,
    pub profile: Digest,
    pub scope_root: Digest,
    pub program_bytes_len: u32,
    pub pc_instances: u32,
    pub post_bytes_len: u32,
    pub max_filing_bytes: u64,
    pub max_court_work: u64,
    pub max_court_scratch_bytes: u64,
    pub poster: PalwBondKeyV2,
    pub posted_daa: u64,
    pub liability_until: u64,
    pub reserved: u64,
    pub refuted: bool,
}
impl FlatModelVectorHeaderV3 {
    pub fn id(&self) -> Hash64 {
        vector_id(self.trace_root)
    }
    pub fn reserved_at(&self, daa: u64) -> u64 {
        if self.refuted || daa >= self.liability_until { 0 } else { self.reserved }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatModelVectorRowV3 {
    pub header: FlatModelVectorHeaderV3,
    pub post: Vec<u8>,
}
pub struct FlatModelVectorViewV3 {
    pub profile: Digest,
    pub scope: FlatResourceScopeV5,
    pub trace: SegClaimViewV1,
}
impl FlatModelVectorViewV3 {
    pub fn context(&self) -> FlatSegContextV5<'_> {
        FlatSegContextV5 { profile: self.profile, claim: self.trace.context() }
    }
}
pub(crate) fn checked_flat_model_vector_with_root(
    route: &PalwKernelRouteStateV1,
    class: Hash64,
    binding: Hash64,
    post: &FlatModelVectorPostV3,
    root: Digest,
) -> Result<FlatModelVectorViewV3, String> {
    if post.version != 3 || post.profile != misaka_palw_kernel::flat_court5::flat_trace_profile_id_v5() {
        return Err("unknown flat vector grammar/profile".into());
    }
    let trace = checked_vector_view_with_trace_root(route, class, binding, &post.trace, root)?;
    let record = route.kernel_class_record_v1(&class).ok_or("flat candidate disappeared")?;
    let scope =
        misaka_palw_kernel::flat_scope5::flat_resource_scope_v5(&trace.program, record.plan.max_positions, trace.encoder.as_ref())
            .map_err(|e| format!("flat candidate resource scope refused: {e:?}"))?;
    if scope.root() != post.scope_root {
        return Err("flat resource declaration differs from independent admission".into());
    }
    Ok(FlatModelVectorViewV3 { profile: post.profile, scope, trace })
}
/// Structural tariff additionally covers the PC map read from the admitted class.
/// No variable stored-program/PC/post/fault decode may precede this reservation.
pub fn flat_model_vector_work_v3(program_bytes: u32, pc_instances: u32, bytes: usize) -> Option<u64> {
    crate::palw_model_artifact_v2::model_artifact_work_v2(
        program_bytes as usize,
        bytes.checked_add((pc_instances as usize).checked_mul(69)?)?,
    )
}
impl PalwKernelRouteStateV1 {
    pub fn flat_model_vector_header_v3(&self, id: &Hash64) -> Option<FlatModelVectorHeaderV3> {
        let bytes = self.aux.get(&(FLAT_MODEL_VECTOR_TABLE_V3, borsh::to_vec(id).ok()?))?;
        let h = FlatModelVectorHeaderV3::deserialize_reader(&mut Cursor::new(bytes)).ok()?;
        (h.id() == *id).then_some(h)
    }
    pub fn flat_model_vectors_of_v3(&self, bond: &PalwBondKeyV2) -> Vec<Hash64> {
        let ids: Vec<Hash64> = self.aux_row(FLAT_MODEL_VECTOR_BOND_TABLE_V3, &borsh::to_vec(bond).unwrap()).unwrap_or_default();
        if ids.len() > MODEL_VECTORS_PER_BOND_V2 { Vec::new() } else { ids }
    }
    pub fn flat_model_vector_reserved_at_v3(&self, bond: &PalwBondKeyV2, daa: u64) -> u128 {
        self.flat_model_vectors_of_v3(bond)
            .iter()
            .filter_map(|id| self.flat_model_vector_header_v3(id))
            .filter(|h| h.poster == *bond)
            .map(|h| {
                if self.flat_vector_unavailable_v3(&h.id()) {
                    0
                } else if !h.refuted && self.flat_vector_open_demands_v3(&h.id()) > 0 {
                    h.reserved as u128
                } else {
                    h.reserved_at(daa) as u128
                }
            })
            .sum()
    }
    pub fn flat_model_vector_row_v3(&self, id: &Hash64) -> Option<FlatModelVectorRowV3> {
        let row: FlatModelVectorRowV3 = self.aux_row(FLAT_MODEL_VECTOR_TABLE_V3, &borsh::to_vec(id).ok()?)?;
        (row.header.id() == *id).then_some(row)
    }
    /// Read-only v6 replay projection over the authenticated, unchanged v5 scope.
    /// This does not grant an execution Final or enforce aggregate network limits.
    pub fn flat_model_vector_replay_envelope_v6(
        &self,
        id: &Hash64,
    ) -> Result<misaka_palw_kernel::flat_replay6::FlatReplayEnvelopeV6, String> {
        let v = self.flat_model_vector_view_v3(id)?;
        misaka_palw_kernel::flat_replay6::flat_replay_envelope_v6(&v.trace.program, &v.scope, v.trace.encoder.as_ref())
    }
    /// Every court rebuilds the exact typed context and independently checks its
    /// persisted resource ceilings. No new scope can weaken an earlier admission.
    pub fn flat_model_vector_view_v3(&self, id: &Hash64) -> Result<FlatModelVectorViewV3, String> {
        let row = self.flat_model_vector_row_v3(id).ok_or("no flat vector")?;
        let h = &row.header;
        if row.post.len() != h.post_bytes_len as usize {
            return Err("flat stored body length changed".into());
        }
        let (post, root) = if self.flat_model_vector_protected_v6(id) {
            let p = crate::palw_flat_model_seal_v6::decode_flat_model_vector_post_v6(&row.post).map_err(|e| e.to_string())?;
            let root = p.trace_root(h.class.as_bytes(), h.binding.as_bytes());
            (p.body, root)
        } else {
            let p = decode_flat_model_vector_post_v3(&row.post).map_err(|e| e.to_string())?;
            let root = p.trace_root(h.class.as_bytes(), h.binding.as_bytes());
            (p, root)
        };
        if root != h.trace_root || post.profile != h.profile || post.scope_root != h.scope_root {
            return Err("flat stored identity changed".into());
        }
        let v = checked_flat_model_vector_with_root(self, h.class, h.binding, &post, root)?;
        if v.scope.worst_court_bytes != h.max_filing_bytes
            || v.scope.worst_court_work != h.max_court_work
            || v.scope.worst_court_scratch_bytes != h.max_court_scratch_bytes
            || v.trace.program.encode().len() != h.program_bytes_len as usize
            || v.trace.params.by_instance.len() != h.pc_instances as usize
        {
            return Err("flat persisted admission changed".into());
        }
        Ok(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> FlatModelVectorPostV3 {
        use misaka_palw_kernel::{evidence::*, seg::*};
        FlatModelVectorPostV3 {
            version: 3,
            profile: misaka_palw_kernel::flat_court5::flat_trace_profile_id_v5(),
            scope_root: [11; 64],
            trace: ModelVectorPostV2 {
                version: 2,
                prompt: vec![0],
                max_new_tokens: 1,
                generated: vec![1],
                evidence: SegmentedEvidenceV2 {
                    version: 2,
                    header: EvidenceHeaderV1 {
                        network_domain: [1; 64],
                        ruleset_digest: [2; 64],
                        class_binding_id: [3; 64],
                        program_root: [4; 64],
                        artifact_root: [5; 64],
                        plan_root: [6; 64],
                    },
                    job_input_root: [7; 64],
                    positions: 1,
                    segment_len: SEG_LEN_V4,
                    claim_root: [8; 64],
                    suite: SuiteParamsV1::of(&misaka_palw_kernel::descriptor::k2_tir_v4_descriptor()),
                },
                segment_roots: vec![[9; 64]],
                implementation_roots: [[10; 64]; 3],
            },
        }
    }
    #[test]
    fn flat_vector_v3_codec_is_separate_exact_and_refuses_nested_counts_before_bodies() {
        let p = fixture();
        let bytes = borsh::to_vec(&p).unwrap();
        assert_eq!(decode_flat_model_vector_post_v3(&bytes).unwrap(), p);
        assert!(decode_model_vector_post_v2(&bytes).is_err());
        assert!(decode_flat_model_vector_post_v3(&borsh::to_vec(&p.trace).unwrap()).is_err());
        for cut in 0..bytes.len() {
            assert!(decode_flat_model_vector_post_v3(&bytes[..cut]).is_err());
        }
        let mut bad = bytes.clone();
        bad.push(0);
        assert!(decode_flat_model_vector_post_v3(&bad).is_err());
        let mut bad = bytes.clone();
        bad[0] = 2;
        assert!(decode_flat_model_vector_post_v3(&bad).is_err());
        let mut bad = bytes;
        bad[132..136].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode_flat_model_vector_post_v3(&bad).unwrap_err().kind(), ErrorKind::InvalidData);
        use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
        for o in [
            K::PostFlatModelVectorV3 { class: [3; 64], binding: [4; 64], post: borsh::to_vec(&p).unwrap() },
            K::RefuteFlatModelVectorV3 { vector: [1; 64], proof: vec![5, 0, 255] },
        ] {
            assert_eq!(K::decode(&o.encode()).unwrap(), o);
            assert!(matches!(o.tag(), 31 | 32));
            assert_eq!(
                crate::palw_lifecycle_objects_v2::palw_kernel_route_inner_fence_v1(&o),
                Some(crate::palw_lifecycle_objects_v2::PalwKernelInnerFenceV1::PanelFreeV1)
            );
        }
    }
    #[test]
    fn flat_vector_v3_identity_binds_profile_scope_all_typed_coordinates_and_tables_are_disjoint() {
        let p = fixture();
        let id = p.id([3; 64], [4; 64]);
        assert_ne!(id, p.trace.id([3; 64], [4; 64]));
        assert_ne!(id, p.id([4; 64], [4; 64]));
        assert_ne!(id, p.id([3; 64], [5; 64]));
        for field in 0..8 {
            let mut q = p.clone();
            match field {
                0 => q.version += 1,
                1 => q.profile[0] ^= 1,
                2 => q.scope_root[0] ^= 1,
                3 => q.trace.prompt[0] += 1,
                4 => q.trace.generated[0] += 1,
                5 => q.trace.evidence.header.plan_root[0] ^= 1,
                6 => q.trace.segment_roots[0][0] ^= 1,
                _ => q.trace.max_new_tokens += 1,
            }
            assert_ne!(id, q.id([3; 64], [4; 64]));
        }
        let tables: [u8; 16] =
            [43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, FLAT_MODEL_VECTOR_TABLE_V3, FLAT_MODEL_VECTOR_BOND_TABLE_V3];
        assert_eq!(tables.into_iter().collect::<std::collections::BTreeSet<_>>().len(), 16);
        assert!(
            flat_model_vector_work_v3(256 * 1024, 1 << 20, 1 << 20).unwrap()
                > crate::palw_model_artifact_v2::model_artifact_work_v2(256 * 1024, 1 << 20).unwrap()
        );
    }
}
