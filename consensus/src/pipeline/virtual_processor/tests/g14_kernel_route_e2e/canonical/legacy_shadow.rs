//! Same work through the legacy TIR Panel and the kernel Panel=1 route.
//! Both routes retain their own identities, admission rules and clocks. This
//! ignored preflight first establishes the honest pair; attack pairs follow.
//! No fixture result is an arming record or a public-network campaign.
use super::*;
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
use misaka_palw_tir_exec::node::{TirArtifactV1, TirBackendV1};
use serde_json::json;
use std::{sync::Arc, time::Instant};

fn stage(name: &str, since: Instant, net: &Net) {
    println!("G14_LEGACY_SHADOW_PHASE {}", json!({"name":name,"wall_ns":since.elapsed().as_nanos(),"daa":net.daa()}));
}

fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, _, premine, floats) = kernel_config_onboarding_with(false);
    let mut params = config.params.clone();
    params.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(0));
    params.palw_anchor_window_v1 = Some(ForkActivation::new(0));
    params.sync_palw_g14_gates_v1();
    params.sync_palw_anchor_window_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        unreachable!()
    };
    assert_eq!(bundle.state.kernel_panel_one_external_gain_bound_v1(), Some(500 * kaspa_consensus_core::constants::SOMPI_PER_KASPA));
    assert!(params.validate_palw_v2().is_err(), "explicit isolated fixture, never production activation");
    (Config::new(params), bundle, premine, floats)
}

