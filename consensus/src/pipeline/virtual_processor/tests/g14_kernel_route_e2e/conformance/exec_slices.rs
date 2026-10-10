//! **RFC-0008 v2 × G14 on the real node: EXEC slice carriage preserves G14** — the composed run of the implementation record's section
//! 12, item 1 (lane X8R), with the round-3 additions of section 14 (GAP-60, GAP-62, ADR-0176 D2).
//!
//! ```text
//! the conformance world (Cw): TIR V2 class → 104 bind → OPV kernel class → 106 kernel-bind → 107 commit → beacon locks on future
//!   OPV Finals → 109 evidence → window closes unrefuted → V2 class Active (all through blocks)
//! → a REAL attempt on that class (card 0, the node's own template, the attempt lane), committing the output an honest IR attempt
//!   would: the bound kernel class's run on the prompt its execution anchor names               [SEAM: see below]
//! → the kernel side, through the route as any producer: the PREFIX job (the anchored prompt, the prefix job nonce) and the three
//!   slice jobs (the stream so far, the slice job nonce) posted; the root card's prefix claim of the REAL run and each executor's
//!   slice claim sealed, then revealed
//! → tag 130: a work session on the REAL claim, its plan the binding's kernel plan, its prefix claim named and its initial boundary
//!   `token_state(prompt ‖ REAL run)` — checked by the fold from chain data (GAP-62)
//! → per slice: the slice statement derived from public rows (`palw_exec_v2_slice_statement_v1`, the producer's input), an
//!   EXEC_SLICE lane block signed by the executor, anchored by the next heartbeat and admitted by the fold against its kernel claim
//! → (a) an outsider, from the read API and the producer's published DA alone, convicts slice 1's lying kernel claim through the route:
//!       slice 1 proven false, slice 2 void, the root void, the REAL claim void (`WorkSliceProvenFalse`), nothing charged at V2;
//!   (b) the honest twin: every kernel claim (prefix included) finalizes with no Panel after its public window, every slice verifies
//!       (its leg cap fixed), the root is ready and the REAL claim's Final hold is released;
//!   (c) GAP-60: a slice whose claim's committed position is withheld is demanded by an outside bond and defaults at the deadline:
//!       slice 1 defaulted, the suffix, the root and the REAL claim void (`WorkSliceDefaulted`); before it, a slice whose prompt does
//!       not continue the stream is carried, anchored and refused `PredecessorMismatch` (rule 4 at node);
//!   (d) GAP-62: a declaration whose initial boundary is not the REAL run is dropped; the root bond's prefix claim lies, an outsider
//!       convicts it, and the whole session — every slice, the root, the REAL claim — is void (`WorkSliceProvenFalse`);
//! → a second node replaying the chain agrees on every root.
//! ```
//!
//! **The seam (named, not hidden).** Everything above runs through blocks except one door: the onboarded class takes the REAL attempt
//! because the processor's `cfg(test)` seam [`exec_v2_test_admit_class_v1`] waives the model-registry lifecycle and room, the seating
//! and the per-bond share gates for that one class (the way `kernel_route_test_attest_artifact_v1` stands in for artifact
//! availability). Under the Lead's GAP-81 decision (2026-10-10, OPVB `c3221bc34`) a V2 REAL claim of an onboarded class earns only on
//! the LEGACY channel, whose old rules need Panel seat readiness this harness cannot prove — so the seam now stands for that channel's
//! admission, not for G14-for-rewards, and no reward gate can replace it (record §14). The slice and route half — the carriage, the
//! bindings, the kernel claims, the outsider's conviction and demand, the OPV Finals, the sync — is the real node's, unseamed.
//!
//! Not run here (and why): the REAL root claim's own `Final` and the capped escrow settlement need the root class's Panel (the
//! legacy channel), which an onboarded class does not have on this harness; the settlement is pinned at fold level
//! (`exec_v2_fold_v1::amendment_1`).
//! The REAL attempt's trace-root lottery search is also a synthetic commitment, not a native trace witness. The kernel runs
//! are checked against their actual public material; do not use this harness to claim honest native receipt/Final evidence.
use super::*;
use crate::pipeline::virtual_processor::processor::exec_v2_test_admit_class_v1;
use crate::pipeline::virtual_processor::tests::OnetimeTxSelector;
use crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::stamp_harness_time;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::block::{MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1;
use kaspa_consensus_core::palw_exec_v2::{
    PALW_EXEC_V2_WIRE_VERSION, PalwExecSubtypeV2, PalwExecV2Envelope, PalwWorkSliceV1, palw_work_slice_payload_root_v2,
};
use kaspa_consensus_core::palw_exec_v2_verify::{
    palw_exec_v2_prefix_job_nonce_v1, palw_exec_v2_real_job_v1, palw_exec_v2_retention_terms_v1, palw_exec_v2_slice_job_nonce_v1,
    palw_exec_v2_slice_job_nonce_v2, palw_exec_v2_token_state_root_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwVoidReasonV2};
use kaspa_consensus_core::palw_work_slice_v2::{
    PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, PalwSliceRefusalV2, PalwWorkPrefixStageV2, PalwWorkRootDeclarationV2, PalwWorkRootPhaseV2,
    PalwWorkRootV2, PalwWorkSliceStageV2, palw_work_plan_range_v2,
};

/// The EXEC v2 payload's fence: low, crossed long before the class activates.
const EXEC_FENCE: u64 = 6;
/// The candidate's fixture seed: a class of its own, so the seam waives nothing another test of this process asks about.
const SEED: u64 = 23;
/// The REAL root's producer (and so the root bond, and the prefix claim's producer).
const ROOT_CARD: usize = 0;
/// The slices' executors (the declaration's extra executors), by slice index; none produced a beacon source claim.
const EXECUTORS: [usize; 3] = [5, 6, 5];
/// The outsider: no executor, not the registrant, not the root bond.
const ACCUSER: usize = 7;
/// Tokens each slice's kernel claim generates.
const RUN: u32 = 2;

fn key(card: usize) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    TestConsensus::palw_v2_registry_keypair(card as u64)
}

fn pubkey(card: usize) -> Vec<u8> {
    key(card).verification_key.as_ref().to_vec()
}

fn sign(card: usize, message: &[u8], context: &[u8], rnd: u8) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&key(card).signing_key, message, context, [rnd; 32]).expect("ML-DSA-87 signs").as_ref().to_vec()
}

/// **The conformance world with the EXEC v2 payload armed, its candidate class driven to `Active` through blocks** — the pass flow of
/// `g14_conformance_evidence_passes_only_after_an_unrefuted_window_and_the_class_activates`, without its assertions.
async fn active_world() -> Cw {
    kaspa_core::log::try_init_logger("warn");
    let mut cw = Cw::over_with(
        SEED,
        |params| {
            params.palw_exec_payload_v2 = Some(ForkActivation::new(EXEC_FENCE));
            params.sync_palw_exec_payload_v2();
            assert!(params.validate_palw_exec_payload_v2().is_err(), "validation refuses the payload fence: the harness arms past it");
        },
        TestConsensus::new,
    )
    .await;
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let post = cw.post(|_, _| {});
    cw.send_post(post, Some(1024)).await;
    let posted = cw.attempt().evidence.expect("the fold accepted the evidence");
    cw.net.beat_to(posted.window_end_daa + 1).await;
    let binding = cw.net.api().unwrap().artifact_binding_v1(&cw.v2_class, &cw.kernel_root).unwrap();
    if cw.net.daa() < binding.final_daa {
        cw.net.beat_to(binding.final_daa + 1).await;
    }
    assert!(
        matches!(cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().status, PalwClassStatusV2::Active),
        "the candidate is Active through blocks: {:?}",
        cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().status
    );
    assert!(cw.net.daa() > EXEC_FENCE, "the payload fence is crossed");
    cw
}

/// The REAL root's public job and run: the claim, the prompt its anchor names and its committed run (GAP-62).
struct RealRun {
    claim: Hash64,
    executor_exposure_sompi: u128,
    prompt: Vec<u32>,
    run: Vec<u32>,
}

impl RealRun {
    /// The session's stream after the prefix: the anchored prompt and the REAL claim's run.
    fn stream(&self) -> Vec<u32> {
        let mut stream = self.prompt.clone();
        stream.extend_from_slice(&self.run);
        stream
    }
}

