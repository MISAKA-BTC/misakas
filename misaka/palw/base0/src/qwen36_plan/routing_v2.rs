//! Prospective selected-router binding. The historical compiler and last_topk
//! choices are untouched; only an explicitly owned V2 plan retargets them.
use super::*;
use kaspa_consensus_core::{
    Hash64,
    palw_material_capacity_v1::PalwMaterialCapacityErrorV1 as E,
    palw_native_resource_v1::{NativeDemandLeaseV1, reserve_native_requests_v1},
    palw_reader_router_v2::{
        PalwReaderRouterDeclarationV2, PalwReaderRouterErrorV2, needs_reader_router_v2, reader_router_message_v2,
    },
    palw_routed_material_v2::PalwMaterialConstructionRequestsV2,
};

/// Internal owner, never a wire value. Backend/ephemeral replay retain the lease
/// at least as long as every cloned per-layer plan table.
pub(crate) struct Qwen36NativePlanV2 {
    pub(crate) plan: Qwen36ProfilePlanV1,
    pub(crate) lease: Option<NativeDemandLeaseV1>,
}
fn identity(why: &str) -> E {
    E::Authentication(why.into())
}
fn router_error(e: PalwReaderRouterErrorV2) -> E {
    match e {
        PalwReaderRouterErrorV2::Resource(e) => E::LocalResource(e),
        other => identity(&other.to_string()),
    }
}
fn quote_name(q: &mut PalwMaterialConstructionRequestsV2, name: &NameRef) -> Result<(), E> {
    let (NameRef::PerLayer(value) | NameRef::Global(value)) = name;
    q.vec_v1::<u8>(value.len() as u64).map_err(E::LocalResource)
}
fn quote_table(q: &mut PalwMaterialConstructionRequestsV2, nodes: &[PlanNode]) -> Result<(), E> {
    q.vec_v1::<PlanNode>(nodes.len() as u64).map_err(E::LocalResource)?;
    for node in nodes {
        q.vec_v1::<PlanInput>(node.inputs.len() as u64).map_err(E::LocalResource)?;
        match &node.op {
            PlanOpV1::RequantSized { name }
            | PlanOpV1::HeadRequant { name, .. }
            | PlanOpV1::ProbsRequant { name }
            | PlanOpV1::Project { name, .. }
            | PlanOpV1::ScaleRow { name }
            | PlanOpV1::MulWide { name }
            | PlanOpV1::SharedApply { name }
            | PlanOpV1::GateApply { name }
            | PlanOpV1::RopePartial { name }
            | PlanOpV1::AttnScores { name }
            | PlanOpV1::Softmax { name }
            | PlanOpV1::AttnValues { name }
            | PlanOpV1::RouterTopk { name, .. }
            | PlanOpV1::MoeCombine { name, .. } => quote_name(q, name)?,
            PlanOpV1::AttnFused { softmax, scores, probs, values } => {
                for name in [softmax, scores, probs, values] {
                    quote_name(q, name)?;
                }
            }
            PlanOpV1::EmbedGather
            | PlanOpV1::EmbedLift
            | PlanOpV1::RmsNormUnit
            | PlanOpV1::HeadRmsNorm { .. }
            | PlanOpV1::GdnHeadNormWide
            | PlanOpV1::RoutedProject { .. }
            | PlanOpV1::SsmConv
            | PlanOpV1::Silu
            | PlanOpV1::Decay
            | PlanOpV1::SigmoidGate
            | PlanOpV1::L2NormSeg { .. }
            | PlanOpV1::GdnStep
            | PlanOpV1::RoutedGatedMul { .. }
            | PlanOpV1::SharedGatedMul
            | PlanOpV1::AddElem => {}
        }
    }
    Ok(())
}
fn bind_table(
    profile: &PalwShapeProfileV3,
    declaration: &PalwReaderRouterDeclarationV2,
    first: u32,
    nodes: &[PalwStepNodeV1],
    table: &mut [PlanNode],
) -> Result<(), E> {
    if nodes.len() != table.len() {
        return Err(identity("native routing table differs from compiled source"));
    }
    for (index, (source, compiled)) in nodes.iter().zip(table.iter_mut()).enumerate() {
        if !needs_reader_router_v2(source) {
            continue;
        }
        let reader = first.checked_add(u32::try_from(index).map_err(|_| E::Capacity)?).ok_or(E::Capacity)?;
        let selected = declaration.selected_router(reader).ok_or_else(|| identity("native reader mapping is incomplete"))?;
        let local = selected.checked_sub(first).ok_or_else(|| identity("selected router lies outside actual reader table"))? as usize;
        if local >= index {
            return Err(identity("selected router must precede its actual reader"));
        }
        let (router, _) = profile.resolve_node_slot(selected).ok_or(E::Incomplete)?;
        if router.kernel_semantics_id != kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)
            || !matches!(table_router_op(nodes, local), Some(()))
        {
            return Err(identity("native Q36 plan selected an unsupported router family"));
        }
        match &mut compiled.op {
            PlanOpV1::RoutedProject { topk, .. } | PlanOpV1::RoutedGatedMul { topk } => *topk = local,
            PlanOpV1::MoeCombine { .. } => {
                if compiled.inputs.get(1) != Some(&PlanInput::Row(local)) {
                    return Err(identity("Q36 combine's declared input differs from its native selected router"));
                }
            }
            _ => return Err(identity("native routed reader has no actual executable Q36 operation")),
        }
    }
    Ok(())
}
fn table_router_op(nodes: &[PalwStepNodeV1], local: usize) -> Option<()> {
    let n = nodes.get(local)?;
    (n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)).then_some(())
}

