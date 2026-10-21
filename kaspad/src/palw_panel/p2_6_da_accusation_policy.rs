//! **`palw_panel`'s `p2_6_da_accusation_policy` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

//! **ADR-0152 §3.8 (Phase 2, P2-6): the automatic data-availability accusation, its funding and
//! its place in the carrier order.** The accusation's gate and its object are consensus-core's
//! (`palw_da_accusation_check_v1`, `palw_da_accusation_object_v1`) and ride the gate, the walk and
//! the fold on real claims in `t46_false_valid_real_claim::m3_da_court` (T34/T54a at processor
//! level); this is the node's own policy around them — when a seat accuses and until when, the
//! book that files each claim once, the fee funder's age and B-3 rules, and the lane order — and
//! the tick's wiring, pinned where it runs.
use super::*;
use kaspa_consensus_core::palw_producer_v2::PalwDaAccusationCheckV1 as C;
use kaspa_consensus_core::palw_state_v2::{PalwDaAdmissionV1, PalwStateV2Error};

fn claim(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

fn the_source() -> &'static str {
    let whole = include_str!("../palw_panel.rs");
    &whole[..whole.find("mod p2_6_da_accusation_policy").expect("this module is in this file")]
}

/// The body of `fn name` in the production source.
fn body_of(source: &'static str, signature: &str) -> &'static str {
    let body = &source[source.find(signature).unwrap_or_else(|| panic!("{signature}"))..];
    &body[..body.find("\n    }\n").expect("its end")]
}

/// **T34 (node half): a seat's `Unavailable` accuses the producer, inside the landing margin, on
/// every class.** Past `palw_rcore_plus` the window is `deadline − 60` (DA-6's "until `bound +
/// window_receipt − 60`"): testnet-12's floor, bound at 1,000 with its 600-DAA window, accuses
/// from its `X_ASK` (1,060) to 1,540 and not after; a 2M claim's window — the chain's `W_r(c)`,
/// 13,995 DAA past §4-quater, which is the deadline the loop now reads for it — reaches 14,935.
/// Nothing below the fence, nothing for a claim this seat's own replay refuted (it was served —
/// the challenger's half), and nothing on a window shorter than the margin (the licence's own
/// accusation covers it).
#[test]
fn t34_an_unavailable_accuses_inside_its_landing_margin_on_every_class() {
    let floor_deadline = 1_000 + 600;
    assert_eq!(palw_seat_da_accuse_by_v1(true, false, 1_000 + PALW_SEAT_MATERIAL_WAIT_CAP_DAA_V1, floor_deadline), Some(1_540));
    assert_eq!(palw_seat_da_accuse_by_v1(true, false, 1_540, floor_deadline), Some(1_540), "to the margin");
    assert_eq!(palw_seat_da_accuse_by_v1(true, false, 1_541, floor_deadline), None, "not past it");
    assert_eq!(palw_seat_da_accuse_by_v1(true, false, 1_060, 1_000 + 13_995), Some(14_935), "the 2M row's window");
    assert_eq!(palw_seat_da_accuse_by_v1(false, false, 1_060, floor_deadline), None, "below palw_rcore_plus");
    assert_eq!(palw_seat_da_accuse_by_v1(true, true, 1_060, floor_deadline), None, "a claim its replay refuted");
    assert_eq!(palw_seat_da_accuse_by_v1(true, false, 10, 40), None, "a window shorter than the margin");
}

fn file() -> C {
    file_as(true)
}

fn file_as(accuser_is_seat: bool) -> C {
    C::File {
        unit: kaspa_consensus_core::palw_da_rcore_v1::PALW_DA_AUTO_NAMED_UNIT_V1,
        admission: PalwDaAdmissionV1 {
            stage: kaspa_consensus_core::palw_da_rcore_v1::PalwDaStageV1::Live,
            accuser_is_seat,
            exposure: 320,
            deadline_daa: 2_300,
        },
    }
}

/// **The chain's answer, as a step**: `File` files; an accusation on chain (open, or opened by
/// this seat before) and an answered unit settle the claim for good; A-6's room is waited out a
/// re-plan at a time; every other refusal of the fold's gate settles it.
#[test]
fn the_chains_answer_files_settles_or_waits_for_room() {
    let bond = PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(9), 0));
    assert_eq!(palw_seat_accuse_step_v1(&file()), PalwSeatAccuseStepV1::File);
    // The P2-6 review, LOW: a bond no longer on the claim's current panel would open a NON-seat
    // session (no pause credit, the non-seat budget) — P2-8d's filing, never this one.
    assert_eq!(palw_seat_accuse_step_v1(&file_as(false)), PalwSeatAccuseStepV1::Settle, "not a seat's accusation");
    assert_eq!(palw_seat_accuse_step_v1(&C::AccusedBefore), PalwSeatAccuseStepV1::Settle);
    assert_eq!(palw_seat_accuse_step_v1(&C::Answered), PalwSeatAccuseStepV1::Settle);
    let room = PalwStateV2Error::AccusationExposureCeiling {
        bond,
        edge: "data-availability session",
        backed: 900,
        accusation: 320,
        ceiling: 1_000,
    };
    assert_eq!(palw_seat_accuse_step_v1(&C::Refused(room)), PalwSeatAccuseStepV1::Retry, "A-6's room frees as sessions close");
    for standing in [
        PalwStateV2Error::DaCourtDormant,
        PalwStateV2Error::MissingClaim(claim(1)),
        PalwStateV2Error::WrongPhase { claim: claim(1), edge: "DefaultAccused" },
        PalwStateV2Error::DaClaimNotAccusable(claim(1)),
        PalwStateV2Error::DaOutsideRetention { claim: claim(1), at: 5, window: 1_200, retention_daa: 6 },
        PalwStateV2Error::DaSessionBudgetExhausted { claim: claim(1), accuser: bond, why: "four" },
        PalwStateV2Error::DaAccuserIsTheProducer(bond),
    ] {
        assert_eq!(palw_seat_accuse_step_v1(&C::Refused(standing.clone())), PalwSeatAccuseStepV1::Settle, "{standing:?}");
    }
}