/// **A REAL attempt of `class` by card `card`**: the node's own template, the attempt lane, the class lottery won, signed by the card.
/// The facts are the node's answer for the class (`palw_producer_facts_v2`); their readiness is printed, not asked — the seam is what
/// admits the class (module doc). **GAP-62:** the attempt commits the output an honest IR attempt of the class would: the run of the
/// bound kernel class on the prompt its execution anchor names, under J5's canonical context — so a prefix claim can carry it.
async fn real_attempt(cw: &mut Cw, card: usize, class: Hash64, nonce: u64) -> RealRun {
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
        PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
    };
    let kernel_class = cw.kernel_class;
    let step = cw.net.ttpb();
    cw.net.chain.ctx.simulated_time += step;
    let bond = cw.net.bond(card);
    let mut t = cw
        .net
        .chain
        .ctx
        .consensus
        .build_block_template(
            MinerData::new(card_payout_spk(card), vec![]),
            Box::new(OnetimeTxSelector::new(Vec::new())),
            TemplateBuildMode::Standard,
        )
        .expect("a template");
    assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id), "the attempt lane");
    stamp_harness_time(&cw.net.config.params, &mut t.block.header, cw.net.chain.ctx.simulated_time);
    t.block.header.nonce = nonce;
    let facts = cw.net.chain.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("the node answers for the class");
    eprintln!(
        "[x8-g14] REAL attempt on {class}: ready = {:?}, class gate = {:?}, pwu = {}",
        facts.ready_to_produce(&pubkey(card)),
        facts.class_admission_refusal,
        facts.pwu
    );
    let bond_facts = facts.bond.as_ref().expect("a genesis card").clone();
    let header = &t.block.header;
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let anchor = execution_anchor_v3(cw.net.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
    // GAP-62: the anchored prompt, J5's canonical context and the honest run of the bound kernel class on that prompt.
    let tir = cw.net.chain.tip_state().1.tir_class_v1(&class).expect("an IR class").facts.clone();
    let form = cw.net.config.params.palw_prompt_ids_form_at(header.daa_score);
    let (prompt, context) = palw_exec_v2_real_job_v1(&tir, &anchor, form).expect("the anchored job");
    let run = greedy(&cw.cand, &cw.net.ledger(), &kernel_class, &prompt, context.exact_decode_tokens.max(1) as usize);
    let mut attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: cw.net.domain,
        challenge: challenge_v2(cw.net.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
        class_id: facts.class_id,
        executor_bond: bond.0,
        executor_pubkey: pubkey(card),
        operator_id: bond_facts.operator_id,
        artifact_root: facts.artifact_root,
        trace_root: Hash64::default(),
        output_root: palw_attempt_output_root_v1(&context, &run),
        execution_root: Hash64::from_u64_word(0xE8EC_0000_0000_0000 | nonce),
        pwu: facts.pwu,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
    };
    // Under the work target a draw costs every class the same work, so a class whose one inference is cheap (this fixture) has a hard
    // ticket: about 2^128 / target ≈ 23M draws expected (two runs took 11.2M and 11.8M, ~5 µs each). The cap makes a miss
    // vanishingly rare (e^-17), not the run short.
    let won = (0u64..400_000_000).position(|draw| {
        attempt.trace_root = Hash64::from_u64_word((nonce << 32) ^ draw ^ 0x7B00_0000_0000_0000);
        attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
        class_ticket_v3(&attempt, anchor) <= facts.class_target
    });
    eprintln!("[x8-g14] the class lottery: {won:?} draws (target {})", facts.class_target);
    assert!(won.is_some(), "the class lottery is winnable");
    let claim = attempt_id_v2(&attempt);
    let signature = sign(card, claim.as_byte_slice(), PALW_ATTEMPT_V2_MLDSA87_CONTEXT, 0x5B);
    t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
    t.block.header.finalize();
    let block = t.block.to_immutable();
    let hash = block.header.hash;
    cw.net
        .chain
        .ctx
        .consensus
        .validate_and_insert_block(block)
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("the REAL attempt on the onboarded class was refused: {e}"));
    assert_eq!(cw.net.chain.sink(), hash, "the attempt is the sink");
    let state = cw.net.chain.tip_state().1;
    let row = state.claim(&claim).unwrap_or_else(|| panic!("the fold took no claim of the onboarded class (block {hash})"));
    assert!(matches!(row.phase, PalwClaimPhaseV2::Provisional), "a fresh REAL claim: {:?}", row.phase);
    assert_eq!(row.job_identity, anchor, "the claim's job identity is its execution anchor");
    let executor_exposure_sompi = cw.net.chain.tip_state().1.claim(&claim).unwrap().reserved;
    RealRun { claim, prompt, run, executor_exposure_sompi }
}

/// The session one REAL claim opens: three `RUN`-token slices with registered IR costs, the binding's kernel plan, the
/// prefix claim `prefix` and the initial boundary `initial` (GAP-62: `token_state(prompt ‖ REAL run)`, unless a test lies), executors
/// `EXECUTORS`. Unsigned and uncarried: [`send_root`] carries it.
fn declaration(cw: &Cw, real: &RealRun, prefix: Hash64, initial: Hash64) -> PalwWorkRootDeclarationV2 {
    let state = cw.net.chain.tip_state().1;
    let row = state.claim(&real.claim).expect("the claim").clone();
    declaration_for_job(cw, real, prefix, initial, row.pwu, row.job_identity)
}

fn declaration_for_job(
    cw: &Cw,
    real: &RealRun,
    prefix: Hash64,
    initial: Hash64,
    _legacy_lottery_weight: u64,
    job_identity: Hash64,
) -> PalwWorkRootDeclarationV2 {
    let binding = cw.net.api().unwrap().kernel_binding_v1(&cw.v2_class).expect("kernel-bound");
    let mut extra: Vec<PalwBondKeyV2> = EXECUTORS.iter().map(|card| cw.net.bond(*card)).collect();
    extra.sort();
    extra.dedup();
    let state = cw.net.chain.tip_state().1;
    let tir = state.tir_class_v1(&cw.v2_class).unwrap();
    let program = misaka_palw_tir::TirProgramV1::decode_canonical(&tir.program).unwrap();
    let registered = state.class(&cw.v2_class).unwrap().registered_daa;
    let cost = if cw.net.config.params.palw_tir_fence2_active_at(registered) {
        kaspa_consensus_core::palw_exec_work_plan_v1::PalwExecWorkCostV1::MinSelectArmsV2
    } else {
        kaspa_consensus_core::palw_exec_work_plan_v1::PalwExecWorkCostV1::AllNodesV1
    };
    let prefix_generated = real.run.len() as u32;
    let plan = kaspa_consensus_core::palw_exec_work_plan_v1::palw_exec_work_plan_v1(
        &program,
        cost,
        tir.facts.max_context,
        real.prompt.len() as u32,
        prefix_generated,
        &[prefix_generated + RUN, prefix_generated + 2 * RUN, prefix_generated + 3 * RUN],
    )
    .unwrap();
    PalwWorkRootDeclarationV2 {
        root_claim_id: real.claim,
        canonical_job_id: job_identity,
        input_root: palw_exec_v2_token_state_root_v1(&[&real.prompt]),
        kernel_version: 1,
        plan_root: binding.plan_root,
        total_work: plan.total_work(),
        boundaries: plan.boundaries().to_vec(),
        initial_state_root: initial,
        prefix_claim: prefix,
        evidence_policy_root: Hash64::from_u64_word(0x34),
        extra_executors: extra,
        expiry_daa: cw.net.daa() + 5_000,
        signature: Vec::new(),
    }
}

/// Sign `declaration` by the root card and carry it in a 0x4b transaction; returns the root the tip then holds, if any (a declaration
/// the fold refuses is dropped as an object and the block stands).
async fn send_root(cw: &mut Cw, mut declaration: PalwWorkRootDeclarationV2) -> Option<PalwWorkRootV2> {
    let claim = declaration.root_claim_id;
    declaration.signature =
        sign(ROOT_CARD, declaration.signing_message(cw.net.domain).as_byte_slice(), PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, 0x71);
    let carrier = cw.net.carrier(ROOT_CARD, &Obj::ExecWorkRootOpenedV2 { declaration: Box::new(declaration) });
    let ttpb = cw.net.ttpb();
    cw.net.chain.heartbeat(ttpb, vec![carrier]).await;
    cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    cw.net.chain.tip_state().1.exec_v2_root_v1(&claim).cloned()
}