impl Qwen36ProfilePlanV1 {
    /// Crate-private: production callers first obtain either own prospective
    /// native execution authority or the owning claim's material authority.
    /// A deserialized provider declaration cannot call a public plan binder.
    pub(crate) fn bind_reader_router_v2(
        &self,
        profile: &PalwShapeProfileV3,
        declaration: &PalwReaderRouterDeclarationV2,
        network: Hash64,
    ) -> Result<Qwen36NativePlanV2, E> {
        declaration.check_profile(profile).map_err(router_error)?;
        let id = reader_router_message_v2(network, declaration);
        if self.class_id != declaration.class_id
            || self.layer_kinds.len() != profile.layer_count as usize
            || self.routing_identity_v2.is_some_and(|old| old != id)
        {
            return Err(identity("native routing plan identity differs from compiled/native class"));
        }
        let mut q = PalwMaterialConstructionRequestsV2::new();
        q.vec_v1::<Qwen36ProfilePlanV1>(1).map_err(E::LocalResource)?;
        q.vec_v1::<Qwen36LayerKind>(self.layer_kinds.len() as u64).map_err(E::LocalResource)?;
        q.vec_v1::<Vec<PlanNode>>(self.layer_kinds.len() as u64).map_err(E::LocalResource)?;
        for table in [&self.pre, &self.gdn, &self.attn, &self.post] {
            quote_table(&mut q, table)?;
        }
        for kind in &self.layer_kinds {
            quote_table(
                &mut q,
                match kind {
                    Qwen36LayerKind::LinearAttention => &self.gdn,
                    Qwen36LayerKind::FullAttention => &self.attn,
                },
            )?;
        }
        let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(E::LocalResource)?;
        let mut plan = Self {
            pre: self.pre.clone(),
            gdn: self.gdn.clone(),
            attn: self.attn.clone(),
            post: self.post.clone(),
            layer_kinds: self.layer_kinds.clone(),
            attn_history: self.attn_history,
            class_id: self.class_id,
            routing_layers_v2: Some(Vec::with_capacity(self.layer_kinds.len())),
            routing_identity_v2: Some(id),
        };
        bind_table(profile, declaration, 0, &profile.pre_nodes, &mut plan.pre)?;
        let mut first = u32::try_from(profile.pre_nodes.len()).map_err(|_| E::Capacity)?;
        for kind in &self.layer_kinds {
            let (source, compiled) = match kind {
                Qwen36LayerKind::LinearAttention => (&profile.gdn_nodes, &self.gdn),
                Qwen36LayerKind::FullAttention => (&profile.attn_nodes, &self.attn),
            };
            let mut table = compiled.clone();
            bind_table(profile, declaration, first, source, &mut table)?;
            first = first.checked_add(u32::try_from(source.len()).map_err(|_| E::Capacity)?).ok_or(E::Capacity)?;
            plan.routing_layers_v2.as_mut().expect("explicit V2 layer inventory").push(table);
        }
        bind_table(profile, declaration, first, &profile.post_nodes, &mut plan.post)?;
        Ok(Qwen36NativePlanV2 { plan, lease })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_reader_router_v2::PalwReaderRouterBindingV2;

    fn second_router(nodes: &mut Vec<PalwStepNodeV1>) {
        let first = nodes.iter().position(|n| n.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)).unwrap();
        let insert = first + 1;
        let duplicate = nodes[first].clone();
        for node in nodes.iter_mut() {
            for input in &mut node.input_refs {
                if *input < PALW_STEP_INPUT_SENTINEL_MIN && *input as usize >= insert {
                    *input += 1;
                }
            }
        }
        nodes.insert(insert, duplicate);
    }

