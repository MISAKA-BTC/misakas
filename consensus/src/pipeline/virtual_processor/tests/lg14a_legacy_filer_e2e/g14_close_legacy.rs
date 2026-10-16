//! **Lane g14-close-legacy (2026-10-16) on the real node: the legacy V2 Attempt route G14-complete with LG14-A + LG14-B armed.**
//!
//! Unlike the parent suite (whose harness sets the dormant fence on its mirror WITHOUT validation — audit G-19), every network here is
//! testnet-12 as launched with the parent's harness cards and re-addressed premine, with `palw_legacy_public_filer_v1` (LG14-A),
//! `palw_legacy_held_da_v2` (LG14-B) and `palw_g14_route_required_v1` (option (b), user decision 2026-10-16) armed at genesis on the
//! `Params`, each mirror written by its own `sync_*`, and the ruleset asserted to pass the WHOLE validation
//! (`Params::validate_palw_v2_with_arming_records_v1`) with a test arm-readiness record per fence naming this genesis and height —
//! and to be refused by `validate_palw_v2` (no record is pinned in the binary).
//!
//! The scenario is the parent's: the producer and EVERY seat collude (all `Valid`); ONE bond registered after genesis through a real
//! `0x4b` carrier, never a seat, prosecutes from what the node's read API and accepted blocks show and its own copy of the floor model.
//! Every object goes through the mempool, the node's own template and the chain block's fold.
use super::*;
use kaspa_consensus_core::palw_false_valid_filing_v1::PalwFalseValidFilingCheckV1;
use kaspa_consensus_core::palw_g14_gates_v1::{
    PALW_G14_ARMING_RECORD_VERSION_V1, PalwG14ArmingRecordV1, PalwG14GatedFenceV1, PalwG14RouteFamilyV1, palw_g14_route_attempt_refusal_v1,
};
use kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2;
use kaspa_consensus_core::palw_offence_attribution_v1::{PalwFalseValidReceiptV1, PalwPanelFalseValidEvidenceV2, palw_false_valid_offence_id_v2};
use kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1;

