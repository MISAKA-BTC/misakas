//! Offline preparation of protected native flat candidates and private seal/reveal material.
//! The network authenticates the signing bond, persisted seal and court at acceptance. These
//! helpers grant no admission, fidelity, availability, reward eligibility or execution Final.
//! Large objects use the existing bounded generic carriage; its aggregate inclusion/retention
//! guarantees remain open. No shipping fence is selected or activated here.
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::{
    Hash64,
    palw_flat_model_seal_v6::{FlatModelVectorPostV6, FlatNativeSealRowV6, decode_flat_model_vector_post_v6},
    palw_flat_model_vector_v3::{FlatModelVectorPostV3, decode_flat_model_vector_post_v3},
    palw_kernel_route_v1::{PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1, palw_kernel_bond_id_v1, palw_kernel_route_message_v1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, palw_object_chunks_v1},
};
use kaspa_pq_validator_core::MessageSigner;
use misaka_palw_kernel::{
    flat_court5::{FlatFaultV5, flat_trace_profile_id_v5},
    flat_seal6::{FLAT_SEAL_MAX_PROOF_BYTES_V6, check_flat_seal_v6, flat_proof_seal_v6},
    hash::Digest,
    route::KernelRouteObjectV1 as K,
};
use std::io::{Cursor, Error, ErrorKind, Write};
pub mod cli;

