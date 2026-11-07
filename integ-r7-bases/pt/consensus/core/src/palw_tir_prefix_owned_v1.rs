//! **The TIR prefix-state family court** (behind `palw_staged_tir_route_v1`). It is a paid,
//! owned, bundled stage over the relation of [`crate::palw_tir_prefix_state_v1`].
//!
//! An outside owner opens it with a native seed (`PalwNativeSeedFamilyV1::PrefixState`). The
//! claim's literal custody (the producer and the held Panel) then owes, slot by slot, every
//! boundary leaf of the prefix state: values and an opening against the claim's own step tree,
//! in canonical order, until `respond_until_daa`. The owner may stream public bytes into its own
//! slot but carries no duty.
//!
//! The first slot that completes decides the verdict. The claim's own leaves either re-derive
//! the job's declared state root (honest) or they do not, which is an objective fault from
//! public, authenticated material alone. A custodian that has not completed its slot by
//! `respond_until_daa` is returned to the native sanction fold at collection. When every
//! custodian withholds, nothing authenticates and all of them are collected: the same default
//! as every staged source.
use crate::Hash64;
use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step_leg::PalwStepOpeningV1;
use crate::palw_tir_prefix_state_v1::{PalwTirPrefixStateStreamV1, PalwTirPrefixStateVerdictV1};
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

/// One boundary leaf's lanes in one signed carrier (the staged carrier is 90,000 bytes).
pub const PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1: usize = 80_000;
/// Sibling bound of a step opening (the step tree's depth).
pub const PALW_TIR_PREFIX_OPENING_DEPTH_V1: usize = 64;