/// One kernel claim of the session: its job and the executor's claim of it.
struct SliceClaim {
    job: KernelJobV1,
    claim: Claim,
}

fn worker_nonce(
    cw: &Cw,
    real: &RealRun,
    decl: &PalwWorkRootDeclarationV2,
    index: u32,
    range: kaspa_consensus_core::palw_exec_v2::PalwWorkRangeV1,
) -> Hash64 {
    let terms = palw_exec_v2_retention_terms_v1(
        decl.expiry_daa,
        real.executor_exposure_sompi,
        cw.net.bundle.state.window_court(),
        &cw.net.ledger().policy,
    )
    .unwrap();
    palw_exec_v2_slice_job_nonce_v2(&slice_key(cw, real, decl, index, range), &terms)
}

/// **The kernel side of a session, all through the route as any producer**: the prefix job (the anchored prompt, the prefix job
/// nonce) and the three slice jobs (each prompt the stream so far, each nonce the slice job nonce of `(root, index, range, job, plan)`),
/// posted by the registrant in one block; the root card's prefix claim of the REAL run (lying where `lie_prefix`) and each
/// `EXECUTORS[i]`'s honest greedy run (lying where `lie(i)`), sealed in one block and revealed within each later block's budget. `extra` jobs — a
/// test's off-stream statements — ride along, each claimed by the named card with its honest run. Returns `(prefix, slices, extras)`.
async fn kernel_claims(
    cw: &mut Cw,
    real: &RealRun,
    decl: &PalwWorkRootDeclarationV2,
    lie_prefix: bool,
    lie: impl Fn(usize) -> bool,
    extra: Vec<(usize, KernelJobV1)>,
) -> (SliceClaim, Vec<SliceClaim>, Vec<SliceClaim>) {
    let kernel_class = cw.kernel_class;
    let ledger = cw.net.ledger();
    let prefix_job = KernelJobV1 {
        class_binding_id: kernel_class,
        prompt: real.prompt.clone(),
        max_new_tokens: real.run.len() as u32,
        decode: DecodeRuleV1::Greedy,
        nonce: palw_exec_v2_prefix_job_nonce_v1(&real.claim, decl.boundaries[0], &decl.canonical_job_id, &decl.plan_root).as_bytes(),
    };
    let mut planned: Vec<(usize, KernelJobV1, Vec<u32>, bool)> = vec![(ROOT_CARD, prefix_job, real.run.clone(), lie_prefix)];
    let mut stream = real.stream();
    for index in 0..3u32 {
        let range = palw_work_plan_range_v2(&decl.boundaries, index).expect("a planned slice");
        let nonce = worker_nonce(cw, real, decl, index, range);
        let job = KernelJobV1 {
            class_binding_id: kernel_class,
            prompt: stream.clone(),
            max_new_tokens: RUN,
            decode: DecodeRuleV1::Greedy,
            nonce: nonce.as_bytes(),
        };
        let generated = greedy(&cw.cand, &ledger, &kernel_class, &job.prompt, RUN as usize);
        stream.extend_from_slice(&generated);
        planned.push((EXECUTORS[index as usize], job, generated, lie(index as usize)));
    }
    for (producer, job) in extra {
        let generated = greedy(&cw.cand, &ledger, &kernel_class, &job.prompt, job.max_new_tokens as usize);
        planned.push((producer, job, generated, false));
    }
    let posts: Vec<(usize, Obj)> =
        planned.iter().map(|(_, job, _, _)| (REGISTRANT, cw.net.route(REGISTRANT, &K::PostJob { job: job.clone() }))).collect();
    cw.net.send(posts).await;
    let posted = cw.net.ledger();
    for (_, job, _, _) in &planned {
        assert!(posted.jobs.contains_key(&job.id()), "the prefix/slice job was posted");
    }
    let mut seals = Vec::new();
    let mut reveals = Vec::new();
    let mut out = Vec::new();
    for (producer, job, generated, lying) in planned {
        let at = matmul_at(&cw.cand.program, 1);
        let produced = produce(&cw.cand, &ledger, &kernel_class, &job, cw.net.kid(producer), generated, |t| {
            if lying {
                bump(&mut t.values[at.0 as usize][at.1 as usize][at.2 as usize], 1)
            }
        });
        let id = produced.claim.id();
        let (seal, reveal) = seal_and_reveal(&ledger, cw.net.kid(producer), &produced.object);
        seals.push((producer, cw.net.route(producer, &seal)));
        reveals.push((producer, reveal));
        out.push(SliceClaim { job, claim: Claim { id, producer, trace: produced.trace, at } });
    }
    cw.net.send(seals).await;
    let sealed = cw.net.ledger();
    for slice in &out {
        let producer = cw.net.kid(slice.claim.producer);
        assert!(
            sealed.seals.contains_key(&(slice.job.id(), producer)),
            "the prefix/slice seal was accepted: producer {}, kernel bond {:?}, REAL reservation {:?}",
            slice.claim.producer,
            sealed.bonds.get(&producer),
            cw.net.chain.tip_state().1.claim(&real.claim).map(|row| row.reserved)
        );
    }
    // The node's small test budget reserves half of its adjudications for prosecutions.
    // Four (or five) reveals cannot all fit in one block; respect the remaining admission room.
    let reveal_capacity = (sealed.policy.max_adjudications_per_block - sealed.policy.prosecution_reserved_runs()) as usize;
    assert!(reveal_capacity > 0, "the policy leaves admission room");
    for batch in reveals.chunks(reveal_capacity) {
        let carriers = batch.iter().map(|(p, o)| (*p, cw.net.route(*p, o))).collect();
        cw.net.send(carriers).await;
    }
    let ledger = cw.net.ledger();
    for s in &out {
        assert!(ledger.claims.contains_key(&s.claim.id), "kernel claim {} committed", Hash64::from_bytes(s.claim.id));
    }
    let mut out = out.into_iter();
    let prefix = out.next().expect("the prefix claim");
    let slices: Vec<SliceClaim> = out.by_ref().take(3).collect();
    (prefix, slices, out.collect())
}

/// Remove only the original REAL header while exercising root admission from an authenticated snapshot.
/// This is a node reader/fold test, not an actual pruning/P2P transport drill; no DAG mining runs while that header is absent.
#[tokio::test]
async fn x8_g14_root_admission_uses_snapshot_origin_when_original_header_is_absent() {
    use crate::model::stores::headers::{HeaderStore, HeaderStoreReader};
    use kaspa_consensus_core::palw_state_v2::{PalwBlockContextV2, PalwStateCarriageV2};
    let mut cw = active_world().await;
    exec_v2_test_admit_class_v1(cw.v2_class, 0);
    let class = cw.v2_class;
    let real = real_attempt(&mut cw, ROOT_CARD, class, 0xA77E).await;
    let initial = palw_exec_v2_token_state_root_v1(&[&real.stream()]);
    let draft = declaration(&cw, &real, Hash64::default(), initial);
    let (prefix, _, _) = kernel_claims(&mut cw, &real, &draft, false, |_| false, Vec::new()).await;
    let mut decl = declaration(&cw, &real, Hash64::from_bytes(prefix.claim.id), initial);
    decl.expiry_daa = draft.expiry_daa;
    decl.signature = sign(ROOT_CARD, decl.signing_message(cw.net.domain).as_byte_slice(), PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, 0x71);
    let object = Obj::ExecWorkRootOpenedV2 { declaration: Box::new(decl) };
    let (tip, state) = cw.net.chain.tip_state();
    let origin = state.exec_v2_origin_v1(&real.claim).unwrap().clone();
    let vp = cw.net.chain.vp();
    let stored = vp.headers_store.get_header_with_block_level(origin.source_block).unwrap();
    assert_eq!(origin.issued_daa, stored.header.daa_score);
    let p = &cw.net.bundle.state;
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap();
    let restored = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(p, Some(state.state_root())).unwrap();
    let point = PalwBlockContextV2 {
        block: tip,
        daa_score: cw.net.daa() + 1,
        blue_score: state.last_point().unwrap().blue_score + 1,
        subsidy: 0,
    };
    assert!(vp.palw_h1_carrier_refusal_on(&state, p, &point, &object).is_none(), "full header positive control");
    let (full, full_delta) = vp.palw_v2_fold_accepted_with_delta_for_tests(&state, p, &point, std::slice::from_ref(&object)).unwrap();
    vp.headers_store.delete(origin.source_block).unwrap();
    assert!(vp.headers_store.get_header(origin.source_block).is_err(), "the original full header really is absent");
    let refusal = vp.palw_h1_carrier_refusal_on(&restored, p, &point, &object);
    let result = vp.palw_v2_fold_accepted_with_delta_for_tests(&restored, p, &point, std::slice::from_ref(&object));
    // Restore the DAG header before any further block processing, including assertion failure teardown.
    vp.headers_store.insert(origin.source_block, stored.header, stored.block_level).unwrap();
    assert!(refusal.is_none(), "snapshot authority must not consult the missing original header: {refusal:?}");
    let (pruned, pruned_delta) = result.unwrap();
    assert_eq!(full.state_root(), pruned.state_root());
    assert_eq!(borsh::to_vec(&full_delta).unwrap(), borsh::to_vec(&pruned_delta).unwrap());
    assert_eq!(
        pruned.exec_v2_job_use_v1(&pruned.exec_v2_root_v1(&real.claim).unwrap().job_work_id).unwrap().issued_daa,
        origin.issued_daa
    );
}

