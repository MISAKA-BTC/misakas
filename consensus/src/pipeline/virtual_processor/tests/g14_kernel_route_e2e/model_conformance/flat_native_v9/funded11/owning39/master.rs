//! **G-N2 on the node (2026-10-18)**: the owning39 fixture through a real complete Master (65), its one component job (consent 43
//! under the Master's composed maximum, Fund 44 recorded as the Master's stage 0, a commit 45), the job's unaccused Final (51) and
//! the Master's Final (66); then the mining line (73 register, 74 seat, 75 commit) and the post-Final accusation (80) of a forged
//! mining claim beside an honest one. Structural test values (a shortened challenge window so a job reaches Final unaccused, a
//! mining policy with test capacities), never measurements.
use super::*;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::palw_native_job_v39::*;
use kaspa_consensus_core::palw_native_master_v39::NativeMasterRegisterV39;
use kaspa_consensus_core::palw_native_mining_v39::*;
use misaka_palw_kernel::flat_component_source39::*;
use misaka_palw_kernel::flat_invocation39::*;
use misaka_palw_kernel::flat_job39::ActualInputResolverV39;
use misaka_palw_kernel::flat_pipeline39::{CheckedPipelineV39, PipelineV39};
use misaka_palw_kernel::flat_point39::FlatClaimV39;
use misaka_palw_kernel::flat_source39::CheckedSourceV39;
use misaka_palw_kernel::hash::object_id;
use misaka_palw_tir::pipeline::{StageDecl, TripRule};

pub(super) const CHALLENGE: u64 = 300;

mod node_e2e;

/// **A stage action, rehearsed then sent**: the object is first folded on the node's tip with the processor's own block fold (so a
/// refusal names its reason instead of silently changing nothing), then carried and folded by the node.
async fn act(net: &mut Net, card: usize, action: u8, body: Vec<u8>) {
    let (_, tip) = net.chain.tip_state();
    let params = net.chain.vp().palw_state_params_v2.clone().expect("testnet-12 is ConsensusV2");
    let probe = net.route(card, &K::StageExecutionV14 { action, body: body.clone() });
    let last = tip.last_point().expect("the node's tip folded a block");
    let ctx = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: Hash64::from_u64_word(0x6E_3900 + net.daa()),
        daa_score: last.daa_score + 2,
        blue_score: last.blue_score + 2,
        subsidy: 0,
    };
    if let Err(why) = net.chain.vp().palw_v2_fold_accepted_for_tests(&tip, &params, &ctx, &[probe]) {
        panic!("stage action {action} by card {card} is refused by the fold: {why:?}");
    }
    let o = stage(net, card, action, body);
    net.send(vec![(card, o)]).await;
}

fn row(net: &Net, kind: u8, id: &impl borsh::BorshSerialize) -> Option<Vec<u8>> {
    net.api().unwrap().aux.get(&(112, borsh::to_vec(&(39u8, kind, id)).unwrap())).cloned()
}
fn ids(net: &Net, kind: u8) -> std::collections::BTreeSet<Digest> {
    net.api().unwrap().aux.keys().filter(|(t, k)| *t == 112 && k.len() == 66 && k.starts_with(&[39, kind])).map(|(_, k)| k[2..66].try_into().unwrap()).collect()
}
fn new_id(net: &Net, kind: u8, before: &std::collections::BTreeSet<Digest>) -> Digest {
    let now = ids(net, kind);
    let fresh: Vec<_> = now.difference(before).copied().collect();
    assert_eq!(fresh.len(), 1, "exactly one new row of kind {kind}: {}", fresh.len());
    fresh[0]
}

/// The registered source, its full-context class and the one-stage complete source every Master here uses.
pub(super) struct Fixture {
    pub checked: CheckedSourceV39,
    pub class: NativeClassWireV39,
    pub class_id: Digest,
    pub pipeline: CheckedPipelineV39,
    pub manifest_bytes: Vec<u8>,
    pub pipeline_bytes: Vec<u8>,
    pub complete_source: Digest,
}

