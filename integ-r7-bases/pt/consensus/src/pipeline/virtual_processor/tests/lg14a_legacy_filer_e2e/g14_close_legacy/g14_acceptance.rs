//! **The external audit's DAA-16,000 acceptance tests D, E, F and G (2026-10-17, `docs/design/palw/g14-external-audit-2026-10-17/
//! rfc1_20_audit_ja.md` §8) on the real node, for the legacy V2 Attempt route with LG14-A + LG14-B + option (b) armed through
//! normal validated `Params` on the existing marked isolated G14 genesis** (the parent's `g14_close_config`, never a mirror-only bypass).
//!
//! Test A (every seat `Valid`, one bond registered after genesis outside the Panel convicts from the node's read API, accepted blocks
//! and its own model copy, never the producer's instance) is the parent's `legacy_v2_attempt_full_collusion_outside_bond_convicts`;
//! D's "withheld → DA default" is `legacy_v2_attempt_held_da_withholding_ends_in_da_default`. This module adds:
//!
//! * **D**: a false accusation of an honest claim is refused and costs the honest producer nothing; a verifier whose OWN
//!   implementation differs (its replay is wrong at one leaf) localizes "its" divergence, but the one-move court recomputes the
//!   producer's committed leaf from the registered model and refuses the accusation — an implementation difference is never
//!   classified as producer fraud.
//! * **E**: the conviction actually reduces the producer's and each liable seat's collateral by exactly what each offence records as
//!   `collected`; the reporter-side credit for every offence is at most what that offence collected, at every block after it; a
//!   second filing of the same accusation or kind-3 evidence collects nothing; the convicted claim's reward, weight and root stay
//!   void (no vesting row, no payout, the root forfeited) long past its would-be Final floor; a replaying node reaches the same roots.
//! * **F**: the same objective conviction when the outsider first acts at `Final − 1` (the last blocks before the floor) and when it
//!   first acts after `Final` inside the liability; the producer's exit request raced against the pursuit does not escape the slash;
//!   a liable seat cannot exit while its `Valid` lock is alive; the post-Final pursuit is open up to the vesting row's DAA expiry and
//!   the reward matures only past it (it also waits on the settlement clock) — the pursuit window and the payout never overlap (the
//!   boundary, stated, not a silent pass). Reorg, restart, full IBD and pruned import of the pursuit are the
//!   parent's `lg14a_*` / `g14r_resume` cases (same fold rules; their harness sets the fence on its mirror) plus the replay roots
//!   asserted here.
//! * **G**: on a network whose G14 set is armed at a height `H`, honest claims accepted at `H − 1`, exactly `H` and `H + 1` are each
//!   licensed by honest seats, reach `Final` at their floor, get their vesting row (the reward) and stand in the selected chain (the
//!   block opportunity) — safety is not bought by stopping honest work.
//!
//! Tests B (per-family detection rates) and C (court capacity and TIR owner admission under saturation) belong to other lanes.
//! No cryptographic computation proof of any kind is used (PRINCIPLES §8): conviction is the shipped one-move court on public
//! material and the verifier's own replay.
use super::*;
use kaspa_consensus_core::palw_state_v2::{PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT, palw_bond_retirement_message_v2};
use kaspa_consensus_core::palw_vesting_v1::{palw_reporter_payout_key_v1, palw_vesting_payout_key_v1};

type LeafEvidence = kaspa_consensus_core::palw_shard_court_v1::PalwLeafEvidenceV1;
type Opening = kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1;

// ---- harness helpers (not rules) -------------------------------------------------------------------------------------------------

/// Send `object` from `card` and report whether the node took it: the mempool's verdict, else the template's. A refused carrier
/// leaves the actor's funding chain where it was.
async fn try_send(lg: &mut Lg, card: usize, object: &Obj) -> Result<(), String> {
    let saved = lg.funding[&card].clone();
    let tx = lg.carrier(card, object);
    if let Err(e) = lg.mempool(&tx) {
        lg.funding.insert(card, saved);
        return Err(format!("mempool: {e}"));
    }
    let ttpb = lg.ttpb();
    let block = lg.chain.heartbeat(ttpb, vec![tx.clone()]).await;
    if !block.transactions.iter().any(|t| t.id() == tx.id()) {
        lg.funding.insert(card, saved);
        return Err("the node's template did not carry it".into());
    }
    lg.chain.heartbeat(ttpb, Vec::new()).await;
    Ok(())
}

/// The one-move accusation `accuser` signs: the producer's disclosed committed half of `leaf` and the rows of the verifier's own
/// model copy. Returns the object, the disclosed evidence and the verifier's rows.
fn one_move_accusation(lg: &mut Lg, id: Hash64, leaf: u64, accuser: usize) -> (Obj, LeafEvidence, Vec<Opening>) {
    let evidence = disclosed_leaf_evidence(lg, id, leaf);
    let own_rows = lg.backend.operand_openings_for(&evidence.refutation).expect("the verifier's own copy opens the leaf's rows");
    let view = lg.view(id);
    let mut accusation = kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtAccusationV1 {
        version: kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        executor_bond: view.producer,
        accuser_bond: lg.bond(accuser),
        leaf_index: leaf,
        refutation: evidence.refutation.clone(),
        artifact_openings: own_rows.clone(),
        prompt_ids_opening: evidence.prompt_ids_opening.clone(),
        signature: Vec::new(),
    };
    let sid = kaspa_consensus_core::palw_shard_court_v1::palw_shard_court_session_id_v1(lg.domain.as_byte_slice(), &accusation);
    lg.rnd = lg.rnd.wrapping_add(1);
    accusation.signature =
        sign(accuser, sid.as_byte_slice(), kaspa_consensus_core::palw_shard_court_v1::PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, lg.rnd);
    (Obj::ShardCourtAccused { accusation: Box::new(accusation) }, evidence, own_rows)
}