    // Source controls only. They exercise actual engine/table dispatch, not a
    // fixture status standing in for native claim admission or selected credit.
    #[test]
    fn separate_native_readers_and_layers_retarget_actual_q36_execution() {
        let (artifact, mut profile) = crate::fuzz_qwen36::tiny_class_v5_for_tests();
        second_router(&mut profile.gdn_nodes);
        second_router(&mut profile.attn_nodes);
        profile.validate_shape().unwrap();
        let engine = Qwen36Engine::new(&artifact);
        let historical = engine.plan_from_profile(&profile).unwrap();
        assert!(historical.routing_layers_v2.is_none());
        let mut bindings = Vec::new();
        let mut noncombine = Vec::new();
        for reader_slot in 0..profile.global_node_count() {
            let (reader, layer) = profile.resolve_node_slot(reader_slot).unwrap();
            if !needs_reader_router_v2(reader) {
                continue;
            }
            let layer = layer.unwrap();
            let routers: Vec<_> = (0..reader_slot)
                .filter(|slot| {
                    let (node, li) = profile.resolve_node_slot(*slot).unwrap();
                    li == Some(layer) && node.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_ROUTER_TOPK)
                })
                .collect();
            assert_eq!(routers.len(), 2);
            let combine = reader.kernel_semantics_id == kernel_semantics_id_v1(KDESC_Q36_MOE_COMBINE);
            let router_slot = routers[if combine { 0 } else { layer as usize % 2 }];
            bindings.push(PalwReaderRouterBindingV2 { reader_slot, router_slot });
            if !combine {
                noncombine.push((reader_slot, layer, router_slot));
            }
        }
        let declaration = PalwReaderRouterDeclarationV2 {
            version: 2,
            class_id: profile.shape_profile_id(),
            artifact_root: Hash64::from_u64_word(44),
            bindings,
        };
        declaration.check_profile(&profile).unwrap();
        let native = historical.bind_reader_router_v2(&profile, &declaration, Hash64::from_u64_word(7)).unwrap();
        let layers = native.plan.routing_layers_v2.as_ref().unwrap();
        assert_eq!(layers.len(), profile.layer_count as usize);
        let mut first = profile.pre_nodes.len() as u32;
        for (li, table) in layers.iter().enumerate() {
            for &(reader, layer, router) in &noncombine {
                if layer as usize != li {
                    continue;
                }
                match &table[(reader - first) as usize].op {
                    PlanOpV1::RoutedProject { topk, .. } | PlanOpV1::RoutedGatedMul { topk } => {
                        assert_eq!(*topk, (router - first) as usize);
                    }
                    _ => panic!("the declared routed reader must be an actual routed operation"),
                }
            }
            first += table.len() as u32;
        }
        let (a, traced) = engine.forward_token_planned(&native.plan, &mut Qwen36Cache::new(&artifact.shape), 1, 0).unwrap();
        let b = engine.forward_token_planned_logits(&native.plan, &mut Qwen36Cache::new(&artifact.shape), 1, 0).unwrap();
        let (c, batched) = engine.forward_prefill_planned(&native.plan, &mut Qwen36Cache::new(&artifact.shape), &[1], 0).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, c);
        assert_eq!(traced, batched[0]);
        let mut missing = declaration.clone();
        missing.bindings.pop();
        assert!(historical.bind_reader_router_v2(&profile, &missing, Hash64::from_u64_word(7)).is_err());
        let mut incomplete = native.plan.clone();
        incomplete.routing_layers_v2.as_mut().unwrap().pop();
        assert!(
            engine.forward_token_planned_logits(&incomplete, &mut Qwen36Cache::new(&artifact.shape), 1, 0).is_err(),
            "a missing native layer never falls back to the V1 table"
        );
    }
}