/// A test arm-readiness record (never pinned in the binary): one distinct non-zero digest per requirement, a review, a commit — the
/// court-capacity record's measured-values evidence being its measurement record's digest.
fn record(fence: PalwG14GatedFenceV1, genesis: Hash64, at: u64) -> PalwG14ArmingRecordV1 {
    let mut evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x60 + i as u8; 32]).collect();
    if fence == PalwG14GatedFenceV1::CourtCapacityV1 {
        evidence[kaspa_consensus_core::palw_court_capacity_v1::PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1] = measured_court().digest();
    }
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: at,
        source_commit: [0x14; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

/// **The court-capacity measurement record this suite arms with (lane g14-court-capacity) — a TEST-ONLY config, never a network's
/// record.** It has the format a network's record takes (`PalwCourtMeasurementV1`), its policy derived by
/// `PalwCourtMeasurementV1::policy` exactly as validation re-derives it, its digest carried as the TEST arm-readiness record's
/// measured-values evidence. The one-move time and sizes are this branch's DEVELOPMENT-MACHINE timings (macOS aarch64, debug build,
/// 2026-10-16; the `[court-op timing]` line of the run below re-prints them), not the slowest supported node's; the other classes are
/// NOT measured at all:
/// * one-move: the real floor leaf recompute of `legacy_v2_attempt_full_collusion_outside_bond_convicts` (`[court-op timing]`, its
///   object's encoded size);
/// * opening, dissection move, close: not driven by any harness yet — each is given the block's whole allotment (one per block, today's
///   rule), and the opening's size is the held opening of `palw_court_op_timing_harness`;
/// * working set: NOT measured — one byte per encoded byte (a floor, not a measurement);
/// * block allotment: 42 one-move verdicts' time, which keeps testnet-12's 42 slots; ρ_court 1,000 sompi (a test economic value).
pub(super) fn measured_court() -> kaspa_consensus_core::palw_court_capacity_v1::PalwCourtMeasurementV1 {
    use kaspa_consensus_core::palw_court_capacity_v1::*;
    const ONE_MOVE_NS: u64 = 213_458;
    const ONE_MOVE_BYTES: u64 = 35_669;
    const OPENING_BYTES: u64 = 10_218;
    let block_court_ns = 42 * ONE_MOVE_NS;
    PalwCourtMeasurementV1 {
        version: PALW_COURT_MEASUREMENT_VERSION_V1,
        class_ns: [ONE_MOVE_NS, block_court_ns, block_court_ns, block_court_ns],
        class_bytes: [ONE_MOVE_BYTES, OPENING_BYTES, ONE_MOVE_BYTES, ONE_MOVE_BYTES],
        class_memory: [ONE_MOVE_BYTES, OPENING_BYTES, ONE_MOVE_BYTES, ONE_MOVE_BYTES],
        block_court_ns,
        block_court_bytes: 42 * ONE_MOVE_BYTES.max(OPENING_BYTES),
        block_court_memory: 42 * ONE_MOVE_BYTES.max(OPENING_BYTES),
        rho_sompi: 1_000,
        ticket_window_daa: 0,
        // Round 2 (g14-court-capacity-r2): 42 one-move verdicts' time and bytes on their own budget; half the capacity reserved.
        block_one_move_ns: block_court_ns,
        block_one_move_bytes: 42 * ONE_MOVE_BYTES.max(OPENING_BYTES),
        reserve_permille: 500,
    }
}

/// **The network**: the parent's harness (testnet-12 as launched, harness cards, the main premine on the newcomer's key) with LG14-A,
/// LG14-B and option (b) armed at genesis through the `Params` — validated with their records, never set on a mirror alone. With
/// `full`, G14 FULLY armed for the legacy route (user decision 2026-10-16): the court-capacity fence too (`palw_court_capacity_v1`, the
/// policy [`measured_court`] derives, its record carrying that measurement's digest — the acceptance walk then holds the policy's court
/// budget instead of one court move a block, and the fused-leaf classes count as routed under option (b)) and the reward liability
/// horizon (`palw_reward_liability_horizon_v1`).
fn g14_close_config(full: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine, (TransactionOutpoint, UtxoEntry)) {
    let (config, _, mut premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    let funding = {
        let (outpoint, entry) = premine.iter_mut().find(|(o, _)| *o == main).expect("testnet-12 mints a main premine output");
        entry.script_public_key = card_payout_spk(NEWCOMER);
        (*outpoint, entry.clone())
    };
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &premine {
        multiset.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = multiset.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    let at = Some(ForkActivation::new(0));
    params.palw_legacy_public_filer_v1 = at;
    params.sync_palw_legacy_public_filer_v1();
    params.palw_legacy_held_da_v2 = at;
    params.palw_g14_route_required_v1 = at;
    let mut fences =
        vec![PalwG14GatedFenceV1::LegacyPublicFilerV1, PalwG14GatedFenceV1::LegacyHeldDaV2, PalwG14GatedFenceV1::G14RouteRequiredV1];
    if full {
        params.palw_court_capacity_v1 = Some(
            kaspa_consensus_core::palw_court_capacity_v1::PalwCourtCapacityFenceV1::measured_at(ForkActivation::new(0), measured_court())
                .expect("the measurement derives a policy"),
        );
        params.sync_palw_court_capacity_v1();
        fences.push(PalwG14GatedFenceV1::CourtCapacityV1);
        params.palw_reward_liability_horizon_v1 = at;
        fences.push(PalwG14GatedFenceV1::RewardLiabilityHorizonV1);
    }
    params.sync_palw_g14_gates_v1();
    let records: Vec<PalwG14ArmingRecordV1> = fences.into_iter().map(|fence| record(fence, params.genesis.hash, 0)).collect();
    params
        .validate_palw_v2_with_arming_records_v1(&records)
        .unwrap_or_else(|e| panic!("the armed ruleset passes the whole validation with its test records: {e:?}"));
    let unpinned = params.validate_palw_v2().expect_err("no record is pinned in this binary: a real network stays refused");
    assert!(format!("{unpinned:?}").contains("no arm-readiness record"), "{unpinned:?}");
    // Harness only (not a PALW rule): proof of work is not what is measured.
    params.skip_proof_of_work = true;
    let config = Config::new(params);
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is ConsensusV2") };
    let bundle = bundle.clone();
    assert!(bundle.state.legacy_public_filer_active_at(0), "LG14-A's mirror");
    let route = bundle.state.g14_route_required_v1().expect("option (b)'s mirror");
    assert!(route.route_armed(PalwG14RouteFamilyV1::LegacyPanelAttempt, 0, false), "the V2 Attempt family's route is armed at genesis");
    (config, bundle, premine, floats, funding)
}

fn g14_lg() -> Lg {
    g14_lg_on(false)
}

fn g14_lg_on(full: bool) -> Lg {
    let (config, bundle, premine, floats, newcomer_funding) = g14_close_config(full);
    assert_eq!(
        bundle.state.court_capacity_v1().map(|m| m.policy),
        full.then(|| measured_court().policy().expect("a policy")),
        "the court-capacity mirror"
    );
    let chain = t12_genesis_chain_on(TestConsensus::new(&config), &config, &bundle, &premine, &floats);
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let mut funding: BTreeMap<usize, (TransactionOutpoint, UtxoEntry)> = floats.iter().cloned().enumerate().collect();
    funding.insert(NEWCOMER, newcomer_funding);
    let backend = backend_of(&config, &bundle);
    Lg { chain, config, bundle, premine, floats, funding, domain, backend, newcomer: None, nonce: 0, rnd: 0, _keep: Vec::new() }
}

/// The licence the chain relied on, read off the accepted blocks (public): each seat's receipt in the form it was licensed in.
fn licence_receipts(lg: &Lg, claim: Hash64) -> Vec<PalwFalseValidReceiptV1> {
    lg.recent_objects(512)
        .into_iter()
        .find_map(|object| match object {
            Obj::ReceiptLicensed { claim: c, receipts } if c == claim => {
                Some(receipts.into_iter().map(PalwFalseValidReceiptV1::Full).collect())
            }
            Obj::ReceiptLicensedV2 { claim: c, receipts } if c == claim => {
                Some(receipts.into_iter().map(PalwFalseValidReceiptV1::Segmented).collect())
            }
            _ => None,
        })
        .expect("the licence rides an accepted block")
}

/// The guilty leaf's evidence the producer disclosed on chain (the one-move court's object, authenticated by the fold before it
/// convicted): the committed tile and its opening, the canonical inputs, the artifact rows' openings and the prompt tile.
fn disclosed_leaf_evidence(lg: &Lg, claim: Hash64, leaf: u64) -> kaspa_consensus_core::palw_shard_court_v1::PalwLeafEvidenceV1 {
    lg.recent_objects(512)
        .into_iter()
        .find_map(|object| match object {
            Obj::MaterialDisclosedV2 {
                claim: c,
                unit: PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: l }),
                answer: PalwDaAnswerV1::Held(carriage),
                ..
            } if c == claim && l == leaf => match carriage.disclosure {
                PalwHeldDisclosureV1::StepLeaf { evidence } => Some(*evidence),
                _ => None,
            },
            _ => None,
        })
        .expect("the guilty leaf's disclosure rides an accepted block")
}

/// **The colluding producer answers what it owes under LG14-B's D2 rule** (`palw_legacy_held_da_v2` in force): a held `StepLeaf`
/// answer carries the leaf's COMMITTED half only — no artifact rows, which are never compelled (ADR-0177 D2); every other unit as the
/// parent's [`Lg::producer_answers`] answers it. Built from the producer's own capture (its obligation, never the verifier's material).
async fn producer_answers_d2(lg: &mut Lg, produced: &Produced) -> bool {
    let duties = lg.chain.ctx.consensus.palw_disclosure_duties_v1(vec![lg.chain.bonds[EXECUTOR]]);
    let form = lg.config.params.palw_prompt_ids_form_v1();
    let work_leaves = lg.view(produced.claim_id).work_leaves;
    let mut objects = Vec::new();
    for duty in duties.duties.iter().filter(|d| d.claim_id == produced.claim_id) {
        let answer = match duty.unit {
            PalwDaUnitV1::Event { row, tile } => PalwDaAnswerV1::Event(
                lg.backend.disclose_trace_event(&produced.material, row, tile).expect("the capture opens the event"),
            ),
            PalwDaUnitV1::Held(missing) => {
                let (binding, mut disclosure) = palw_da_held_disclosure_from_capture_v1(
                    &lg.backend,
                    &produced.material,
                    &produced.prompt,
                    produced.roots,
                    work_leaves,
                    missing,
                    form,
                    || lg.backend.disclose_trace_event(&produced.material, u32::MAX, u8::MAX).map(|d| d.binding().clone()),
                )
                .unwrap_or_else(|e| panic!("the producer's capture answers {missing:?}: {e}"));
                if let PalwHeldDisclosureV1::StepLeaf { evidence } = &mut disclosure {
                    evidence.artifact_openings.clear();
                }
                palw_da_held_answer_v1(produced.claim_id, missing, binding, disclosure)
            }
            other => panic!("a unit this case does not demand: {other:?}"),
        };
        lg.rnd = lg.rnd.wrapping_add(1);
        let rnd = lg.rnd;
        let object = palw_da_answer_object_v1(
            &lg.domain,
            produced.claim_id,
            duty.unit,
            answer,
            duty.discloser,
            lg.bundle.court.max_close_bytes(),
            |message, context| Some(sign(EXECUTOR, message, context, rnd)),
        )
        .unwrap_or_else(|e| panic!("the node's builder builds the answer: {e}"));
        objects.push((EXECUTOR, object));
    }
    if objects.is_empty() {
        return false;
    }
    lg.send(objects).await;
    true
}

/// The common engine (`palw_fraud_filer_next_v1`, the parent's [`Verifier::step`]) run until its next move is the terminal demand of
/// the located leaf; returns the leaf. The binding read and the consecutive authenticated ranges are the parent's.
async fn pursue_to_terminal(v: &mut Verifier, lg: &mut Lg, produced: &Produced, cap: usize) -> u64 {
    for _ in 0..cap {
        let facts = v.facts(lg);
        let binding = v.binding.clone();
        let demandable = |leaf: u64| binding.as_ref().is_some_and(|b| !palw_da_step_leaf_is_fused_v1(b, leaf));
        if let PalwFilerActionV1::Demand(PalwLegacyProbeV1::Terminal { leaf }) =
            palw_fraud_filer_next_v1(PalwFilerRoleV1::PublicBond, &facts, v.mismatch(lg), &v.bisect, demandable)
        {
            return leaf;
        }
        assert_eq!(v.step(lg, produced, &|_| false).await, None, "the pursuit reaches its terminal before any verdict");
    }
    panic!("no terminal in {cap} steps ({} demands), localizer {:?}", v.demands, v.bisect);
}

/// **The headline case (audit G-01 option (a), G-06, G-13): producer + every seat `Valid` on a forged leaf; one bond registered after
/// genesis, outside the Panel, convicts through the public filer from public material only.** Asserted, on the node:
/// option (b)'s gate admits the V2 Attempt (its route is armed); the reservation holds `Final` past its floor; the lie is localized
/// on the reservation's own budget; under LG14-B's D2 rule the producer discloses the leaf's committed half only and the outsider's
/// one-move accusation, with rows from its own model copy, convicts the producer and voids the claim; every relied-on `Valid` seat is
/// then convicted by a kind-3 filing built from that public material and the licence on chain, and slashed (its lock alive — G-13);
/// the claim never reaches `Final`, so no safe weight, no vesting row, its root forfeited; the outsider is made whole; a replaying node
/// reaches the same roots.
#[tokio::test]
async fn legacy_v2_attempt_full_collusion_outside_bond_convicts() {
    full_collusion_outside_bond_convicts(false).await;
}

/// **The same headline case with G14 fully armed** (lane g14-court-capacity: the court-capacity fence with its measured config, the
/// reward liability horizon): every court object rides the node's acceptance walk under the policy's block budget (no longer one court
/// move a block), the one-move accusation is charged its work units, and the outsider's conviction, the seats' slashes and the replay
/// are unchanged.
#[tokio::test]
async fn legacy_v2_attempt_full_collusion_outside_bond_convicts_with_court_capacity() {
    full_collusion_outside_bond_convicts(true).await;
}

async fn full_collusion_outside_bond_convicts(court: bool) {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg_on(court);
    lg.beat(3).await;
    let newcomer = lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, true).await;
    let id = produced.claim_id;
    let accepted = lg.view(id).accepted_daa;
    // Option (b): the V2 Attempt family has an armed G14 route at its acceptance, so it was admitted (with its reward and weight).
    assert_eq!(palw_g14_route_attempt_refusal_v1(&lg.bundle.state, accepted), None, "option (b) counts V2 Attempt as routed");
    let claim = lg.chain.tip_state().1.claim(&id).expect("the claim").clone();
    assert!(claim.escrowed_reward > 0 || claim.pwu > 0, "an admitted attempt carries its reward and weight");
    // Discover → Check from public reads and the verifier's own replay; reserve before the panel binds.
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert!(v.mismatch(&lg), "the verifier's own replay does not reproduce the committed root");
    let newcomer_before = lg.collateral(NEWCOMER);
    assert_eq!(v.step(&mut lg, &produced, &|_| false).await, None, "the engine reserves first");
    assert!(lg.view(id).record.is_some_and(|r| r.live.contains_key(&newcomer)), "the reservation landed");
    // Every seat colludes.
    let seats = lg.bind(id).await;
    let licensed = lg.license(id, &seats).await;
    let (_, state) = lg.chain.tip_state();
    let safe_before = state.safe_weight();
    let producer_before = lg.collateral(EXECUTOR);
    let seat_before: Vec<(usize, u64)> = seats.iter().map(|card| (*card, lg.collateral(*card))).collect();
    for card in &seats {
        assert!(state.slashable_lock(lg.bond(*card), id).is_some(), "card {card}'s Valid is locked");
    }
    // G-06: Final held until the pursuit closes — past the claim's floor, still licensed, no deadline.
    let floor = licensed + lg.bundle.state.window_challenge_at(licensed);
    lg.beat_to(floor + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the reservation holds Final past its floor");
    assert_eq!(lg.view(id).deadline_daa, None, "held: no Final deadline");
    // The pursuit: binding read, consecutive authenticated ranges against the verifier's own run, to the located leaf.
    let lie = pursue_to_terminal(&mut v, &mut lg, &produced, 400).await;
    assert_eq!(Some(lie), produced.lie_leaf, "localized at the lie");
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "still held while the pursuit runs");
    // The terminal (LG14-B, ADR-0177 D2): the reserved demand of the leaf; the producer owes its committed half only — no model rows.
    let object = v.demand(&mut lg, PalwLegacyProbeV1::Terminal { leaf: lie });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(producer_answers_d2(&mut lg, &produced).await, "the producer answers the committed half it owes");
    let evidence = disclosed_leaf_evidence(&lg, id, lie);
    assert!(evidence.artifact_openings.is_empty(), "D2: no model rows were compelled from the producer");
    assert!(!lg.view(id).court_convicted, "retrieval is not adjudication: nothing convicted yet");
    // The verifier's own model copy supplies the rows; the one-move accusation (signed by the outsider) convicts the producer.
    let own_rows = lg.backend.operand_openings_for(&evidence.refutation).expect("the verifier's own copy opens the leaf's rows");
    let view = lg.view(id);
    let mut accusation = kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtAccusationV1 {
        version: kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        executor_bond: view.producer,
        accuser_bond: newcomer,
        leaf_index: lie,
        refutation: evidence.refutation.clone(),
        artifact_openings: own_rows.clone(),
        prompt_ids_opening: evidence.prompt_ids_opening.clone(),
        signature: Vec::new(),
    };
    let sid = kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_session_id_v1(lg.domain.as_byte_slice(), &accusation);
    lg.rnd = lg.rnd.wrapping_add(1);
    accusation.signature =
        sign(NEWCOMER, sid.as_byte_slice(), kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, lg.rnd);
    // `[court-op timing]` (the court-capacity fence's timing harness, its node half): the one-move verdict — the leaf recompute on
    // the verifier's real model rows — timed here on this machine and build. NOT a measured record (that is the slowest supported
    // node's, release build, at each class's maximum profile).
    {
        let (_, state) = lg.chain.tip_state();
        let class_id = state.claim(&id).expect("the claim").class_id;
        let bound_to = kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2 {
            execution_root: view.execution_root,
            class_id,
            artifact_root: state.class(&class_id).expect("the class").artifact_root,
        };
        let daa = lg.daa();
        let ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(
            &lg.bundle.court,
            lg.config.params.palw_court_ladder_active_at(daa),
        );
        let mut samples: Vec<u128> = (0..5)
            .map(|_| {
                let t = std::time::Instant::now();
                let verdict =
                    kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_verdict_at_v2(&accusation, &bound_to, ladder, true);
                let elapsed = t.elapsed().as_nanos();
                assert!(
                    matches!(verdict, Ok(kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty)),
                    "{verdict:?}"
                );
                elapsed
            })
            .collect();
        samples.sort_unstable();
        let object = Obj::ShardCourtAccused { accusation: Box::new(accusation.clone()) };
        eprintln!(
            "[court-op timing] one-move verdict (floor class, real rows): {} B object, {} rows; ns min {} median {} max {} — {}-{} {} build, NOT a measured record",
            borsh::to_vec(&object).map(|b| b.len()).unwrap_or(0),
            own_rows.len(),
            samples[0],
            samples[samples.len() / 2],
            samples[samples.len() - 1],
            std::env::consts::OS,
            std::env::consts::ARCH,
            if cfg!(debug_assertions) { "debug" } else { "release" },
        );
    }
    lg.send(vec![(NEWCOMER, Obj::ShardCourtAccused { accusation: Box::new(accusation) })]).await;
    let view = lg.view(id);
    eprintln!("[g14-close] {} demands; lie at {lie}; phase {:?}", v.demands, view.phase);
    assert!(view.court_convicted, "an objective court conviction is recorded");
    assert!(matches!(view.phase, PalwClaimPhaseV2::Voided { .. }), "the claim is void: {:?}", view.phase);
    assert!(lg.collateral(EXECUTOR) < producer_before, "the producer is slashed");
    // Every relied-on Valid seat: a kind-3 filing from public material — the disclosed committed half, the verifier's own rows and
    // the licence, all on chain or its own.
    let receipts = licence_receipts(&lg, id);
    assert_eq!(receipts.len(), seats.len(), "one receipt per colluding seat");
    let mut filings = Vec::new();
    let mut liable = Vec::new();
    for receipt in receipts {
        let seat = PalwBondKeyV2(receipt.inner().seat_bond.0);
        let card = lg.card_of(&seat);
        let object = PalwPanelFalseValidEvidenceV2::filed_v2(
            id,
            receipt,
            PalwPanelContradictionV1::StepArithmetic { refutation: evidence.refutation.clone(), operand_openings: own_rows.clone() },
            evidence.prompt_ids_opening.clone(),
        )
        .object_v2();
        match lg.chain.ctx.consensus.palw_false_valid_filing_check_v1(object.clone()) {
            Some(PalwFalseValidFilingCheckV1::File { full_attestation, .. }) => {
                eprintln!("[g14-close] card {card}: liable (full attestation {full_attestation})");
                liable.push(card);
                filings.push((NEWCOMER, object));
            }
            other => eprintln!("[g14-close] card {card}: not filed ({other:?})"),
        }
    }
    assert!(!filings.is_empty(), "the colluding seats are liable for the located lie");
    lg.send(filings).await;
    let (_, state) = lg.chain.tip_state();
    for (card, before) in &seat_before {
        let seat = lg.bond(*card);
        if liable.contains(card) {
            assert!(lg.collateral(*card) < *before, "card {card}: the Valid seat is slashed");
            assert!(state.slashable_lock(seat, id).is_none(), "card {card}: its lock is taken (it was alive: G-13)");
            assert!(state.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &id)).is_some(), "card {card}: one kind-3 row");
        }
    }
    // Every seat whose receipt attested the whole job is liable for the lie (a full Valid covers every leaf).
    let full_seats: Vec<usize> = licence_receipts(&lg, id)
        .into_iter()
        .filter(|r| matches!(r, PalwFalseValidReceiptV1::Full(_)))
        .map(|r| lg.card_of(&PalwBondKeyV2(r.inner().seat_bond.0)))
        .collect();
    for card in &full_seats {
        assert!(liable.contains(card), "card {card}'s full Valid covers the lie");
    }
    eprintln!("[g14-close] {} seats, {} liable and slashed, {} full receipts", seats.len(), liable.len(), full_seats.len());
    // Coinbase, weight and safe weight refused: never Final, no vesting row, the root forfeited, safe weight unchanged.
    let (_, state) = lg.chain.tip_state();
    assert_eq!(state.safe_weight(), safe_before, "no safe weight from the convicted claim");
    assert!(state.vesting_row(&id).is_none(), "no vesting row: its reward never vests");
    assert!(state.palw_execution_root_is_forfeited_v1(&claim.execution_root), "the proven-false root is forfeited");
    assert!(!matches!(state.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. })));
    if kaspa_consensus_core::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&state, &lg.bond(EXECUTOR)) {
        eprintln!("[g14-close] the producer's bond is frozen (ADR-0160): it produces nothing further");
    }
    // The outsider: every deposit and refuted exposure returned.
    assert!(lg.view(id).record.is_none(), "the reservation closed with the conviction");
    assert_eq!(lg.exposure(NEWCOMER), 0);
    assert!(lg.collateral(NEWCOMER) >= newcomer_before, "the honest verifier is not charged");
    // A node replaying the chain reaches the same roots, block by block.
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        let hash = block.header.hash;
        arrive(&z, block, "the first node's block").await;
        assert_eq!(root_at(&z, hash), root_at(&lg.chain, hash), "the replaying node's root at {hash}");
    }
}