/// The kind-3 filings against every relied-on `Valid` seat the node's check would accept for the located lie (public material only),
/// with the seat's card.
fn false_valid_filings(lg: &Lg, id: Hash64, evidence: &LeafEvidence, own_rows: &[Opening]) -> Vec<(usize, Obj)> {
    let mut filings = Vec::new();
    for receipt in licence_receipts(lg, id) {
        let seat = PalwBondKeyV2(receipt.inner().seat_bond.0);
        let card = lg.card_of(&seat);
        let object = PalwPanelFalseValidEvidenceV2::filed_v2(
            id,
            receipt,
            PalwPanelContradictionV1::StepArithmetic { refutation: evidence.refutation.clone(), operand_openings: own_rows.to_vec() },
            evidence.prompt_ids_opening.clone(),
        )
        .object_v2();
        if let Some(PalwFalseValidFilingCheckV1::File { .. }) = lg.chain.ctx.consensus.palw_false_valid_filing_check_v1(object.clone()) {
            filings.push((card, object));
        }
    }
    filings
}

/// An openable, non-fused step leaf in the second half of `binding`'s run (where the parent's lie is put).
fn second_half_leaf(binding: &PalwStepBindingV2, held: Option<&std::collections::BTreeSet<u64>>) -> u64 {
    let n = binding.step_leaf_count;
    (n / 2..n)
        .find(|leaf| {
            held.is_none_or(|h| h.contains(leaf))
                && kaspa_consensus_core::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, *leaf)
                    .is_some()
                && !palw_da_step_leaf_is_fused_v1(binding, *leaf)
        })
        .expect("an openable, non-fused leaf")
}

fn reserve_by_hand(lg: &Lg, id: Hash64, rnd: u8) -> Obj {
    let view = lg.view(id);
    let reservation = PalwDisputeReservationV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        reserver: lg.bond(NEWCOMER),
    };
    palw_dispute_reserved_object_v1(lg.domain, reservation, |m| {
        sign(NEWCOMER, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, rnd)
    })
}

fn release(lg: &Lg, id: Hash64, rnd: u8) -> Obj {
    palw_dispute_released_object_v1(lg.domain, id, lg.bond(NEWCOMER), |m| {
        sign(NEWCOMER, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, rnd)
    })
}

/// When the outside verifier first acts on the lying claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FirstLook {
    /// Before the panel is bound (the parent's headline case).
    BeforeBind,
    /// The last blocks before the claim's Final floor: its reservation lands while the claim is still licensed.
    FinalMinusOne,
    /// After the claim reached `Final`, inside the liability (its vesting row unmatured).
    AfterFinal,
}

/// What a full-collusion conviction left behind, for the acceptance assertions.
struct Convicted {
    produced: Produced,
    seats: Vec<usize>,
    accusation: Obj,
    filings: Vec<(usize, Obj)>,
    /// Collateral before the conviction, by card (the producer and every seat).
    before: BTreeMap<usize, u64>,
    safe_before: u128,
}

