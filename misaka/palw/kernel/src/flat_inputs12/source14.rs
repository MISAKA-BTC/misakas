//! Explicit V2 source namespace for stage14. Real weights remain separate from
//! external job input declarations and maximum-geometry quotation identifiers.
use super::*;
use borsh::{BorshDeserialize, BorshSerialize};
use std::io::{Cursor, Read};

pub const STAGE_SOURCE_MAX_BYTES_V14: usize = misaka_palw_tir::program::MAX_PROGRAM_BYTES + 64 + 16_384 * 69 + 10;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatStageSourceWireV14 {
    pub version: u16,
    pub program_bytes: Vec<u8>,
    pub inventory_root: Digest,
    pub source_parameters: ParamCommitmentsV1,
}
pub struct CheckedStageSourceV14 {
    wire: FlatStageSourceWireV14,
    program: TirProgramV2,
    id: Digest,
}
impl CheckedStageSourceV14 {
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
    pub fn program_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-source-program/v14", &self.wire.program_bytes)
    }
    pub fn input_declarations_root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-source-input-declarations/v14", &self.program.inputs)
    }
    pub fn check_template(&self, template: &FlatInputTemplateV12) -> Result<(), String> {
        if template.program != self.program || template.source_parameters != self.parameters().root() {
            return Err("stage template differs from immutable source program or real weights".into());
        }
        template.specialize(template.lengths.maxima, template.max_positions)?;
        Ok(())
    }
}
impl FlatStageSourceWireV14 {
    pub fn check(&self) -> Result<CheckedStageSourceV14, String> {
        if self.version != 14
            || self.program_bytes.len() > misaka_palw_tir::program::MAX_PROGRAM_BYTES
            || self.source_parameters.by_instance.len() > SOURCE_PC_INSTANCES_MAX_V12
        {
            return Err("stage source version or wire extent exceeds fixed caps".into());
        }
        let program = TirProgramV2::decode_canonical(&self.program_bytes).map_err(|e| e.to_string())?;
        if program.inputs.is_empty()
            || program.inputs.len() > INPUT_COUNT_MAX_V12
            || !matches!(program.output, OutputDecl::Rows { .. } | OutputDecl::Final { .. })
            || !program.post_writes().is_empty()
            || program.inputs.iter().any(|d| !matches!(d.source, InputSource::External { .. }))
            || program.blocks.iter().flat_map(|b| &b.nodes).flat_map(|n| &n.inputs).any(|r| *r == misaka_palw_tir::Ref::Input(0))
        {
            return Err("source14 requires exact implemented external stage semantics".into());
        }
        shape_payload(&program)?;
        let mut keys = std::collections::BTreeSet::new();
        let count = program
            .params
            .iter()
            .try_fold(0usize, |n, p| n.checked_add(if p.per_layer { program.schedule.layers.len() } else { 1 }))
            .ok_or("stage source namespace overflow")?;
        if count != self.source_parameters.by_instance.len() {
            return Err("source14 commitment count differs from real weight declarations".into());
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
            return Err("source14 PC includes a substituted weight or virtual input".into());
        }
        misaka_palw_tir::interval_v2::analyze_ranges_v2(&program).map_err(|e| e.to_string())?;
        let id = object_id(
            b"misaka-palw/kernel/stage-source-binding/v14",
            &(
                14u16,
                object_id(b"misaka-palw/kernel/stage-source-program/v14", &self.program_bytes),
                object_id(b"misaka-palw/kernel/stage-source-input-declarations/v14", &program.inputs),
                self.source_parameters.root(),
                self.inventory_root,
            ),
        );
        Ok(CheckedStageSourceV14 { wire: self.clone(), program, id })
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > STAGE_SOURCE_MAX_BYTES_V14 {
            return Err("stage source frame exceeds cap".into());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        if version != 14 {
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

/// Native stores this compact declarative class beside its independently
/// authenticated source row. It contains no actual input roots or tensor data.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatStageClassWireV14 {
    pub version: u16,
    pub source_binding: Digest,
    pub length_template_bytes: Vec<u8>,
    pub max_positions: u32,
    pub policy_root: Digest,
}
pub struct CheckedStageClassV14 {
    wire: FlatStageClassWireV14,
    template: FlatInputTemplateV12,
    source: CheckedStageSourceV14,
    maximum: super::bounded14::CheckedStageJobV14,
    root: Digest,
}
impl CheckedStageClassV14 {
    pub fn root(&self) -> Digest {
        self.root
    }
    pub fn source(&self) -> &CheckedStageSourceV14 {
        &self.source
    }
    pub fn template(&self) -> &FlatInputTemplateV12 {
        &self.template
    }
    pub fn policy_root(&self) -> Digest {
        self.wire.policy_root
    }
    pub fn maximum_resources(&self) -> &crate::flat_resources10::FlatResourceVectorV10 {
        self.maximum.maximum_resources()
    }
    pub fn maximum_scope(&self) -> &crate::flat_scope9::FlatResourceScopeV9 {
        self.maximum.scope()
    }
    pub fn check_job(&self, job: &super::bounded14::FlatStageJobV14) -> Result<super::bounded14::CheckedStageJobV14, String> {
        let checked = super::bounded14::derive_stage_job_v14(&self.template, job, self.source.parameters())?;
        checked
            .resources()
            .quantities
            .within(&self.maximum_resources().quantities)
            .map_err(|axis| format!("actual stage exceeds independently checked class maximum at {axis:?}"))?;
        Ok(checked)
    }
}
impl FlatStageClassWireV14 {
    pub fn check(&self, source: &FlatStageSourceWireV14) -> Result<CheckedStageClassV14, String> {
        if self.version != 14 || self.policy_root == [0; 64] {
            return Err("stage class requires its explicit versioned policy".into());
        }
        let source = source.check()?;
        if source.id() != self.source_binding {
            return Err("stage class source binding differs".into());
        }
        let lengths = FlatLengthTemplateV7::from_bytes(&self.length_template_bytes)?;
        let template = FlatInputTemplateV12 {
            program: source.program().clone(),
            lengths,
            max_positions: self.max_positions,
            source_parameters: source.parameters().root(),
        };
        source.check_template(&template)?;
        let job = super::bounded14::FlatStageJobV14 {
            version: 14,
            template: template.root()?,
            requester: [0; 64],
            producer: [0; 64],
            nonce: [0; 64],
            actual: template.lengths.maxima,
            positions: template.max_positions,
            input_roots: template
                .program
                .inputs
                .iter()
                .enumerate()
                .map(|(i, d)| {
                    object_id(
                        b"misaka-palw/kernel/maximum-input-declaration/v14",
                        &(template.root().expect("checked root"), i as u16, d),
                    )
                })
                .collect(),
        };
        let maximum = super::bounded14::derive_stage_job_v14(&template, &job, source.parameters())?;
        let a = maximum.admission();
        let root = object_id(
            b"misaka-palw/kernel/stage-class/v14",
            &(self, template.root()?, a.maxima, a.max_stage_program, a.max_scope, a.max_replay, a.max_resources),
        );
        Ok(CheckedStageClassV14 { wire: self.clone(), template, source, maximum, root })
    }
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > crate::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 + 138 {
            return Err("stage class frame exceeds cap".into());
        }
        let mut r = Cursor::new(bytes);
        let version = u16::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        if version != 14 {
            return Err("unknown stage class version".into());
        }
        let source_binding = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let n = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())? as usize;
        if n > crate::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 || n > bytes.len().saturating_sub(r.position() as usize) {
            return Err("stage class length template count exceeds bounded wire".into());
        }
        let mut length_template_bytes = vec![0; n];
        r.read_exact(&mut length_template_bytes).map_err(|e| e.to_string())?;
        let max_positions = u32::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let policy_root = Digest::deserialize_reader(&mut r).map_err(|e| e.to_string())?;
        let out = Self { version, source_binding, length_template_bytes, max_positions, policy_root };
        if r.position() as usize != bytes.len() || borsh::to_vec(&out).map_err(|e| e.to_string())? != bytes {
            return Err("stage class frame has surplus or noncanonical bytes".into());
        }
        FlatLengthTemplateV7::from_bytes(&out.length_template_bytes)?;
        Ok(out)
    }
}
