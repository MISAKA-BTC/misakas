//! **A gateway request all the way into a consensus fold** (RFC-0001 §A + §2.7; delivery task 7): the t12 free-prompt end-to-end pattern
//! (`misaka-palw-base0/tests/fp_job_v4_t12_e2e.rs`) with the producer side replaced by the GATEWAY. In process, no node, no network:
//!
//! ```text
//!  POST body ─▶ admission ─▶ prepare_request ─▶ worker (the real BASE-0 engine) ─▶ caller-side bindings ─▶ evidence ─▶ outbox commitment
//!        (gateway: normalization, FP V3 or V4)                                                                              │
//!                                                                                                                  rail (fp-submit)
//!  claim visible in the fold ◀─ apply_palw_transition_v7 ◀─ extraction walk (stateless door) ◀─ signed-shaped 0x4a tx ◀── plan + handoff
//!        │
//!        └─▶ the node's claim row ─▶ receipt::check_against_chain, status (never final), a seat's replay: Valid
//! ```
//!
//! testnet-12's shipped ruleset for the V3 (live) path; the same copy with `palw_fp_decode_rules` armed at a low height — the flag-day entry
//! the integration adds — for V4. The class is the BASE-0 floor, the one class both the chain fixtures and this in-process worker serve; its
//! registered context is 12 tokens, so the requests are tiny and the template is the passthrough (see `testkit`).
//!
//! **What is real and what is a fixture.** Real: the gateway's whole chat path, the worker's engine and capture, the sign-gate-shaped
//! commitment, fp-submit's `plan_submission`/`execute_handoff` (staging order, `.partial` → committed only on acceptance), the extraction walk's
//! stateless checks, the fold. Fixture: the executor's key (the l5 helpers' `pubkey_of(1)`, as in the base0 e2e; the signature is the door's
//! other test), the broadcast (a closure that accepts), and the panel (the e2e's `PanelBound`/`ReceiptLicensed` objects).

#![allow(clippy::too_many_arguments)]

#[path = "../../consensus/core/tests/dos_l5_common.rs"]
mod l5;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1, Params, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1, PalwFpIntervalVerdictV1};
use kaspa_consensus_core::palw_fp_objects_v3::{PalwFpClassCapsV1, PalwFpDerivedWorkCapV1, palw_fp_objects_from_accepted_txs_by_class_v1};
use kaspa_consensus_core::palw_freeprompt_v3::{
    PalwFpCommitmentTxPayloadV3, PalwFpDecodeRulesV1, PalwFpWorkerResultV3, PalwFreePromptCommitmentV3, fp_claim_id_v3, fp_job_id_v3,
};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwCertifiedFamilyStateV2, PalwCertifiedLaneV1, PalwChainStateV2, PalwClaimPhaseV2,
    PalwConsensusObjectV2, PalwPanelSeatV2, PalwStateCarriageV2, PalwStateV2Error, apply_palw_transition_v7,
};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT;
use kaspa_consensus_core::tx::Transaction;
use l5::*;
use misaka_palw_remote::track::ChainObservation;

use crate::testkit::{FloorWorker, certified_facts, config, identity, offline_source, temp_dir};
use crate::{receipt, serving, status};

const FORM: PalwPromptIdsFormV1 = PalwPromptIdsFormV1::MerkleV1;
/// The decode-rules fence's height on the V4 copy — past genesis, below every post-launch flag day.
const FENCE: u64 = 110;

fn t12_with_v4() -> Params {
    let mut p = palw_t12_shipped_params();
    (PALW_FP_DECODE_RULES_POST_LAUNCH_FENCE_V1.set)(&mut p, Some(ForkActivation::new(FENCE)));
    p.validate_palw_v2().expect("testnet-12 with the decode rules armed assembles");
    p
}