/// **The headline flow, parameterised by when the outsider first looks**: producer + every seat `Valid` on a forged leaf; the
/// newcomer (registered after genesis, never a seat) reserves, localizes on its reservation's budget, demands the leaf's committed
/// half (LG14-B D2: no model rows compelled), and convicts with the one-move accusation over its own model's rows; then files kind-3
/// evidence against every liable `Valid` seat.
async fn convict(lg: &mut Lg, first_look: FirstLook) -> Convicted {
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, true).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(lg, NEWCOMER, id);
    assert!(v.mismatch(lg), "the verifier's own replay does not reproduce the committed root");
    if first_look == FirstLook::BeforeBind {
        assert_eq!(v.step(lg, &produced, &|_| false).await, None, "the engine reserves first");
    }
    let seats = lg.bind(id).await;
    let licensed = lg.license(id, &seats).await;
    let floor = licensed + lg.bundle.state.window_challenge_at(licensed);
    match first_look {
        FirstLook::BeforeBind => {}
        FirstLook::FinalMinusOne => {
            // The reservation's carrier and its folding block land in the last blocks before the floor.
            lg.beat_to(floor.saturating_sub(4)).await;
            assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "still licensed just before the floor");
            assert_eq!(v.step(lg, &produced, &|_| false).await, None, "the engine reserves at Final − 1");
            eprintln!("[g14-accept F-1] reserved at DAA {} (floor {floor})", lg.daa());
            assert!(lg.view(id).record.is_some_and(|r| r.live.contains_key(&lg.bond(NEWCOMER))), "the late reservation landed");
            lg.beat_to(floor + 2).await;
            assert!(
                matches!(lg.view(id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
                "the reservation made at Final − 1 holds Final past the floor: {:?}",
                lg.view(id).phase
            );
        }
        FirstLook::AfterFinal => {
            lg.beat_to(floor + 2).await;
            assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "nobody looked: Final");
            assert!(lg.chain.tip_state().1.vesting_row(&id).is_some_and(|r| r.matured_at.is_none()), "inside the liability");
            assert_eq!(v.step(lg, &produced, &|_| false).await, None, "the engine reserves after Final");
            assert!(lg.view(id).record.is_some_and(|r| r.live.contains_key(&lg.bond(NEWCOMER))), "the post-Final reservation landed");
        }
    }
    let safe_before = lg.chain.tip_state().1.safe_weight();
    let mut before: BTreeMap<usize, u64> = seats.iter().map(|card| (*card, lg.collateral(*card))).collect();
    before.insert(EXECUTOR, lg.collateral(EXECUTOR));
    let lie = pursue_to_terminal(&mut v, lg, &produced, 400).await;
    assert_eq!(Some(lie), produced.lie_leaf, "{first_look:?}: localized at the lie");
    let object = v.demand(lg, PalwLegacyProbeV1::Terminal { leaf: lie });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(producer_answers_d2(lg, &produced).await, "the producer answers the committed half it owes");
    let (accusation, evidence, own_rows) = one_move_accusation(lg, id, lie, NEWCOMER);
    assert!(evidence.artifact_openings.is_empty(), "D2: no model rows were compelled from the producer");
    lg.send(vec![(NEWCOMER, accusation.clone())]).await;
    let view = lg.view(id);
    assert!(view.court_convicted, "{first_look:?}: an objective court conviction is recorded ({:?})", view.phase);
    assert!(lg.collateral(EXECUTOR) < before[&EXECUTOR], "{first_look:?}: the producer is slashed");
    let filings = false_valid_filings(lg, id, &evidence, &own_rows);
    assert!(!filings.is_empty(), "{first_look:?}: the colluding seats are liable for the located lie");
    let sent: Vec<(usize, Obj)> = filings.iter().map(|(_, object)| (NEWCOMER, object.clone())).collect();
    lg.send(sent).await;
    for (card, _) in &filings {
        assert!(lg.collateral(*card) < before[card], "{first_look:?}: liable seat card {card} is slashed");
    }
    Convicted { produced, seats, accusation, filings, before, safe_before }
}

/// The reporter-side credit the chain holds for offence `key`: its pending (reveal-window) reward, its awarded row and its queued
/// payout — whichever stage the award is in.
fn reporter_credit(state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2, key: &Hash64) -> u64 {
    state.reward_pending(key).map(|r| r.amount).unwrap_or(0)
        + state.reporter_reward(key).map(|r| r.amount).unwrap_or(0)
        + state.pending_payout(&palw_reporter_payout_key_v1(key)).map(|r| r.amount).unwrap_or(0)
}

/// Every conviction recorded about `claim` (any kind), with its accused outpoint and what it collected.
fn offences_of(lg: &Lg, claim: Hash64, seats: &[usize]) -> Vec<(Hash64, TransactionOutpoint, u64)> {
    let state = lg.chain.tip_state().1;
    let mut out: Vec<(Hash64, TransactionOutpoint, u64)> =
        state.consumed_offences_iter().filter(|(_, o)| o.claim_id == claim).map(|(k, o)| (*k, o.accused, o.collected)).collect();
    for card in seats {
        let key = palw_false_valid_offence_id_v2(&lg.bond(*card).0, &claim);
        if let Some(o) = state.consumed_offence(&key)
            && !out.iter().any(|(k, ..)| *k == key)
        {
            out.push((key, o.accused, o.collected));
        }
    }
    out
}

// ---- D ---------------------------------------------------------------------------------------------------------------------------

