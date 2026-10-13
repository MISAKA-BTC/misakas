//! Canonical full V2 source namespace28. Source admission preserves all stage
//! declarations without claiming their execution or semantic fidelity. Real weights remain separate from
//! external job input declarations and maximum-geometry quotation identifiers.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{Cursor, Read};

pub const STAGE_SOURCE_MAX_BYTES_V28: usize = misaka_palw_tir::program::MAX_PROGRAM_BYTES + 64 + 16_384 * 69 + 10;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatStageSourceWireV28 {
    pub version: u16,
    pub program_bytes: Vec<u8>,
    pub inventory_root: Digest,
    pub source_parameters: ParamCommitmentsV1,
}
pub struct CheckedStageSourceV28 {
    wire: FlatStageSourceWireV28,
    program: TirProgramV2,
    id: Digest,
}
impl CheckedStageSourceV28 {
    pub fn id(&self) -> Digest {
        self.id
    }
    pub fn program(&self) -> &TirProgramV2 {
        &self.program
    }
    pub fn parameters(&self) -> &ParamCommitmentsV1 {
        &self.wire.source_parameters
    }
    pub fn inventory_root(&self) -> Digest {
        self.wire.inventory_root
    }
    /// Source correctness is a relation between the real declarations, true R
    /// inventory and bound PC. Program labels/math/input declarations cannot
    /// create a fresh relation after this exact real-weight relation is refuted.
    pub fn relation_root(&self) -> Digest {
        object_id(
            b"misaka-palw/kernel/stage-real-source-relation/v14",
            &(14u16, &self.program.params, self.parameters().root(), self.inventory_root()),
        )
    }
    pub fn program_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-source-program/v28", &self.wire.program_bytes)
    }
    pub fn input_declarations_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-source-input-declarations/v28", &self.program.inputs)
    }
    pub fn check_template(&self, template: &FlatInputTemplateV12) -> Result<(), String> {
        if template.program != self.program || template.source_parameters != self.parameters().root() {
            return Err("stage template differs from immutable source program or real weights".into());
        }
        if template.program.v1_view().encode() != template.lengths.max_program_bytes || template.max_positions == 0 || template.max_positions > self.program.history_bound { return Err("source28 template changes canonical maximum geometry".into()); }
        Ok(())
    }
}
impl FlatStageSourceWireV28 {
    pub fn check(&self) -> Result<CheckedStageSourceV28, String> {
        if self.version != 28
            || self.program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
            || self.source_parameters.by_instance.len() > SOURCE_PC_INSTANCES_MAX_V12
        {
            return Err("stage source version or wire extent exceeds fixed caps".into());
        }
        let program = TirProgramV2::decode_canonical(&self.program_bytes).map_err(|e| e.to_string())?;
        // The canonical source preserves every V2 stage, including Logits,
        // Random, upstream inputs and post writes. Execution authority is a
        // separate checked class/master contract; source admission grants none.
        if program.inputs.len() > INPUT_COUNT_MAX_V12 || self.inventory_root == [0; 64] {
            return Err("source28 declaration or inventory extent exceeds cap".into());
        }
        misaka_palw_tir::validate_v2::validate_v2(&program).map_err(|e| e.to_string())?;
        let mut raw = 0u128;
        let mut expanded = 0u128;
        for d in &program.inputs {
            let cells = d.shape.iter().try_fold(1u128, |n,x| n.checked_mul(*x as u128)).ok_or("source28 input geometry overflow")?;
            raw = raw.checked_add(cells.checked_mul(d.dtype.width() as u128).ok_or("source28 input bytes overflow")?).ok_or("source28 input sum overflow")?;
            expanded = expanded.checked_add(cells.checked_mul(16).ok_or("source28 expanded input overflow")?).ok_or("source28 input sum overflow")?;
        }
        if raw > INPUT_RAW_BYTES_MAX_V12 || expanded > INPUT_REFERENCE_BYTES_MAX_V12 { return Err("source28 bounded declared inputs exceed caps".into()); }
        let mut keys = std::collections::BTreeSet::new();
        let count = program
            .params
            .iter()
            .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
            .ok_or("stage source namespace overflow")?;
        if count != self.source_parameters.by_instance.len() {
            return Err("source28 commitment count differs from real weight declarations".into());
        }
        for (j, p) in program.params.iter().enumerate() {
            if p.per_layer {
                for l in 0..program.schedule.layers.len() {
                    keys.insert((j as u16, Some(l as u16)));
                }
            } else {
                keys.insert((j as u16, None));
            }
        }
        if keys != self.source_parameters.by_instance.keys().copied().collect() {
            return Err("source28 PC includes a substituted weight or virtual input".into());
        }
        misaka_palw_tir::interval_v2::analyze_ranges_v2(&program).map_err(|e| e.to_string())?;
        let id = object_id(
            b"misaka-palw/kernel/stage-source-binding/v28",
            &(
                28u16,
                object_id(b"misaka-palw/kernel/stage-source-program/v28", &self.program_bytes),
                object_id(b"misaka-palw/kernel/stage-source-input-declarations/v28", &program.inputs),
                self.source_parameters.root(),
                self.inventory_root,
            ),
        );
        Ok(CheckedStageSourceV28 { wire: self.clone(), program, id })
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > STAGE_SOURCE_MAX_BYTES_V28 {
            return Err("stage source frame exceeds cap".into());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        if version != 28 {
            return Err("unknown stage source version".into());
        }
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > misaka_palw_tir::program::MAX_PROGRAM_BYTES || n > bytes.len().saturating_sub(r.position() as usize) {
            return Err("stage source program count exceeds bounded wire".into());
        }
        let mut program_bytes = vec![0; n];
        r.read_exact(&mut program_bytes).map_err(|e| e.to_string())?;
        let inventory_root = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > SOURCE_PC_INSTANCES_MAX_V12 || n > bytes.len().saturating_sub(r.position() as usize) / 67 {
            return Err("stage source PC count exceeds bounded wire".into());
        }
        let mut by_instance = BTreeMap::new();
        let mut previous = None;
        for _ in 0..n {
            let key = <(u16, Option<u16>)>::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
            if previous.is_some_and(|p| p >= key) {
                return Err("stage source PC order is not canonical".into());
            }
            previous = Some(key);
            by_instance.insert(key, Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?);
        }
        let wire = Self { version, program_bytes, inventory_root, source_parameters: ParamCommitmentsV1 { by_instance } };
        if r.position() as usize != bytes.len() || borsh::to_vec(&wire).map_err(|e| e.to_string())? != bytes {
            return Err("stage source frame has surplus or noncanonical bytes".into());
        }
        wire.check()?;
        Ok(wire)
    }
}