/// Exercise the canonical prefix's context and its continuations without the expensive REAL lottery.
/// This covers kernel admission only; the composed tests below additionally bind an admitted REAL claim.
#[tokio::test]
async fn x8_g14_kernel_claims_accept_a_canonical_prefix_and_its_continuations() {
    let mut cw = active_world().await;
    let identity = Hash64::from_u64_word(0xA77E);
    let facts = cw.net.chain.tip_state().1.tir_class_v1(&cw.v2_class).unwrap().facts.clone();
    let (prompt, context) = palw_exec_v2_real_job_v1(&facts, &identity, cw.net.config.params.palw_prompt_ids_form_at(cw.net.daa()))
        .expect("a canonical public job");
    let run = greedy(&cw.cand, &cw.net.ledger(), &cw.kernel_class, &prompt, context.exact_decode_tokens.max(1) as usize);
    let real = RealRun { claim: Hash64::from_u64_word(0xA77F), prompt, run, executor_exposure_sompi: 0 };
    let initial = palw_exec_v2_token_state_root_v1(&[&real.stream()]);
    let draft = declaration_for_job(&cw, &real, Hash64::default(), initial, 1, identity);
    let (_, slices, _) = kernel_claims(&mut cw, &real, &draft, false, |_| false, Vec::new()).await;
    assert_eq!(slices.len(), 3);
}

/// The fields the slice job nonce binds, for slice `index` of the session `decl` declares (the rest is the claim's to bind).
fn slice_key(
    cw: &Cw,
    real: &RealRun,
    decl: &PalwWorkRootDeclarationV2,
    index: u32,
    range: kaspa_consensus_core::palw_exec_v2::PalwWorkRangeV1,
) -> PalwWorkSliceV1 {
    PalwWorkSliceV1 {
        root_claim_id: real.claim,
        slice_index: index,
        class_id: cw.v2_class,
        canonical_job_id: decl.canonical_job_id,
        kernel_version: decl.kernel_version,
        plan_root: decl.plan_root,
        canonical_range: range,
        predecessor_state_root: Hash64::default(),
        result_state_root: Hash64::default(),
        input_root: Hash64::default(),
        output_root: Hash64::default(),
        evidence_root: Hash64::default(),
        da_root: Hash64::default(),
        executor_bond: cw.net.bond(EXECUTORS[index as usize]),
    }
}

/// **One slice through the lane, judged by the fold however it judges it**: the statement derived from the tip's public rows (the
/// producer's input), an `EXEC_SLICE` block built from the node's own template and signed by the executor, inserted (it never moves
/// the sink), and anchored by the next heartbeat — whose fold judges it against its kernel claim. Returns `(the sink it was inserted
/// under, the lane block, the anchoring block)`.
async fn carry_slice_raw(cw: &mut Cw, root_claim: Hash64, index: u32, claim: &Claim) -> (BlockHash, Block, BlockHash) {
    let card = claim.producer;
    let slice = cw
        .net
        .chain
        .vp()
        .palw_exec_v2_slice_statement_v1_impl(root_claim, index, Hash64::from_bytes(claim.id), cw.net.bond(card))
        .unwrap_or_else(|why| panic!("the slice statement for {index}: {why}"));
    carry_slice_statement_raw(cw, slice, card).await
}

/// An adversary can sign a statement without asking the producer API to validate it first.
async fn carry_slice_statement_raw(cw: &mut Cw, slice: PalwWorkSliceV1, card: usize) -> (BlockHash, Block, BlockHash) {
    let index = slice.slice_index;
    let vp = cw.net.chain.vp();
    let mut template = cw
        .net
        .chain
        .ctx
        .consensus
        .build_block_template(
            MinerData::new(card_payout_spk(card), vec![]),
            Box::new(OnetimeTxSelector::new(Vec::new())),
            TemplateBuildMode::Standard,
        )
        .expect("a template");
    stamp_harness_time(&cw.net.config.params, &mut template.block.header, cw.net.chain.ctx.simulated_time);
    let adapted = vp.exec_v2_slice_adapt_block_template(template, card_payout_spk(card)).expect("an EXEC_SLICE template");
    let anchor = adapted.selected_parent_hash;
    let mut block: MutableBlock = adapted.block;
    block.header.nonce = 0;
    block.header.palw_commitment = Vec::new();
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let mut envelope = PalwExecV2Envelope {
        version: PALW_EXEC_V2_WIRE_VERSION,
        network_domain: cw.net.domain,
        anchor,
        subtype: PalwExecSubtypeV2::Slice,
        tx_permit: None,
        payload_root: palw_work_slice_payload_root_v2(&slice),
        executor_bond: cw.net.bond(card),
        work_slice: Some(slice),
        pubkey: pubkey(card),
        signature: vec![0; kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MLDSA87_SIGNATURE_LEN],
    };
    let message = envelope.signing_message(pre_pow, block.header.timestamp, block.header.nonce).expect("a slice envelope signs");
    envelope.signature = sign(card, message.as_byte_slice(), envelope.mldsa87_context(), 0x72);
    block.header.palw_commitment = envelope.encode();
    block.header.finalize();
    let hash = block.header.hash;
    let sink = cw.net.chain.sink();
    let lane_block = block.to_immutable();
    cw.net
        .chain
        .ctx
        .consensus
        .validate_and_insert_block(lane_block.clone())
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("slice {index}'s lane block was refused: {e}"));
    assert_eq!(cw.net.chain.sink(), sink, "a lane block never moves the sink");
    let ttpb = cw.net.ttpb();
    cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    let anchoring = cw.net.chain.sink();
    assert!(cw.net.chain.tip_state().1.exec_v2_anchored_v1(&hash), "slice {index}'s carrier is anchored");
    (sink, lane_block, anchoring)
}

/// Commit the actual public job and output, retaining the root's planned range even when the
/// execution scope is wrong. Only adversarial tests use this unchecked statement builder.
fn untrusted_slice_statement(cw: &Cw, root_claim: Hash64, index: u32, kernel: &SliceClaim) -> PalwWorkSliceV1 {
    let state = cw.net.chain.tip_state().1;
    let root = state.exec_v2_root_v1(&root_claim).unwrap();
    let ledger = cw.net.ledger();
    let misaka_palw_kernel::ledger::ClaimBodyV1::Program { claim, .. } = &ledger.claims[&kernel.claim.id].body else {
        panic!("a program claim");
    };
    PalwWorkSliceV1 {
        root_claim_id: root_claim,
        slice_index: index,
        class_id: root.class_id,
        canonical_job_id: root.canonical_job_id,
        kernel_version: root.kernel_version,
        plan_root: root.plan_root,
        canonical_range: palw_work_plan_range_v2(&root.boundaries, index).unwrap(),
        predecessor_state_root: palw_exec_v2_token_state_root_v1(&[&kernel.job.prompt]),
        result_state_root: palw_exec_v2_token_state_root_v1(&[&kernel.job.prompt, &claim.generated]),
        input_root: palw_exec_v2_token_state_root_v1(&[&kernel.job.prompt]),
        output_root: palw_exec_v2_token_state_root_v1(&[&claim.generated]),
        evidence_root: Hash64::from_bytes(kernel.claim.id),
        da_root: Hash64::from_bytes(claim.evidence_root),
        executor_bond: cw.net.bond(kernel.claim.producer),
    }
}