/// **D: a false accusation of an honest claim is refused.** A bond outside the Panel reserves an honest claim, reads its binding,
/// demands one leaf's committed half by hand (the engine, whose replay agrees, would file nothing) and then accuses that leaf in the
/// one-move court with the registered model's rows. The court recomputes the producer's committed tile and refuses: no conviction,
/// the producer's collateral untouched; the accuser's release lets the claim reach `Final` at its deadline, its reward vesting.
#[tokio::test]
async fn g14_accept_d_a_false_accusation_of_an_honest_claim_is_refused() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let (produced, _, _) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert!(!v.mismatch(&lg), "the honest root reproduces");
    let object = reserve_by_hand(&lg, id, 0x71);
    lg.send(vec![(NEWCOMER, object)]).await;
    let binding_read = PalwLegacyProbeV1::Binding { row: 0, tile: 0 };
    let object = v.demand(&mut lg, binding_read);
    lg.send(vec![(NEWCOMER, object)]).await;
    for _ in 0..3 {
        if v.learn(&lg, binding_read) {
            break;
        }
        if !producer_answers_d2(&mut lg, &produced).await {
            lg.beat(1).await;
        }
    }
    let binding = v.binding.clone().expect("the binding, read off the producer's authenticated answer");
    let leaf = second_half_leaf(&binding, None);
    let producer_before = lg.collateral(EXECUTOR);
    let object = v.demand(&mut lg, PalwLegacyProbeV1::Terminal { leaf });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(producer_answers_d2(&mut lg, &produced).await, "the honest producer answers the committed half it owes");
    let (accusation, ..) = one_move_accusation(&mut lg, id, leaf, NEWCOMER);
    let outcome = try_send(&mut lg, NEWCOMER, &accusation).await;
    eprintln!("[g14-accept D false] the false accusation: {outcome:?}");
    let view = lg.view(id);
    assert!(!view.court_convicted && !view.executor_refuted, "a false accusation convicts nobody");
    assert!(!matches!(view.phase, PalwClaimPhaseV2::Voided { .. }), "the honest claim is not void: {:?}", view.phase);
    assert_eq!(lg.collateral(EXECUTOR), producer_before, "the honest producer is not charged");
    let object = release(&lg, id, 0x72);
    lg.send(vec![(NEWCOMER, object)]).await;
    let deadline = lg.view(id).deadline_daa.expect("release re-arms the claim's own Final deadline");
    lg.beat_to(deadline + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "the honest claim reaches Final");
    assert_eq!(lg.collateral(EXECUTOR), producer_before);
    assert!(lg.chain.tip_state().1.vesting_row(&id).is_some(), "its reward vests");
}

/// **D: a verifier implementation difference is not producer fraud.** The claim is honest; the verifier's OWN execution is wrong at
/// one leaf (its machine, not the producer's). Its replay disagrees, so the common engine reserves and bisects — against the
/// producer's authenticated ranges — down to the verifier's own faulty leaf, and demands it. The producer discloses the committed
/// half; the one-move court recomputes that tile from the registered model's rows and refuses the accusation. Nobody is convicted,
/// the producer is not charged, and the claim reaches `Final` once the mistaken verifier releases.
#[tokio::test]
async fn g14_accept_d_a_verifier_implementation_difference_is_not_producer_fraud() {
    use misaka_palw_base0::produce::base0_material_decode_v1;
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let (produced, _, _) = licensed_claim(&mut lg, false).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert!(!v.mismatch(&lg), "a correct verifier agrees");
    // The verifier's own implementation differs at one leaf: its "honest" run is the job with that tile moved.
    let view = lg.view(id);
    let (canonical, prompt) = lg.backend.job_for_anchor(view.job_identity).expect("the floor implies the recorded job");
    let job = palw_attempt_job_v1(canonical, true);
    let (binding, tiles, ..) = base0_material_decode_v1(&v.honest.material).expect("the verifier's capture decodes");
    let held: std::collections::BTreeSet<u64> = tiles.iter().map(|(i, _)| *i).collect();
    let own_bug = second_half_leaf(&binding, Some(&held));
    let buggy = lg.backend.execute_with_injected_fault(&job, &prompt, own_bug).expect("the verifier's own (buggy) run");
    v.roots = PalwClaimRootsV1 { execution_root: buggy.execution_root, trace_root: buggy.trace_root, ..v.roots };
    v.honest = buggy;
    assert!(v.mismatch(&lg), "the buggy verifier disagrees with the honest producer");
    let producer_before = lg.collateral(EXECUTOR);
    let located = pursue_to_terminal(&mut v, &mut lg, &produced, 400).await;
    assert_eq!(located, own_bug, "the localizer lands on the verifier's own divergence");
    let object = v.demand(&mut lg, PalwLegacyProbeV1::Terminal { leaf: located });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(producer_answers_d2(&mut lg, &produced).await, "the honest producer answers the committed half it owes");
    let (accusation, ..) = one_move_accusation(&mut lg, id, located, NEWCOMER);
    let outcome = try_send(&mut lg, NEWCOMER, &accusation).await;
    eprintln!("[g14-accept D impl] the mistaken accusation: {outcome:?}");
    let view = lg.view(id);
    assert!(!view.court_convicted && !view.executor_refuted, "an implementation difference is never producer fraud");
    assert!(!matches!(view.phase, PalwClaimPhaseV2::Voided { .. }), "the honest claim is not void: {:?}", view.phase);
    assert_eq!(lg.collateral(EXECUTOR), producer_before, "the honest producer is not charged");
    let object = release(&lg, id, 0x73);
    lg.send(vec![(NEWCOMER, object)]).await;
    let deadline = lg.view(id).deadline_daa.expect("release re-arms the claim's own Final deadline");
    lg.beat_to(deadline + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "the honest claim reaches Final");
    assert_eq!(lg.collateral(EXECUTOR), producer_before);
}

// ---- E ---------------------------------------------------------------------------------------------------------------------------

