//! New court25/resource29 typed geometry. Frozen V12/V14 scope9 identities are
//! neither relaxed nor re-used. Maximum and actual programs are independent.
use super::*;
use crate::{
    flat_inputs12::{bounded14::FlatStageJobV14, source28::CheckedStageSourceV28},
    flat_resources29::{FlatResourceVectorV29, derive_flat_resources_v29},
    flat_scope25::{CheckedResourceScopeV25, derive_checked_flat_resource_scope_v25},
};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatStageAdmissionV28 {
    pub version: u16,
    pub source: Digest,
    pub template: Digest,
    pub job: Digest,
    pub source_parameters: Digest,
    pub inventory: Digest,
    pub maxima: FlatJobLengthsV7,
    pub max_positions: u32,
    pub maximum_program: Digest,
    pub maximum_scope: Digest,
    pub maximum_resources: Digest,
    pub actual_lengths: FlatJobLengthsV7,
    pub positions: u32,
    pub actual_program: Digest,
    pub actual_scope: Digest,
    pub actual_resources: Digest,
    pub combined_parameters: Digest,
    pub inputs: Digest,
}
impl FlatStageAdmissionV28 {
    pub fn root(&self) -> Digest {
        object_id(b"misaka-palw/kernel/stage-max-actual-admission/v28", self)
    }
}
pub struct CheckedStageJobV28 {
    job: FlatStageJobV14,
    admission: FlatStageAdmissionV28,
    program: TirProgramV2,
    scope: CheckedResourceScopeV25,
    maximum_scope: CheckedResourceScopeV25,
    actual: FlatResourceVectorV29,
    maximum: FlatResourceVectorV29,
    parameters: ParamCommitmentsV1,
    input_raw_bytes: u128,
    input_reference_bytes: u128,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, borsh::BorshDeserialize)]