fn fold_at(p: &Params, parent: &PalwChainStateV2, c: &PalwBlockContextV2, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let sp = bundle(p).state;
    let f = flags(p, c.daa_score);
    let mut e = extras(p, c.daa_score);
    e.fp_derived_work_daa = p.palw_fp_derived_work_fence().map(|f| f.daa_score());
    e.fp_da_pins_active = p.palw_fp_da_pins_fence().is_some_and(|f| f.is_active(c.daa_score));
    apply_palw_transition_v7(
        parent,
        &sp,
        None,
        c,
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
    .map(|(s, _, _)| s)
}

fn floor_profile() -> Box<kaspa_consensus_core::palw_step::PalwShapeProfileV3> {
    Box::new(
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor profile"),
    )
}

/// The floor's two on-chain prerequisites (a certified family and a registry boundary), as the base0 e2e restates them.
fn seed_prereqs(p: &Params, s: &PalwChainStateV2) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleRowV1, PalwModelLifecycleV1, palw_lifecycle_profile_v1,
    };
    let (floor, _l, target, _sv) = genesis_classes(p)[0];
    let family = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_fp_certified_families_v1()
        .into_iter()
        .find(|f| f.drilled_class_id == floor_profile().shape_profile_id())
        .expect("BASE-0's fp family is pinned");
    let fp = floor_profile();
    let (pf, dc) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, pf, dc);
    let work = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &job).expect("floor work derives");
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = bundle(p).panel.seat_count();
    let expected_q32 = kaspa_consensus_core::palw_economic_compute_v1::palw_expected_attempts_q32_v1(target);
    let row = PalwModelLifecycleRowV1 {
        state: PalwModelLifecycleV1::Active,
        work,
        profile: palw_lifecycle_profile_v1(&work, expected_q32, &globals, false),
        since_span: 0,
        probes_passed: 0,
        probes_failed: 0,
        probes_passed_this_span: 0,
        probes_failed_this_span: 0,
        ready_seats: 0,
        inflight_claims: 0,
        utilization_permille: 0,
        admission_milli: 0,
        cap_utilization_permille: 0,
        priced_share_permille: 0,
    };
    let mut c = PalwStateCarriageV2::from_state(s);
    c.fp_certified_families.insert(family.digest(), PalwCertifiedFamilyStateV2 { family, certified_daa: 0 });
    c.model_lifecycles.insert(floor, row);
    c.into_state_v3(&bundle(p).state, None, false, p.palw_canonical_work_daa()).expect("carriage rebuilds")
}

/// The chain up to the commitment: an executor bond, the floor's prerequisites, its graph published.
fn chain(p: &Params) -> (PalwChainStateV2, Hash64) {
    let floor = genesis_classes(p)[0].0;
    let g = genesis_state(p);
    let s = fold_at(p, &g, &ctx(1, 100, 1, 0), &[bond_obj(1, at_least_the_floor(p, 100_000_000_000))]).expect("the executor bonds");
    let s = seed_prereqs(p, &s);
    let publish = PalwConsensusObjectV2::ClassLaneCertified { class_id: floor, lane: PalwCertifiedLaneV1::FreePrompt, profile: floor_profile() };
    let s = fold_at(p, &s, &ctx(2, 101, 2, 0), &[publish]).expect("the floor publishes its graph");
    (s, floor)
}

/// The accepted-transaction walk exactly as the processor runs it, under `rules`.
fn walk(p: &Params, s: &PalwChainStateV2, tx: &Transaction, rules: PalwFpDecodeRulesV1) -> Vec<PalwConsensusObjectV2> {
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    let ladder = bundle(p).court.max_step_leaf_count();
    palw_fp_objects_from_accepted_txs_by_class_v1(
        std::slice::from_ref(tx),
        domain,
        &bundle(p).freeprompt,
        kaspa_consensus_core::BlockHash::default(),
        false,
        |class_id| PalwFpClassCapsV1 {
            step_ladder: s.class_step_ladder_v1(class_id, ladder),
            held: s.class_is_held_v1(class_id),
            derived_work: match s.fp_work_profile_of(class_id) {
                Some(profile) => PalwFpDerivedWorkCapV1::Derived(profile),
                None => PalwFpDerivedWorkCapV1::Unpublished,
            },
            logits_q24: s.class_commits_q24_logits_v1(class_id),
            prefix_state_armed: false,
            prefix_inherit_armed: false,
            prefix_inherit_class_safe: false,
            constraint_armed: false,
            constraint_v2_armed: false,
            tokenizer: kaspa_consensus_core::palw_fp_tokenizer_v1::PalwFpTokenizerRuleV1::Dormant,
        },
        true,
        false,
        FORM,
        rules,
        // The executor's key here is a fixture's; the signature is the door's other test.
        |_, _, _, _| true,
    )
    .objects
    .into_iter()
    .map(|carried| carried.object)
    .collect()
}

