//! Authenticated links to accepted public prefix material. The court's rooted head, not
//! a peer-selected fork flag or a disk scan, selects one exact backwards history chain.
use crate::{BlockHash, Hash64, palw_constraint_stream_carriage_v1::PalwConstraintStreamMoveV1};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1: usize = 90_224;
// Seed metadata384 plus shared claim metadata256 (72-byte row and two indices).
pub const PALW_STAGED_CACHE_LEASE_STORAGE_BYTES_V1: u64 = 640;
const DOMAIN: &[u8] = b"misaka-palw/prefix-public/accepted-history/v1";

/// Native cache maintenance metadata, created only by successful rooted seed
/// acceptance. Its expiry is not a consensus deadline or a peer-selected lease.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwStagedCacheLeaseV1 {
    pub claim: Hash64,
    pub owner: crate::palw_state_v2::PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    /// Across accepted forks this is their maximum original retention horizon.
    /// Taking that union never extends a court's own immutable prosecution clock.
    pub material_until_daa: u64,
}
impl PalwStagedCacheLeaseV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        if self.claim == Hash64::default() || self.seed_id == Hash64::default() || self.reserved_daa >= self.material_until_daa {
            return Err("native cache lease lacks an exact accepted seed and retention horizon".into());
        }
        Ok(())
    }
    pub fn merge_accepted_fork_v1(&mut self, other: Self) -> Result<(), String> {
        self.check_v1()?;
        other.check_v1()?;
        if (self.claim, self.owner, self.reserved_daa, self.seed_id) != (other.claim, other.owner, other.reserved_daa, other.seed_id) {
            return Err("accepted cache lease changes immutable seed identity".into());
        }
        self.material_until_daa = self.material_until_daa.max(other.material_until_daa);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedCacheGcPageV1 {
    pub pruning_block: BlockHash,
    pub irreversible_daa: u64,
    pub leases: Vec<PalwStagedCacheLeaseV1>,
    pub next: Option<(u64, Hash64)>,
}

/// Retention union for material shared by every owner of one claim. It is
/// installed by native claim/seed acceptance, never by a local file or peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwSharedClaimCacheLeaseV1 {
    pub claim: Hash64,
    pub material_until_daa: u64,
}
impl PalwSharedClaimCacheLeaseV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        if self.claim == Hash64::default() || self.material_until_daa == 0 {
            return Err("shared public material lease lacks its original claim or retention horizon".into());
        }
        Ok(())
    }
    pub fn merge_accepted_fork_v1(&mut self, other: Self) -> Result<(), String> {
        self.check_v1()?;
        other.check_v1()?;
        if self.claim != other.claim {
            return Err("shared public material union substitutes another claim".into());
        }
        self.material_until_daa = self.material_until_daa.max(other.material_until_daa);
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSharedClaimCacheGcPageV1 {
    pub pruning_block: BlockHash,
    pub irreversible_daa: u64,
    pub leases: Vec<PalwSharedClaimCacheLeaseV1>,
    pub next: Option<(u64, Hash64)>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwPrefixHistoryRequestV1 {
    pub claim: Hash64,
    pub owner: crate::palw_state_v2::PalwBondKeyV2,
    pub reserved_daa: u64,
    pub seed_id: Hash64,
    pub head: Hash64,
    pub sequence: u64,
    pub material_until_daa: u64,
}
impl PalwPrefixHistoryRequestV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        if self.sequence == 0 || self.head == Hash64::default() || self.reserved_daa >= self.material_until_daa {
            return Err("public prefix history request has no exact bounded head/clock".into());
        }
        Ok(())
    }
    pub fn context_id_v1(&self) -> Result<Hash64, String> {
        self.check_v1()?;
        let raw = borsh::to_vec(self).map_err(|e| e.to_string())?;
        Ok(Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/prefix-public/history-request/v1")
                .hash(&raw)
                .as_bytes()
                .try_into()
                .expect("64"),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwPrefixHistoryRecordV1 {
    pub previous_head: Hash64,
    pub sequence: u64,
    pub accepted_block: BlockHash,
    pub accepted_daa: u64,
    pub material_until_daa: u64,
    pub movement: PalwConstraintStreamMoveV1,
}
impl PalwPrefixHistoryRecordV1 {
    pub fn new(
        previous_head: Hash64,
        sequence: u64,
        accepted_block: BlockHash,
        accepted_daa: u64,
        material_until_daa: u64,
        movement: PalwConstraintStreamMoveV1,
    ) -> Result<Self, String> {
        let row = Self { previous_head, sequence, accepted_block, accepted_daa, material_until_daa, movement };
        row.check_v1()?;
        Ok(row)
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.movement.check()?;
        if self.sequence == 0
            || (self.sequence == 1) != (self.previous_head == Hash64::default())
            || self.accepted_daa < self.movement.reserved_daa
            || self.accepted_daa >= self.material_until_daa
            || borsh::object_length(&self.movement).map_err(|e| e.to_string())? > 90_000
            || borsh::object_length(self).map_err(|e| e.to_string())? > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1
        {
            return Err("public prefix history link changes bounded wire sequence or immutable clock".into());
        }
        Ok(())
    }
    pub fn head_v1(&self) -> Result<Hash64, String> {
        self.check_v1()?;
        let encoded = borsh::to_vec(self).map_err(|e| e.to_string())?;
        Ok(Self::encoded_head_v1(&encoded))
    }
    fn encoded_head_v1(encoded: &[u8]) -> Hash64 {
        Hash64::from_bytes(blake2b_simd::Params::new().hash_length(64).key(DOMAIN).hash(&encoded).as_bytes().try_into().expect("64"))
    }
    /// Authenticate the bounded raw carrier against a native rooted request
    /// before a nested Borsh vector can allocate from peer-selected lengths.
    pub fn decode_authenticated_request_v1(encoded: &[u8], request: &PalwPrefixHistoryRequestV1) -> Result<Self, String> {
        request.check_v1()?;
        if encoded.len() > PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 || Self::encoded_head_v1(encoded) != request.head {
            return Err("public history carrier does not match its bounded original accepted head".into());
        }
        let record: Self = borsh::from_slice(encoded).map_err(|e| e.to_string())?;
        record.authenticates_request_v1(request)?;
        Ok(record)
    }
    /// The read snapshot supplies every expected field. No archive metadata chooses the
    /// owner, seed, branch, sequence or extension of the original paid material horizon.
    pub fn authenticates_v1(
        &self,
        head: Hash64,
        sequence: u64,
        claim: Hash64,
        owner: crate::palw_state_v2::PalwBondKeyV2,
        reserved_daa: u64,
        seed_id: Hash64,
        material_until_daa: u64,
    ) -> Result<(), String> {
        if self.head_v1()? != head
            || self.sequence != sequence
            || (self.movement.claim, self.movement.reserver, self.movement.reserved_daa, self.movement.seed_id)
                != (claim, owner, reserved_daa, seed_id)
            || self.material_until_daa != material_until_daa
        {
            return Err("public prefix history record does not authenticate under accepted court head".into());
        }
        Ok(())
    }
    pub fn authenticates_request_v1(&self, request: &PalwPrefixHistoryRequestV1) -> Result<(), String> {
        request.check_v1()?;
        self.authenticates_v1(
            request.head,
            request.sequence,
            request.claim,
            request.owner,
            request.reserved_daa,
            request.seed_id,
            request.material_until_daa,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_constraint_stream_carriage_v1::PalwConstraintStreamActionV1,
        palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
        palw_state_v2::PalwBondKeyV2,
        tx::{TransactionId, TransactionOutpoint},
    };
    fn record() -> PalwPrefixHistoryRecordV1 {
        PalwPrefixHistoryRecordV1::new(
            Hash64::default(),
            1,
            BlockHash::from_u64_word(8),
            100,
            200,
            PalwConstraintStreamMoveV1 {
                version: 1,
                claim: Hash64::from_u64_word(1),
                reserver: PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(2), index: 0 }),
                reserved_daa: 90,
                seed_id: Hash64::from_u64_word(3),
                phase: PalwConstraintStreamPhaseV1::PrefixCourt,
                action: PalwConstraintStreamActionV1::Prefix(crate::palw_prefix_family_court_v1::PalwPrefixCourtActionV1::FinishTile),
                signature: vec![7; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
            },
        )
        .unwrap()
    }
    #[test]
    fn accepted_head_commits_to_wire_signature_branch_clock_and_order_without_reset() {
        let first = record();
        let head = first.head_v1().unwrap();
        first
            .authenticates_v1(
                head,
                1,
                first.movement.claim,
                first.movement.reserver,
                first.movement.reserved_daa,
                first.movement.seed_id,
                first.material_until_daa,
            )
            .unwrap();
        for field in 0..7 {
            let mut bad = first.clone();
            match field {
                0 => bad.movement.signature[0] ^= 1,
                1 => bad.movement.claim = Hash64::from_u64_word(91),
                2 => bad.movement.seed_id = Hash64::from_u64_word(92),
                3 => bad.accepted_block = BlockHash::from_u64_word(93),
                4 => bad.accepted_daa += 1,
                5 => bad.material_until_daa += 1,
                _ => {
                    bad.sequence = 2;
                    bad.previous_head = Hash64::from_u64_word(94);
                }
            }
            assert!(
                bad.authenticates_v1(
                    head,
                    1,
                    first.movement.claim,
                    first.movement.reserver,
                    first.movement.reserved_daa,
                    first.movement.seed_id,
                    first.material_until_daa
                )
                .is_err()
            );
        }
        let second = PalwPrefixHistoryRecordV1::new(head, 2, BlockHash::from_u64_word(9), 101, 200, first.movement.clone()).unwrap();
        assert_ne!(second.head_v1().unwrap(), head);
        assert_eq!(second.previous_head, head);
        assert!(PalwPrefixHistoryRecordV1::new(Hash64::default(), 2, first.accepted_block, 100, 200, first.movement.clone()).is_err());
        assert!(PalwPrefixHistoryRecordV1::new(head, 2, first.accepted_block, 200, 200, first.movement).is_err());
    }
    #[test]
    fn untrusted_record_bytes_authenticate_before_nested_carrier_decoding() {
        let row = record();
        let request = PalwPrefixHistoryRequestV1 {
            claim: row.movement.claim,
            owner: row.movement.reserver,
            reserved_daa: row.movement.reserved_daa,
            seed_id: row.movement.seed_id,
            head: row.head_v1().unwrap(),
            sequence: row.sequence,
            material_until_daa: row.material_until_daa,
        };
        let encoded = borsh::to_vec(&row).unwrap();
        assert_eq!(PalwPrefixHistoryRecordV1::decode_authenticated_request_v1(&encoded, &request).unwrap(), row);
        let mut tampered = encoded.clone();
        // The signature length is the final vector prefix for this scalar action.
        let offset = tampered.len() - crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN - 4;
        tampered[offset..offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let error = PalwPrefixHistoryRecordV1::decode_authenticated_request_v1(&tampered, &request).unwrap_err();
        assert!(error.contains("accepted head"));
        assert!(PalwPrefixHistoryRecordV1::decode_authenticated_request_v1(&encoded[..encoded.len() - 1], &request).is_err());
        assert!(
            PalwPrefixHistoryRecordV1::decode_authenticated_request_v1(
                &vec![0; PALW_PREFIX_HISTORY_MAX_RECORD_BYTES_V1 + 1],
                &request,
            )
            .is_err()
        );
        let mut wrong_owner = request;
        wrong_owner.owner.0.index += 1;
        assert!(PalwPrefixHistoryRecordV1::decode_authenticated_request_v1(&encoded, &wrong_owner).is_err());
    }
}