pub(super) async fn fixture(net: &mut Net) -> Fixture {
    let wire = source_wire_at(2);
    let checked = wire.clone().check().expect("the fixture source checks");
    act(net, 0, REGISTER_SOURCE_ACTION_V39, borsh::to_vec(&wire).unwrap()).await;
    let policies = net.config.params.palw_native_execution_v39.as_ref().unwrap().policies.clone();
    let derived = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(&checked, 2).unwrap();
    let class = NativeClassWireV39 {
        version: 39,
        source: checked.root(),
        policy: policies.root(),
        max_positions: 2,
        scope: derived.scope().root(),
        resources: derived.root(),
    };
    act(net, 0, REGISTER_CLASS_ACTION_V39, borsh::to_vec(&class).unwrap()).await;
    let class_id = class.check(&checked, &policies).unwrap().root();
    assert!(row(net, 2, &class_id).is_some(), "the full-context class is registered");
    let program = checked.program().clone();
    let pipeline_wire = PipelineV39 {
        version: 39,
        stages: vec![StageDecl { name: "only".into(), program: 0, trip: TripRule::Fixed { n: 2 }, max_trip: 2, tokens: None, bind: vec![] }],
        output_stage: 0,
    };
    let pipeline = pipeline_wire.check(std::slice::from_ref(&program)).expect("the one-stage pipeline checks");
    let manifest = CompleteComponentManifestV39 {
        version: 39,
        repository: "fixture/owning39".into(),
        revision: [1; 20],
        original_task_metadata: [9; 64],
        original_pipeline: pipeline.root(),
        original_required_components: vec!["model".into()],
        source_files: vec![OriginalFilePinV39 { path: "model.bin".into(), bytes: 1, sha256: [1; 32] }],
        components: vec![SourceComponentV39 { name: "model".into(), source_files: vec![0], stages: vec![0] }],
        stages: vec![ComponentStageV39 {
            original_source: checked.root(),
            original_profile: checked.profile_root(),
            original_parameter: checked.parameters().root(),
            original_program: stage_program_root_v39(&program),
            original_inventory: checked.inventory_root(),
            input_declarations: stage_input_declarations_root_v39(&program),
            output_declaration: stage_output_declaration_root_v39(&program),
            original_context_max: 2,
            lowered_context_max: 2,
        }],
        inputs: vec![],
    };
    let manifest_bytes = manifest.to_bytes().unwrap();
    let pipeline_bytes = pipeline_wire.to_bytes().unwrap();
    let encoded =
        2u64 + 4 + manifest_bytes.len() as u64 + 4 + pipeline_bytes.len() as u64 + 4 + 4 + checked.wire().to_bytes().len() as u64;
    let complete_source = object_id(
        b"misaka-palw/kernel/complete-original-source-bundle/v39",
        &(39u16, manifest.root(), pipeline.root(), vec![checked.root()], encoded),
    );
    Fixture { checked, class, class_id, pipeline, manifest_bytes, pipeline_bytes, complete_source }
}

/// One complete Master (owner card 2, its one stage produced by `producer`) with its funded component job and the producer's
/// commit of the value `value` (5 is the honest computation, anything else a lie).
pub(super) struct Component {
    pub master: Digest,
    pub job: Digest,
    pub maximum: CheckedJobMaximumV39,
    pub quota: misaka_palw_kernel::flat_quota11::owning39::CheckedQuotaV39,
    pub claim: FlatClaimV39,
    pub dir: std::path::PathBuf,
    pub committed: u64,
    pub temporal: NativeTemporalV39,
}

pub(super) async fn master_component(net: &mut Net, fx: &Fixture, producer: usize, sequence: u64, value: i128) -> Component {
    let policies = net.config.params.palw_native_execution_v39.as_ref().unwrap().policies.clone();
    let tip = net.chain.tip_state().1;
    let ops: Vec<Digest> = (0..4).map(|c| tip.bond(&net.bond(c)).unwrap().operator_id.as_bytes()).collect();
    let op = |c: usize| ops[c];
    let first = net.api().unwrap().execution_nonce_counter_v11(&op(2)).unwrap().next_nonce;
    let request = MasterPublicRequestV39 {
        version: 39,
        complete_source: fx.complete_source,
        requester: net.kid(2),
        requester_operator: op(2),
        first_nonce: nonce_bytes_v11(first),
        producers: vec![MasterProducerV39 { producer: net.kid(producer), operator: op(producer), reward: 20 }],
        prompt: vec![],
        negative: vec![],
        source: vec![],
        key: vec![],
        steps: 0,
        scalars: vec![],
        images: vec![],
        seed: [6; 32],
        requested_new_tokens: 0,
        eos: vec![],
        finalized: vec![],
    };
    request.check(&fx.pipeline).expect("the public request checks");
    let register = NativeMasterRegisterV39 {
        version: 39,
        sequence,
        deadline: net.daa() + 90_000,
        classes: vec![fx.class_id],
        producer_bonds: vec![net.bond(producer)],
        manifest: fx.manifest_bytes.clone(),
        pipeline: fx.pipeline_bytes.clone(),
        request: request.to_bytes(),
    };
    let master = register.root(net.bond(2));
    act(net, 2, 65, borsh::to_vec(&register).unwrap()).await;
    assert!(row(net, 80, &master).is_some(), "the complete Master is registered (65)");
    let composed = compose_component_request_v39(master, &request, &fx.pipeline, &fx.checked, 0, &[]).unwrap();
    let (job_max, material) = composed.finish(&[]).unwrap();
    let maximum = CheckedJobMaximumV39::check(&fx.checked, &job_max, &material).unwrap();
    let quota =
        misaka_palw_kernel::flat_quota11::owning39::derive_quota_v39(&fx.checked, &maximum, fx.class_id, policies.root(), 2, 32).unwrap();
    let final_policy = net.config.params.palw_native_final_v27.as_ref().unwrap().policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&net.config.params);
    let service =
        kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(&mass, &final_policy.queue_policy).unwrap();
    let checked_class = fx.class.check(&fx.checked, &policies).unwrap();
    let contract = CheckedNativeMaximumV39::derive(
        &checked_class, &maximum, &quota, &policies, &final_policy, &net.api().unwrap().header.policy, &mass, &service,
    )
    .unwrap_or_else(|why| panic!("the component's maximum contract: {why}"));
    let temporal = contract.record().temporal.clone();
    let interval = policies.execution.quota.interval_daa;
    let now = net.daa() + 1;
    let consent = NativeConsentWireV39 {
        version: 39,
        class: fx.class_id,
        requester: net.bond(2),
        requester_operator: op(2),
        producer_operator: op(producer),
        reward: 20,
        expires: now + temporal.commit_window.min(interval - now % interval - 1),
        maximum_contract: contract.record().root(),
        maximum: borsh::to_vec(&job_max).unwrap(),
        material: borsh::to_vec(&material).unwrap(),
    };
    let before = ids(net, 18);
    act(net, producer, 43, borsh::to_vec(&consent).unwrap()).await;
    let consent_id = new_id(net, 18, &before);
    let before = ids(net, 16);
    act(net, 2, 44, borsh::to_vec(&NativeFundWireV39 { version: 39, consent: consent_id }).unwrap()).await;
    let job = new_id(net, 16, &before);
    assert!(row(net, 84, &job).is_some(), "the Fund is recorded as the Master's stage 0");
    let (claim, dir) = commit_value(net, &fx.checked, &maximum, job, producer, value).await;
    Component { master, job, maximum, quota, claim, dir, committed: net.daa(), temporal }
}