/// **The node's claim row** for a claim in the fold's state — the fields `GetPalwFreePromptClaim` returns (`rpc/service`), from the same
/// `PalwClaimStateV2`.
fn claim_row(s: &PalwChainStateV2, id: &Hash64, sink: Hash64, daa: u64) -> (kaspa_rpc_core::GetPalwFreePromptClaimResponse, ChainObservation) {
    let row = match s.claim(id) {
        None => kaspa_rpc_core::GetPalwFreePromptClaimResponse::default(),
        Some(c) => {
            let (phase, phase_daa) = match &c.phase {
                PalwClaimPhaseV2::Provisional => ("provisional", 0),
                PalwClaimPhaseV2::ReceiptLicensed { .. } => ("receipt_licensed", daa),
                other => panic!("this test folds no further than a licence: {other:?}"),
            };
            kaspa_rpc_core::GetPalwFreePromptClaimResponse {
                found: true,
                claim_id: id.to_string(),
                is_free_prompt: matches!(c.source, kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. }),
                class_id: c.class_id.to_string(),
                executor_bond: format!("{}:{}", c.bond.0.transaction_id, c.bond.0.index),
                output_root: c.output_root.to_string(),
                trace_root: c.trace_root.to_string(),
                execution_root: c.execution_root.to_string(),
                work_leaves: c.work_leaves,
                phase: phase.into(),
                phase_daa,
                accepted_block: c.accepted_block.to_string(),
                accepted_daa: c.accepted_daa,
                trace_retention_daa: c.trace_retention_daa,
                ..Default::default()
            }
        }
    };
    let observation = crate::chain::observation_from_claim_reply(sink, daa, false, &row).expect("the row folds");
    (row, observation)
}

struct Observe(ChainObservation);
impl status::ClaimObserver for Observe {
    fn observe(&self, _: Hash64, _: Option<Hash64>) -> Result<(ChainObservation, usize), String> {
        Ok((self.0.clone(), 1))
    }
}

/// Everything a gateway request produced, read back from the outbox the way the rail reads it.
struct Produced {
    body: serde_json::Value,
    commitment: PalwFreePromptCommitmentV3,
    result: PalwFpWorkerResultV3,
    capture: Vec<u8>,
    stem: String,
}

fn produce(p: &Params, w: &FloorWorker, dir: &std::path::Path, decode_rules: bool, request: &serde_json::Value) -> Produced {
    let (mut cfg, mut id, mut facts) = (config(dir), identity(&w.profile), certified_facts(decode_rules));
    // The gateway's identity is the chain fixture's executor: the bond the fold knows, its key, its operator, the network's domain.
    id.network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(p.net.to_string().as_bytes(), Some(p.genesis.hash));
    id.executor_bond = bond_key(1).0;
    id.bond_txid_hex = id.executor_bond.transaction_id.to_string();
    id.executor_pubkey = pubkey_of(1);
    id.operator_id = kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&operator_pubkey_of(1));
    facts.anchor_block = h(0xA0);
    facts.anchor_daa = 100;
    cfg.max_decode_cap = 8;
    let source = offline_source(dir);
    let body = crate::testkit::chat(&cfg, &id, w, &facts, &source, request, &serving::AlwaysPresent).expect("the gateway answers");
    assert_eq!(body["misaka"]["committed"], true, "{}", body["misaka"]["not_committed_because"]);
    let stem = format!("fp-job-{}", &body["misaka"]["fp_job_id"].as_str().unwrap()[..16]);
    let commitment: PalwFreePromptCommitmentV3 = borsh::from_slice(&std::fs::read(dir.join(format!("{stem}.commitment-unsigned.borsh"))).unwrap()).unwrap();
    let result: PalwFpWorkerResultV3 = borsh::from_slice(&std::fs::read(dir.join(format!("{stem}.result.borsh"))).unwrap()).unwrap();
    let capture = crate::evidence::read_capture(dir, &fp_job_id_v3(&commitment.job)).expect("the retained capture");
    assert_eq!(fp_claim_id_v3(&commitment).to_string(), body["misaka"]["fp_claim_id"].as_str().unwrap(), "the response names the claim the outbox holds");
    Produced { body, commitment, result, capture, stem }
}

