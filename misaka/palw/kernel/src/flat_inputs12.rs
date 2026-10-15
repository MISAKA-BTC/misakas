//! Bounded external stage inputs and their exact T/P/A specialization.
//! This opt-in component binds frozen program-v2 inputs to a canonical v1
//! view and a job-specific combined PC. It does not grant native eligibility,
//! install input availability/retention, or handle random/post-write stages.
use crate::{
    flat_lengths7::{FlatJobLengthsV7, FlatLengthAxisV7, FlatLengthTemplateV7, FlatShapeSlotV7},
    flat_scope9::{FlatResourceScopeV9, flat_resource_scope_v9},
    hash::{Digest, object_id},
    merkle3::tensor_commitment_v3,
    trace::ParamCommitmentsV1,
};
use borsh::BorshSerialize;
use misaka_palw_tir::{
    Tensor,
    interp_v2::InputProvider,
    program_v2::{InputSource, OutputDecl, TirProgramV2},
};
use std::collections::BTreeMap;

pub mod bounded14;
pub mod bounded28;
pub mod source14;
pub mod source28;
pub mod composition32;
pub mod court32;

pub const INPUT_COUNT_MAX_V12: usize = 16;
pub const INPUT_RAW_BYTES_MAX_V12: u128 = 16 << 20;
pub const INPUT_REFERENCE_BYTES_MAX_V12: u128 = 32 << 20;
pub const SOURCE_PC_INSTANCES_MAX_V12: usize = 16_384;

