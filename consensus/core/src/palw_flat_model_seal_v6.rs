//! Explicit protected candidate grammar and persisted actor-bound evidence seals.
//! Dormant consumer component; v3 statements retain their historical meaning.
use crate::{Hash64, palw_flat_model_vector_v3::*, palw_kernel_route_v1::PalwKernelRouteStateV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    flat_seal6::FlatSealRowV6,
    hash::{Digest, object_id},
};

pub const FLAT_PROTECTED_TABLE_V6: u8 = 64;
pub const FLAT_SEAL_TABLE_V6: u8 = 65;
pub const FLAT_SEAL_ACTOR_TABLE_V6: u8 = 66;
pub const FLAT_SEAL_COUNT_TABLE_V6: u8 = 67;
pub const FLAT_NATIVE_SEAL_MATURITY_DAA_V6: u64 = 3;
pub const FLAT_NATIVE_SEAL_TTL_DAA_V6: u64 = 100;
pub const FLAT_NATIVE_SEALS_PER_BOND_V6: usize = 4;
pub const FLAT_NATIVE_SEALS_MAX_V6: u32 = 1024;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatModelVectorPostV6 {
    pub version: u16,
    pub body: FlatModelVectorPostV3,
}
impl FlatModelVectorPostV6 {
    pub fn trace_root(&self, class: Digest, binding: Digest) -> Digest {
        object_id(
            b"misaka-palw/flat-model-vector/protected-trace/v6",
            &(
                self.version,
                self.body.trace_root(class, binding),
                FLAT_NATIVE_SEAL_MATURITY_DAA_V6,
                FLAT_NATIVE_SEAL_TTL_DAA_V6,
                FLAT_NATIVE_SEALS_PER_BOND_V6 as u32,
                FLAT_NATIVE_SEALS_MAX_V6,
            ),
        )
    }
    pub fn id(&self, class: Digest, binding: Digest) -> Hash64 {
        vector_id(self.trace_root(class, binding))
    }
}
pub fn decode_flat_model_vector_post_v6(bytes: &[u8]) -> std::io::Result<FlatModelVectorPostV6> {
    if bytes.len() < 2 || bytes.len() > FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3 || bytes[..2] != 6u16.to_le_bytes() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid bounded protected flat-v6 statement"));
    }
    Ok(FlatModelVectorPostV6 { version: 6, body: decode_flat_model_vector_post_v3(&bytes[2..])? })
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatNativeSealRowV6 {
    pub seal: FlatSealRowV6,
    pub bond: PalwBondKeyV2,
    pub reserved: u64,
}
impl PalwKernelRouteStateV1 {
    pub fn flat_model_vector_protected_v6(&self, vector: &Hash64) -> bool {
        self.aux_row::<bool>(FLAT_PROTECTED_TABLE_V6, &borsh::to_vec(vector).unwrap()).unwrap_or(false)
    }
    pub fn flat_model_seal_v6(&self, vector: &Hash64, actor: &Digest) -> Option<FlatNativeSealRowV6> {
        let row: FlatNativeSealRowV6 = self.aux_row(FLAT_SEAL_TABLE_V6, &borsh::to_vec(&(vector, actor)).ok()?)?;
        (row.seal.version == 6
            && row.seal.vector == vector.as_bytes()
            && row.seal.actor == *actor
            && crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(&row.bond) == *actor)
            .then_some(row)
    }
    pub fn flat_model_seals_of_v6(&self, bond: &PalwBondKeyV2) -> Vec<Hash64> {
        let ids: Vec<Hash64> = self.aux_row(FLAT_SEAL_ACTOR_TABLE_V6, &borsh::to_vec(bond).unwrap()).unwrap_or_default();
        if ids.len() <= FLAT_NATIVE_SEALS_PER_BOND_V6 { ids } else { Vec::new() }
    }
    pub fn flat_model_seal_count_v6(&self) -> u32 {
        self.aux_row(FLAT_SEAL_COUNT_TABLE_V6, &[]).unwrap_or(0)
    }
    /// Expired deposits remain held until an explicit bounded expiry forfeits them.
    pub fn flat_model_seal_reserved_v6(&self, bond: &PalwBondKeyV2) -> u128 {
        let actor = crate::palw_kernel_route_v1::palw_kernel_bond_id_v1(bond);
        self.flat_model_seals_of_v6(bond).iter().filter_map(|v| self.flat_model_seal_v6(v, &actor)).map(|r| r.reserved as u128).sum()
    }
}
