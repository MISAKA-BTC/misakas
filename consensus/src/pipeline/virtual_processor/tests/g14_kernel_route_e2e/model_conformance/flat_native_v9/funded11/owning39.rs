//! **The owning39 (v39) job fixture on the ordinary node (G14 lane closure, 2026-10-18)**: source registration (41) and class
//! registration (42) of a real `FlatSourceWireV39`, under native39 armed beside funded11 and Final27 (`text29`'s config), so the
//! G-N2 mining court and the v39 job court can run on real rows. Structural test values, never measurements.
use super::*;
use kaspa_consensus_core::palw_native_execution_v39::*;
use misaka_palw_kernel::flat_job39::CheckedJobMaximumV39;
use misaka_palw_kernel::flat_source39::*;
use misaka_palw_tir::{ConstDecl, DType, Prim, Ref, Schedule, program_v2::OutputDecl, program_v39::*};

fn source_wire() -> FlatSourceWireV39 {
    source_wire_at(32)
}

/// The fixture source with original context `context` (a Master component's class must cover the whole original context).
fn source_wire_at(context: u32) -> FlatSourceWireV39 {
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
                nodes: vec![NodeV39 {
                    prim: NodePrimV39::Frozen(Prim::Cast),
                    inputs: vec![Ref::Const(0)],
                    out: out.clone(),
                    commit: true,
                }],
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
        original_context_max: context,
        original_task_metadata: [9; 64],
        original_inventory: vec![],
    }
}

/// text29's config plus native39, its policies bound to the funded policies, the Final27 policy and the physical service.
fn owning_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    owning_config_with(None)
}

/// `owning_config`, optionally with a shorter challenge window (a job that must reach Final unaccused in the test).
fn owning_config_with(challenge: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let f = sparse9(false);
    let (mut cfg, b, p, fl) = super::text29::text_config(
        misaka_palw_kernel::flat_resources29::derive_flat_resources_v29(&f.p, &f.pc, 3, None).unwrap().court.court_set,
    );
    let domain = cfg.params.palw_funded_execution_v11.as_ref().unwrap().policies.execution.network_domain;
    // The same TEST calibration for the v39 ordinary court profile (sorted by profile, as Final27 requires).
    {
        let policy = &mut cfg.params.palw_native_final_v27.as_mut().unwrap().policy;
        let mut c = policy.calibrations[0].clone();
        c.profile = misaka_palw_kernel::flat_resources39::ordinary_court_id_v39();
        policy.calibrations.push(c);
        policy.calibrations.sort_by_key(|c| c.profile);
    }
    // v39's commit-transport window bounds its paged obligations' deadline (5 DAA in the probe: ~88 packets in 5 DAA at one slot per
    // DAA is 17.6× the physical service). A test horizon, not a measurement.
    cfg.params.palw_funded_execution_v11.as_mut().unwrap().policies.execution.horizons.commit_window = 200;
    if let Some(c) = challenge {
        cfg.params.palw_funded_execution_v11.as_mut().unwrap().policies.execution.horizons.challenge_window = c;
    }
    let mut fence = PalwNativeExecutionFenceV39::unapproved_probe_at(ForkActivation::new(0), domain);
    let final_policy = cfg.params.palw_native_final_v27.as_ref().unwrap().policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&cfg.params);
    fence.policies.execution = cfg.params.palw_funded_execution_v11.as_ref().unwrap().policies.clone();
    fence.policies.final_policy = final_policy.root();
    fence.policies.physical_service =
        kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
            &mass,
            &final_policy.queue_policy,
        )
        .unwrap()
        .root();
    // The whole v39 opportunity (commit + challenge + proof/delivery tail) under the widened test horizons.
    fence.policies.maximum_horizon = 99_000;
    cfg.params.palw_native_execution_v39 = Some(fence);
    (cfg, b, p, fl)
}

