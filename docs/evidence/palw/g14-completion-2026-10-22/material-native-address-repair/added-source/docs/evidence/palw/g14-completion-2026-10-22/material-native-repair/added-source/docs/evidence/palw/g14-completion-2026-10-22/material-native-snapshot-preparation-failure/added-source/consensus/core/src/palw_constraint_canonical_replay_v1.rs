//! Private primitive replay candidate. No public ingress, original admission,
//! signed move, custody duty, fee receipt, retention or complete-graph authority.
//! The caller must separately admit native base cloning and primitive scratch.
use super::{Hash64, PalwConstraintStreamCourtV1 as Court, PalwConstraintStreamFaultV1, PalwConstraintStreamPhaseV1 as Phase};
use crate::palw_step_leg::PalwStepOpeningV1;
pub(crate) struct RenderingMaterial<'a> {
    pub(crate) opening: &'a PalwStepOpeningV1,
    pub(crate) bytes: Option<&'a [u8]>,
}
/// Borrowed material is verified by existing begin/finish primitives. A missing
/// item and an authenticated absent rendering are different: outer None is Need.
pub(crate) trait Material {
    fn eog(&self) -> Option<&[u32]>;
    fn rendering(&self, id: u32) -> Option<RenderingMaterial<'_>>;
}
#[derive(Clone, Copy)]
pub(crate) struct WorkBudget {
    pub(crate) max_material_bytes: u64,
    pub(crate) max_calls: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Work {
    pub(crate) material_bytes: u64,
    pub(crate) calls: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    NeedEog,
    NeedRendering(u32),
    InitialState,
    Budget,
    Native(String),
}
#[derive(Debug)]
pub(crate) struct Candidate {
    context: Hash64,
    execution_root: Hash64,
    table_root: Hash64,
    job: Hash64,
    finding: Option<PalwConstraintStreamFaultV1>,
    work: Work,
}
impl Candidate {
    pub(crate) fn identity(&self) -> (Hash64, Hash64, Hash64, Hash64) {
        (self.context, self.execution_root, self.table_root, self.job)
    }
    pub(crate) fn finding(&self) -> Option<&PalwConstraintStreamFaultV1> {
        self.finding.as_ref()
    }
    pub(crate) fn work(&self) -> Work {
        self.work
    }
}
fn charge(work: &mut Work, budget: WorkBudget, bytes: u64, calls: u64) -> Result<(), Refusal> {
    let next = Work {
        material_bytes: work.material_bytes.checked_add(bytes).ok_or(Refusal::Budget)?,
        calls: work.calls.checked_add(calls).ok_or(Refusal::Budget)?,
    };
    if next.material_bytes > budget.max_material_bytes || next.calls > budget.max_calls {
        return Err(Refusal::Budget);
    }
    *work = next;
    Ok(())
}
fn native<T>(v: Result<T, String>) -> Result<T, Refusal> {
    v.map_err(Refusal::Native)
}
/// `base` must be the native original context supplied by the future private
/// adapter. This function does NOT authenticate a caller's base as chain state.
/// It always leaves base/material untouched, including on Need/error/budget.
/// Material is fully authenticated before a local finding is exposed.
pub(crate) fn build(base: &Court, material: &impl Material, budget: WorkBudget) -> Result<Candidate, Refusal> {
    if base.phase() != Phase::Eog || base.eog_offset_v1() != Some(0) || base.rendering.is_some() {
        return Err(Refusal::InitialState);
    }
    let ids = material.eog().ok_or(Refusal::NeedEog)?;
    let mut work = Work::default();
    charge(&mut work, budget, u64::try_from(ids.len()).map_err(|_| Refusal::Budget)?.checked_mul(4).ok_or(Refusal::Budget)?, 0)?;
    let mut c = base.clone();
    let context = c.seed_id();
    for part in ids.chunks(crate::palw_eog_stream_v1::PALW_EOG_STREAM_PART_IDS_V1) {
        charge(&mut work, budget, 0, 1)?;
        let offset = c.eog_offset_v1().ok_or(Refusal::InitialState)?;
        native(c.push_eog(context, offset, part))?;
    }
    charge(&mut work, budget, 0, 1)?;
    native(c.finish_eog())?;
    while c.phase() != Phase::Complete {
        let id = native(c.next_rendering_id())?;
        if c.is_eog(id) {
            charge(&mut work, budget, 0, 1)?;
            native(c.finish_eog_token(context))?;
            continue;
        }
        let input = material.rendering(id).ok_or(Refusal::NeedRendering(id))?;
        let length = u32::try_from(input.bytes.map_or(0, <[u8]>::len)).map_err(|_| Refusal::Budget)?;
        let proof_bytes = borsh::object_length(input.opening).map_err(|e| Refusal::Native(e.to_string()))? as u64;
        charge(&mut work, budget, proof_bytes.checked_add(u64::from(length)).ok_or(Refusal::Budget)?, 1)?;
        native(c.begin_rendering(context, id, input.bytes.is_some(), length, input.opening))?;
        if let Some(bytes) = input.bytes {
            let mut offset = 0u32;
            for part in bytes.chunks(crate::palw_rendering_stream_v1::PALW_RENDERING_STREAM_PART_BYTES_V1) {
                charge(&mut work, budget, 0, 1)?;
                native(c.push_rendering(context, offset, part))?;
                offset = offset.checked_add(part.len() as u32).ok_or(Refusal::Budget)?;
            }
        }
        charge(&mut work, budget, 0, 1)?;
        native(c.finish_rendering())?;
    }
    let finding = native(c.fault())?;
    Ok(Candidate { context, execution_root: c.execution_root_v1(), table_root: c.table_root, job: c.job_id, finding, work })
}