/// **The de-duplication: each accuser accuses a claim once, and no storm of duplicates rides.**
/// The `Unavailable` and the licence note the same claim once (the licence may only widen its
/// window). While its accusation is queued, or its carrier sent less than a re-plan ago, the
/// claim is not asked again; a carrier lost past the re-plan is asked again — the chain decides,
/// and from the moment the session exists its answer is `AccusedBefore`, which settles the claim;
/// a later trigger then costs one read and files nothing. A room refusal is asked again a re-plan
/// later; a window that runs out is forgotten. Driven through a scripted chain over thirty ticks
/// of triggers: exactly one accusation is built.
#[test]
fn the_book_files_each_claim_once_and_the_chain_ends_it() {
    let (c1, c2) = (claim(1), claim(2));
    let mut book = PalwSeatAccusationsV1::default();
    book.want(c1, 1_540);
    book.want(c1, 1_100);
    book.want(c1, 1_600);
    assert!(book.wants(&c1));
    let nothing = |_: &Hash64| false;
    let never = |_: &Hash64| None;
    assert_eq!(book.due(1_060, nothing, never), vec![c1], "one entry, however many triggers");
    assert!(book.due(1_060, |c| *c == c1, never).is_empty(), "not while queued");
    assert!(book.due(1_069, nothing, |_| Some(1_060)).is_empty(), "not while its carrier is plausibly in flight");
    assert_eq!(book.due(1_070, nothing, |_| Some(1_060)), vec![c1], "a lost carrier is asked for again after a re-plan");
    book.refused(&c1, 1_070);
    assert!(book.due(1_079, nothing, never).is_empty(), "room: asked a re-plan later");
    assert_eq!(book.due(1_080, nothing, never), vec![c1]);
    assert!(book.due(1_601, nothing, never).is_empty() && !book.wants(&c1), "the widened window runs out");
    book.want(c2, 2_000);
    book.settle(&c2);
    assert!(book.due(1_100, nothing, never).is_empty(), "settled");

    // Thirty ticks of the tick's own sequence on a scripted chain: triggers every tick (the
    // `Unavailable`, then the licence), one queue, a carrier that lands two ticks after it is sent.
    let mut book = PalwSeatAccusationsV1::default();
    let key = palw_da_accusation_queue_key_v1(c1);
    let (mut queued, mut sent_at, mut on_chain_at, mut built) = (false, None::<u64>, None::<u64>, 0);
    for tick in 0..30u64 {
        let daa = 1_060 + tick;
        book.want(c1, daa + PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1);
        if on_chain_at.is_none_or(|at| daa < at) && sent_at.is_some_and(|at| daa >= at + 2) {
            on_chain_at = Some(daa);
        }
        let chain = |daa: u64| if on_chain_at.is_some_and(|at| daa >= at) { C::AccusedBefore } else { file() };
        for c in book.due(daa, |c| *c == c1 && queued, |c| (*c == c1).then_some(sent_at).flatten()) {
            match palw_seat_accuse_step_v1(&chain(daa)) {
                PalwSeatAccuseStepV1::File => {
                    built += 1;
                    queued = true;
                }
                PalwSeatAccuseStepV1::Settle => book.settle(&c),
                PalwSeatAccuseStepV1::Retry => book.refused(&c, daa),
            }
        }
        // The priority lane sends what is queued, keyed as the court queue keys it.
        if queued {
            assert_eq!(key, (c1, u32::MAX, false));
            queued = false;
            sent_at = Some(daa);
        }
    }
    assert_eq!(built, 1, "one accusation for thirty ticks of triggers");
    assert!(on_chain_at.is_some(), "and the chain holds it");
}

