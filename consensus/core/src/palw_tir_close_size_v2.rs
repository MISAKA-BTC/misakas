//! Explicit LOCAL TIR-only close sizing for the adopted wide Q24 profile.
//!
//! Every step lane is conservatively quoted at eight bytes, including narrow
//! intermediate/state leaves. This does not change their actual mixed-width
//! encoding. Results are data, not court acceptance, native resource leases,
//! registration, prices, credit or activation. GEN/composite sizing is unsupported.
//! Context checks establish geometry and the exact wide scheme, not authenticated
//! class/job assignment, artifact/tokenizer/runtime identity or accepted history.
//! As for V1, the caller supplies the longest job it needs to cover. A shorter
//! valid job does not establish a bound for every job in the original layout.
use crate::{
    palw_step::PalwStepCoordinateV1,
    palw_tir_class_v1::PalwTirLayoutV1,
    palw_tir_close_size_v1::{PalwTirCloseBoundV1, PalwTirCloseSizingV1, PalwTirParamFormV1},
    palw_tir_court_v1::PalwTirInventoryIndexV1,
    palw_tir_step_v1::{PalwTirJobShapeV1, PalwTirStateInstanceV1, PalwTirStepError, PalwTirStepSpaceV1, PalwTirStepSpaceV2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{TirProgramV1, validate::ProgramInfo};

/// Local limits only. Neither this configuration nor the sizing result reserves
/// the program, read maps, range cache, serialization buffers or native work.
#[derive(Clone, Copy, Debug)]
pub struct PalwTirCloseSizingV2 {
    pub form: PalwTirParamFormV1,
    pub court: bool,
    pub cap: u64,
    pub stop_above: Option<(u64, u64)>,
}

/// Nonserialized upper bounds with an explicit V2 type. There is intentionally
/// no conversion into a legacy bound/admission object and no authority token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirCloseBoundV2 {
    pub block: u8,
    pub node: u16,
    pub checkpoint: Option<u16>,
    pub dissected: bool,
    pub close_bytes: u64,
    pub root_claim_bytes: u64,
}

/// The V2 result cannot be passed directly to the legacy admission comparison.
///
/// ```compile_fail
/// use kaspa_consensus_core::{palw_tir_close_size_v2::PalwTirCloseBoundsV2,
///     palw_tir_admission_v1::palw_tir_carried_close_bounds_admit_v1};
/// fn legacy_accept(result: &PalwTirCloseBoundsV2) {
///     palw_tir_carried_close_bounds_admit_v1(result.bounds(), u64::MAX).unwrap();
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirCloseBoundsV2 {
    bounds: Vec<PalwTirCloseBoundV2>,
    sizing_work: u64,
}
impl PalwTirCloseBoundsV2 {
    pub fn bounds(&self) -> &[PalwTirCloseBoundV2] {
        &self.bounds
    }
    pub fn sizing_work(&self) -> u64 {
        self.sizing_work
    }
    fn from_local(bounds: Vec<PalwTirCloseBoundV1>, sizing_work: u64) -> Self {
        Self {
            bounds: bounds
                .into_iter()
                .map(|b| PalwTirCloseBoundV2 {
                    block: b.block,
                    node: b.node,
                    checkpoint: b.checkpoint,
                    dissected: b.dissected,
                    close_bytes: b.close_bytes,
                    root_claim_bytes: b.root_claim_bytes,
                })
                .collect(),
            sizing_work,
        }
    }
}

fn prepare(
    space: &PalwTirStepSpaceV2,
    job: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV2,
) -> Result<(PalwTirInventoryIndexV1, PalwTirCloseSizingV1), String> {
    // Strict V2 geometry and context checks precede any inventory allocation.
    space.job_shape(job).map_err(|e| e.to_string())?;
    if matches!(sizing.form, PalwTirParamFormV1::Composite { .. }) {
        return Err("wide Q24 composite close sizing is not implemented".into());
    }
    if sizing.cap == 0 {
        return Err(crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.into());
    }
    let geometry = CloseGeometry::from_v2(space).with_allocation_cap(sizing.cap);
    geometry.guard_allocation(space.program().params.len() as u64)?;
    geometry.guard_allocation(space.occurrences().len() as u64)?;
    geometry.guard_allocation(space.layout().commit_tiles.len() as u64)?;
    geometry.guard_allocation(space.layout().state_tiles.len() as u64)?;
    // Bound the metadata's per-param layer lists before their constructor
    // allocates them. This is metadata cardinality, not tensor payload size.
    let instances = space.program().params.iter().try_fold(0u64, |n, p| {
        let layers = if p.per_layer { space.program().schedule.layers.len() as u64 } else { 1 };
        n.checked_add(layers).ok_or("wide Q24 parameter metadata cardinality overflow")
    })?;
    geometry.guard_allocation(instances)?;
    // Derive the inventory from this exact immutable program. A caller cannot
    // substitute another program's inventory or a peer-supplied width flag.
    let inventory = PalwTirInventoryIndexV1::new(space.program()).ok_or("wide Q24 inventory is not representable")?;
    let legacy_shape = PalwTirCloseSizingV1 { form: sizing.form, court: sizing.court, cap: sizing.cap, stop_above: sizing.stop_above };
    Ok((inventory, legacy_shape))
}

/// Element twin, with all candidates priced at the conservative eight-byte
/// step-lane bound. The work counter measures sizing, not model execution.
pub fn palw_tir_worst_closes_work_v2(
    space: &PalwTirStepSpaceV2,
    job: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV2,
) -> Result<PalwTirCloseBoundsV2, String> {
    let (inventory, shape) = prepare(space, job, sizing)?;
    let (bounds, sizing_work) = crate::palw_tir_close_size_v1::palw_tir_worst_closes_work_wide_v2(space, &inventory, job, &shape)?;
    Ok(PalwTirCloseBoundsV2::from_local(bounds, sizing_work))
}

/// Range twin of the same V2 data sizing. It does not enable a range/court fence
/// or any native consumer. Its work counter is independent of the element twin.
pub fn palw_tir_worst_closes_range_work_v2(
    space: &PalwTirStepSpaceV2,
    job: &PalwJobContextV2,
    sizing: &PalwTirCloseSizingV2,
) -> Result<PalwTirCloseBoundsV2, String> {
    let (inventory, shape) = prepare(space, job, sizing)?;
    let (bounds, sizing_work) =
        crate::palw_tir_close_range_v1::palw_tir_worst_closes_range_work_wide_v2(space, &inventory, job, &shape)?;
    Ok(PalwTirCloseBoundsV2::from_local(bounds, sizing_work))
}

/// Shared read-only geometry, private to sizing. It exposes neither the wide
/// space's internal V1 representation nor any encoder/builder/admission method.
#[derive(Clone, Copy, Debug)]
pub(crate) enum CloseGeometry<'a> {
    Legacy(&'a PalwTirStepSpaceV1),
    Wide { space: &'a PalwTirStepSpaceV2, allocation_cap: u64 },
}
impl<'a> CloseGeometry<'a> {
    pub(crate) fn from_v1(space: &'a PalwTirStepSpaceV1) -> Self {
        Self::Legacy(space)
    }
    pub(crate) fn from_v2(space: &'a PalwTirStepSpaceV2) -> Self {
        Self::Wide { space, allocation_cap: crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1 }
    }
    /// Local pre-allocation limit, never a native RAM lease. It does not add
    /// work to successful walks and therefore preserves the V1 work oracle.
    pub(crate) fn with_allocation_cap(self, cap: u64) -> Self {
        match self {
            Self::Legacy(_) => self,
            Self::Wide { space, .. } => {
                Self::Wide { space, allocation_cap: cap.min(crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1) }
            }
        }
    }
    pub(crate) fn guard_allocation(self, count: u64) -> Result<(), String> {
        if let Self::Wide { allocation_cap, .. } = self {
            if count > allocation_cap || count > u64::from(u32::MAX) || usize::try_from(count).is_err() {
                return Err(crate::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_OVER_CAP_V1.into());
            }
        }
        Ok(())
    }
    pub(crate) fn program(self) -> &'a TirProgramV1 {
        match self {
            Self::Legacy(s) => &s.program,
            Self::Wide { space: s, .. } => s.program(),
        }
    }
    pub(crate) fn info(self) -> &'a ProgramInfo {
        match self {
            Self::Legacy(s) => &s.info,
            Self::Wide { space: s, .. } => s.info(),
        }
    }
    pub(crate) fn layout(self) -> &'a PalwTirLayoutV1 {
        match self {
            Self::Legacy(s) => &s.layout,
            Self::Wide { space: s, .. } => s.layout(),
        }
    }
    /// Legacy spaces expose mutable program metadata. Recheck just the lane
    /// family before quoting/reading it; this is not a full NF/layout validator.
    /// Unknown schemes retain their old meaning for narrow committed/final outputs.
    pub(crate) fn require_legacy_width(self) -> Result<(), String> {
        let Self::Legacy(space) = self else { return Ok(()) };
        let program = &space.program;
        if program.wide_logits_q24() {
            return Err("wide Q24 requires explicit v2 close sizing; legacy step lanes are four bytes".into());
        }
        let committed_wide = program.blocks.iter().flat_map(|b| &b.nodes).any(|n| n.commit && !n.out.dtype.committable());
        let final_wide = program
            .blocks
            .get(program.schedule.post as usize)
            .and_then(|b| b.nodes.get(program.logits as usize))
            .is_some_and(|n| !n.out.dtype.committable());
        if committed_wide || final_wide {
            return Err("non-four-byte committed or final lanes cannot use legacy close sizing".into());
        }
        Ok(())
    }
    pub(crate) fn job_shape(self, ctx: &PalwJobContextV2) -> Result<PalwTirJobShapeV1, PalwTirStepError> {
        self.require_legacy_width().map_err(PalwTirStepError::Layout)?;
        match self {
            Self::Legacy(s) => s.job_shape(ctx),
            Self::Wide { space: s, .. } => s.job_shape(ctx),
        }
    }
    pub(crate) fn running_total(self, job: &PalwTirJobShapeV1, position: u32) -> u128 {
        match self {
            Self::Legacy(s) => s.running_total(job, position),
            Self::Wide { space: s, .. } => s.running_total(job, position),
        }
    }
    pub(crate) fn occurrences(self) -> &'a [(u8, Option<u16>)] {
        match self {
            Self::Legacy(s) => s.occurrences(),
            Self::Wide { space: s, .. } => s.occurrences(),
        }
    }
    pub(crate) fn program_slots(self) -> u32 {
        match self {
            Self::Legacy(s) => s.program_slots(),
            Self::Wide { space: s, .. } => s.program_slots(),
        }
    }
    pub(crate) fn commit_tile_len(self, block: u8, node: u16) -> Option<u32> {
        match self {
            Self::Legacy(s) => s.commit_tile_len(block, node),
            Self::Wide { space: s, .. } => s.commit_tile_len(block, node),
        }
    }
    pub(crate) fn node_slot(self, occurrence: usize, node: u16) -> Option<u32> {
        match self {
            Self::Legacy(s) => s.node_slot(occurrence, node),
            Self::Wide { space: s, .. } => s.node_slot(occurrence, node),
        }
    }
    pub(crate) fn fixed_instances(self) -> &'a [PalwTirStateInstanceV1] {
        match self {
            Self::Legacy(s) => s.fixed_instances(),
            Self::Wide { space: s, .. } => s.fixed_instances(),
        }
    }
    pub(crate) fn hist_instances(self) -> &'a [PalwTirStateInstanceV1] {
        match self {
            Self::Legacy(s) => s.hist_instances(),
            Self::Wide { space: s, .. } => s.hist_instances(),
        }
    }
    pub(crate) fn fixed_instance_index(self, state: u16, layer: Option<u16>) -> Option<usize> {
        match self {
            Self::Legacy(s) => s.fixed_instance_index(state, layer),
            Self::Wide { space: s, .. } => s.fixed_instance_index(state, layer),
        }
    }
    pub(crate) fn hist_instance_index(self, state: u16, layer: Option<u16>) -> Option<usize> {
        match self {
            Self::Legacy(s) => s.hist_instance_index(state, layer),
            Self::Wide { space: s, .. } => s.hist_instance_index(state, layer),
        }
    }
    pub(crate) fn leaf_index(self, ctx: &PalwJobContextV2, coord: &PalwStepCoordinateV1) -> Option<u64> {
        match self {
            Self::Legacy(s) => s.leaf_index(ctx, coord),
            Self::Wide { space: s, .. } => s.leaf_index(ctx, coord),
        }
    }
}