/// [`carry_slice_raw`], and the fold admitted it. Returns `(the sink it was inserted under, the lane block)` for the replay.
async fn carry_slice(cw: &mut Cw, root_claim: Hash64, index: u32, claim: &Claim) -> (BlockHash, Block) {
    let (sink, lane_block, _) = carry_slice_raw(cw, root_claim, index, claim).await;
    let state = cw.net.chain.tip_state().1;
    let row = state.exec_v2_slice_v1(&root_claim, index).unwrap_or_else(|| {
        panic!(
            "slice {index} was refused by the fold (root {:?})",
            state.exec_v2_root_v1(&root_claim).map(|r| (&r.phase, r.next_index))
        )
    });
    assert_eq!(row.carrier, lane_block.header.hash);
    assert_eq!(row.evidence_root, Hash64::from_bytes(claim.id), "the slice names its kernel claim");
    (sink, lane_block)
}

/// **A second node replays the first one's chain AND its lane** — each lane block arrives right after the chain block it was inserted
/// under (its anchor and lane parents are then known), so the chain block that anchors it finds its heads, exactly as the IBD's
/// two lists deliver them. (`Net::replay` sends the selected chain alone, which an anchoring block refuses: its heads are missing.)
async fn replay_with_lane(net: &Net, lane: &[(BlockHash, Block)]) -> T12Chain {
    let z = t12_genesis_chain(&net.config, &net.bundle, &net.premine, &net.floats);
    for b in chain_blocks(&net.chain, net.chain.sink()) {
        let hash = b.header.hash;
        arrive(&z, b, "a chain block of the first node").await;
        for (_, l) in lane.iter().filter(|(under, _)| *under == hash) {
            arrive(&z, l.clone(), "a lane block of the first node").await;
        }
    }
    z
}

/// A session under way: the REAL run, its prefix claim, the slices' claims, the off-stream extras and the lane carried so far.
struct Session {
    real: RealRun,
    prefix: SliceClaim,
    slices: Vec<SliceClaim>,
    extras: Vec<SliceClaim>,
    lane: Vec<(BlockHash, Block)>,
}

/// **The REAL claim, its kernel side and the opened root** (no slice carried yet). The prefix claim lies where `lie_prefix`, slice `i`'s
/// where `lie(i)`; `extra` jobs ride along (see [`kernel_claims`]).
async fn opened(
    cw: &mut Cw,
    lie_prefix: bool,
    lie: impl Fn(usize) -> bool,
    extra: impl FnOnce(&Cw, &RealRun, &PalwWorkRootDeclarationV2) -> Vec<(usize, KernelJobV1)>,
) -> Session {
    opened_with_lifetime(cw, lie_prefix, lie, extra, 5_000).await
}

/// Fix the offered expiry before any worker signs. A short real interval lets
/// the abort case reach its actual deadline without changing the network clocks.
async fn opened_with_lifetime(
    cw: &mut Cw,
    lie_prefix: bool,
    lie: impl Fn(usize) -> bool,
    extra: impl FnOnce(&Cw, &RealRun, &PalwWorkRootDeclarationV2) -> Vec<(usize, KernelJobV1)>,
    lifetime_daa: u64,
) -> Session {
    // The seam (module doc): the onboarded class takes REAL work from here. Residual: the legacy channel's Panel admission of a V2 root
    // (GAP-81), which this harness cannot prove.
    exec_v2_test_admit_class_v1(cw.v2_class, 0);
    let class = cw.v2_class;
    let real = real_attempt(cw, ROOT_CARD, class, 0xA77E).await;
    let initial = palw_exec_v2_token_state_root_v1(&[&real.stream()]);
    let mut draft = declaration(cw, &real, Hash64::default(), initial);
    draft.expiry_daa = cw.net.daa().checked_add(lifetime_daa).unwrap();
    let extra = extra(cw, &real, &draft);
    let (prefix, slices, extras) = kernel_claims(cw, &real, &draft, lie_prefix, lie, extra).await;
    let mut decl = declaration(cw, &real, Hash64::from_bytes(prefix.claim.id), initial);
    // Jobs have already been signed: preserve the agreed expiry rather than
    // refreshing it from the later tip while filling the prefix claim id.
    decl.expiry_daa = draft.expiry_daa;
    let root = send_root(cw, decl).await.unwrap_or_else(|| panic!("the session did not open on the REAL claim"));
    assert_eq!(root.phase, PalwWorkRootPhaseV2::Open);
    assert_eq!(root.root_bond, cw.net.bond(ROOT_CARD));
    assert_eq!(root.class_id, cw.v2_class);
    assert_eq!(root.prefix_claim, Hash64::from_bytes(prefix.claim.id), "GAP-62: the session names its prefix claim");
    assert_eq!(root.prefix, PalwWorkPrefixStageV2::Pending, "the prefix claim is live, not yet Final");
    assert_eq!(root.last_state_root, initial, "slice 0 continues the REAL claim's run");
    Session { real, prefix, slices, extras, lane: Vec::new() }
}

/// The REAL claim, the root and the three kernel-claimed slices, all carried and admitted (none verified yet).
async fn session(cw: &mut Cw, lie_prefix: bool, lie: impl Fn(usize) -> bool) -> Session {
    let mut s = opened(cw, lie_prefix, lie, |_, _, _| Vec::new()).await;
    for index in 0..3 {
        let carried = carry_slice(cw, s.real.claim, index as u32, &s.slices[index].claim).await;
        s.lane.push(carried);
    }
    let state = cw.net.chain.tip_state().1;
    let root = state.exec_v2_root_v1(&s.real.claim).unwrap();
    assert_eq!((root.next_index, root.phase.clone()), (3, PalwWorkRootPhaseV2::Complete), "every planned slice admitted");
    assert!(
        !state.claim(&s.real.claim).unwrap().phase.is_terminal(),
        "the REAL claim is live while its slices wait: {:?}",
        state.claim(&s.real.claim).unwrap().phase
    );
    s
}

/// A real signed legacy job has no worker retention consent. Its truthful trace
/// is insufficient to attach it to EXEC; the worker must sign the v2 job instead.
#[tokio::test]
async fn x8_g14_a_signed_legacy_job_cannot_attach_without_worker_retention_terms_on_the_real_node() {
    let mut cw = active_world().await;
    let s = opened(
        &mut cw,
        false,
        |_| false,
        |cw, real, decl| {
            let range = palw_work_plan_range_v2(&decl.boundaries, 0).unwrap();
            vec![(
                EXECUTORS[0],
                KernelJobV1 {
                    class_binding_id: cw.kernel_class,
                    prompt: real.stream(),
                    max_new_tokens: RUN,
                    decode: DecodeRuleV1::Greedy,
                    nonce: palw_exec_v2_slice_job_nonce_v1(&slice_key(cw, real, decl, 0, range)).as_bytes(),
                },
            )]
        },
    )
    .await;
    let native = s.real.claim;
    let legacy = &s.extras[0];
    let before = cw.net.chain.tip_state().1;
    let before_row = cw.net.ledger().claims[&legacy.claim.id].clone();
    for executor in EXECUTORS {
        assert_eq!(before.reserved_exposure(&cw.net.bond(executor)), 0, "a signed root declaration cannot commit another worker");
    }
    assert!(
        cw.net
            .chain
            .vp()
            .palw_exec_v2_slice_statement_v1_impl(native, 0, Hash64::from_bytes(legacy.claim.id), cw.net.bond(legacy.claim.producer))
            .unwrap_err()
            .contains("job nonce")
    );
    let statement = untrusted_slice_statement(&cw, native, 0, legacy);
    let (_, _, anchor) = carry_slice_statement_raw(&mut cw, statement, legacy.claim.producer).await;
    let after = cw.net.chain.tip_state().1;
    assert_eq!(after.exec_v2_root_v1(&native).unwrap().next_index, 0);
    assert!(after.exec_v2_slice_v1(&native, 0).is_none());
    let after_row = &cw.net.ledger().claims[&legacy.claim.id];
    assert_eq!(after_row.liability_until, before_row.liability_until);
    assert_eq!(after_row.reserved, before_row.reserved);
    assert_eq!(after.exec_v2_reward_vesting_v1(&native), before.exec_v2_reward_vesting_v1(&native));
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![native], Some(anchor)).unwrap();
    assert!(read.block.as_ref().unwrap().verdicts.iter().any(|v| v.code == 26));
    let root = &read.roots[0];
    let terms = root.retention_terms.expect("the actual network terms are public before a worker signs");
    let expected = palw_exec_v2_retention_terms_v1(
        after.exec_v2_root_v1(&native).unwrap().expiry_daa,
        after.exec_v2_root_v1(&native).unwrap().executor_exposure,
        cw.net.bundle.state.window_court(),
        &cw.net.ledger().policy,
    )
    .unwrap();
    assert_eq!(terms, expected);
    assert_eq!(root.next_job_nonce.as_deref(), Some(Hash64::from_bytes(s.slices[0].job.nonce).to_string().as_str()));
    let json: serde_json::Value = serde_json::from_str(&read.to_json()).unwrap();
    assert_eq!(json["roots"][0]["retentionTerms"]["liabilityUntil"], terms.liability_until);
    assert_eq!(json["roots"][0]["nextJobNonce"], Hash64::from_bytes(s.slices[0].job.nonce).to_string());
    assert_eq!(json["roots"][0]["retentionTerms"]["executorExposureSompi"], terms.executor_exposure_sompi.to_string());
    carry_slice(&mut cw, native, 0, &s.slices[0].claim).await;
    let consenting = cw.net.chain.tip_state().1;
    assert_eq!(consenting.exec_v2_root_v1(&native).unwrap().next_index, 1);
    assert_eq!(consenting.reserved_exposure(&cw.net.bond(EXECUTORS[0])), terms.executor_exposure_sompi);
    assert_eq!(consenting.reserved_exposure(&cw.net.bond(EXECUTORS[1])), 0);
}

