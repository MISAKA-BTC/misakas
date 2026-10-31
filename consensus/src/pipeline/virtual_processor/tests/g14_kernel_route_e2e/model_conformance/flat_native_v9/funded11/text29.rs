//! **The text29 job harness (G14 lane closure, 2026-10-17)**: a real EOS-stopped text job on the ordinary node — the native source
//! binding, `TEXT_REGISTER`, `TEXT_PREPARE`, the producer's consent, `TEXT_FUND` and `TEXT_COMMIT` — so text29's courts run on
//! real rows. The Final27 policy carries a TEST calibration for the fixture's verifier profile (structural, never a measurement).
use super::*;
use kaspa_consensus_core::palw_native_final_v27::{NativeFinalCalibrationV27, PalwNativeFinalFenceV27};
use kaspa_consensus_core::palw_native_text_stop_v29::*;

/// Final27 armed with a calibration for `profile`, the funded horizons widened as in `public_filing`.
pub(super) fn text_config(profile: Digest) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut cfg, b, p, f) = native_config();
    let domain = cfg.params.palw_funded_execution_v11.as_ref().unwrap().policies.execution.network_domain;
    let pol = &mut cfg.params.palw_funded_execution_v11.as_mut().unwrap().policies;
    // text29's whole public pursuit (cold reference, sequential acquisition, localization, court, paged proof transport) must fit
    // the challenge window and the whole liability one quota interval: test horizons, not measurements.
    pol.quota.interval_daa = 100_000;
    pol.discovery.requests_per_session = 128;
    // The producer's prepaid answers cover the discovery sessions, the protected slots and the ordinary lane (discovery13's rule).
    pol.quota.future_attempts[2] = 128 * 4 + 8 + pol.discovery.ordinary_lifetime_requests;
    pol.execution.quota_policy_root = pol.quota.root();
    pol.discovery.quota_policy_root = pol.quota.root();
    let e = &mut pol.execution;
    e.horizons.da_answer = 200;
    e.horizons.challenge_window = 30_000;
    e.horizons.retention = 300;
    e.required_producer_reserve = e.required_producer_reserve.max(10_000_000_000_000);
    e.finder_permille = 490;
    let mut fence = PalwNativeFinalFenceV27::unapproved_probe_at(ForkActivation::new(0), domain);
    fence.policy.calibrations = vec![NativeFinalCalibrationV27 {
        profile,
        maximum_reference_bytes: u64::MAX / 4,
        maximum_court_bytes: u64::MAX / 4,
        maximum_court_work: u64::MAX / 4,
        maximum_court_scratch: u64::MAX / 4,
        whole_claim_detection_numerator: 1,
        whole_claim_detection_denominator: 1,
        measured_cold_reference_daa: 1,
        measured_sequential_discovery_daa: 1,
        measured_prove_before_seal_daa: 1,
        measured_proof_delivery_daa: 1,
        restart_reorg_margin_daa: 1,
        measurement_root: [1; 64],
        source_availability_root: [2; 64],
        whole_claim_detection_root: [3; 64],
        real_delivery_service_root: [4; 64],
        independent_native_final_root: [5; 64],
        original_source_ready: true,
        whole_claim_detector_ready: true,
        actual_delivery_ready: true,
        independent_node_ready: true,
    }];
    cfg.params.palw_native_final_v27 = Some(fence);
    (cfg, b, p, f)
}

fn verifier_profile(f: &Sparse9, positions: u32) -> Digest {
    misaka_palw_kernel::flat_resources29::derive_flat_resources_v29(&f.p, &f.pc, positions, None).unwrap().court.court_set
}

fn rows_with_prefix(net: &Net, table: u8, prefix: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    net.api()
        .unwrap()
        .aux
        .iter()
        .filter(|((t, k), _)| *t == table && k.starts_with(prefix))
        .map(|((_, k), v)| (k.clone(), v.clone()))
        .collect()
}