/// **E: collateral is actually reduced, the reporter is paid only from what was collected, nothing is collected twice, and the
/// invalidated reward, weight and root cannot be used.**
#[tokio::test]
async fn g14_accept_e_collected_once_reporter_from_collected_and_rights_void() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let c = convict(&mut lg, FirstLook::BeforeBind).await;
    let id = c.produced.claim_id;
    let claim = lg.chain.tip_state().1.claim(&id).expect("the claim row stays").clone();
    let offences = offences_of(&lg, id, &c.seats);
    eprintln!("[g14-accept E] offences {offences:?}");
    assert!(!offences.is_empty(), "convictions are recorded");
    // Collateral: each accused bond fell by exactly the sum its offences collected (one funnel, no phantom debit).
    for (card, before) in &c.before {
        let outpoint = lg.bond(*card).0;
        let collected: u64 = offences.iter().filter(|(_, accused, _)| *accused == outpoint).map(|(.., c)| *c).sum();
        let drop = before - lg.collateral(*card);
        eprintln!("[g14-accept E] card {card}: collateral fell {drop}, its offences collected {collected}");
        if *card == EXECUTOR {
            assert!(collected > 0 && drop > 0, "the producer's collateral is actually reduced");
        }
        assert_eq!(drop, collected, "card {card}: the debit is exactly what its offences record as collected");
    }
    let total_collected: u64 = offences.iter().map(|(.., c)| *c).sum();
    // The reporter side, at every stage the award passes through, never exceeds what its offence collected.
    for step in 0..6u64 {
        let state = lg.chain.tip_state().1;
        let mut total = 0u64;
        for (key, _, collected) in &offences {
            let credit = reporter_credit(&state, key);
            assert!(credit <= *collected, "step {step}: offence {key}'s reporter credit {credit} exceeds its collected {collected}");
            total += credit;
        }
        eprintln!("[g14-accept E] step {step} (DAA {}): reporter credit {total} of {total_collected} collected", lg.daa());
        assert!(total <= total_collected, "step {step}: reporter credit {total} > collected {total_collected}");
        lg.beat(1 + step * 5).await;
    }
    // No double collection: the same accusation and the same kind-3 filings again.
    let collaterals: Vec<u64> = c.before.keys().map(|card| lg.collateral(*card)).collect();
    let n_offences = lg.chain.tip_state().1.consumed_offences_iter().count();
    let again = try_send(&mut lg, NEWCOMER, &c.accusation).await;
    eprintln!("[g14-accept E] the accusation again: {again:?}");
    for (_, filing) in &c.filings {
        let again = try_send(&mut lg, NEWCOMER, filing).await;
        eprintln!("[g14-accept E] a kind-3 filing again: {again:?}");
    }
    assert_eq!(c.before.keys().map(|card| lg.collateral(*card)).collect::<Vec<_>>(), collaterals, "nothing collected twice");
    assert_eq!(lg.chain.tip_state().1.consumed_offences_iter().count(), n_offences, "no second offence row");
    // The invalidated rights, long past the claim's would-be Final floor: no vesting row, no payout, the root forfeited.
    let accepted = lg.view(id).accepted_daa;
    lg.beat_to(accepted + 2 * lg.bundle.state.window_challenge_at(accepted) + 10).await;
    let state = lg.chain.tip_state().1;
    assert!(matches!(state.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { .. })), "void stays void");
    assert!(state.vesting_row(&id).is_none(), "no vesting row: the reward never vests");
    assert!(state.pending_payout(&palw_vesting_payout_key_v1(&id)).is_none(), "no payout queued for the void claim");
    assert!(state.palw_execution_root_is_forfeited_v1(&claim.execution_root), "the proven-false root is forfeited");
    assert!(state.safe_weight() >= c.safe_before, "safe weight only grows from other work");
    eprintln!(
        "[g14-accept E] producer frozen after the conviction: {}",
        kaspa_consensus_core::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&state, &lg.bond(EXECUTOR))
    );
    // A node replaying the chain from genesis reaches the same roots, block by block.
    let z = t12_genesis_chain_on(TestConsensus::new(&lg.config), &lg.config, &lg.bundle, &lg.premine, &lg.floats);
    for block in chain_blocks(&lg.chain, lg.chain.sink()) {
        let hash = block.header.hash;
        arrive(&z, block, "the first node's block").await;
        assert_eq!(root_at(&z, hash), root_at(&lg.chain, hash), "the replaying node's root at {hash}");
    }
}

// ---- F ---------------------------------------------------------------------------------------------------------------------------

/// **F: the outsider first acts at `Final − 1`** — its reservation lands in the last blocks before the floor, holds `Final`, and the
/// pursuit ends in the same objective conviction as the headline case.
#[tokio::test]
async fn g14_accept_f_the_same_conviction_from_final_minus_one() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let c = convict(&mut lg, FirstLook::FinalMinusOne).await;
    let state = lg.chain.tip_state().1;
    assert!(matches!(state.claim(&c.produced.claim_id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Voided { .. })));
    assert!(state.vesting_row(&c.produced.claim_id).is_none());
}