pub struct FlatInputTemplateV12 {
    pub program: TirProgramV2,
    pub lengths: FlatLengthTemplateV7,
    pub max_positions: u32,
    pub source_parameters: Digest,
}
pub struct FlatInputSpecializationV12 {
    pub program: TirProgramV2,
    pub scope: FlatResourceScopeV9,
    pub admission: FlatInputAdmissionV12,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatInputAdmissionV12 {
    pub version: u16,
    pub template: Digest,
    pub actual: FlatJobLengthsV7,
    pub positions: u32,
    pub stage_program: Digest,
    pub view_program: Digest,
    pub scope: Digest,
    pub raw_bytes: u128,
    pub reference_bytes: u128,
}
impl FlatInputAdmissionV12 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/typed-input-admission/v12", self)
    }
}
fn shape_payload(p: &TirProgramV2) -> Result<(u128, u128), String> {
    if p.inputs.is_empty() || p.inputs.len() > INPUT_COUNT_MAX_V12 {
        return Err("bounded external stage needs 1..16 inputs".into());
    }
    let (mut raw, mut expanded) = (0u128, 0u128);
    for d in &p.inputs {
        let cells = d.shape.iter().try_fold(1u128, |n, x| n.checked_mul(*x as u128)).ok_or("input extent overflow")?;
        raw = raw.checked_add(cells.checked_mul(d.dtype.width() as u128).ok_or("input byte overflow")?).ok_or("input sum overflow")?;
        expanded = expanded.checked_add(cells.checked_mul(16).ok_or("input expansion overflow")?).ok_or("input sum overflow")?;
    }
    if raw > INPUT_RAW_BYTES_MAX_V12 || expanded > INPUT_REFERENCE_BYTES_MAX_V12 {
        return Err("external input raw/reference payload exceeds fixed component caps".into());
    }
    Ok((raw, expanded))
}
impl FlatInputTemplateV12 {
    pub fn root(&self) -> Result<Digest, String> {
        crate::flat_court4::size_with_limit(&self.program, misaka_palw_tir::program::MAX_PROGRAM_BYTES as u64)
            .map_err(|e| e.to_string())?;
        crate::flat_court4::size_with_limit(&self.lengths, crate::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 as u64)
            .map_err(|e| e.to_string())?;
        Ok(object_id(
            b"misaka-palw/kernel/typed-input-template/v12",
            &(12u16, &self.program, &self.lengths, self.max_positions, self.source_parameters),
        ))
    }
    pub fn specialize(&self, actual: FlatJobLengthsV7, positions: u32) -> Result<FlatInputSpecializationV12, String> {
        let template = self.root()?;
        misaka_palw_tir::validate_v2::validate_v2(&self.program).map_err(|e| e.to_string())?;
        if self.max_positions == 0
            || positions == 0
            || positions > self.max_positions
            || self.max_positions > self.program.history_bound
            || !matches!(self.program.output, OutputDecl::Rows { .. } | OutputDecl::Final { .. })
            || !self.program.post_writes().is_empty()
            || self.program.inputs.iter().any(|d| !matches!(d.source, InputSource::External { .. }))
        {
            return Err("external stage extent/output or unsupported random/post-write effects".into());
        }
        let view = self.program.v1_view();
        if view.encode() != self.lengths.max_program_bytes {
            return Err("length template does not bind exact maximum stage view".into());
        }
        // Every advertised present modality must bind a real stage input axis;
        // a token/patch/audio count is never just an unconnected label.
        let first = self.program.params.len();
        let mut present = [false; 3];
        for binding in &self.lengths.bindings {
            if let FlatShapeSlotV7::Parameter { parameter, .. } = binding.slot {
                let input = (parameter as usize).checked_sub(first).ok_or("cannot resize source weights as job input")?;
                if input >= self.program.inputs.len() {
                    return Err("length parameter slot is not an external stage input".into());
                }
                present[match binding.length {
                    FlatLengthAxisV7::Token => 0,
                    FlatLengthAxisV7::Patch => 1,
                    FlatLengthAxisV7::Audio => 2,
                }] = true;
            }
            if matches!(binding.slot, FlatShapeSlotV7::State { .. }) {
                return Err("external input specialization cannot resize persistent state".into());
            }
        }
        for (i, axis) in [FlatLengthAxisV7::Token, FlatLengthAxisV7::Patch, FlatLengthAxisV7::Audio].into_iter().enumerate() {
            if (self.lengths.maxima.get(axis) != 0) != present[i] {
                return Err("declared modality has no exact input axis binding".into());
            }
        }
        shape_payload(&self.program)?;
        // The maximum and each actual graph independently pass frozen v2 range
        // analysis and complete scalar9 scope. A small input hides no bad maximum.
        misaka_palw_tir::interval_v2::analyze_ranges_v2(&self.program).map_err(|e| e.to_string())?;
        flat_resource_scope_v9(&view, self.max_positions, None).map_err(|e| format!("{e:?}"))?;
        let specialized = self.lengths.program(actual)?;
        let mut program = self.program.clone();
        for (i, d) in program.inputs.iter_mut().enumerate() {
            d.shape = specialized.params[first + i].shape.clone();
        }
        for (b, from) in program.blocks.iter_mut().zip(&specialized.blocks) {
            b.carry_in = from.carry_in.clone();
            b.carry_out = from.carry_out.clone();
            for (n, from) in b.nodes.iter_mut().zip(&from.nodes) {
                n.out = from.out.clone();
            }
        }
        if program.v1_view() != specialized {
            return Err("specialization changed data, source declarations, primitive attributes or state".into());
        }
        misaka_palw_tir::validate_v2::validate_v2(&program).map_err(|e| e.to_string())?;
        misaka_palw_tir::interval_v2::analyze_ranges_v2(&program).map_err(|e| e.to_string())?;
        let scope = flat_resource_scope_v9(&specialized, positions, None).map_err(|e| format!("{e:?}"))?;
        let (raw_bytes, reference_bytes) = shape_payload(&program)?;
        let admission = FlatInputAdmissionV12 {
            version: 12,
            template,
            actual,
            positions,
            stage_program: object_id(b"misaka-palw/kernel/typed-input-stage-program/v12", &program),
            view_program: scope.program,
            scope: scope.root(),
            raw_bytes,
            reference_bytes,
        };
        Ok(FlatInputSpecializationV12 { program, scope, admission })
    }
    pub fn check(&self, admission: &FlatInputAdmissionV12) -> Result<FlatInputSpecializationV12, String> {
        let own = self.specialize(admission.actual, admission.positions)?;
        if own.admission != *admission {
            return Err("typed input admission differs from independent derivation".into());
        }
        Ok(own)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatInputCommitmentsV12 {
    pub version: u16,
    pub admission: Digest,
    pub job: Digest,
    pub input_roots: Vec<Digest>,
}
impl FlatInputCommitmentsV12 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/typed-input-commitments/v12", self)
    }
}
pub struct RawExternalInputV12<'a> {
    pub dtype: misaka_palw_tir::DType,
    pub shape: &'a [u32],
    pub bytes: &'a [u8],
}
pub trait RawExternalInputsV12 {
    fn input(&self, input: u16) -> Option<RawExternalInputV12<'_>>;
}
/// Prepaid local lifetime allowance; it is not a native fee or shared quota.
pub struct FlatInputBudgetV12 {
    pub raw_bytes: u128,
    pub reference_bytes: u128,
    pub attempts: u32,
}
pub struct AuthenticatedInputsV12 {
    commitments: FlatInputCommitmentsV12,
    values: BTreeMap<u16, Tensor>,
    combined_pc: ParamCommitmentsV1,
    first: u16,
    positions: u32,
}
impl InputProvider for AuthenticatedInputsV12 {
    fn input(&self, k: u16, pos: u32) -> Option<Tensor> {
        if pos >= self.positions {
            return None;
        }
        self.values.get(&k).cloned()
    }
}
impl AuthenticatedInputsV12 {
    pub fn commitments(&self) -> &FlatInputCommitmentsV12 {
        &self.commitments
    }
    pub fn combined_pc(&self) -> &ParamCommitmentsV1 {
        &self.combined_pc
    }
    /// Virtual input params of the exact frozen view; source weights must come
    /// from the separately authenticated real source PC, never this map.
    pub fn view_input(&self, index: u16, layer: Option<u16>) -> Option<Tensor> {
        if layer.is_some() {
            return None;
        }
        self.values.get(&index.checked_sub(self.first)?).cloned()
    }
}
pub fn acquire_authenticated_inputs_v12(
    template: &FlatInputTemplateV12,
    admission: &FlatInputAdmissionV12,
    job: Digest,
    expected: &FlatInputCommitmentsV12,
    source_pc: &ParamCommitmentsV1,
    provider: &dyn RawExternalInputsV12,
    budget: &mut FlatInputBudgetV12,
) -> Result<AuthenticatedInputsV12, String> {
    let own = template.check(admission)?;
    if expected.version != 12
        || expected.job != job
        || expected.admission != admission.root()
        || expected.input_roots.len() != own.program.inputs.len()
    {
        return Err("typed input job/admission/root count mismatch".into());
    }
    // Authenticate the exact real parameter set before merging any virtual
    // job input roots. A missing/extra/per-layer-substituted source is refused.
    let mut keys = std::collections::BTreeSet::new();
    let count = own
        .program
        .params
        .iter()
        .try_fold(0usize, |n, d| n.checked_add(if d.per_layer { own.program.schedule.layers.len() } else { 1 }))
        .ok_or("source-PC count overflow")?;
    if count > SOURCE_PC_INSTANCES_MAX_V12 || source_pc.by_instance.len() != count {
        return Err("source-PC extent exceeds component cap or declaration".into());
    }
    for (j, d) in own.program.params.iter().enumerate() {
        if d.per_layer {
            for l in 0..own.program.schedule.layers.len() {
                keys.insert((j as u16, Some(l as u16)));
            }
        } else {
            keys.insert((j as u16, None));
        }
    }
    if source_pc.by_instance.keys().copied().collect::<std::collections::BTreeSet<_>>() != keys {
        return Err("typed stage real source-PC namespace mismatch".into());
    }
    if source_pc.root() != template.source_parameters {
        return Err("source-PC differs from immutable typed stage template".into());
    }
    let raw = budget.raw_bytes.checked_sub(admission.raw_bytes).ok_or("typed input raw allowance")?;
    let expanded = budget.reference_bytes.checked_sub(admission.reference_bytes).ok_or("typed input reference allowance")?;
    let attempts = budget.attempts.checked_sub(1).ok_or("typed input attempt allowance")?;
    budget.raw_bytes = raw;
    budget.reference_bytes = expanded;
    budget.attempts = attempts;
    let first = own.program.params.len() as u16;
    let mut pc = source_pc.clone();
    let mut values = BTreeMap::new();
    for (i, d) in own.program.inputs.iter().enumerate() {
        let image = provider.input(i as u16).ok_or("missing public typed input")?;
        let cells = d.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d as usize)).ok_or("input cells overflow")?;
        if image.dtype != d.dtype || image.shape != d.shape || cells.checked_mul(d.dtype.width()) != Some(image.bytes.len()) {
            return Err("typed input image differs from admitted dtype/shape/byte extent".into());
        }
        let value = Tensor::from_le_bytes(d.dtype, &d.shape.iter().map(|d| *d as usize).collect::<Vec<_>>(), image.bytes)
            .map_err(|e| e.to_string())?;
        let (lo, hi) = d.interval();
        if value.data.iter().any(|v| *v < lo || *v > hi) {
            return Err("typed input outside declared interval".into());
        }
        let root = tensor_commitment_v3(&value);
        if root != expected.input_roots[i] {
            return Err("public typed input fails exact job root".into());
        }
        pc.by_instance.insert((first + i as u16, None), root);
        values.insert(i as u16, value);
    }
    Ok(AuthenticatedInputsV12 { commitments: expected.clone(), values, combined_pc: pc, first, positions: admission.positions })
}
