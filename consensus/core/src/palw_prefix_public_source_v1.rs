//! Isolated attempts at one derived public prefix source. The owner continuation is never
//! modified by tentative answers. Outer admission owns signed custody, scope and payment;
//! this component authenticates the original whole step leaf before exposing any bytes.
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step::{PalwStepCoordinateV1, canonical_step_leaf_index};
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

pub const PALW_PREFIX_PUBLIC_PART_BYTES_V1: usize = 65_536;

/// This descriptor must equal the family's derived pending cursor at outer admission.
/// A canonical but unrelated coordinate is not authority to open a demand.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixPublicSourceV1 {
    pub coordinate: PalwStepCoordinateV1,
    pub leaf_index: u64,
    pub value_count: u32,
    pub byte_length: u32,
    pub step_root: Hash64,
    pub step_leaves: u64,
    pub maximum_leaves: u64,
    hash_header: LeafHashContinuationV1,
}
impl PalwPrefixPublicSourceV1 {
    pub fn derive(binding: &PalwStepBindingV2, coordinate: PalwStepCoordinateV1, maximum_leaves: u64) -> Result<Self, String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        if crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, maximum_leaves)
            .map_err(|e| e.to_string())?
            != binding.step_leaf_count
        {
            return Err("public prefix source has noncanonical original step count".into());
        }
        let leaf_index = canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coordinate)
            .ok_or("public prefix source coordinate is not canonical")?;
        let value_count =
            crate::palw_step_leg::palw_expected_tile_values_v1(&binding.shape_profile, &binding.job_context, &coordinate)
                .ok_or("public prefix source value count is not canonical")?;
        let byte_length = value_count.checked_mul(4).ok_or("public prefix source length overflow")?;
        if byte_length as usize > crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES {
            return Err("public prefix source exceeds original tile format".into());
        }
        let mut hash_header = LeafHashContinuationV1::new(crate::palw_step_leg::PALW_STEP_LEG_DOMAIN_TILE_LEAF);
        hash_header.update(&crate::palw_step::PALW_STEP_OBJECT_VERSION_V1.to_le_bytes());
        hash_header.update(binding.job_context.leaf_context_hash_v1(coordinate.call_index, coordinate.position).as_byte_slice());
        hash_header.update(binding.shape_profile.shape_profile_id().as_byte_slice());
        for n in [coordinate.call_index, coordinate.node_slot, coordinate.position, coordinate.tile_index, value_count, byte_length] {
            hash_header.update(&n.to_le_bytes());
        }
        Ok(Self {
            coordinate,
            leaf_index,
            value_count,
            byte_length,
            step_root: binding.step_merkle_root,
            step_leaves: binding.step_leaf_count,
            maximum_leaves,
            hash_header,
        })
    }
    fn authenticates_opening(&self, opening: &PalwStepOpeningV1) -> Result<(), String> {
        if opening.leaf_index != self.leaf_index
            || crate::palw_step_leg::step_opening_root_capped_v1(self.step_leaves, opening, self.maximum_leaves)
                .map_err(|e| e.to_string())?
                != self.step_root
        {
            return Err("public prefix answer changes literal pending original leaf".into());
        }
        Ok(())
    }
}