/// **F: the outsider first acts after `Final`, inside the liability** — the post-Final reservation, the same localization and the
/// same one-move conviction; the Final is reversed, nothing of its reward is paid, the producer and the liable seats slashed.
#[tokio::test]
async fn g14_accept_f_the_same_conviction_after_final_inside_the_liability() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let c = convict(&mut lg, FirstLook::AfterFinal).await;
    let id = c.produced.claim_id;
    let state = lg.chain.tip_state().1;
    eprintln!("[g14-accept F after-final] phase {:?}", state.claim(&id).map(|c| &c.phase));
    assert!(!matches!(state.claim(&id).map(|c| &c.phase), Some(PalwClaimPhaseV2::Final { .. })), "the Final is reversed");
    assert!(state.vesting_row(&id).is_none_or(|r| r.matured_at.is_none()), "nothing of its reward matured");
    assert!(state.pending_payout(&palw_vesting_payout_key_v1(&id)).is_none(), "no payout from the reversed Final");
}

/// **F: an exit raced against the pursuit escapes nothing.** As soon as the outsider reserves and the claim is licensed, a liable
/// seat and the producer each ask to retire their bonds. The seat's request does not start its exit while its `Valid` lock is alive;
/// the producer's releases no collateral while its claim is pursued; the conviction still slashes the producer.
#[tokio::test]
async fn g14_accept_f_a_bond_exit_raced_against_the_pursuit_escapes_nothing() {
    use kaspa_consensus_core::palw_state_v2::PalwBondStatusV2;
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    lg.beat(3).await;
    lg.register_newcomer().await;
    let produced = lg.real_attempt(EXECUTOR, true).await;
    let id = produced.claim_id;
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert_eq!(v.step(&mut lg, &produced, &|_| false).await, None, "the engine reserves first");
    let seats = lg.bind(id).await;
    lg.license(id, &seats).await;
    fn retire(lg: &mut Lg, card: usize) -> Obj {
        let bond = lg.bond(card);
        let message = palw_bond_retirement_message_v2(lg.domain, &bond);
        let signature = lg.signed(card, message.as_byte_slice(), PALW_BOND_RETIREMENT_V2_MLDSA87_CONTEXT);
        Obj::BondRetireRequested { bond, signature }
    }
    let retiring = |lg: &Lg, card: usize| {
        matches!(lg.chain.tip_state().1.bond(&lg.bond(card)).map(|b| b.status.clone()), Some(PalwBondStatusV2::Retiring { .. }))
    };
    let producer_before = lg.collateral(EXECUTOR);
    let seat = seats[0];
    let seat_retire = retire(&mut lg, seat);
    let seat_outcome = try_send(&mut lg, seat, &seat_retire).await;
    eprintln!("[g14-accept F exit] the locked seat's exit: {seat_outcome:?}, retiring {}", retiring(&lg, seat));
    assert!(seat_outcome.is_err() || !retiring(&lg, seat), "a seat with a live Valid lock cannot start its exit");
    let producer_retire = retire(&mut lg, EXECUTOR);
    let producer_outcome = try_send(&mut lg, EXECUTOR, &producer_retire).await;
    eprintln!("[g14-accept F exit] the producer's exit: {producer_outcome:?}, retiring {}", retiring(&lg, EXECUTOR));
    assert_eq!(lg.collateral(EXECUTOR), producer_before, "an exit request releases no collateral while the claim is pursued");
    let lie = pursue_to_terminal(&mut v, &mut lg, &produced, 400).await;
    let object = v.demand(&mut lg, PalwLegacyProbeV1::Terminal { leaf: lie });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(producer_answers_d2(&mut lg, &produced).await);
    let (accusation, ..) = one_move_accusation(&mut lg, id, lie, NEWCOMER);
    lg.send(vec![(NEWCOMER, accusation)]).await;
    assert!(lg.view(id).court_convicted, "the raced exit did not stop the conviction");
    assert!(lg.collateral(EXECUTOR) < producer_before, "the producer is slashed despite its exit request");
}