/// Nobody accuses within the window: an independent Final (51, card 1), then the Master's Final (66, its owner).
pub(super) async fn finalize(net: &mut Net, c: &Component) {
    let at = c.committed + c.temporal.challenge_window + c.temporal.final_tail + 4;
    net.beat_to(at).await;
    let mut body = 39u16.to_le_bytes().to_vec();
    body.push(0);
    body.extend_from_slice(&c.job);
    act(net, 1, 51, body).await;
    assert!(row(net, 23, &c.job).is_some(), "the component job reached Final unaccused (its single credit is recorded)");
    act(net, 2, 66, c.master.to_vec()).await;
    assert!(row(net, 83, &c.master).is_some(), "the Master is Final (66)");
}

/// The producer's commit (45) of the fixture job with carry value `value`, archived exactly as the SDK would; returns the claim.
pub(super) async fn commit_value(
    net: &mut Net,
    checked: &CheckedSourceV39,
    maximum: &CheckedJobMaximumV39,
    job: Digest,
    producer: usize,
    value: i128,
) -> (FlatClaimV39, std::path::PathBuf) {
    use misaka_palw_kernel::{flat_detect39::*, flat_material39::*, flat_trace39::*};
    struct NoPages;
    impl MaterialPageReaderV39 for NoPages {
        fn page(&self, _: Digest, _: u16) -> Result<MaterialPageV39, String> {
            Err("no material".into())
        }
    }
    let inputs = ActualInputResolverV39::derive(checked, maximum, &[], &NoPages).unwrap();
    static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let run = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("g14n2-master39-{}-{}-{}-{run}", std::process::id(), net.daa(), producer));
    std::fs::create_dir_all(&dir).unwrap();
    let mut archive = misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39::create(&dir.join("producer"), checked, 1 << 20).unwrap();
    archive.begin(checked.root(), maximum.root(), inputs.root()).unwrap();
    let a = misaka_palw_tir::Tensor::new(DType::I16, vec![], vec![5]).unwrap();
    let b = misaka_palw_tir::Tensor::new(DType::I16, vec![], vec![value]).unwrap();
    let (ca, cb) = (misaka_palw_kernel::merkle3::tensor_commitment_v3(&a), misaka_palw_kernel::merkle3::tensor_commitment_v3(&b));
    let mut roots = Vec::new();
    for pos in 0..2 {
        archive.node(pos, 0, 0, &a, ca).unwrap();
        archive.node(pos, 1, 0, &b, cb).unwrap();
        let root = misaka_palw_kernel::seg::position_root_of_v1(pos, &[vec![ca], vec![cb]]);
        archive.position_finished(pos, root, &b).unwrap();
        roots.push(root);
    }
    let actual = misaka_palw_kernel::flat_resources39::derive_flat_resources_v39(checked, 2).unwrap();
    let claim = FlatClaimV39 {
        version: 39,
        source: checked.root(),
        maximum: maximum.root(),
        generated: object_id(b"misaka-palw/kernel/actual-generated/v39", &inputs.generated()),
        maximum_scope: maximum.resources().scope().root(),
        actual_scope: actual.scope().root(),
        input_binding: inputs.root(),
        positions: 2,
        segment_roots: misaka_palw_kernel::seg::segment_roots_of_position_roots_v1(&roots),
    };
    archive.finish(&TraceSummaryV39 { claim: claim.clone(), position_roots: roots, output_roots: vec![cb] }).unwrap();
    let commit = NativeCommitWireV39 { version: 39, job, generated: vec![], claim: claim.clone() };
    act(net, producer, 45, borsh::to_vec(&commit).unwrap()).await;
    assert!(row(net, 28, &job).is_some(), "the claim is committed");
    (claim, dir)
}