fn bounded_bytes<T: BorshSerialize>(value: &T, max: usize) -> Result<Vec<u8>, String> {
    struct Limited {
        bytes: Vec<u8>,
        max: usize,
    }
    impl Write for Limited {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.max - self.bytes.len() {
                return Err(Error::new(ErrorKind::InvalidData, "prepared statement exceeds its byte bound"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Limited { bytes: Vec::new(), max };
    value.serialize(&mut writer).map_err(|e| e.to_string())?;
    Ok(writer.bytes)
}

/// Bind all three implementation records to the newly protected root. Source execution,
/// job/input binding and the resource scope must still pass independent native admission.
pub fn prepare_flat_model_post_v6(class: Hash64, binding: Hash64, body: FlatModelVectorPostV3) -> Result<(Hash64, K), String> {
    if body.profile != flat_trace_profile_id_v5() {
        return Err("protected preparation requires the explicit flat-v5 trace profile".into());
    }
    let cap = kaspa_consensus_core::palw_flat_model_vector_v3::FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3;
    let body = decode_flat_model_vector_post_v3(&bounded_bytes(&body, cap)?).map_err(|e| e.to_string())?;
    if body.trace.implementation_roots != [body.trace_root(class.as_bytes(), binding.as_bytes()); 3] {
        return Err("source implementation records do not agree on this class/binding/trace root".into());
    }
    let mut post = FlatModelVectorPostV6 { version: 6, body };
    post.body.trace.implementation_roots = [post.trace_root(class.as_bytes(), binding.as_bytes()); 3];
    let bytes = bounded_bytes(&post, cap)?;
    decode_flat_model_vector_post_v6(&bytes).map_err(|e| e.to_string())?;
    Ok((
        post.id(class.as_bytes(), binding.as_bytes()),
        K::PostFlatModelVectorV6 { class: class.as_bytes(), binding: binding.as_bytes(), post: bytes },
    ))
}

/// Private local material. Never broadcast or print it while posting the public seal.
/// Salt is generated with the OS RNG by the CLI; SDK callers must supply a fresh secret salt.
#[derive(Clone, PartialEq, Eq, BorshSerialize)]
pub struct FlatProofSecretV6 {
    pub version: u16,
    pub network_domain: Hash64,
    pub vector: Hash64,
    pub bond: PalwBondKeyV2,
    pub salt: Digest,
    pub proof: Vec<u8>,
}
pub const FLAT_PROOF_SECRET_MAX_BYTES_V6: usize = FLAT_SEAL_MAX_PROOF_BYTES_V6 + 266;
impl std::fmt::Debug for FlatProofSecretV6 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlatProofSecretV6")
            .field("version", &self.version)
            .field("vector", &self.vector)
            .field("proof_bytes", &self.proof.len())
            .finish_non_exhaustive()
    }
}
impl FlatProofSecretV6 {
    pub fn new(network_domain: Hash64, vector: Hash64, bond: PalwBondKeyV2, salt: Digest, proof: Vec<u8>) -> Result<Self, String> {
        let secret = Self { version: 6, network_domain, vector, bond, salt, proof };
        secret.validate(network_domain, &bond)?;
        Ok(secret)
    }
    fn validate(&self, domain: Hash64, bond: &PalwBondKeyV2) -> Result<(), String> {
        if self.version != 6 || self.network_domain != domain || self.bond != *bond {
            return Err("private flat-v6 material differs from the selected network or signing bond".into());
        }
        if self.proof.len() > FLAT_SEAL_MAX_PROOF_BYTES_V6 {
            return Err("flat-v6 proof exceeds the seal carrier bound".into());
        }
        // Shape/codec verification only: the public acquired context is needed to convict.
        FlatFaultV5::from_bytes(&self.proof).map_err(|e| format!("flat-v5 proof grammar: {e}"))?;
        Ok(())
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        self.validate(self.network_domain, &self.bond)?;
        borsh::to_vec(self).map_err(|e| e.to_string())
    }
    /// Bound the raw input, then the nested proof count before allocating it; consume exactly.
    pub fn from_bytes(bytes: &[u8]) -> std::io::Result<Self> {
        let bad = || Error::new(ErrorKind::InvalidData, "invalid bounded private flat-v6 material");
        if bytes.len() > FLAT_PROOF_SECRET_MAX_BYTES_V6 {
            return Err(bad());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r)?;
        if version != 6 {
            return Err(bad());
        }
        let network_domain = Hash64::deserialize_reader(&mut r)?;
        let vector = Hash64::deserialize_reader(&mut r)?;
        let bond = PalwBondKeyV2::deserialize_reader(&mut r)?;
        let salt = Digest::deserialize_reader(&mut r)?;
        let count = u32::deserialize_reader(&mut r)? as usize;
        let offset = r.position() as usize;
        if count > FLAT_SEAL_MAX_PROOF_BYTES_V6 || bytes.len().checked_sub(offset) != Some(count) {
            return Err(bad());
        }
        Self::new(network_domain, vector, bond, salt, bytes[offset..].to_vec()).map_err(|_| bad())
    }
    pub fn seal_object(&self, domain: Hash64, bond: &PalwBondKeyV2) -> Result<K, String> {
        self.validate(domain, bond)?;
        let seal = flat_proof_seal_v6(&self.vector.as_bytes(), &palw_kernel_bond_id_v1(bond), &self.salt, &self.proof)
            .map_err(|e| format!("seal: {e:?}"))?;
        Ok(K::SealFlatModelProofV6 { vector: self.vector.as_bytes(), seal })
    }
    /// Check supplied fixed metadata and maturity before creating a public reveal. A row from
    /// a remote/local file is not authenticated chain state; consensus rechecks it independently.
    pub fn reveal_object(&self, domain: Hash64, bond: &PalwBondKeyV2, row: &FlatNativeSealRowV6, daa: u64) -> Result<K, String> {
        self.validate(domain, bond)?;
        if row.bond != *bond {
            return Err("seal row belongs to another signing bond".into());
        }
        check_flat_seal_v6(&row.seal, &self.vector.as_bytes(), &palw_kernel_bond_id_v1(bond), &self.salt, &self.proof, daa)
            .map_err(|e| format!("supplied seal row refuses reveal: {e:?}"))?;
        Ok(K::RevealFlatModelProofV6 { vector: self.vector.as_bytes(), salt: self.salt, proof: self.proof.clone() })
    }
}

/// Sign only the four protected native operations with the existing chain-specific message
/// and ML-DSA context. A key's correspondence to the selected bond is checked by consensus.
pub fn sign_flat_object_v6(
    key: &dyn MessageSigner,
    domain: Hash64,
    bond: PalwBondKeyV2,
    object: &K,
) -> Result<PalwConsensusObjectV2, String> {
    if !matches!(
        object,
        K::PostFlatModelVectorV6 { .. }
            | K::SealFlatModelProofV6 { .. }
            | K::RevealFlatModelProofV6 { .. }
            | K::ExpireFlatModelSealV6 { .. }
    ) {
        return Err("flat-v6 signer accepts only protected native operations".into());
    }
    if object.encoded_len() > object.max_encoded_bytes() {
        return Err("protected object exceeds its route byte bound".into());
    }
    let bytes = object.encode();
    K::decode(&bytes).map_err(|e| format!("protected object: {e}"))?;
    let message = palw_kernel_route_message_v1(domain, &bond, &bytes);
    let signature = key.sign_message(message.as_byte_slice(), PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1)?;
    let signed = PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: bond, signature };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&signed).map_err(str::to_string)?;
    Ok(signed)
}

/// Existing transport parts, retaining the signature on the exact assembled object. This
/// does not promise timely inclusion through the generic lane's shared eight-group table.
pub fn flat_object_carriers_v6(object: PalwConsensusObjectV2) -> Result<Vec<PalwConsensusObjectV2>, String> {
    match &object {
        PalwConsensusObjectV2::KernelRouteV1 { bytes, .. } => {
            let inner = K::decode(bytes).map_err(|e| e.to_string())?;
            if !matches!(
                inner,
                K::PostFlatModelVectorV6 { .. }
                    | K::SealFlatModelProofV6 { .. }
                    | K::RevealFlatModelProofV6 { .. }
                    | K::ExpireFlatModelSealV6 { .. }
            ) {
                return Err("carriers require a protected native object".into());
            }
        }
        _ => return Err("carriers require a signed kernel-route object".into()),
    }
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(str::to_string)?;
    Ok(palw_object_chunks_v1(&object).map_err(|e| e.to_string())?.unwrap_or_else(|| vec![object]))
}
