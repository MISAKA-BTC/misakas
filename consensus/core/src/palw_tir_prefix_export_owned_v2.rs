//! Distinct producer-only source-export court. No old consumed-prefix seed,
//! row, certificate or responsibility is reinterpreted. Registration, native
//! admission and original all-branch funding are supplied by the owning fold.
use crate::Hash64;
use crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_tir_prefix_export_relation_v2::{TirExportPrefixStreamV2, TirExportTargetV2};
use crate::palw_tir_prefix_owned_v1::PalwTirPrefixLeafAnswerV1;
use crate::palw_tir_prefix_state_v1::PalwTirPrefixStateVerdictV1;
use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

/// Both allocations and the original frozen policy are resolved by the fold.
/// A declaration of these IDs is never evidence that funding was accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TirExportCourtSeedV2 {
    pub version: u16,
    pub target: TirExportTargetV2,
    pub export_budget: Hash64,
    pub source_allocation: Hash64,
    pub court_allocation: Hash64,
    pub policy: Hash64,
    pub owner: PalwBondKeyV2,
}
impl TirExportCourtSeedV2 {
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.version != 2
            || self.target.version != 2
            || self.export_budget == Hash64::default()
            || self.source_allocation == Hash64::default()
            || self.court_allocation == Hash64::default()
            || self.policy == Hash64::default()
            || self.target.source.accepted_block == Hash64::default()
            || self.target.source.intent == Hash64::default()
            || self.target.source.source_claim == Hash64::default()
            || self.target.prefix.is_genesis()
            || self.target.material_until == u64::MAX
            || self.target.liability_until < self.target.source.accepted_daa
            || self.target.material_until < self.target.liability_until
        {
            return Err("source-export seed changes its purpose, accepted subject or finite allocations".into());
        }
        Ok(())
    }
    pub(crate) fn id(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/source-export/court-seed/v2", self))
    }
    /// Full accepted identity, ordinal, owner and independent funded draft are
    /// in this key. Neither raw ID nor (claim, owner) can replace it.
    pub(crate) fn key(&self) -> Hash64 {
        Hash64::from_bytes(misaka_palw_kernel::hash::object_id(b"misaka-palw/source-export/court-key/v2", self))
    }
}

/// The nested V1 type is only its bounded leaf/opening representation. The
/// new versioned purpose/key is carried explicitly; no old carrier tag changes.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TirExportLeafMoveV2 {
    pub version: u16,
    pub court: Hash64,
    pub answer: PalwTirPrefixLeafAnswerV1,
}

/// Returned only from a completed rooted court. It is NOT a generic Prefix
/// computation fault and cannot be used to charge the source's old approvers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TirExportMismatchV2 {
    pub version: u16,
    pub court: Hash64,
    pub seed: Hash64,
    pub target: TirExportTargetV2,
    pub producer: PalwBondKeyV2,
    pub derived: Hash64,
}

