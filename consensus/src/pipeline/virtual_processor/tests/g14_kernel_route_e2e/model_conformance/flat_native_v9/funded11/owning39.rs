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
    // The same TEST calibration for the v39 ordinary court profile (sorted by profile, as Final27 requires).
    {
        let policy = &mut cfg.params.palw_native_final_v27.as_mut().unwrap().policy;
        let mut c = policy.calibrations[0].clone();
        c.profile = misaka_palw_kernel::flat_resources39::ordinary_court_id_v39();
        policy.calibrations.push(c);
        policy.calibrations.sort_by_key(|c| c.profile);
    }
    let mut fence = PalwNativeExecutionFenceV39::unapproved_probe_at(ForkActivation::new(0), domain);
    let final_policy = cfg.params.palw_native_final_v27.as_ref().unwrap().policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&cfg.params);
    fence.policies.execution = cfg.params.palw_funded_execution_v11.as_ref().unwrap().policies.clone();
    fence.policies.final_policy = final_policy.root();
    fence.policies.physical_service =
        kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(&mass, &final_policy.queue_policy)
            .unwrap()
            .root();
    // The whole v39 opportunity (commit + challenge + proof/delivery tail) under the widened test horizons.
    fence.policies.maximum_horizon = 99_000;
    cfg.params.palw_native_execution_v39 = Some(fence);
    (cfg, b, p, fl)
}

fn stage(net: &mut Net, card: usize, action: u8, body: Vec<u8>) -> Obj {
    net.route(card, &K::StageExecutionV14 { action, body })
}

#[tokio::test]
async fn owning39_fixture_registers_consents_and_funds_a_real_job() {
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
    let class_id: Digest =
        net.api().unwrap().aux.keys().find(|(t, k)| *t == 112 && k.starts_with(&[39, 2])).map(|(_, k)| k[2..66].try_into().unwrap()).unwrap();
    // ---- the producer's consent (43) over the derived maximum contract, then the requester's Fund (44) ----
    use misaka_palw_kernel::flat_job39::*;
    let manifest = MaterialManifestV39 { version: 39, source, master: [0; 64], bindings: vec![] };
    let request = JobMaximumV39 {
        version: 39,
        source,
        requester: net.kid(2),
        producer: net.kid(0),
        nonce: nonce_bytes_v11(0),
        stop: RequestStopV39::Fixed { positions: 2, tokens: vec![0, 0] },
        input_manifest: manifest.root(),
    };
    let checked_class = class.check(&checked, &policies).unwrap();
    assert_eq!(checked_class.root(), class_id);
    let maximum = CheckedJobMaximumV39::check(&checked, &request, &manifest).unwrap();
    let quota = misaka_palw_kernel::flat_quota11::owning39::derive_quota_v39(&checked, &maximum, class_id, policies.root(), 2, 32).unwrap();
    let final_policy = net.config.params.palw_native_final_v27.as_ref().unwrap().policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&net.config.params);
    let service =
        kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(&mass, &final_policy.queue_policy).unwrap();
    let contract = kaspa_consensus_core::palw_native_job_v39::CheckedNativeMaximumV39::derive(
        &checked_class, &maximum, &quota, &policies, &final_policy, &net.api().unwrap().header.policy, &mass, &service,
    );
    let contract = match contract {
        Ok(c) => c,
        Err(why) => panic!("owning39 maximum contract (the capacity the job would need vs the service): {why}"),
    };
    let tip = net.chain.tip_state().1;
    let interval = policies.execution.quota.interval_daa;
    let now = net.daa() + 1;
    let consent = kaspa_consensus_core::palw_native_job_v39::NativeConsentWireV39 {
        version: 39,
        class: class_id,
        requester: net.bond(2),
        requester_operator: tip.bond(&net.bond(2)).unwrap().operator_id.as_bytes(),
        producer_operator: tip.bond(&net.bond(0)).unwrap().operator_id.as_bytes(),
        reward: 20,
        expires: now + contract.record().temporal.commit_window.min(interval - now % interval - 1),
        maximum_contract: contract.record().root(),
        maximum: borsh::to_vec(&request).unwrap(),
        material: borsh::to_vec(&manifest).unwrap(),
    };
    let o = stage(&mut net, 0, 43, borsh::to_vec(&consent).unwrap());
    net.send(vec![(0, o)]).await;
    let consents: Vec<Digest> =
        net.api().unwrap().aux.keys().filter(|(t, k)| *t == 112 && k.starts_with(&[39, 18])).map(|(_, k)| k[2..66].try_into().unwrap()).collect();
    assert_eq!(consents.len(), 1, "the producer's v39 consent landed");
    let fund = kaspa_consensus_core::palw_native_job_v39::NativeFundWireV39 { version: 39, consent: consents[0] };
    let o = stage(&mut net, 2, 44, borsh::to_vec(&fund).unwrap());
    net.send(vec![(2, o)]).await;
    let jobs = net.api().unwrap().aux.keys().filter(|(t, k)| *t == 112 && k.starts_with(&[39, 16])).count();
    assert_eq!(jobs, 1, "the v39 job is funded");
    let replay = net.replay().await;
    net.assert_same(&replay, "owning39 source and class");
}