/// **The queue key is the claim on the accusing side, and never an answer's or a court move's**:
/// a court move is keyed by its session id and an R-core answer is the responder's (`true`), so
/// the entry the priority lane carries is recognised as this seat's accusation and nothing else is.
#[test]
fn the_accusation_queue_key_is_the_claim_on_the_accusing_side() {
    let key = palw_da_accusation_queue_key_v1(claim(5));
    assert_eq!(key, (claim(5), u32::MAX, false));
    let accused = PalwConsensusObjectV2::DefaultAccused {
        claim: claim(5),
        missing_event_index: 0,
        accuser: PalwBondKeyV2(TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(9), 0)),
        signature: vec![1],
    };
    assert!(palw_da_accusation_queued_v1(key.1, key.2, &accused));
    assert!(!palw_da_accusation_queued_v1(0, false, &accused), "another round is not this seat's accusation");
    assert!(!palw_da_accusation_queued_v1(key.1, true, &accused), "nor the responder's side");
}

/// **The tick wires it where it runs**: the `Unavailable` notes the claim after its receipt is
/// broadcast; the accusation step asks the chain before it builds, builds with the ONE builder,
/// queues under the accusation's key, and runs before the collector and submitter; queued
/// accusations are asked again while they wait; and the licence seam notes the claim instead of
/// saying the filing is Phase 2's.
#[test]
fn the_tick_accuses_after_the_unavailable_and_before_the_carriers() {
    let source = the_source();
    let receipt = source
        .find("self.flow_context.broadcast_palw_seat_receipt(bytes).await;\n                }\n                if valid {")
        .expect("the receipt");
    let noted = source.find("accusations.want(duty.claim_id, by);").expect("the Unavailable notes the claim");
    let step = source.find("// --- P2-6: this seat's accusations of withholding ---").expect("the accusation step");
    let asked = step
        + source[step..]
            .find("let Some(check) = session.palw_da_accusation_check_v1(claim, bond_key) else")
            .expect("asked of the chain");
    let rechecked = step
        + source[step..]
            .find("court_outbox.retain_pending(|(claim, round, responder, object)| {")
            .expect("the queued ones asked again");
    let built = step + source[step..].find("palw_da_rcore_v1::palw_da_accusation_object_v1(").expect("the ONE builder");
    let queued = built + source[built..].find("let key = palw_da_accusation_queue_key_v1(claim);").expect("keyed");
    let carriers = source.find("// --- the collector + submitter's half ---").expect("the carriers");
    assert!(receipt < noted && noted < step, "noted once the receipt is out, before the step");
    assert!(step < rechecked && rechecked < asked && asked < built && built < queued && queued < carriers);
    let pruned = queued
        + source[queued..]
            .find(
                "court_outbox.retain_submitted(|key, _| *key != palw_da_accusation_queue_key_v1(key.0) || accusations.wants(&key.0));",
            )
            .expect("the debounce leaves with the book entry");
    assert!(pruned < carriers);
    let licence = source.find("if palw_seat_unserved_licence_v1(phase) {").expect("the licence seam");
    assert!(source[licence..].contains("accusations.want(claim, current_daa.saturating_add(PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1));"));
    assert!(!source.contains("the DefaultAccused filing that answers it is Phase 2's"), "the seam files now");
}

/// **The fee funder takes a coinbase output the mempool takes, and nothing B-3 holds** (T52's
/// funder half, F13): on testnet-12 (`coinbase_spend_maturity` 600) a coinbase 600 DAA old funds a
/// carrier and one 599 DAA old does not, nor one from a block ahead of the point; an ordinary
/// output funds at any age; this node's own bond output and any output in B-3's locked set never do.
#[test]
fn t52_the_fee_funder_takes_a_600_daa_old_coinbase_and_never_a_held_output() {
    let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let maturity = params.coinbase_spend_maturity();
    assert_eq!(maturity, 600, "testnet-12's coinbase spend maturity");
    let outpoint = |n: u64| TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(n), 0);
    let entry = |block_daa_score: u64, is_coinbase: bool| UtxoEntry {
        amount: 1_000,
        script_public_key: kaspa_consensus_core::tx::ScriptPublicKey::default(),
        block_daa_score,
        is_coinbase,
    };
    let none = HashSet::new();
    let usable = |n: u64, e: &UtxoEntry, pov: u64, own: Option<TransactionOutpoint>, locked: &HashSet<TransactionOutpoint>| {
        palw_fee_funding_usable_v1(&outpoint(n), e, pov, maturity, own, locked)
    };
    assert!(usable(1, &entry(1_000, true), 1_600, None, &none), "600 DAA old");
    assert!(!usable(1, &entry(1_001, true), 1_600, None, &none), "599 DAA old");
    assert!(!usable(1, &entry(1_700, true), 1_600, None, &none), "ahead of the point");
    assert!(usable(1, &entry(1_599, false), 1_600, None, &none), "an ordinary output at any age");
    assert!(!usable(1, &entry(0, false), 1_600, Some(outpoint(1)), &none), "never this node's bond");
    let locked: HashSet<TransactionOutpoint> = [outpoint(2)].into_iter().collect();
    assert!(!usable(2, &entry(0, false), 1_600, None, &locked), "never an output B-3 holds");
    assert!(!usable(2, &entry(0, true), 1_600, None, &locked), "a mature coinbase B-3 holds neither");
    assert!(usable(3, &entry(0, true), 1_600, Some(outpoint(1)), &locked), "a mature coinbase nobody holds");
}

