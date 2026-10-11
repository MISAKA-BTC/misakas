//! Bounded public acquisition of a named prefix's committed K/V writes.
//! The producer serves ordinary authenticated CKW units; the filer retains only output
//! tile preimages and paths for the prefix consistency court, never a secret cache snapshot.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3;
use kaspa_consensus_core::palw_legacy_held_da_v2::{PalwCommittedKernelWitnessV2, PalwLegacyHeldUnitV2};
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_constrained_job_target_v1, palw_prefix_state_fault_v1};
use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;
use kaspa_consensus_core::palw_prefix_state_court_v1::{PalwPrefixKvPlanV1, PalwPrefixKvTileV1};
use kaspa_consensus_core::palw_step_leg::PalwStepBindingV2;

#[derive(Clone, Debug)]
pub(super) struct PrefixPlanV1 {
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub job_id: Hash64,
    plan: PalwPrefixKvPlanV1,
    tiles: Vec<PalwPrefixKvTileV1>,
    evidence_bytes: u64,
    maximum_evidence_bytes: u64,
}

impl PrefixPlanV1 {
    /// Exact claim/job authentication precedes even a resource or servability refusal.
    /// None means this job names no state; an unsupported or oversized prefix remains an
    /// explicit pending obligation, never an honest-replay verdict.
    pub(super) fn for_job(
        target: &PalwOffenceTargetV1, binding: &PalwStepBindingV2, job: &PalwFreePromptJobV3,
        maximum_evidence_bytes: u64,
    ) -> Result<Option<Self>, String> {
        if !job.is_prefix_state() { return Ok(None); }
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        let named = kaspa_consensus_core::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
        let plan = PalwPrefixKvPlanV1::of_profile(&binding.shape_profile, named.prefix_tokens).map_err(str::to_string)?;
        // version + claim + contradiction tag + vector length + option framing, with room
        // for the fixed evidence wrapper. This is a conservative carrier accounting bound.
        let evidence_bytes = u64::try_from(borsh::object_length(&(binding, job)).map_err(|e| e.to_string())?)
            .map_err(|_| "the prefix descriptor size overflows")?.checked_add(256).ok_or("the prefix evidence size overflows")?;
        if evidence_bytes > maximum_evidence_bytes { return Err("the prefix descriptor exceeds its court evidence budget".into()); }
        Ok(Some(Self { claim: target.claim_id, execution_root: target.execution_root, job_id: binding.job_context.job_id,
            plan, tiles: Vec::new(), evidence_bytes, maximum_evidence_bytes }))
    }

    pub(super) fn next_unit(&self, binding: &PalwStepBindingV2) -> Result<Option<PalwLegacyHeldUnitV2>, String> {
        if binding.committed_execution_root != self.execution_root || binding.job_context.job_id != self.job_id {
            return Err("the prefix plan is another claim's binding".into());
        }
        let Some((coord, _)) = self.plan.coordinate(&binding.shape_profile, self.tiles.len() as u64) else { return Ok(None) };
        let leaf = kaspa_consensus_core::palw_step::canonical_step_leaf_index(&binding.shape_profile, &binding.job_context, &coord)
            .ok_or("the prefix cache coordinate has no canonical leaf")?;
        Ok(Some(PalwLegacyHeldUnitV2::KernelWitness { leaf }))
    }

    pub(super) fn learn(
        &mut self, binding: &PalwStepBindingV2, witness: &PalwCommittedKernelWitnessV2,
    ) -> Result<(), String> {
        let Some(PalwLegacyHeldUnitV2::KernelWitness { leaf }) = self.next_unit(binding)? else {
            return Err("the prefix plan is already complete".into());
        };
        let r = &witness.refutation;
        if r.binding != *binding || r.output_opening.leaf_index != leaf {
            return Err("the CKW is not the next prefix tile".into());
        }
        kaspa_consensus_core::palw_step_refute::check_execution_step_committed_half_v1(
            r, witness.prompt_ids_opening.as_ref(),
            kaspa_consensus_core::palw_offence_attribution_v1::PALW_FALSE_VALID_NETWORK_LADDER_V1,
        ).map_err(|e| e.to_string())?;
        let (coord, values) = self.plan.coordinate(&binding.shape_profile, self.tiles.len() as u64)
            .ok_or("the prefix tile coordinate disappeared")?;
        if r.output_preimage.coord != coord || r.output_preimage.value_count != values
            || r.output_preimage.values_le.len() as u64 != u64::from(values)*4
        { return Err("the prefix cache tile is not the canonical preimage".into()); }
        let tile = PalwPrefixKvTileV1 { opening: r.output_opening.clone(), preimage: r.output_preimage.clone() };
        let bytes = u64::try_from(borsh::object_length(&tile).map_err(|e| e.to_string())?)
            .map_err(|_| "the prefix tile size overflows")?;
        let total = self.evidence_bytes.checked_add(bytes).ok_or("the prefix evidence size overflows")?;
        if total > self.maximum_evidence_bytes {
            return Err("the prefix needs incremental digest carriage beyond the finite court budget".into());
        }
        self.tiles.push(tile);
        self.evidence_bytes = total;
        Ok(())
    }

    pub(super) fn contradiction(
        &self, target: &PalwOffenceTargetV1, binding: &PalwStepBindingV2, job: &PalwFreePromptJobV3,
    ) -> Result<Option<PalwPanelContradictionV1>, String> {
        if target.claim_id != self.claim || target.execution_root != self.execution_root || binding.job_context.job_id != self.job_id {
            return Err("the prefix evidence is another claim's".into());
        }
        if self.tiles.len() as u64 != self.plan.tile_count { return Err("the prefix acquisition is incomplete".into()); }
        if !palw_prefix_state_fault_v1(target, binding, job, &self.tiles).map_err(|e| e.to_string())? { return Ok(None); }
        Ok(Some(PalwPanelContradictionV1::PrefixStateMismatch {
            binding: binding.clone(), job: Box::new(job.clone()), tiles: self.tiles.clone(),
        }))
    }
}