/// Register, prepare, consent and fund one text29 job; returns the job id.
pub(super) async fn text_funded(net: &mut Net, f: &Sparse9, positions: u32) -> Digest {
    let (bind, _, _) = admitted_bound(net, f, positions).await;
    let program_bytes = f.p.encode();
    let source = net.api().unwrap().model_artifact_binding_header_v2(&bind).expect("the source binding");
    let register = NativeTextRegisterV29 {
        version: 29,
        source_binding: bind.as_bytes(),
        original_source_components: native_text_source_components_v29(
            bind.as_bytes(),
            source.program_root,
            source.kernel_param_root.as_bytes(),
            source.model_inventory_root.as_bytes(),
        ),
        maximum_positions: positions,
        program_bytes,
        real_parameters: f.pc.clone(),
    };
    action(net, 0, TEXT_REGISTER_ACTION_V29, borsh::to_vec(&register).unwrap()).await;
    let classes = rows_with_prefix(net, EXECUTION_CLASS_TABLE_V11, &[29, 0]);
    assert_eq!(classes.len(), 1, "the text29 class is registered");
    let class: Digest = classes[0].0[2..66].try_into().unwrap();
    let prompt = vec![0u32, 1];
    let req_bond = net.chain.tip_state().1.bond(&net.bond(2)).unwrap().operator_id.as_bytes();
    let prod_bond = net.chain.tip_state().1.bond(&net.bond(0)).unwrap().operator_id.as_bytes();
    let request = NativeTextRequestV29 {
        version: 29,
        class,
        requester: net.kid(2),
        requester_operator: req_bond,
        producer: net.kid(0),
        producer_operator: prod_bond,
        nonce: nonce_bytes_v11(0),
        prompt_root: prompt_root_v29(&prompt),
        prompt_len: prompt.len() as u32,
        maximum_new_tokens: 1,
        eos_token: 3,
        reward: 20,
    };
    let wire = NativeTextPrepareV29 { version: 29, request: request.clone(), prompt: prompt.clone() };
    action(net, 2, TEXT_PREPARE_ACTION_V29, borsh::to_vec(&wire).unwrap()).await;
    let preps = rows_with_prefix(net, EXECUTION_PREPARATION_TABLE_V11, &[29, 1]);
    assert_eq!(preps.len(), 1, "the preparation is admitted");
    let preparation: Digest = preps[0].0[2..66].try_into().unwrap();
    let mut cursor = std::io::Cursor::new(&preps[0].1[..]);
    let _: NativeTextPrepareV29 = borsh::BorshDeserialize::deserialize_reader(&mut cursor).unwrap();
    let maximum: NativeTextMaximumV29 = borsh::BorshDeserialize::deserialize_reader(&mut cursor).unwrap();
    let interval = net.config.params.palw_funded_execution_v11.as_ref().unwrap().policies.quota.interval_daa;
    let now = net.daa() + 1;
    let consent = NativeTextConsentV29 {
        version: 29,
        preparation,
        request: request.root(),
        maximum: maximum.clone(),
        producer: net.kid(0),
        producer_operator: prod_bond,
        requester: net.kid(2),
        requester_operator: req_bond,
        nonce: request.nonce,
        admitted_interval: now / interval,
        accepted_daa: now,
        expires_daa: now + maximum.commit_window.min(interval - now % interval - 1),
        public_protocol: maximum.public_pursuit,
        queue_policy: maximum.queue_policy,
        queue_plan: maximum.queue_plan,
    };
    action(net, 0, TEXT_ACCEPT_ACTION_V29, borsh::to_vec(&consent).unwrap()).await;
    assert_eq!(rows_with_prefix(net, EXECUTION_PREPARATION_TABLE_V11, &[29, 3]).len(), 1, "the producer's consent landed at {now}");
    let fund = NativeTextFundV29 { version: 29, preparation, initial_consent: consent.root() };
    action(net, 2, TEXT_FUND_ACTION_V29, borsh::to_vec(&fund).unwrap()).await;
    let jobs = rows_with_prefix(net, EXECUTION_JOB_TABLE_V11, &[29, 0]);
    assert!(!jobs.is_empty(), "the text29 job is funded");
    jobs[0].0[2..66].try_into().unwrap()
}