/// **LG14-B's held unit withheld: the objective DA default, never a presumed computation fault.** The outsider reserves, reads the
/// binding, and demands the public descent's first unit (the step tree's root node, tag 157) on its reservation's own budget; the
/// colluding producer stays silent. At the session's deadline the claim is voided `ProducerWithholding` — `da_defaulted`, with no
/// court conviction and no executor refutation recorded — the producer is charged, no seat is charged for a unit its automatic
/// answering never built, and the outsider is made whole. The claim never reaches `Final`.
#[tokio::test]
async fn legacy_v2_attempt_held_da_withholding_ends_in_da_default() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let (produced, seats, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    let newcomer = lg.bond(NEWCOMER);
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    let withhold = |unit: &PalwDaUnitV1| matches!(unit, PalwDaUnitV1::LegacyHeldV2(_));
    // Reserve, then the binding read (answered: the producer owes it and answers).
    for _ in 0..8 {
        if v.binding.is_some() {
            break;
        }
        v.step(&mut lg, &produced, &withhold).await;
    }
    let binding = v.binding.clone().expect("the binding, read off the producer's authenticated answer");
    assert!(lg.view(id).record.is_some_and(|r| r.live.contains_key(&newcomer)), "reserved");
    let producer_before = lg.collateral(EXECUTOR);
    let seat_before: Vec<(usize, u64)> = seats.iter().map(|card| (*card, lg.collateral(*card))).collect();
    let safe_before = lg.chain.tip_state().1.safe_weight();
    // The descent's first unit: the root node of the claim's committed step tree (tag 157).
    let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(binding.step_leaf_count);
    let unit = PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
    let object = v.demand(&mut lg, PalwLegacyProbeV1::HeldNode { unit });
    assert!(matches!(object, Obj::LegacyHeldDemandedV2 { .. }), "tag 157");
    lg.send(vec![(NEWCOMER, object)]).await;
    let session = lg
        .view(id)
        .sessions
        .iter()
        .find(|(accuser, session)| *accuser == newcomer && session.units.contains(&PalwDaUnitV1::LegacyHeldV2(unit)))
        .map(|(_, session)| session.clone())
        .expect("the reserved held session is open");
    assert!(session.accuser_is_seat, "seat-like: it pauses the claim (a reserved session)");
    assert_eq!(lg.view(id).record.unwrap().live[&newcomer].sessions_opened, 2, "binding read + this unit, on its own budget");
    // The producer stays silent on the held unit (it keeps answering everything else it owes).
    lg.beat_to(session.deadline_daa + 2).await;
    for _ in 0..4 {
        if lg.view(id).da_defaulted {
            break;
        }
        lg.producer_answers(&produced, &withhold).await;
        lg.beat(1).await;
    }
    let view = lg.view(id);
    eprintln!("[g14-close held DA] phase {:?}, licensed {licensed}, now {}", view.phase, lg.daa());
    assert!(view.da_defaulted, "the objective DA default");
    assert!(
        matches!(view.phase, PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::ProducerWithholding, .. }),
        "voided ProducerWithholding: {:?}",
        view.phase
    );
    assert!(!view.court_convicted && !view.executor_refuted, "silence is never a presumed computation fault");
    assert!(lg.collateral(EXECUTOR) < producer_before, "the withholding producer is charged");
    for (card, before) in &seat_before {
        assert_eq!(lg.collateral(*card), *before, "card {card}: no seat is charged for a held unit it never owed");
    }
    let (_, state) = lg.chain.tip_state();
    assert_eq!(state.safe_weight(), safe_before, "no weight");
    assert!(state.vesting_row(&id).is_none());
    assert!(lg.view(id).record.is_none(), "the default released the reservation");
    assert_eq!(lg.exposure(NEWCOMER), 0, "the outsider is made whole");
}

