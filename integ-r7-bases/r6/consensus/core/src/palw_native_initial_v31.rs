//! Independent paid initial source/class transport. This is ordinary
//! pre-Fund registration carriage, never an own-proof29 or Final authority.
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::hash::{Digest, object_id};
pub const INITIAL_START_ACTION_V31: u8 = 41;
pub const INITIAL_PART_ACTION_V31: u8 = 42;
pub const INITIAL_ASSEMBLE_ACTION_V31: u8 = 43;
pub const INITIAL_COLLECT_ACTION_V31: u8 = 44;
pub const INITIAL_MAX_BYTES_V31: usize = 16 << 20;
pub const INITIAL_PART_BYTES_V31: usize = 32 << 10;
pub const INITIAL_MAX_PARTS_V31: u16 = 512;
pub const INITIAL_MAX_LIVE_V31: usize = 4;
pub const INITIAL_HEADER_BYTES_V31: usize = 2048;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeInitialStartV31 {
    pub version: u16,
    pub namespace: u16,
    pub action: u8,
    pub sequence: u64,
    pub bytes: u32,
    pub count: u16,
    pub original_object: Digest,
    pub expires_daa: u64,
}
impl NativeInitialStartV31 {
    pub fn check(&self, now: u64, interval_end: u64) -> Result<(), String> {
        if self.version != 31
            || !matches!((self.namespace, self.action), (29, 27) | (28, 19) | (28, 20) | (28, 33))
            || self.bytes == 0
            || self.bytes as usize > INITIAL_MAX_BYTES_V31
            || self.count == 0
            || self.count > INITIAL_MAX_PARTS_V31
            || (self.bytes as usize).div_ceil(INITIAL_PART_BYTES_V31) != self.count as usize
            || self.original_object == [0; 64]
            || self.expires_daa <= now
            || self.expires_daa >= interval_end
        {
            return Err("initial31 original bounded target/count/interval differs".into());
        }
        Ok(())
    }
    pub fn id(&self, owner: &PalwBondKeyV2) -> Digest {
        object_id(b"misaka-palw/native/original-initial-owner-object/v31", &(owner, self))
    }
}
pub fn initial_object_root_v31(body: &[u8]) -> Digest {
    object_id(b"misaka-palw/native/original-initial-canonical-body/v31", &body)
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeInitialHeaderV31 {
    pub version: u16,
    pub id: Digest,
    pub owner: PalwBondKeyV2,
    pub operator: Digest,
    pub admitted_interval: u64,
    pub start: NativeInitialStartV31,
    pub accepted: [u8; 64],
    pub attempts: [u8; 512],
    pub assembly_attempts: u8,
}
impl NativeInitialHeaderV31 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/native/original-initial-header/v31", self)
    }
    pub fn has_part(&self, i: u16) -> bool {
        i < 512 && self.accepted[i as usize / 8] & (1 << (i % 8)) != 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct NativeInitialPartV31 {
    pub version: u16,
    pub id: Digest,
    pub index: u16,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BorrowedNativeInitialPartV31<'a> {
    pub id: Digest,
    pub index: u16,
    pub bytes: &'a [u8],
    pub declared: u32,
}
pub fn borrowed_initial_part_v31(body: &[u8]) -> Option<BorrowedNativeInitialPartV31<'_>> {
    if body.len() < 72 || body.len() > 72 + INITIAL_PART_BYTES_V31 || body[..2] != 31u16.to_le_bytes() {
        return None;
    }
    let id = body[2..66].try_into().ok()?;
    let index = u16::from_le_bytes(body[66..68].try_into().ok()?);
    if index >= INITIAL_MAX_PARTS_V31 {
        return None;
    }
    Some(BorrowedNativeInitialPartV31 { id, index, declared: u32::from_le_bytes(body[68..72].try_into().ok()?), bytes: &body[72..] })
}
pub(crate) fn initial_key_v31<T: BorshSerialize>(value: &T) -> Vec<u8> {
    borsh::to_vec(&(31u8, value)).expect("fixed initial31 key")
}
impl crate::palw_kernel_route_v1::PalwKernelRouteStateV1 {
    pub fn native_initial_next_sequence_v31(&self) -> Option<u64> {
        let Some(raw) = self.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, initial_key_v31(&0u8))) else {
            return Some(0);
        };
        if raw.len() < 14 || raw.len() > 1024 || raw[..2] != 31u16.to_le_bytes() {
            return None;
        }
        let count = u32::from_le_bytes(raw[10..14].try_into().ok()?) as usize;
        if count > INITIAL_MAX_LIVE_V31 || raw.len() != 14 + count * 128 {
            return None;
        }
        Some(u64::from_le_bytes(raw[2..10].try_into().ok()?))
    }
    pub fn native_initial_header_v31(&self, id: Digest) -> Option<NativeInitialHeaderV31> {
        let raw = self.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, initial_key_v31(&(id, 1u8))))?;
        if raw.len() > INITIAL_HEADER_BYTES_V31 {
            return None;
        }
        let (root, h): (Digest, NativeInitialHeaderV31) = borsh::from_slice(raw).ok()?;
        if h.version != 31
            || h.id != id
            || h.root() != root
            || h.start.id(&h.owner) != id
            || h.start.count == 0
            || h.start.count > 512
            || h.start.bytes as usize > INITIAL_MAX_BYTES_V31
            || h.start.bytes == 0
            || (h.start.bytes as usize).div_ceil(INITIAL_PART_BYTES_V31) != h.start.count as usize
            || h.start.sequence >= self.native_initial_next_sequence_v31()?
            || !matches!((h.start.namespace, h.start.action), (28, 19) | (28, 20) | (28, 33) | (29, 27))
            || h.attempts.iter().any(|v| *v > 2)
            || h.assembly_attempts > 2
        {
            return None;
        }
        let global = self.aux.get(&(crate::palw_flat_execution_v11::EXECUTION_QUOTA_TABLE_V11, initial_key_v31(&0u8)))?;
        if !global[14..].chunks_exact(128).any(|v| v[..64] == h.id && v[64..] == h.operator) {
            return None;
        }
        Some(h)
    }
    pub fn native_initial_namespace_v31(&self, id: Digest) -> Option<u16> {
        Some(self.native_initial_header_v31(id)?.start.namespace)
    }
    pub fn native_initial_owner_v31(&self, id: Digest, index: Option<u16>, now: u64) -> Option<PalwBondKeyV2> {
        let h = self.native_initial_header_v31(id)?;
        if now > h.start.expires_daa {
            return None;
        }
        if let Some(i) = index {
            if i >= h.start.count || h.has_part(i) || h.attempts[i as usize] >= 2 {
                return None;
            }
        }
        Some(h.owner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::TransactionOutpoint;
    use kaspa_hashes::Hash64;
    #[test]
    fn initial_contract_keeps_full_extent_target_and_original_owner() {
        let owner = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([1; 64]), 0));
        let mut start = NativeInitialStartV31 {
            version: 31,
            namespace: 28,
            action: 19,
            sequence: 9,
            bytes: INITIAL_MAX_BYTES_V31 as u32,
            count: 512,
            original_object: [2; 64],
            expires_daa: 999,
        };
        start.check(1, 1000).unwrap();
        let id = start.id(&owner);
        let another = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes([3; 64]), 0));
        assert_ne!(id, start.id(&another));
        start.count = 511;
        assert!(start.check(1, 1000).is_err());
        start.count = 512;
        start.expires_daa = 1000;
        assert!(start.check(1, 1000).is_err());
        start.expires_daa = 999;
        start.action = 23;
        assert!(start.check(1, 1000).is_err(), "initial transport cannot dispatch Fund/Commit/Final");
        assert_ne!(initial_object_root_v31(b"source"), initial_object_root_v31(b"class"));
    }
    #[test]
    fn borrowed_part_bounds_index_and_defers_hostile_inner_length() {
        let part = NativeInitialPartV31 { version: 31, id: [1; 64], index: 256, bytes: vec![3; 32 << 10] };
        let raw = borsh::to_vec(&part).unwrap();
        assert_eq!(borrowed_initial_part_v31(&raw).unwrap().index, 256, "does not alias oldu8 index0");
        let mut hostile = raw.clone();
        hostile[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
        let borrowed = borrowed_initial_part_v31(&hostile).unwrap();
        assert_eq!(borrowed.bytes.len(), 32 << 10, "metadata borrows bounded actual bytes before original paid attempt");
        assert_eq!(borrowed.declared, u32::MAX);
        hostile.push(0);
        assert!(borrowed_initial_part_v31(&hostile).is_none());
        let mut invalid = raw;
        invalid[66..68].copy_from_slice(&512u16.to_le_bytes());
        assert!(borrowed_initial_part_v31(&invalid).is_none());
    }
}
