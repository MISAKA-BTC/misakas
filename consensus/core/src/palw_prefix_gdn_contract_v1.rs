//! Bounded authenticated contract for one prospective Q36 recurrence head. The canonical
//! production inventory contains one68-byte parameter leaf per head, so no entire scale
//! table is copied or interpreted as a new commitment.
use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use crate::palw_base0_a16::A16QuantParams;
use crate::palw_class_semantics_v1::PalwPrefixStateRelationV1;
use crate::palw_freeprompt_v3::PalwFreePromptJobV3;
use crate::palw_offence_attribution_v1::{PalwOffenceTargetV1, palw_constrained_job_target_v1};
use crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1;
use crate::palw_step::PalwStepTableV1;
use crate::palw_step_leg::PalwStepBindingV2;
use crate::palw_step_refute::{PalwWeightOracleV1, qwen36_gdn_slice_v1};
use borsh::{BorshDeserialize, BorshSerialize};
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub(crate) struct PalwPrefixGdnContractV1 {
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub job_id: Hash64,
    pub layer: u16,
    pub head: u32,
    pub prefix_tokens: u32,
    pub d_k: u32,
    pub d_v: u32,
    pub parameters: [u8; 68],
}
impl PalwPrefixGdnContractV1 {
    pub fn new(
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        layer: u16,
        head: u32,
        openings: &[PalwArtifactOpeningV1],
    ) -> Result<Self, String> {
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        let row = target
            .registered_semantics
            .as_ref()
            .filter(|r| r.matches_context(&binding.job_context))
            .ok_or("head has no immutable family statement")?;
        if row.class_id != target.class_id
            || row.artifact_root != target.artifact_root
            || row.prefix_state_relation != Some(PalwPrefixStateRelationV1::Qwen36HybridCacheV1)
        {
            return Err("head changes immutable class/artifact/family relation".into());
        }
        PalwPrefixStateRelationV1::Qwen36HybridCacheV1.check_profile(&binding.shape_profile)?;
        let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
        if named.prefix_tokens >= binding.job_context.declared_prefill_tokens {
            return Err("head is not a proper authenticated prefix".into());
        }
        let (_, node, widths) = crate::palw_prefix_gdn_replay_v1::head_geometry(&binding.shape_profile, layer, head)?;
        qwen36_gdn_slice_v1(&binding.shape_profile, node, &widths, head).ok_or("head source slices not canonical")?;
        // Exact source leaf, not a containing byte range. This preserves original inventory
        // identity and avoids verifying/copying unrelated parameter leaves in one carrier.
        if openings.len() != 1 || openings[0].operand.bytes.len() != 68 {
            return Err("head requires exactly its one canonical68-byte parameter leaf".into());
        }
        let proven = PalwProvenOperandsV1::from_openings_v1(openings, target.artifact_root).map_err(|e| e.to_string())?;
        let offset = head.checked_mul(68).ok_or("head parameter offset overflow")?;
        let parameters: [u8; 68] = proven
            .operand_bytes(&node.weight_name, Some(layer), offset, 68)
            .ok_or("original registered head parameter leaf not proven")?
            .try_into()
            .map_err(|_| "head parameter length mismatch")?;
        for triple in parameters.chunks_exact(A16QuantParams::WIRE_BYTES) {
            A16QuantParams::from_wire(triple).map_err(|e| format!("registered head parameter domain {e:?}"))?;
        }
        Ok(Self {
            claim: target.claim_id,
            execution_root: target.execution_root,
            job_id: binding.job_context.job_id,
            layer,
            head,
            prefix_tokens: named.prefix_tokens,
            d_k: binding.shape_profile.gdn_head_k_dim,
            d_v: binding.shape_profile.gdn_head_v_dim,
            parameters,
        })
    }
    pub fn source_range_v1(
        &self,
        binding: &PalwStepBindingV2,
        position: u32,
        ordinal: usize,
    ) -> Result<crate::palw_prefix_source_stream_v1::PalwPrefixSourceRangeV1, String> {
        if position >= self.prefix_tokens
            || ordinal >= 5
            || binding.committed_execution_root != self.execution_root
            || binding.job_context.job_id != self.job_id
        {
            return Err("head source changes owned position/context".into());
        }
        let (_, node, widths) = crate::palw_prefix_gdn_replay_v1::head_geometry(&binding.shape_profile, self.layer, self.head)?;
        let ranges =
            qwen36_gdn_slice_v1(&binding.shape_profile, node, &widths, self.head).ok_or("head source slices not canonical")?;
        let r = usize::from(node.input_refs[ordinal]);
        let source = &binding.shape_profile.gdn_nodes[r];
        Ok(crate::palw_prefix_source_stream_v1::PalwPrefixSourceRangeV1 {
            node_slot: binding
                .shape_profile
                .global_node_slot(PalwStepTableV1::Gdn, self.layer, r)
                .ok_or("head source slot missing")?,
            position,
            tile_len: source.tile_len,
            width: widths[ordinal],
            start: ranges[ordinal].0,
            end: ranges[ordinal].1,
        })
    }
    pub fn input_rows(
        &self,
        binding: &PalwStepBindingV2,
        position: u32,
        inputs: &[Vec<PalwPrefixKvTileV1>],
        maximum_leaves: u64,
    ) -> Result<Vec<Vec<i32>>, String> {
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| e.to_string())?;
        if binding.committed_execution_root != self.execution_root
            || binding.job_context.job_id != self.job_id
            || binding.shape_profile.gdn_head_k_dim != self.d_k
            || binding.shape_profile.gdn_head_v_dim != self.d_v
            || position >= self.prefix_tokens
            || inputs.len() != 5
        {
            return Err("head inputs change immutable context/geometry/position".into());
        }
        let (_, node, widths) = crate::palw_prefix_gdn_replay_v1::head_geometry(&binding.shape_profile, self.layer, self.head)?;
        let ranges =
            qwen36_gdn_slice_v1(&binding.shape_profile, node, &widths, self.head).ok_or("head source slices not canonical")?;
        let mut rows = Vec::with_capacity(5);
        for (i, reference) in node.input_refs.iter().enumerate() {
            let r = usize::from(*reference);
            let source = &binding.shape_profile.gdn_nodes[r];
            let slot =
                binding.shape_profile.global_node_slot(PalwStepTableV1::Gdn, self.layer, r).ok_or("head source slot missing")?;
            rows.push(crate::palw_prefix_gdn_replay_v1::authenticated_slice(
                binding,
                slot,
                position,
                source.tile_len,
                widths[i],
                ranges[i],
                &inputs[i],
                maximum_leaves,
            )?);
        }
        Ok(rows)
    }
}