/// **An honest V2 Attempt under G14 fully armed reaches `Final` and is paid after the liability-horizon hold** (user decision
/// 2026-10-16: the testnet-12 fence moves only once G14 is fully armed with option (b) kept AND PALW rewards, block production and
/// `Final` proceed normally). LG14-A, LG14-B, option (b), the court-capacity fence (the measured config [`measured_court`]) and the
/// reward liability horizon are armed at genesis and validated with their records. Asserted on the node: option (b) counts the floor
/// AND the fused-leaf classes as routed (court capacity in force); a real honest floor attempt is admitted with its reward, bound,
/// licensed by every seat, and reaches `Final` at its own deadline with safe weight; its vesting row holds the reward to
/// `F + window_court` (the liability horizon) — nothing moves before — and once both clocks are met the row matures and the producer's
/// leg is minted by a real coinbase; blocks keep coming throughout.
#[tokio::test]
async fn legacy_v2_attempt_honest_claim_is_final_and_paid_under_full_g14() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg_on(true);
    // Option (b) under the full arming: every genesis class is routed, the fused ones because court capacity is in force.
    {
        let (_, state) = lg.chain.tip_state();
        let daa = lg.daa();
        let mut fused = 0;
        for class_id in state.classes_iter().map(|(id, _)| *id).collect::<Vec<_>>() {
            let is_fused = state.class(&class_id).is_some_and(|c| c.fused_attention);
            fused += usize::from(is_fused);
            assert_eq!(
                kaspa_consensus_core::palw_g14_gates_v1::palw_g14_route_attempt_refusal_for_class_v1(&lg.bundle.state, daa, is_fused),
                None,
                "class {class_id} (fused {is_fused}) is routed under the full arming"
            );
        }
        assert!(fused > 0, "testnet-12 has fused-leaf classes, and they are routed");
    }
    let (produced, seats, licensed) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let (_, state) = lg.chain.tip_state();
    let claim = state.claim(&id).expect("the claim").clone();
    assert!(claim.escrowed_reward > 0, "an admitted honest attempt carries its reward");
    let safe_before = state.safe_weight();
    // Final at its own deadline (nothing holds it: no reservation, no court).
    let deadline = state.deadline_of(&id).expect("a licensed claim owes its Final deadline");
    assert!(deadline >= licensed, "{deadline} ≥ {licensed}");
    lg.beat_to(deadline + 1).await;
    let (_, state) = lg.chain.tip_state();
    let PalwClaimPhaseV2::Final { final_daa } = state.claim(&id).expect("the claim").phase else {
        panic!("Final at its deadline: {:?}", state.claim(&id).map(|c| &c.phase))
    };
    assert!(state.safe_weight() > safe_before, "Final adds the claim's safe weight");
    let wc = lg.bundle.state.window_court();
    let row = state.vesting_row(&id).expect("a Final names a vesting row").clone();
    assert_eq!(row.expiry_daa, final_daa + wc, "the reward is held to the liability horizon F + window_court");
    assert!(row.matured_at.is_none());
    let key = kaspa_consensus_core::palw_vesting_v1::palw_vesting_payout_key_v1(&id);
    let moved_before = state.vesting_counters().moved;
    // Nothing moves before the horizon.
    lg.beat_to(final_daa + wc - 1).await;
    let (_, state) = lg.chain.tip_state();
    assert!(state.vesting_row(&id).is_some_and(|r| r.matured_at.is_none()), "held until F + window_court");
    assert!(state.pending_payout(&key).is_none());
    assert_eq!(state.vesting_counters().moved, moved_before);
    // Past it, once the second clock is met (its licences or its liveness escape), the row matures and the producer's leg — fixed at
    // Final — is minted by a real coinbase (the block that moves it, or the next).
    let leg = row.producer.amount;
    assert!(leg > 0, "the producer's leg carries the reward");
    let bound = row.expiry_daa + 2 * wc + 64;
    let ttpb = lg.ttpb();
    let mut moved_at = None;
    let mut paid_at = None;
    while paid_at.is_none() {
        assert!(lg.daa() <= bound, "the row matures by its second clock's bound ({bound})");
        let block = lg.chain.heartbeat(ttpb, Vec::new()).await;
        let (_, state) = lg.chain.tip_state();
        if moved_at.is_none() && state.vesting_row(&id).is_none() {
            moved_at = Some(lg.daa());
        }
        if moved_at.is_some() && block.transactions[0].outputs.iter().any(|o| o.value == leg) {
            assert!(state.pending_payout(&key).is_none(), "the minted leg leaves the queue");
            paid_at = Some(lg.daa());
        }
        assert!(moved_at.is_none_or(|at| lg.daa() <= at + 3), "a moved leg is minted within a block or two");
    }
    let (_, state) = lg.chain.tip_state();
    assert!(state.vesting_counters().moved > moved_before, "the row's reward moved to payment");
    eprintln!(
        "[g14-full] honest claim {id}: licensed {licensed}, Final {final_daa}, held to {} (F + {wc}), matured at {:?}, producer leg {leg} sompi minted at {}; {} seats",
        row.expiry_daa,
        moved_at,
        paid_at.unwrap(),
        seats.len()
    );
}