/// **F: the pursuit window and the payout never overlap — the boundary stated as a test.** A lying claim nobody pursued reaches
/// `Final`. Up to its vesting row's DAA expiry (`F + window_court`) the node still serves its dispute view and admits a reservation,
/// and nothing of its reward is queued. Past the expiry the claim row retires: no dispute view, no reservation can be recorded, the
/// producer's collateral is untouched — and the vesting row matures only then or later (it also waits on the settlement clock, V-4),
/// so the reward is paid strictly after the last moment a pursuit could open. A paid reward is settled; an unpaid one inside the
/// window is pursuable (the parent's after-Final conviction and `g14_accept_f_the_same_conviction_after_final_inside_the_liability`).
#[tokio::test]
async fn g14_accept_f_the_pursuit_window_and_the_payout_never_overlap() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg();
    let (produced, _, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "nobody looked: Final");
    let expiry = lg.chain.tip_state().1.vesting_row(&id).expect("its vesting row").expiry_daa;
    // Inside the window: pursuable, nothing paid.
    lg.beat_to(expiry.saturating_sub(6)).await;
    let state = lg.chain.tip_state().1;
    assert!(state.vesting_row(&id).is_some_and(|r| r.matured_at.is_none()), "unmatured inside the window");
    assert!(state.pending_payout(&palw_vesting_payout_key_v1(&id)).is_none(), "nothing paid inside the window");
    let view = lg.view(id);
    let check = lg.chain.ctx.consensus.palw_legacy_dispute_reservation_check_v1(palw_fraud_filer_reservation_v1(&view, lg.bond(NEWCOMER)));
    assert!(check.as_ref().is_some_and(|r| r.is_ok()), "just before the expiry the node still admits a reservation: {check:?}");
    // Past the window: retired, not pursuable, untouched; the payout (if any yet) comes only now.
    lg.beat_to(expiry + 2).await;
    let producer_before = lg.collateral(EXECUTOR);
    let state = lg.chain.tip_state().1;
    let row = state.vesting_row(&id).cloned();
    eprintln!(
        "[g14-accept F horizon] past the DAA expiry {expiry} (now {}): claim row {:?}, vesting (final, expiry, settled, matured) {:?}",
        lg.daa(),
        state.claim(&id).map(|c| c.phase.clone()),
        row.as_ref().map(|r| (r.final_daa, r.expiry_daa, r.settled_at_final, r.matured_at))
    );
    assert!(lg.chain.ctx.consensus.palw_legacy_dispute_v1(id).is_none(), "past the window the claim's dispute view is retired");
    assert!(row.as_ref().is_none_or(|r| r.matured_at.is_none_or(|at| at >= expiry)), "the reward matures only past the window");
    // The same reservation the node admitted inside the window (its roots from the view read then), sent now.
    let reservation = PalwDisputeReservationV1 {
        version: PALW_DISPUTE_RESERVATION_VERSION_V1,
        claim: id,
        execution_root: view.execution_root,
        trace_root: view.trace_root,
        reserver: lg.bond(NEWCOMER),
    };
    let object = palw_dispute_reserved_object_v1(lg.domain, reservation, |m| {
        sign(NEWCOMER, m, kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_LEGACY_DISPUTE_MLDSA87_CONTEXT_V1, 0x74)
    });
    let outcome = try_send(&mut lg, NEWCOMER, &object).await;
    eprintln!("[g14-accept F horizon] a reservation carrier past the window: {outcome:?}");
    assert!(lg.chain.tip_state().1.legacy_dispute_v1(&id).is_none(), "no reservation is recorded past the window");
    assert_eq!(lg.collateral(EXECUTOR), producer_before, "the settled claim's producer is untouched");
}

/// **F, closed under the full G14 set (G-15 `palw_reward_liability_horizon_v1` armed): every unpaid reward stays prosecutable.** The
/// same lying claim nobody pursued, past its vesting row's DAA expiry with the settlement clock not yet met: the claim is NOT retired,
/// its dispute view is still served, the node admits a reservation, and the outsider's pursuit convicts it — the reward never pays.
#[tokio::test]
async fn g14_accept_f_under_g15_an_unpaid_reward_past_its_expiry_is_still_prosecuted() {
    kaspa_core::log::try_init_logger("warn");
    let mut lg = g14_lg_on(true);
    let (produced, _, licensed) = licensed_claim(&mut lg, true).await;
    let id = produced.claim_id;
    lg.beat_to(licensed + lg.bundle.state.window_challenge_at(licensed) + 2).await;
    assert!(matches!(lg.view(id).phase, PalwClaimPhaseV2::Final { .. }), "nobody looked: Final");
    let expiry = lg.chain.tip_state().1.vesting_row(&id).expect("its vesting row").expiry_daa;
    lg.beat_to(expiry + 25).await;
    let state = lg.chain.tip_state().1;
    let row = state.vesting_row(&id).cloned();
    eprintln!(
        "[g14-accept F G-15] past the DAA expiry {expiry} (now {}): claim row {:?}, vesting matured {:?}",
        lg.daa(),
        state.claim(&id).map(|c| c.phase.clone()),
        row.as_ref().map(|r| r.matured_at)
    );
    assert!(row.as_ref().is_some_and(|r| r.matured_at.is_none()), "unmatured: the settlement clock is not met in this harness");
    assert!(state.pending_payout(&palw_vesting_payout_key_v1(&id)).is_none(), "unpaid");
    assert!(lg.chain.ctx.consensus.palw_legacy_dispute_v1(id).is_some(), "G-15: the unpaid claim is not retired");
    let mut v = Verifier::check(&lg, NEWCOMER, id);
    assert_eq!(v.step(&mut lg, &produced, &|_| false).await, None, "the engine reserves past the DAA expiry");
    assert!(lg.view(id).record.is_some(), "the reservation landed");
    let producer_before = lg.collateral(EXECUTOR);
    let lie = pursue_to_terminal(&mut v, &mut lg, &produced, 400).await;
    let object = v.demand(&mut lg, PalwLegacyProbeV1::Terminal { leaf: lie });
    lg.send(vec![(NEWCOMER, object)]).await;
    assert!(producer_answers_d2(&mut lg, &produced).await);
    let (accusation, ..) = one_move_accusation(&mut lg, id, lie, NEWCOMER);
    lg.send(vec![(NEWCOMER, accusation)]).await;
    assert!(lg.view(id).court_convicted, "convicted past the DAA expiry, before the reward matured");
    assert!(lg.collateral(EXECUTOR) < producer_before, "the producer is slashed");
    assert!(lg.chain.tip_state().1.pending_payout(&palw_vesting_payout_key_v1(&id)).is_none(), "the reward never pays");
}

