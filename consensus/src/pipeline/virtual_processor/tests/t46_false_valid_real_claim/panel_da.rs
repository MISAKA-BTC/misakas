//! **Lane J1 (2026-10-27): the PanelDa mandatory disclosure (ADR-0206) through a real node's gate, acceptance walk and fold.**
//!
//! testnet-12 with its harness cards; `palw_panel_da`, `palw_court_clock_v2` and `palw_panel_da_disclosure_v1` are armed at DAA 0
//! INSIDE THIS TEST with its prerequisite `palw_legacy_public_filer_v1` (the bundle mirrors written by the `sync_*` functions; PanelDa's and the clock's own gates pass; only the testnet-12 arming-record pin is bypassed, as the canonical harness does). The
//! honest poster's job is a PanelDa text job: the commitment carries no ids. The status a user reads is op 227's
//! `panelDaDisclosure/v1` document (`palw_panel_da_disclosure_view_json_v1`, what `misaka palw panel-da status` prints); the
//! disclosure is the object `misaka palw panel-da disclose` builds (`palw_panel_da_disclosure_object_v1`), checked against that
//! document the way the CLI's `check_against_view` checks it, then carried gate → walk → fold.
//!
//! * (a) disclosed before the deadline → accepted, the deposit returned, `Final` past the floor, nothing burned;
//! * (b) never disclosed → past the deadline the claim is void `PanelDaDisclosureDefault`, exactly the deposit burned, no conviction;
//! * (c) a late disclosure and a disclosure of other ids are refused;
//! * (d) a second disclosure is refused and changes nothing.

use super::*;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_panel_da_disclosure_v1::{palw_panel_da_disclosure_object_v1, palw_panel_da_disclosure_view_json_v1};
use kaspa_consensus_core::palw_user_views_v1::PalwUserViewClockV1;

fn harness_panel_da() -> H {
    kaspa_core::log::try_init_logger("warn");
    let (config, _bundle, _premine, floats) = super::super::t12_round_lane_e2e::t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_panel_da = Some(ForkActivation::new(0));
    params.palw_court_clock_v2 = Some(ForkActivation::new(0));
    params.palw_panel_da_disclosure_v1 = Some(ForkActivation::new(0));
    // The court clock's prerequisite (G14 U06/U07): the legacy public filer, in force at 0 too.
    params.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
    params.sync_palw_legacy_public_filer_v1();
    params.sync_palw_g14_gates_v1();
    params.validate_palw_panel_da_disclosure_v1().expect("PanelDa's own arming gate passes");
    params.validate_palw_court_clock_v2().expect("the court clock's gate passes");
    // Only the per-network arming-record pin refuses DAA 0 on testnet-12 (no record names it); the harness bypasses that one check
    // as the canonical / cert harnesses do (`Config::new`, not `ConfigBuilder::build`).
    let config = Config::new(params);
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else {
        unreachable!("testnet-12 is ConsensusV2")
    };
    let bundle = bundle.clone();
    assert_eq!(bundle.state.panel_da_disclosure_from_daa(), Some(0), "the fold's mirror is armed");
    harness_over(config, bundle, floats)
}

/// What a user reads with `misaka palw panel-da status <CLAIM>`.
fn status(walk: &Walk, claim: &Hash64) -> serde_json::Value {
    let clock = PalwUserViewClockV1 { next_daa: walk.daa + 1, raw_depth: None, objective_offence: false };
    palw_panel_da_disclosure_view_json_v1(&walk.state, claim, &clock).expect("the claim is held")
}

/// The CLI's pre-submit check (`palw_panel_da::check_against_view`): governed, undisclosed, in time, same count, the hash binds.
fn cli_check(doc: &serde_json::Value, ids: &[u32]) -> Result<(), &'static str> {
    use kaspa_consensus_core::palw_prompt_ids_v1::{PalwPromptIdsFormV1 as F, prompt_token_ids_match_v1};
    if doc["governed"] != true || doc["deadlineDaa"].is_null() {
        return Err("not governed");
    }
    if doc["disclosed"] == true {
        return Err("already disclosed");
    }
    if doc["nowDaa"].as_u64().unwrap() > doc["deadlineDaa"].as_u64().unwrap() {
        return Err("late");
    }
    if doc["promptTokens"].as_u64() != Some(ids.len() as u64) {
        return Err("count");
    }
    let h: Hash64 = doc["promptTokenIdsHash"].as_str().unwrap().parse().unwrap();
    if !(prompt_token_ids_match_v1(F::Flat, ids, &h) || prompt_token_ids_match_v1(F::MerkleV1, ids, &h)) {
        return Err("hash");
    }
    Ok(())
}

