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
//! Not run here (and why): the REAL root claim's own `Final` and the capped, netted settlement need the root class's Panel (the
//! legacy channel), which an onboarded class does not have on this harness; the settlement is pinned at fold level
//! (`exec_v2_fold_v1::amendment_1`).
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
    palw_exec_v2_prefix_job_nonce_v1, palw_exec_v2_real_job_v1, palw_exec_v2_slice_job_nonce_v1, palw_exec_v2_token_state_root_v1,
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
/// Canonical work per slice.
const SLICE_WORK: u64 = 1_000;
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
    RealRun { claim, prompt, run }
}

/// The session one REAL claim opens: three slices of `SLICE_WORK` after the claim's own admitted work, the binding's kernel plan, the
/// prefix claim `prefix` and the initial boundary `initial` (GAP-62: `token_state(prompt ‖ REAL run)`, unless a test lies), executors
/// `EXECUTORS`. Unsigned and uncarried: [`send_root`] carries it.
fn declaration(cw: &Cw, real: &RealRun, prefix: Hash64, initial: Hash64) -> PalwWorkRootDeclarationV2 {
    let state = cw.net.chain.tip_state().1;
    let row = state.claim(&real.claim).expect("the claim").clone();
    let binding = cw.net.api().unwrap().kernel_binding_v1(&cw.v2_class).expect("kernel-bound");
    let prefix_work = row.pwu;
    let mut extra: Vec<PalwBondKeyV2> = EXECUTORS.iter().map(|card| cw.net.bond(*card)).collect();
    extra.sort();
    extra.dedup();
    PalwWorkRootDeclarationV2 {
        root_claim_id: real.claim,
        canonical_job_id: row.job_identity,
        input_root: palw_exec_v2_token_state_root_v1(&[&real.prompt]),
        kernel_version: 1,
        plan_root: binding.plan_root,
        total_work: prefix_work + 3 * SLICE_WORK,
        boundaries: vec![prefix_work, prefix_work + SLICE_WORK, prefix_work + 2 * SLICE_WORK, prefix_work + 3 * SLICE_WORK],
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

/// **The kernel side of a session, all through the route as any producer**: the prefix job (the anchored prompt, the prefix job
/// nonce) and the three slice jobs (each prompt the stream so far, each nonce the slice job nonce of `(root, index, range, job, plan)`),
/// posted by the registrant in one block; the root card's prefix claim of the REAL run (lying where `lie_prefix`) and each
/// `EXECUTORS[i]`'s honest greedy run (lying where `lie(i)`), sealed in one block and revealed in the next. `extra` further jobs — a
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
        let nonce = palw_exec_v2_slice_job_nonce_v1(&slice_key(cw, real, decl, index, range));
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
        seals.push((
            producer,
            cw.net.route(producer, &K::SealClaim { producer: cw.net.kid(producer), job: job.id(), seal: claim_seal_v1(&id) }),
        ));
        reveals.push((producer, produced.object));
        out.push(SliceClaim { job, claim: Claim { id, producer, trace: produced.trace, at } });
    }
    cw.net.send(seals).await;
    let reveals: Vec<(usize, Obj)> = reveals.into_iter().map(|(p, o)| (p, cw.net.route(p, &o))).collect();
    cw.net.send(reveals).await;
    let ledger = cw.net.ledger();
    for s in &out {
        assert!(ledger.claims.contains_key(&s.claim.id), "kernel claim {} committed", Hash64::from_bytes(s.claim.id));
    }
    let mut out = out.into_iter();
    let prefix = out.next().expect("the prefix claim");
    let slices: Vec<SliceClaim> = out.by_ref().take(3).collect();
    (prefix, slices, out.collect())
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
    let vp = cw.net.chain.vp();
    let slice = vp
        .palw_exec_v2_slice_statement_v1_impl(root_claim, index, Hash64::from_bytes(claim.id), cw.net.bond(card))
        .unwrap_or_else(|why| panic!("the slice statement for {index}: {why}"));
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
    // The seam (module doc): the onboarded class takes REAL work from here. Residual: the legacy channel's Panel admission of a V2 root
    // (GAP-81), which this harness cannot prove.
    exec_v2_test_admit_class_v1(cw.v2_class, 0);
    let class = cw.v2_class;
    let real = real_attempt(cw, ROOT_CARD, class, 0xA77E).await;
    let initial = palw_exec_v2_token_state_root_v1(&[&real.stream()]);
    let draft = declaration(cw, &real, Hash64::default(), initial);
    let extra = extra(cw, &real, &draft);
    let (prefix, slices, extras) = kernel_claims(cw, &real, &draft, lie_prefix, lie, extra).await;
    let decl = declaration(cw, &real, Hash64::from_bytes(prefix.claim.id), initial);
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
    // Op 240's read model names it.
    let read = cw.net.chain.ctx.consensus.palw_exec_v2_observation_v1(vec![root_claim], None).expect("the payload is armed");
    assert_eq!(read.roots[0].slices[1].verification_state.as_deref(), Some("Convicted"));
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "replay");
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
    let reward = cw.net.ledger().policy.claim_reward;
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
            let nonce = palw_exec_v2_slice_job_nonce_v1(&slice_key(cw, real, decl, 1, range));
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
    let (under, lane_block, anchoring) = carry_slice_raw(&mut cw, root_claim, 1, &s.extras[0].claim).await;
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
    let wrong = declaration(&cw, &real, prefix_id, palw_exec_v2_token_state_root_v1(&[&real.prompt]));
    assert!(send_root(&mut cw, wrong).await.is_none(), "a wrong initial boundary opens nothing");
    let linked = declaration(&cw, &real, prefix_id, initial);
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