/// **(a) G14 on a slice**: the executor of slice 1 commits a kernel claim whose trace lies (one MatMul value). An outsider — a fresh
/// verifier built from the node's read API and the executor's published DA alone, no Panel, no producer state — files the proof; the
/// route convicts and slashes the executor's real bond; the same block's sync makes slice 1 proven false, voids slice 2 (it chains from
/// slice 1's result) and the root, and voids the REAL claim `WorkSliceProvenFalse` without charging it.
#[tokio::test]
async fn x8_g14_an_outsider_convicts_a_slices_kernel_claim_and_its_suffix_and_root_void_on_the_real_node() {
    let mut cw = active_world().await;
    let s = session(&mut cw, false, |index| index == 1).await;
    let root_claim = s.real.claim;
    let lie = &s.slices[1].claim;
    let before = cw.net.collateral(lie.producer);
    let root_bond_collateral = cw.net.collateral(ROOT_CARD);
    let api = cw.net.api().expect("the read API serves the route");
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x5C);
    let proof = match fresh.check(lie.id, &lie.published(&cw.cand, &[]), &cw.cand.params) {
        OutsiderFindingV1::Prosecute(proof) => proof,
        other => panic!("the fresh verifier should prosecute slice 1's claim: {other:?}"),
    };
    let o = cw.net.route(ACCUSER, &K::FileProof { accuser: cw.net.kid(ACCUSER), claim: lie.id, proof });
    cw.net.send(vec![(ACCUSER, o)]).await;
    assert!(cw.net.ledger().claims[&lie.id].convicted, "the route convicted the slice's kernel claim");
    assert!(cw.net.collateral(lie.producer) < before, "the executor's real bond was slashed");

    let state = cw.net.chain.tip_state().1;
    assert!(
        matches!(state.exec_v2_slice_v1(&root_claim, 0).unwrap().stage, PalwWorkSliceStageV2::Pending),
        "the prefix keeps its stage"
    );
    assert!(matches!(state.exec_v2_slice_v1(&root_claim, 1).unwrap().stage, PalwWorkSliceStageV2::ProvenFalse { .. }));
    assert!(matches!(state.exec_v2_slice_v1(&root_claim, 2).unwrap().stage, PalwWorkSliceStageV2::Voided { .. }), "the suffix voids");
    assert!(matches!(state.exec_v2_root_v1(&root_claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 1, .. }));
    assert!(
        matches!(
            state.claim(&root_claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceProvenFalse, .. }
        ),
        "{:?}",
        state.claim(&root_claim).unwrap().phase
    );
    assert_eq!(cw.net.collateral(ROOT_CARD), root_bond_collateral, "the root bond is not charged for another bond's lie");
    let ledger = cw.net.ledger();
    for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert!(!ledger.exec_reward_holds.contains_key(&k.claim.id), "void releases every reward hold");
        assert!(!ledger.job_escrows.contains_key(&k.job.id()), "void refunds every bound job escrow");
        assert!(!ledger.claims[&k.claim.id].rewarded, "void creates no producer reward");
    }

    // Op 240's read model names it.
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![root_claim], None).expect("the payload is armed");
    assert_eq!(read.roots[0].slices[1].verification_state.as_deref(), Some("Convicted"));
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "replay");
}

/// Truthful worker computations can all finalize and still have zero income:
/// only native root Final authorizes its fee/share settlement. Cancellation
/// refunds the poster while the signed kernel fraud-liability term remains.
#[tokio::test]
async fn x8_g14_completed_worker_claims_earn_nothing_when_the_root_expires_on_the_real_node() {
    let mut cw = active_world().await;
    let mut s = opened_with_lifetime(&mut cw, false, |_| false, |_, _, _| Vec::new(), 256).await;
    let native = s.real.claim;
    for (index, claim) in s.slices.iter().enumerate() {
        s.lane.push(carry_slice(&mut cw, native, index as u32, &claim.claim).await);
    }
    let api = cw.net.api().unwrap();
    let last_final = s
        .slices
        .iter()
        .chain(std::iter::once(&s.prefix))
        .map(|claim| api.claim_read_v1(&claim.claim.id).unwrap().unwrap().opv.unwrap().final_floor_daa)
        .max()
        .unwrap();
    let expiry = cw.net.chain.tip_state().1.exec_v2_root_v1(&native).unwrap().expiry_daa;
    assert!(last_final + 1 < expiry, "the signed expiry must allow all worker computations to finalize");
    cw.net.beat_to(last_final + 1).await;
    let before = cw.net.chain.tip_state().1;
    assert!(before.exec_v2_root_v1(&native).unwrap().ready_for_final());
    assert!(!before.claim(&native).unwrap().phase.is_terminal(), "worker Finals are not native root Final");
    let ledger = cw.net.ledger();
    let poster = cw.net.kid(REGISTRANT);
    let poster_collateral = ledger.bonds[&poster].collateral;
    let poster_reserved = ledger.bonds[&poster].reserved;
    let escrow_sum: u64 = s
        .slices
        .iter()
        .chain(std::iter::once(&s.prefix))
        .map(|claim| {
            let row = &ledger.claims[&claim.claim.id];
            assert!(matches!(row.life.state, ClaimStateV1::Final { .. }));
            assert!(!row.rewarded && row.reserved > 0);
            let escrow = ledger.job_escrows[&claim.job.id()];
            assert_eq!(escrow.poster, poster);
            escrow.amount
        })
        .sum();
    assert!(escrow_sum > 0);
    let worker_collateral: Vec<_> = EXECUTORS.iter().map(|card| (*card, cw.net.collateral(*card))).collect();
    for card in EXECUTORS {
        assert_eq!(
            before.reserved_exposure(&cw.net.bond(card)),
            s.real.executor_exposure_sompi,
            "completed work still holds one native reservation per participating worker"
        );
    }
    // Do not fabricate a native Panel receipt. The root owner never completes
    // that route; its agreed expiry is the real cancellation transition.
    cw.net.beat_to(expiry).await;
    let after = cw.net.chain.tip_state().1;
    assert!(matches!(after.exec_v2_root_v1(&native).unwrap().phase,
        PalwWorkRootPhaseV2::Voided { voided_daa, .. } if voided_daa == expiry));
    assert!(matches!(after.claim(&native).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkRootExpired, .. }));
    assert!(after.vesting_row(&native).is_none() && after.exec_v2_reward_vesting_v1(&native).is_none());
    let ledger = cw.net.ledger();
    assert_eq!(ledger.bonds[&poster].collateral, poster_collateral, "cancellation debits no fee or reward");
    assert!(
        ledger.bonds[&poster].reserved <= poster_reserved - escrow_sum,
        "all four job escrows are returned; unrelated obligations may also have expired"
    );
    for claim in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert!(!ledger.job_escrows.contains_key(&claim.job.id()));
        assert!(!ledger.exec_reward_holds.contains_key(&claim.claim.id));
        let row = &ledger.claims[&claim.claim.id];
        assert!(!row.rewarded, "a truthful worker Final earns no fee after the root cancels");
        assert!(
            row.reserved > 0 && row.liability_until.unwrap() > cw.net.daa(),
            "returning the job escrow does not shorten the worker's signed fraud-liability term"
        );
    }
    for (card, collateral) in worker_collateral {
        assert_eq!(cw.net.collateral(card), collateral, "no worker payment or slashing on this abort");
        assert_eq!(after.reserved_exposure(&cw.net.bond(card)), 0, "native root reservation returns once");
    }
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![native], None).unwrap();
    assert!(read.roots[0].next_job_nonce.is_none());
    let replay = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&replay, "completed work, unpaid root expiry replay");
}