/// **The funder asks it of every candidate**: the remembered outpoints and the recovery scan both
/// go through `palw_fee_funding_usable_v1` at the virtual DAA with the chain's locked set, and the
/// scan no longer skips every coinbase; a remembered output that cannot pay a carrier is skipped,
/// and the scan keeps the largest that pays (`PalwFeeFundingScanV1::offer`).
#[test]
fn the_funder_asks_the_one_rule_of_every_candidate() {
    let funder = body_of(the_source(), "    async fn resolve_fee_funding(");
    assert!(funder.contains("let pov_daa = session.get_virtual_daa_score();"));
    assert!(funder.contains("let spend_maturity = self.consensus_config.params.coinbase_spend_maturity();"));
    assert!(funder.contains("session.palw_locked_bond_outpoints_v2()"));
    let rule =
        funder.find("palw_fee_funding_usable_v1(outpoint, entry, pov_daa, spend_maturity, self.bond, &locked)").expect("one rule");
    let remembered =
        funder.find("if !usable(outpoint, &entry) || !palw_fee_funding_pays_v1(entry.amount) {").expect("the remembered outpoints");
    let scan = funder.find("scan.offer(outpoint, entry, usable, is_free);").expect("the scan");
    assert!(rule < remembered && remembered < scan);
    assert!(!funder.contains("entry.is_coinbase"), "no blanket coinbase skip");
    assert!(!funder.contains("found = Some((outpoint, entry));\n                break;"), "not the first output the set lists");
}

fn coin(amount: u64, block_daa_score: u64, is_coinbase: bool) -> UtxoEntry {
    UtxoEntry { amount, script_public_key: kaspa_consensus_core::tx::ScriptPublicKey::default(), block_daa_score, is_coinbase }
}

fn outpoint(n: u64) -> TransactionOutpoint {
    TransactionOutpoint::new(kaspa_consensus_core::tx::TransactionId::from_u64_word(n), 0)
}

/// **F13 on the drain path** (the P2-6 review, MEDIUM): a residue at or below one carrier's fee is
/// exhausted — the tick drops a chain whose mined tip holds it, and the funder skips it — and the
/// scan moves to a coinbase the mempool takes (600 DAA old on testnet-12), the largest one, never
/// a 599-DAA-old one however large, never the residue. The floor is twice the heaviest standard
/// carrier's relay fee, so a chain at or above it never fails a carrier for want of funds.
#[test]
fn f13_a_drained_residue_is_left_for_a_mature_coinbase() {
    let maturity = kaspa_consensus_core::config::params::palw_t12_shipped_params().coinbase_spend_maturity();
    let floor = palw_fee_funding_floor_v1();
    assert_eq!(floor, 12_000_000, "2 × the relay fee of a 480,000-mass carrier: 0.12 MSK");
    assert_eq!(floor, 2 * relay_fee_for_compute_mass(MAXIMUM_STANDARD_TRANSACTION_MASS), "pays the heaviest carrier twice");
    // The residue: below one carrier's fee (the builder's `funding UTXO holds N sompi`).
    let residue = (outpoint(1), coin(200_000, 1_500, false));
    assert!(residue.1.amount <= relay_fee_for_compute_mass(0), "it pays no carrier at all");
    assert!(palw_fee_chain_drained_v1(Some(&residue)), "the tick drops the drained chain");
    assert!(!palw_fee_chain_drained_v1(Some(&(outpoint(2), coin(floor, 1_500, false)))), "a chain at the floor carries on");
    assert!(!palw_fee_chain_drained_v1(None));
    // The scan, as `resolve_fee_funding` runs it at DAA 1,600 with nothing held or pending.
    let pov = 1_600;
    let none = HashSet::new();
    let usable = |o: &TransactionOutpoint, e: &UtxoEntry| palw_fee_funding_usable_v1(o, e, pov, maturity, None, &none);
    let free = |_: &TransactionOutpoint| true;
    let mut scan = PalwFeeFundingScanV1::default();
    scan.offer(residue.0, residue.1.clone(), usable, free);
    scan.offer(outpoint(3), coin(50_000_000_000, 1_001, true), usable, free); // 599 DAA old
    scan.offer(outpoint(4), coin(300_000_000, 1_000, true), usable, free); // 600 DAA old
    scan.offer(outpoint(5), coin(400_000_000, 900, true), usable, free); // 700 DAA old, larger
    scan.offer(outpoint(6), coin(100_000_000, 800, true), usable, free);
    let (found, _) = scan.found.clone().expect("the rewards fund the filing");
    assert_eq!(found, outpoint(5), "the largest mature coinbase, never the residue nor a 599-DAA-old one");
    assert_eq!((scan.dust, scan.unripe, scan.under_script, scan.busy), (1, 1, 4, 0));
    // With only the residue and the young coinbase, nothing resolves: the residue is not funding.
    let mut scan = PalwFeeFundingScanV1::default();
    scan.offer(residue.0, residue.1.clone(), usable, free);
    scan.offer(outpoint(3), coin(50_000_000_000, 1_001, true), usable, free);
    assert!(scan.found.is_none(), "a residue is exhausted, and the young coinbase waits its maturity");
    // An output our own mempool spends is not funding either.
    let mut scan = PalwFeeFundingScanV1::default();
    scan.offer(outpoint(4), coin(300_000_000, 1_000, true), usable, |_| false);
    assert!(scan.found.is_none() && scan.busy == 1);
    // And the tick asks it where the chain's tip is mined, then resolves afresh.
    let source = the_source();
    let mined = source.find("if session.get_virtual_utxo_entry(tip).is_some() {").expect("the mined tip");
    let drained = mined + source[mined..].find("if palw_fee_chain_drained_v1(chained_funding.as_ref()) {").expect("the drain rule");
    let resolved = drained
        + source[drained..]
            .find("chained_funding = self.resolve_fee_funding(&session, palw_fee_funding_floor_v1()).await;")
            .expect("resolved afresh");
    assert!(!source[mined..resolved].contains("} else if"), "inside the mined-tip branch");
    assert!(resolved - mined < 1_500);
}