#[tokio::test]
async fn text29_harness_registers_prepares_consents_and_funds_a_real_job() {
    kaspa_core::log::try_init_logger("warn");
    let f = sparse9(false);
    let mut net = Net::over_cfg(text_config(verifier_profile(&f, 3)), TestConsensus::new);
    let job = text_funded(&mut net, &f, 3).await;
    println!("text29 job {}", faster_hex::hex_string(&job[..8]));
    let replay = net.replay().await;
    net.assert_same(&replay, "text29 job funded");
}

use misaka_palw_kernel::flat_court25::{FlatContextV25, FlatFaultV25, verify_flat_fault_v25};
use misaka_palw_kernel::flat_detect5::{FlatPublicReadsV5, FlatReadV5};
use misaka_palw_kernel::flat_detect25::*;
use misaka_palw_kernel::flat_replay25::*;
use misaka_palw_kernel::flat_scope25::*;

/// The producer's text29 trace (prompt `[0, 1]`, generated `[3]` = EOS), optionally corrupted like `statement`'s case 4.
struct Text29Trace {
    trace: misaka_palw_kernel::trace::TraceV1,
    c: SegmentedCommitmentsV1,
    roots: Vec<Digest>,
    prompt: Vec<u32>,
    generated: Vec<u32>,
}
fn text_trace(f: &Sparse9, corrupt: bool) -> Text29Trace {
    let prompt = vec![0u32, 1];
    let mut trace = misaka_palw_kernel::trace::trace_v1(&f.p, &f.params, &prompt).unwrap();
    if corrupt {
        trace.values[1][0][6].data[0] += 1;
    }
    let c = SegmentedCommitmentsV1::new(
        trace
            .values
            .iter()
            .map(|p| {
                p.iter()
                    .map(|s| s.iter().map(|t| misaka_palw_kernel::flat_court5::flat_profile_commitment_v5(t).unwrap()).collect())
                    .collect()
            })
            .collect(),
    );
    let roots = c.segment_roots();
    Text29Trace { trace, c, roots, prompt, generated: vec![3] }
}
/// The producer's material, as its public answers would carry it (the same openings `flat_graph25`'s reads serve).
struct Material<'a>(&'a Text29Trace);
impl FlatPublicReadsV5 for Material<'_> {
    fn node(&self, u: FlatReadV5) -> Option<misaka_palw_kernel::seg::NodeOpeningV1> {
        self.0.c.node_opening(u.position, u.occurrence, u.node)
    }
    fn hash(&self, u: FlatReadV5) -> Option<misaka_palw_kernel::merkle5::FlatLeafHashV5> {
        let t = &self.0.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let h = FlatTreeV5::of(t).ok()?.opening(u.leaf?)?;
        misaka_palw_kernel::merkle5::decode_flat_hash_v5(&borsh::to_vec(&h).unwrap()).ok()
    }
}
impl FlatPublicReadsV25 for Material<'_> {
    fn header(&self, r: FlatHeaderReadV25) -> Option<Vec<u8>> {
        let u = r.node;
        let t = &self.0.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let shape = t.shape.iter().map(|d| *d as u64).collect();
        let header = if t.data.len() >= 64 {
            misaka_palw_kernel::flat_court5::FlatHeaderV5::Flat { dtype: t.dtype.tag(), shape, root: FlatTreeV5::of(t).ok()?.root() }
        } else {
            let tree = TreesV3::of(t);
            misaka_palw_kernel::flat_court5::FlatHeaderV5::Legacy {
                dtype: t.dtype.tag(),
                shape,
                row_root: tree.row_root(),
                col_root: tree.col_root(),
            }
        };
        FlatHeaderValueV25 { version: 25, scope: r.scope, header }.to_bytes().ok()
    }
    fn value(&self, r: FlatValueReadV25) -> Option<Vec<u8>> {
        let u = r.node;
        let t = &self.0.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        let o = self.0.c.node_opening(u.position, u.occurrence, u.node)?;
        let mut value = FlatOperandV5 { node: Some(o), ..Default::default() };
        if t.data.len() >= 64 {
            let tree = FlatTreeV5::of(t).ok()?;
            value.flat.push(FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(r.element)?)?);
        } else {
            let (row, tile, _) = misaka_palw_kernel::merkle3::LayoutV3::of(&t.shape).row_leaf_of(r.element)?;
            value.legacy.push(LeafOpeningV3::of_trees(t, &TreesV3::of(t), 0, row, tile)?);
        }
        FlatValueV25 { version: 25, scope: r.scope, value }.to_bytes().ok()
    }
}
fn context<'a>(f: &'a Sparse9, t: &'a Text29Trace, scope: &'a FlatResourceScopeV25) -> FlatContextV25<'a> {
    FlatContextV25 {
        scope,
        base: misaka_palw_kernel::flat_court5::FlatSegContextV5 {
            profile: misaka_palw_kernel::flat_court5::flat_trace_profile_id_v5(),
            claim: misaka_palw_kernel::element::SegClaimContextV1 {
                program: &f.p,
                params: &f.pc,
                segment_roots: &t.roots,
                positions: 2,
                prompt_len: 2,
                prompt_root: misaka_palw_kernel::seg::prompt_root_of_ids_v1(&t.prompt),
                inline_prompt: Some(&t.prompt),
                generated: &t.generated,
                decode: misaka_palw_kernel::job::DecodeRuleV1::Greedy,
                encoder: None,
            },
        },
    }
}
/// Commit the stopped post of `t` to the funded `job` (producer card 0).
async fn text_commit(net: &mut Net, f: &Sparse9, job: Digest, t: &Text29Trace) {
    use misaka_palw_kernel::seg::*;
    let header: Vec<u8> =
        rows_with_prefix(net, EXECUTION_JOB_TABLE_V11, &[29, 0]).into_iter().find(|(k, _)| k[2..66] == job).unwrap().1;
    // JobHeader: job ‖ class ‖ funding ‖ …
    let class: Digest = header[64..128].try_into().unwrap();
    let funding: Digest = header[128..192].try_into().unwrap();
    let prep = rows_with_prefix(net, EXECUTION_POLICY_TABLE_V11, &[29, 0])
        .into_iter()
        .find(|(k, _)| k[2..66] == job)
        .expect("the job's preparation")
        .1;
    let mut cursor = std::io::Cursor::new(&prep[..]);
    let wire: NativeTextPrepareV29 = borsh::BorshDeserialize::deserialize_reader(&mut cursor).unwrap();
    let maximum: NativeTextMaximumV29 = borsh::BorshDeserialize::deserialize_reader(&mut cursor).unwrap();
    let actual = misaka_palw_kernel::flat_resources29::derive_flat_resources_v29(&f.p, &f.pc, 2, None).unwrap();
    let domain = net.config.params.palw_funded_execution_v11.as_ref().unwrap().policies.execution.network_domain;
    let evidence = SegmentedEvidenceV2 {
        version: SEG_EVIDENCE_VERSION_V2,
        header: misaka_palw_kernel::evidence::EvidenceHeaderV1 {
            network_domain: domain,
            ruleset_digest: maximum.full_court_profile25,
            class_binding_id: class,
            program_root: misaka_palw_kernel::public::program_root_v1(&f.p.encode()),
            artifact_root: maximum.parameter,
            plan_root: maximum.maximum_resource29,
        },
        job_input_root: job_input_root_v2(2, &prompt_root_of_ids_v1(&wire.prompt), &t.generated[..t.generated.len() - 1]),
        positions: 2,
        segment_len: SEG_LEN_V4,
        claim_root: claim_root_v1(2, &t.roots),
        suite: misaka_palw_kernel::evidence::SuiteParamsV1::of(&f.d),
    };
    let post = NativeTextPostV29 {
        version: 29,
        job,
        original_funding: funding,
        maximum_contract: maximum.root(),
        requester_nonce: wire.request.nonce,
        generated: t.generated.clone(),
        actual_scope25: actual.court.root(),
        actual_resource29: actual.vector.root(),
        evidence,
        segment_roots: t.roots.clone(),
        final_state: [0x5F; 64],
    };
    action(net, 0, TEXT_COMMIT_ACTION_V29, borsh::to_vec(&post).unwrap()).await;
}
fn text_book_terminal(net: &Net, job: Digest) -> Vec<u8> {
    rows_with_prefix(net, EXECUTION_JOB_TABLE_V11, &[29, 1]).into_iter().find(|(k, _)| k[2..66] == job).unwrap().1
}
fn public_text_filing(job: Digest, fault: &FlatFaultV25) -> Vec<u8> {
    let inner = fault.to_bytes().unwrap();
    let mut filing = 25u16.to_le_bytes().to_vec();
    filing.push(0);
    filing.extend_from_slice(&(inner.len() as u32).to_le_bytes());
    filing.extend_from_slice(&inner);
    let mut body = job.to_vec();
    body.extend_from_slice(&(filing.len() as u32).to_le_bytes());
    body.extend_from_slice(&filing);
    body
}