/// **(b) The honest twin**: every kernel claim — the prefix's and each slice's — finalizes after its public window with no Panel; each
/// slice verifies in the block its claim does (its leg cap fixed then), the prefix verifies (GAP-62), the root is ready and the REAL
/// claim's `Final` hold is released. A second node agrees.
#[tokio::test]
async fn x8_g14_honest_slices_verify_through_kernel_finals_and_the_root_is_ready_on_the_real_node() {
    let mut cw = active_world().await;
    let s = session(&mut cw, false, |_| false).await;
    let root_claim = s.real.claim;
    let state = cw.net.chain.tip_state().1;
    assert!(state.exec_v2_holds_final_v1(&root_claim), "the root holds its claim's Final while its slices are unverified");
    let poster_collateral = cw.net.collateral(REGISTRANT);
    let api = cw.net.api().unwrap();
    let ends: Vec<u64> = s
        .slices
        .iter()
        .chain(std::iter::once(&s.prefix))
        .map(|k| api.claim_read_v1(&k.claim.id).unwrap().unwrap().opv.expect("an OPV claim").final_floor_daa)
        .collect();
    cw.net.beat_to(ends.iter().copied().max().unwrap() + 1).await;
    for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert!(matches!(cw.net.claim_state(&k.claim.id), ClaimStateV1::Final { .. }), "{:?}", cw.net.claim_state(&k.claim.id));
    }
    let state = cw.net.chain.tip_state().1;
    let ledger = cw.net.ledger();
    assert_eq!(cw.net.collateral(REGISTRANT), poster_collateral, "kernel Finals cannot debit EXEC fees before root Final");
    for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert!(!ledger.claims[&k.claim.id].rewarded, "EXEC producer has no early reward");
        assert!(ledger.job_escrows.contains_key(&k.job.id()), "poster escrow remains reserved");
        assert_eq!(ledger.exec_reward_holds[&k.claim.id].owner_root, root_claim.as_bytes());
        let root = state.exec_v2_root_v1(&root_claim).unwrap();
        let floor = root.expiry_daa + ledger.policy.liability_daa + ledger.policy.court_deadline_daa + ledger.policy.proof_grace_daa;
        assert!(
            ledger.claims[&k.claim.id].liability_until.unwrap() >= floor,
            "the actual computation Final preserves the root's collectible liability floor"
        );
    }
    let reward = ledger.policy.claim_reward;
    for index in 0..3u32 {
        match state.exec_v2_slice_v1(&root_claim, index).unwrap().stage {
            PalwWorkSliceStageV2::Verified { leg_cap, route_reward, .. } => {
                let row = &cw.net.ledger().claims[&s.slices[index as usize].claim.id];
                let paid = if row.rewarded { reward } else { 0 };
                assert_eq!(
                    leg_cap,
                    row.reserved.saturating_sub(paid),
                    "slice {index}: the cap is the reservation net of the route's reward"
                );
                assert_eq!(route_reward, paid, "slice {index}: the route's reward is snapshotted for D2's netting");
            }
            other => panic!("slice {index} is not verified: {other:?}"),
        }
    }
    let root = state.exec_v2_root_v1(&root_claim).unwrap();
    assert!(matches!(root.prefix, PalwWorkPrefixStageV2::Verified { .. }), "GAP-62: the prefix verified: {:?}", root.prefix);
    assert!(root.ready_for_final(), "{root:?}");
    assert!(!state.exec_v2_holds_final_v1(&root_claim), "the REAL claim's Final hold is released");
    assert!(!state.claim(&root_claim).unwrap().phase.is_terminal(), "{:?}", state.claim(&root_claim).unwrap().phase);
    for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert_eq!(state.exec_v2_root_of_kernel_claim_v1(&Hash64::from_bytes(k.claim.id)), Some(root_claim), "BUDGET's H-3a read");
    }
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "replay");
}

/// **(c) GAP-60: the slice DA default on the real node, and rule 4 at node.**
///
/// * Rule 4: before slice 1, its executor carries a statement backed by a claim of a job whose prompt skips slice 0's run (the right
///   slice job nonce, the wrong stream). The lane block is carried and anchored, and the fold refuses it `PredecessorMismatch` (op 240's
///   refusal record names it); nothing is credited and the root still expects slice 1.
/// * The default: slice 1's executor publishes its claim's DA without one committed position. An outsider's fresh verifier, from the
///   read API and that DA alone, can neither pass nor convict — it demands the position. Nobody answers; at the deadline the route
///   defaults the claim, and the same block's sync defaults slice 1, voids slice 2 and the root, and voids the REAL claim
///   `WorkSliceDefaulted` — no seat is involved (an OPV class has none), and the root bond is charged nothing.
#[tokio::test]
async fn x8_g14_a_withheld_slice_is_demanded_by_an_outsider_and_defaults_its_suffix_and_root_on_the_real_node() {
    let mut cw = active_world().await;
    // The off-stream job: slice 1's nonce, a prompt that skips slice 0's run, claimed by slice 1's executor.
    let mut s = opened(
        &mut cw,
        false,
        |_| false,
        |cw, real, decl| {
            let range = palw_work_plan_range_v2(&decl.boundaries, 1).expect("slice 1");
            let nonce = worker_nonce(cw, real, decl, 1, range);
            vec![(
                EXECUTORS[1],
                KernelJobV1 {
                    class_binding_id: cw.kernel_class,
                    prompt: real.stream(),
                    max_new_tokens: RUN,
                    decode: DecodeRuleV1::Greedy,
                    nonce: nonce.as_bytes(),
                },
            )]
        },
    )
    .await;
    let root_claim = s.real.claim;
    let carried = carry_slice(&mut cw, root_claim, 0, &s.slices[0].claim).await;
    s.lane.push(carried);
    // ---- rule 4 at node ----
    assert!(
        cw.net
            .chain
            .vp()
            .palw_exec_v2_slice_statement_v1_impl(
                root_claim,
                1,
                Hash64::from_bytes(s.extras[0].claim.id),
                cw.net.bond(s.extras[0].claim.producer)
            )
            .is_err(),
        "the producer refuses the off-stream execution before publishing"
    );
    let raw = untrusted_slice_statement(&cw, root_claim, 1, &s.extras[0]);
    let (under, lane_block, anchoring) = carry_slice_statement_raw(&mut cw, raw, s.extras[0].claim.producer).await;
    s.lane.push((under, lane_block.clone()));
    let state = cw.net.chain.tip_state().1;
    assert!(state.exec_v2_slice_v1(&root_claim, 1).is_none(), "the discontinuous slice is credited nothing");
    assert_eq!(state.exec_v2_root_v1(&root_claim).unwrap().next_index, 1, "the root still expects slice 1");
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![root_claim], Some(anchoring)).expect("armed");
    let verdicts = read.block.expect("the anchoring block's refusal record").verdicts;
    assert!(
        verdicts
            .iter()
            .any(|v| v.carrier == lane_block.header.hash.to_string() && v.code == PalwSliceRefusalV2::PredecessorMismatch.code()),
        "refused by rule 4, by name: {verdicts:?}"
    );
    // ---- the honest slices 1 and 2 ----
    for index in 1..3 {
        let carried = carry_slice(&mut cw, root_claim, index as u32, &s.slices[index].claim).await;
        s.lane.push(carried);
    }
    // ---- the default ----
    let withheld = &s.slices[1].claim;
    let executor_before = cw.net.collateral(withheld.producer);
    let root_bond_collateral = cw.net.collateral(ROOT_CARD);
    let api = cw.net.api().expect("the read API serves the route");
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x6D);
    let finding = fresh.check(withheld.id, &withheld.published(&cw.cand, &[withheld.at]), &cw.cand.params);
    assert_eq!(finding, OutsiderFindingV1::Demand(vec![(0, withheld.at.0)]), "no pass, no conviction: one position to demand");
    let o =
        cw.net.route(ACCUSER, &K::FileDemand { demander: cw.net.kid(ACCUSER), claim: withheld.id, stage: 0, position: withheld.at.0 });
    cw.net.send(vec![(ACCUSER, o)]).await;
    let deadline = cw.net.ledger().demands[&(withheld.id, 0, withheld.at.0)].deadline_daa;
    cw.net.beat_to(deadline).await;
    assert!(
        matches!(cw.net.claim_state(&withheld.id), ClaimStateV1::Unavailable { producer_defaulted: true, .. }),
        "the route's default: {:?}",
        cw.net.claim_state(&withheld.id)
    );
    assert!(cw.net.collateral(withheld.producer) < executor_before, "the executor pays the route's default penalty");
    let state = cw.net.chain.tip_state().1;
    assert!(matches!(state.exec_v2_slice_v1(&root_claim, 1).unwrap().stage, PalwWorkSliceStageV2::Defaulted { .. }));
    assert!(matches!(state.exec_v2_slice_v1(&root_claim, 2).unwrap().stage, PalwWorkSliceStageV2::Voided { .. }), "the suffix voids");
    assert!(matches!(state.exec_v2_root_v1(&root_claim).unwrap().phase, PalwWorkRootPhaseV2::Voided { from_index: 1, .. }));
    assert!(
        matches!(
            state.claim(&root_claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceDefaulted, .. }
        ),
        "{:?}",
        state.claim(&root_claim).unwrap().phase
    );
    assert_eq!(cw.net.collateral(ROOT_CARD), root_bond_collateral, "the root bond is not charged for another bond's default");
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "replay");
}