// ---- G ---------------------------------------------------------------------------------------------------------------------------

/// The isolated component network with LG14-A, LG14-B and option (b) active at `at`.
/// Normal validation accepts the marked test genesis and refuses the unmarked control.
fn g14_cross_lg(at: u64) -> Lg {
    let (config, _, mut premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    let newcomer_funding = {
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
    let fence = Some(ForkActivation::new(at));
    params.palw_legacy_public_filer_v1 = fence;
    params.sync_palw_legacy_public_filer_v1();
    params.palw_legacy_held_da_v2 = fence;
    params.palw_g14_route_required_v1 = fence;
    params.sync_palw_g14_gates_v1();
    let unisolated = params.validate_palw_v2().expect_err("unmarked genesis still refuses production arming");
    assert!(format!("{unisolated:?}").contains("no arm-readiness record"), "{unisolated:?}");
    let incomplete = params.validate_palw_court_policy_completeness_v1(
        kaspa_consensus_core::palw_g14_gates_v1::PalwG14GatedFenceV1::LegacyHeldDaV2,
    ).expect_err("missing F02/F03 policy independently refuses an unmarked genesis");
    assert!(format!("{incomplete:?}").contains("OneOutsiderCapacityIncomplete"), "{incomplete:?}");
    params.genesis = palw_isolated_g14_genesis_v1(&params.genesis, Hash64::from_bytes([0x15; 64]))
        .expect("bounded isolated fence-boundary test genesis");
    assert!(params.has_isolated_g14_genesis_v1());
    params.validate_palw_v2().expect("normal isolated G14 validation at the boundary fence");
    params.skip_proof_of_work = true;
    let config = Config::new(params);
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is ConsensusV2") };
    let bundle = bundle.clone();
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

/// **G: honest claims accepted just before, exactly at and just after the fence settle normally.** On a network whose G14 set is
/// armed at `H`, three honest attempts are accepted back to back at virtual DAA `H − 1`, `H` and `H + 1`; each is bound, licensed by
/// its (honest) seats, reaches `Final` at its floor, gets its vesting row with a positive reward and stands in the selected chain
/// (the block opportunity). Option (b) admits all three, and the outsider's own replay finds nothing to pursue in any of them.
#[tokio::test]
async fn g14_accept_g_honest_claims_before_at_and_after_the_fence_settle() {
    kaspa_core::log::try_init_logger("warn");
    const H: u64 = 40;
    let mut lg = g14_cross_lg(H);
    lg.beat(3).await;
    lg.register_newcomer().await;
    let mut produced = Vec::new();
    for want in [H - 1, H, H + 1] {
        while lg.chain.ctx.consensus.get_virtual_daa_score() < want {
            lg.beat(1).await;
        }
        let p = lg.real_attempt(EXECUTOR, false).await;
        let accepted = lg.view(p.claim_id).accepted_daa;
        eprintln!("[g14-accept G] honest claim {} accepted at DAA {accepted} (wanted {want})", p.claim_id);
        produced.push((want, accepted, p));
    }
    let accepted: Vec<u64> = produced.iter().map(|(_, a, _)| *a).collect();
    assert!(accepted[0] < H && accepted[1] == H && accepted[2] > H, "claims straddle the fence exactly: {accepted:?} around {H}");
    for (_, accepted, p) in &produced {
        assert_eq!(
            palw_g14_route_attempt_refusal_v1(&lg.bundle.state, *accepted),
            None,
            "DAA {accepted}: the V2 Attempt family is admitted (no route requirement below H, routed at and after)"
        );
        let v = Verifier::check(&lg, NEWCOMER, p.claim_id);
        assert!(!v.mismatch(&lg), "DAA {accepted}: the outsider's own replay agrees — nothing to pursue");
    }
    let mut floors = Vec::new();
    for (_, _, p) in &produced {
        let seats = lg.bind(p.claim_id).await;
        let licensed = lg.license(p.claim_id, &seats).await;
        floors.push(licensed + lg.bundle.state.window_challenge_at(licensed));
    }
    lg.beat_to(floors.iter().copied().max().expect("three floors") + 2).await;
    let selected = chain_blocks(&lg.chain, lg.chain.sink());
    let state = lg.chain.tip_state().1;
    for (want, accepted, p) in &produced {
        let row = state.claim(&p.claim_id).expect("the claim row");
        assert!(matches!(row.phase, PalwClaimPhaseV2::Final { .. }), "DAA {accepted} ({want}): Final, not {:?}", row.phase);
        let vesting = state.vesting_row(&p.claim_id).unwrap_or_else(|| panic!("DAA {accepted}: its reward vests"));
        assert!(vesting.producer.amount > 0 || vesting.escrowed_reward > 0, "DAA {accepted}: a positive reward");
        assert!(
            selected.iter().any(|b| b.header.daa_score == *accepted && !b.header.palw_commitment.is_empty()),
            "DAA {accepted}: the attempt's block stands in the selected chain"
        );
        eprintln!(
            "[g14-accept G] DAA {accepted}: Final, escrowed {} sompi, producer leg {} sompi",
            vesting.escrowed_reward, vesting.producer.amount
        );
    }
}