/// What the rail does with the unsigned commitment: sign-shaped payload → the 0x4a transaction → fp-submit's plan and handoff, with the
/// broadcast a closure that accepts. Returns the transaction the node would hold.
fn submit(dir: &std::path::Path, produced: &Produced, chain_daa: u64) -> (Transaction, misaka_palw_fp_submit::FpSubmitted) {
    let payload = PalwFpCommitmentTxPayloadV3 {
        version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        commitment: produced.commitment.clone(),
        prompt_token_ids: produced.result.prompt_token_ids.clone(),
        signature: vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, borsh::to_vec(&payload).unwrap());
    let retention = dir.join("retention");
    let staging = misaka_palw_fp_submit::FpStaging {
        retention_dir: Some(&retention),
        capture: Some(&produced.capture),
        output_token_ids: Some(&produced.result.output_token_ids),
        dsl_payload: None,
        prompt_token_ids: Some(&produced.result.prompt_token_ids),
        expiry: Some(misaka_palw_fp_submit::AnchorExpiry::new(produced.commitment.job.anchor_daa, 3_000)),
    };
    let plan = misaka_palw_fp_submit::plan_submission(&tx, &staging, chain_daa).expect("the plan");
    struct Accept;
    impl misaka_palw_fp_submit::FpBroadcast for Accept {
        async fn broadcast(&self, tx: &Transaction) -> Result<String, String> {
            Ok(tx.id().to_string())
        }
    }
    let submitted = futures_lite_block_on(misaka_palw_fp_submit::execute_handoff(&plan, &tx, Some(&retention), Accept)).expect("the handoff");
    (tx, submitted)
}

/// A minimal single-future executor: the handoff's future never awaits anything that is not ready.
fn futures_lite_block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    fn noop_raw() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw()
        }
        fn noop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    let waker = unsafe { Waker::from_raw(noop_raw()) };
    let mut cx = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let Poll::Ready(out) = future.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

/// A seat replays every interval of the claim from the gateway's own capture.
fn seats_replay(w: &FloorWorker, produced: &Produced) {
    let backend = w.backend();
    let job = &produced.commitment.job;
    let ids = &produced.result.prompt_token_ids;
    let roots = PalwClaimRootsV1 {
        execution_root: produced.commitment.execution_root,
        trace_root: produced.commitment.trace_root,
        anchor: fp_job_id_v3(job),
        attempt_draw: None,
        output_root: None,
        job_pin: None,
    };
    let count = backend.fp_interval_count_for(job.prompt_tokens, produced.commitment.decode_tokens_executed).expect("intervals");
    assert!(count > 0);
    for index in 0..count {
        let opening = backend.open_fp_interval(&produced.capture, index, ids).expect("the executor opens it");
        backend.fp_forget_seat_state_v1();
        // A seat recomputes the checkpoint root of the prefix the opening covers from the ids it holds, as the base0 e2e does.
        if let Some((_, covered, _)) = misaka_palw_base0::fp_interval::base0_fp_interval_opening_anchor_v1(&opening) {
            let ctx = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&opening).unwrap().binding.job_context;
            backend.checkpoint_root_for_context_v1(&ctx, ids, &produced.result.output_token_ids, covered).expect("the seat recomputes");
        }
        let verdict = backend.verify_fp_interval_opening_under_job_v1(
            &opening,
            roots,
            index,
            ids,
            produced.commitment.work_leaves,
            job,
            &produced.result.output_token_ids,
        );
        assert_eq!(verdict, PalwFpIntervalVerdictV1::Valid, "interval {index}");
    }
}

