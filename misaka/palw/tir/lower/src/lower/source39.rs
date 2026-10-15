//! Generic HF lowering into owning39. Old recipes supply primitive expansions,
//! never old source bytes, old validation proxies or old admission authority.
use super::*;
use tir::program_v2::{FIRST_INPUT_REF_V2, InputDecl, InputSource, OutputDecl};
use tir::program_v39::{BlockV39, NodeV39, SourceProfileV39, TensorTypeV39, TirProgramV39};
pub struct LoweredSourceV39 {
    pub program: TirProgramV39,
    recipe: Lowered,
    lifted: Vec<u16>,
}
impl LoweredSourceV39 {
    pub fn lifted_parameters(&self) -> &[u16] {
        &self.lifted
    }
    #[allow(clippy::too_many_arguments)]
    pub fn materialise_stream(
        &self,
        hl: &HlProgram,
        loader: &dyn crate::float_ref::stream::OccParams,
        statistics: &BTreeMap<String, crate::float_ref::SiteStat>,
        policy: &crate::quant::QuantPolicy,
        opts: &super::StreamOpts,
        sink: &mut dyn super::TensorSink,
        progress: &dyn Fn(usize, usize),
    ) -> Result<super::StreamMaterialised> {
        super::materialise_stream(&self.recipe, hl, loader, statistics, policy, opts, sink, progress)
    }
}
pub fn lower_source_v39(
    hl: &HlProgram,
    opts: &LowerOpts,
    profile: SourceProfileV39,
    lifts: &[(&str, InputSource)],
) -> Result<LoweredSourceV39> {
    profile.check().map_err(LowerError::bad)?;
    let recipe = lower_recipe(hl, opts, Some(&profile))?;
    if lifts.len() > tir::program_v2::MAX_INPUTS_V2 {
        return Err(LowerError::bad("SOURCE39_EXTERNAL_INPUT_COUNT"));
    }
    let mut lifted = Vec::new();
    let mut inputs = Vec::new();
    for (name, source) in lifts {
        let j = recipe
            .program
            .params
            .iter()
            .position(|p| p.name == *name)
            .ok_or_else(|| LowerError::bad(format!("SOURCE39_ORIGINAL_INPUT_ABSENT:{name}")))? as u16;
        if lifted.contains(&j) {
            return Err(LowerError::bad("SOURCE39_DUPLICATE_ORIGINAL_INPUT"));
        }
        let p = &recipe.program.params[j as usize];
        if p.per_layer {
            return Err(LowerError::not_lowerable("SOURCE39_PER_LAYER_EXTERNAL_LIFT"));
        }
        lifted.push(j);
        inputs.push(InputDecl { name: p.name.clone(), dtype: p.dtype, shape: p.shape.clone(), source: *source });
    }
    let mut remap = BTreeMap::new();
    let mut params = Vec::new();
    for (j, p) in recipe.program.params.iter().enumerate() {
        if !lifted.contains(&(j as u16)) {
            remap.insert(j as u16, params.len() as u16);
            params.push(p.clone());
        }
    }
    let blocks = recipe
        .program
        .blocks
        .iter()
        .map(|b| BlockV39 {
            name: b.name.clone(),
            carry_in: b.carry_in.iter().map(TensorTypeV39::from_frozen).collect(),
            carry_out: b.carry_out.clone(),
            nodes: b
                .nodes
                .iter()
                .map(|n| {
                    let mut node = NodeV39::from_frozen(n);
                    for r in &mut node.inputs {
                        if let tir::Ref::Param(j) = *r {
                            *r = if let Some(i) = lifted.iter().position(|v| *v == j) {
                                tir::Ref::Input(FIRST_INPUT_REF_V2 + i as u8)
                            } else {
                                tir::Ref::Param(remap[&j])
                            };
                        }
                    }
                    if matches!(n.prim, tir::Prim::StateWrite { .. } | tir::Prim::HistAppend { .. }) {
                        node.commit = true;
                    }
                    node
                })
                .collect(),
        })
        .collect();
    let program = TirProgramV39 {
        version: 39,
        profile,
        prim_set_id: tir::program_v39::PRIM_SET_ID_V39,
        token_bound: recipe.program.token_bound,
        history_bound: recipe.program.history_bound,
        inputs,
        params,
        consts: recipe.program.consts.clone(),
        states: recipe.program.states.clone(),
        blocks,
        schedule: recipe.program.schedule.clone(),
        output: OutputDecl::Logits { node: recipe.program.logits, scheme_id: recipe.program.logits_scheme_id },
    };
    tir::validate_v39::validate_v39(&program).map_err(LowerError::bad)?;
    Ok(LoweredSourceV39 { program, recipe, lifted })
}