/// An outsider's independent replay against the producer's archive: the filing, if the commit lied.
fn outsider_filing(fx: &Fixture, c: &Component, net: &Net) -> Option<Vec<u8>> {
    use misaka_palw_kernel::{flat_detect39::*, flat_material39::*, flat_point39::*};
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
    let inputs = ActualInputResolverV39::derive(&fx.checked, &c.maximum, &[], &NoPages).unwrap();
    let post = row(net, 28, &c.job).unwrap();
    let on_chain = NativeCommitWireV39::from_bytes(&post, 2).unwrap();
    let mut own = misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39::create(&c.dir.join("auditor"), &fx.checked, 1 << 20).unwrap();
    let replay = execute_independent_replay_v39(&fx.checked, &c.maximum, &inputs, &NoParams, &mut own).unwrap();
    let context = CheckedPointContextV39::check(&fx.checked, &c.maximum, on_chain.claim.root(), &on_chain.claim, &inputs).unwrap();
    let mut producer = misaka_palw_sdk::native_trace_archive_v39::DiskTraceArchiveV39::open_existing(
        &c.dir.join("producer"),
        &fx.checked,
        &c.maximum,
        &inputs,
        &c.claim,
        1 << 20,
    )
    .unwrap();
    match detect_first_fault_v39(&context, &c.quota, &replay, &mut producer).unwrap() {
        FindingV39::Fault { filing, conviction } => {
            assert_eq!(check_flat_filing_v39(&context, &filing).unwrap(), conviction, "the court check convicts on the node's post");
            Some(filing)
        }
        _ => None,
    }
}

#[tokio::test]
async fn owning39_master_component_job_reaches_final_and_the_master_is_final() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(owning_config_with(Some(CHALLENGE)), TestConsensus::new);
    net.beat_to(5).await;
    let fx = fixture(&mut net).await;
    let c = master_component(&mut net, &fx, 0, 0, 7).await;
    println!("owning39/master: commit at {}, temporal {:?}", c.committed, c.temporal);
    finalize(&mut net, &c).await;
    let replay = net.replay().await;
    net.assert_same(&replay, "owning39 Master Final");
    let _ = std::fs::remove_dir_all(&c.dir);
}

/// `owning_config_with(CHALLENGE)` plus the owning39 mining fence (TEST capacities and premise roots, never measured) and RFC-0018
/// Gate C (its design certification policy, unchanged), mirrored onto the bundle the chain is built from.
fn mining_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    mining_config_with(100, false)
}

/// **TEST ONLY**: [`mining_config`] whose budget is a V3 policy with bond formula v2, so a native39 receipt's liability is priced by
/// the formula (not the bare pwu exposure) and its export cap can pay a block. The policy is the lane-VAL pinned V3 estimate; only the
/// harness bonds are sized to it (collateral moved from the main premine, nothing minted). Production values and the gate are unchanged.
pub(super) fn mining_config_v3_budget() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    mining_config_with(100, true)
}

fn mining_config_with(collateral_multiplier: u64, v3_budget: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (cfg, mut bundle, mut premine, floats) = owning_config_with(Some(CHALLENGE));
    let mut params = cfg.params.clone();
    // HARNESS ONLY: fund the design m=47 draws and the anchor producer with real
    // collateral. Move it from the main premine; neither caps nor k/m are relaxed.
    let total_before: u128 = premine.iter().map(|(_, e)| u128::from(e.amount)).sum();
    let mut added = 0u64;
    for object in &mut bundle.genesis_objects {
        if let Obj::BondRegistered { bond, collateral, .. } = object {
            let entry = premine.iter_mut().find(|(outpoint, _)| *outpoint == bond.0).expect("the bond has funded collateral");
            assert_eq!(entry.1.amount, *collateral);
            let enlarged = collateral.checked_mul(collateral_multiplier).expect("test collateral fits");
            added = added.checked_add(enlarged - *collateral).unwrap();
            *collateral = enlarged;
            entry.1.amount = enlarged;
        }
    }
    let donor = premine.iter_mut().filter(|(outpoint, _)| !floats.iter().any(|(f, _)| f == outpoint))
        .max_by_key(|(_, e)| e.amount).expect("the main premine");
    donor.1.amount = donor.1.amount.checked_sub(added).expect("the main premine funds the extra bonds");
    assert_eq!(premine.iter().map(|(_, e)| u128::from(e.amount)).sum::<u128>(), total_before, "no money is minted for the harness");
    let mut multiset = kaspa_muhash::MuHash::new();
    for (outpoint, entry) in &premine {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    params.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle.clone());
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(), Some(params.genesis.hash),
    ).as_bytes();
    // The fixture's funded genesis defines a new domain. Rebind the whole native
    // policy graph, including roots that authenticate subordinate policies.
    let funded = &mut params.palw_funded_execution_v11.as_mut().unwrap().policies;
    funded.quota.network_domain = domain;
    funded.execution.network_domain = domain;
    funded.execution.quota_policy_root = funded.quota.root();
    funded.discovery.quota_policy_root = funded.quota.root();
    let execution = funded.clone();
    let final_policy = &mut params.palw_native_final_v27.as_mut().unwrap().policy;
    final_policy.network_domain = domain;
    final_policy.queue_policy.network_domain = domain;
    let final_policy = final_policy.clone();
    let mass = kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(&params);
    let service = kaspa_consensus_core::palw_flat_execution_capacity_v22::NativeCarriageServiceV22::reserved_scheduler_v30(
        &mass, &final_policy.queue_policy,
    ).unwrap();
    let native = &mut params.palw_native_execution_v39.as_mut().unwrap().policies;
    native.network_domain = domain;
    native.execution = execution;
    native.final_policy = final_policy.root();
    native.physical_service = service.root();
    native.check(domain).expect("the funded fixture has one consistent native policy domain");
    let mut fence = PalwNativeMiningFenceV39::unapproved_probe_at(ForkActivation::new(0), domain);
    let p = &mut fence.policy;
    // The premise must cover the design threshold k/m = 32/47 (the probe's 2/3 does not: 2·47 < 32·3): a TEST premise.
    p.honest_stake_numerator = 7;
    p.honest_stake_denominator = 10;
    p.assumption_review = [1; 64];
    p.qualification = [2; 64];
    // One MAC a quantum (the fixture's work is a handful of MACs; the probe's 2^30 makes every fixture Master sub-quantum).
    p.network_quantum_mac = 1;
    p.network_slash_per_mac = 1_000;
    p.claim_window_daa = 10_000;
    p.receipt_lifetime_daa = 100;
    p.maximum_claims_per_model_epoch = 4;
    p.maximum_live_receipts_per_model = 4;
    p.maximum_quanta_per_receipt = 4;
    params.palw_native_mining_v39 = Some(fence);
    params.palw_model_eligibility_v1 = Some(ForkActivation::new(0));
    params.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(0));
    params.palw_independently_work_certified_v1 = Some(ForkActivation::new(0));
    if v3_budget {
        let fence = params.palw_bond_budget_v1.as_mut().expect("the owning39 world arms the budget");
        // TEST-ONLY policy: the pinned V3 estimate with D2 rights slicing by rho (formula v2 refuses a budget without it). The
        // production estimate itself is untouched.
        let mut policy = kaspa_consensus_core::palw_values_p4_v1::palw_bond_budget_policy_provisional_v1();
        policy.slice_rights_by_rho = true;
        fence.policy = policy;
        params.palw_bond_formula_v2 =
            Some(kaspa_consensus_core::palw_bond_formula_v2::PalwBondFormulaFenceV2::at(ForkActivation::new(0)));
        params.sync_palw_bond_budget_v1();
    }
    params.sync_palw_rfc0018_v1();
    let mut tmp = params.clone();
    tmp.palw_consensus_mode = kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle);
    tmp.sync_palw_rfc0018_v1();
    tmp.sync_palw_bond_budget_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = tmp.palw_consensus_mode else { unreachable!() };
    assert!(bundle.state.rfc0018_v1().is_some_and(|m| m.gate_c_at(0)), "Gate C is mirrored");
    (Config::new(params), bundle, premine, floats)
}

