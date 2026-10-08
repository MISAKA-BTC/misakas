//! **RFC-0008 v2 × G14 on the real node: EXEC slice carriage preserves G14** — the composed run of the implementation record's section
//! 12, item 1 (lane X8R).
//!
//! ```text
//! the conformance world (Cw): TIR V2 class → 104 bind → OPV kernel class → 106 kernel-bind → 107 commit → beacon locks on future
//!   OPV Finals → 109 evidence → window closes unrefuted → V2 class Active (all through blocks)
//! → a REAL attempt on that class (card 0, the node's own template, the attempt lane)            [SEAM: see below]
//! → tag 130: a work session on the REAL claim, its plan the binding's kernel plan
//! → per slice: a kernel job carrying the slice job nonce (PostJob), the executor's OPV claim of it (seal, reveal), the slice statement
//!   derived from public rows (`palw_exec_v2_slice_statement_v1`, the producer's input), an EXEC_SLICE lane block signed by the
//!   executor, anchored by the next heartbeat and admitted by the fold against the kernel claim (the verification binding)
//! → (a) an outsider, from the read API and the producer's published DA alone, convicts slice 1's lying kernel claim through the route:
//!       slice 1 proven false, slice 2 void, the root void, the REAL claim void (`WorkSliceProvenFalse`), nothing charged at V2;
//!   (b) the honest twin: every kernel claim finalizes with no Panel after its public window, every slice verifies (its leg cap fixed),
//!       the root is ready and the REAL claim's Final hold is released;
//! → a second node replaying the chain agrees on every root.
//! ```
//!
//! **The seam (named, not hidden).** Everything above runs through blocks except one door: the onboarded class takes the REAL attempt
//! because the processor's `cfg(test)` seam [`exec_v2_test_admit_class_v1`] waives the model-registry lifecycle and room, the seating
//! and the per-bond share gates for that one class (the way `kernel_route_test_attest_artifact_v1` stands in for artifact
//! availability). **Residual: an onboarded class admitting REAL work under its real budget = the G14-for-rewards gap** (readiness
//! matrix; assigned to OPVB with derived eligibility). When that lands, the seam is swapped for it. The slice and route half — the
//! carriage, the binding, the kernel claims, the outsider's conviction, the OPV Finals, the sync — is the real node's, unseamed.
//!
//! Not run here (and why): the REAL root claim's own `Final` and the capped settlement need the root class's Panel (REAL's lifecycle),
//! which an onboarded class does not have on this harness — the same gap; the settlement is pinned at fold level
//! (`exec_v2_fold_v1::amendment_1`).
use super::*;
use crate::pipeline::virtual_processor::processor::exec_v2_test_admit_class_v1;
use crate::pipeline::virtual_processor::tests::OnetimeTxSelector;
use crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::stamp_harness_time;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::block::{MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::palw_exec_v2::{
    PALW_EXEC_V2_WIRE_VERSION, PalwExecSubtypeV2, PalwExecV2Envelope, PalwWorkSliceV1, palw_work_slice_payload_root_v2,
};
use kaspa_consensus_core::palw_exec_v2_verify::{palw_exec_v2_slice_job_nonce_v1, palw_exec_v2_token_state_root_v1};
use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwVoidReasonV2};
use kaspa_consensus_core::palw_work_slice_v2::{
    PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, PalwWorkRootDeclarationV2, PalwWorkRootPhaseV2, PalwWorkRootV2, PalwWorkSliceStageV2,
    palw_work_plan_range_v2,
};

/// The EXEC v2 payload's fence: low, crossed long before the class activates.
const EXEC_FENCE: u64 = 6;
/// The candidate's fixture seed: a class of its own, so the seam waives nothing another test of this process asks about.
const SEED: u64 = 23;
/// The REAL root's producer (and so the root bond).
const ROOT_CARD: usize = 0;
/// The slices' executors (the declaration's extra executors), by slice index; none produced a beacon source claim.
const EXECUTORS: [usize; 3] = [5, 6, 5];
/// The outsider: no executor, not the registrant, not the root bond.
const ACCUSER: usize = 7;
/// Canonical work per slice.
const SLICE_WORK: u64 = 1_000;
/// Tokens each slice's kernel claim generates.
const RUN: u32 = 2;
/// The session's prompt: the token stream slice 0 continues.
const PROMPT: [u32; 3] = [5, 11, 2];

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