#[tokio::test]
async fn x8_g14_a_signed_tiny_run_is_refused_as_a_larger_work_cell_on_the_real_node() {
    let mut cw = active_world().await;
    let mut s = opened(
        &mut cw,
        false,
        |_| false,
        |cw, real, decl| {
            let range = palw_work_plan_range_v2(&decl.boundaries, 0).unwrap();
            vec![(
                EXECUTORS[0],
                KernelJobV1 {
                    class_binding_id: cw.kernel_class,
                    prompt: real.stream(),
                    max_new_tokens: RUN - 1,
                    decode: DecodeRuleV1::Greedy,
                    nonce: worker_nonce(cw, real, decl, 0, range).as_bytes(),
                },
            )]
        },
    )
    .await;
    let root_claim = s.real.claim;
    let tiny = &s.extras[0];
    assert!(
        cw.net
            .chain
            .vp()
            .palw_exec_v2_slice_statement_v1_impl(root_claim, 0, Hash64::from_bytes(tiny.claim.id), cw.net.bond(tiny.claim.producer))
            .is_err(),
        "the producer refuses the short kernel run"
    );
    let raw = untrusted_slice_statement(&cw, root_claim, 0, tiny);
    let (under, lane_block, anchoring) = carry_slice_statement_raw(&mut cw, raw, tiny.claim.producer).await;
    s.lane.push((under, lane_block.clone()));
    let state = cw.net.chain.tip_state().1;
    assert!(state.exec_v2_slice_v1(&root_claim, 0).is_none(), "the signed attack earns no work");
    let root = state.exec_v2_root_v1(&root_claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.verified_work), (0, 0, 0));
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![root_claim], Some(anchoring)).unwrap();
    assert!(
        read.block
            .unwrap()
            .verdicts
            .iter()
            .any(|v| v.carrier == lane_block.header.hash.to_string() && v.code == PalwSliceRefusalV2::ExecutionScopeMismatch.code()),
        "the validator refuses the execution scope by name"
    );
    let carried = carry_slice(&mut cw, root_claim, 0, &s.slices[0].claim).await;
    s.lane.push(carried);
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "replay of refused tiny run and honest full cell");
}

/// **(d) GAP-62: the initial boundary is linked to the REAL output, and a lie in the REAL run is an outsider's conviction.**
///
/// * A declaration whose initial boundary is the anchored prompt alone (not the REAL claim's run) is dropped by the fold: no root.
/// * The root bond's prefix claim — the public carrier of the REAL claim's run — lies (one MatMul value). An outsider's fresh verifier,
///   from the read API and the root bond's published DA alone, convicts it through the route; the route slashes the root bond's real
///   bond, and the same block's sync voids the whole session: the prefix proven false, every slice void, the root
///   `Voided { from_index: 0 }`, the REAL claim `WorkSliceProvenFalse`. Op 240 names the prefix claim `Convicted`.
#[tokio::test]
async fn x8_g14_a_lying_real_run_is_convicted_through_its_prefix_claim_and_voids_the_whole_session_on_the_real_node() {
    let mut cw = active_world().await;
    exec_v2_test_admit_class_v1(cw.v2_class, 0);
    let class = cw.v2_class;
    let real = real_attempt(&mut cw, ROOT_CARD, class, 0xD06E).await;
    let initial = palw_exec_v2_token_state_root_v1(&[&real.stream()]);
    let draft = declaration(&cw, &real, Hash64::default(), initial);
    let (prefix, slices, _) = kernel_claims(&mut cw, &real, &draft, true, |_| false, Vec::new()).await;
    let prefix_id = Hash64::from_bytes(prefix.claim.id);
    // A boundary the root bond chose — the prompt without the run — is not the REAL output: the fold drops it.
    let mut wrong = declaration(&cw, &real, prefix_id, palw_exec_v2_token_state_root_v1(&[&real.prompt]));
    wrong.expiry_daa = draft.expiry_daa;
    assert!(send_root(&mut cw, wrong).await.is_none(), "a wrong initial boundary opens nothing");
    let mut linked = declaration(&cw, &real, prefix_id, initial);
    linked.expiry_daa = draft.expiry_daa;
    let root = send_root(&mut cw, linked).await.expect("the linked boundary opens");
    assert_eq!(root.prefix, PalwWorkPrefixStageV2::Pending);
    let mut lane = Vec::new();
    for (index, k) in slices.iter().enumerate() {
        lane.push(carry_slice(&mut cw, real.claim, index as u32, &k.claim).await);
    }
    let root_bond_before = cw.net.collateral(ROOT_CARD);
    let api = cw.net.api().expect("the read API serves the route");
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x7E);
    let proof = match fresh.check(prefix.claim.id, &prefix.claim.published(&cw.cand, &[]), &cw.cand.params) {
        OutsiderFindingV1::Prosecute(proof) => proof,
        other => panic!("the fresh verifier should prosecute the prefix claim: {other:?}"),
    };
    let o = cw.net.route(ACCUSER, &K::FileProof { accuser: cw.net.kid(ACCUSER), claim: prefix.claim.id, proof });
    cw.net.send(vec![(ACCUSER, o)]).await;
    assert!(cw.net.ledger().claims[&prefix.claim.id].convicted, "the route convicted the prefix claim");
    assert!(cw.net.collateral(ROOT_CARD) < root_bond_before, "the root bond's real bond was slashed by the route");
    let state = cw.net.chain.tip_state().1;
    let root = state.exec_v2_root_v1(&real.claim).unwrap();
    assert!(matches!(root.prefix, PalwWorkPrefixStageV2::ProvenFalse { .. }), "{:?}", root.prefix);
    assert!(matches!(root.phase, PalwWorkRootPhaseV2::Voided { from_index: 0, .. }), "{:?}", root.phase);
    for index in 0..3 {
        assert!(matches!(state.exec_v2_slice_v1(&real.claim, index).unwrap().stage, PalwWorkSliceStageV2::Voided { .. }));
    }
    assert!(
        matches!(
            state.claim(&real.claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceProvenFalse, .. }
        ),
        "{:?}",
        state.claim(&real.claim).unwrap().phase
    );
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![real.claim], None).expect("armed");
    assert_eq!(read.roots[0].prefix_verification_state.as_deref(), Some("Convicted"));
    let z = replay_with_lane(&cw.net, &lane).await;
    cw.net.assert_same(&z, "replay");
}
