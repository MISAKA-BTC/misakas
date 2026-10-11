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
use crate::palw_prefix_gdn_contract_v1::PalwPrefixGdnContractV1;
use crate::palw_prefix_gdn_paged_v1::{PALW_GDN_PAGE_WORDS_V1, PalwGdnOwnedPageOpeningV1, PalwGdnPagedStateV1};
use crate::palw_prefix_gdn_replay_v1::PalwPrefixGdnHeadReplayV1;
use crate::palw_prefix_q36_public_rows_v1::PalwQ36PrefixPublicRowV1;
use crate::palw_prefix_source_stream_v1::{PalwPrefixSourceRangeV1, PalwPrefixSourceStreamV1};
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
    /// An alternate bounded representation of the same registered head relation.
    BeginPagedHead {
        parameters: Vec<crate::palw_artifact::PalwArtifactOpeningV1>,
    },
    HeadPageRead {
        opening: PalwGdnOwnedPageOpeningV1,
    },
    HeadPageWrite {
        opening: PalwGdnOwnedPageOpeningV1,
    },
    HeadPageDigest {
        opening: PalwGdnOwnedPageOpeningV1,
    },
    HeadInputOpening {
        opening: PalwStepOpeningV1,
    },
    HeadInputPart {
        offset: u32,
        bytes: Vec<u8>,
    },
    HeadInputFinish,
    BeginPublicRow {
        length: u32,
    },
    PublicInputOpening {
        opening: PalwStepOpeningV1,
    },
    PublicInputPart {
        offset: u32,
        bytes: Vec<u8>,
    },
    PublicInputFinish,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPrefixFamilyCursorV1 {
    TileOpening { ordinal: u64 },
    TileBytes { ordinal: u64, length: u32, offset: u32 },
    TileFinish { ordinal: u64 },
    PublicRow { unit: PalwPrefixCacheUnitV1, length: u64, offset: u64 },
    HeadParameters { layer: u16, head: u32 },
    HeadInputs { layer: u16, head: u32, position: u32 },
    HeadDigest { layer: u16, head: u32, length: u64, offset: u64 },
    Complete,
    HeadPage { layer: u16, head: u32, position: u32, index: u64, write: bool, root: Hash64 },
    HeadPageDigest { layer: u16, head: u32, index: u64, length: u64, offset: u64, root: Hash64 },
    HeadInputOpening { layer: u16, head: u32, position: u32, input: u8, coordinate: PalwStepCoordinateV1 },
    HeadInputBytes { layer: u16, head: u32, position: u32, input: u8, coordinate: PalwStepCoordinateV1, length: u32, offset: u32 },
    HeadInputFinish { layer: u16, head: u32, position: u32, input: u8, coordinate: PalwStepCoordinateV1 },
    PublicInputOpening { unit: PalwPrefixCacheUnitV1, source: u32, coordinate: PalwStepCoordinateV1 },
    PublicInputBytes { unit: PalwPrefixCacheUnitV1, source: u32, coordinate: PalwStepCoordinateV1, length: u32, offset: u32 },
    PublicInputFinish { unit: PalwPrefixCacheUnitV1, source: u32, coordinate: PalwStepCoordinateV1 },
}
/// A mandatory custodian may answer only this canonical ORIGINAL public step source.
/// Challenger-owned recurrence pages are never a producer/Panel DA obligation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixPublicSourceV1 {
    pub coordinate: PalwStepCoordinateV1,
    pub leaf_index: u64,
    pub value_count: u32,
    pub length: u32,
    pub offset: u32,
    pub needs_opening: bool,
    pub execution_root: Hash64,
    pub step_root: Hash64,
    pub step_leaf_count: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct PublicInputsV1 {
    unit: PalwPrefixCacheUnitV1,
    offset: u64,
    length: u64,
    sources: Vec<PalwPrefixSourceRangeV1>,
    source: u32,
    bytes: Vec<u8>,
    stream: PalwPrefixSourceStreamV1,
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
struct HeadInputsV1 {
    rows: Vec<Vec<i32>>,
    stream: PalwPrefixSourceStreamV1,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum Q36HeadV1 {
    Whole(Box<PalwPrefixGdnHeadReplayV1>),
    Paged { contract: PalwPrefixGdnContractV1, state: Box<PalwGdnPagedStateV1>, input: Option<HeadInputsV1> },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum FamilyV1 {
    Dense(Box<PalwPrefixStateStreamV1>),
    Base0 { digest: PalwBase0PrefixDigestV1, pending: Option<Base0PendingTileV1> },
    Q36 { digest: PalwQwen36PrefixCacheDigestV1, head: Option<Box<Q36HeadV1>>, public: Option<Box<PublicInputsV1>> },
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
/// Mutable continuation only; the immutable binding, job and semantic statement remain in
/// their original rooted row instead of being copied into every journal delta.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum CheckpointFamilyV1 {
    Dense(crate::palw_prefix_state_stream_v1::PalwPrefixStreamCheckpointV1),
    Base0 { digest: PalwBase0PrefixDigestV1, pending: Option<Base0PendingTileV1> },
    Q36 { digest: PalwQwen36PrefixCacheDigestV1, head: Option<Box<Q36HeadV1>>, public: Option<Box<PublicInputsV1>> },
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPrefixFamilyCheckpointV1 {
    context: Hash64,
    claim: Hash64,
    execution_root: Hash64,
    family: CheckpointFamilyV1,
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
            PalwPrefixStateRelationV1::Qwen36HybridCacheV1 => FamilyV1::Q36 {
                digest: PalwQwen36PrefixCacheDigestV1::new(&binding.shape_profile, named.prefix_tokens)?,
                head: None,
                public: None,
            },
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
    pub fn cursor_v1(&self) -> Result<PalwPrefixFamilyCursorV1, String> {
        self.check()?;
        let tile = |ordinal, length, offset| {
            if length == offset {
                PalwPrefixFamilyCursorV1::TileFinish { ordinal }
            } else {
                PalwPrefixFamilyCursorV1::TileBytes { ordinal, length, offset }
            }
        };
        match &self.family {
            FamilyV1::Dense(c) => Ok(if c.next_tile() == c.total_tiles() {
                PalwPrefixFamilyCursorV1::Complete
            } else if let Some((length, offset)) = c.pending_tile_v1() {
                tile(c.next_tile(), length, offset)
            } else {
                PalwPrefixFamilyCursorV1::TileOpening { ordinal: c.next_tile() }
            }),
            FamilyV1::Base0 { digest, pending } => {
                if let Some(p) = pending {
                    // Converter may already advance after the final bytes, while old leaf
                    // authentication still awaits its finish move.
                    let ordinal = digest.next_tile_ordinal_v1() - u64::from(p.offset == p.length);
                    Ok(tile(ordinal, p.length, p.offset))
                } else if digest.next_coordinate(&self.binding.shape_profile)?.is_some() {
                    Ok(PalwPrefixFamilyCursorV1::TileOpening { ordinal: digest.next_tile_ordinal_v1() })
                } else {
                    Ok(PalwPrefixFamilyCursorV1::Complete)
                }
            }
            FamilyV1::Q36 { digest, head, public } => {
                if let Some(p) = public {
                    let (coordinate, pending) = p.stream.cursor_v1().ok_or("public source complete without transition")?;
                    Ok(match pending {
                        None => PalwPrefixFamilyCursorV1::PublicInputOpening { unit: p.unit, source: p.source, coordinate },
                        Some((length, offset)) if length == offset => {
                            PalwPrefixFamilyCursorV1::PublicInputFinish { unit: p.unit, source: p.source, coordinate }
                        }
                        Some((length, offset)) => {
                            PalwPrefixFamilyCursorV1::PublicInputBytes { unit: p.unit, source: p.source, coordinate, length, offset }
                        }
                    })
                } else {
                    match digest.next_unit() {
                        None => Ok(PalwPrefixFamilyCursorV1::Complete),
                        Some((PalwPrefixCacheUnitV1::GdnHead { layer, head: h }, length, offset)) => Ok(match head {
                            None => PalwPrefixFamilyCursorV1::HeadParameters { layer, head: h },
                            Some(r) => match r.as_ref() {
                                Q36HeadV1::Whole(r) if r.next_position() < self.prefix_tokens => {
                                    PalwPrefixFamilyCursorV1::HeadInputs { layer, head: h, position: r.next_position() }
                                }
                                Q36HeadV1::Whole(_) => PalwPrefixFamilyCursorV1::HeadDigest { layer, head: h, length, offset },
                                Q36HeadV1::Paged { state, input, .. } => {
                                    if let Some(input) = input {
                                        let (coordinate, pending) =
                                            input.stream.cursor_v1().ok_or("input stream complete without transition")?;
                                        match pending {
                                            None => PalwPrefixFamilyCursorV1::HeadInputOpening {
                                                layer,
                                                head: h,
                                                position: state.position(),
                                                input: input.rows.len() as u8,
                                                coordinate,
                                            },
                                            Some((length, offset)) if length == offset => PalwPrefixFamilyCursorV1::HeadInputFinish {
                                                layer,
                                                head: h,
                                                position: state.position(),
                                                input: input.rows.len() as u8,
                                                coordinate,
                                            },
                                            Some((length, offset)) => PalwPrefixFamilyCursorV1::HeadInputBytes {
                                                layer,
                                                head: h,
                                                position: state.position(),
                                                input: input.rows.len() as u8,
                                                coordinate,
                                                length,
                                                offset,
                                            },
                                        }
                                    } else if let Some((index, write)) = state.next_page() {
                                        PalwPrefixFamilyCursorV1::HeadPage {
                                            layer,
                                            head: h,
                                            position: state.position(),
                                            index,
                                            write,
                                            root: state.root(),
                                        }
                                    } else if !state.complete() {
                                        PalwPrefixFamilyCursorV1::HeadInputs { layer, head: h, position: state.position() }
                                    } else {
                                        PalwPrefixFamilyCursorV1::HeadPageDigest {
                                            layer,
                                            head: h,
                                            index: self.completed_page_index_v1(offset)?,
                                            length,
                                            offset,
                                            root: state.root(),
                                        }
                                    }
                                }
                            },
                        }),
                        Some((unit, length, offset)) => Ok(PalwPrefixFamilyCursorV1::PublicRow { unit, length, offset }),
                    }
                }
            }
        }
    }
    pub fn next_public_source_v1(&self) -> Result<Option<PalwPrefixPublicSourceV1>, String> {
        self.check()?;
        let cursor = self.cursor_v1()?;
        let (coordinate, count, offset, needs_opening) = match cursor {
            PalwPrefixFamilyCursorV1::TileOpening { ordinal } | PalwPrefixFamilyCursorV1::TileBytes { ordinal, .. } => {
                let plan = crate::palw_prefix_state_court_v1::PalwPrefixKvPlanV1::of_profile(
                    &self.binding.shape_profile,
                    self.prefix_tokens,
                )?;
                let (coord, count) =
                    plan.coordinate(&self.binding.shape_profile, ordinal).ok_or("public KV source coordinate missing")?;
                let (offset, opening) = match cursor {
                    PalwPrefixFamilyCursorV1::TileBytes { offset, .. } => (offset, false),
                    _ => (0, true),
                };
                (coord, count, offset, opening)
            }
            PalwPrefixFamilyCursorV1::HeadInputOpening { coordinate, .. }
            | PalwPrefixFamilyCursorV1::HeadInputBytes { coordinate, .. }
            | PalwPrefixFamilyCursorV1::PublicInputOpening { coordinate, .. }
            | PalwPrefixFamilyCursorV1::PublicInputBytes { coordinate, .. } => {
                let source = match &self.family {
                    FamilyV1::Q36 { public: Some(p), .. } => p.stream.source_v1(),
                    FamilyV1::Q36 { head: Some(h), .. } => match h.as_ref() {
                        Q36HeadV1::Paged { input: Some(i), .. } => i.stream.source_v1(),
                        _ => return Err("public source has no owned stream".into()),
                    },
                    _ => return Err("public source has no owned stream".into()),
                };
                let count = u32::try_from(
                    u64::from(source.tile_len).min(source.width - u64::from(coordinate.tile_index) * u64::from(source.tile_len)),
                )
                .map_err(|_| "public source count overflow")?;
                let (offset, opening) = match cursor {
                    PalwPrefixFamilyCursorV1::HeadInputBytes { offset, .. }
                    | PalwPrefixFamilyCursorV1::PublicInputBytes { offset, .. } => (offset, false),
                    _ => (0, true),
                };
                (coordinate, count, offset, opening)
            }
            _ => return Ok(None),
        };
        Ok(Some(PalwPrefixPublicSourceV1 {
            coordinate,
            leaf_index: crate::palw_step::canonical_step_leaf_index(
                &self.binding.shape_profile,
                &self.binding.job_context,
                &coordinate,
            )
            .ok_or("public source is not canonical")?,
            value_count: count,
            length: count.checked_mul(4).ok_or("public source length overflow")?,
            offset,
            needs_opening,
            execution_root: self.execution_root(),
            step_root: self.binding.step_merkle_root,
            step_leaf_count: self.binding.step_leaf_count,
        }))
    }
    fn completed_page_index_v1(&self, offset: u64) -> Result<u64, String> {
        let dk = u64::from(self.binding.shape_profile.gdn_head_k_dim);
        if dk == 0 || offset % 4 != 0 {
            return Err("completed page has invalid geometry/offset".into());
        }
        let word = offset / 4;
        let column = word % dk;
        if column % PALW_GDN_PAGE_WORDS_V1 as u64 != 0 {
            return Err("completed page is not at a canonical page boundary".into());
        }
        Ok((word / dk) * dk.div_ceil(PALW_GDN_PAGE_WORDS_V1 as u64) + column / PALW_GDN_PAGE_WORDS_V1 as u64)
    }
    /// The outsider stores only deterministic transition output; no producer matrix is used.
    pub fn preview_page_write_v1(&self, opening: &PalwGdnOwnedPageOpeningV1) -> Result<Vec<i32>, String> {
        self.check()?;
        let FamilyV1::Q36 { head: Some(h), .. } = &self.family else {
            return Err("no owned Q36 head".into());
        };
        let Q36HeadV1::Paged { state, .. } = h.as_ref() else {
            return Err("head is not paged".into());
        };
        state.as_ref().clone().write_page(self.context, opening)
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
                    if p.length == 0
                        || p.offset > p.length
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
            FamilyV1::Q36 { digest, head, public } => {
                if let Some(p) = public {
                    if head.is_some()
                        || digest
                            .next_unit()
                            .is_none_or(|(unit, total, offset)| unit != p.unit || offset != p.offset || p.length > total - offset)
                        || p.source as usize >= p.sources.len()
                    {
                        return Err("public cache source changes owned canonical interval/phase".into());
                    }
                    let plan = PalwQ36PrefixPublicRowV1::new(&self.binding, self.prefix_tokens, p.unit, p.offset, p.length)?;
                    let sources: Vec<_> = plan
                        .sources
                        .iter()
                        .map(|s| PalwPrefixSourceRangeV1 {
                            node_slot: s.node_slot,
                            position: s.position,
                            tile_len: s.tile_len,
                            width: s.row_values,
                            start: s.start,
                            end: s.end,
                        })
                        .collect();
                    if p.sources != sources
                        || !p.stream.matches_v1(self.context, self.execution_root(), p.sources[p.source as usize])
                        || p.bytes.len() as u64 != p.sources[..p.source as usize].iter().map(|s| (s.end - s.start) * 4).sum::<u64>()
                    {
                        return Err("public cache source changes registered provenance/retained prefix".into());
                    }
                    p.stream.check()?;
                }
                if let Some(h) = head {
                    let Some((PalwPrefixCacheUnitV1::GdnHead { layer, head }, _, _)) = digest.next_unit() else {
                        return Err("prefix recurrence is not the owned canonical unit".into());
                    };
                    match h.as_ref() {
                        Q36HeadV1::Whole(h) => {
                            if !h.matches_contract_v1(self.claim, &self.binding, layer, head, self.prefix_tokens)
                                || h.next_position() > self.prefix_tokens
                                || h.resident_state_bytes() > self.maximum_working_set_bytes
                            {
                                return Err("prefix recurrence changes owned canonical unit or resource bounds".into());
                            }
                        }
                        Q36HeadV1::Paged { contract, state, input } => {
                            if contract.claim != self.claim
                                || contract.execution_root != self.execution_root()
                                || contract.job_id != self.binding.job_context.job_id
                                || contract.layer != layer
                                || contract.head != head
                                || contract.prefix_tokens != self.prefix_tokens
                                || contract.d_k != self.binding.shape_profile.gdn_head_k_dim
                                || contract.d_v != self.binding.shape_profile.gdn_head_v_dim
                                || !state.matches_contract_v1(
                                    self.context,
                                    contract.prefix_tokens,
                                    contract.d_k,
                                    contract.d_v,
                                    &contract.parameters,
                                )
                                || state.resident_bytes_v1() > self.maximum_working_set_bytes
                            {
                                return Err("paged recurrence changes immutable contract or resident bound".into());
                            }
                            if let Some(i) = input {
                                if state.next_page().is_some() || state.complete() || i.rows.len() >= 5 {
                                    return Err("streamed head input changes recurrence phase".into());
                                }
                                let source = contract.source_range_v1(&self.binding, state.position(), i.rows.len())?;
                                if !i.stream.matches_v1(self.context, self.execution_root(), source) {
                                    return Err("streamed input changes canonical source".into());
                                }
                                for (ordinal, row) in i.rows.iter().enumerate() {
                                    let source = contract.source_range_v1(&self.binding, state.position(), ordinal)?;
                                    if row.len() as u64 != source.end - source.start {
                                        return Err("streamed head row has noncanonical retained count".into());
                                    }
                                }
                                i.stream.check()?;
                            } else if state.next_page().is_none() && !state.complete() {
                                return Err("paged recurrence has no owned input phase".into());
                            }
                            state.check()?;
                        }
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
            (FamilyV1::Q36 { digest, head, public }, PalwPrefixCourtActionV1::PublicRow { length, inputs }) => {
                if head.is_some() || public.is_some() {
                    return Err("Q36 owned source must complete before another public row".into());
                }
                let (unit, _, offset) = digest.next_unit().ok_or("Q36 cache complete")?;
                let row = PalwQ36PrefixPublicRowV1::new(&self.binding, self.prefix_tokens, unit, offset, u64::from(*length))?;
                let bytes = row.authenticate(&self.binding, self.prefix_tokens, inputs, self.maximum_leaves)?;
                digest.push(unit, offset, &bytes)?;
            }
            (FamilyV1::Q36 { digest, head, public }, PalwPrefixCourtActionV1::BeginHead { parameters }) => {
                let Some((PalwPrefixCacheUnitV1::GdnHead { layer, head: h }, _, 0)) = digest.next_unit() else {
                    return Err("Q36 owned recurrence is not the next untouched canonical cache unit".into());
                };
                if head.is_some() || public.is_some() {
                    return Err("Q36 source already owned".into());
                }
                *head = Some(Box::new(Q36HeadV1::Whole(Box::new(PalwPrefixGdnHeadReplayV1::new(
                    &target,
                    &self.binding,
                    &self.job,
                    layer,
                    h,
                    parameters,
                    self.maximum_working_set_bytes,
                )?))));
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadPosition { inputs }) => {
                match head.as_mut().ok_or("Q36 owned recurrence not initialized")?.as_mut() {
                    Q36HeadV1::Whole(h) => h.advance(&target, &self.binding, inputs, self.maximum_leaves)?,
                    Q36HeadV1::Paged { .. } => return Err("paged recurrence requires streamed original input leaves".into()),
                }
            }
            (FamilyV1::Q36 { digest, head, .. }, PalwPrefixCourtActionV1::HeadDigest { length }) => {
                let Q36HeadV1::Whole(h) = head.as_ref().ok_or("Q36 owned recurrence not initialized")?.as_ref() else {
                    return Err("paged recurrence digest requires its authenticated completed page".into());
                };
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
            (FamilyV1::Q36 { digest, head, public }, PalwPrefixCourtActionV1::BeginPagedHead { parameters }) => {
                let Some((PalwPrefixCacheUnitV1::GdnHead { layer, head: h }, _, 0)) = digest.next_unit() else {
                    return Err("paged head is not the next untouched canonical unit".into());
                };
                if head.is_some() || public.is_some() {
                    return Err("source already owned".into());
                }
                let contract = PalwPrefixGdnContractV1::new(&target, &self.binding, &self.job, layer, h, parameters)?;
                let state =
                    PalwGdnPagedStateV1::new(context, contract.prefix_tokens, contract.d_k, contract.d_v, contract.parameters)?;
                if state.resident_bytes_v1() > self.maximum_working_set_bytes {
                    return Err("paged head exceeds granted resident envelope".into());
                }
                let source = contract.source_range_v1(&self.binding, state.position(), 0)?;
                let input = Some(HeadInputsV1 {
                    rows: vec![],
                    stream: PalwPrefixSourceStreamV1::new(context, self.binding.committed_execution_root, source)?,
                });
                *head = Some(Box::new(Q36HeadV1::Paged { contract, state: Box::new(state), input }));
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadPageRead { opening }) => {
                let Q36HeadV1::Paged { state, .. } = head.as_mut().ok_or("head not owned")?.as_mut() else {
                    return Err("head is not paged".into());
                };
                state.read_page(context, opening)?;
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadPageWrite { opening }) => {
                let Q36HeadV1::Paged { state, contract, input } = head.as_mut().ok_or("head not owned")?.as_mut() else {
                    return Err("head is not paged".into());
                };
                state.write_page(context, opening)?;
                if state.next_page().is_none() && !state.complete() {
                    let source = contract.source_range_v1(&self.binding, state.position(), 0)?;
                    *input = Some(HeadInputsV1 {
                        rows: vec![],
                        stream: PalwPrefixSourceStreamV1::new(context, self.binding.committed_execution_root, source)?,
                    });
                }
            }
            (FamilyV1::Q36 { digest, head, .. }, PalwPrefixCourtActionV1::HeadPageDigest { opening }) => {
                let Q36HeadV1::Paged { contract, state, .. } = head.as_ref().ok_or("head not owned")?.as_ref() else {
                    return Err("head is not paged".into());
                };
                let (unit, total, offset) = digest.next_unit().ok_or("cache complete")?;
                let word = offset / 4;
                let column = word % u64::from(contract.d_k);
                if offset % 4 != 0 || column % PALW_GDN_PAGE_WORDS_V1 as u64 != 0 {
                    return Err("completed page starts outside canonical row/page interval".into());
                }
                let index = (word / u64::from(contract.d_k)) * u64::from(contract.d_k.div_ceil(PALW_GDN_PAGE_WORDS_V1 as u32))
                    + column / PALW_GDN_PAGE_WORDS_V1 as u64;
                let values = state.completed_page(context, index, opening)?;
                let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
                if bytes.is_empty() || offset + bytes.len() as u64 > total {
                    return Err("completed page exceeds canonical cache unit".into());
                }
                digest.push(unit, offset, &bytes)?;
                if offset + bytes.len() as u64 == total {
                    *head = None;
                }
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadInputOpening { opening }) => {
                let Q36HeadV1::Paged { input: Some(input), .. } = head.as_mut().ok_or("head not owned")?.as_mut() else {
                    return Err("head has no streamed input phase".into());
                };
                input.stream.begin_tile(context, &self.binding, opening, self.maximum_leaves)?;
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadInputPart { offset, bytes }) => {
                let Q36HeadV1::Paged { input: Some(input), .. } = head.as_mut().ok_or("head not owned")?.as_mut() else {
                    return Err("head has no streamed input phase".into());
                };
                input.stream.push_part(context, *offset, bytes)?;
            }
            (FamilyV1::Q36 { head, .. }, PalwPrefixCourtActionV1::HeadInputFinish) => {
                let Q36HeadV1::Paged { contract, state, input } = head.as_mut().ok_or("head not owned")?.as_mut() else {
                    return Err("head is not paged".into());
                };
                let i = input.as_mut().ok_or("head has no streamed input phase")?;
                i.stream.finish_tile(context)?;
                if i.stream.complete() {
                    i.rows.push(i.stream.authenticated_values_v1()?.to_vec());
                    if i.rows.len() == 5 {
                        if i.rows[3].len() != 1 || i.rows[4].len() != 1 {
                            return Err("head gates are not scalars".into());
                        }
                        state.begin_position(
                            context,
                            &i.rows[0],
                            &i.rows[1],
                            &i.rows[2],
                            i64::from(i.rows[3][0]),
                            i64::from(i.rows[4][0]),
                        )?;
                        *input = None;
                    } else {
                        let source = contract.source_range_v1(&self.binding, state.position(), i.rows.len())?;
                        i.stream = PalwPrefixSourceStreamV1::new(context, self.binding.committed_execution_root, source)?;
                    }
                }
            }
            (FamilyV1::Q36 { digest, head, public }, PalwPrefixCourtActionV1::BeginPublicRow { length }) => {
                if head.is_some() || public.is_some() {
                    return Err("Q36 source already owned".into());
                }
                let (unit, _, offset) = digest.next_unit().ok_or("cache complete")?;
                let row = PalwQ36PrefixPublicRowV1::new(&self.binding, self.prefix_tokens, unit, offset, u64::from(*length))?;
                if row.sources.is_empty() {
                    digest.push(unit, offset, &vec![0; *length as usize])?;
                } else {
                    let sources: Vec<_> = row
                        .sources
                        .iter()
                        .map(|s| PalwPrefixSourceRangeV1 {
                            node_slot: s.node_slot,
                            position: s.position,
                            tile_len: s.tile_len,
                            width: s.row_values,
                            start: s.start,
                            end: s.end,
                        })
                        .collect();
                    let stream = PalwPrefixSourceStreamV1::new(context, self.binding.committed_execution_root, sources[0])?;
                    *public = Some(Box::new(PublicInputsV1 {
                        unit,
                        offset,
                        length: u64::from(*length),
                        sources,
                        source: 0,
                        bytes: vec![],
                        stream,
                    }));
                }
            }
            (FamilyV1::Q36 { public: Some(p), .. }, PalwPrefixCourtActionV1::PublicInputOpening { opening }) => {
                p.stream.begin_tile(context, &self.binding, opening, self.maximum_leaves)?
            }
            (FamilyV1::Q36 { public: Some(p), .. }, PalwPrefixCourtActionV1::PublicInputPart { offset, bytes }) => {
                p.stream.push_part(context, *offset, bytes)?
            }
            (FamilyV1::Q36 { digest, public: Some(p), .. }, PalwPrefixCourtActionV1::PublicInputFinish) => {
                p.stream.finish_tile(context)?;
                if p.stream.complete() {
                    p.bytes.extend(p.stream.authenticated_values_v1()?.iter().flat_map(|v| v.to_le_bytes()));
                    p.source += 1;
                    if p.source as usize == p.sources.len() {
                        if p.bytes.len() as u64 != p.length {
                            return Err("streamed public cache sources do not exactly cover interval".into());
                        }
                        digest.push(p.unit, p.offset, &p.bytes)?;
                    } else {
                        p.stream = PalwPrefixSourceStreamV1::new(
                            context,
                            self.binding.committed_execution_root,
                            p.sources[p.source as usize],
                        )?;
                    }
                }
            }
            _ => return Err("prefix move is for another immutable family/phase".into()),
        }
        if let FamilyV1::Q36 { public, .. } = &mut self.family {
            if public.as_ref().is_some_and(|p| p.source as usize == p.sources.len()) {
                *public = None;
            }
        }
        self.check()
    }
    pub(crate) fn checkpoint_v1(&self) -> PalwPrefixFamilyCheckpointV1 {
        let family = match &self.family {
            FamilyV1::Dense(c) => CheckpointFamilyV1::Dense(c.checkpoint_v1()),
            FamilyV1::Base0 { digest, pending } => CheckpointFamilyV1::Base0 { digest: digest.clone(), pending: pending.clone() },
            FamilyV1::Q36 { digest, head, public } => {
                CheckpointFamilyV1::Q36 { digest: digest.clone(), head: head.clone(), public: public.clone() }
            }
        };
        PalwPrefixFamilyCheckpointV1 { context: self.context, claim: self.claim, execution_root: self.execution_root(), family }
    }
    pub(crate) fn restore_checkpoint_v1(&mut self, cp: &PalwPrefixFamilyCheckpointV1) -> Result<(), String> {
        if cp.context != self.context || cp.claim != self.claim || cp.execution_root != self.execution_root() {
            return Err("prefix continuation checkpoint changes immutable seed/claim/root".into());
        }
        // Check a temporary continuation before changing the rooted row. Dense and BASE0
        // are bounded; a Q36 whole-head CP currently requires the declared head budget.
        let mut candidate = self.clone();
        match (&mut candidate.family, &cp.family) {
            (FamilyV1::Dense(c), CheckpointFamilyV1::Dense(p)) => c.restore_checkpoint_v1(p)?,
            (FamilyV1::Base0 { digest, pending }, CheckpointFamilyV1::Base0 { digest: d, pending: p }) => {
                *digest = d.clone();
                *pending = p.clone();
            }
            (FamilyV1::Q36 { digest, head, public }, CheckpointFamilyV1::Q36 { digest: d, head: h, public: p }) => {
                *digest = d.clone();
                *head = h.clone();
                *public = p.clone();
            }
            _ => return Err("prefix continuation checkpoint changes immutable family".into()),
        }
        candidate.check()?;
        self.family = candidate.family;
        Ok(())
    }
    pub fn finish(&self) -> Result<bool, String> {
        self.check()?;
        let root = match &self.family {
            FamilyV1::Dense(c) => c.finish()?,
            FamilyV1::Base0 { digest, pending: None } => digest.finish(&self.binding.shape_profile)?,
            FamilyV1::Q36 { digest, head: None, public: None } => digest.finish()?,
            _ => return Err("prefix still has unauthenticated old bytes or incomplete recurrence".into()),
        };
        Ok(root != self.named_root)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    /// Complete public grammar fixture for rooted staged-court tests. The registered graph
    /// is the canonical BASE0 i8 family, not an A16 alias; native family goldens are separate.
    pub(crate) fn qualified_prefix_case_for_tests_v1(
        fault: bool,
        inherited: bool,
    ) -> (PalwOffenceTargetV1, PalwStepBindingV2, PalwFreePromptJobV3, Vec<PalwPrefixKvTileV1>) {
        use crate::palw_step_leg::{PalwStepMerkleTreeV1, binding_commitment_root_v1, step_tile_leaf_hash_ctx_v1};
        let (mut binding, mut tiles, _) = crate::palw_prefix_state_court_v1::tests::fixture(false);
        let (_, accusation) = crate::palw_decode_constraint_v1::tests::public_constrained_case_for_tests_v1(false);
        let mut job = accusation.job;
        job.version = if inherited { 12 } else { 11 };
        job.class_id = binding.shape_profile.shape_profile_id();
        job.prompt_tokens = 4;
        job.decode_token_limit = 2;
        job.max_context_tokens = binding.shape_profile.n_ctx;
        job.decode = Some(crate::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);
        job.prompt_token_ids_hash = binding.job_context.prompt_token_ids_hash;
        let mut digest = PalwBase0PrefixDigestV1::new(&binding.shape_profile, 2).unwrap();
        for (ordinal, tile) in tiles.iter_mut().enumerate() {
            tile.preimage.values_le =
                (0..tile.preimage.value_count).flat_map(|i| (((i + ordinal as u32) % 127) as i32 - 63).to_le_bytes()).collect();
            digest.push_values(&binding.shape_profile, tile.preimage.coord, 0, &tile.preimage.values_le).unwrap();
        }
        let state_root = digest.finish(&binding.shape_profile).unwrap();
        job.tail = Some(crate::palw_freeprompt_v3::PalwFpJobTailV1::Prefix(crate::palw_freeprompt_v3::PalwFpPrefixStateV1 {
            state_root: if fault { Hash64::from_u64_word(0xbad19) } else { state_root },
            prefix_tokens: 2,
            class_id: job.class_id,
        }));
        let class = crate::palw_fp_execution_v3::PalwFpClassFactsV3 {
            model_profile_id: binding.job_context.model_profile_id,
            runtime_manifest_hash: binding.job_context.runtime_manifest_hash,
            runtime_class_id: binding.job_context.runtime_class_id,
            shape_profile_id: job.class_id,
            cu_ruleset_id: binding.job_context.cu_ruleset_id,
        };
        binding.job_context = crate::palw_fp_execution_v3::palw_fp_job_context_v3(
            &job,
            &class,
            &crate::palw_fp_execution_v3::palw_fp_run_facts_for_executed_v1(&job, 2),
            &binding.job_context.network_id,
        )
        .unwrap();
        binding.activation_leg_root = crate::palw_attempt_rules_v1::palw_int_activation_leg_root_v1(&binding.job_context);
        let mut hashes = vec![Hash64::default(); binding.step_leaf_count as usize];
        for tile in &mut tiles {
            tile.opening.leaf_hash = step_tile_leaf_hash_ctx_v1(&binding.job_context, &job.class_id, &tile.preimage);
            hashes[tile.opening.leaf_index as usize] = tile.opening.leaf_hash;
        }
        let tree = PalwStepMerkleTreeV1::build_capped_v1(&hashes, 1 << 32).unwrap();
        binding.step_merkle_root = tree.root();
        for tile in &mut tiles {
            tile.opening.siblings = tree.path_v1(tile.opening.leaf_index as usize).unwrap();
        }
        binding.committed_execution_root = binding_commitment_root_v1(&binding);
        crate::palw_step_leg::verify_binding_v1(&binding).unwrap();
        let artifact_root = Hash64::from_u64_word(0xa19);
        let mut semantic = PalwClassSemanticsV1::of_registration(job.class_id, artifact_root, &binding.job_context, 0);
        semantic.prefix_state_relation = Some(PalwPrefixStateRelationV1::Base0Int8KvV1);
        let target = PalwOffenceTargetV1 {
            claim_id: Hash64::from_u64_word(0xc19),
            class_id: job.class_id,
            artifact_root,
            executor_bond: crate::palw_state_v2::PalwBondKeyV2(job.executor_bond),
            execution_root: binding.committed_execution_root,
            lane: Some(PalwClaimSourceKindV1::FreePrompt),
            segment_count: None,
            phase: None,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&binding.job_context),
            trace_root: binding.full_logits_trace_root,
            output_root: Hash64::default(),
            registered_semantics: Some(semantic),
        };
        palw_constrained_job_target_v1(&target, &binding, &job).unwrap();
        (target, binding, job, tiles)
    }
    #[test]
    fn immutable_i8_job_fixture_authenticates_the_full_prefix_and_refuses_foreign_seed() {
        for inherited in [false, true] {
            for fault in [false, true] {
                let (target, binding, job, tiles) = qualified_prefix_case_for_tests_v1(fault, inherited);
                let context = Hash64::from_u64_word(0xc17019);
                let mut c = PalwPrefixFamilyCourtV1::new(context, &target, &binding, &job, 1 << 32, 1 << 20).unwrap();
                let mut previous = c.checkpoint_v1();
                for (ordinal, tile) in tiles.iter().enumerate() {
                    assert_eq!(c.cursor_v1().unwrap(), PalwPrefixFamilyCursorV1::TileOpening { ordinal: ordinal as u64 });
                    let source = c.next_public_source_v1().unwrap().unwrap();
                    assert_eq!(source.coordinate, tile.preimage.coord);
                    assert_eq!(source.leaf_index, tile.opening.leaf_index);
                    assert_eq!(source.length, tile.preimage.values_le.len() as u32);
                    assert_eq!(source.value_count, tile.preimage.value_count);
                    assert!(source.needs_opening);
                    assert_eq!(source.offset, 0);
                    assert_eq!(source.step_root, binding.step_merkle_root);
                    c.movement(context, &PalwPrefixCourtActionV1::BeginTile { opening: tile.opening.clone() }).unwrap();
                    let before = borsh::to_vec(&c).unwrap();
                    assert!(
                        c.movement(
                            Hash64::default(),
                            &PalwPrefixCourtActionV1::TilePart { offset: 0, bytes: tile.preimage.values_le.clone() }
                        )
                        .is_err()
                    );
                    assert_eq!(borsh::to_vec(&c).unwrap(), before);
                    c.movement(context, &PalwPrefixCourtActionV1::TilePart { offset: 0, bytes: tile.preimage.values_le.clone() })
                        .unwrap();
                    assert_eq!(c.cursor_v1().unwrap(), PalwPrefixFamilyCursorV1::TileFinish { ordinal: ordinal as u64 });
                    assert!(c.finish().is_err());
                    assert!(c.next_public_source_v1().unwrap().is_none());
                    c.movement(context, &PalwPrefixCourtActionV1::FinishTile).unwrap();
                    let cp = c.checkpoint_v1();
                    assert!(borsh::to_vec(&cp).unwrap().len() < 2048);
                    c.restore_checkpoint_v1(&previous).unwrap();
                    c.restore_checkpoint_v1(&cp).unwrap();
                    previous = cp;
                }
                assert_eq!(c.cursor_v1().unwrap(), PalwPrefixFamilyCursorV1::Complete);
                assert_eq!(c.finish(), Ok(fault));
            }
        }
    }
}