/// The honest PanelDa claim, committed through the processor's walk; returns the walk, the claim and its prompt ids.
fn committed(h: &H) -> (Walk, Hash64, Vec<u32>) {
    use kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3;
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PANEL_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT, PALW_FP_V3_VERSION,
        PalwFpCommitmentTxPayloadV3, PalwFreePromptJobV3, fp_claim_id_v3,
    };
    let mut walk = h.genesis_walk();
    let floor = h.floor();
    let facts = h.ctx.consensus.palw_producer_facts_v2(floor, Some(h.cards[EXECUTOR].0)).expect("a V2 network answers");
    let evidence = misaka_palw_base0::e2e_drill::rc_free_prompt_evidence_v1(misaka_palw_base0::e2e_drill::PalwRcFamilyV1::Base0)
        .expect("the floor drills its free-prompt lane");
    h.carry(
        &mut walk,
        vec![Obj::FamilyCertified {
            evidence: Box::new(kaspa_consensus_core::palw_state_v2::PalwCertificationEvidenceV1::FreePrompt(evidence)),
        }],
    );
    h.carry(
        &mut walk,
        vec![Obj::ClassLaneCertified {
            class_id: floor,
            lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt,
            profile: Box::new(h.backend.profile().clone()),
        }],
    );
    let ids: Vec<u32> = vec![3, 5, 8, 13];
    let prompt: Vec<usize> = ids.iter().map(|t| *t as usize).collect();
    let n_ctx = h.backend.profile().n_ctx;
    let job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: h.domain,
        class_id: floor,
        executor_bond: h.cards[EXECUTOR].0,
        executor_pubkey: card_pubkey(EXECUTOR),
        operator_id: walk.state.bond(&h.cards[EXECUTOR]).unwrap().operator_id,
        anchor_block: h.config.params.genesis.hash,
        anchor_daa: walk.daa,
        job_nonce: [0x7A; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            h.backend.prompt_ids_form(),
            &ids,
        )
        .expect("a short prompt commits"),
        prompt_tokens: ids.len() as u32,
        decode_token_limit: n_ctx - ids.len() as u32,
        max_context_tokens: n_ctx,
        privacy_mode: PALW_FP_PRIVACY_PANEL_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    let honest = h.backend.execute_free_prompt(&job, &prompt).expect("the floor runs the prompt");
    let context = h.backend.capture_shape(&honest.outcome.material).expect("a shape").job_context;
    let commitment = palw_fp_commitment_from_context_v3(&job, &context, &honest, walk.daa + facts.min_trace_retention_daa)
        .expect("the run becomes a commitment");
    let signature = sign(EXECUTOR, fp_claim_id_v3(&commitment).as_byte_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT);
    // PanelDa: the ids stay off the commitment transaction.
    let payload =
        PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment: commitment.clone(), prompt_token_ids: vec![], signature };
    let tx = kaspa_consensus_core::tx::Transaction::new(
        0,
        vec![],
        vec![],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT.clone(),
        0,
        borsh::to_vec(&payload).unwrap(),
    );
    let point = walk.next();
    let extraction = h.vp().palw_v2_fp_objects_of_txs_for_tests(std::slice::from_ref(&tx), &walk.state, point.daa_score);
    assert!(extraction.skipped.is_empty(), "the walk takes the PanelDa carrier: {:?}", extraction.skipped);
    let objects: Vec<Obj> = extraction.objects.into_iter().map(|c| c.object).collect();
    h.carry(&mut walk, objects);
    let id = fp_claim_id_v3(&commitment);
    assert!(walk.state.claim(&id).is_some(), "the commitment opened a claim");
    assert!(walk.state.panel_da_disclosure_v1(&id).is_some(), "the governed claim's disclosure row is written at admission");
    (walk, id, ids)
}

/// Refused at `point` by the gate or the fold; if the fold alone refused, nothing about the row changed either way.
fn refused_at(h: &H, walk: &Walk, point: &PalwBlockContextV2, object: &Obj) {
    let gate = h.validate(&walk.state, point, object);
    let fold = h.fold(&walk.state, point, std::slice::from_ref(object));
    assert!(gate.is_err() && fold.is_err(), "refused by the gate ({gate:?}) and the fold ({:?})", fold.as_ref().map(|_| ()));
}

