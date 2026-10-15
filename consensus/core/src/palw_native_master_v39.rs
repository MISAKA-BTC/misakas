//! Owning39 complete native Master identity. These public wire/readonly rows
//! grant no eligibility, escrow credit, block or Final weight by themselves.
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
pub const MASTER_REGISTER_MAX_BYTES_V39: usize = 46 << 20;
pub const MASTER_COMPONENTS_MAX_V39: usize = 64;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct NativeMasterRegisterV39 {
    pub version: u16,
    pub sequence: u64,
    pub deadline: u64,
    pub classes: Vec<Digest>,
    pub producer_bonds: Vec<PalwBondKeyV2>,
    pub manifest: Vec<u8>,
    pub pipeline: Vec<u8>,
    pub request: Vec<u8>,
}
impl NativeMasterRegisterV39 {
    pub fn root(&self, owner: PalwBondKeyV2) -> Digest {
        object_id(b"misaka-palw/native/original-complete-master/v39", &(owner, self))
    }
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        use std::io::{Cursor, Read};
        fn read<T: BorshDeserialize>(r: &mut Cursor<&[u8]>) -> Result<T, String> {
            T::deserialize_reader(r).map_err(|_| "Master39 exact fixed field".into())
        }
        fn bytes(r: &mut Cursor<&[u8]>, cap: usize) -> Result<Vec<u8>, String> {
            let n = read::<u32>(r)? as usize;
            if n > cap || n > r.get_ref().len().saturating_sub(r.position() as usize) {
                return Err("Master39 bounded original vector prefix".into());
            }
            let mut v = vec![0; n];
            r.read_exact(&mut v).map_err(|e| e.to_string())?;
            Ok(v)
        }
        if raw.len() > MASTER_REGISTER_MAX_BYTES_V39 {
            return Err("Master39 original carrier extent".into());
        }
        let mut r = Cursor::new(raw);
        let version = read(&mut r)?;
        let sequence = read(&mut r)?;
        let deadline = read(&mut r)?;
        let n = read::<u32>(&mut r)? as usize;
        if version != 39 || n == 0 || n > MASTER_COMPONENTS_MAX_V39 || n > raw.len().saturating_sub(r.position() as usize) / 64 {
            return Err("Master39 original stage count/version".into());
        }
        let mut classes = Vec::with_capacity(n);
        for _ in 0..n {
            let c = read(&mut r)?;
            if c == [0; 64] {
                return Err("Master39 original zero class".into());
            }
            classes.push(c);
        }
        let producers = read::<u32>(&mut r)? as usize;
        if producers != n {
            return Err("Master39 exact original producer key count".into());
        }
        let mut producer_bonds = Vec::with_capacity(n);
        for _ in 0..n {
            producer_bonds.push(read(&mut r)?);
        }
        let manifest = bytes(&mut r, 4 << 20)?;
        let pipeline = bytes(&mut r, 1 << 20)?;
        let request = bytes(&mut r, 40 << 20)?;
        if r.position() as usize != raw.len() {
            return Err("Master39 trailing bytes".into());
        }
        Ok(Self { version, sequence, deadline, classes, producer_bonds, manifest, pipeline, request })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMasterComponentV39 {
    pub stage: u16,
    pub class: Digest,
    pub source: Digest,
    pub profile: Digest,
    pub program: Digest,
    pub inventory: Digest,
    pub parameter: Digest,
    pub input: Digest,
    pub output: Digest,
    pub original_context: u32,
    pub maximum_positions: u32,
    pub maximum_scope: Digest,
    pub maximum_resources: Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMasterRecordV39 {
    pub version: u16,
    pub id: Digest,
    pub owner: PalwBondKeyV2,
    pub operator: Digest,
    pub sequence: u64,
    pub admitted: u64,
    pub deadline: u64,
    pub policy: Digest,
    pub task: Digest,
    pub complete_source: Digest,
    pub manifest: Digest,
    pub pipeline: Digest,
    pub request: Digest,
    pub class_set: Digest,
    pub profile_set: Digest,
    pub program_set: Digest,
    pub inventory_set: Digest,
    pub parameter_set: Digest,
    pub maximum_scope_set: Digest,
    pub maximum_resource_set: Digest,
    pub components: Vec<NativeMasterComponentV39>,
    pub registration_bytes: u32,
    pub restore_bytes: u64,
}
impl NativeMasterRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-complete-master-record/v39", self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMasterFundedComponentV39 {
    pub stage: u16,
    pub job: Digest,
    pub funding: Digest,
    pub maximum_contract: Digest,
    pub maximum: Digest,
    pub invocation: Digest,
    pub input_binding: Digest,
    pub requester_operator: Digest,
    pub producer_operator: Digest,
    pub admitted: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMasterComponentFinalV39 {
    pub original: NativeMasterFundedComponentV39,
    pub claim: Digest,
    pub final_book: Digest,
    pub settlement: Digest,
    pub single_credit: Digest,
    pub payout: crate::Hash64,
    pub queue: crate::Hash64,
    pub amount: u64,
    pub finalized: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMasterFinalRecordV39 {
    pub version: u16,
    pub master: Digest,
    pub original_record: Digest,
    pub complete_source: Digest,
    pub request: Digest,
    pub components: Vec<NativeMasterComponentFinalV39>,
    pub single_credit_set: Digest,
    pub established: u64,
}
impl NativeMasterFinalRecordV39 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-complete-Master-Final/v39", self)
    }
}

/// Original source/request identity reconstructed from a fully authenticated
/// owning39 aux stream under the auditor's own policy and header anchor. This
/// view does not attest to computation correctness, Final or mining eligibility.
pub struct CheckedNativeMasterSourceAuditV39 {
    pub(crate) record: NativeMasterRecordV39,
    pub(crate) request: misaka_palw_kernel::flat_invocation39::MasterPublicRequestV39,
    pub(crate) pipeline: misaka_palw_kernel::flat_pipeline39::CheckedPipelineV39,
    pub(crate) funded: Vec<Option<NativeMasterFundedComponentV39>>,
    pub(crate) state: crate::Hash64,
    pub(crate) auxiliary: crate::Hash64,
    pub(crate) daa: u64,
}
impl CheckedNativeMasterSourceAuditV39 {
    pub fn record(&self) -> &NativeMasterRecordV39 {
        &self.record
    }
    pub fn request(&self) -> &misaka_palw_kernel::flat_invocation39::MasterPublicRequestV39 {
        &self.request
    }
    pub fn pipeline(&self) -> &misaka_palw_kernel::flat_pipeline39::CheckedPipelineV39 {
        &self.pipeline
    }
    pub fn funded(&self) -> &[Option<NativeMasterFundedComponentV39>] {
        &self.funded
    }
    pub fn state(&self) -> crate::Hash64 {
        self.state
    }
    pub fn auxiliary(&self) -> crate::Hash64 {
        self.auxiliary
    }
    pub fn daa(&self) -> u64 {
        self.daa
    }
}

/// New actions67..70 carry paid metadata for either Master65 or consent43.
/// Source57..60 and all older source/reply grammars retain their meanings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum NativeMetadataTargetV39 {
    Master,
    ProducerConsent,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeMetadataUploadStartV39 {
    pub version: u16,
    pub target: NativeMetadataTargetV39,
    pub object: Digest,
    pub bytes: u32,
    pub count: u16,
    pub sequence: u64,
    pub deadline: u64,
    pub body: Digest,
}
impl NativeMetadataUploadStartV39 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        if raw.len() != 153 || raw[..2] != 39u16.to_le_bytes() || raw[2] > 1 {
            return Err("metadata39 exact Start grammar".into());
        }
        borsh::from_slice(raw).map_err(|_| "metadata39 Start fixed fields".into())
    }
    pub fn maximum(&self) -> usize {
        match self.target {
            NativeMetadataTargetV39::Master => MASTER_REGISTER_MAX_BYTES_V39,
            NativeMetadataTargetV39::ProducerConsent => crate::palw_native_job_v39::NativeConsentWireV39::MAX_BYTES,
        }
    }
    pub fn check(&self, now: u64, until: u64) -> Result<(), String> {
        if self.version != 39
            || [self.object, self.body].contains(&[0; 64])
            || self.bytes == 0
            || self.bytes as usize > self.maximum()
            || self.count as u64 != (self.bytes as u64).div_ceil(32768)
            || self.count == 0
            || self.count > 1472
            || self.deadline <= now
            || self.deadline > until
        {
            return Err("metadata39 original target/bytes/pages/clock bound".into());
        }
        Ok(())
    }
    pub fn id(&self, owner: PalwBondKeyV2) -> Digest {
        object_id(b"misaka-palw/native/original-paid-metadata-upload/v39", &(owner, self))
    }
}
pub fn metadata_body_hash_v39(raw: &[u8]) -> Digest {
    object_id(b"misaka-palw/native/original-paid-metadata-body/v39", &raw)
}
/// Original Master root without decoding or allocating its large public facts.
/// The register actor separately enforces canonical bounded wire semantics.
pub fn master_root_of_bytes_v39(owner: PalwBondKeyV2, raw: &[u8]) -> Digest {
    let prefix = borsh::to_vec(&owner).expect("fixed bond");
    let mut h = misaka_palw_kernel::hash::keyed(b"misaka-palw/native/original-complete-master/v39");
    h.update(&((prefix.len() + raw.len()) as u64).to_le_bytes());
    h.update(&prefix);
    h.update(raw);
    misaka_palw_kernel::hash::finish(h)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn owner(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::Hash64::from_bytes([n; 64]), 0))
    }
    #[test]
    fn streamed_master_identity_matches_canonical_owner_bound_object() {
        let wire = NativeMasterRegisterV39 {
            version: 39,
            sequence: 3,
            deadline: 91,
            classes: vec![[1; 64], [2; 64]],
            producer_bonds: vec![owner(3), owner(4)],
            manifest: vec![8; 32769],
            pipeline: vec![6; 19],
            request: vec![7; 65537],
        };
        let raw = borsh::to_vec(&wire).unwrap();
        assert_eq!(master_root_of_bytes_v39(owner(1), &raw), wire.root(owner(1)));
        assert_ne!(master_root_of_bytes_v39(owner(2), &raw), wire.root(owner(1)));
        let mut reconstructed = Vec::new();
        for page in raw.chunks(32768) {
            reconstructed.extend_from_slice(page);
        }
        assert_eq!(metadata_body_hash_v39(&reconstructed), metadata_body_hash_v39(&raw));
        let last = reconstructed.len() - 1;
        reconstructed[last] ^= 1;
        assert_ne!(master_root_of_bytes_v39(owner(1), &reconstructed), wire.root(owner(1)));
    }
    #[test]
    fn hostile_nested_master_counts_and_cross_target_uploads_are_refused() {
        let mut raw = borsh::to_vec(&(39u16, 0u64, 42u64, u32::MAX)).unwrap();
        assert!(NativeMasterRegisterV39::from_bytes(&raw).is_err());
        raw[18..22].copy_from_slice(&1u32.to_le_bytes());
        raw.extend_from_slice(&[1; 64]);
        raw.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(NativeMasterRegisterV39::from_bytes(&raw).is_err());
        let mut start = NativeMetadataUploadStartV39 {
            version: 39,
            target: NativeMetadataTargetV39::Master,
            object: [1; 64],
            bytes: MASTER_REGISTER_MAX_BYTES_V39 as u32,
            count: 1472,
            sequence: 0,
            deadline: 50,
            body: [2; 64],
        };
        assert!(start.check(10, 60).is_ok());
        let id = start.id(owner(1));
        start.target = NativeMetadataTargetV39::ProducerConsent;
        assert_ne!(start.id(owner(1)), id);
        assert!(start.check(10, 60).is_err());
        start.bytes = 32769;
        start.count = 1;
        assert!(start.check(10, 60).is_err());
        start.count = 2;
        assert!(start.check(10, 60).is_ok());
        start.deadline = 10;
        assert!(start.check(10, 60).is_err());
    }
}