/// Runs one tick's carrier sites through the tick's own scheduler, in the tick's own order: each
/// site that holds something sends while its gate (`PalwCarrierSlotsV1::offers`) lets it.
fn carrier_tick(
    last: Option<PalwCarrierLaneV1>,
    holds: impl Fn(PalwCarrierSiteV1) -> bool,
) -> (Vec<PalwCarrierSiteV1>, Option<PalwCarrierLaneV1>) {
    let mut slots = PalwCarrierSlotsV1::new(last);
    let (mut inflight, mut sent) = (0usize, Vec::new());
    for site in PalwCarrierSiteV1::TICK_ORDER {
        slots.at(site, inflight);
        if slots.offers(site, inflight) && holds(site) {
            inflight += 1;
            sent.push(site);
        }
    }
    (sent, slots.finish(inflight))
}

/// **The carrier priority: convictions and DA above ordinary carriers, licences still carried, and
/// one carrier in flight at every site.** A tick whose first site sends sends nothing else — the
/// canonical claim and the class registration included (the P2-6 review, LOW: they chained a second
/// and third carrier onto the priority lane's change); on the licences' turn this node's own
/// carriers go first as they did before P2-6, then the collector, then the priority lane; a court
/// queue that never empties shares the slots with a licence always waiting one for one, and a slot
/// nobody else wants goes to it.
#[test]
fn the_priority_lane_goes_first_and_licences_are_still_carried() {
    use super::PalwCarrierLaneV1::{Licence, Ordinary, Priority};
    use super::PalwCarrierSiteV1::{Licences, Own, OwnReceipts, PriorityAfterLicences, PriorityFirst, ReadinessEscalated};
    // Every site holds something but M1's: no possession proof is about to lapse.
    let all = |site: PalwCarrierSiteV1| site != ReadinessEscalated;
    assert_eq!(carrier_tick(None, all), (vec![PriorityFirst], Some(Priority)), "priority first, and nothing after it");
    assert_eq!(carrier_tick(Some(Licence), all), (vec![PriorityFirst], Some(Priority)));
    assert_eq!(carrier_tick(Some(Ordinary), all), (vec![PriorityFirst], Some(Priority)));
    assert_eq!(carrier_tick(Some(Priority), all), (vec![Own], Some(Ordinary)), "the licences' turn: own carriers first");
    let storm = |site: PalwCarrierSiteV1| matches!(site, PriorityFirst | PriorityAfterLicences | Licences);
    assert_eq!(carrier_tick(Some(Priority), storm), (vec![Licences], Some(Licence)), "then the licences");
    let only_priority = |site: PalwCarrierSiteV1| matches!(site, PriorityFirst | PriorityAfterLicences);
    assert_eq!(carrier_tick(Some(Priority), only_priority), (vec![PriorityAfterLicences], Some(Priority)), "an empty turn");
    let only_receipts = |site: PalwCarrierSiteV1| site == OwnReceipts;
    assert_eq!(carrier_tick(Some(Priority), only_receipts), (vec![OwnReceipts], Some(Ordinary)));
    assert_eq!(carrier_tick(Some(Priority), |_| false), (vec![], Some(Priority)), "nothing sent: the turn stands");
    // One in flight: with a carrier unconfirmed, no site sends.
    for last in [None, Some(Priority), Some(Licence)] {
        let slots = PalwCarrierSlotsV1::new(last);
        assert!(PalwCarrierSiteV1::TICK_ORDER.iter().all(|site| !slots.offers(*site, MAX_INFLIGHT_CARRIERS)));
    }
    // A DA storm against a licence always waiting: one for one, never two priority slots in a row.
    let run = |ticks: usize, licence_waits: &dyn Fn(usize) -> bool| {
        let (mut last, mut sent) = (None, Vec::new());
        for tick in 0..ticks {
            let (out, lane) = carrier_tick(last, |site| match site {
                PriorityFirst | PriorityAfterLicences => true,
                Licences => licence_waits(tick),
                ReadinessEscalated | Own | OwnReceipts => false,
            });
            assert_eq!(out.len(), 1, "the storm always fills the slot, and only the slot");
            sent.push(lane.expect("a lane"));
            last = lane;
        }
        sent
    };
    let storm = run(100, &|_| true);
    assert_eq!(storm.iter().filter(|lane| **lane == Licence).count(), 50, "a DA storm shares the slots one for one");
    assert!(storm.windows(2).all(|pair| pair != [Priority, Priority]), "never two priority slots while a licence waits");
    let sparse = run(100, &|tick| tick % 10 == 3);
    assert_eq!(sparse.iter().filter(|lane| **lane == Licence).count(), 10, "every waiting licence is carried in its slot");
}

