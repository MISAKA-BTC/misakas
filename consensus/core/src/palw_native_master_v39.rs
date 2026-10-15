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