/// **A REAL attempt of `class` by card `card`**: the node's own template, the attempt lane, the class lottery won, signed by the card.
/// The facts are the node's answer for the class (`palw_producer_facts_v2`); their readiness is printed, not asked — the seam is what
/// admits the class (module doc).
async fn real_attempt(net: &mut Net, card: usize, class: Hash64, nonce: u64) -> Hash64 {
    use kaspa_consensus_core::palw_attempt_v2::{
        PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
        PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
    };
    let step = net.ttpb();
    net.chain.ctx.simulated_time += step;
    let bond = net.bond(card);
    let mut t = net
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
    stamp_harness_time(&net.config.params, &mut t.block.header, net.chain.ctx.simulated_time);
    t.block.header.nonce = nonce;
    let facts = net.chain.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("the node answers for the class");
    eprintln!(
        "[x8-g14] REAL attempt on {class}: ready = {:?}, class gate = {:?}, pwu = {}",
        facts.ready_to_produce(&pubkey(card)),
        facts.class_admission_refusal,
        facts.pwu
    );
    let bond_facts = facts.bond.as_ref().expect("a genesis card").clone();
    let header = &t.block.header;
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
    let mut attempt = PalwAttemptUnsignedV2 {
        version: PALW_ATTEMPT_V2_VERSION,
        network_domain: net.domain,
        challenge: challenge_v2(net.domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
        class_id: facts.class_id,
        executor_bond: bond.0,
        executor_pubkey: pubkey(card),
        operator_id: bond_facts.operator_id,
        artifact_root: facts.artifact_root,
        trace_root: Hash64::default(),
        output_root: Hash64::from_u64_word(0x0078_0000_0000_0000 | nonce),
        execution_root: Hash64::from_u64_word(0xE8EC_0000_0000_0000 | nonce),
        pwu: facts.pwu,
        trace_manifest_root: Hash64::default(),
        trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
        trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
    };
    let anchor = execution_anchor_v3(net.domain, pre_pow, facts.class_id, &bond.0, header.nonce);
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
    net.chain
        .ctx
        .consensus
        .validate_and_insert_block(block)
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("the REAL attempt on the onboarded class was refused: {e}"));
    assert_eq!(net.chain.sink(), hash, "the attempt is the sink");
    let state = net.chain.tip_state().1;
    let row = state.claim(&claim).unwrap_or_else(|| panic!("the fold took no claim of the onboarded class (block {hash})"));
    assert!(matches!(row.phase, PalwClaimPhaseV2::Provisional), "a fresh REAL claim: {:?}", row.phase);
    claim
}

/// The session one REAL claim opens: three slices of `SLICE_WORK` after the claim's own admitted work, the binding's kernel plan, the
/// prompt's token state as the initial boundary, executors `EXECUTORS`, signed by the root card and carried by a 0x4b transaction.
async fn open_root(cw: &mut Cw, claim: Hash64) -> PalwWorkRootV2 {
    let state = cw.net.chain.tip_state().1;
    let row = state.claim(&claim).expect("the claim").clone();
    let binding = cw.net.api().unwrap().kernel_binding_v1(&cw.v2_class).expect("kernel-bound");
    let prefix = row.pwu;
    let mut extra: Vec<PalwBondKeyV2> = EXECUTORS.iter().map(|card| cw.net.bond(*card)).collect();
    extra.sort();
    extra.dedup();
    let mut declaration = PalwWorkRootDeclarationV2 {
        root_claim_id: claim,
        canonical_job_id: row.job_identity,
        input_root: palw_exec_v2_token_state_root_v1(&[&PROMPT]),
        kernel_version: 1,
        plan_root: binding.plan_root,
        total_work: prefix + 3 * SLICE_WORK,
        boundaries: vec![prefix, prefix + SLICE_WORK, prefix + 2 * SLICE_WORK, prefix + 3 * SLICE_WORK],
        initial_state_root: palw_exec_v2_token_state_root_v1(&[&PROMPT]),
        evidence_policy_root: Hash64::from_u64_word(0x34),
        extra_executors: extra,
        expiry_daa: cw.net.daa() + 5_000,
        signature: Vec::new(),
    };
    declaration.signature =
        sign(ROOT_CARD, declaration.signing_message(cw.net.domain).as_byte_slice(), PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, 0x71);
    let carrier = cw.net.carrier(ROOT_CARD, &Obj::ExecWorkRootOpenedV2 { declaration: Box::new(declaration) });
    let ttpb = cw.net.ttpb();
    let carrying = cw.net.chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the declaration's carrier is in the block");
    cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    let state = cw.net.chain.tip_state().1;
    state.exec_v2_root_v1(&claim).cloned().unwrap_or_else(|| {
        panic!(
            "the session did not open on the REAL claim (phase {:?}, binding plan {})",
            state.claim(&claim).map(|c| &c.phase),
            binding.plan_root
        )
    })
}