/// The licence collector leaves a claim alone for [`LICENCE_IDLE_REPLAN_DAA`] when its candidates came to no object, asks it
/// again at once when a receipt arrives or leaves, and a collector's tick never spends more than its budget asking
/// (the 2026-10-03 panel starvation: ~200 s of a 280 s tick on a 5.104 seat).
#[test]
fn the_licence_collector_rests_a_claim_whose_candidates_came_to_nothing_and_a_tick_has_a_budget() {
    let asked_at = Some((7u64, 100u64));
    assert!(palw_licence_idle_v1(asked_at.as_ref(), 7, 100), "the same candidates the same DAA: idle");
    assert!(palw_licence_idle_v1(asked_at.as_ref(), 7, 100 + LICENCE_IDLE_REPLAN_DAA - 1), "and inside the replan interval");
    assert!(!palw_licence_idle_v1(asked_at.as_ref(), 7, 100 + LICENCE_IDLE_REPLAN_DAA), "re-asked once the interval passes");
    assert!(!palw_licence_idle_v1(asked_at.as_ref(), 8, 100), "a receipt arrived or left: asked at once");
    assert!(!palw_licence_idle_v1(None, 7, 100), "a claim never asked is asked");
    assert!(LICENCE_IDLE_REPLAN_DAA < COURT_MOVE_REPLAN_DAA, "a licence waits less than a supplementary set");
    assert!(COLLECTOR_ASSEMBLE_BUDGET <= std::time::Duration::from_secs(30), "a tick's assembler work is bounded");
    // The fingerprint the loop keys on moves with the candidates (and is the supplementary collector's own).
    let none: (Vec<PalwSeatReceiptV3>, Vec<PalwSeatReceiptV2>) = (Vec::new(), Vec::new());
    assert_eq!(
        palw_supplementary_candidates_fingerprint_v1(&none.0, &none.1),
        palw_supplementary_candidates_fingerprint_v1(&none.0, &none.1)
    );
}

