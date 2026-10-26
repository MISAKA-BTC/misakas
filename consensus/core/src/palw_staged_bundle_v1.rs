//! Bounded required-family work owned by one literal public verifier. Old court
//! variants remain wire-identical; bundle children cannot recursively be bundles.
use crate::palw_rendering_resources_v1::{PalwRenderingClaimTermsV1, PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1};
use crate::palw_staged_court_state_v1::PalwStagedCheckpointV1;
use crate::palw_staged_court_state_v1::{PalwStagedClaimContractV1, PalwStagedCourtKeyV1, PalwStagedCourtRowV1};
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_STAGED_REQUIRED_FAMILIES_V1: usize = 3;
/// Full-row child admission and activation retain every original context. The
/// bound covers all three stages' reversible row replacements, not just the seed.
pub fn staged_context_resource_plan_v1(
    profile_bytes: u64,
    semantic: PalwRenderingResourcePlanV1,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<PalwRenderingResourcePlanV1, String> {
    let n = |x: u128| u64::try_from(x).map_err(|_| "retained stage context resource overflow".to_string());
    let context = u128::from(profile_bytes)
        + 90_000
        + crate::palw_freeprompt_v3::PALW_FP_COMMITMENT_TX_MAX_BYTES as u128
        + crate::palw_prefix_history_v1::PALW_STAGED_CACHE_LEASE_STORAGE_BYTES_V1 as u128
        + 4096;
    let storage = (context + u128::from(semantic.row_bytes)) * 24;
    let work = context * 8 + 4096;
    Ok(PalwRenderingResourcePlanV1 {
        prefix_tokens: 0,
        vocab: 0,
        maximum_stream_bytes: 180_000,
        maximum_stream_leaves: 0,
        maximum_eog_bytes: 0,
        maximum_moves: 2,
        maximum_parser_steps: n(work)?,
        maximum_blake_blocks: n(work.div_ceil(128))?,
        inclusion_daa: n(2 * u128::from(config.prices.inclusion_margin_daa))?,
        rent_sompi: n(180_000 * u128::from(config.prices.rent_per_stream_byte)
            + work * u128::from(config.prices.rent_per_parser_step)
            + storage * u128::from(config.prices.rent_per_row_byte))?,
        deposit_sompi: n(2 * u128::from(config.prices.deposit_per_move))?,
        row_bytes: n(storage)?,
        resident_ram_bytes: n(context * 24 + u128::from(semantic.resident_ram_bytes) * 8)?,
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedFamilyV1 {
    PublicPrefix,
    Constraint,
    Tokenizer,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedFamilyPlanV1 {
    pub family: PalwStagedFamilyV1,
    pub acquisition: Option<PalwRenderingResourcePlanV1>,
    pub terms: PalwRenderingClaimTermsV1,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedBundlePlanV1 {
    pub terms: PalwRenderingClaimTermsV1,
    pub inventory: Option<PalwTokenizerInventoryPlanV1>,
    pub stages: Vec<PalwStagedFamilyPlanV1>,
}
impl BorshSerialize for PalwStagedBundlePlanV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        self.terms.serialize(writer)?;
        self.inventory.serialize(writer)?;
        self.stages.serialize(writer)
    }
}
impl BorshDeserialize for PalwStagedBundlePlanV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let terms = PalwRenderingClaimTermsV1::deserialize_reader(reader)?;
        let inventory = Option::<PalwTokenizerInventoryPlanV1>::deserialize_reader(reader)?;
        let count = u32::deserialize_reader(reader)? as usize;
        if !(1..=PALW_STAGED_REQUIRED_FAMILIES_V1).contains(&count) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "required family count is not bounded"));
        }
        let mut stages = Vec::with_capacity(count);
        for _ in 0..count {
            stages.push(PalwStagedFamilyPlanV1::deserialize_reader(reader)?);
        }
        let result = Self { terms, inventory, stages };
        result.check_v1().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(result)
    }
}
impl PalwStagedBundlePlanV1 {
    pub fn inventory_until_v1(&self) -> Result<Option<u64>, String> {
        self.inventory
            .as_ref()
            .map(|p| {
                self.terms
                    .open_until_daa
                    .checked_add(p.envelope.inclusion_daa)
                    .ok_or_else(|| "immutable raw intake clock overflow".to_string())
            })
            .transpose()
    }
    pub fn preseed_source_until_v1(&self, family: PalwStagedFamilyV1, config: PalwStagedPublicCourtConfigV1) -> Result<u64, String> {
        let stage = self.stages.iter().find(|s| s.family == family).ok_or("source family is not required")?;
        stage
            .terms
            .open_until_daa
            .checked_sub(config.prices.inclusion_margin_daa.checked_mul(2).ok_or("seed transition margin overflow")?)
            .filter(|n| *n > self.terms.accepted_daa)
            .ok_or_else(|| "source has no immutable prepaid seed/activation margin".into())
    }
    pub fn derive_v1(
        accepted_daa: u64,
        families: Vec<(PalwStagedFamilyV1, PalwRenderingResourcePlanV1)>,
        config: PalwStagedPublicCourtConfigV1,
    ) -> Result<Self, String> {
        Self::derive_with_inventory_v1(accepted_daa, families, None, config)
    }
    pub fn derive_with_inventory_v1(
        accepted_daa: u64,
        families: Vec<(PalwStagedFamilyV1, PalwRenderingResourcePlanV1)>,
        inventory: Option<PalwTokenizerInventoryPlanV1>,
        config: PalwStagedPublicCourtConfigV1,
    ) -> Result<Self, String> {
        Self::derive_with_acquisitions_v1(accepted_daa, families.into_iter().map(|(f, p)| (f, p, None)).collect(), inventory, config)
    }
    pub fn derive_with_acquisitions_v1(
        accepted_daa: u64,
        families: Vec<(PalwStagedFamilyV1, PalwRenderingResourcePlanV1, Option<PalwRenderingResourcePlanV1>)>,
        inventory: Option<PalwTokenizerInventoryPlanV1>,
        config: PalwStagedPublicCourtConfigV1,
    ) -> Result<Self, String> {
        if !(1..=PALW_STAGED_REQUIRED_FAMILIES_V1).contains(&families.len()) {
            return Err("required family set is empty or unbounded".into());
        }
        let mut stages = Vec::with_capacity(families.len());
        let mut sum = families[0].1;
        let mut acquisition_sum = None;
        let mut end = accepted_daa;
        if let Some(inventory) = &inventory {
            end = end
                .checked_add(config.acquisition_daa)
                .and_then(|n| n.checked_add(inventory.envelope.inclusion_daa))
                .ok_or("bundle intake allocation clock overflow")?;
        }
        for (index, (family, plan, acquisition)) in families.into_iter().enumerate() {
            if index != 0 {
                sum = sum.checked_sum_v1(plan)?;
            }
            if let Some(acquisition) = acquisition {
                acquisition_sum = Some(match acquisition_sum {
                    Some(sum) => PalwRenderingResourcePlanV1::checked_sum_v1(sum, acquisition)?,
                    None => acquisition,
                });
            }
            let open = end
                .checked_add(config.acquisition_daa)
                .and_then(|n| n.checked_add(acquisition.map_or(0, |p| p.inclusion_daa)))
                .ok_or("bundle acquisition clock overflow")?;
            end = open.checked_add(plan.inclusion_daa).ok_or("bundle stream clock overflow")?;
            let collect = end.checked_add(config.collection_daa).ok_or("bundle collection clock overflow")?;
            stages.push(PalwStagedFamilyPlanV1 {
                family,
                acquisition,
                terms: PalwRenderingClaimTermsV1 {
                    accepted_daa,
                    open_until_daa: open,
                    stream_until_daa: end,
                    collect_until_daa: collect,
                    material_until_daa: collect,
                    bond_exit_after_daa: collect,
                    plan,
                },
            });
        }
        let collect = end.checked_add(config.collection_daa).ok_or("bundle final collection clock overflow")?;
        let material = collect.checked_add(config.reorg_retention_daa).ok_or("bundle retained evidence clock overflow")?;
        // Every signed child retains its original context, and each existing child
        // carrier also pays its unchanged minimum rent after the bundle prepayment.
        sum.rent_sompi = sum.rent_sompi.checked_mul(2).ok_or("bundle child fee envelope overflow")?;
        if let Some(acquisition) = acquisition_sum {
            sum = sum.checked_sum_v1(acquisition)?;
        }
        if let Some(inventory) = &inventory {
            sum = sum.checked_sum_v1(inventory.envelope)?;
        }
        sum.rent_sompi.checked_add(sum.deposit_sompi).ok_or("bundle capital envelope overflow")?;
        for stage in &mut stages {
            stage.terms.material_until_daa = material;
            stage.terms.bond_exit_after_daa = material;
        }
        let original_open = accepted_daa.checked_add(config.acquisition_daa).ok_or("bundle first opening clock overflow")?;
        let result = Self {
            terms: PalwRenderingClaimTermsV1 {
                accepted_daa,
                open_until_daa: original_open,
                stream_until_daa: end,
                collect_until_daa: collect,
                material_until_daa: material,
                bond_exit_after_daa: material,
                plan: sum,
            },
            inventory,
            stages,
        };
        result.check_v1()?;
        Ok(result)
    }
    pub fn check_v1(&self) -> Result<(), String> {
        if !(1..=PALW_STAGED_REQUIRED_FAMILIES_V1).contains(&self.stages.len())
            || self.stages.windows(2).any(|w| w[0].family >= w[1].family)
            || self.terms.accepted_daa >= self.terms.open_until_daa
            || self.terms.open_until_daa >= self.terms.stream_until_daa
            || self.terms.stream_until_daa >= self.terms.collect_until_daa
            || self.terms.collect_until_daa > self.terms.material_until_daa
            || self.terms.material_until_daa > self.terms.bond_exit_after_daa
        {
            return Err("bundle changes finite required family set or original clocks".into());
        }
        let mut sum = self.stages[0].terms.plan;
        for stage in self.stages.iter().skip(1) {
            sum = sum.checked_sum_v1(stage.terms.plan)?;
        }
        sum.rent_sompi = sum.rent_sompi.checked_mul(2).ok_or("bundle fee sum overflow")?;
        for stage in &self.stages {
            if let Some(acquisition) = stage.acquisition {
                sum = sum.checked_sum_v1(acquisition)?;
            }
        }
        if let Some(inventory) = &self.inventory {
            sum = sum.checked_sum_v1(inventory.envelope)?;
        }
        if sum != self.terms.plan
            || self.stages[0].terms.open_until_daa < self.terms.open_until_daa
            || self.stages.last().unwrap().terms.stream_until_daa != self.terms.stream_until_daa
        {
            return Err("bundle maximum cannot replace independently funded family sum".into());
        }
        for stage in &self.stages {
            let t = stage.terms;
            if t.accepted_daa != self.terms.accepted_daa
                || t.open_until_daa >= t.stream_until_daa
                || t.stream_until_daa >= t.collect_until_daa
                || t.collect_until_daa > t.material_until_daa
                || t.material_until_daa != self.terms.material_until_daa
                || t.bond_exit_after_daa != self.terms.bond_exit_after_daa
            {
                return Err("family changes immutable bundle allocation or evidence hold".into());
            }
        }
        Ok(())
    }
    pub fn prepayment_rent_v1(&self) -> Result<u64, String> {
        let mut rent = self.inventory.as_ref().map_or(0, |p| p.envelope.rent_sompi);
        for stage in &self.stages {
            rent = rent
                .checked_add(stage.terms.plan.rent_sompi)
                .and_then(|n| n.checked_add(stage.acquisition.map_or(0, |p| p.rent_sompi)))
                .ok_or("bundle prepayment overflow")?;
        }
        Ok(rent)
    }
    pub fn id_v1(&self, claim: Hash64) -> Hash64 {
        let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/staged-required-bundle/v1").to_state();
        hash.update(claim.as_byte_slice());
        hash.update(&borsh::to_vec(self).expect("bounded plan encoding"));
        Hash64::from_bytes(hash.finalize().as_bytes().try_into().expect("64"))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedBundleContractV1 {
    pub claim: PalwStagedClaimContractV1,
    pub plan: PalwStagedBundlePlanV1,
    pub config: PalwStagedPublicCourtConfigV1,
    pub history_custody: crate::palw_history_custody_v1::PalwHistoryCustodyContractV1,
    pub original_binding: crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1,
    /// The node custodian an external producer delegated its literal answer slot to, if any.
    /// It may sign the producer slot's answers; the producer alone stays liable for them.
    pub producer_custodian: Option<crate::palw_state_v2::PalwBondKeyV2>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerInventoryPlanV1 {
    pub maximum_bytes: u64,
    pub envelope: PalwRenderingResourcePlanV1,
}
impl PalwTokenizerInventoryPlanV1 {
    pub fn derive_v1(maximum_bytes: u64, config: PalwStagedPublicCourtConfigV1) -> Result<Self, String> {
        let n = |value: u128| u64::try_from(value).map_err(|_| "tokenizer inventory resource overflow".to_string());
        let moves = maximum_bytes.div_ceil(65_536).checked_add(2).ok_or("inventory move count overflow")?;
        let bytes = n(u128::from(maximum_bytes) + u128::from(moves) * 5_632)?;
        let hashes = n(u128::from(maximum_bytes).div_ceil(128) * 4 + 32)?;
        let work = n(u128::from(maximum_bytes) * 4 + u128::from(moves) * 1_024)?;
        let resident = 128 * 1_024;
        let rent = n(u128::from(bytes) * u128::from(config.prices.rent_per_stream_byte)
            + u128::from(work) * u128::from(config.prices.rent_per_parser_step)
            + 2_048 * u128::from(config.prices.rent_per_row_byte))?;
        Ok(Self {
            maximum_bytes,
            envelope: PalwRenderingResourcePlanV1 {
                prefix_tokens: 0,
                vocab: 1,
                maximum_stream_bytes: bytes,
                maximum_stream_leaves: 1,
                maximum_eog_bytes: 0,
                maximum_moves: moves,
                maximum_parser_steps: work,
                maximum_blake_blocks: hashes,
                inclusion_daa: n(u128::from(moves) * u128::from(config.prices.inclusion_margin_daa))?,
                rent_sompi: rent,
                deposit_sompi: n(u128::from(moves) * u128::from(config.prices.deposit_per_move))?,
                row_bytes: 2_048,
                resident_ram_bytes: resident,
            },
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTokenizerInventoryV1 {
    pub maximum_bytes: u64,
    pub expected_hash: Hash64,
    pub declared_bytes: u64,
    pub until_daa: u64,
    pub offset: u64,
    pub sealed: bool,
    pub moves_used: u64,
    hash: crate::palw_rendering_stream_v1::LeafHashContinuationV1,
}
impl PalwTokenizerInventoryV1 {
    pub fn new_v1(plan: &PalwTokenizerInventoryPlanV1, expected_hash: Hash64, declared_bytes: u64, until_daa: u64) -> Self {
        let mut hash = crate::palw_rendering_stream_v1::LeafHashContinuationV1::new(
            crate::palw_tokenizer_rendering_v1::PALW_TOKENIZER_RAW_DOMAIN_V1,
        );
        hash.update(&plan.maximum_bytes.to_le_bytes());
        Self {
            maximum_bytes: plan.maximum_bytes,
            expected_hash,
            declared_bytes,
            until_daa,
            offset: 0,
            sealed: false,
            moves_used: 0,
            hash,
        }
    }
    pub fn part_v1(&mut self, now: u64, offset: u64, bytes: &[u8], plan: &PalwTokenizerInventoryPlanV1) -> Result<(), String> {
        if now >= self.until_daa
            || self.sealed
            || offset != self.offset
            || bytes.is_empty()
            || bytes.len() > 65_536
            || self.moves_used >= plan.envelope.maximum_moves
        {
            return Err("raw intake changes paid offset, EOF or frozen clock".into());
        }
        let end =
            offset.checked_add(bytes.len() as u64).filter(|end| *end <= self.maximum_bytes).ok_or("raw intake exceeds owner quote")?;
        self.hash.update(bytes);
        self.offset = end;
        self.moves_used += 1;
        Ok(())
    }
    pub fn finish_v1(&mut self, now: u64, plan: &PalwTokenizerInventoryPlanV1) -> Result<(), String> {
        if now >= self.until_daa
            || self.sealed
            || self.moves_used >= plan.envelope.maximum_moves
            || self.offset != self.maximum_bytes
            || self.hash.byte_count() != Some(u128::from(self.offset) + 128 + 8)
            || self.hash.finish() != self.expected_hash
        {
            return Err("raw inventory has no immutable registered file hash at exact EOF".into());
        }
        self.sealed = true;
        self.moves_used += 1;
        Ok(())
    }
    pub fn length_fault_v1(&self) -> bool {
        self.sealed && self.offset != self.declared_bytes
    }
}

/// The fixed codec rejects nested bundles before attempting their deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedFamilyRowV1(pub PalwStagedCourtRowV1);
impl BorshSerialize for PalwStagedFamilyRowV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        if self.family_v1().is_none() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "not a typed family"));
        }
        self.0.serialize(writer)
    }
}
impl BorshDeserialize for PalwStagedFamilyRowV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let row = match u8::deserialize_reader(reader)? {
            1 => PalwStagedCourtRowV1::Constraint(BorshDeserialize::deserialize_reader(reader)?),
            2 => PalwStagedCourtRowV1::Tokenizer(BorshDeserialize::deserialize_reader(reader)?),
            3 => PalwStagedCourtRowV1::Prefix(BorshDeserialize::deserialize_reader(reader)?),
            4 => PalwStagedCourtRowV1::PublicPrefix(BorshDeserialize::deserialize_reader(reader)?),
            9 => PalwStagedCourtRowV1::TirPrefix(BorshDeserialize::deserialize_reader(reader)?),
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "nested or unknown family court")),
        };
        Ok(Self(row))
    }
}
impl PalwStagedFamilyRowV1 {
    pub fn family_v1(&self) -> Option<PalwStagedFamilyV1> {
        match self.0 {
            PalwStagedCourtRowV1::Constraint(_) => Some(PalwStagedFamilyV1::Constraint),
            PalwStagedCourtRowV1::Tokenizer(_) => Some(PalwStagedFamilyV1::Tokenizer),
            PalwStagedCourtRowV1::Prefix(_) | PalwStagedCourtRowV1::PublicPrefix(_) | PalwStagedCourtRowV1::TirPrefix(_) => {
                Some(PalwStagedFamilyV1::PublicPrefix)
            }
            _ => None,
        }
    }
    pub fn honest_complete_v1(&self) -> Result<bool, String> {
        match &self.0 {
            PalwStagedCourtRowV1::Constraint(c) => Ok(c.court().phase()
                == crate::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1::Complete
                && matches!(c.certificate(), Ok(None))),
            PalwStagedCourtRowV1::Tokenizer(c) => Ok(matches!(c.certificate(), Ok(None))),
            PalwStagedCourtRowV1::Prefix(c) => {
                Ok(matches!(c.cursor_v1()?, crate::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1::Complete)
                    && matches!(c.certificate(), Ok(None)))
            }
            PalwStagedCourtRowV1::PublicPrefix(c) => Ok(c.pending_progress_v1().is_none()
                && matches!(c.cursor_v1()?, crate::palw_prefix_family_court_v1::PalwPrefixFamilyCursorV1::Complete)
                && matches!(c.certificate(), Ok(None))),
            PalwStagedCourtRowV1::TirPrefix(c) => Ok(c.honest_complete_v1()),
            _ => Err("bundle child is not a typed court".into()),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedSeedContextV1 {
    pub seed: Box<crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1>,
    pub binding: Box<crate::palw_step_leg::PalwStepBindingV2>,
    pub job: Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>,
}
/// A TIR stage's admitted context (behind `palw_staged_tir_route_v1`): the native seed, the
/// claim's admitted TIR public header (read from the native contract, never the carrier) and job.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedNativeSeedContextV1 {
    pub seed: Box<crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamNativeSeedV1>,
    pub header: Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
    pub job: Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>,
}
/// An opened stage's admitted context: a legacy seed with its resolved binding, or a native TIR
/// seed with the claim's admitted header.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwStagedStageContextV1 {
    Legacy(PalwStagedSeedContextV1),
    Native(PalwStagedNativeSeedContextV1),
}
impl PalwStagedStageContextV1 {
    pub fn seed_id_v1(&self) -> Hash64 {
        match self {
            Self::Legacy(c) => Hash64::from_bytes(c.seed.signing_message().try_into().expect("seed digest width")),
            Self::Native(c) => c.seed.seed_id_v1(),
        }
    }
    pub fn legacy_v1(&self) -> Option<&PalwStagedSeedContextV1> {
        match self {
            Self::Legacy(c) => Some(c),
            Self::Native(_) => None,
        }
    }
    pub fn native_v1(&self) -> Option<&PalwStagedNativeSeedContextV1> {
        match self {
            Self::Native(c) => Some(c),
            Self::Legacy(_) => None,
        }
    }
    pub fn job_v1(&self) -> &crate::palw_freeprompt_v3::PalwFreePromptJobV3 {
        match self {
            Self::Legacy(c) => &c.job,
            Self::Native(c) => &c.job,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSelectedSourceContextV1 {
    pub binding: Box<crate::palw_step_leg::PalwStepBindingV2>,
    pub job: Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>,
}
/// The TIR selected source's retained context (behind `palw_staged_tir_route_v1`): the claim's
/// admitted header and job; the fold re-derives the decode view from it and the class record.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwNativeSourceContextV1 {
    pub header: Box<crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1>,
    pub job: Box<crate::palw_freeprompt_v3::PalwFreePromptJobV3>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedOpenedFamilyV1 {
    pub row: PalwStagedFamilyRowV1,
    pub context: crate::palw_staged_court_state_v1::PalwSharedCourtV1<PalwStagedStageContextV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwOwnedStagedBundleV1 {
    pub claim: Hash64,
    pub owner: PalwBondKeyV2,
    pub reserved_daa: u64,
    pub execution_root: Hash64,
    pub plan: PalwStagedBundlePlanV1,
    pub inventory: Option<PalwTokenizerInventoryV1>,
    pub selected: Option<crate::palw_selected_event_source_v1::PalwSelectedEventSessionV1>,
    pub selected_context: Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<PalwSelectedSourceContextV1>>,
    /// A TIR claim's selected source context (the session is `selected`); exclusive with `selected_context`.
    pub selected_native: Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<PalwNativeSourceContextV1>>,
    pub flat: Option<
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1>,
    >,
    pub flat_context: Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<PalwSelectedSourceContextV1>>,
    /// Greedy Float32 Constraint source (behind `palw_staged_float_constraint_v1`).
    pub float: Option<
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<
            crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1,
        >,
    >,
    pub float_context: Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<PalwSelectedSourceContextV1>>,
    pub history_recovery: std::collections::BTreeMap<
        Hash64,
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_history_custody_v1::PalwHistoryRecoveryV1>,
    >,
    pub active: Option<u8>,
    pub opened: Vec<PalwStagedOpenedFamilyV1>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedFamilyCheckpointV1(pub PalwStagedCheckpointV1);
impl BorshSerialize for PalwStagedFamilyCheckpointV1 {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match &self.0 {
            PalwStagedCheckpointV1::Bundle(_) => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "nested bundle checkpoint")),
            _ => self.0.serialize(writer),
        }
    }
}
impl BorshDeserialize for PalwStagedFamilyCheckpointV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let checkpoint = match u8::deserialize_reader(reader)? {
            0 => PalwStagedCheckpointV1::Constraint(BorshDeserialize::deserialize_reader(reader)?),
            1 => PalwStagedCheckpointV1::Tokenizer(BorshDeserialize::deserialize_reader(reader)?),
            2 => PalwStagedCheckpointV1::Prefix(BorshDeserialize::deserialize_reader(reader)?),
            3 => PalwStagedCheckpointV1::PublicPrefix(BorshDeserialize::deserialize_reader(reader)?),
            5 => PalwStagedCheckpointV1::TirPrefix(BorshDeserialize::deserialize_reader(reader)?),
            _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "nested or unknown bundle checkpoint")),
        };
        Ok(Self(checkpoint))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwStagedBundleCheckpointV1 {
    pub bundle_id: Hash64,
    pub active_index: Option<u8>,
    pub active_seed: Option<Hash64>,
    pub inventory: Option<PalwTokenizerInventoryV1>,
    pub selected: Option<crate::palw_selected_event_source_v1::PalwSelectedEventSessionV1>,
    pub flat: Option<
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_flat_constraint_stream_v1::PalwFlatConstraintSourceSessionV1>,
    >,
    pub float: Option<
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<
            crate::palw_float_constraint_source_v1::PalwFloatConstraintSourceSessionV1,
        >,
    >,
    pub history_recovery: std::collections::BTreeMap<
        Hash64,
        crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_history_custody_v1::PalwHistoryRecoveryV1>,
    >,
    pub active_checkpoint: Option<PalwStagedFamilyCheckpointV1>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStagedBundleProgressV1 {
    pub bundle_id: Hash64,
    pub plan: PalwStagedBundlePlanV1,
    pub active_index: Option<u8>,
    pub pending_seed: Option<Hash64>,
    pub inventory: Option<(u64, u64, bool, bool)>,
}
impl PalwOwnedStagedBundleV1 {
    pub fn progress_v1(&self) -> PalwStagedBundleProgressV1 {
        let pending = self.active.map_or(0, |index| index as usize + 1);
        PalwStagedBundleProgressV1 {
            bundle_id: self.plan.id_v1(self.claim),
            plan: self.plan.clone(),
            active_index: self.active,
            pending_seed: self.opened.get(pending).map(|stage| stage.context.seed_id_v1()),
            inventory: self.inventory.as_ref().map(|i| (i.offset, i.maximum_bytes, i.sealed, i.length_fault_v1())),
        }
    }
    pub fn active_v1(&self) -> Option<&PalwStagedCourtRowV1> {
        self.active.and_then(|i| self.opened.get(i as usize)).map(|s| &s.row.0)
    }
    pub fn active_mut_v1(&mut self) -> Option<&mut PalwStagedCourtRowV1> {
        self.active.and_then(|i| self.opened.get_mut(i as usize)).map(|s| &mut s.row.0)
    }
    pub fn seed_row_v1(&self, seed_id: Hash64) -> Option<&PalwStagedCourtRowV1> {
        self.opened.iter().find(|s| s.context.seed_id_v1() == seed_id).map(|s| &s.row.0)
    }
    /// The active stage's LEGACY seed (`None` for a native TIR stage: see [`Self::active_native_seed_v1`]).
    pub fn active_seed_v1(&self) -> Option<&crate::palw_constraint_stream_carriage_v1::PalwConstraintStreamSeedV1> {
        self.active
            .and_then(|index| self.opened.get(index as usize))
            .and_then(|stage| stage.context.legacy_v1())
            .map(|c| c.seed.as_ref())
    }
    pub fn active_native_seed_v1(&self) -> Option<&PalwStagedNativeSeedContextV1> {
        self.active.and_then(|index| self.opened.get(index as usize)).and_then(|stage| stage.context.native_v1())
    }
    /// The active stage's seed identity, legacy or native.
    pub fn active_seed_id_v1(&self) -> Option<Hash64> {
        self.active.and_then(|index| self.opened.get(index as usize)).map(|stage| stage.context.seed_id_v1())
    }
    pub fn inventory_fault_v1(&self) -> Result<Option<crate::palw_tokenizer_owned_court_v1::PalwTokenizerCourtFaultV1>, String> {
        self.check_v1()?;
        Ok(self.inventory.as_ref().is_some_and(PalwTokenizerInventoryV1::length_fault_v1).then_some(
            crate::palw_tokenizer_owned_court_v1::PalwTokenizerCourtFaultV1 {
                claim: self.claim,
                reserver: self.owner,
                reserved_daa: self.reserved_daa,
                seed_id: self.plan.id_v1(self.claim),
                execution_root: self.execution_root,
            },
        ))
    }
    pub(crate) fn checkpoint_v1(&self) -> Result<PalwStagedBundleCheckpointV1, String> {
        Ok(PalwStagedBundleCheckpointV1 {
            bundle_id: self.plan.id_v1(self.claim),
            active_index: self.active,
            active_seed: self.active_seed_id_v1(),
            inventory: self.inventory.clone(),
            selected: self.selected.clone(),
            flat: self.flat.clone(),
            float: self.float.clone(),
            history_recovery: self.history_recovery.clone(),
            active_checkpoint: self.active_v1().map(|row| row.checkpoint_v1().map(PalwStagedFamilyCheckpointV1)).transpose()?,
        })
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwStagedBundleCheckpointV1) -> Result<(), String> {
        let current = self.checkpoint_v1()?;
        if (cp.bundle_id, cp.active_index, cp.active_seed) != (current.bundle_id, current.active_index, current.active_seed) {
            return Err("bundle checkpoint changes immutable stage activation or seed".into());
        }
        match (&self.inventory, &cp.inventory) {
            (Some(old), Some(new))
                if (old.maximum_bytes, old.expected_hash, old.declared_bytes, old.until_daa)
                    == (new.maximum_bytes, new.expected_hash, new.declared_bytes, new.until_daa) => {}
            (None, None) => {}
            _ => return Err("bundle checkpoint changes immutable inventory or intake clock".into()),
        }
        let mut candidate = self.clone();
        match (&self.flat, &cp.flat) {
            (Some(old), Some(new))
                if (
                    old.claim,
                    old.owner,
                    old.reserved_daa,
                    old.bundle_id,
                    old.source_id,
                    old.respond_until_daa,
                    old.collect_until_daa,
                    old.producer,
                    &old.custodians,
                ) == (
                    new.claim,
                    new.owner,
                    new.reserved_daa,
                    new.bundle_id,
                    new.source_id,
                    new.respond_until_daa,
                    new.collect_until_daa,
                    new.producer,
                    &new.custodians,
                ) => {}
            (None, None) => {}
            _ => return Err("checkpoint changes immutable flat source admission or custody".into()),
        }
        match (&self.float, &cp.float) {
            (Some(old), Some(new))
                if (
                    old.claim,
                    old.owner,
                    old.reserved_daa,
                    old.bundle_id,
                    old.source_id,
                    old.respond_until_daa,
                    old.collect_until_daa,
                    old.producer,
                    &old.custodians,
                ) == (
                    new.claim,
                    new.owner,
                    new.reserved_daa,
                    new.bundle_id,
                    new.source_id,
                    new.respond_until_daa,
                    new.collect_until_daa,
                    new.producer,
                    &new.custodians,
                ) => {}
            (None, None) => {}
            _ => return Err("checkpoint changes immutable Float32 source admission or custody".into()),
        }
        match (&self.selected, &cp.selected) {
            (Some(old), Some(new))
                if (
                    old.claim,
                    old.owner,
                    old.reserved_daa,
                    old.bundle_id,
                    old.respond_until_daa,
                    old.collect_until_daa,
                    old.minimum_response_span_daa,
                    &old.custodians,
                ) == (
                    new.claim,
                    new.owner,
                    new.reserved_daa,
                    new.bundle_id,
                    new.respond_until_daa,
                    new.collect_until_daa,
                    new.minimum_response_span_daa,
                    &new.custodians,
                ) => {}
            (None, None) => {}
            _ => return Err("checkpoint changes immutable selected source admission or custody".into()),
        }
        candidate.inventory = cp.inventory.clone();
        candidate.selected = cp.selected.clone();
        candidate.flat = cp.flat.clone();
        candidate.float = cp.float.clone();
        candidate.history_recovery = cp.history_recovery.clone();
        match (candidate.active_mut_v1(), &cp.active_checkpoint) {
            (Some(row), Some(cp)) => row.restore_checkpoint_v1(&cp.0)?,
            (None, None) => {}
            _ => return Err("bundle checkpoint changes typed child presence".into()),
        }
        candidate.check_v1()?;
        *self = candidate;
        Ok(())
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.plan.check_v1()?;
        if self.history_recovery.len() > PALW_STAGED_REQUIRED_FAMILIES_V1 {
            return Err("bundle history recovery set exceeds required family bound".into());
        }
        for (seed, recovery) in &self.history_recovery {
            recovery.check_v1()?;
            if (recovery.initial.claim, recovery.initial.owner, recovery.initial.reserved_daa, recovery.initial.seed_id)
                != (self.claim, self.owner, self.reserved_daa, *seed)
                || self.seed_row_v1(*seed).is_none()
                || recovery.collect_until_daa > self.plan.terms.collect_until_daa
            {
                return Err("bundle recovery substitutes paid original stage identity or horizon".into());
            }
        }
        if self.selected_native.is_some() && self.selected_context.is_some() {
            return Err("selected source retains both a legacy and a native original context".into());
        }
        if let Some(native) = &self.selected_native {
            if self.selected.is_none() || native.header.committed_execution_root != self.execution_root {
                return Err("native selected source changes literal paid frame or original execution".into());
            }
        }
        match (&self.selected, &self.selected_context) {
            (Some(_), None) if self.selected_native.is_some() => {}
            (Some(selected), Some(context)) => {
                if (selected.claim, selected.owner, selected.reserved_daa, selected.bundle_id)
                    != (self.claim, self.owner, self.reserved_daa, self.plan.id_v1(self.claim))
                    || selected.requests.len() > crate::palw_selected_event_source_v1::PALW_SELECTED_EVENT_MAX_REQUESTS_V1
                    || selected.custodians.contains(&self.owner)
                    || selected.minimum_response_span_daa == 0
                    || selected
                        .reserved_daa
                        .checked_add(selected.minimum_response_span_daa)
                        .is_none_or(|until| until > selected.respond_until_daa)
                    || selected.collect_until_daa > self.plan.terms.collect_until_daa
                    || context.binding.committed_execution_root != self.execution_root
                {
                    return Err("selected source changes literal paid frame or original execution".into());
                }
            }
            (None, None) => {}
            _ => return Err("selected source has no retained authenticated original context".into()),
        }
        match (&self.flat, &self.flat_context) {
            (Some(flat), Some(context)) => {
                flat.check_v1()?;
                if (flat.claim, flat.owner, flat.reserved_daa, flat.bundle_id)
                    != (self.claim, self.owner, self.reserved_daa, self.plan.id_v1(self.claim))
                    || flat.collect_until_daa > self.plan.terms.collect_until_daa
                    || context.binding.committed_execution_root != self.execution_root
                    || self.selected.is_some()
                {
                    return Err("flat source changes original frame, scheme or source allocation".into());
                }
            }
            (None, None) => {}
            _ => return Err("flat source loses original authenticated public context".into()),
        }
        match (&self.float, &self.float_context) {
            (Some(float), Some(context)) => {
                float.check_v1()?;
                if (float.claim, float.owner, float.reserved_daa, float.bundle_id)
                    != (self.claim, self.owner, self.reserved_daa, self.plan.id_v1(self.claim))
                    || float.collect_until_daa > self.plan.terms.collect_until_daa
                    || context.binding.committed_execution_root != self.execution_root
                    || context.binding.shape_profile.lane != crate::palw_step::PalwStepLaneV1::Float32
                    || self.selected.is_some()
                    || self.flat.is_some()
                {
                    return Err("Float32 source changes original frame, lane or source allocation".into());
                }
            }
            (None, None) => {}
            _ => return Err("Float32 source loses original authenticated public context".into()),
        }
        match (&self.inventory, &self.plan.inventory) {
            (Some(inventory), Some(plan))
                if inventory.maximum_bytes == plan.maximum_bytes
                    && inventory.offset <= plan.maximum_bytes
                    && inventory.moves_used <= plan.envelope.maximum_moves
                    && inventory.hash.byte_count() == Some(u128::from(inventory.offset) + 128 + 8)
                    && (!inventory.sealed || inventory.hash.finish() == inventory.expected_hash) => {}
            (None, None) => {}
            _ => return Err("bundle inventory exceeds its literal prepaid maximum or EOF authenticator".into()),
        }
        if self.opened.len() > self.plan.stages.len()
            || self.active.is_some_and(|i| i as usize >= self.opened.len() || self.opened.len() > i as usize + 2)
            || (self.active.is_none() && self.opened.len() > 1)
        {
            return Err("bundle active/pending family count is not bounded".into());
        }
        for (index, stage) in self.opened.iter().enumerate() {
            stage.row.0.check(&PalwStagedCourtKeyV1::Owner(self.claim, self.owner))?;
            let (row_terms, row_reserved, row_seed) = match &stage.row.0 {
                PalwStagedCourtRowV1::Constraint(row) => (row.terms, row.reserved_daa, row.seed_id()),
                PalwStagedCourtRowV1::Tokenizer(row) => (row.terms, row.reserved_daa, row.seed_id()),
                PalwStagedCourtRowV1::Prefix(row) => (row.terms, row.reserved_daa, row.seed_id()),
                PalwStagedCourtRowV1::PublicPrefix(row) => (row.terms, row.reserved_daa, row.seed_id()),
                PalwStagedCourtRowV1::TirPrefix(row) => (row.terms, row.reserved_daa, row.seed_id()),
                _ => return Err("bundle retains an untyped or recursive child".into()),
            };
            if stage.row.family_v1() != Some(self.plan.stages[index].family)
                || row_reserved != self.reserved_daa
                || row_terms != self.plan.stages[index].terms
                || row_seed != stage.context.seed_id_v1()
            {
                return Err("opened family changes signed original context or required stage".into());
            }
            match stage.context.as_ref() {
                PalwStagedStageContextV1::Legacy(context) => {
                    let seed = &context.seed;
                    seed.check(90_000)?;
                    if (seed.claim, seed.reserver, seed.reserved_daa) != (self.claim, self.owner, self.reserved_daa)
                        || context.binding.committed_execution_root != seed.binding.committed_execution_root
                        || context.binding.committed_execution_root != self.execution_root
                        || seed.resolved_binding_v1(context.job.class_id, Some(&context.binding.shape_profile))? != *context.binding
                        || seed.job.as_deref().is_some_and(|j| j != context.job.as_ref())
                    {
                        return Err("opened family changes signed original context or required stage".into());
                    }
                }
                PalwStagedStageContextV1::Native(context) => {
                    let seed = &context.seed;
                    seed.check(90_000)?;
                    if (seed.claim, seed.reserver, seed.reserved_daa) != (self.claim, self.owner, self.reserved_daa)
                        || context.header.committed_execution_root != self.execution_root
                        || seed.view_identity
                            != crate::palw_constraint_stream_carriage_v1::palw_native_view_identity_v1(&context.header)
                        || seed.job.as_deref().is_some_and(|j| j != context.job.as_ref())
                    {
                        return Err("opened native family changes signed original header or required stage".into());
                    }
                }
            }
            if self.active.is_some_and(|active| index < active as usize) && !stage.row.honest_complete_v1()? {
                return Err("bundle retired a family without rooted honest completion".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prerequisite_source_and_intake_are_allocated_before_original_child_opening() {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let semantic = crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(128, 2, config.prices)
            .unwrap()
            .court_envelope(19, config.prices)
            .unwrap();
        let acquisition = staged_context_resource_plan_v1(256, semantic, config).unwrap();
        let intake = PalwTokenizerInventoryPlanV1::derive_v1(128, config).unwrap();
        let plan = PalwStagedBundlePlanV1::derive_with_acquisitions_v1(
            100,
            vec![
                (PalwStagedFamilyV1::Constraint, semantic, Some(acquisition)),
                (PalwStagedFamilyV1::Tokenizer, semantic, Some(acquisition)),
            ],
            Some(intake.clone()),
            config,
        )
        .unwrap();
        let intake_until = 100 + config.acquisition_daa + intake.envelope.inclusion_daa;
        assert_eq!(plan.inventory_until_v1().unwrap(), Some(intake_until));
        assert_eq!(plan.stages[0].terms.open_until_daa, intake_until + config.acquisition_daa + acquisition.inclusion_daa);
        assert_eq!(plan.stages[0].terms.stream_until_daa, plan.stages[0].terms.open_until_daa + semantic.inclusion_daa);
        assert_eq!(
            plan.stages[1].terms.open_until_daa,
            plan.stages[0].terms.stream_until_daa + config.acquisition_daa + acquisition.inclusion_daa
        );
        assert_eq!(
            plan.terms.plan.maximum_moves,
            semantic.maximum_moves * 2 + acquisition.maximum_moves * 2 + intake.envelope.maximum_moves
        );
        assert_eq!(
            plan.preseed_source_until_v1(PalwStagedFamilyV1::Constraint, config).unwrap(),
            plan.stages[0].terms.open_until_daa - config.prices.inclusion_margin_daa * 2
        );
    }
    #[test]
    fn owner_raw_intake_authenticates_original_length_prefix_and_keyed_hash_at_eof() {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let bytes = b"{\"model\":{\"vocab\":{\"x\":0}}}";
        let plan = PalwTokenizerInventoryPlanV1::derive_v1(bytes.len() as u64, config).unwrap();
        let root = crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(bytes);
        let mut intake = PalwTokenizerInventoryV1::new_v1(&plan, root, 1, 10_000);
        intake.part_v1(100, 0, bytes, &plan).unwrap();
        intake.finish_v1(101, &plan).unwrap();
        assert!(intake.sealed && intake.length_fault_v1());
        assert!(intake.finish_v1(102, &plan).is_err());
        let mut incomplete = PalwTokenizerInventoryV1::new_v1(&plan, root, bytes.len() as u64, 10_000);
        incomplete.part_v1(100, 0, &bytes[..bytes.len() - 1], &plan).unwrap();
        assert!(incomplete.finish_v1(101, &plan).is_err());
        let mut substituted = PalwTokenizerInventoryV1::new_v1(&plan, Hash64::from_u64_word(888), 1, 10_000);
        substituted.part_v1(100, 0, bytes, &plan).unwrap();
        assert!(substituted.finish_v1(101, &plan).is_err());
    }
    #[test]
    fn required_family_sum_freezes_later_windows_and_rejects_overflow_and_duplicates() {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let p = crate::palw_rendering_resources_v1::PalwTokenizerResourcePlanV1::derive_with_key_bound(128, 2, config.prices)
            .unwrap()
            .court_envelope(19, config.prices)
            .unwrap();
        let bundle = PalwStagedBundlePlanV1::derive_v1(
            100,
            vec![(PalwStagedFamilyV1::Constraint, p), (PalwStagedFamilyV1::Tokenizer, p)],
            config,
        )
        .unwrap();
        assert_eq!(bundle.terms.plan.maximum_moves, p.maximum_moves * 2);
        assert_eq!(bundle.terms.plan.rent_sompi, p.rent_sompi * 4);
        assert!(bundle.stages[1].terms.open_until_daa > bundle.stages[0].terms.stream_until_daa);
        assert!(
            PalwStagedBundlePlanV1::derive_v1(
                100,
                vec![(PalwStagedFamilyV1::Constraint, p), (PalwStagedFamilyV1::Constraint, p)],
                config
            )
            .is_err()
        );
        let mut impossible = p;
        impossible.maximum_moves = u64::MAX;
        assert!(p.checked_sum_v1(impossible).is_err());
        assert!(PalwStagedBundlePlanV1::derive_v1(u64::MAX, vec![(PalwStagedFamilyV1::Tokenizer, p)], config).is_err());
        let encoded = borsh::to_vec(&bundle).unwrap();
        assert_eq!(borsh::from_slice::<PalwStagedBundlePlanV1>(&encoded).unwrap(), bundle);
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_staged_bundle_v1.rs"]
mod native_resource_inventory_v1;