fn mining_claim(net: &Net, id: &Digest) -> NativeMiningClaimRecordV39 {
    let raw = row(net, MINING_CLAIM_KIND_V39, id).expect("the mining claim row");
    borsh::from_slice::<(Digest, NativeMiningClaimRecordV39)>(&raw).unwrap().1
}

/// The certification row of a mining claim (aux 136, the native `'V'` key).
fn cert_row(net: &Net, c: &NativeMiningClaimRecordV39) -> Option<kaspa_consensus_core::palw_security_certification_v1::PalwClaimCertificationRowV1> {
    use kaspa_consensus_core::palw_security_certification_v1::*;
    let key = certification_native_key_v39(&palw_native_certification_id_v39(&c.claim_hash()));
    net.api().unwrap().aux.get(&(PALW_SECURITY_CERTIFICATION_TABLE_V1, key)).map(|raw| borsh::from_slice(raw).unwrap())
}

/// Every drawn bond (by card) attests `Reproduced` (inner 75) on the claim's certification — colluding attestors on a forged claim.
async fn attest_all(net: &mut Net, c: &NativeMiningClaimRecordV39, except: &[usize]) -> Vec<usize> {
    use misaka_palw_kernel::model_eligibility_v1::*;
    let row = cert_row(net, c).expect("the certification row");
    let id = kaspa_consensus_core::palw_security_certification_v1::palw_native_certification_id_v39(&c.claim_hash());
    let mut cards = Vec::new();
    for card in 0..8usize {
        if except.contains(&card) || !row.draws.contains(&net.kid(card)) {
            continue;
        }
        let attestation = EligibilityAttestationV1 {
            version: ELIGIBILITY_ATTESTATION_VERSION_V1,
            network: net.domain.as_bytes(),
            exam: id,
            verifier: net.kid(card),
            verdict: EligibilityVerdictV1::Reproduced,
            transcript: [0xA0 | card as u8; 64],
            claim_mismatches: vec![],
        };
        if cards.is_empty() {
            let mut unknown = attestation.clone();
            unknown.exam = [0xFD; 64];
            let object = net.route(card, &K::AttestEligibilityExamV1 { attestation: unknown });
            let funding = net.funding[card].clone();
            let tx = net.carrier(card, &object);
            let error = net.mempool(&tx).expect_err("unknown ids cannot reopen the disabled model-exam route");
            net.funding[card] = funding;
            assert!(error.to_string().contains("eligibility exam object is refused"), "unexpected refusal: {error}");
        }
        let o = net.route(card, &K::AttestEligibilityExamV1 { attestation });
        net.send(vec![(card, o)]).await;
        cards.push(card);
    }
    cards
}

