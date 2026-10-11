//! Typed prospective prefix-state continuation. The accepted job and immutable semantic
//! snapshot choose the relation; an accuser never supplies an initial cache or recurrence.
//! Old step leaves authenticate at their original hash before a completed verdict is usable.
//! This pure court is rooted/owned only by the outer signed staged-court admission.
use crate::Hash64;
use crate::palw_class_semantics_v1::{PalwClassSemanticsV1, PalwPrefixStateRelationV1};
use crate::palw_freeprompt_v3::PalwFreePromptJobV3;
use crate::palw_offence_attribution_v1::{PalwClaimSourceKindV1, PalwOffenceTargetV1, palw_constrained_job_target_v1};
use crate::palw_prefix_base0_codec_v1::PalwBase0PrefixDigestV1;
use crate::palw_prefix_family_state_v1::{PalwPrefixCacheUnitV1, PalwQwen36PrefixCacheDigestV1};
use crate::palw_prefix_gdn_replay_v1::PalwPrefixGdnHeadReplayV1;
use crate::palw_prefix_q36_public_rows_v1::PalwQ36PrefixPublicRowV1;
use crate::palw_prefix_state_court_v1::PalwPrefixKvTileV1;
use crate::palw_prefix_state_stream_v1::PalwPrefixStateStreamV1;
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_step::PalwStepCoordinateV1;
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPrefixCourtActionV1 {
    BeginTile {
        opening: PalwStepOpeningV1,
    },
    TilePart {
        offset: u32,
        bytes: Vec<u8>,
    },
    FinishTile,
    /// Unit and offset are the court's canonical cursor, never caller-selected state.
    PublicRow {
        length: u32,
        inputs: Vec<Vec<PalwPrefixKvTileV1>>,
    },
    BeginHead {
        parameters: Vec<crate::palw_artifact::PalwArtifactOpeningV1>,
    },
    HeadPosition {
        inputs: Vec<Vec<PalwPrefixKvTileV1>>,
    },
    /// Feed an internally completed head into the digest in bounded moves.
    HeadDigest {
        length: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Base0PendingTileV1 {
    coordinate: PalwStepCoordinateV1,
    length: u32,
    offset: u32,
    expected: Hash64,
    hash: LeafHashContinuationV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum FamilyV1 {
    Dense(Box<PalwPrefixStateStreamV1>),
    Base0 { digest: PalwBase0PrefixDigestV1, pending: Option<Base0PendingTileV1> },
    Q36 { digest: PalwQwen36PrefixCacheDigestV1, head: Option<Box<PalwPrefixGdnHeadReplayV1>> },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixFamilyCourtV1 {
    context: Hash64,
    claim: Hash64,
    binding: PalwStepBindingV2,
    job: Box<PalwFreePromptJobV3>,
    semantic: PalwClassSemanticsV1,
    named_root: Hash64,
    prefix_tokens: u32,
    maximum_leaves: u64,
    maximum_working_set_bytes: u64,
    family: FamilyV1,
}
impl PalwPrefixFamilyCourtV1 {
    pub fn new(
        context: Hash64,
        target: &PalwOffenceTargetV1,
        binding: &PalwStepBindingV2,
        job: &PalwFreePromptJobV3,
        maximum_leaves: u64,
        maximum_working_set_bytes: u64,
    ) -> Result<Self, String> {
        palw_constrained_job_target_v1(target, binding, job).map_err(|e| e.to_string())?;
        let semantic = target.registered_semantics.as_ref().ok_or("prefix claim has no immutable semantic snapshot")?;
        if !semantic.matches_context(&binding.job_context)
            || semantic.class_id != target.class_id
            || semantic.artifact_root != target.artifact_root
        {
            return Err("prefix semantic statement changes the accepted class/artifact/context".into());
        }
        let relation = semantic.prefix_state_relation.ok_or("historical prefix claim has no qualified family relation")?;
        relation.check_profile(&binding.shape_profile)?;
        let named = crate::palw_fp_prefix_v1::palw_fp_prefix_shape_v1(job).map_err(|e| e.to_string())?;
        if named.prefix_tokens >= binding.job_context.declared_prefill_tokens
            || crate::palw_step::step_leaf_count_capped_v1(&binding.shape_profile, &binding.job_context, maximum_leaves)
                .map_err(|e| e.to_string())?
                != binding.step_leaf_count
        {
            return Err("prefix is not a proper authenticated canonical step space".into());
        }
        let family = match relation {
            PalwPrefixStateRelationV1::DenseInt32KvV1 => FamilyV1::Dense(Box::new(PalwPrefixStateStreamV1::new(
                context,
                target.execution_root,
                binding,
                named.prefix_tokens,
                maximum_leaves,
            )?)),
            PalwPrefixStateRelationV1::Base0Int8KvV1 => {
                FamilyV1::Base0 { digest: PalwBase0PrefixDigestV1::new(&binding.shape_profile, named.prefix_tokens)?, pending: None }
            }
            PalwPrefixStateRelationV1::Qwen36HybridCacheV1 => {
                FamilyV1::Q36 { digest: PalwQwen36PrefixCacheDigestV1::new(&binding.shape_profile, named.prefix_tokens)?, head: None }
            }
        };
        Ok(Self {
            context,
            claim: target.claim_id,
            binding: binding.clone(),
            job: Box::new(job.clone()),
            semantic: semantic.clone(),
            named_root: named.state_root,
            prefix_tokens: named.prefix_tokens,
            maximum_leaves,
            maximum_working_set_bytes,
            family,
        })
    }
    fn target(&self) -> PalwOffenceTargetV1 {
        PalwOffenceTargetV1 {
            claim_id: self.claim,
            class_id: self.semantic.class_id,
            artifact_root: self.semantic.artifact_root,
            executor_bond: crate::palw_state_v2::PalwBondKeyV2(self.job.executor_bond),
            execution_root: self.binding.committed_execution_root,
            lane: Some(PalwClaimSourceKindV1::FreePrompt),
            segment_count: None,
            phase: None,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&self.binding.job_context),
            trace_root: self.binding.full_logits_trace_root,
            output_root: Hash64::default(),
            registered_semantics: Some(self.semantic.clone()),
        }
    }
    pub fn same_contract_v1(&self, other: &Self) -> bool {
        self.context == other.context
            && self.claim == other.claim
            && self.binding == other.binding
            && self.job == other.job
            && self.semantic == other.semantic
            && self.named_root == other.named_root
            && self.prefix_tokens == other.prefix_tokens
            && self.maximum_leaves == other.maximum_leaves
            && self.maximum_working_set_bytes == other.maximum_working_set_bytes
    }
    pub fn execution_root(&self) -> Hash64 {
        self.binding.committed_execution_root
    }
    pub fn check(&self) -> Result<(), String> {
        match &self.family {
            FamilyV1::Dense(c) => c.check(),
            FamilyV1::Base0 { digest, pending } => {
                let next = digest.next_coordinate(&self.binding.shape_profile)?;
                if let Some(p) = pending {
                    if p.offset > p.length
                        || p.length as usize > crate::palw_step_leg::PALW_STEP_LEG_MAX_TILE_BYTES
                        || p.hash.byte_count() != Some(282 + u128::from(p.offset))
                        || p.offset % 4 != 0
                    {
                        return Err("prefix old tile counters inconsistent".into());
                    }
                    // The tentative converter can advance its cursor after the final chunk;
                    // completion still requires FinishTile authenticating the old leaf.
                    if p.offset < p.length && next.is_none_or(|(c, _, offset)| c != p.coordinate || offset != p.offset / 4) {
                        return Err("BASE0 tentative converter changes the pending old tile cursor".into());
                    }
                }
                Ok(())
            }
            FamilyV1::Q36 { digest, head } => {
                if let Some(h) = head {
                    if !matches!(digest.next_unit(), Some((PalwPrefixCacheUnitV1::GdnHead { .. }, _, _)))
                        || h.next_position() > self.prefix_tokens
                        || h.resident_state_bytes() > self.maximum_working_set_bytes
                    {
                        return Err("prefix recurrence changes owned canonical unit or resource bounds".into());
                    }
                }
                Ok(())
            }
        }
    }
    pub fn movement(&mut self, context: Hash64, action: &PalwPrefixCourtActionV1) -> Result<(), String> {
        self.check()?;
        if context != self.context {
            return Err("prefix move changes owned seed context".into());
        }
        let target = self.target();
        match (&mut self.family, action) {
            (FamilyV1::Dense(c), PalwPrefixCourtActionV1::BeginTile { opening }) => c.begin_tile(context, c.next_tile(), opening)?,
            (FamilyV1::Dense(c), PalwPrefixCourtActionV1::TilePart { offset, bytes }) => {
                c.push_tile_bytes(context, c.next_tile(), *offset, bytes)?
            }
            (FamilyV1::Dense(c), PalwPrefixCourtActionV1::FinishTile) => c.finish_tile()?,
            (FamilyV1::Base0 { digest, pending }, PalwPrefixCourtActionV1::BeginTile { opening }) => {
                if pending.is_some() {
                    return Err("BASE0 prefix already has a pending old tile".into());
                }
                let (coordinate, count, offset) =
                    digest.next_coordinate(&self.binding.shape_profile)?.ok_or("BASE0 prefix complete")?;
                if offset != 0 {
                    return Err("BASE0 tile starts after its canonical beginning".into());
                }
                let index =
                    crate::palw_step::canonical_step_leaf_index(&self.binding.shape_profile, &self.binding.job_context, &coordinate)
                        .ok_or("BASE0 coordinate absent")?;
                if opening.leaf_index != index
                    || crate::palw_step_leg::step_opening_root_capped_v1(self.binding.step_leaf_count, opening, self.maximum_leaves)
                        .map_err(|e| e.to_string())?
                        != self.binding.step_merkle_root
                {
                    return Err("BASE0 tile opening is not the original claim leaf".into());
                }
                let length = count.checked_mul(4).ok_or("BASE0 tile bytes overflow")?;
                let mut hash = LeafHashContinuationV1::new(crate::palw_step_leg::PALW_STEP_LEG_DOMAIN_TILE_LEAF);
                hash.update(&crate::palw_step::PALW_STEP_OBJECT_VERSION_V1.to_le_bytes());
                hash.update(self.binding.job_context.leaf_context_hash_v1(coordinate.call_index, coordinate.position).as_byte_slice());
                hash.update(self.binding.shape_profile.shape_profile_id().as_byte_slice());
                for n in [coordinate.call_index, coordinate.node_slot, coordinate.position, coordinate.tile_index, count, length] {
                    hash.update(&n.to_le_bytes());
                }
                *pending = Some(Base0PendingTileV1 { coordinate, length, offset: 0, expected: opening.leaf_hash, hash });
            }
            (FamilyV1::Base0 { digest, pending }, PalwPrefixCourtActionV1::TilePart { offset, bytes }) => {
                let p = pending.as_ref().ok_or("BASE0 old tile opening missing")?;
                if *offset != p.offset
                    || bytes.is_empty()
                    || bytes.len() > 65_536
                    || bytes.len() % 4 != 0
                    || bytes.len() as u64 > u64::from(p.length - p.offset)
                {
                    return Err("BASE0 tile chunk changes bounded ordered byte interval".into());
                }
                digest.push_values(&self.binding.shape_profile, p.coordinate, *offset / 4, bytes)?;
                let p = pending.as_mut().unwrap();
                p.hash.update(bytes);
                p.offset += bytes.len() as u32;
            }
            (FamilyV1::Base0 { pending, .. }, PalwPrefixCourtActionV1::FinishTile) => {
                let p = pending.as_ref().ok_or("BASE0 old tile opening missing")?;
                if p.offset != p.length || p.hash.finish() != p.expected {
                    return Err("BASE0 old whole tile has not authenticated".into());
                }
                *pending = None;
            }
            (FamilyV1::Q36 { digest, head }, PalwPrefixCourtActionV1::PublicRow { length, inputs }) => {
                if head.is_some() {
                    return Err("Q36 recurrence must complete before another public row".into());
                }
                let (unit, _, offset) = digest.next_unit().ok_or("Q36 cache complete")?;
                let row = PalwQ36PrefixPublicRowV1::new(&self.binding, self.prefix_tokens, unit, offset, u64::from(*length))?;
                let bytes = row.authenticate(&self.binding, self.prefix_tokens, inputs, self.maximum_leaves)?;
                digest.push(unit, offset, &bytes)?;
            }
            (FamilyV1::Q36 { digest, head }, PalwPrefixCourtActionV1::BeginHead { parameters }) => {
                let Some((PalwPrefixCacheUnitV1::GdnHead { layer, head: h }, _, 0)) = digest.next_unit() else {
                    return Err("Q36 owned recurrence is not the next untouched canonical cache unit".into());
                };
                if head.is_some() {
                    return Err("Q36 recurrence already owned".into());
                }
                *head = Some(Box::new(PalwPrefixGdnHeadReplayV1::new(
                    &target,
                    &self.binding,
                    &self.job,
                    layer,
                    h,
                    parameters,
                    self.maximum_working_set_bytes,
                )?));
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadPosition { inputs }) => {
                head.as_mut().ok_or("Q36 owned recurrence not initialized")?.advance(
                    &target,
                    &self.binding,
                    inputs,
                    self.maximum_leaves,
                )?;
            }
            (FamilyV1::Q36 { digest, head }, PalwPrefixCourtActionV1::HeadDigest { length }) => {
                let h = head.as_ref().ok_or("Q36 owned recurrence not initialized")?;
                let matrix = h.completed_matrix()?;
                let (unit, total, offset) = digest.next_unit().ok_or("Q36 cache complete")?;
                if *length == 0 || *length > 65_536 || length % 4 != 0 || u64::from(*length) > total - offset {
                    return Err("Q36 completed head chunk changes bounded canonical interval".into());
                }
                let start = usize::try_from(offset / 4).map_err(|_| "head index overflows")?;
                let end = start
                    .checked_add(*length as usize / 4)
                    .filter(|n| *n <= matrix.len())
                    .ok_or("head chunk outside completed state")?;
                let bytes: Vec<u8> = matrix[start..end].iter().flat_map(|v| v.to_le_bytes()).collect();
                digest.push(unit, offset, &bytes)?;
                if offset + u64::from(*length) == total {
                    *head = None;
                }
            }
            _ => return Err("prefix move is for another immutable family/phase".into()),
        }
        self.check()
    }
    pub fn finish(&self) -> Result<bool, String> {
        self.check()?;
        let root = match &self.family {
            FamilyV1::Dense(c) => c.finish()?,
            FamilyV1::Base0 { digest, pending: None } => digest.finish(&self.binding.shape_profile)?,
            FamilyV1::Q36 { digest, head: None } => digest.finish()?,
            _ => return Err("prefix still has unauthenticated old bytes or incomplete recurrence".into()),
        };
        Ok(root != self.named_root)
    }
}