/// **Action 21 on a real text29 job**: a lying stopped post is convicted by an outsider's public filing (no paid owner); the same
/// filing against an honest job is dismissed.
#[tokio::test]
async fn text29_a_public_filing_convicts_a_lying_text_job_without_a_paid_owner() {
    kaspa_core::log::try_init_logger("warn");
    let f = sparse9(false);
    for corrupt in [true, false] {
        let mut net = Net::over_cfg(text_config(verifier_profile(&f, 3)), TestConsensus::new);
        let job = text_funded(&mut net, &f, 3).await;
        let liar = text_trace(&f, true);
        let posted = if corrupt { text_trace(&f, true) } else { text_trace(&f, false) };
        text_commit(&mut net, &f, job, &posted).await;
        let committed = text_book_terminal(&net, job);
        // The outsider's fault, found by an independent replay of the lie (its material served as the producer's answers).
        let scope = flat_resource_scope_v25(&f.p, 2, None).unwrap();
        let envelope = flat_replay_envelope_v25(&f.p, &scope, None).unwrap();
        let r = &envelope.reference;
        let limits = FlatReplayLimitsV25 {
            reference: misaka_palw_kernel::flat_replay6::FlatReplayLimitsV6 {
                position_tensor_bytes: r.position_tensor_bytes,
                operand_tensor_bytes: r.largest_operand_tensor_bytes,
                history_tensor_bytes: r.history_rows_tensor_bytes,
                parameter_hash_bytes: r.parameter_hash_bytes_per_attempt,
                trace_hash_bytes: r.trace_hash_bytes_per_attempt,
                node_reads: r.node_reads_per_attempt,
                hash_reads: r.hash_reads_per_attempt,
                reply_bytes: r.reply_bytes_per_attempt,
                attempts: 1,
            },
            topk_scan_work: envelope.topk_scan_work_per_attempt,
            topk_auxiliary_bytes: envelope.topk_auxiliary_bytes,
            selected_value_reads: envelope.selected_value_reads_per_attempt,
            header_reads: envelope.header_reads_per_attempt,
            scope_checker_work: envelope.scope_checker_work_per_attempt,
        };
        let mut session = FlatReplaySessionV25::new(limits);
        let result = session.check(
            &context(&f, &liar, &scope),
            &envelope,
            &Material(&liar),
            &|i, l| f.params.tensors.get(&(i, l)).cloned(),
            &liar.prompt,
        );
        let FlatReplayResultV25::Finding(FlatFindingV25::Fault(fault)) = result else { panic!("no fault in the lie: {result:?}") };
        verify_flat_fault_v25(&context(&f, &liar, &scope), &fault).unwrap();
        let producer_before = net.collateral(0);
        action(&mut net, 7, EXECUTION_ACTION_TEXT_PUBLIC_FILING_V29, public_text_filing(job, &fault)).await;
        let after = text_book_terminal(&net, job);
        if corrupt {
            assert_ne!(after, committed, "the lying text job is convicted by the public filing");
            assert!(net.collateral(0) < producer_before, "the producer is slashed");
            println!("text29: producer slashed {} sompi", producer_before - net.collateral(0));
        } else {
            assert_eq!(after, committed, "an honest text job is untouched by another job's fault");
            assert_eq!(net.collateral(0), producer_before);
        }
        let replay = net.replay().await;
        net.assert_same(&replay, "text29 public filing");
    }
}