/// **G-N2 end to end on the node**: two complete Masters of the same model line — stage 0 forged by card 0 (carry 7 for 5), stage 0
/// computed honestly by card 3 — both reach job Final unaccused and Master Final; the model is registered (73), both producers take a
/// seat (74) and commit a mining claim (75) past the design verifier maturity (7 days); seed anchors from attempt blocks draw Gate C
/// (design m = 47, k = 32, with replacement over the matured pool); every drawn bond attests `Reproduced` (colluding on the forged
/// one), both certify after the design prosecution window and get receipts (77). An outsider (card 1) replays independently and files
/// (80, kind 0) against the forged claim: its producer's reserved exposure is collected, its receipt revoked, its certification
/// withdrawn, its attestors' locks charged, its hold released. The same filing against the honest claim changes nothing.
#[tokio::test]
async fn g14n2_node_a_forged_mining_claim_is_convicted_after_its_master_final_and_the_honest_one_is_untouched() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(mining_config(), TestConsensus::new);
    net.beat_to(5).await;
    let fx = fixture(&mut net).await;
    let forged = master_component(&mut net, &fx, 0, 0, 7).await;
    let honest = master_component(&mut net, &fx, 3, 1, 5).await;
    assert!(outsider_filing(&fx, &honest, &net).is_none(), "the honest commit yields no filing");
    finalize(&mut net, &forged).await;
    finalize(&mut net, &honest).await;
    // ---- the model line (73, registrant card 1) and the producers' seats (74) ----
    let wire = NativeMiningModelWireV39 { version: 39, template_master: forged.master, sequence: 0, share_permille: 0 };
    let before = ids(&net, MINING_MODEL_KIND_V39);
    act(&mut net, 1, REGISTER_MODEL_ACTION_V39, borsh::to_vec(&wire).unwrap()).await;
    let model = new_id(&net, MINING_MODEL_KIND_V39, &before);
    for card in [0usize, 3] {
        act(&mut net, card, MODEL_SEAT_ACTION_V39, borsh::to_vec(&NativeMiningSeatWireV39 { version: 39, model, kind: 0 }).unwrap()).await;
        assert!(row(&net, MINING_SEAT_KIND_V39, &(model, net.bond(card))).is_some(), "card {card} holds a possession seat");
    }
    // ---- past the design verifier maturity (the genesis bonds become the Gate C pool), the mining claims (75) ----
    let cert = net.chain.vp().palw_state_params_v2.clone().unwrap().rfc0018_v1().unwrap().certification;
    net.beat_to(cert.verifier_maturity_daa + 2).await;
    let mut claims = Vec::new();
    for (card, c) in [(0usize, &forged), (3, &honest)] {
        let before = ids(&net, MINING_CLAIM_KIND_V39);
        let w = NativeMiningCommitWireV39 { version: 39, model, master: c.master, sequence: 0 };
        act(&mut net, card, MINING_COMMIT_ACTION_V39, borsh::to_vec(&w).unwrap()).await;
        claims.push(new_id(&net, MINING_CLAIM_KIND_V39, &before));
        assert!(row(&net, MINING_JOB_HOLD_KIND_V39, &c.job).is_some(), "the credited component's material is held");
    }
    let (bad_claim, good_claim) = (mining_claim(&net, &claims[0]), mining_claim(&net, &claims[1]));
    // ---- seed anchors (attempt blocks) until both certifications are drawn ----
    let ttpb = net.ttpb();
    for round in 0..64 {
        if cert_row(&net, &bad_claim).is_some_and(|r| r.anchor.is_some()) && cert_row(&net, &good_claim).is_some_and(|r| r.anchor.is_some()) {
            break;
        }
        {
            // Rotate across genesis bonds: the first card's unsettled pre-Final weight can hit its
            // Final-weight cap before it can seed the Gate C anchor. The cap is a frozen rule;
            // let another eligible bond provide the attempt instead of weakening the budget.
            let attempt_card = 5 + (round % 3);
            let (block, _) = net.chain.build_attempt(attempt_card, ttpb, Vec::new(), &|_| true);
            let envelope = kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2::decode_wire(&block.header.palw_commitment).unwrap();
            let (_, tip) = net.chain.tip_state();
            let params = net.chain.vp().palw_state_params_v2.clone().unwrap();
            let last = tip.last_point().unwrap();
            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                block: block.header.hash,
                daa_score: block.header.daa_score,
                blue_score: last.blue_score + 1,
                subsidy: 0,
            };
            match net.chain.vp().palw_v2_fold_attempt_for_tests(&tip, &params, &point, &[], &envelope, &block.header) {
                Err(why) => println!("g14n2/node: the anchor attempt is refused by the fold: {why:?}"),
                Ok((_, _, skipped)) => println!("g14n2/node: the anchor attempt folds; skipped {skipped:?}"),
            }
        }
        let attempt_card = 5 + (round % 3);
        let (attempt_block, attempt_claim) = net.chain.attempt(attempt_card, ttpb, Vec::new(), &|_| true).await;
        let tip = net.chain.tip_state().1;
        println!(
            "g14n2/node: attempt at daa {} (sink is it: {}) admitted {} anchor {:?}",
            net.daa(),
            net.chain.sink() == attempt_block.header.hash,
            tip.claim(&attempt_claim).is_some(),
            tip.round_seed_anchor().map(|a| a.span)
        );
        let to = net.daa() + 8;
        net.beat_to(to).await;
    }
    let drawn = cert_row(&net, &bad_claim).unwrap();
    println!(
        "g14n2/node: tip daa {}, seed anchor {:?}, committed_span {}, drawn {:?}",
        net.daa(),
        net.chain.tip_state().1.round_seed_anchor().map(|a| a.span),
        drawn.committed_span,
        drawn.anchor
    );
    assert!(drawn.anchor.is_some() && !drawn.draws.is_empty(), "the forged claim is drawn (outcome {:?})", drawn.outcome);
    assert_eq!(drawn.draws.len(), cert.m as usize, "m = 47 positions");
    // The honest producer (card 3) never vouches for the forged claim.
    let colluders = attest_all(&mut net, &bad_claim, &[1, 3]).await;
    attest_all(&mut net, &good_claim, &[]).await;
    println!("g14n2/node: forged claim drawn over {} distinct bonds, attested by cards {colluders:?}", colluders.len());
    for (label, claim) in [("forged", &bad_claim), ("honest", &good_claim)] {
        let row = cert_row(&net, claim).expect("the drawn certification row");
        let per_card: Vec<(usize, usize)> = (0..8usize)
            .map(|card| (card, row.draws.iter().filter(|draw| **draw == net.kid(card)).count()))
            .filter(|(_, count)| *count > 0)
            .collect();
        println!(
            "g14n2/node: {label} draw histogram {per_card:?}; positive positions {}/{} (k={})",
            row.positive_positions(),
            row.draws.len(),
            cert.k
        );
        assert!(
            row.positive_positions() >= cert.k as u32,
            "the selected deterministic draw does not meet Gate C before its prosecution window: {label}, positive {}/{}, k={}, histogram {per_card:?}",
            row.positive_positions(),
            row.draws.len(),
            cert.k
        );
    }
    // ---- the prosecution window, then both certify and receive their receipts (77) ----
    // The verdict reads the CERTIFICATION ROW's final_daa (the DAA its row recorded the Final, which can trail the mining claim's
    // own Final by a few DAA), so wait past the later of the two rows' windows — the window itself is not shortened.
    let row_final = [&bad_claim, &good_claim].iter().map(|c| cert_row(&net, c).unwrap().final_daa).max().unwrap();
    let until = drawn.final_daa.max(row_final) + cert.prosecution_window_daa + 2;
    eprintln!("g14n2/node: prosecution wait from DAA {} to {until} (Final {}, window {})", net.daa(), drawn.final_daa, cert.prosecution_window_daa);
    net.beat_to(until).await;
    for c in [&bad_claim, &good_claim] {
        let r = cert_row(&net, c).unwrap();
        assert!(r.outcome.is_some_and(|(o, _)| o.certifies()), "certified by the colluding draws: {:?}", r.outcome);
        let subject = NativeMiningSubjectWireV39 { version: 39, kind: 0, id: c.id };
        act(&mut net, 1, ISSUE_RECEIPT_ACTION_V39, borsh::to_vec(&subject).unwrap()).await;
        assert!(row(&net, MINING_RECEIPT_KIND_V39, &native_mining_receipt_id_v39(c.id)).is_some(), "the receipt is issued");
    }
    let locks = |net: &Net, card: usize| net.api().unwrap().signer_locks_of_v1(&net.bond(card));
    let locked: Vec<(usize, u64)> = colluders.iter().map(|card| (*card, locks(&net, *card))).collect();
    assert!(locked.iter().all(|(_, l)| *l > 0), "every Reproduced attestor holds a lock: {locked:?}");
    let safe_before = net.chain.tip_state().1.safe_weight();
    // ---- the outsider's filing: on the honest claim nothing; on the forged one the conviction ----
    let filing = outsider_filing(&fx, &forged, &net).expect("the forged commit yields a filing");
    let (c0, c3) = (net.collateral(0), net.collateral(3));
    let accuse = |claim: Digest| NativeMiningAccusationWireV39 { version: 39, kind: 0, claim, stage: 0, body: filing.clone() };
    act(&mut net, 1, ACCUSE_MINING_ACTION_V39, borsh::to_vec(&accuse(good_claim.id)).unwrap()).await;
    assert_eq!(net.collateral(3), c3, "a filing that does not verify on the honest component never touches its producer");
    assert!(row(&net, MINING_CONVICTED_KIND_V39, &good_claim.id).is_none());
    let attestor_collateral: Vec<(usize, u64)> = colluders.iter().map(|card| (*card, net.collateral(*card))).collect();
    act(&mut net, 1, ACCUSE_MINING_ACTION_V39, borsh::to_vec(&accuse(bad_claim.id)).unwrap()).await;
    assert!(row(&net, MINING_CONVICTED_KIND_V39, &bad_claim.id).is_some(), "the forged mining claim is convicted after its Master Final");
    let collected = c0 - net.collateral(0);
    assert_eq!(collected, u64::try_from(bad_claim.price.reserved_exposure).unwrap().min(c0), "the reserved exposure is collected");
    assert_eq!(mining_claim(&net, &bad_claim.id).closed.map(|(_, issued)| issued), Some(false), "closed without reward");
    assert!(row(&net, MINING_RECEIPT_KIND_V39, &native_mining_receipt_id_v39(bad_claim.id)).is_none(), "the receipt is revoked");
    assert!(row(&net, MINING_JOB_HOLD_KIND_V39, &forged.job).is_none(), "the hold ends with the conviction");
    assert!(net.chain.tip_state().1.safe_weight() <= safe_before, "no safe weight survives from the forged claim");
    assert!(
        !kaspa_consensus_core::palw_security_certification_v1::palw_native_mining_security_certified_v39(&net.chain.tip_state().1, &bad_claim.claim_hash()),
        "the forged claim's certification is withdrawn"
    );
    for (card, before) in &attestor_collateral {
        assert!(net.collateral(*card) < *before, "attestor card {card} is charged ({} → {})", before, net.collateral(*card));
    }
    println!(
        "g14n2/node: collected {collected} (exposure {}), attestors {:?} → {:?}",
        bad_claim.price.reserved_exposure,
        attestor_collateral,
        colluders.iter().map(|c| net.collateral(*c)).collect::<Vec<_>>()
    );
    // ---- the honest claim stands, certified and receipted ----
    assert_eq!(net.collateral(3), c3, "the honest producer is untouched");
    assert_eq!(mining_claim(&net, &good_claim.id).closed.map(|(_, i)| i), Some(true), "the honest claim keeps its receipt");
    assert!(row(&net, MINING_RECEIPT_KIND_V39, &native_mining_receipt_id_v39(good_claim.id)).is_some());
    assert!(kaspa_consensus_core::palw_security_certification_v1::palw_native_mining_security_certified_v39(&net.chain.tip_state().1, &good_claim.claim_hash()));
    let replay = net.replay().await;
    net.assert_same(&replay, "G-N2 node conviction");
    for c in [&forged, &honest] {
        let _ = std::fs::remove_dir_all(&c.dir);
    }
}