mod master;

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
    let class_id: Digest = net
        .api()
        .unwrap()
        .aux
        .keys()
        .find(|(t, k)| *t == 112 && k.starts_with(&[39, 2]))
        .map(|(_, k)| k[2..66].try_into().unwrap())
        .unwrap();
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
    let quota =
        misaka_palw_kernel::flat_quota11::owning39::derive_quota_v39(&checked, &maximum, class_id, policies.root(), 2, 32).unwrap();
    let final_policy = net.config.params.palw_native_final_v27.as_ref().unwrap().policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&net.config.params);
    let service = kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
        &mass,
        &final_policy.queue_policy,
    )
    .unwrap();
    let contract = kaspa_consensus_core::palw_native_job_v39::CheckedNativeMaximumV39::derive(
        &checked_class,
        &maximum,
        &quota,
        &policies,
        &final_policy,
        &net.api().unwrap().header.policy,
        &mass,
        &service,
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
    let consents: Vec<Digest> = net
        .api()
        .unwrap()
        .aux
        .keys()
        .filter(|(t, k)| *t == 112 && k.starts_with(&[39, 18]))
        .map(|(_, k)| k[2..66].try_into().unwrap())
        .collect();
    assert_eq!(consents.len(), 1, "the producer's v39 consent landed");
    let fund = kaspa_consensus_core::palw_native_job_v39::NativeFundWireV39 { version: 39, consent: consents[0] };
    let o = stage(&mut net, 2, 44, borsh::to_vec(&fund).unwrap());
    net.send(vec![(2, o)]).await;
    let jobs = net.api().unwrap().aux.keys().filter(|(t, k)| *t == 112 && k.starts_with(&[39, 16])).count();
    assert_eq!(jobs, 1, "the v39 job is funded");
    let job: Digest = net
        .api()
        .unwrap()
        .aux
        .keys()
        .find(|(t, k)| *t == 112 && k.starts_with(&[39, 16]))
        .map(|(_, k)| k[2..66].try_into().unwrap())
        .unwrap();
    // ---- the producer commits a LYING claim (carry value 7 for 5), exactly the SDK's dishonest archive ----
    use misaka_palw_kernel::{flat_detect39::*, flat_material39::*, flat_point39::*, flat_trace39::*};
    struct NoPages;
    impl MaterialPageReaderV39 for NoPages {
        fn page(&self, _: Digest, _: u16) -> Result<MaterialPageV39, String> {
            Err("no material".into())
        }
    }
    struct NoParams;
    impl misaka_palw_tir::interp_v39::ParamSourceV39 for NoParams {
        fn descriptor(&self, _: u16, _: Option<u16>) -> Option<misaka_palw_tir::interp_v39::ParamDescriptorV39> {
            None
        }
        fn read_range(&self, _: u16, _: Option<u16>, _: u64, _: usize) -> misaka_palw_tir::TirResult<Vec<i128>> {
            Err(misaka_palw_tir::TirError::new(misaka_palw_tir::TirErrorKind::Missing, "fixture has no parameters"))
        }
    }
    let inputs = ActualInputResolverV39::derive(&checked, &maximum, &[], &NoPages).unwrap();
    let dir = std::env::temp_dir().join(format!("g14n2-own39-{}-{}", std::process::id(), net.daa()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut dishonest =
        misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39::create(&dir.join("producer"), &checked, 1 << 20).unwrap();
    dishonest.begin(checked.root(), maximum.root(), inputs.root()).unwrap();
    let a = misaka_palw_tir::Tensor::new(DType::I16, vec![], vec![5]).unwrap();
    let b7 = misaka_palw_tir::Tensor::new(DType::I16, vec![], vec![7]).unwrap();
    let (ca, cb) = (misaka_palw_kernel::merkle3::tensor_commitment_v3(&a), misaka_palw_kernel::merkle3::tensor_commitment_v3(&b7));
    let mut roots = Vec::new();
    for pos in 0..2 {
        dishonest.node(pos, 0, 0, &a, ca).unwrap();
        dishonest.node(pos, 1, 0, &b7, cb).unwrap();
        let root = misaka_palw_kernel::seg::position_root_of_v1(pos, &[vec![ca], vec![cb]]);
        dishonest.position_finished(pos, root, &b7).unwrap();
        roots.push(root);
    }
    let actual = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(&checked, 2).unwrap();
    let claim = FlatClaimV39 {
        version: 39,
        source: checked.root(),
        maximum: maximum.root(),
        generated: misaka_palw_kernel::hash::object_id(b"misaka-palw/kernel/actual-generated/v39", &inputs.generated()),
        maximum_scope: maximum.resources().scope().root(),
        actual_scope: actual.scope().root(),
        input_binding: inputs.root(),
        positions: 2,
        segment_roots: misaka_palw_kernel::seg::segment_roots_of_position_roots_v1(&roots),
    };
    dishonest.finish(&TraceSummaryV39 { claim: claim.clone(), position_roots: roots, output_roots: vec![cb] }).unwrap();
    let commit =
        kaspa_consensus_core::palw_native_job_v39::NativeCommitWireV39 { version: 39, job, generated: vec![], claim: claim.clone() };
    let o = stage(&mut net, 0, 45, borsh::to_vec(&commit).unwrap());
    net.send(vec![(0, o)]).await;
    let post =
        net.api().unwrap().aux.get(&(112, borsh::to_vec(&(39u8, 28u8, job)).unwrap())).cloned().expect("the lying claim is committed");
    // ---- an outsider's independent replay finds the fault; the court's own check accepts it against the NODE's committed post ----
    let on_chain = kaspa_consensus_core::palw_native_job_v39::NativeCommitWireV39::from_bytes(&post, 2).unwrap();
    let mut own =
        misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39::create(&dir.join("auditor"), &checked, 1 << 20).unwrap();
    let replay = execute_independent_replay_v39(&checked, &maximum, &inputs, &NoParams, &mut own).unwrap();
    let context = CheckedPointContextV39::check(&checked, &maximum, on_chain.claim.root(), &on_chain.claim, &inputs).unwrap();
    let mut producer_archive = misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39::open_existing(
        &dir.join("producer"),
        &checked,
        &maximum,
        &inputs,
        &claim,
        1 << 20,
    )
    .unwrap();
    let FindingV39::Fault { filing, conviction } = detect_first_fault_v39(&context, &quota, &replay, &mut producer_archive).unwrap()
    else {
        panic!("the lie yields a filing")
    };
    assert_eq!(
        check_flat_filing_v39(&context, &filing).unwrap(),
        conviction,
        "the v39 court check convicts on the node's committed post"
    );
    println!("owning39: lying claim committed on the node, filing {} bytes, conviction {conviction:?}", filing.len());
    let _ = std::fs::remove_dir_all(&dir);
    let replay = net.replay().await;
    net.assert_same(&replay, "owning39 source and class");
}