fn legacy_licence(net: &Net, claim: Hash64) -> Obj {
    let state = net.chain.tip_state().1;
    let panel = state.panel(&claim).expect("legacy claim bound by its ordinary anchor");
    let signed_daa = net.chain.ctx.consensus.get_virtual_daa_score();
    let receipts = panel
        .seats
        .iter()
        .map(|seat| {
            let card = net.chain.bonds.iter().position(|bond| *bond == seat.bond).expect("fixture card");
            let message = palw_receipt_message_v2(net.domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x71; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect::<Vec<_>>();
    net.chain.vp().palw_v2_receipt_quorum_assemble_impl(claim, &receipts).expect("ordinary legacy licence assembler")
}

#[tokio::test]
#[ignore = "explicit shared-input legacy/kernel measurement under buildslot"]
async fn g14_legacy_kernel_honest_shadow_measure() {
    kaspa_core::log::try_init_logger("warn");
    crate::pipeline::virtual_processor::processor::kernel_route_test_set_adjudication_cap_v1(64);
    let started = Instant::now();
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let common = onb_fixture(7);
    kernel_route_test_attest_artifact_v1(Hash64::from_bytes(common.pc.root()), 0);
    let registration = net.v2_registration(&common, 1, net.daa() + 5);
    let Obj::ClassRegisteredTirV1 { class_id: legacy_class, activation_daa, .. } = &registration else { unreachable!() };
    let (legacy_class, activation_daa) = (*legacy_class, *activation_daa);
    net.send(vec![(1, registration)]).await;
    net.beat_to(activation_daa + 1).await;
    assert!(net.chain.tip_state().1.tir_class_v1(&legacy_class).is_some());
    stage("legacy_class_registration", started, &net);

    let artifact_dir = std::env::temp_dir().join(format!("g14-legacy-shadow-{}", std::process::id()));
    std::fs::create_dir(&artifact_dir).expect("fresh process artifact directory");
    let artifact_path = artifact_dir.join("wide128-7.palwtir");
    let mut tensors =
        |j, layer| common.params.tensors.get(&(j, layer)).map(|tensor| tensor.to_le_bytes()).ok_or_else(|| "missing tensor".to_string());
    misaka_palw_tir_artifact::write_container_v1(
        &artifact_path,
        &common.program,
        borsh::to_vec(&common.class.layout).unwrap(),
        common.class.tokenizer_id.as_bytes(),
        "g14-shadow-wide128-7".into(),
        &mut tensors,
    )
    .unwrap();
    let artifact = Arc::new(TirArtifactV1::open(&artifact_path).unwrap());
    assert_eq!(artifact.inventory_root().unwrap().0, common.artifact_root);
    assert_eq!(artifact.class().unwrap(), common.class);
    let facts = PalwTirJobFactsV1::of_class(&common.class, legacy_class).unwrap();
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&common.class).unwrap());
    let backend = TirBackendV1::new(
        "g14-shadow-wide128-7".into(),
        artifact,
        common.artifact_root,
        canonical,
        net.config.params.palw_prompt_ids_form_v1(),
        net.bundle.court.max_step_leaf_count(),
    )
    .unwrap();

    let begin = Instant::now();
    let ttpb = net.ttpb();
    let execution = net.chain.real_attempt_with_backend(1, ttpb, legacy_class, &backend).await;
    stage("legacy_real_attempt", begin, &net);
    let capture = backend.decode_capture(&execution.material).unwrap();
    let legacy_admitted = net.chain.tip_state().1.claim(&execution.claim).unwrap().accepted_daa;
    let job_context = &capture.binding.job_context;
    let prompt = capture.prompt.iter().map(|token| *token as usize).collect::<Vec<_>>();
    let replay_begin = Instant::now();
    let replay = backend.execute(job_context, &prompt).unwrap();
    assert_eq!(backend.decode_capture(&replay.material).unwrap().generated, capture.generated);
    assert_eq!(
        (replay.trace_root, replay.output_root, replay.execution_root),
        (
            capture.binding.full_logits_trace_root,
            net.chain.tip_state().1.claim(&execution.claim).unwrap().output_root,
            capture.binding.committed_execution_root
        )
    );
    let legacy_replay_ns = replay_begin.elapsed().as_nanos();

    let begin = Instant::now();
    net.chain.attempt_at_the_anchor_slot(execution.claim, 7).await;
    let legacy_seats = net.chain.tip_state().1.panel(&execution.claim).expect("legacy panel").seats.len();
    let licence = legacy_licence(&net, execution.claim);
    net.send(vec![(1, licence)]).await;
    assert!(matches!(net.chain.tip_state().1.claim(&execution.claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    stage("legacy_panel_and_licence", begin, &net);

    let register = K::RegisterClass {
        descriptor: k2_tir_v2_descriptor().digest(),
        program_bytes: common.program.encode(),
        plan: common.plan.clone(),
        param_commitments: common.pc.clone(),
    };
    let object = net.route(1, &register);
    net.send(vec![(1, object)]).await;
    let kernel_class = single_class_id_v1(
        k2_tir_v2_descriptor().digest(),
        &common.program.encode(),
        &common.plan,
        &common.pc,
        VerificationModeV1::PanelLicensed,
    );
    assert!(net.ledger().classes.contains_key(&kernel_class));
    let fx = Fixture { program: common.program.clone(), params: common.params.clone(), plan: common.plan.clone(), pc: common.pc.clone() };
    let mut w = World { net, fx, class: kernel_class, jobs: 0 };
    let job = KernelJobV1 {
        class_binding_id: kernel_class,
        prompt: capture.prompt.clone(),
        max_new_tokens: capture.generated.len() as u32,
        decode: DecodeRuleV1::Greedy,
        nonce: [0x73; 64],
    };
    assert!(job.prompt.len() + capture.generated.len() - 1 <= MAX_POSITIONS as usize);
    let post = w.net.route(1, &K::PostJob { job: job.clone() });
    w.net.send(vec![(1, post)]).await;
    let begin = Instant::now();
    let ledger = w.net.ledger();
    let generated = greedy(&w.fx, &ledger, &kernel_class, &job.prompt, job.max_new_tokens as usize);
    assert_eq!(generated, capture.generated, "identical program, weights, prompt and decode produce the same output");
    let produced = produce(&w.fx, &ledger, &kernel_class, &job, w.net.kid(1), generated, |_| {});
    let claim = Claim { id: produced.claim.id(), producer: 1, trace: produced.trace, at: matmul_at(&w.fx.program, 1) };
    let (seal, reveal) = seal_and_reveal(&ledger, w.net.kid(1), &produced.object);
    let object = w.net.route(1, &seal);
    w.net.send(vec![(1, object)]).await;
    let object = w.net.route(1, &reveal);
    w.net.send(vec![(1, object)]).await;
    let kernel_admitted = w.net.ledger().claims.get(&claim.id).expect("kernel claim admitted").committed_daa;
    let committed = w.net.daa();
    seed::until_seated(&mut w.net, &claim.id, committed).await;
    assert_eq!(w.seats(&claim.id).len(), 1);
    w.cover(&claim.id).await;
    stage("kernel_same_work_admission_and_receipt", begin, &w.net);
    let floor = final_floor(&w, Mode::Panel, &claim.id);
    w.net.beat_to(floor).await;
    let ClaimStateV1::Final { final_daa: kernel_final } = w.net.claim_state(&claim.id) else { panic!("kernel honest claim not Final") };
    let mut legacy_final = None;
    for _ in 0..10_000 {
        let state = w.net.chain.tip_state().1;
        let row = state.claim(&execution.claim).expect("legacy claim retained");
        if let PalwClaimPhaseV2::Final { final_daa } = row.phase {
            legacy_final = Some(final_daa);
            break;
        }
        let step = w.net.ttpb();
        w.net.chain.heartbeat(step, Vec::new()).await;
    }
    assert!(legacy_final.is_some(), "legacy claim must reach Final under its own unchanged rule");
    println!(
        "G14_LEGACY_SHADOW {}",
        json!({"schema":1,"case":"honest","fixture_only":true,"arming_evidence":false,
        "scope":"legacy_panel_and_kernel_panel1_same_work","program_root":program_root_v1(&common.program.encode()).to_vec(),
        "legacy_class":legacy_class.to_string(),"kernel_class":kernel_class.to_vec(),"legacy_claim":execution.claim.to_string(),"kernel_claim":claim.id.to_vec(),
        "route_claim_ids_equal":false,"same_program_weights_prompt_output":true,"prompt":capture.prompt,"generated":capture.generated,
        "legacy_seats":legacy_seats,"kernel_seats":1,"legacy_admitted":legacy_admitted,"kernel_admitted":kernel_admitted,
        "legacy_final_daa":legacy_final,"kernel_final_daa":kernel_final,"legacy_replay_wall_ns":legacy_replay_ns,
        "legacy_capture_bytes":execution.material.len(),"gate_c_measured":false,"attack_cases_measured":false,"per_seat_honest_verification_cost_measured":false,"total_wall_ns":started.elapsed().as_nanos()})
    );
    drop(backend);
    std::fs::remove_file(&artifact_path).unwrap();
    std::fs::remove_dir(&artifact_dir).unwrap();
}