/// Version-2 evidence references an actually rooted completed export court.
/// The evidence never contains a producer-supplied row or outcome. The native
/// dispatcher must fetch the row at `court`, then resolve its exact born source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct TirExportRefutationV2 {
    pub version: u8,
    pub court: Hash64,
    pub seed: Hash64,
}
impl TirExportRefutationV2 {
    /// `rooted` is fetched by the ordered fold, not decoded from this evidence.
    pub(crate) fn resolve_rooted(
        &self,
        rooted: &OwnedTirExportCourtV2,
        accused: PalwBondKeyV2,
        now_daa: u64,
        now_blue: u64,
    ) -> Result<Option<TirExportMismatchV2>, String> {
        if self.version != 2
            || self.court != rooted.key()
            || self.seed != rooted.seed.id()
            || accused != rooted.producer
            || now_daa < rooted.seed.target.source.accepted_daa
            || now_blue <= rooted.seed.target.source.accepted_blue_score
            || now_daa > rooted.seed.target.liability_until
        {
            return Err("source-export refutation changes rooted purpose, producer, source birth or signed liability clock".into());
        }
        rooted.mismatch()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct OwnedTirExportCourtV2 {
    seed: TirExportCourtSeedV2,
    producer: PalwBondKeyV2,
    reserved_daa: u64,
    terms: PalwRenderingClaimTermsV1,
    rent_paid: u64,
    deposit_locked: u64,
    moves_used: u64,
    custodians: BTreeSet<PalwBondKeyV2>,
    template: TirExportPrefixStreamV2,
    slots: BTreeMap<PalwBondKeyV2, TirExportPrefixStreamV2>,
    collected: bool,
    verdict: Option<PalwTirPrefixStateVerdictV1>,
}
impl OwnedTirExportCourtV2 {
    /// Call only with an owner-derived source/export/template, accepted funding
    /// and the actual separately bound material custodians. The old Gate C
    /// approval alone does not enlist a signer in a new export service duty.
    /// Reserve map membership, template/slot clones and encoded rows BEFORE call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn admit(
        seed: TirExportCourtSeedV2,
        producer: PalwBondKeyV2,
        reserved_daa: u64,
        terms: PalwRenderingClaimTermsV1,
        now: u64,
        funding: u64,
        custodians: BTreeSet<PalwBondKeyV2>,
        template: TirExportPrefixStreamV2,
        material_until: u64,
        producer_exit: u64,
        custody_exits: &[u64],
    ) -> Result<Self, String> {
        seed.check()?;
        template.check()?;
        if now < terms.accepted_daa
            || now >= terms.open_until_daa
            || reserved_daa > now
            || reserved_daa < seed.target.source.accepted_daa
            || template.target() != seed.target
            || template.ordinal() != 0
            || !custodians.contains(&producer)
            || custodians.contains(&seed.owner)
            || custody_exits.len() != custodians.len().saturating_sub(1)
            || terms.collect_until_daa > seed.target.liability_until
            || terms.material_until_daa > seed.target.material_until
        {
            return Err("source-export court changes its accepted source, actual custody or signed finite clocks".into());
        }
        // Export service custody is the actually signed set. It may consist
        // solely of its producer; an old computation panel is not implicitly
        // enlisted. Each real custodian still needs its full accepted exit.
        if material_until < terms.material_until_daa
            || producer_exit < terms.bond_exit_after_daa
            || custody_exits.iter().any(|at| *at < terms.bond_exit_after_daa)
        {
            return Err("source-export lacks actual material or signed custodian exit coverage".into());
        }
        let required = terms.plan.rent_sompi.checked_add(terms.plan.deposit_sompi).ok_or("source-export funding overflow")?;
        if funding < required {
            return Err("source-export court lacks its full accepted funding".into());
        }
        let row = Self {
            seed,
            producer,
            reserved_daa,
            terms,
            rent_paid: terms.plan.rent_sompi,
            deposit_locked: terms.plan.deposit_sompi,
            moves_used: 1,
            custodians,
            template,
            slots: BTreeMap::new(),
            collected: false,
            verdict: None,
        };
        row.check()?;
        Ok(row)
    }
    pub(crate) fn seed(&self) -> TirExportCourtSeedV2 {
        self.seed
    }
    pub(crate) fn key(&self) -> Hash64 {
        self.seed.key()
    }
    pub(crate) fn total(&self) -> u64 {
        self.template.total()
    }
    pub(crate) fn ordinal(&self, who: &PalwBondKeyV2) -> Option<u64> {
        (self.custodians.contains(who) || *who == self.seed.owner)
            .then(|| self.slots.get(who).map_or(0, TirExportPrefixStreamV2::ordinal))
    }
    pub(crate) fn check(&self) -> Result<(), String> {
        self.seed.check()?;
        self.template.check()?;
        let slots = u64::try_from(self.custodians.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or("source-export literal slot count overflow")?;
        let leaves = self.total().checked_mul(slots).ok_or("source-export cumulative leaf count overflow")?;
        let minimum_moves = leaves.checked_add(2).ok_or("source-export cumulative control count overflow")?;
        let observed_moves = self
            .slots
            .values()
            .try_fold(1u64 + u64::from(self.collected), |n, slot| n.checked_add(slot.ordinal()))
            .ok_or("source-export observed control count overflow")?;
        let required_stream_end = self.terms.plan.deadline(self.terms.open_until_daa)?;
        if self.template.target() != self.seed.target
            || self.template.ordinal() != 0
            || self.moves_used == 0
            || self.moves_used > self.terms.plan.maximum_moves
            || self.moves_used != observed_moves
            || self.terms.plan.maximum_moves < minimum_moves
            || self.terms.plan.maximum_stream_leaves < leaves
            || self.terms.plan.inclusion_daa == 0
            || self.terms.stream_until_daa < required_stream_end
            || self.rent_paid != self.terms.plan.rent_sompi
            || self.deposit_locked != self.terms.plan.deposit_sompi
            || self.terms.stream_until_daa >= self.terms.collect_until_daa
            || self.terms.accepted_daa < self.seed.target.source.accepted_daa
            || self.reserved_daa < self.terms.accepted_daa
            || self.reserved_daa >= self.terms.open_until_daa
            || self.terms.accepted_daa >= self.terms.open_until_daa
            || self.terms.open_until_daa >= self.terms.stream_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa > self.terms.bond_exit_after_daa
            || self.terms.collect_until_daa > self.seed.target.liability_until
            || self.terms.material_until_daa > self.seed.target.material_until
            || !self.custodians.contains(&self.producer)
            || self.custodians.contains(&self.seed.owner)
            || self.slots.len() > self.custodians.len().saturating_add(1)
        {
            return Err("source-export court loses its immutable purpose, custody, funding or progress".into());
        }
        for (who, slot) in &self.slots {
            slot.check()?;
            if self.ordinal(who).is_none() || !slot.same_contract(&self.template) {
                return Err("source-export slot changes its original source contract or answerer".into());
            }
        }
        if self.verdict.is_some() != self.slots.values().any(|slot| slot.verdict().is_some())
            || self.verdict.is_some_and(|v| self.slots.values().all(|slot| slot.verdict() != Some(v)))
        {
            return Err("source-export outcome is not a completed authenticated slot".into());
        }
        Ok(())
    }
    pub(crate) fn apply_leaf(
        &mut self,
        now: u64,
        space: &crate::palw_tir_step_v1::PalwTirStepSpaceV1,
        context: &crate::palw_v2::PalwJobContextV2,
        movement: &TirExportLeafMoveV2,
    ) -> Result<(), String> {
        self.check()?;
        movement.answer.check_wire_v1()?;
        let a = &movement.answer;
        if movement.version != 2
            || movement.court != self.key()
            || a.seed_id != self.seed.id()
            || self.collected
            || now < self.reserved_daa
            || now >= self.terms.stream_until_daa
            || self.ordinal(&a.answerer) != Some(a.ordinal)
        {
            return Err("source-export move substitutes purpose, court, answerer, turn or exclusive clock".into());
        }
        let moves = self
            .moves_used
            .checked_add(1)
            .filter(|n| *n <= self.terms.plan.maximum_moves)
            .ok_or("source-export move exceeds its original accepted cumulative budget")?;
        let mut slot = self.slots.get(&a.answerer).cloned().unwrap_or_else(|| self.template.clone());
        slot.leaf(
            space,
            context,
            &a.values_le,
            &a.opening,
            crate::palw_tir_prefix_owned_v1::PALW_TIR_PREFIX_LEAF_MAX_BYTES_V1 as u64,
        )?;
        if self.verdict.is_none() {
            self.verdict = slot.verdict();
        }
        self.slots.insert(a.answerer, slot);
        self.moves_used = moves;
        Ok(())
    }
    /// Default collection concerns separately accepted custody service only.
    /// It never manufactures a prefix mismatch from missing material.
    pub(crate) fn collect(&mut self, now: u64) -> Result<Vec<PalwBondKeyV2>, String> {
        self.check()?;
        if self.collected || now < self.terms.stream_until_daa || now >= self.terms.collect_until_daa {
            return Err("source-export collection repeats or misses its paid exclusive clock".into());
        }
        let moves = self
            .moves_used
            .checked_add(1)
            .filter(|n| *n <= self.terms.plan.maximum_moves)
            .ok_or("source-export collection exceeds its original accepted cumulative budget")?;
        let missing = self.custodians.iter().filter(|who| self.ordinal(who).is_none_or(|n| n < self.total())).copied().collect();
        self.collected = true;
        self.moves_used = moves;
        Ok(missing)
    }
    /// Native resolver re-loads THIS rooted row and exact accepted source before
    /// dispatching producer-only PrefixExportRefuted. No caller verdict is used.
    pub(crate) fn mismatch(&self) -> Result<Option<TirExportMismatchV2>, String> {
        self.check()?;
        match self.verdict.ok_or("source-export has no completed authenticated boundary")? {
            PalwTirPrefixStateVerdictV1::Honest => Ok(None),
            PalwTirPrefixStateVerdictV1::StateRootMismatch { derived } => Ok(Some(TirExportMismatchV2 {
                version: 2,
                court: self.key(),
                seed: self.seed.id(),
                target: self.seed.target,
                producer: self.producer,
                derived,
            })),
        }
    }
}

crate::palw_native_resource_v1::native_struct!(TirExportCourtSeedV2; version,target,export_budget,source_allocation,court_allocation,policy,owner);
crate::palw_native_resource_v1::wire::wire_struct!(TirExportCourtSeedV2; u16,TirExportTargetV2,Hash64,Hash64,Hash64,Hash64,PalwBondKeyV2);
crate::palw_native_resource_v1::native_struct!(TirExportLeafMoveV2; version,court,answer);
crate::palw_native_resource_v1::wire::wire_struct!(TirExportLeafMoveV2; u16,Hash64,PalwTirPrefixLeafAnswerV1);
crate::palw_native_resource_v1::native_struct!(TirExportMismatchV2; version,court,seed,target,producer,derived);
crate::palw_native_resource_v1::wire::wire_struct!(TirExportMismatchV2; u16,Hash64,Hash64,TirExportTargetV2,PalwBondKeyV2,Hash64);
crate::palw_native_resource_v1::native_struct!(TirExportRefutationV2; version,court,seed);
crate::palw_native_resource_v1::wire::wire_struct!(TirExportRefutationV2; u8,Hash64,Hash64);
crate::palw_native_resource_v1::native_struct!(OwnedTirExportCourtV2; seed,producer,reserved_daa,terms,rent_paid,deposit_locked,moves_used,custodians,template,slots,collected,verdict);
crate::palw_native_resource_v1::wire::wire_struct!(OwnedTirExportCourtV2; TirExportCourtSeedV2,PalwBondKeyV2,u64,PalwRenderingClaimTermsV1,u64,u64,u64,BTreeSet<PalwBondKeyV2>,TirExportPrefixStreamV2,BTreeMap<PalwBondKeyV2,TirExportPrefixStreamV2>,bool,Option<PalwTirPrefixStateVerdictV1>);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportIdentityV1;
    use crate::palw_fp_prefix_v1::palw_fp_prefix_state_root_v1;
    use crate::palw_freeprompt_v3::PalwFpPrefixStateV1;
    use crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1;
    use crate::palw_step_leg::step_opening_from_leaf_source_capped_v1;
    use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};
    use crate::palw_tir_prefix_owned_v1::tir_prefix_resource_plan_v1;
    use crate::palw_tir_prefix_state_v1::palw_tir_prefix_kv_digest_v1;
    use crate::palw_tir_step_v1::PalwTirStepSpaceV1;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(n), 0))
    }
    struct Fixture {
        x: TinyExecution,
        space: PalwTirStepSpaceV1,
        header: PalwPublicTirBindingHeaderV1,
        seed: TirExportCourtSeedV2,
        terms: PalwRenderingClaimTermsV1,
        honest: Hash64,
    }
    impl Fixture {
        fn new() -> Self {
            let x = tiny_execution(None);
            let space = PalwTirStepSpaceV1::new(&x.binding.class).unwrap();
            let header = PalwPublicTirBindingHeaderV1::of_binding_v1(&x.binding);
            let class = header.job_context.shape_profile_id;
            let k = 2;
            assert!(k <= header.job_context.declared_prefill_tokens);
            let total = space.export_prefix_state_leaf_count_v2(&header.job_context, k).unwrap();
            let digest = palw_tir_prefix_kv_digest_v1(&space, &header.job_context, k, |leaf| {
                Ok(x.preimages[leaf.index as usize].values_le.clone())
            })
            .unwrap();
            let honest = palw_fp_prefix_state_root_v1(&class, k, &digest);
            let config = crate::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1::network_v1();
            let leaf_bytes = x.preimages.iter().map(|leaf| leaf.values_le.len() as u64).max().unwrap();
            let plan = tir_prefix_resource_plan_v1(total, leaf_bytes, 8, 2, config.prices).unwrap();
            let terms = PalwRenderingClaimTermsV1::derive(
                100,
                plan,
                config.acquisition_daa,
                config.collection_daa,
                config.reorg_retention_daa,
            )
            .unwrap();
            let seed = TirExportCourtSeedV2 {
                version: 2,
                target: TirExportTargetV2 {
                    version: 2,
                    source: PalwPrefixExportIdentityV1 {
                        intent: h(40),
                        source_claim: h(41),
                        accepted_block: h(42),
                        accepted_daa: 100,
                        accepted_blue_score: 5,
                    },
                    export_ordinal: 0,
                    prefix: PalwFpPrefixStateV1 {
                        class_id: class,
                        prefix_tokens: k,
                        state_root: palw_fp_prefix_state_root_v1(&class, k, &h(99)),
                    },
                    source_execution_root: header.committed_execution_root,
                    liability_until: terms.collect_until_daa,
                    material_until: terms.material_until_daa,
                },
                export_budget: h(43),
                source_allocation: h(44),
                court_allocation: h(45),
                policy: h(46),
                owner: key(9),
            };
            Self { x, space, header, seed, terms, honest }
        }
        fn open_seed(&self, seed: TirExportCourtSeedV2) -> OwnedTirExportCourtV2 {
            let template = TirExportPrefixStreamV2::from_authenticated_v2(&self.space, &self.header, seed.target, 1 << 20).unwrap();
            OwnedTirExportCourtV2::admit(
                seed,
                key(1),
                100,
                self.terms,
                101,
                self.terms.plan.rent_sompi.checked_add(self.terms.plan.deposit_sompi).unwrap(),
                BTreeSet::from([key(1), key(2)]),
                template,
                self.terms.material_until_daa,
                self.terms.bond_exit_after_daa,
                &[self.terms.bond_exit_after_daa],
            )
            .unwrap()
        }
        fn open(&self, honest: bool) -> OwnedTirExportCourtV2 {
            let mut seed = self.seed;
            if honest {
                seed.target.prefix.state_root = self.honest;
            }
            self.open_seed(seed)
        }
        fn movement(&self, court: &OwnedTirExportCourtV2, who: PalwBondKeyV2) -> TirExportLeafMoveV2 {
            let ordinal = court.ordinal(&who).unwrap();
            let leaf = self
                .space
                .export_prefix_state_leaf_v2(&self.header.job_context, court.seed.target.prefix.prefix_tokens, ordinal)
                .unwrap();
            TirExportLeafMoveV2 {
                version: 2,
                court: court.key(),
                answer: PalwTirPrefixLeafAnswerV1 {
                    answerer: who,
                    seed_id: court.seed.id(),
                    ordinal,
                    values_le: self.x.preimages[leaf.index as usize].values_le.clone(),
                    opening: step_opening_from_leaf_source_capped_v1(self.header.step_leaf_count, leaf.index, 1 << 20, |i| {
                        self.x.hashes[i as usize]
                    })
                    .unwrap(),
                },
            }
        }
        fn finish(&self, court: &mut OwnedTirExportCourtV2, who: PalwBondKeyV2) {
            for _ in 0..court.total() {
                let movement = self.movement(court, who);
                court.apply_leaf(102, &self.space, &self.header.job_context, &movement).unwrap();
            }
        }
    }

    #[test]
    fn rfc40_tir_export_owned_outside_completion_decides_truth_without_waiving_custody_default() {
        let f = Fixture::new();
        let mut withheld = f.open(false);
        assert!(withheld.mismatch().is_err());
        assert!(withheld.collect(f.terms.stream_until_daa - 1).is_err());
        assert_eq!(withheld.collect(f.terms.stream_until_daa).unwrap(), vec![key(1), key(2)]);
        assert!(withheld.mismatch().is_err(), "missing service never manufactures a prefix truth verdict");
        let mut court = f.open(false);
        f.finish(&mut court, key(9));
        let mismatch = court.mismatch().unwrap().unwrap();
        assert_eq!(
            (mismatch.version, mismatch.court, mismatch.seed, mismatch.target, mismatch.producer, mismatch.derived),
            (2, court.key(), court.seed.id(), court.seed.target, key(1), f.honest)
        );
        assert_eq!(court.ordinal(&key(1)), Some(0));
        assert_eq!(court.ordinal(&key(2)), Some(0));
        assert_eq!(
            court.collect(f.terms.stream_until_daa).unwrap(),
            vec![key(1), key(2)],
            "an outside completed truth stream never discharges separately bound custody service"
        );
        assert!(court.collect(f.terms.stream_until_daa).is_err());
        let restored: OwnedTirExportCourtV2 = borsh::from_slice(&borsh::to_vec(&court).unwrap()).unwrap();
        assert_eq!(restored, court);
        assert_eq!(restored.mismatch().unwrap(), Some(mismatch));
        let mut honest = f.open(true);
        f.finish(&mut honest, key(2));
        assert_eq!(honest.mismatch().unwrap(), None);
        assert_eq!(honest.collect(f.terms.stream_until_daa).unwrap(), vec![key(1)]);
    }

    #[test]
    fn rfc40_tir_export_owned_wrong_scope_opening_capacity_or_clock_never_advances() {
        let f = Fixture::new();
        for kind in 0..9 {
            let mut court = f.open(false);
            let mut movement = f.movement(&court, key(1));
            match kind {
                0 => movement.version = 1,
                1 => movement.court = h(70),
                2 => movement.answer.seed_id = h(71),
                3 => movement.answer.ordinal = 1,
                4 => movement.answer.answerer = key(77),
                5 => movement.answer.opening.leaf_hash = h(72),
                6 => movement.answer.values_le[0] ^= 1,
                7 => court.terms.plan.maximum_moves = 1,
                _ => movement.answer.opening.leaf_index += 1,
            }
            let before = court.clone();
            assert!(court.apply_leaf(102, &f.space, &f.header.job_context, &movement).is_err(), "refusal case {kind}");
            assert_eq!(court, before, "refusal case {kind} must not consume ordinal, budget or verdict");
        }
        for now in [99, f.terms.stream_until_daa, f.terms.collect_until_daa] {
            let mut court = f.open(false);
            let movement = f.movement(&court, key(9));
            let before = court.clone();
            assert!(court.apply_leaf(now, &f.space, &f.header.job_context, &movement).is_err());
            assert_eq!(court, before);
        }
        let mut court = f.open(false);
        let movement = f.movement(&court, key(1));
        let mut context = f.header.job_context.clone();
        context.job_id = h(73);
        let before = court.clone();
        assert!(court.apply_leaf(102, &f.space, &context, &movement).is_err());
        assert_eq!(court, before);
    }

    #[test]
    fn rfc40_tir_export_owned_birth_export_ordinal_owner_and_funding_allocation_have_distinct_keys() {
        let f = Fixture::new();
        let original = f.open(false);
        let movement = f.movement(&original, key(1));
        let mut keys = BTreeSet::from([original.key()]);
        for kind in 0..4 {
            let mut seed = f.seed;
            match kind {
                0 => {
                    seed.target.source.accepted_block = h(80);
                    seed.target.source.accepted_blue_score += 1;
                }
                1 => seed.target.export_ordinal += 1,
                2 => seed.owner = key(8),
                _ => seed.court_allocation = h(81),
            }
            assert_eq!(seed.target.source.source_claim, original.seed.target.source.source_claim);
            let mut other = f.open_seed(seed);
            assert!(keys.insert(other.key()));
            assert_ne!(other.seed.id(), original.seed.id());
            let before = other.clone();
            assert!(other.apply_leaf(102, &f.space, &f.header.job_context, &movement).is_err());
            assert_eq!(other, before);
        }
    }

    #[test]
    fn rfc40_tir_export_owned_admission_keeps_actual_funding_and_finite_holds_mandatory() {
        let f = Fixture::new();
        for kind in 0..7 {
            let mut seed = f.seed;
            let mut now = 101;
            let mut funding = f.terms.plan.rent_sompi.checked_add(f.terms.plan.deposit_sompi).unwrap();
            let mut material = f.terms.material_until_daa;
            let mut producer_exit = f.terms.bond_exit_after_daa;
            let mut custody_exit = f.terms.bond_exit_after_daa;
            match kind {
                0 => now = 99,
                1 => now = f.terms.open_until_daa,
                2 => funding -= 1,
                3 => material -= 1,
                4 => producer_exit -= 1,
                5 => custody_exit -= 1,
                _ => seed.source_allocation = Hash64::default(),
            }
            let template = TirExportPrefixStreamV2::from_authenticated_v2(&f.space, &f.header, seed.target, 1 << 20).unwrap();
            assert!(
                OwnedTirExportCourtV2::admit(
                    seed,
                    key(1),
                    100,
                    f.terms,
                    now,
                    funding,
                    BTreeSet::from([key(1), key(2)]),
                    template,
                    material,
                    producer_exit,
                    &[custody_exit]
                )
                .is_err(),
                "native admission refusal {kind}"
            );
        }
    }

    #[test]
    fn rfc40_tir_export_owned_producer_only_custody_does_not_enlist_an_old_computation_panel() {
        let f = Fixture::new();
        let template = TirExportPrefixStreamV2::from_authenticated_v2(&f.space, &f.header, f.seed.target, 1 << 20).unwrap();
        let actual_material_until = f.seed.target.material_until.checked_add(100).unwrap();
        let mut court = OwnedTirExportCourtV2::admit(
            f.seed,
            key(1),
            100,
            f.terms,
            101,
            f.terms.plan.rent_sompi.checked_add(f.terms.plan.deposit_sompi).unwrap(),
            BTreeSet::from([key(1)]),
            template,
            actual_material_until,
            f.terms.bond_exit_after_daa,
            &[],
        )
        .unwrap();
        assert_eq!(court.ordinal(&key(2)), None, "old computation membership is not an export custody consent");
        let absent_panel = f.movement(&court, key(1));
        let mut forged = absent_panel.clone();
        forged.answer.answerer = key(2);
        let before = court.clone();
        assert!(court.apply_leaf(102, &f.space, &f.header.job_context, &forged).is_err());
        assert_eq!(court, before);
        f.finish(&mut court, key(9));
        assert_eq!(court.mismatch().unwrap().unwrap().producer, key(1));
        let used = court.moves_used;
        assert_eq!(court.collect(f.terms.stream_until_daa).unwrap(), vec![key(1)]);
        assert_eq!(court.moves_used, used + 1, "the accepted collection consumes its own paid move");
        assert!(court.check().is_ok());
    }

    #[test]
    fn rfc40_tir_export_owned_collection_cannot_exceed_capacity_or_restore_invalid_clock_order() {
        let f = Fixture::new();
        let mut court = f.open(false);
        court.terms.plan.maximum_moves = 1;
        let before = court.clone();
        assert!(court.collect(f.terms.stream_until_daa).is_err());
        assert_eq!(court, before, "collection capacity refusal leaves both paid cursor and collected bit unchanged");
        for kind in 0..6 {
            let mut row = f.open(false);
            match kind {
                0 => row.terms.accepted_daa = row.seed.target.source.accepted_daa - 1,
                1 => row.reserved_daa = row.terms.open_until_daa,
                2 => row.terms.accepted_daa = row.terms.open_until_daa,
                3 => row.terms.open_until_daa = row.terms.stream_until_daa,
                4 => row.terms.collect_until_daa = row.terms.material_until_daa + 1,
                _ => row.terms.bond_exit_after_daa = row.terms.material_until_daa - 1,
            }
            assert!(row.check().is_err(), "restored clock ordering case {kind}");
        }
    }

    #[test]
    fn rfc40_tir_export_owned_full_aligned_prefill_boundary_uses_honest_source_execution() {
        use crate::palw_step_leg::{step_merkle_root_v1, step_tile_leaf_hash_v1};
        use crate::palw_step_refute::{base0_decode_token_select_v1, tiled_logits_trace_root_v1};
        use crate::palw_tir_step_v1::{PalwTirLeafKindV1, palw_tir_execution_root_v1, verify_tir_binding_v1};
        let mut f = Fixture::new();
        // Re-execute this SAME tiny program from its actual artifact bytes.
        // Relative coord.position is not an absolute execution position; also,
        // the old P=3 job did not encode the post/logits leaf needed at new P=2's
        // final prefill position. Filtering old preimages cannot build this job.
        use misaka_palw_tir::{Interpreter, MapParams, RunState, Tensor};
        let mut bytes_of = std::collections::BTreeMap::<(u16, Option<u16>), Vec<u8>>::new();
        for operand in &f.x.ops {
            let param = f
                .space
                .program
                .params
                .iter()
                .position(|p| p.name == operand.tensor_name)
                .expect("the real tiny artifact names a declared parameter");
            let data = bytes_of.entry((u16::try_from(param).unwrap(), operand.layer)).or_default();
            assert_eq!(operand.row_start as usize, data.len(), "actual artifact pieces are contiguous bytes");
            data.extend_from_slice(&operand.bytes);
        }
        let mut params = MapParams::default();
        for ((param, layer), data) in bytes_of {
            let declaration = &f.space.program.params[param as usize];
            let shape = declaration.shape.iter().map(|d| *d as usize).collect::<Vec<_>>();
            params.tensors.insert((param, layer), Tensor::from_le_bytes(declaration.dtype, &shape, &data).unwrap());
        }
        f.x.prompt.truncate(2);
        f.x.binding.job_context.declared_prefill_tokens = 2;
        f.x.binding.job_context.exact_decode_tokens = 1;
        f.x.binding.job_context.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(&f.x.prompt);
        let ctx = f.x.binding.job_context.clone();
        let interp = Interpreter::new(&f.space.program).unwrap();
        let mut state = RunState::default();
        let (mut commits, mut after) = (Vec::new(), Vec::new());
        f.x.rows.clear();
        f.x.generated.clear();
        for (position, token) in f.x.prompt.iter().copied().enumerate() {
            let step = interp.step(&params, &mut state, token).expect("genuine P=2 causal execution");
            if position + 1 == f.x.prompt.len() {
                let row = step.logits.data.iter().map(|v| i32::try_from(*v).unwrap()).collect::<Vec<_>>();
                f.x.generated.push(base0_decode_token_select_v1(&row) as u32);
                f.x.rows.push(row);
            }
            commits.push(step.commits);
            after.push(state.clone());
        }
        assert_eq!(f.x.rows.len(), ctx.exact_decode_tokens as usize);
        let occurrences = f.space.occurrences();
        f.x.preimages.clear();
        for position in 0..ctx.declared_prefill_tokens {
            for leaf in f.space.leaves_of_position(&ctx, position) {
                let count = leaf.value_count as usize;
                let values = match leaf.kind {
                    PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                        let (block, layer) = occurrences[occurrence as usize];
                        let value = &commits[position as usize]
                            .iter()
                            .find(|c| c.block == block && c.layer == layer && c.node == node)
                            .expect("the interpreter produced this canonical commit")
                            .value;
                        value.data[first_element as usize..first_element as usize + count].to_vec()
                    }
                    PalwTirLeafKindV1::State { state: slot, layer, first_element, .. } => {
                        let value = after[position as usize].fixed.get(&(slot, layer)).unwrap();
                        value.data[first_element as usize..first_element as usize + count].to_vec()
                    }
                    PalwTirLeafKindV1::HistTile { .. } => unreachable!("this tiny class has fixed state only"),
                };
                f.x.preimages.push(crate::palw_tir_step_v1::palw_tir_leaf_preimage_v1(&leaf, &values).unwrap());
            }
        }
        let count = f.space.leaf_count_capped(&ctx, 1 << 20).unwrap();
        assert_eq!(f.x.preimages.len() as u64, count);
        for (index, preimage) in f.x.preimages.iter().enumerate() {
            let canonical = f.space.leaf_at(&ctx, index as u64).unwrap();
            assert_eq!(preimage.coord, canonical.coord);
            assert_eq!(preimage.value_count, canonical.value_count);
        }
        let class = ctx.shape_profile_id;
        f.x.hashes = f.x.preimages.iter().map(|leaf| step_tile_leaf_hash_v1(&ctx.context_hash(), &class, leaf)).collect();
        f.x.binding.step_leaf_count = count;
        f.x.binding.step_merkle_root = step_merkle_root_v1(&f.x.hashes).unwrap();
        f.x.binding.full_logits_trace_root = tiled_logits_trace_root_v1(&ctx, &f.x.rows, &f.x.generated).unwrap();
        f.x.binding.committed_execution_root = palw_tir_execution_root_v1(
            &ctx.context_hash(),
            &f.x.binding.full_logits_trace_root,
            &class,
            count,
            &f.x.binding.step_merkle_root,
        );
        verify_tir_binding_v1(&f.x.binding, 1 << 20).unwrap();
        let rules = crate::palw_tir_court_v1::PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 20,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
        };
        for index in 0..count {
            let refutation = crate::palw_tir_court_v1::build_tir_cone_refutation_v1(&f.x.binding, index, &f.x, &rules).unwrap();
            assert_eq!(
                crate::palw_tir_court_v1::check_tir_cone_refutation_v1(&refutation, &rules),
                Err(crate::palw_step_refute::PalwStepRefuteError::NoFaultFound),
                "genuine new-job leaf {index}"
            );
        }
        f.header = PalwPublicTirBindingHeaderV1::of_binding_v1(&f.x.binding);
        f.seed.target.source_execution_root = f.header.committed_execution_root;
        assert_eq!(f.seed.target.prefix.prefix_tokens, ctx.declared_prefill_tokens);
        assert_eq!(f.space.prefix_state_leaf_count_v1(&ctx, 2), None, "legacy consumed-prefix meaning is unchanged");
        assert!(f.space.export_prefix_state_leaf_count_v2(&ctx, 2).is_some());
        let mut lying = f.open(false);
        f.finish(&mut lying, key(9));
        assert_eq!(lying.mismatch().unwrap().unwrap().derived, f.honest);
        let mut honest = f.open(true);
        f.finish(&mut honest, key(1));
        assert_eq!(honest.mismatch().unwrap(), None);
    }
}