/// One slice's kernel side: its job (the slice job nonce, the stream so far as the prompt), the executor's claim of it.
struct SliceClaim {
    job: KernelJobV1,
    claim: Claim,
}

/// **The kernel jobs and claims of the session's slices**: each job's prompt is the stream so far, its nonce the slice job nonce of
/// `(root, index, range, job, plan)`; each claim is `EXECUTORS[i]`'s honest greedy run, its committed trace lying where `lie(i)`. Posted
/// by the registrant, sealed in one block and revealed in the next — through the route, as any producer.
async fn slice_claims(cw: &mut Cw, root_claim: Hash64, root: &PalwWorkRootV2, lie: impl Fn(usize) -> bool) -> Vec<SliceClaim> {
    let kernel_class = cw.kernel_class;
    let ledger = cw.net.ledger();
    let mut stream = PROMPT.to_vec();
    let mut planned = Vec::new();
    for index in 0..3u32 {
        let range = palw_work_plan_range_v2(&root.boundaries, index).expect("a planned slice");
        let nonce = palw_exec_v2_slice_job_nonce_v1(&PalwWorkSliceV1 {
            root_claim_id: root_claim,
            slice_index: index,
            class_id: root.class_id,
            canonical_job_id: root.canonical_job_id,
            kernel_version: root.kernel_version,
            plan_root: root.plan_root,
            canonical_range: range,
            predecessor_state_root: Hash64::default(),
            result_state_root: Hash64::default(),
            input_root: Hash64::default(),
            output_root: Hash64::default(),
            evidence_root: Hash64::default(),
            da_root: Hash64::default(),
            executor_bond: cw.net.bond(EXECUTORS[index as usize]),
        });
        let job = KernelJobV1 {
            class_binding_id: kernel_class,
            prompt: stream.clone(),
            max_new_tokens: RUN,
            decode: DecodeRuleV1::Greedy,
            nonce: nonce.as_bytes(),
        };
        let generated = greedy(&cw.cand, &ledger, &kernel_class, &job.prompt, RUN as usize);
        stream.extend_from_slice(&generated);
        planned.push((job, generated));
    }
    let posts: Vec<(usize, Obj)> =
        planned.iter().map(|(job, _)| (REGISTRANT, cw.net.route(REGISTRANT, &K::PostJob { job: job.clone() }))).collect();
    cw.net.send(posts).await;
    let mut seals = Vec::new();
    let mut reveals = Vec::new();
    let mut out = Vec::new();
    for (index, (job, generated)) in planned.into_iter().enumerate() {
        let producer = EXECUTORS[index];
        let at = matmul_at(&cw.cand.program, 1);
        let produced = produce(&cw.cand, &ledger, &kernel_class, &job, cw.net.kid(producer), generated, |t| {
            if lie(index) {
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
        assert!(ledger.claims.contains_key(&s.claim.id), "slice claim {} committed", Hash64::from_bytes(s.claim.id));
    }
    out
}

/// **One slice through the lane**: the statement derived from the tip's public rows (the producer's input), an `EXEC_SLICE` block built
/// from the node's own template and signed by the executor, inserted (it never moves the sink), and anchored by the next heartbeat —
/// whose fold judges it against its kernel claim. Returns `(the sink it was inserted under, the lane block)` for the replay.
async fn carry_slice(cw: &mut Cw, root_claim: Hash64, index: u32, claim: &Claim) -> (BlockHash, Block) {
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
    let state = cw.net.chain.tip_state().1;
    assert!(state.exec_v2_anchored_v1(&hash), "slice {index}'s carrier is anchored");
    let row = state.exec_v2_slice_v1(&root_claim, index).unwrap_or_else(|| {
        panic!(
            "slice {index} was refused by the fold (root {:?})",
            state.exec_v2_root_v1(&root_claim).map(|r| (&r.phase, r.next_index))
        )
    });
    assert_eq!(row.carrier, hash);
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

/// The REAL claim, the root and the three kernel-claimed slices, all carried and admitted (none verified yet).
async fn session(cw: &mut Cw, lie: impl Fn(usize) -> bool) -> (Hash64, Vec<SliceClaim>, Vec<(BlockHash, Block)>) {
    // The seam (module doc): the onboarded class takes REAL work from here. Residual: an onboarded class admitting REAL work under its
    // real budget = the G14-for-rewards gap.
    exec_v2_test_admit_class_v1(cw.v2_class, 0);
    let root_claim = real_attempt(&mut cw.net, ROOT_CARD, cw.v2_class, 0xA77E).await;
    let root = open_root(cw, root_claim).await;
    assert_eq!(root.phase, PalwWorkRootPhaseV2::Open);
    assert_eq!(root.root_bond, cw.net.bond(ROOT_CARD));
    assert_eq!(root.class_id, cw.v2_class);
    let slices = slice_claims(cw, root_claim, &root, lie).await;
    let mut lane = Vec::new();
    for (index, s) in slices.iter().enumerate() {
        lane.push(carry_slice(cw, root_claim, index as u32, &s.claim).await);
    }
    let state = cw.net.chain.tip_state().1;
    let root = state.exec_v2_root_v1(&root_claim).unwrap();
    assert_eq!((root.next_index, root.phase.clone()), (3, PalwWorkRootPhaseV2::Complete), "every planned slice admitted");
    assert!(
        !state.claim(&root_claim).unwrap().phase.is_terminal(),
        "the REAL claim is live while its slices wait: {:?}",
        state.claim(&root_claim).unwrap().phase
    );
    (root_claim, slices, lane)
}

/// **(a) G14 on a slice**: the executor of slice 1 commits a kernel claim whose trace lies (one MatMul value). An outsider — a fresh
/// verifier built from the node's read API and the executor's published DA alone, no Panel, no producer state — files the proof; the
/// route convicts and slashes the executor's real bond; the same block's sync makes slice 1 proven false, voids slice 2 (it chains from
/// slice 1's result) and the root, and voids the REAL claim `WorkSliceProvenFalse` without charging it.
#[tokio::test]
async fn x8_g14_an_outsider_convicts_a_slices_kernel_claim_and_its_suffix_and_root_void_on_the_real_node() {
    let mut cw = active_world().await;
    let (root_claim, slices, lane) = session(&mut cw, |index| index == 1).await;
    let lie = &slices[1].claim;
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
    let z = replay_with_lane(&cw.net, &lane).await;
    cw.net.assert_same(&z, "replay");
}

/// **(b) The honest twin**: every slice's kernel claim finalizes after its public window with no Panel; each slice verifies in the block
/// its claim does (its leg cap fixed then); the root is ready and the REAL claim's `Final` hold is released. A second node agrees.
#[tokio::test]
async fn x8_g14_honest_slices_verify_through_kernel_finals_and_the_root_is_ready_on_the_real_node() {
    let mut cw = active_world().await;
    let (root_claim, slices, lane) = session(&mut cw, |_| false).await;
    let state = cw.net.chain.tip_state().1;
    assert!(state.exec_v2_holds_final_v1(&root_claim), "the root holds its claim's Final while its slices are unverified");
    let api = cw.net.api().unwrap();
    let ends: Vec<u64> =
        slices.iter().map(|s| api.claim_read_v1(&s.claim.id).unwrap().unwrap().opv.expect("an OPV claim").final_floor_daa).collect();
    cw.net.beat_to(ends.iter().copied().max().unwrap() + 1).await;
    for s in &slices {
        assert!(matches!(cw.net.claim_state(&s.claim.id), ClaimStateV1::Final { .. }), "{:?}", cw.net.claim_state(&s.claim.id));
    }
    let state = cw.net.chain.tip_state().1;
    let reward = cw.net.ledger().policy.claim_reward;
    for index in 0..3u32 {
        match state.exec_v2_slice_v1(&root_claim, index).unwrap().stage {
            PalwWorkSliceStageV2::Verified { leg_cap, .. } => {
                let reserved = cw.net.ledger().claims[&slices[index as usize].claim.id].reserved;
                assert_eq!(
                    leg_cap,
                    reserved.saturating_sub(reward),
                    "slice {index}: the cap is the reservation net of the route's reward"
                );
            }
            other => panic!("slice {index} is not verified: {other:?}"),
        }
    }
    let root = state.exec_v2_root_v1(&root_claim).unwrap();
    assert!(root.ready_for_final(), "{root:?}");
    assert!(!state.exec_v2_holds_final_v1(&root_claim), "the REAL claim's Final hold is released");
    assert!(!state.claim(&root_claim).unwrap().phase.is_terminal(), "{:?}", state.claim(&root_claim).unwrap().phase);
    let z = replay_with_lane(&cw.net, &lane).await;
    cw.net.assert_same(&z, "replay");
}
