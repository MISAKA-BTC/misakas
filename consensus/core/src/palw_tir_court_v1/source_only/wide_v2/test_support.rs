//! Test-only read-set discovery, never an authenticated source or production builder.
use super::*;
use crate::{
    palw_artifact::PalwArtifactOperandV1,
    palw_positive_check_v2::expanded_plan::{ExpandedRelation, ExpandedRelationKind},
    palw_step_leg::PalwStepTileLeafV1,
};

pub(crate) struct ReadSet {
    pub steps: Vec<u64>,
    pub params: Vec<u32>,
    pub prompt: bool,
    pub decode: bool,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn read_set(
    binding: &PalwTirStepBindingV1,
    relation: &ExpandedRelation,
    steps: &[PalwStepTileLeafV1],
    params: &[PalwArtifactOperandV1],
    prompt: &[u32],
    generated: &[u32],
    rules: &PalwTirCourtRulesV1,
    limits: TirSourceOnlyLimitsV1,
) -> ReadSet {
    let verified = binding_geometry(binding, relation.legacy_frontier, rules).unwrap();
    let inventory = PalwTirInventoryIndexV1::new(verified.space().program()).unwrap();
    let target = match relation.kind {
        ExpandedRelationKind::Legacy(leaf) => Target::Legacy(leaf),
        ExpandedRelationKind::RecomputeNode { ctx, node, first_lane, lane_count, .. } => {
            Target::Node { ctx, node, first_lane, lane_count }
        }
    };
    let mut geometry = target_geometry(&verified, binding, relation.legacy_frontier, target, limits).unwrap();
    geometry.prepare_elements(target).unwrap();
    let mut source = TirSource {
        space: verified.space(),
        ctx: &binding.job_context,
        job: verified.job(),
        inventory: &inventory,
        units: Units {
            steps: steps.iter().enumerate().map(|(i, p)| (i as u64, p.values_le.clone())).collect(),
            params: params.iter().enumerate().map(|(i, p)| (i as u32, (p.row_start, p.bytes.clone()))).collect(),
            prompt: prompt.iter().copied().enumerate().map(|(i, v)| (i as u32, v)).collect(),
            generated: Some(generated.to_vec()),
            store: None,
            prompt_form: rules.prompt_form,
        },
        before: relation.legacy_frontier,
        used: Requests::default(),
        leaf_cache: BTreeMap::new(),
        supplied_ctx: None,
        supplied: BTreeMap::new(),
        used_supplied: BTreeSet::new(),
        read_budget: None,
    };
    evaluate_target(&verified, target, &geometry, &mut source, rules).unwrap();
    ReadSet {
        steps: source.used.steps.into_iter().collect(),
        params: source.used.params.into_iter().collect(),
        prompt: !source.used.prompt.is_empty(),
        decode: source.used.decode,
    }
}