/// The loop's stopwatch (the 2026-10-03 panel starvation): an iteration is recorded however it ends, the status line
/// names its length and its slowest phase, and a phase is what ran since the previous mark.
#[test]
fn a_panel_tick_is_recorded_with_its_slowest_phase_on_every_way_out_of_the_iteration() {
    fn one_iteration(exit_early: bool) {
        let mut guard = PalwTickGuardV1::new();
        std::thread::sleep(std::time::Duration::from_millis(5));
        guard.phase("quick");
        std::thread::sleep(std::time::Duration::from_millis(40));
        guard.phase("slow");
        if exit_early {
            // A `continue` or an early return: the guard drops with whatever phases it has.
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    for exit_early in [false, true] {
        one_iteration(exit_early);
        let status = palw_panel_tick_status_v1();
        assert!(status.starts_with("tick_last_s="), "the status names the last iteration: {status}");
        assert!(status.contains(&format!("tick_max{PALW_PANEL_TICK_HISTORY_V1}_s=")), "{status}");
        assert!(status.contains("tick_slowest=slow:"), "the 40 ms phase is the slowest of the three: {status}");
    }
    // The history is bounded.
    for _ in 0..(PALW_PANEL_TICK_HISTORY_V1 + 5) {
        drop(PalwTickGuardV1::new());
    }
    assert!(PALW_PANEL_TICKS_V1.lock().unwrap().0.len() <= PALW_PANEL_TICK_HISTORY_V1);
}

/// **The tick is that scheduler**: the submitter builds one `PalwCarrierSlotsV1` from the last
/// lane, marks each site in `TICK_ORDER`, asks the one gate at every site — the priority lane at
/// both of its sites, the canonical claim, the class registration and the possession proofs, the
/// collector and the supplementary receipts — and reads the lane of record back at the end; no
/// site asks the in-flight cap by itself any more, so none can skip the scheduler.
#[test]
fn the_tick_runs_every_carrier_site_through_the_one_scheduler_in_tick_order() {
    let source = the_source();
    let submitter = &source[source.find("// --- the collector + submitter's half ---").expect("the submitter")..];
    let submitter = &submitter[..submitter.find("// Forget what the chain has moved past").expect("its end")];
    let built = submitter.find("let mut slots = PalwCarrierSlotsV1::new(last_lane);").expect("one scheduler");
    let sites = &submitter[built..];
    let mut at = 0;
    for site in PalwCarrierSiteV1::TICK_ORDER {
        let mark = format!("slots.at(PalwCarrierSiteV1::{site:?}, inflight);");
        let found = sites.find(&mark).unwrap_or_else(|| panic!("{mark}"));
        assert!(found >= at, "{site:?} in TICK_ORDER");
        assert_eq!(sites.matches(&mark).count(), 1, "{site:?} marked once");
        at = found;
    }
    let gate = |site: &str| sites.matches(&format!("slots.offers(PalwCarrierSiteV1::{site}, inflight)")).count();
    assert_eq!(gate("ReadinessEscalated"), 1, "M1: the one escalated possession proof, ahead of the court queue");
    assert_eq!(gate("PriorityFirst"), 1);
    assert_eq!(
        gate("Own"),
        4,
        "the canonical claim, an evaluation claim (RFC-0004 A10), the class registration, the possession proofs"
    );
    assert_eq!(
        gate("Licences"),
        4,
        "the collector's licences; the supplementary collector's entry and each offer (F4 part 2); RFC-0006's carriage of a layer-shard part, \
             plan declaration or shard possession proof"
    );
    let supplementary = sites.find("session.palw_v2_supplementary_assemble(claim, v3, v2)").expect("the supplementary collector");
    let licences_at = sites.find("slots.at(PalwCarrierSiteV1::Licences, inflight);").expect("the Licences site");
    let priority_after = sites.find("slots.at(PalwCarrierSiteV1::PriorityAfterLicences, inflight);").expect("the priority lane after");
    assert!(
        licences_at < supplementary && supplementary < priority_after,
        "the supplementary collector rides the Licences site: after the licences, before the priority lane's turn after them"
    );
    assert_eq!(gate("PriorityAfterLicences"), 1);
    assert_eq!(gate("OwnReceipts"), 3, "a seat's own receipt, and (RFC-0007) its sealed vertex and its trap object (Part IV.1)");
    assert!(sites.contains("if slots.offers(PalwCarrierSiteV1::Own, inflight)\n                    && self.config.canonical_claims"));
    assert!(sites.contains("if slots.offers(PalwCarrierSiteV1::Own, inflight)\n                    && !registration_waits"));
    assert_eq!(sites.matches(".carry_priority_v1(").count(), 2, "two call sites, each behind its gate");
    for call in sites.match_indices(".carry_priority_v1(").map(|(i, _)| i) {
        let gated = sites[..call].rfind("if slots.offers(PalwCarrierSiteV1::Priority").expect("gated");
        assert!(call - gated < 120, "the call is the gate's body");
    }
    for own_gate in ["inflight < MAX_INFLIGHT_CARRIERS", "inflight >= MAX_INFLIGHT_CARRIERS"] {
        assert!(!sites.contains(own_gate), "no site asks the cap by itself: {own_gate}");
    }
    assert!(sites.contains("last_lane = slots.finish(inflight);"));
    assert!(!submitter.contains("std::mem::take(&mut court_pending)"), "the court queue is the priority lane's alone");
}

/// **N-5 / P2-6: an accusation stands on a seat that was never served** (the P2-6 review, HIGH and
/// LOW). A seat that asked on its first tick (unserved), was then served — its replay of the job
/// running (`Waiting`) — and was still replaying when the licence landed is not in the seam's
/// `ended`, so nothing is accused; a seat that held nothing to the end is. Served is sticky: a
/// later tick that finds nothing does not make it unserved again. A resuming seat that holds the
/// job and lacks an opening files its `Unavailable` but is served (it abstains from accusing); one
/// that holds nothing is not.
#[test]
fn a_seat_served_after_its_first_tick_accuses_nobody_when_the_licence_lands() {
    let (served_late, never, resumed) = (claim(1), claim(2), claim(3));
    let mut service = PalwSeatServiceV1::default();
    let mut book = PalwSeatAccusationsV1::default();
    // Tick 1: both seats reached the material wait holding nothing (the tail, a free-prompt claim).
    for c in [served_late, never] {
        assert_eq!(palw_seat_tail_v1(true, true, false, || false), PalwSeatTailV1::Waits { unserved: true });
        service.note_unserved(c, 1_001);
    }
    // Tick 2: the job arrives for one, and its replay starts: served.
    assert!(palw_seat_replay_served_v1(&PalwSeatReplayStepV1::Waiting));
    assert!(palw_seat_replay_served_v1(&PalwSeatReplayStepV1::Refuted));
    assert!(palw_seat_replay_served_v1(&PalwSeatReplayStepV1::Licensed));
    assert!(!palw_seat_replay_served_v1(&PalwSeatReplayStepV1::NoVerdict), "NoVerdict says nothing");
    service.note_served(served_late);
    // Tick 3: the pool was swept and the tail finds nothing again — served stays served.
    service.note_unserved(served_late, 1_003);
    assert!(!service.is_unserved(&served_late) && service.is_unserved(&never));
    // The resume: holding the job but no opening is served; holding nothing is not.
    let starved = |served| PalwSeatResumeStepV1::Starved { missing: vec![], served };
    assert!(palw_seat_resume_served_v1(&starved(false), true), "holds the job: abstains");
    assert!(!palw_seat_resume_served_v1(&starved(false), false), "holds nothing: unserved");
    assert!(palw_seat_resume_served_v1(&starved(true), false), "an authenticated opening");
    assert!(palw_seat_resume_served_v1(&PalwSeatResumeStepV1::Waiting, false));
    assert!(matches!(service.resumed(resumed, 1_001, false, starved(false)), PalwSeatResumeStepV1::Starved { .. }));
    assert!(service.is_unserved(&resumed), "a resume that holds nothing");
    service.resumed(resumed, 1_002, true, starved(false));
    assert!(!service.is_unserved(&resumed), "then the job the answer authenticates: served");
    // The licence lands on all three while the replay still runs: the duties end.
    let ended = service.ended(|_| false);
    assert_eq!(ended, vec![(never, 1_001)], "only the seat that was never served");
    for (c, _) in &ended {
        let licensed = kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 1_010 };
        if palw_seat_unserved_licence_v1(Some(&licensed)) {
            book.want(*c, 1_010 + PALW_SEAT_DA_ACCUSE_MARGIN_DAA_V1);
        }
    }
    assert!(book.wants(&never) && !book.wants(&served_late) && !book.wants(&resumed));
    assert!(service.ended(|_| false).is_empty(), "each ends once");
    // A redraw that brings a claim back starts it afresh.
    service.note_unserved(served_late, 2_000);
    assert!(service.is_unserved(&served_late), "the duty ended; served left with it");
}

/// **Where the tick records service** (the P2-6 review, HIGH): every tick an unanswered
/// free-prompt duty noted unserved is asked whether this seat now holds a job of it (pool or
/// retention) before any arm or memory deferral; the replay pass's step, the resume's step, the
/// tail's `Silent`, a fault found and a `Valid` note service; the attempt lane's resume is served
/// by the anchor's job; an `Unavailable` accuses only an unserved claim; the seam reads
/// `ended`; and no site writes the old map.
#[test]
fn the_tick_records_service_where_it_sees_it() {
    let source = the_source();
    let worker = body_of(source, "    pub async fn worker(");
    assert!(!worker.contains("unserved.entry(") && !worker.contains("unserved.remove("), "the first-absence map is gone");
    let block = &source[source.find("let verdict = 'verdict: {").unwrap()..];
    let block = &block[..block.find("let Some(verdict) = verdict else { continue };").unwrap()];
    let pre = source.find("if seat_r && duty.free_prompt && service.is_unserved(&duty.claim_id) {").expect("the pre-check");
    let budget = source.find("self.replay_memory_budget_v1(&session, Some((duty.class_id, duty.artifact_root)))").expect("deferral");
    let verdict = source.find("let verdict = 'verdict: {").unwrap();
    assert!(pre < budget && budget < verdict, "asked before a replay deferred for memory skips the verdict");
    assert!(source[pre..budget].contains(".fp_job_material_for_claim(&duty.claim_id, duty.class_id, &duty.executor_bond, pooled)"));
    assert_eq!(block.matches("match service.resumed(duty.claim_id, current_daa, holds_job, step) {").count(), 1, "the FP resume");
    assert_eq!(block.matches("if palw_seat_replay_served_v1(&pass.0) {").count(), 1, "the FP replay");
    let fault = block.find("if self.seat_found_fault_v1(&duty.claim_id) {").unwrap();
    assert!(block[fault..fault + 300].contains("service.note_served(duty.claim_id);"), "a fault found");
    let attempt =
        block.find("if seat_r_duty.role == PalwSeatRRoleV1::PartialResumes {\n                                //").expect("C7");
    let route = attempt + block[attempt..].find(".seat_s4_resume_v1(").unwrap();
    assert!(block[attempt..route].contains("service.note_served(duty.claim_id);"), "the attempt lane's job is the anchor's");
    assert_eq!(
        block.matches("service.note_unserved(duty.claim_id, current_daa)").count(),
        3,
        "the tensor claim's pass (RFC-0003), the FP replay, the tail"
    );
    let valid = source.find("if valid {\n                    service.note_served(duty.claim_id);").expect("a Valid");
    let hook = source.find(
        "if matches!(verdict, PalwReceiptVerdictV2::Unavailable { .. })\n                    && service.is_unserved(&duty.claim_id)",
    );
    assert!(hook.is_some_and(|hook| valid < hook), "an Unavailable accuses only an unserved claim");
    assert!(source.contains("let ended = service.ended(|claim| duty_claims.contains(claim));"), "the seam");
}
