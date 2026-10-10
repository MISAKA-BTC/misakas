//! RFC-0008 WORK: derive credited continuation work from registered IR and absolute token positions.
//!
//! This is an executable design reference, not an active consensus caller. Root lottery PWU is a
//! different quantity. A continuation may replay its prompt to reconstruct state; that replay is
//! not new useful work. Every cell below uses the SAME whole-job facts, including the original
//! prefill boundary and history window. The accepted class selects the existing work-cost version.

use crate::palw_canonical_work_v1::{PalwCanonicalExecutionFactsV1, PalwCanonicalWorkVectorV1};
use crate::palw_exec_v2::PALW_EXEC_V2_MAX_SLICES_PER_ROOT;
use crate::palw_tir_work_v1::{PalwTirWorkError, palw_tir_work_shape_with_v1};
use crate::palw_work_slice_v2::PALW_EXEC_V2_MAX_ROOT_WORK;
use misaka_palw_tir::TirProgramV1;
use std::ops::Range;

/// Existing TIR work policies. Read this from the accepted class, never from a producer's plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwExecWorkCostV1 {
    AllNodesV1,
    MinSelectArmsV2,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwExecWorkPlanErrorV1 {
    #[error("a continuation plan needs a nonempty prompt and REAL prefix")]
    EmptyPrefix,
    #[error("a continuation plan needs 1..={PALW_EXEC_V2_MAX_SLICES_PER_ROOT} strictly increasing suffix ends")]
    SuffixEnds,
    #[error("the whole execution exceeds its accepted class's position bound")]
    Positions,
    #[error("the derived work overflows or exceeds the root work bound")]
    WorkBound,
    #[error("the work cells do not conserve the whole execution vector")]
    Conservation,
    #[error("declared work differs from the registered IR execution")]
    DeclaredWork,
    #[error("the slice job's prompt or decode length differs from its execution cell")]
    SliceShape,
    #[error(transparent)]
    Ir(#[from] PalwTirWorkError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecWorkCellV1 {
    positions: Range<u32>,
    prompt_tokens: u32,
    generated_tokens: u32,
    vector: PalwCanonicalWorkVectorV1,
}

impl PalwExecWorkCellV1 {
    pub fn positions(&self) -> Range<u32> {
        self.positions.clone()
    }

    pub fn prompt_tokens(&self) -> u32 {
        self.prompt_tokens
    }

    pub fn generated_tokens(&self) -> u32 {
        self.generated_tokens
    }

    pub fn vector(&self) -> PalwCanonicalWorkVectorV1 {
        self.vector
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecWorkPlanV1 {
    cost: PalwExecWorkCostV1,
    prefix: PalwCanonicalWorkVectorV1,
    slices: Vec<PalwExecWorkCellV1>,
    whole: PalwCanonicalWorkVectorV1,
    /// Cumulative arithmetic MAC-equivalents; traffic remains in the vector's byte dimensions.
    boundaries: Vec<u64>,
}

impl PalwExecWorkPlanV1 {
    pub fn cost(&self) -> PalwExecWorkCostV1 {
        self.cost
    }

    pub fn prefix(&self) -> PalwCanonicalWorkVectorV1 {
        self.prefix
    }

    pub fn slices(&self) -> &[PalwExecWorkCellV1] {
        &self.slices
    }

    pub fn whole(&self) -> PalwCanonicalWorkVectorV1 {
        self.whole
    }

    pub fn boundaries(&self) -> &[u64] {
        &self.boundaries
    }

    pub fn total_work(&self) -> u64 {
        *self.boundaries.last().expect("the constructor requires suffix cells")
    }

    pub fn check_declared_work(&self, total: u64, boundaries: &[u64]) -> Result<(), PalwExecWorkPlanErrorV1> {
        if total != self.total_work() || boundaries != self.boundaries {
            return Err(PalwExecWorkPlanErrorV1::DeclaredWork);
        }
        Ok(())
    }

    pub fn check_slice_job(&self, index: usize, prompt_tokens: usize, generated_tokens: usize) -> Result<(), PalwExecWorkPlanErrorV1> {
        let cell = self.slices.get(index).ok_or(PalwExecWorkPlanErrorV1::SliceShape)?;
        if prompt_tokens != cell.prompt_tokens as usize || generated_tokens != cell.generated_tokens as usize {
            return Err(PalwExecWorkPlanErrorV1::SliceShape);
        }
        Ok(())
    }
}

fn components(v: PalwCanonicalWorkVectorV1) -> [u128; 10] {
    [
        v.dense_matmul,
        v.routed_expert_matmul,
        v.attention_prefill,
        v.attention_decode,
        v.recurrence,
        v.normalization,
        v.other_verified_ops,
        v.weight_traffic_bytes,
        v.kv_read_bytes,
        v.kv_write_bytes,
    ]
}

fn checked_accumulate(sum: &mut [u128; 10], vector: PalwCanonicalWorkVectorV1) -> Result<u64, PalwExecWorkPlanErrorV1> {
    let values = components(vector);
    // Existing TIR shape arithmetic saturates. A saturated dimension cannot authorize a plan.
    if values.contains(&u128::MAX) {
        return Err(PalwExecWorkPlanErrorV1::WorkBound);
    }
    for (total, value) in sum.iter_mut().zip(values) {
        *total = total.checked_add(value).ok_or(PalwExecWorkPlanErrorV1::WorkBound)?;
    }
    let arithmetic = sum[..7].iter().try_fold(0u128, |s, v| s.checked_add(*v)).ok_or(PalwExecWorkPlanErrorV1::WorkBound)?;
    if arithmetic > PALW_EXEC_V2_MAX_ROOT_WORK as u128 {
        return Err(PalwExecWorkPlanErrorV1::WorkBound);
    }
    Ok(arithmetic as u64)
}

/// Derive one uncached semantic execution, with cumulative generated-token ends for its suffixes.
/// `prefix_generated` is the REAL context's exact decode; `class_max_positions` and `cost` are the
/// accepted class's values. The final suffix end fixes whole-job G before costs are computed.
///
/// The prefix runs [0, P + G_prefix - 1). A suffix (G_prev, G_end] runs
/// [P + G_prev - 1, P + G_end - 1). In particular, the last token emitted by the prefix is first
/// consumed by the continuation. Reconstructed history is not paid twice. No per-position loop.
pub fn palw_exec_work_plan_v1(
    program: &TirProgramV1,
    cost: PalwExecWorkCostV1,
    class_max_positions: u32,
    prompt_tokens: u32,
    prefix_generated: u32,
    suffix_ends: &[u32],
) -> Result<PalwExecWorkPlanV1, PalwExecWorkPlanErrorV1> {
    use PalwExecWorkPlanErrorV1::{Conservation, EmptyPrefix, Positions, SuffixEnds, WorkBound};
    if prompt_tokens == 0 || prefix_generated == 0 {
        return Err(EmptyPrefix);
    }
    if suffix_ends.is_empty() || suffix_ends.len() > PALW_EXEC_V2_MAX_SLICES_PER_ROOT as usize {
        return Err(SuffixEnds);
    }
    let mut previous = prefix_generated;
    for end in suffix_ends {
        if *end <= previous {
            return Err(SuffixEnds);
        }
        previous = *end;
    }
    let end_position = |g: u32| {
        prompt_tokens.checked_add(g - 1).filter(|end| *end <= class_max_positions && *end <= program.history_bound).ok_or(Positions)
    };
    end_position(previous)?;
    let shape = palw_tir_work_shape_with_v1(program, cost == PalwExecWorkCostV1::MinSelectArmsV2)?;
    let occurrences = 0..program.occurrences().len();
    let facts = PalwCanonicalExecutionFactsV1::uncached(prompt_tokens, previous);
    let whole = shape.work_v1(&facts)?;
    let prefix_end = end_position(prefix_generated)?;
    let prefix = shape.work_cell_v1(occurrences.clone(), 0..prefix_end as u128, &facts)?;
    let mut sum = [0u128; 10];
    let mut boundaries = vec![checked_accumulate(&mut sum, prefix)?];
    if boundaries[0] == 0 {
        return Err(WorkBound);
    }
    let mut slices = Vec::with_capacity(suffix_ends.len());
    previous = prefix_generated;
    for generated_end in suffix_ends {
        let positions = end_position(previous)?..end_position(*generated_end)?;
        let vector = shape.work_cell_v1(occurrences.clone(), positions.start as u128..positions.end as u128, &facts)?;
        let cumulative = checked_accumulate(&mut sum, vector)?;
        if cumulative <= *boundaries.last().unwrap() {
            return Err(WorkBound);
        }
        boundaries.push(cumulative);
        slices.push(PalwExecWorkCellV1 {
            positions,
            prompt_tokens: prompt_tokens.checked_add(previous).ok_or(Positions)?,
            generated_tokens: *generated_end - previous,
            vector,
        });
        previous = *generated_end;
    }
    if sum != components(whole) {
        return Err(Conservation);
    }
    Ok(PalwExecWorkPlanV1 { cost, prefix, slices, whole, boundaries })
}
