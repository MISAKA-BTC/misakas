//! Exact artifact units under the newly signed complete kernel codec. Native
//! v2 roots use complete authenticated rows; native v3 roots use their actual
//! bounded row tiles. One suite is never substituted for another root.
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::{
    hash::Digest,
    ledger::{ClaimBodyV1, KernelLedgerV1},
    spec::{SpecClassKindV1, composite::ComponentV1},
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::program::ParamDecl;
pub const PALW_KERNEL_SEGMENTED_PART_STAGE_V4: u8 = 254;
pub const PALW_KERNEL_PARAMETER_STAGE_V4: u8 = 255;
/// A local material transport unit cap. It does not reinterpret a class's
/// commitment or silently retile a v2 row under a v3 root.
pub const PALW_KERNEL_MATERIAL_UNIT_BYTES_V4: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwKernelParameterOpeningV4 {
    RowV2(misaka_palw_kernel::merkle::TensorOpeningV1),
    TileV3(misaka_palw_kernel::merkle3::LeafOpeningV3),
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelParameterRowV4 {
    pub program: u16,
    pub parameter: u16,
    pub layer: Option<u16>,
    /// Row ordinal for v2; row-leaf ordinal (line*row_tiles+tile) for v3.
    pub row: u64,
    pub opening: PalwKernelParameterOpeningV4,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelSegmentedPartV4 {
    pub position: u32,
    pub part: u32,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Copy)]
pub struct PalwKernelParameterSpecV4<'a> {
    pub program: u16,
    pub parameter: u16,
    pub layer: Option<u16>,
    pub declaration: &'a ParamDecl,
    pub commitment: Digest,
    pub suite: u32,
    pub rows: u64,
    pub row_len: u64,
    pub row_tiles: u64,
}
impl PalwKernelParameterSpecV4<'_> {
    pub fn units(&self) -> Result<u64, String> {
        self.rows.checked_mul(self.row_tiles).ok_or("artifact row/tile count overflow".into())
    }
    /// Exact maximum Borsh response for one native unit, derived before any
    /// tensor decode/opening allocation. Values are i128 in v2's frozen Borsh
    /// grammar, and dtype-width elements in v3's frozen custom grammar.
    pub fn maximum_unit_bytes(&self) -> Result<u64, String> {
        let max_path = misaka_palw_kernel::merkle::depth(self.units()?) as u64;
        let option = if self.layer.is_some() { 3u64 } else { 1 };
        let outer = 2u64 + 2 + option + 8 + 1;
        let shape = (self.declaration.shape.len() as u64).checked_mul(8).ok_or("artifact shape bytes overflow")?;
        let (header, values) = match self.suite {
            2 => (1u64 + 4 + shape + 1 + 8 + 4 + 4 + 64, self.row_len.checked_mul(16)),
            3 => (
                1u64 + 4 + shape + 1 + 8 + 8 + 4 + 4 + 64,
                self.row_len.min(misaka_palw_kernel::merkle3::TILE_V3).checked_mul(self.declaration.dtype.width() as u64),
            ),
            _ => return Err("unsupported actual parameter commitment suite".into()),
        };
        outer
            .checked_add(header)
            .and_then(|n| n.checked_add(values?))
            .and_then(|n| n.checked_add(max_path.checked_mul(64)?))
            .ok_or("artifact unit encoded size overflow".into())
    }
}
fn specs<'a>(
    program: u16,
    decls: &'a [ParamDecl],
    commitments: &'a ParamCommitmentsV1,
    suite: u32,
    excluded: &[(u16, Option<u16>)],
    mut visit: impl FnMut(PalwKernelParameterSpecV4<'a>) -> Result<(), String>,
) -> Result<(), String> {
    if suite != 2 && suite != 3 {
        return Err("parameter commitment suite has no implemented row codec".into());
    }
    for ((parameter, layer), commitment) in &commitments.by_instance {
        if excluded.contains(&(*parameter, *layer)) {
            continue;
        }
        let declaration = decls.get(*parameter as usize).ok_or("artifact parameter declaration absent")?;
        if declaration.per_layer != layer.is_some() {
            return Err("artifact parameter layer inconsistent".into());
        }
        if declaration.shape.len() > misaka_palw_tir::types::MAX_RANK || declaration.shape.iter().any(|d| *d == 0) {
            return Err("artifact parameter shape invalid".into());
        }
        let elements = declaration
            .shape
            .iter()
            .try_fold(1u64, |n, d| n.checked_mul(u64::from(*d)))
            .ok_or("artifact parameter shape product overflow")?;
        let (rows, row_len) = if declaration.shape.len() <= 1 {
            (1, elements)
        } else {
            let last = *declaration.shape.last().ok_or("parameter last axis absent")? as u64;
            (elements / last, last)
        };
        let row_tiles = if suite == 3 { row_len.div_ceil(misaka_palw_kernel::merkle3::TILE_V3) } else { 1 };
        visit(PalwKernelParameterSpecV4 {
            program,
            parameter: *parameter,
            layer: *layer,
            declaration,
            commitment: *commitment,
            suite,
            rows,
            row_len,
            row_tiles,
        })?;
    }
    Ok(())
}
/// Canonical program/parameter/layer order. Mutable memory-slot overlays are
/// excluded from immutable artifacts: their actual entry state is separately
/// retained in the typed claim's authenticated pre-state and subsequent writes.
pub fn visit_kernel_parameters_v4<'a>(
    ledger: &'a KernelLedgerV1,
    claim: &Digest,
    mut visit: impl FnMut(PalwKernelParameterSpecV4<'a>) -> Result<(), String>,
) -> Result<(), String> {
    let row = ledger.claims.get(claim).ok_or("artifact claim absent")?;
    match &row.body {
        ClaimBodyV1::Program { .. } | ClaimBodyV1::Segmented { .. } => {
            let class = ledger.classes.get(&row.class_binding_id).ok_or("artifact class absent")?;
            specs(0, &class.program.params, &class.param_commitments, class.descriptor.commitment_suite_id, &[], visit)
        }
        ClaimBodyV1::Pipeline { .. } => {
            let class = ledger.pipeline_classes.get(&row.class_binding_id).ok_or("pipeline artifact class absent")?;
            if class.programs.len() != class.param_commitments.len() {
                return Err("pipeline artifact program map mismatch".into());
            }
            for (index, (program, commitments)) in class.programs.iter().zip(&class.param_commitments).enumerate() {
                specs(
                    u16::try_from(index).map_err(|_| "artifact program index overflow")?,
                    &program.params,
                    commitments,
                    class.descriptor.commitment_suite_id,
                    &[],
                    &mut visit,
                )?;
            }
            Ok(())
        }
        ClaimBodyV1::Spec(_) => {
            let class = ledger.typed.classes.get(&row.class_binding_id).ok_or("typed artifact class absent")?;
            match &class.kind {
                SpecClassKindV1::Retrieval { .. } => Ok(()),
                SpecClassKindV1::Memory { rule, root, .. } => {
                    // MAX_MEMORY_SLOTS_V1 is sixteen; avoid allocating a slot map.
                    let mut excluded = [(0, None); misaka_palw_kernel::spec::memory::MAX_MEMORY_SLOTS_V1];
                    if root.slots.len() > excluded.len() {
                        return Err("memory artifact slot cap exceeded".into());
                    }
                    for (target, slot) in excluded.iter_mut().zip(&root.slots) {
                        *target = slot.param;
                    }
                    specs(
                        0,
                        &rule.program.params,
                        &rule.param_commitments,
                        rule.descriptor.commitment_suite_id,
                        &excluded[..root.slots.len()],
                        visit,
                    )
                }
                SpecClassKindV1::Composite { components, .. } => {
                    for (index, component) in components.iter().enumerate() {
                        if let ComponentV1::Model(model) = component {
                            specs(
                                u16::try_from(index).map_err(|_| "artifact component index overflow")?,
                                &model.program.params,
                                &model.param_commitments,
                                model.descriptor.commitment_suite_id,
                                &[],
                                &mut visit,
                            )?;
                        }
                    }
                    Ok(())
                }
            }
        }
    }
}
pub fn kernel_parameter_unit_count_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    max_units: u64,
    max_unit_bytes: u64,
) -> Result<u64, String> {
    let mut total = 0u64;
    visit_kernel_parameters_v4(ledger, claim, |spec| {
        if spec.maximum_unit_bytes()? > max_unit_bytes {
            return Err("unsupported artifact row exceeds the actual unit carrier before enrollment".into());
        }
        total = total.checked_add(spec.units()?).ok_or("artifact total row count overflow")?;
        if total > max_units {
            return Err("artifact unit count cap exceeded before enrollment".into());
        }
        Ok(())
    })?;
    Ok(total)
}
/// Re-derive ordinal membership from the registered artifact, not from fields
/// the provider placed in an opening's body.
pub fn kernel_parameter_at_v4<'a>(
    ledger: &'a KernelLedgerV1,
    claim: &Digest,
    ordinal: u32,
) -> Result<(PalwKernelParameterSpecV4<'a>, u64), String> {
    let mut at = ordinal as u64;
    let mut found = None;
    visit_kernel_parameters_v4(ledger, claim, |spec| {
        if found.is_none() {
            let count = spec.units()?;
            if at < count {
                found = Some((spec, at));
            } else {
                at -= count;
            }
        }
        Ok(())
    })?;
    found.ok_or("artifact ordinal is outside the actual immutable parameter inventory".into())
}
pub fn classify_kernel_parameter_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    ordinal: u32,
    unit: &PalwKernelParameterRowV4,
) -> Result<(), String> {
    let (spec, row) = kernel_parameter_at_v4(ledger, claim, ordinal)?;
    if unit.program != spec.program || unit.parameter != spec.parameter || unit.layer != spec.layer || unit.row != row {
        return Err("artifact row metadata substitutes its canonical ordinal".into());
    }
    let shape_matches = |dtype: u8, shape: &[u64]| {
        dtype == spec.declaration.dtype.tag()
            && shape.len() == spec.declaration.shape.len()
            && shape.iter().zip(&spec.declaration.shape).all(|(a, b)| *a == u64::from(*b))
    };
    let authentic = match (&unit.opening, spec.suite) {
        (PalwKernelParameterOpeningV4::RowV2(opening), 2) => {
            opening.axis == misaka_palw_kernel::merkle::AXIS_ROW
                && opening.index == row
                && shape_matches(opening.dtype, &opening.shape)
                && opening.authenticates(&spec.commitment)
        }
        (PalwKernelParameterOpeningV4::TileV3(opening), 3) => {
            opening.axis == misaka_palw_kernel::merkle::AXIS_ROW
                && opening.line == row / spec.row_tiles
                && opening.tile == row % spec.row_tiles
                && shape_matches(opening.dtype, &opening.shape)
                && opening.authenticates(&spec.commitment)
        }
        _ => false,
    };
    if !authentic {
        return Err("artifact row is not authenticated by its actual native commitment suite".into());
    }
    Ok(())
}