pub struct FlatStageClassWireV28 {
    pub version: u16,
    pub source: Digest,
    pub lengths: Vec<u8>,
    pub max_positions: u32,
    pub policy: Digest,
    pub component_manifest:Digest,
    pub complete_source:Digest,
    pub stage:u16,
}
pub struct CheckedStageClassV28 {
    wire: FlatStageClassWireV28,
    source: CheckedStageSourceV28,
    template: FlatInputTemplateV12,
    maximum_scope: CheckedResourceScopeV25,
    maximum_resources: FlatResourceVectorV29,
    source_restore_bytes:u64,
    root: Digest,
}
impl CheckedStageClassV28 {
    pub fn root(&self) -> Digest {
        self.root
    }
    pub fn wire(&self) -> &FlatStageClassWireV28 {
        &self.wire
    }
    pub fn source(&self) -> &CheckedStageSourceV28 {
        &self.source
    }
    pub fn template(&self) -> &FlatInputTemplateV12 {
        &self.template
    }
    pub fn maximum_scope(&self) -> &CheckedResourceScopeV25 {
        &self.maximum_scope
    }
    pub fn maximum_resources(&self) -> &FlatResourceVectorV29 {
        &self.maximum_resources
    }
    pub fn source_restore_bytes(&self)->u64 {self.source_restore_bytes}
    /// Read-only native original bundle membership must already authenticate
    /// this complete canonical byte extent. The identity binds the immutable
    /// full component manifest; no caller-provided quota vector is accepted.
    pub fn bind_complete_bundle(&mut self,bundle:&CheckedStageBundleV28)->Result<(),String>{
        self.check_component_manifest(&bundle.manifest)?;
        if self.wire.complete_source!=bundle.root||bundle.sources.get(self.wire.stage as usize).is_none_or(|s|*s!=self.source.id())||bundle.bytes>16<<20{return Err("stage28 complete original source bundle differs".into());}
        self.source_restore_bytes=bundle.bytes;Ok(())
    }
    pub fn check_component_manifest(&self,manifest:&crate::flat_component_source28::CheckedComponentManifestV28)->Result<(),String>{
        let stage=manifest.manifest().stages.get(self.wire.stage as usize).ok_or("stage28 complete manifest stage absent")?;
        if self.wire.component_manifest!=manifest.root()||stage.original_program!=crate::flat_component_source28::stage_program_root_v28(self.source.program())||stage.original_inventory!=self.source.inventory_root()||stage.input_declarations!=crate::flat_component_source28::stage_input_declarations_root_v28(self.source.program())||stage.output_declaration!=crate::flat_component_source28::stage_output_declaration_root_v28(self.source.program())||stage.original_context_max>self.source.program().history_bound||stage.original_context_max==0||stage.lowered_context_max!=stage.original_context_max||self.wire.max_positions>stage.original_context_max{return Err("stage28 class changes original complete component/source/IO/context manifest".into());}Ok(())
    }
    pub fn check_job(&self, job: &FlatStageJobV14) -> Result<CheckedStageJobV28, String> {
        let checked = derive_stage_job_v28(&self.template, &self.source, job)?;
        if checked.maximum_scope().root() != self.maximum_scope.root()
            || checked.maximum_resources().root() != self.maximum_resources.root()
        {
            return Err("stage28 job cannot change original maximum class reservation".into());
        }
        Ok(checked)
    }
}
impl FlatStageClassWireV28 {
    pub fn from_bytes(raw: &[u8]) -> Result<Self, String> {
        const TAIL: usize = 4 + 64 + 64 + 64 + 2;
        if raw.len() < 2 + 64 + 4 + TAIL
            || raw.len() > crate::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 + 2 + 64 + 4 + TAIL
            || u16::from_le_bytes(raw[..2].try_into().unwrap()) != 28
        {
            return Err("stage28 class bounded version/extent".into());
        }
        let n = u32::from_le_bytes(raw[66..70].try_into().unwrap()) as usize;
        if n > crate::flat_lengths7::FLAT_LENGTH_MAX_WIRE_BYTES_V7 || raw.len() != 70 + n + TAIL {
            return Err("stage28 class exact bounded original lengths count".into());
        }
        crate::flat_lengths7::FlatLengthTemplateV7::from_bytes(&raw[70..70 + n])?;
        borsh::from_slice(raw).map_err(|_| "stage28 class bounded framing".into())
    }
    pub fn check(&self, source: &CheckedStageSourceV28) -> Result<CheckedStageClassV28, String> {
        if self.version != 28 || self.source != source.id() || self.policy == [0; 64] || self.component_manifest==[0;64] || self.complete_source==[0;64] {
            return Err("stage28 class changes original source/policy".into());
        }
        let lengths = crate::flat_lengths7::FlatLengthTemplateV7::from_bytes(&self.lengths)?;
        let template = FlatInputTemplateV12 {
            program: source.program().clone(),
            lengths,
            max_positions: self.max_positions,
            source_parameters: source.parameters().root(),
        };
        let template_root = stage_template_root_v28(&template, source)?;
        let job = FlatStageJobV14 {
            version: 14,
            template: template_root,
            requester: [1; 64],
            producer: [2; 64],
            nonce: [0; 64],
            actual: template.lengths.maxima,
            positions: template.max_positions,
            input_roots: template
                .program
                .inputs
                .iter()
                .enumerate()
                .map(|(i, d)| object_id(b"misaka-palw/kernel/stage-max-declaration-only/v28", &(template_root, i as u16, d)))
                .collect(),
        };
        let checked = derive_stage_job_v28(&template, source, &job)?;
        let root = object_id(
            b"misaka-palw/kernel/stage-source-court-class/v28",
            &(
                self,
                source.id(),
                source.parameters().root(),
                source.inventory_root(),
                source.input_declarations_root(),
                checked.maximum_scope().root(),
                checked.maximum_resources().root(),
            ),
        );
        Ok(CheckedStageClassV28 {
            wire: self.clone(),
            source: super::source28::FlatStageSourceWireV28 {
                version: 28,
                program_bytes: source.program().encode(),
                inventory_root: source.inventory_root(),
                source_parameters: source.parameters().clone(),
            }
            .check()?,
            template,
            maximum_scope: checked.maximum_scope,
            maximum_resources: checked.maximum,
            source_restore_bytes:source.program().encode().len() as u64+borsh::to_vec(source.parameters()).map_err(|e|e.to_string())?.len() as u64+4096,
            root,
        })
    }
}
pub struct CheckedStageBundleV28 {manifest:crate::flat_component_source28::CheckedComponentManifestV28,sources:Vec<Digest>,root:Digest,bytes:u64}
impl CheckedStageBundleV28 {
    pub fn root(&self)->Digest {self.root}
    pub fn bytes(&self)->u64 {self.bytes}
    pub fn manifest(&self)->&crate::flat_component_source28::CheckedComponentManifestV28 {&self.manifest}
}
/// Immutable all-component source identity and exact canonical carrier extent,
/// derived from checked original V2/real-PC sources, never raw quota digests.
pub fn check_stage_bundle_v28(manifest:&crate::flat_component_source28::CompleteComponentManifestV28,sources:&[CheckedStageSourceV28])->Result<CheckedStageBundleV28,String>{
    let programs=sources.iter().map(|s|s.program().clone()).collect::<Vec<_>>();let inventories=sources.iter().map(|s|s.inventory_root()).collect::<Vec<_>>();let contexts=manifest.stages.iter().map(|s|s.original_context_max).collect::<Vec<_>>();
    let checked=crate::flat_component_source28::check_component_manifest_v28(manifest,&programs,&inventories,&contexts,&manifest.original_required_components,manifest.original_task_metadata,&manifest.inputs).map_err(|e|format!("{e:?}"))?;
    let ids=sources.iter().map(|s|s.id()).collect::<Vec<_>>();let root=object_id(b"misaka-palw/kernel/stage-complete-real-source-bundle/v28",&(28u16,checked.root(),&ids));
    let mut bytes=2u64+4+borsh::to_vec(manifest).map_err(|e|e.to_string())?.len() as u64+4;
    for source in sources {let wire=super::source28::FlatStageSourceWireV28{version:28,program_bytes:source.program().encode(),inventory_root:source.inventory_root(),source_parameters:source.parameters().clone()};bytes=bytes.checked_add(4+borsh::to_vec(&wire).map_err(|e|e.to_string())?.len() as u64).ok_or("stage28 original source extent overflow")?;}
    if bytes>16<<20{return Err("stage28 full original bundle exceeds bounded carrier".into());}
    Ok(CheckedStageBundleV28{manifest:checked,sources:ids,root,bytes})
}
impl CheckedStageJobV28 {
    pub fn job(&self) -> &FlatStageJobV14 {
        &self.job
    }
    pub fn admission(&self) -> &FlatStageAdmissionV28 {
        &self.admission
    }
    pub fn program(&self) -> &TirProgramV2 {
        &self.program
    }
    pub fn scope(&self) -> &CheckedResourceScopeV25 {
        &self.scope
    }
    pub fn maximum_scope(&self) -> &CheckedResourceScopeV25 {
        &self.maximum_scope
    }
    pub fn resources(&self) -> &FlatResourceVectorV29 {
        &self.actual
    }
    pub fn maximum_resources(&self) -> &FlatResourceVectorV29 {
        &self.maximum
    }
    pub fn combined_parameters(&self) -> &ParamCommitmentsV1 {
        &self.parameters
    }
    pub fn input_raw_bytes(&self) -> u128 {
        self.input_raw_bytes
    }
    pub fn input_reference_bytes(&self) -> u128 {
        self.input_reference_bytes
    }
    pub fn trace_geometry(&self,position:u32,occurrence:u16,node:u16)->Result<StageTraceGeometryV28,String>{
        if position>=self.job.positions{return Err("stage28 trace position exceeds original actual extent".into());}
        let program=self.program.v1_view();let p=crate::element::PreparedV1::without_scope(&program,None)?;
        let block=p.w.occurrences.get(occurrence as usize).ok_or("stage28 trace occurrence absent")?.0;
        let n=program.blocks[block as usize].nodes.get(node as usize).ok_or("stage28 trace node absent")?;
        let shapes=p.shapes(position,occurrence,node);
        Ok(StageTraceGeometryV28{dtype:n.out.dtype,shape:shapes.out,node_index:p.index(occurrence,node),node_count:p.node_count,positions:self.job.positions})
    }
    pub fn acquire(&self, provider: &dyn RawExternalInputsV12) -> Result<Vec<Tensor>, String> {
        let mut out = Vec::with_capacity(self.program.inputs.len());
        for (i, decl) in self.program.inputs.iter().enumerate() {
            let raw = provider.input(i as u16).ok_or("stage28 original external image absent")?;
            let count = decl.shape.iter().try_fold(1usize, |n, d| n.checked_mul(*d as usize)).ok_or("stage28 input shape overflow")?;
            if raw.dtype != decl.dtype
                || raw.shape != decl.shape
                || raw.bytes.len() != count.checked_mul(decl.dtype.width()).ok_or("stage28 input bytes overflow")?
            {
                return Err("stage28 image differs from original independently derived dtype/shape/extent".into());
            }
            let shape: Vec<usize> = decl.shape.iter().map(|d| *d as usize).collect();
            let data = Tensor::from_le_bytes(raw.dtype, &shape, raw.bytes).map_err(|e| e.to_string())?;
            let (lo, hi) = decl.interval();
            if data.data.iter().any(|v| *v < lo || *v > hi) || tensor_commitment_v3(&data) != self.job.input_roots[i] {
                return Err("stage28 original input interval/PC differs".into());
            }
            out.push(data);
        }
        Ok(out)
    }
}
pub struct StageTraceGeometryV28 {pub dtype:misaka_palw_tir::DType,pub shape:Vec<usize>,pub node_index:u64,pub node_count:u64,pub positions:u32}
pub fn decode_stage_node_opening_v28(raw:&[u8])->Result<crate::seg::NodeOpeningV1,String>{
    if raw.len()>16384{return Err("stage28 original node opening cap".into());}let mut r=std::io::Cursor::new(raw);let o=crate::flat_court4::read_node(&mut r).map_err(|e|e.to_string())?;if r.position() as usize!=raw.len(){return Err("stage28 node opening surplus bytes".into());}Ok(o)
}
pub fn stage_template_root_v28(template: &FlatInputTemplateV12, source: &CheckedStageSourceV28) -> Result<Digest, String> {
    if template.program != *source.program() || template.source_parameters != source.parameters().root() {
        return Err("stage28 template substitutes original source/weights".into());
    }
    template.root()?;
    Ok(object_id(
        b"misaka-palw/kernel/stage-checker25-template/v28",
        &(28u16, source.id(), &template.program, &template.lengths, template.max_positions, template.source_parameters),
    ))
}
pub(super) fn specialize(template: &FlatInputTemplateV12, lengths: FlatJobLengthsV7, positions: u32) -> Result<TirProgramV2, String> {
    misaka_palw_tir::validate_v2::validate_v2(&template.program).map_err(|e| e.to_string())?;
    if template.max_positions == 0
        || positions == 0
        || positions > template.max_positions
        || template.max_positions > template.program.history_bound
    {
        return Err("stage28 extent/output/random/post-state/token semantics unsupported".into());
    }
    if template.program.v1_view().encode() != template.lengths.max_program_bytes {
        return Err("stage28 lengths do not bind original maximum graph".into());
    }
    let first = template.program.params.len();
    let mut present = [false; 3];
    for b in &template.lengths.bindings {
        match b.slot {
            FlatShapeSlotV7::Parameter { parameter, .. } => {
                let i = (parameter as usize).checked_sub(first).ok_or("stage28 cannot resize real weights")?;
                if i >= template.program.inputs.len() {
                    return Err("stage28 input axis undeclared".into());
                }
                present[match b.length {
                    FlatLengthAxisV7::Token => 0,
                    FlatLengthAxisV7::Patch => 1,
                    FlatLengthAxisV7::Audio => 2,
                }] = true;
            }
            FlatShapeSlotV7::State { .. } => return Err("stage28 cannot resize persistent state or H".into()),
            _ => {}
        }
    }
    for (i, a) in [FlatLengthAxisV7::Token, FlatLengthAxisV7::Patch, FlatLengthAxisV7::Audio].into_iter().enumerate() {
        if (template.lengths.maxima.get(a) != 0) != present[i] {
            return Err("stage28 declared modality lacks independent actual/max axis".into());
        }
    }
    shape_payload_v28(&template.program)?;
    misaka_palw_tir::interval_v2::analyze_ranges_v2(&template.program).map_err(|e| e.to_string())?;
    let view = template.lengths.program(lengths)?;
    let mut p = template.program.clone();
    for (i, d) in p.inputs.iter_mut().enumerate() {
        d.shape = view.params[first + i].shape.clone();
    }
    for (b, v) in p.blocks.iter_mut().zip(&view.blocks) {
        b.carry_in = v.carry_in.clone();
        b.carry_out = v.carry_out.clone();
        for (n, v) in b.nodes.iter_mut().zip(&v.nodes) {
            n.out = v.out.clone();
        }
    }
    if p.v1_view() != view {
        return Err("stage28 shape changed primitive attributes/source/state/H".into());
    }
    misaka_palw_tir::validate_v2::validate_v2(&p).map_err(|e| e.to_string())?;
    misaka_palw_tir::interval_v2::analyze_ranges_v2(&p).map_err(|e| e.to_string())?;
    Ok(p)
}
pub fn derive_stage_job_v28(
    template: &FlatInputTemplateV12,
    source: &CheckedStageSourceV28,
    job: &FlatStageJobV14,
) -> Result<CheckedStageJobV28, String> {
    let template_root = stage_template_root_v28(template, source)?;
    if job.version != 14
        || job.template != template_root
        || job.input_roots.len() != template.program.inputs.len()
        || job.input_roots.len() > INPUT_COUNT_MAX_V12
    {
        return Err("stage28 bounded job input count/template/version differs".into());
    }
    let maximum = specialize(template, template.lengths.maxima, template.max_positions)?;
    let actual = specialize(template, job.actual, job.positions)?;
    let mut pc = source.parameters().clone();
    let mut max_pc = pc.clone();
    let first = source.program().params.len();
    for (i, d) in maximum.inputs.iter().enumerate() {
        max_pc.by_instance.insert(
            ((first + i) as u16, None),
            object_id(b"misaka-palw/kernel/stage-max-declaration-only/v28", &(template_root, i as u16, d)),
        );
        pc.by_instance.insert(((first + i) as u16, None), job.input_roots[i]);
    }
    let max_scope =
        derive_checked_flat_resource_scope_v25(&maximum.v1_view(), template.max_positions, None).map_err(|e| format!("{e:?}"))?;
    let scope = derive_checked_flat_resource_scope_v25(&actual.v1_view(), job.positions, None).map_err(|e| format!("{e:?}"))?;
    let max_resource = derive_flat_resources_v29(&maximum.v1_view(), &max_pc, template.max_positions, None)?;
    let resource = derive_flat_resources_v29(&actual.v1_view(), &pc, job.positions, None)?;
    if max_resource.court != *max_scope.scope() || resource.court != *scope.scope() {
        return Err("stage28 independent maximum/actual resource checker scopes differ".into());
    }
    let (raw, reference) = shape_payload_v28(&actual)?;
    let admission = FlatStageAdmissionV28 {
        version: 28,
        source: source.id(),
        template: template_root,
        job: job.root(),
        source_parameters: source.parameters().root(),
        inventory: source.inventory_root(),
        maxima: template.lengths.maxima,
        max_positions: template.max_positions,
        maximum_program: object_id(b"misaka-palw/kernel/stage-maximum-program/v28", &maximum),
        maximum_scope: max_scope.root(),
        maximum_resources: max_resource.vector.root(),
        actual_lengths: job.actual,
        positions: job.positions,
        actual_program: object_id(b"misaka-palw/kernel/stage-actual-program/v28", &actual),
        actual_scope: scope.root(),
        actual_resources: resource.vector.root(),
        combined_parameters: pc.root(),
        inputs: object_id(b"misaka-palw/kernel/stage-original-inputs/v28", &(job.root(), &job.input_roots)),
    };
    Ok(CheckedStageJobV28 {
        job: job.clone(),
        admission,
        program: actual,
        scope,
        maximum_scope: max_scope,
        actual: resource.vector,
        maximum: max_resource.vector,
        parameters: pc,
        input_raw_bytes: raw,
        input_reference_bytes: reference,
    })
}

/// New28 geometry includes zero-input causal terminal stages. Source/runtime
/// authority for token, random and post-state semantics belongs to invocation32.
fn shape_payload_v28(p:&TirProgramV2)->Result<(u128,u128),String>{
    if p.inputs.len()>INPUT_COUNT_MAX_V12{return Err("stage28 input count exceeds cap".into());}
    let(mut raw,mut expanded)=(0u128,0u128);for d in &p.inputs{let cells=d.shape.iter().try_fold(1u128,|a,n|a.checked_mul(*n as u128)).ok_or("stage28 input shape overflow")?;raw=raw.checked_add(cells.checked_mul(d.dtype.width() as u128).ok_or("stage28 input bytes overflow")?).ok_or("stage28 input sum overflow")?;expanded=expanded.checked_add(cells.checked_mul(16).ok_or("stage28 input expansion overflow")?).ok_or("stage28 input sum overflow")?;}if raw>INPUT_RAW_BYTES_MAX_V12||expanded>INPUT_REFERENCE_BYTES_MAX_V12{return Err("stage28 declared input extent exceeds original bounded geometry".into());}Ok((raw,expanded))
}