#[test]
fn a_v3_gateway_request_becomes_a_claim_the_fold_holds_a_receipt_the_chain_confirms_and_a_status_that_is_not_final() {
    let p = palw_t12_shipped_params();
    let (s, _floor) = chain(&p);
    let dir = temp_dir("e2e-chain-v3");
    let w = FloorWorker::new(&dir.join("traces"), FORM);
    let request = serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }], "max_tokens": 4 });
    let produced = produce(&p, &w, &dir, false, &request);
    assert_eq!(produced.commitment.job.version, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION, "the live lane: FP Job V3");

    // ---- submit: fp-submit's plan and handoff ----
    let (tx, submitted) = submit(&dir, &produced, 105);
    let claim_id = fp_claim_id_v3(&produced.commitment);
    assert_eq!(submitted.claim_id, claim_id);
    let material = submitted.material_path.expect("the material was committed beside the retention dir after the node accepted");
    assert!(material.exists());
    let partials = std::fs::read_dir(dir.join("retention")).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".partial")).count();
    assert_eq!(partials, 0, "after the node accepted, every staged file took its real name");

    // ---- the node: the extraction walk, then the fold ----
    let objects = walk(&p, &s, &tx, PalwFpDecodeRulesV1::Dormant);
    assert_eq!(objects.len(), 1, "the carrier is stateless-admissible and the walk opens one claim");
    let s = fold_at(&p, &s, &ctx(3, 112, 3, 0), &objects).expect("the real fold opens the gateway's claim");
    let claim = s.claim(&claim_id).expect("THE CLAIM IS VISIBLE in the state");
    assert_eq!((claim.output_root, claim.execution_root, claim.trace_root), (produced.commitment.output_root, produced.commitment.execution_root, produced.commitment.trace_root));
    assert_eq!(claim.work_leaves, produced.commitment.work_leaves);
    assert_eq!(claim.bond, bond_key(1), "and it names the gateway's executor bond");

    // ---- the receipt, against the node's claim row ----
    let (row, observation) = claim_row(&s, &claim_id, h(0x51), 113);
    let sealed = receipt::ReceiptV1::from_json(&produced.body["misaka"]["receipt"]).unwrap().with_txid(Some(submitted.txid.clone()));
    match receipt::check_against_chain(&sealed, &row).expect("the node's row agrees with the receipt the user holds") {
        receipt::ChainStanding::Agrees { phase, label } => assert_eq!((phase.as_str(), label), ("provisional", "UNVERIFIED_REMOTE_STATE")),
        other => panic!("{other:?}"),
    }
    // ... and the user's output recomputes to the committed root from the retained job context.
    let ctx_hex = produced.body["misaka"]["job_context"].as_str().unwrap();
    let mut raw = vec![0u8; ctx_hex.len() / 2];
    faster_hex::hex_decode(ctx_hex.as_bytes(), &mut raw).unwrap();
    let job_context: kaspa_consensus_core::palw_v2::PalwJobContextV2 = borsh::from_slice(&raw).unwrap();
    let ids: Vec<u32> = serde_json::from_value(produced.body["misaka"]["output_token_ids"].clone()).unwrap();
    let shown = produced.body["choices"][0]["message"]["content"].as_str().unwrap();
    let verdict = receipt::verify_receipt(
        &sealed,
        &receipt::ReceiptWitness {
            commitment: &produced.commitment,
            output_token_ids: &ids,
            shown: Some(shown.as_bytes()),
            job_context: Some(&job_context),
            rule: receipt::OutputRule::CoreV1,
            render: None,
        },
    )
    .expect("the receipt verifies against the commitment the chain holds");
    assert!(verdict.output_root_recomputed && verdict.claim_matches_commitment);
    // A row for ANOTHER output is refused: the receipt is authenticated by the chain's claim, not trusted.
    let mut tampered_row = row.clone();
    tampered_row.output_root = h(0xBAD).to_string();
    assert!(receipt::check_against_chain(&sealed, &tampered_row).is_err());

    // ---- the status: committed in the outbox, `submitted` once the chain holds it, never final ----
    let local = status::read_local_facts(&dir, &produced.stem).expect("the outbox summary");
    assert_eq!(status::derive_status(&local, None).status, status::RequestStatus::Committed);
    let observer = Observe(observation);
    let report = status::status_with_chain(&local, Some(&observer), 60, None);
    assert_eq!(report.status, status::RequestStatus::Submitted, "{report:?}");
    assert_eq!(report.chain_state.as_deref(), Some("Included"));
    assert!(report.provenance.as_deref().unwrap().starts_with("UNVERIFIED_REMOTE_STATE") && !report.is_final());

    // ---- the Panel: a seat replays the gateway's capture to Valid, and a Valid quorum licenses the claim ----
    seats_replay(&w, &produced);
    let bonds = genesis_bonds(&p);
    let seats: Vec<PalwPanelSeatV2> = bonds[0..5].iter().map(|(k, o, _)| PalwPanelSeatV2 { bond: *k, operator_id: *o }).collect();
    let s = fold_at(&p, &s, &ctx(4, 113, 4, 0), &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h(0xA1), seats }]).expect("the panel binds");
    let receipts: Vec<PalwSeatReceiptV2> = bonds[0..3]
        .iter()
        .map(|(k, _, _)| PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: *k, signed_daa: 114, signature: Vec::new() })
        .collect();
    let s = fold_at(&p, &s, &ctx(5, 114, 5, 0), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts }]).expect("the Valid quorum licenses");
    assert!(matches!(s.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let (_, licensed) = claim_row(&s, &claim_id, h(0x52), 120);
    let report = status::status_with_chain(&local, Some(&Observe(licensed)), 60, None);
    assert_eq!((report.status, report.chain_state.as_deref()), (status::RequestStatus::Submitted, Some("Licensed")), "licensed is still not final");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_v4_gateway_request_is_carried_only_past_the_test_armed_fence_credited_decode_work_only_and_replayed() {
    let p = t12_with_v4();
    let (s, _floor) = chain(&p);
    let dir = temp_dir("e2e-chain-v4");
    let w = FloorWorker::new(&dir.join("traces"), FORM);
    // A sampled, penalized request with a stop string: every V4 control the gateway normalizes.
    let request = serde_json::json!({
        "messages": [{ "role": "user", "content": "hi" }],
        "max_tokens": 4,
        "temperature": 0.5,
        "seed": "3c".repeat(32),
        "repeat_penalty": 2.0,
        "presence_penalty": 0.5,
        "stop": ["~~~"],
    });
    let produced = produce(&p, &w, &dir, true, &request);
    let job = &produced.commitment.job;
    assert_eq!(job.version, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION, "past the fence every job is V4");
    let decode = job.decode.as_ref().expect("a V4 job carries its decode config");
    assert_eq!((decode.repeat_penalty_q, decode.presence_penalty_q), (131_072, 1 << 23), "the gateway normalized the request's floats to the frozen fixed point");
    assert_eq!(decode.stop_sequences, vec![vec![126, 126, 126]], "the stop string was spelled into token ids by the worker");
    assert_eq!((job.temperature_q, job.sampling_seed), (1 << 23, [0x3C; 32]));
    let (tx, _submitted) = submit(&dir, &produced, 105);
    let claim_id = fp_claim_id_v3(&produced.commitment);

    // The fence: no V4 claim below it, and (the base0 e2e's other half) the same fold prices decode work only.
    assert!(walk(&p, &s, &tx, PalwFpDecodeRulesV1::at(Some(FENCE), FENCE - 1)).is_empty(), "no V4 claim below the fence");
    let objects = walk(&p, &s, &tx, PalwFpDecodeRulesV1::at(Some(FENCE), 112));
    assert_eq!(objects.len(), 1, "the V4 carrier opens a claim past the fence");
    let armed = fold_at(&p, &s, &ctx(3, 112, 3, 0), &objects).expect("the real fold opens the V4 claim");
    let claim = armed.claim(&claim_id).expect("the V4 claim is visible");
    assert_eq!(claim.bond, bond_key(1));
    // D10: past the fence the claim is credited its DECODE leaves — strictly less than the unarmed ruleset credits the same commitment.
    let unarmed = palw_t12_shipped_params();
    let (s_unarmed, _) = chain(&unarmed);
    let s_unarmed = fold_at(&unarmed, &s_unarmed, &ctx(3, 112, 3, 0), &objects).expect("the unarmed fold prices it too");
    assert!(claim.pwu < s_unarmed.claim(&claim_id).expect("priced unarmed").pwu, "D10: prefill is priced at zero ({} !< {})", claim.pwu, s_unarmed.claim(&claim_id).unwrap().pwu);
    // The receipt's decode-rule digest is the V4 rule, and a seat replays the sampled, penalized, stopped run to Valid.
    let sealed = receipt::ReceiptV1::from_json(&produced.body["misaka"]["receipt"]).unwrap();
    assert_eq!(sealed.body.decode_config_digest, receipt::decode_rule_digest(job));
    assert_eq!(sealed.body.job_version, kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V4_VERSION);
    seats_replay(&w, &produced);
    let (row, _) = claim_row(&armed, &claim_id, h(0x53), 113);
    assert!(receipt::check_against_chain(&sealed, &row).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Failure at each hand-over leaves one consistent world: a refused broadcast leaves no material and no claim; an expired commitment is never
/// even staged (fp-submit's SA-1(b), reached from a real gateway commitment).
#[test]
fn the_handoff_leaves_no_material_when_the_node_refuses_and_an_expired_commitment_is_never_staged() {
    let p = palw_t12_shipped_params();
    let dir = temp_dir("e2e-chain-handoff");
    let w = FloorWorker::new(&dir.join("traces"), FORM);
    let request = serde_json::json!({ "messages": [{ "role": "user", "content": "hi" }], "max_tokens": 4 });
    let produced = produce(&p, &w, &dir, false, &request);
    let payload = PalwFpCommitmentTxPayloadV3 {
        version: kaspa_consensus_core::palw_freeprompt_v3::PALW_FP_V3_VERSION,
        commitment: produced.commitment.clone(),
        prompt_token_ids: produced.result.prompt_token_ids.clone(),
        signature: vec![0x5A; kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
    };
    let tx = Transaction::new(0, vec![], vec![], 0, SUBNETWORK_ID_PALW_FP_COMMITMENT, 0, borsh::to_vec(&payload).unwrap());
    let retention = dir.join("retention-refused");
    let staging = |expiry| misaka_palw_fp_submit::FpStaging {
        retention_dir: Some(&retention),
        capture: Some(&produced.capture),
        output_token_ids: Some(&produced.result.output_token_ids),
        dsl_payload: None,
        prompt_token_ids: Some(&produced.result.prompt_token_ids),
        expiry,
    };
    // Past the anchor's time to live: refused before a single file exists.
    let expiry = Some(misaka_palw_fp_submit::AnchorExpiry::new(produced.commitment.job.anchor_daa, 3_000));
    assert!(misaka_palw_fp_submit::plan_submission(&tx, &staging(expiry), produced.commitment.job.anchor_daa + 3_001).is_err());
    assert!(!retention.exists());
    // The node refuses the broadcast: the staged `.partial` goes away and no claim exists.
    let plan = misaka_palw_fp_submit::plan_submission(&tx, &staging(expiry), 105).unwrap();
    struct Refuse;
    impl misaka_palw_fp_submit::FpBroadcast for Refuse {
        async fn broadcast(&self, _: &Transaction) -> Result<String, String> {
            Err("mempool: rejected".into())
        }
    }
    assert!(futures_lite_block_on(misaka_palw_fp_submit::execute_handoff(&plan, &tx, Some(&retention), Refuse)).is_err());
    let leftovers: Vec<_> = std::fs::read_dir(&retention).map(|d| d.flatten().collect()).unwrap_or_default();
    assert!(leftovers.is_empty(), "no material for a claim the node never saw: {leftovers:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