/// (a) + (c, wrong ids) + (d): the honest poster reads the request, discloses in time, is refused a second time, and Finals unburned.
#[tokio::test]
async fn j1_panel_da_honest_poster_discloses_in_time_through_the_public_path() {
    let h = harness_panel_da();
    let (mut walk, claim, ids) = committed(&h);
    let producer = h.cards[EXECUTOR];
    let doc = status(&walk, &claim);
    assert_eq!(
        (doc["governed"].as_bool(), doc["disclosed"].as_bool(), doc["deposit"].as_str()),
        (Some(true), Some(false), Some("locked")),
        "{doc}"
    );
    let deadline = doc["deadlineDaa"].as_u64().expect("a deadline");
    assert!(deadline > walk.daa && doc["remainingDaa"].as_u64().unwrap() > 0, "{doc}");
    assert!(doc["depositSompi"].as_u64().unwrap() > 0);
    let slashed = walk.state.bond(&producer).unwrap().slashed;

    // (c) a disclosure of other ids: the CLI refuses it, and so does the chain.
    let wrong = vec![3, 5, 8, 14];
    assert_eq!(cli_check(&doc, &wrong), Err("hash"));
    refused_at(&h, &walk, &walk.next(), &palw_panel_da_disclosure_object_v1(claim, wrong));

    // (a) the honest disclosure, through gate → walk → fold.
    cli_check(&doc, &ids).expect("the CLI's check passes");
    h.carry(&mut walk, vec![palw_panel_da_disclosure_object_v1(claim, ids.clone())]);
    let doc = status(&walk, &claim);
    assert_eq!((doc["disclosed"].as_bool(), doc["deposit"].as_str()), (Some(true), Some("returned")), "{doc}");
    assert_eq!(walk.state.panel_da_disclosure_v1(&claim).unwrap().prompt_token_ids, ids, "the ids are public state");
    assert_eq!(walk.state.panel_da_deposit_locked_v1(&producer), 0, "the deposit is returned");

    // (d) a resend: the CLI refuses it, the chain refuses it, nothing changes.
    assert_eq!(cli_check(&doc, &ids), Err("already disclosed"));
    let row = walk.state.panel_da_disclosure_v1(&claim).cloned();
    refused_at(&h, &walk, &walk.next(), &palw_panel_da_disclosure_object_v1(claim, ids.clone()));
    assert_eq!(walk.state.panel_da_disclosure_v1(&claim).cloned(), row);

    // To Final: no default, nothing burned.
    h.bind(&mut walk, claim);
    h.license_v2(&mut walk, claim);
    h.sweep_to_final(&mut walk, claim);
    assert!(matches!(walk.state.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "Final");
    assert!(walk.daa > deadline, "Final only past the disclosure deadline");
    assert_eq!(walk.state.bond(&producer).unwrap().slashed, slashed, "no burn");
}

/// (b) + (c, late): nobody discloses; a late disclosure is refused; past the deadline the claim defaults — the deposit burned, nothing
/// more.
#[tokio::test]
async fn j1_panel_da_undisclosed_claim_defaults_and_a_late_disclosure_is_refused() {
    let h = harness_panel_da();
    let (mut walk, claim, ids) = committed(&h);
    let producer = h.cards[EXECUTOR];
    let deadline = status(&walk, &claim)["deadlineDaa"].as_u64().unwrap();
    let deposit = walk.state.panel_da_disclosure_v1(&claim).unwrap().deposit_sompi;
    h.bind(&mut walk, claim);
    h.license_v2(&mut walk, claim);
    // (c) late: the first DAA past the deadline.
    let late = palw_panel_da_disclosure_object_v1(claim, ids.clone());
    refused_at(&h, &walk, &walk.at(deadline + 1), &late);
    let before = walk.state.bond(&producer).unwrap().clone();
    // (b) empty blocks until the claim leaves ReceiptLicensed.
    for _ in 0..20_000 {
        let point = walk.next();
        let next = h.fold(&walk.state, &point, &[]).expect("an empty block folds");
        walk.advance(&point, next);
        if !matches!(walk.state.claim(&claim).map(|c| &c.phase), Some(PalwClaimPhaseV2::ReceiptLicensed { .. })) {
            break;
        }
    }
    assert!(
        matches!(
            walk.state.claim(&claim).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PanelDaDisclosureDefault, .. }
        ),
        "{:?}",
        walk.state.claim(&claim).unwrap().phase
    );
    assert!(walk.daa > deadline, "the default lands only past the deadline");
    let after = walk.state.bond(&producer).unwrap();
    assert_eq!(after.slashed, before.slashed + deposit, "exactly the deposit is burned");
    assert_eq!(after.collateral, before.collateral - deposit, "a default is not a conviction: nothing beyond the deposit");
    assert_eq!(status(&walk, &claim)["deposit"], "burned");
}
