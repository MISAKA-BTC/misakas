//! **The owning39 (v39) job fixture on the ordinary node (G14 lane closure, 2026-10-18)**: source registration (41) and class
//! registration (42) of a real `FlatSourceWireV39`, under native39 armed beside funded11 and Final27 (`text29`'s config), so the
//! G-N2 mining court and the v39 job court can run on real rows. Structural test values, never measurements.
use super::*;
use kaspa_consensus_core::palw_native_execution_v39::*;
use misaka_palw_kernel::flat_source39::*;
use misaka_palw_tir::{ConstDecl, DType, Prim, Ref, Schedule, program_v2::OutputDecl, program_v39::*};

fn source_wire() -> FlatSourceWireV39 {
    let out = TensorTypeV39::scalar(DType::I16);
    let p = TirProgramV39 {
        version: 39,
        profile: SourceProfileV39::full_source(),
        prim_set_id: PRIM_SET_ID_V39,
        token_bound: 16,
        history_bound: 32,
        inputs: vec![],
        params: vec![],
        consts: vec![ConstDecl { dtype: DType::I16, shape: vec![], data: 5i16.to_le_bytes().to_vec() }],
        states: vec![],
        blocks: vec![
            BlockV39 {
                name: "pre".into(),
                carry_in: vec![],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::Const(0)], out: out.clone(), commit: true }],
                carry_out: vec![0],
            },
            BlockV39 {
                name: "post".into(),
                carry_in: vec![out.clone()],
                nodes: vec![NodeV39 { prim: NodePrimV39::Frozen(Prim::Cast), inputs: vec![Ref::CarryIn(0)], out, commit: true }],
                carry_out: vec![],
            },
        ],
        schedule: Schedule { pre: 0, layers: vec![], post: 1 },
        output: OutputDecl::Final { node: 0 },
    };
    FlatSourceWireV39 {
        version: 39,
        program_bytes: p.encode(),
        inventory_root: derive_inventory_root_v39(&p, [9; 64], &[]).unwrap(),
        real_parameters: misaka_palw_kernel::trace::ParamCommitmentsV1::default(),
        original_context_max: 32,
        original_task_metadata: [9; 64],
        original_inventory: vec![],
    }
}

/// text29's config plus native39, its policies bound to the funded policies, the Final27 policy and the physical service.
fn owning_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let f = sparse9(false);
    let (mut cfg, b, p, fl) = super::text29::text_config(misaka_palw_kernel::flat_resources29::derive_flat_resources_v29(&f.p, &f.pc, 3, None).unwrap().court.court_set);
    let domain = cfg.params.palw_funded_execution_v11.as_ref().unwrap().policies.execution.network_domain;
    let mut fence = PalwNativeExecutionFenceV39::unapproved_probe_at(ForkActivation::new(0), domain);
    let final_policy = cfg.params.palw_native_final_v27.as_ref().unwrap().policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&cfg.params);
    fence.policies.execution = cfg.params.palw_funded_execution_v11.as_ref().unwrap().policies.clone();
    fence.policies.final_policy = final_policy.root();
    fence.policies.physical_service =
        kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(&mass, &final_policy.queue_policy)
            .unwrap()
            .root();
    cfg.params.palw_native_execution_v39 = Some(fence);
    (cfg, b, p, fl)
}

fn stage(net: &mut Net, card: usize, action: u8, body: Vec<u8>) -> Obj {
    net.route(card, &K::StageExecutionV14 { action, body })
}

#[tokio::test]
async fn owning39_fixture_registers_a_real_source_and_class() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(owning_config(), TestConsensus::new);
    net.beat_to(5).await;
    let wire = source_wire();
    let checked = wire.clone().check().expect("the fixture source checks");
    let o = stage(&mut net, 0, REGISTER_SOURCE_ACTION_V39, borsh::to_vec(&wire).unwrap());
    net.send(vec![(0, o)]).await;
    let source = checked.root();
    let key = borsh::to_vec(&(39u8, 0u8, source)).unwrap();
    assert!(net.api().unwrap().aux.contains_key(&(112, key)), "the v39 source is registered");
    let policies = net.config.params.palw_native_execution_v39.as_ref().unwrap().policies.clone();
    let derived = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(&checked, 2).unwrap();
    let class = NativeClassWireV39 {
        version: 39,
        source,
        policy: policies.root(),
        max_positions: 2,
        scope: derived.scope().root(),
        resources: derived.root(),
    };
    let o = stage(&mut net, 0, REGISTER_CLASS_ACTION_V39, borsh::to_vec(&class).unwrap());
    net.send(vec![(0, o)]).await;
    let classes = net.api().unwrap().aux.keys().filter(|(t, k)| *t == 112 && k.starts_with(&[39, 2])).count();
    assert_eq!(classes, 1, "the v39 class is registered");
    let replay = net.replay().await;
    net.assert_same(&replay, "owning39 source and class");
}