/// A custodian's (or the owner's) next boundary leaf. Authority is the typed answerer.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTirPrefixLeafAnswerV1 {
    pub answerer: PalwBondKeyV2,
    pub seed_id: Hash64,
    /// The slot's cursor: binds the packet to one turn.
    pub ordinal: u64,
    pub values_le: Vec<u8>,
    pub opening: PalwStepOpeningV1,
}
impl BorshDeserialize for PalwTirPrefixLeafAnswerV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        fn bounded<T: BorshDeserialize, R: std::io::Read>(reader: &mut R, max: usize) -> std::io::Result<Vec<T>> {
            let length = u32::deserialize_reader(reader)? as usize;
            if length > max {
                return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "TIR prefix leaf exceeds its paid carrier"));
            }
            let mut values = Vec::new();
            values
                .try_reserve_exact(length)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::OutOfMemory, "TIR prefix leaf allocation refused"))?;
            for _ in 0..length {
                values.push(T::deserialize_reader(reader)?);
            }
            Ok(values)
        }
        Ok(Self {
            answerer: BorshDeserialize::deserialize_reader(reader)?,
            seed_id: BorshDeserialize::deserialize_reader(reader)?,
            ordinal: BorshDeserialize::deserialize_reader(reader)?,
            values_le: bounded(reader, PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1)?,
            opening: PalwStepOpeningV1 {
                leaf_index: BorshDeserialize::deserialize_reader(reader)?,
                leaf_hash: BorshDeserialize::deserialize_reader(reader)?,
                siblings: bounded(reader, PALW_TIR_PREFIX_OPENING_DEPTH_V1)?,
            },
        })
    }
}
impl PalwTirPrefixLeafAnswerV1 {
    pub fn check_wire_v1(&self) -> Result<(), String> {
        if self.values_le.is_empty()
            || self.values_le.len() > PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1
            || !self.values_le.len().is_multiple_of(4)
            || self.opening.siblings.len() > PALW_TIR_PREFIX_OPENING_DEPTH_V1
        {
            return Err("TIR prefix leaf exceeds its bounded original envelope".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedTirPrefixCourtV1 {
    pub claim: Hash64,
    pub reserver: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub terms: PalwRenderingClaimTermsV1,
    pub rent_paid: u64,
    pub deposit_locked: u64,
    pub moves_used: u64,
    seed_id: Hash64,
    execution_root: Hash64,
    pub producer: PalwBondKeyV2,
    pub custodians: BTreeSet<PalwBondKeyV2>,
    /// Every custodian owes its complete boundary stream by this clock.
    pub respond_until_daa: u64,
    template: PalwTirPrefixStateStreamV1,
    slots: BTreeMap<PalwBondKeyV2, PalwTirPrefixStateStreamV1>,
    collected: bool,
    verdict: Option<PalwTirPrefixStateVerdictV1>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedTirPrefixCheckpointV1 {
    slots: BTreeMap<PalwBondKeyV2, PalwTirPrefixStateStreamV1>,
    moves_used: u64,
    collected: bool,
    verdict: Option<PalwTirPrefixStateVerdictV1>,
}

impl PalwOwnedTirPrefixCourtV1 {
    /// The fold supplies the native seed id, the claim-stamped stage terms and literal custody,
    /// and an UNSTARTED stream over the claim's admitted header and registered class
    /// ([`PalwTirPrefixStateStreamV1::new_v1`]).
    #[allow(clippy::too_many_arguments)]
    pub fn admit_v1(
        seed_id: Hash64,
        claim: Hash64,
        reserver: PalwBondKeyV2,
        reserved_daa: u64,
        execution_root: Hash64,
        terms: PalwRenderingClaimTermsV1,
        now: u64,
        funding: u64,
        producer: PalwBondKeyV2,
        custodians: BTreeSet<PalwBondKeyV2>,
        template: PalwTirPrefixStateStreamV1,
        material_until: u64,
        producer_exit: u64,
        panel_exits: &[u64],
    ) -> Result<Self, String> {
        if now < terms.accepted_daa || now >= terms.open_until_daa || reserved_daa > now {
            return Err("TIR prefix court is outside its claim-stamped opening window".into());
        }
        if !custodians.contains(&producer) || custodians.contains(&reserver) {
            return Err("TIR prefix court needs literal producer custody and an outside owner".into());
        }
        terms.check_holds(material_until, producer_exit, panel_exits)?;
        if funding < terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("TIR prefix funding overflow")? {
            return Err("TIR prefix court has no full prepaid work budget".into());
        }
        template.check_v1()?;
        if template.ordinal_v1() != 0 {
            return Err("TIR prefix court opens over an unstarted stream".into());
        }
        let row = Self {
            claim,
            reserver,
            reserved_daa,
            terms,
            rent_paid: terms.plan.rent_sompi,
            deposit_locked: terms.plan.deposit_sompi,
            moves_used: 1,
            seed_id,
            execution_root,
            producer,
            custodians,
            respond_until_daa: terms.stream_until_daa,
            template,
            slots: BTreeMap::new(),
            collected: false,
            verdict: None,
        };
        row.check()?;
        Ok(row)
    }
    pub fn key(&self) -> (Hash64, PalwBondKeyV2) {
        (self.claim, self.reserver)
    }
    pub fn seed_id(&self) -> Hash64 {
        self.seed_id
    }
    pub fn execution_root_v1(&self) -> Hash64 {
        self.execution_root
    }
    pub fn total_v1(&self) -> u64 {
        self.template.total_v1()
    }
    pub fn verdict_v1(&self) -> Option<PalwTirPrefixStateVerdictV1> {
        self.verdict
    }
    pub fn collected_v1(&self) -> bool {
        self.collected
    }
    /// The next ordinal `signer`'s slot owes (`0` for an unstarted slot); `None` for a stranger.
    pub fn slot_ordinal_v1(&self, signer: &PalwBondKeyV2) -> Option<u64> {
        if !self.custodians.contains(signer) && *signer != self.reserver {
            return None;
        }
        Some(self.slots.get(signer).map_or(0, PalwTirPrefixStateStreamV1::ordinal_v1))
    }
    pub fn has_response_duty_v1(&self, signer: &PalwBondKeyV2) -> bool {
        !self.collected && self.custodians.contains(signer) && self.slot_ordinal_v1(signer).is_some_and(|o| o < self.total_v1())
    }
    pub fn response_deadline_v1(&self, signer: &PalwBondKeyV2) -> Option<u64> {
        self.custodians.contains(signer).then_some(self.respond_until_daa)
    }

    pub fn check(&self) -> Result<(), String> {
        if self.rent_paid != self.terms.plan.rent_sompi
            || self.deposit_locked != self.terms.plan.deposit_sompi
            || self.moves_used == 0
            || self.moves_used > self.terms.plan.maximum_moves
            || self.respond_until_daa != self.terms.stream_until_daa
            || self.terms.stream_until_daa >= self.terms.collect_until_daa
            || !self.custodians.contains(&self.producer)
            || self.custodians.contains(&self.reserver)
            || self.template.ordinal_v1() != 0
        {
            return Err("TIR prefix court loses its immutable terms, custody or clocks".into());
        }
        self.template.check_v1()?;
        for (signer, slot) in &self.slots {
            slot.check_v1()?;
            if (!self.custodians.contains(signer) && *signer != self.reserver) || !slot.same_contract_v1(&self.template) {
                return Err("TIR prefix slot changes its literal signer or original contract".into());
            }
        }
        let first = self.slots.values().find_map(PalwTirPrefixStateStreamV1::verdict_v1);
        if self.verdict.is_some() != first.is_some() || self.verdict.is_some_and(|v| self.slots.values().all(|s| s.verdict_v1() != Some(v))) {
            return Err("TIR prefix verdict is not a completed slot's".into());
        }
        Ok(())
    }

    /// One authenticated boundary leaf into the answerer's isolated slot. `space` and `context`
    /// are derived natively by the fold (the claim's admitted header, registered program and
    /// published layout); nothing on the carrier names them.
    pub fn answer_v1(
        &mut self,
        now: u64,
        space: &crate::palw_tir_step_v1::PalwTirStepSpaceV1,
        context: &crate::palw_v2::PalwJobContextV2,
        answer: &PalwTirPrefixLeafAnswerV1,
    ) -> Result<(), String> {
        answer.check_wire_v1()?;
        let custodian = self.custodians.contains(&answer.answerer);
        if self.collected
            || answer.seed_id != self.seed_id
            || now < self.reserved_daa
            || (custodian && now >= self.respond_until_daa)
            || (!custodian && answer.answerer != self.reserver)
            || now >= self.terms.stream_until_daa
            || self.slot_ordinal_v1(&answer.answerer) != Some(answer.ordinal)
        {
            return Err("TIR prefix answer substitutes signer, seed, turn or exclusive paid clock".into());
        }
        let moves = self.moves_used.checked_add(1).filter(|m| *m <= self.terms.plan.maximum_moves);
        let moves = moves.ok_or("TIR prefix stream exceeds the claim-admitted cumulative work")?;
        let mut slot = self.slots.get(&answer.answerer).cloned().unwrap_or_else(|| self.template.clone());
        slot.leaf_v1(space, context, &answer.values_le, &answer.opening)?;
        if self.verdict.is_none() {
            self.verdict = slot.verdict_v1();
        }
        self.slots.insert(answer.answerer, slot);
        self.moves_used = moves;
        Ok(())
    }

    /// After the custodians' clock: literal identities that owe their stream, for the native
    /// sanction fold (not a local assertion of guilt).
    pub fn collect_v1(&mut self, now: u64) -> Result<Vec<PalwBondKeyV2>, String> {
        if self.collected || now < self.respond_until_daa || now >= self.terms.collect_until_daa {
            return Err("TIR prefix collection is outside its original exclusive clocks or repeats a sanction".into());
        }
        let total = self.total_v1();
        let missing = self.custodians.iter().filter(|key| self.slot_ordinal_v1(key).is_none_or(|o| o < total)).copied().collect();
        self.collected = true;
        Ok(missing)
    }

    /// `Ok(Some)` when a completed slot re-derived another state root than the job declared,
    /// `Ok(None)` when it re-derived the declared one, `Err` before any slot completed.
    pub fn certificate(&self) -> Result<Option<crate::palw_prefix_owned_court_v1::PalwPrefixCourtFaultV1>, String> {
        self.check()?;
        match self.verdict.ok_or("no TIR prefix slot has completed its authenticated boundary")? {
            PalwTirPrefixStateVerdictV1::Honest => Ok(None),
            PalwTirPrefixStateVerdictV1::StateRootMismatch { .. } => Ok(Some(crate::palw_prefix_owned_court_v1::PalwPrefixCourtFaultV1 {
                claim: self.claim,
                reserver: self.reserver,
                reserved_daa: self.reserved_daa,
                seed_id: self.seed_id,
                execution_root: self.execution_root,
            })),
        }
    }
    /// An honest stage is complete once a slot re-derived the declared root and the custody
    /// clock was collected (every custodian answered or was charged).
    pub fn honest_complete_v1(&self) -> bool {
        self.collected && self.verdict == Some(PalwTirPrefixStateVerdictV1::Honest)
    }

    pub(crate) fn checkpoint_v1(&self) -> PalwOwnedTirPrefixCheckpointV1 {
        PalwOwnedTirPrefixCheckpointV1 {
            slots: self.slots.clone(),
            moves_used: self.moves_used,
            collected: self.collected,
            verdict: self.verdict,
        }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwOwnedTirPrefixCheckpointV1) -> Result<(), String> {
        let mut candidate = self.clone();
        candidate.slots = cp.slots.clone();
        candidate.moves_used = cp.moves_used;
        candidate.collected = cp.collected;
        candidate.verdict = cp.verdict;
        candidate.check()?;
        *self = candidate;
        Ok(())
    }
}

/// The family's resource envelope: every literal slot (custody plus the owner) streams every
/// boundary leaf with its reorg replay, plus the seed and the collection. `leaf_bytes` is the
/// largest boundary leaf's lanes; `depth` the step tree's opening depth.
pub fn tir_prefix_resource_plan_v1(
    total_leaves: u64,
    leaf_bytes: u64,
    depth: u64,
    maximum_custodians: u32,
    prices: crate::palw_rendering_resources_v1::PalwRenderingResourcePricesV1,
) -> Result<crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1, String> {
    if total_leaves == 0
        || leaf_bytes == 0
        || leaf_bytes > PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1 as u64
        || maximum_custodians == 0
        || prices.inclusion_margin_daa == 0
    {
        return Err("TIR prefix quote needs a bounded boundary, leaf and custody envelope".into());
    }
    let n = |x: u128| u64::try_from(x).map_err(|_| "TIR prefix quote overflow".to_string());
    let slots = u128::from(maximum_custodians) + 1;
    let per_slot = u128::from(total_leaves) + u128::from(prices.reorg_replay_moves);
    let moves = per_slot.checked_mul(slots).and_then(|m| m.checked_add(2)).ok_or("TIR prefix move overflow")?;
    let carrier = u128::from(leaf_bytes) + u128::from(depth.min(PALW_TIR_PREFIX_OPENING_DEPTH_V1 as u64)) * 64 + 256;
    let retained = moves * (carrier + 5632);
    // Stored: per slot one continuation (a 128-byte buffer and its words) plus the template.
    let row = (slots + 1) * 1024 + 4096;
    let work = u128::from(total_leaves) * slots * (u128::from(leaf_bytes) + carrier);
    Ok(crate::palw_rendering_resources_v1::PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab: 1,
        maximum_stream_bytes: n(retained)?,
        maximum_stream_leaves: n(u128::from(total_leaves) * slots)?,
        maximum_eog_bytes: 0,
        maximum_moves: n(moves)?,
        maximum_parser_steps: n(work)?,
        maximum_blake_blocks: n(work.div_ceil(128) + moves * 64)?,
        inclusion_daa: n(moves * u128::from(prices.inclusion_margin_daa))?,
        rent_sompi: n(retained * u128::from(prices.rent_per_stream_byte)
            + row * u128::from(prices.rent_per_row_byte)
            + work * u128::from(prices.rent_per_parser_step))?,
        deposit_sompi: n(moves * u128::from(prices.deposit_per_move))?,
        row_bytes: n(row)?,
        resident_ram_bytes: n(row * 2 + carrier * 2 + 65_536)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1;
    use crate::palw_step_leg::step_opening_from_leaf_source_capped_v1;
    use crate::palw_tir_court_v1::test_support::tiny_execution;
    use crate::palw_tir_prefix_state_v1::palw_tir_prefix_kv_digest_v1;
    use crate::palw_tir_step_v1::PalwTirStepSpaceV1;

    fn key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
    }

    /// **G14, TIR prefix-state, from public material only.** The producer and its entire held
    /// Panel collude on a TIR prefix-state claim. Either they withhold, and both are collected at
    /// the original clock. Or they answer: a forged boundary leaf never authenticates against the
    /// claim's step tree, and the claim's own authentic leaves re-derive a state root. That root
    /// convicts a lying declaration and clears an honest one. No step reads a capture, a vote or
    /// a local run.
    #[test]
    fn g14_tir_prefix_state_full_collusion_is_collected_or_convicted_from_the_claims_own_leaves() {
        let x = tiny_execution(None);
        let class_id = x.binding.class.class_id(&x.binding.artifact_root);
        let ctx = x.binding.job_context.clone();
        let space = PalwTirStepSpaceV1::new(&x.binding.class).unwrap();
        // checkpoint_interval 2, h_tile 1, a 3-token prompt: only k = 2 is an aligned boundary.
        let k = 2u32;
        assert!(space.prefix_state_leaf_count_v1(&ctx, 1).is_none(), "an unaligned boundary is refused");
        assert!(space.prefix_state_leaf_count_v1(&ctx, 3).is_none(), "the boundary lies inside the prompt");
        let total = space.prefix_state_leaf_count_v1(&ctx, k).unwrap();
        assert!(total > 0);
        let digest =
            palw_tir_prefix_kv_digest_v1(&space, &ctx, k, |leaf| Ok(x.preimages[leaf.index as usize].values_le.clone())).unwrap();
        let honest_root = palw_fp_prefix_state_root_v1(&class_id, k, &digest);
        let lying_root = palw_fp_prefix_state_root_v1(&class_id, k, &Hash64::from_u64_word(9));
        let count = x.hashes.len() as u64;
        let maximum = 1u64 << 20;
        let opening = |i: u64| step_opening_from_leaf_source_capped_v1(count, i, maximum, |j| x.hashes[j as usize]).unwrap();
        let config = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1();
        let plan = tir_prefix_resource_plan_v1(total, 64, 8, 2, config.prices).unwrap();
        let terms = crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1::derive(
            100,
            plan,
            config.acquisition_daa,
            config.collection_daa,
            config.reorg_retention_daa,
        )
        .unwrap();
        let (producer, panel, owner) = (key(1), key(2), key(9));
        let open = |declared: Hash64| {
            let template = PalwTirPrefixStateStreamV1::new_v1(
                &space,
                class_id,
                &ctx,
                x.binding.step_merkle_root,
                count,
                maximum,
                k,
                declared,
            )
            .unwrap();
            PalwOwnedTirPrefixCourtV1::admit_v1(
                Hash64::from_u64_word(7),
                Hash64::from_u64_word(8),
                owner,
                100,
                x.binding.committed_execution_root,
                terms,
                101,
                u64::MAX,
                producer,
                BTreeSet::from([producer, panel]),
                template,
                terms.material_until_daa,
                terms.bond_exit_after_daa,
                &[terms.bond_exit_after_daa],
            )
            .unwrap()
        };
        let answer = |court: &PalwOwnedTirPrefixCourtV1, signer: PalwBondKeyV2, values_le: Vec<u8>, leaf: u64| PalwTirPrefixLeafAnswerV1 {
            answerer: signer,
            seed_id: court.seed_id(),
            ordinal: court.slot_ordinal_v1(&signer).unwrap(),
            values_le,
            opening: opening(leaf),
        };
        let stream = |court: &mut PalwOwnedTirPrefixCourtV1, signer: PalwBondKeyV2| {
            for ordinal in 0..total {
                let leaf = space.prefix_state_leaf_v1(&ctx, k, ordinal).unwrap();
                let a = answer(court, signer, x.preimages[leaf.index as usize].values_le.clone(), leaf.index);
                court.answer_v1(102, &space, &ctx, &a).unwrap();
            }
        };
        // (1) Total withholding: both custodians are collected; nothing convicts or clears.
        let mut withheld = open(lying_root);
        assert!(withheld.collect_v1(withheld.respond_until_daa - 1).is_err());
        let mut missing = withheld.collect_v1(withheld.respond_until_daa).unwrap();
        missing.sort();
        assert_eq!(missing, vec![producer, panel]);
        assert!(withheld.certificate().is_err() && !withheld.honest_complete_v1());
        // (2) A forged boundary leaf never authenticates; the slot does not move.
        let mut court = open(lying_root);
        let first = space.prefix_state_leaf_v1(&ctx, k, 0).unwrap();
        let mut forged = x.preimages[first.index as usize].values_le.clone();
        forged[0] ^= 1;
        let a = answer(&court, panel, forged, first.index);
        assert!(court.answer_v1(102, &space, &ctx, &a).is_err());
        assert_eq!(court.slot_ordinal_v1(&panel), Some(0));
        // (3) The claim's own authentic leaves convict the lying declaration.
        stream(&mut court, producer);
        assert!(matches!(court.verdict_v1(), Some(PalwTirPrefixStateVerdictV1::StateRootMismatch { .. })));
        assert!(court.certificate().unwrap().is_some());
        assert_eq!(court.collect_v1(court.respond_until_daa).unwrap(), vec![panel], "the silent Panel member is still charged");
        // (4) An honest declaration clears, completing only after the custody clock is collected.
        let mut honest = open(honest_root);
        stream(&mut honest, panel);
        assert_eq!(honest.verdict_v1(), Some(PalwTirPrefixStateVerdictV1::Honest));
        assert!(honest.certificate().unwrap().is_none() && !honest.honest_complete_v1());
        let restored = honest.checkpoint_v1();
        let mut replayed = open(honest_root);
        replayed.restore_checkpoint_v1(&restored).unwrap();
        assert_eq!(replayed, honest, "the stored slots restart exactly");
        assert_eq!(honest.collect_v1(honest.respond_until_daa).unwrap(), vec![producer]);
        assert!(honest.honest_complete_v1());
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_tir_prefix_owned_v1.rs"]
mod native_resource_inventory_v1;
