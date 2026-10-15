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
                                tir::Ref::Input(FIRST_INPUT_REF_V2 + i as u16)
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