/// **Finding (2026-10-18), then the user's decision (10-18): bill the bytes the action carries.** Before it, Their metadata prepay charges the kernel
/// route's per-block court-work budget (`max_court_work_per_block = 2^30`, `prosecution_reserve_permille = 500`: ordinary admission may
/// spend 2^29 = 536,870,912) with `bytes × 16,384 + 2^20` (Master 65/66 and the component's invocation restore) or `bytes × 8,192 +
/// 65,536` (mining 73/75), and every such action restores at least 65,536 bytes of fixed envelope before its own body. The fold then
/// returns without writing (`prepay` → `Ok(false)`): the action is carried, charged nothing, and changes nothing. This test pins the
/// refusal on the node and prints the numbers; the end-to-end tests above are ignored until the value is decided.
#[tokio::test]
async fn owning39_master_registration_is_billed_on_its_carried_bytes_within_the_block_work_budget() {
    kaspa_core::log::try_init_logger("warn");
    let mut net = Net::over_cfg(owning_config_with(Some(CHALLENGE)), TestConsensus::new);
    net.beat_to(5).await;
    let fx = fixture(&mut net).await;
    let tip = net.chain.tip_state().1;
    let ops: Vec<Digest> = (0..3).map(|c| tip.bond(&net.bond(c)).unwrap().operator_id.as_bytes()).collect();
    let first = net.api().unwrap().execution_nonce_counter_v11(&ops[2]).unwrap().next_nonce;
    let request = MasterPublicRequestV39 {
        version: 39,
        complete_source: fx.complete_source,
        requester: net.kid(2),
        requester_operator: ops[2],
        first_nonce: nonce_bytes_v11(first),
        producers: vec![MasterProducerV39 { producer: net.kid(0), operator: ops[0], reward: 20 }],
        prompt: vec![],
        negative: vec![],
        source: vec![],
        key: vec![],
        steps: 0,
        scalars: vec![],
        images: vec![],
        seed: [6; 32],
        requested_new_tokens: 0,
        eos: vec![],
        finalized: vec![],
    };
    let register = NativeMasterRegisterV39 {
        version: 39,
        sequence: 0,
        deadline: net.daa() + 90_000,
        classes: vec![fx.class_id],
        producer_bonds: vec![net.bond(0)],
        manifest: fx.manifest_bytes.clone(),
        pipeline: fx.pipeline_bytes.clone(),
        request: request.to_bytes(),
    };
    let raw = borsh::to_vec(&register).unwrap();
    let master = register.root(net.bond(2));
    let source_bytes = fx.checked.wire().to_bytes().len() as u64;
    let extent = raw.len() as u64 + source_bytes + 4_096;
    let work = extent * 16_384 + (1 << 20);
    let (cap, reserve) = (1u64 << 30, 500u64);
    let admission = cap - cap * reserve / 1000;
    println!(
        "owning39 Master 65: register {} bytes, source {source_bytes} bytes, restore extent {extent} bytes → work {work} vs ordinary          admission limit {admission} (block cap {cap}); mining 73 work {} vs the same limit",
        raw.len(),
        76u64 * 8_192 + 65_536
    );
    assert!(work < admission, "the Master's carried bytes fit the ordinary admission limit");
    assert!(76u64 * 8_192 + 65_536 < admission, "so does the mining registration");
    act(&mut net, 2, 65, raw).await;
    assert!(row(&net, 80, &master).is_some(), "the Master registers");
}