/// Complete multi-image join over actual prior projector outputs. Every input
/// is a separately authenticated upstream tensor in pipeline39, not a synthetic
/// parameter or caller-supplied combined PC. The output keeps original row order.
pub fn join_image_rows_v39(
    upstream: &[&tir::program_v39::TirProgramV39],
    profile: SourceProfileV39,
) -> Result<tir::program_v39::TirProgramV39> {
    profile.check().map_err(LowerError::bad)?;
    if upstream.is_empty() || upstream.len() > tir::program_v2::MAX_INPUTS_V2 {
        return Err(LowerError::bad("SOURCE39_IMAGE_JOIN_INPUT_BOUND"));
    }
    let mut declarations = Vec::new();
    let mut rows = 0u32;
    let mut width = None;
    for (i, program) in upstream.iter().enumerate() {
        tir::validate_v39::validate_v39(program).map_err(LowerError::bad)?;
        if !matches!(program.output, OutputDecl::Final { .. }) {
            return Err(LowerError::bad("SOURCE39_IMAGE_JOIN_REQUIRES_ORIGINAL_FINAL_TENSORS"));
        }
        let ty = &program.blocks[program.schedule.post as usize].nodes[program.output.node() as usize].out;
        let [tir::program_v39::DimV39::Fixed(r), tir::program_v39::DimV39::Fixed(w)] = ty.shape.as_slice() else {
            return Err(LowerError::bad("SOURCE39_IMAGE_JOIN_FIXED_ROW_MATRIX"));
        };
        if ty.dtype != tir::DType::I32 || width.is_some_and(|n| n != *w) {
            return Err(LowerError::bad("SOURCE39_IMAGE_JOIN_ORIGINAL_DTYPE_WIDTH"));
        }
        width = Some(*w);
        rows = rows.checked_add(*r).ok_or_else(|| LowerError::bad("SOURCE39_IMAGE_JOIN_ROW_OVERFLOW"))?;
        declarations.push(InputDecl {
            name: format!("image_join39.projector.{i}"),
            dtype: ty.dtype,
            shape: vec![*r, *w],
            source: InputSource::External { lo: i32::MIN as i64, hi: i32::MAX as i64 },
        });
    }
    let width = width.ok_or_else(|| LowerError::bad("SOURCE39_IMAGE_JOIN_EMPTY"))?;
    let mut pb = tir::builder::ProgramBuilder::new(1, 64);
    let refs = declarations.iter().map(|d| pb.param(&d.name, d.dtype, &d.shape, false)).collect::<Vec<_>>();
    let pre = {
        let mut b = pb.block_for_program_v39("image_join39.original_order", vec![], &profile).map_err(LowerError::bad)?;
        let mut level = refs;
        while level.len() > 1 {
            level = level.chunks(8).map(|part| if part.len() == 1 { part[0] } else { b.concat(part, 0) }).collect();
        }
        let value = b.cast(level[0], tir::DType::I32);
        b.commit(value);
        b.finish(&[value])
    };
    let post = {
        let mut b = pb
            .block_for_program_v39("image_join39.output", vec![tir::TensorType::fixed(tir::DType::I32, &[rows, width])], &profile)
            .map_err(LowerError::bad)?;
        let value = b.clamp(tir::Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, tir::DType::I32);
        b.commit(value);
        b.finish(&[])
    };
    let output = (pb.blocks[post as usize].nodes.len() - 1) as u16;
    let recipe = pb.finish(pre, vec![], post, output);
    let program = tir::program_v39::TirProgramV39 {
        version: 39,
        profile,
        prim_set_id: tir::program_v39::PRIM_SET_ID_V39,
        token_bound: 1,
        history_bound: 64,
        inputs: declarations,
        params: vec![],
        consts: recipe.consts,
        states: vec![],
        blocks: recipe
            .blocks
            .iter()
            .map(|b| BlockV39 {
                name: b.name.clone(),
                carry_in: b.carry_in.iter().map(TensorTypeV39::from_frozen).collect(),
                nodes: b
                    .nodes
                    .iter()
                    .map(|n| {
                        let mut n = NodeV39::from_frozen(n);
                        for r in &mut n.inputs {
                            if let tir::Ref::Param(j) = *r {
                                *r = tir::Ref::Input(tir::program_v2::FIRST_INPUT_REF_V2 + j as u8);
                            }
                        }
                        n
                    })
                    .collect(),
                carry_out: b.carry_out.clone(),
            })
            .collect(),
        schedule: recipe.schedule,
        output: OutputDecl::Final { node: output },
    };
    tir::validate_v39::validate_v39(&program).map_err(LowerError::bad)?;
    Ok(program)
}