/// The new codec flattens the native segmented (position, part) layout into one
/// canonical ordinal range. Callers reserve the native derived-layout workspace
/// before entering this walk; encoded input bytes alone are not that bound.
pub fn kernel_segmented_unit_count_v4(ledger: &KernelLedgerV1, claim: &Digest, max_units: u64) -> Result<u64, String> {
    let row = ledger.claims.get(claim).ok_or("segmented material claim absent")?;
    let ClaimBodyV1::Segmented { evidence, .. } = &row.body else { return Ok(0) };
    if u64::from(evidence.positions) > max_units {
        return Err("segmented position count exceeds material unit cap".into());
    }
    let class = ledger.classes.get(&row.class_binding_id).ok_or("segmented material class absent")?;
    let mut count = 0u64;
    for position in 0..evidence.positions {
        let parts = misaka_palw_kernel::seg_da::position_part_bytes_v1(&class.program, position)?;
        count = count.checked_add(parts.len() as u64).ok_or("segmented unit count overflow")?;
        if count > max_units {
            return Err("segmented part inventory exceeds material unit cap".into());
        }
    }
    Ok(count)
}
pub fn kernel_segmented_at_v4(ledger: &KernelLedgerV1, claim: &Digest, ordinal: u32) -> Result<(u32, u32), String> {
    let row = ledger.claims.get(claim).ok_or("segmented material claim absent")?;
    let ClaimBodyV1::Segmented { evidence, .. } = &row.body else {
        return Err("segmented unit substitutes the actual claim kind".into());
    };
    let class = ledger.classes.get(&row.class_binding_id).ok_or("segmented material class absent")?;
    let mut at = u64::from(ordinal);
    for position in 0..evidence.positions {
        let count = misaka_palw_kernel::seg_da::position_part_bytes_v1(&class.program, position)?.len() as u64;
        if at < count {
            return Ok((position, u32::try_from(at).map_err(|_| "segmented part index overflow")?));
        }
        at -= count;
    }
    Err("segmented ordinal is outside the native part inventory".into())
}
pub fn classify_kernel_segmented_v4(
    ledger: &KernelLedgerV1,
    claim: &Digest,
    ordinal: u32,
    unit: &PalwKernelSegmentedPartV4,
) -> Result<(), String> {
    let expected = kernel_segmented_at_v4(ledger, claim, ordinal)?;
    if expected != (unit.position, unit.part) {
        return Err("segmented unit substitutes its canonical position/part".into());
    }
    let row = ledger.claims.get(claim).ok_or("segmented material claim absent")?;
    let ClaimBodyV1::Segmented { evidence, segment_roots, .. } = &row.body else {
        return Err("segmented material claim kind substituted".into());
    };
    let class = ledger.classes.get(&row.class_binding_id).ok_or("segmented material class absent")?;
    let actual =
        misaka_palw_kernel::seg_da::classify_part_v1(&class.program, segment_roots, evidence.positions, unit.position, &unit.bytes)
            .map_err(str::to_string)?;
    if actual != unit.part {
        return Err("segmented response part substituted".into());
    }
    Ok(())
}