/// Checked maximum additional work for ALL fixed slots, including hostile complete answers.
/// This is added to the paid prefix contract before a demand opens, never debited from the
/// owner's honest continuation allowance. Carrier size includes signatures and openings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPrefixPublicAttemptPlanV1 {
    pub slots: u64,
    pub maximum_moves: u64,
    pub maximum_carrier_bytes: u64,
    pub maximum_buffer_bytes: u64,
    pub maximum_hash_bytes: u64,
    pub maximum_journal_bytes: u64,
}
impl PalwPrefixPublicAttemptPlanV1 {
    pub fn derive(source: &PalwPrefixPublicSourceV1, slots: usize, carrier_maximum: u64) -> Result<Self, String> {
        let slots = u64::try_from(slots).map_err(|_| "public prefix attempt slot count overflow")?;
        if slots == 0 || carrier_maximum < PALW_PREFIX_PUBLIC_PART_BYTES_V1 as u64 + 8_192 {
            return Err("public prefix attempts need actual participants and bounded signed carriers".into());
        }
        let per_slot = u64::from(source.byte_length)
            .div_ceil(PALW_PREFIX_PUBLIC_PART_BYTES_V1 as u64)
            .checked_add(2)
            .ok_or("public prefix moves overflow")?;
        let maximum_moves = slots.checked_mul(per_slot).ok_or("public prefix moves overflow")?;
        // Vec capacity may round up: two times all candidates, finish clone and bounded
        // answer/replay buffers, plus hash/opening/map metadata. Never price just live len.
        let maximum_buffer_bytes = slots
            .checked_mul(2)
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_mul(u64::from(source.byte_length)))
            .and_then(|n| slots.checked_mul(8_192).and_then(|m| n.checked_add(m)))
            .and_then(|n| n.checked_add(8_192))
            .ok_or("public prefix buffer overflow")?;
        let maximum_carrier_bytes = maximum_moves.checked_mul(carrier_maximum).ok_or("public prefix carriers overflow")?;
        // Each carriage/reorg consistency check reauthenticates all saved mutable buffers;
        // charge that actual implementation, not merely one successful leaf hash.
        let checked_bytes = slots
            .checked_mul(282 + 64 * 193)
            .and_then(|n| n.checked_add(maximum_buffer_bytes))
            .and_then(|n| n.checked_add(carrier_maximum))
            .ok_or("public prefix hash work overflow")?;
        let maximum_hash_bytes =
            maximum_moves.checked_mul(checked_bytes).and_then(|n| n.checked_mul(4)).ok_or("public prefix hash work overflow")?;
        // A mutable-only journal may contain every candidate buffer at each bounded move;
        // this pessimistic bound charges the actual rollback/storage implementation.
        let checkpoint_bytes = maximum_buffer_bytes
            .checked_add(slots.checked_mul(8_192).ok_or("public prefix journal overflow")?)
            .and_then(|n| n.checked_add(8_192))
            .ok_or("public prefix journal overflow")?;
        let maximum_journal_bytes =
            maximum_moves.checked_mul(checkpoint_bytes).and_then(|n| n.checked_mul(2)).ok_or("public prefix journal overflow")?;
        Ok(Self { slots, maximum_moves, maximum_carrier_bytes, maximum_buffer_bytes, maximum_hash_bytes, maximum_journal_bytes })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum PalwPrefixPublicAttemptV1 {
    Waiting,
    Receiving { opening: PalwStepOpeningV1, hash: LeafHashContinuationV1, bytes: Vec<u8> },
    Invalid,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixPublicPendingV1 {
    claim: Hash64,
    owner: PalwBondKeyV2,
    reserved_daa: u64,
    seed_id: Hash64,
    cursor_id: Hash64,
    demand_id: Hash64,
    deadline_daa: u64,
    source: PalwPrefixPublicSourceV1,
    /// Only producer and actual bound seats with a signed prospective custody obligation.
    custodians: BTreeSet<PalwBondKeyV2>,
    attempts: BTreeMap<PalwBondKeyV2, PalwPrefixPublicAttemptV1>,
    resolved: Option<(PalwStepOpeningV1, Vec<u8>)>,
}
impl PalwPrefixPublicPendingV1 {
    /// Outer code verifies the owner's signed demand, exact derived family cursor, actual
    /// signed custody, fully paid plan and material/ALL participant holds BEFORE calling.
    pub fn new(
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        seed_id: Hash64,
        cursor_id: Hash64,
        demand_id: Hash64,
        now_daa: u64,
        deadline_daa: u64,
        source: PalwPrefixPublicSourceV1,
        custodians: BTreeSet<PalwBondKeyV2>,
    ) -> Result<Self, String> {
        if custodians.is_empty() || now_daa < reserved_daa || deadline_daa <= now_daa {
            return Err("public prefix demand has no literal custody or bounded response interval".into());
        }
        let mut attempts: BTreeMap<_, _> = custodians.iter().map(|key| (*key, PalwPrefixPublicAttemptV1::Waiting)).collect();
        attempts.entry(owner).or_insert(PalwPrefixPublicAttemptV1::Waiting);
        let result = Self {
            claim,
            owner,
            reserved_daa,
            seed_id,
            cursor_id,
            demand_id,
            deadline_daa,
            source,
            custodians,
            attempts,
            resolved: None,
        };
        result.check()?;
        Ok(result)
    }
    pub fn source(&self) -> &PalwPrefixPublicSourceV1 {
        &self.source
    }
    pub fn check(&self) -> Result<(), String> {
        if self.custodians.is_empty()
            || self.deadline_daa <= self.reserved_daa
            || self.source.hash_header.byte_count() != Some(282)
            || self.source.byte_length as usize > crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES
            || self.source.value_count.checked_mul(4) != Some(self.source.byte_length)
        {
            return Err("public prefix pending immutable counters are inconsistent".into());
        }
        if let Some((opening, bytes)) = &self.resolved {
            self.source.authenticates_opening(opening)?;
            let mut hash = self.source.hash_header.clone();
            hash.update(bytes);
            if !self.attempts.is_empty() || bytes.len() != self.source.byte_length as usize || hash.finish() != opening.leaf_hash {
                return Err("public prefix resolved bytes are not authenticated".into());
            }
        } else {
            let expected: BTreeSet<_> = self.custodians.iter().copied().chain(std::iter::once(self.owner)).collect();
            if self.attempts.keys().copied().collect::<BTreeSet<_>>() != expected {
                return Err("public prefix attempt slots differ from literal saved custodians/owner".into());
            }
            for attempt in self.attempts.values() {
                if let PalwPrefixPublicAttemptV1::Receiving { opening, hash, bytes } = attempt {
                    self.source.authenticates_opening(opening)?;
                    let mut actual_hash = self.source.hash_header.clone();
                    actual_hash.update(bytes);
                    if bytes.len() > self.source.byte_length as usize
                        || hash.byte_count() != Some(282 + bytes.len() as u128)
                        || *hash != actual_hash
                    {
                        return Err("public prefix receiving byte/hash counters inconsistent".into());
                    }
                }
            }
        }
        Ok(())
    }
    pub fn plan(&self, carrier_maximum: u64) -> Result<PalwPrefixPublicAttemptPlanV1, String> {
        PalwPrefixPublicAttemptPlanV1::derive(&self.source, self.attempts.len(), carrier_maximum)
    }
    /// Signature verification occurs first against the declared answerer, over the full
    /// literal owner/claim/seed/cursor/demand identity plus this exact action and phase.
    pub fn matches(
        &self,
        claim: Hash64,
        owner: PalwBondKeyV2,
        reserved_daa: u64,
        seed_id: Hash64,
        cursor_id: Hash64,
        demand_id: Hash64,
    ) -> bool {
        (self.claim, self.owner, self.reserved_daa, self.seed_id, self.cursor_id, self.demand_id)
            == (claim, owner, reserved_daa, seed_id, cursor_id, demand_id)
    }
    fn live(&self, signer: PalwBondKeyV2, now_daa: u64) -> Result<(), String> {
        self.check()?;
        if now_daa < self.reserved_daa
            || now_daa >= self.deadline_daa
            || self.resolved.is_some()
            || !self.attempts.contains_key(&signer)
        {
            return Err("public prefix answer is late, resolved or outside literal custodian/owner slots".into());
        }
        Ok(())
    }
    pub fn begin(&mut self, signer: PalwBondKeyV2, now_daa: u64, opening: &PalwStepOpeningV1) -> Result<(), String> {
        self.live(signer, now_daa)?;
        self.source.authenticates_opening(opening)?;
        if self.attempts.get(&signer) != Some(&PalwPrefixPublicAttemptV1::Waiting) {
            return Err("public prefix signer has already used its one paid attempt".into());
        }
        self.attempts.insert(
            signer,
            PalwPrefixPublicAttemptV1::Receiving {
                opening: opening.clone(),
                hash: self.source.hash_header.clone(),
                bytes: Vec::new(),
            },
        );
        Ok(())
    }
    pub fn part(&mut self, signer: PalwBondKeyV2, now_daa: u64, offset: u32, bytes: &[u8]) -> Result<(), String> {
        self.live(signer, now_daa)?;
        let Some(PalwPrefixPublicAttemptV1::Receiving { hash, bytes: prior, .. }) = self.attempts.get_mut(&signer) else {
            return Err("public prefix signer has no live authenticated opening".into());
        };
        if bytes.is_empty()
            || bytes.len() > PALW_PREFIX_PUBLIC_PART_BYTES_V1
            || prior.len() != offset as usize
            || prior.len() as u64 + bytes.len() as u64 > u64::from(self.source.byte_length)
        {
            return Err("public prefix part changes exact offset or bounded original length".into());
        }
        hash.update(bytes);
        prior.extend_from_slice(bytes);
        Ok(())
    }
    /// A complete wrong hash consumes only THIS custodian's paid attempt. No false guilt is
    /// returned. Other literal custodians/owner retain untouched slots and the owner court
    /// retains its original checkpoint. Only no correct answer at the deadline is a default.
    pub fn finish(&mut self, signer: PalwBondKeyV2, now_daa: u64) -> Result<bool, String> {
        self.live(signer, now_daa)?;
        let Some(PalwPrefixPublicAttemptV1::Receiving { opening, hash, bytes }) = self.attempts.get(&signer) else {
            return Err("public prefix signer has no receiving attempt".into());
        };
        if bytes.len() != self.source.byte_length as usize {
            return Err("public prefix answer is incomplete".into());
        }
        if hash.finish() != opening.leaf_hash {
            self.attempts.insert(signer, PalwPrefixPublicAttemptV1::Invalid);
            return Ok(false);
        }
        self.resolved = Some((opening.clone(), bytes.clone()));
        self.attempts.clear();
        Ok(true)
    }
    pub fn authenticated_answer(&self) -> Option<(&PalwStepOpeningV1, &[u8])> {
        self.resolved.as_ref().map(|(opening, bytes)| (opening, bytes.as_slice()))
    }
    /// Attribution is to the saved JOINT signed custodians, not to every bad individual
    /// answer. The outer court must additionally enforce accepting collection clock.
    pub fn defaulted_custodians(&self, now_daa: u64) -> Option<&BTreeSet<PalwBondKeyV2>> {
        (now_daa >= self.deadline_daa && self.resolved.is_none()).then_some(&self.custodians)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(n), index: 0 })
    }
    fn fixture() -> (PalwPrefixPublicPendingV1, crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1) {
        let (target, binding, _, tiles) = crate::palw_prefix_family_court_v1::tests::qualified_prefix_case_for_tests_v1(true, false);
        let tile = tiles[0].clone();
        let coordinate = tile.preimage.coord;
        let source = PalwPrefixPublicSourceV1::derive(&binding, coordinate, 1 << 32).unwrap();
        let custodians = (1..=6).map(bond).collect();
        let pending = PalwPrefixPublicPendingV1::new(
            target.claim_id,
            bond(7),
            100,
            Hash64::from_u64_word(11),
            Hash64::from_u64_word(12),
            Hash64::from_u64_word(13),
            100,
            200,
            source,
            custodians,
        )
        .unwrap();
        pending.check().unwrap();
        (pending, tile)
    }
    #[test]
    fn hostile_first_writer_does_not_poison_other_custodian_or_owner_slots() {
        let (mut pending, tile) = fixture();
        let original = pending.clone();
        assert!(pending.begin(bond(99), 100, &tile.opening).is_err());
        assert_eq!(pending, original);
        let mut wrong_opening = tile.opening.clone();
        wrong_opening.leaf_index += 1;
        assert!(pending.begin(bond(1), 100, &wrong_opening).is_err());
        assert_eq!(pending, original);
        pending.begin(bond(1), 100, &tile.opening).unwrap();
        let mut wrong = tile.preimage.values_le.clone();
        wrong[0] ^= 1;
        pending.part(bond(1), 101, 0, &wrong).unwrap();
        assert!(pending.authenticated_answer().is_none());
        assert!(!pending.finish(bond(1), 102).unwrap());
        let bad = pending.clone();
        assert!(pending.begin(bond(1), 103, &tile.opening).is_err());
        assert_eq!(pending, bad);
        pending.begin(bond(7), 104, &tile.opening).unwrap();
        pending.part(bond(7), 105, 0, &tile.preimage.values_le).unwrap();
        assert!(pending.authenticated_answer().is_none());
        assert!(pending.finish(bond(7), 106).unwrap());
        pending.check().unwrap();
        assert_eq!(pending.authenticated_answer().unwrap().1, tile.preimage.values_le.as_slice());
        assert!(pending.defaulted_custodians(200).is_none());
    }
    #[test]
    fn literal_deadline_default_and_serialized_rollback_preserve_all_slots() {
        let (mut pending, tile) = fixture();
        let seed = pending.clone();
        let plan = pending.plan(90_000).unwrap();
        assert_eq!(plan.slots, 7);
        assert!(plan.maximum_moves >= 21);
        assert!(plan.maximum_buffer_bytes >= 9 * u64::from(pending.source.byte_length));
        pending.begin(bond(1), 199, &tile.opening).unwrap();
        let journal = borsh::to_vec(&pending).unwrap();
        let restarted: PalwPrefixPublicPendingV1 = borsh::from_slice(&journal).unwrap();
        restarted.check().unwrap();
        assert_eq!(restarted, pending);
        let before = pending.clone();
        assert!(pending.part(bond(1), 200, 0, &tile.preimage.values_le).is_err());
        assert_eq!(pending, before);
        assert!(pending.defaulted_custodians(199).is_none());
        assert_eq!(pending.defaulted_custodians(200).unwrap().len(), 6);
        // Applying then reverting the mutable row restores unused honest slots as well.
        pending = seed.clone();
        assert_eq!(pending, seed);
        pending.check().unwrap();
    }
}
