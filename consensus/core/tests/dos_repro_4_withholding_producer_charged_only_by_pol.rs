//! **DoS repro 4: a withholding producer, measured at an attacker the audit's #9 admits, on the
//! fixed code (#10).**
//!
//! Finding (at the audit's commit): a producer whose claim never binds, or binds and never
//! licenses, was not charged by the fold. `sweep_deadlines` voided a `Provisional` claim past
//! `window_bind` and a `PanelBound` claim at its second receipt timeout through `void_claim`; the
//! only producer charge was `DefaultAccused` -> disclose-window lapse ->
//! `void_and_slash(ProducerWithholding)`, which took `claim.reserved` (38,540 sompi on the floor).
//! Report §2 row 4: others pinned 1,280.34 MSK x 1,202 DAA against 0.000385 MSK x 1,204 DAA of the
//! producer's and a charge of 0 — 3,316,585x (3,322,103x on the DA route).
//!
//! **What changed.** Option A put the claim's escrow on the producer's bond next to its weight;
//! #9 (b38356fe) refuses an attempt whose escrow-inclusive reservation would take the bond past
//! `collateral x ratio` on the live state; #10 (same commit) voids the SECOND `ReceiptTimeout` and a
//! `ProducerWithholding` through `void_and_slash`, which past the fence takes weight + escrow. A
//! `BindTimeout` stays uncharged by design (the chain binds panels itself; a bind that never
//! happened is the network's capacity failing). So the 1,000 MSK producer these repros modelled
//! can no longer post a single floor attempt: every attacker below is sized from the runtime
//! ([`admitted_per_attempt`]) — `ceil((reserved + escrow) x 1000 / ratio)` per concurrent attempt.
//!
//! The pre-fence behaviour is kept as `PRE-FENCE DEFECT RECORD`s that fold the same scenario with
//! the audit fence forced off (`fold_pre_fence`) at the same rescaled attacker.
//!
//! Everything below runs through the real fold (`apply_palw_transition_v7`) on testnet-12's own
//! genesis state, bundle params and fences, using the `dos_l5_common` fixture. Panels are drawn by
//! the real `derive_panel_v2`. Acceptance-layer checks (signatures, the full-policy draw) are not
//! run here. They do not change the fold's charge: every seat's duty is priced per claim, so which
//! 5 of the 8 genesis bonds are seated does not move the defender figures.
//!
//! Run: cargo test -p kaspa-consensus-core --test dos_repro_4_withholding_producer_charged_only_by_pol -- --nocapture --test-threads=2

#[path = "dos_l5_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_panel_economy_v1::{palw_panel_seat_exposure_v1, palw_seat_exposure_v1};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, derive_panel_v2};
use kaspa_consensus_core::palw_pwu::palw_pwu_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2,
    PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error, PalwVoidReasonV2, palw_da_accusation_exposure_v2, palw_da_disclose_window_daa_v1,
};

const ATTACKER: u64 = 4;
const N_UNBOUND: u64 = 1_200;
/// The report's pre-fix figures (§2 row 4), for the before/after lines.
const BEFORE_RATIO_TIMEOUT: f64 = 3_316_585.0;
const BEFORE_RATIO_DA: f64 = 3_322_103.0;

type FoldFn = fn(
    &Params,
    &PalwStateParamsV2,
    &PalwChainStateV2,
    &PalwBlockContextV2,
    &[PalwConsensusObjectV2],
    PalwBlockWorkV3<'_>,
    Hash64,
) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error>;

fn seated(p: &Params, s: &PalwChainStateV2, claim: &Hash64, anchor: Hash64) -> Vec<PalwPanelSeatV2> {
    let b = bundle(p);
    derive_panel_v2(s, &b.panel, claim, anchor, b.state.min_collateral_sompi()).expect("the real draw seats a panel")
}

fn all_bonds_reserved(s: &PalwChainStateV2, keys: &[PalwBondKeyV2]) -> u128 {
    keys.iter().map(|k| s.reserved_exposure(k)).sum()
}

struct Book {
    last: u64,
    others: u128,
    producer: u128,
}
impl Book {
    /// Integrate reserved sompi x DAA over [last, now), with the state that held on that interval.
    fn tick(&mut self, s: &PalwChainStateV2, genesis_keys: &[PalwBondKeyV2], now: u64) {
        let dt = (now - self.last) as u128;
        self.others += all_bonds_reserved(s, genesis_keys) * dt;
        self.producer += s.reserved_exposure(&bond_key(ATTACKER)) * dt;
        self.last = now;
    }
}

/// The floor row, its pwu, and what #9 admits per concurrent floor attempt (read off the runtime).
fn floor_terms(p: &Params) -> (Hash64, u64, AdmittedPerAttempt) {
    let (floor, leaves, target, _) = genesis_classes(p)[0];
    let pwu = palw_pwu_v1(target, leaves);
    (floor, pwu, admitted_per_attempt(p, floor, pwu, T12_BLOCK_SUBSIDY_SOMPI))
}

fn setup(p: &Params, collateral: u64) -> (PalwStateParamsV2, PalwChainStateV2) {
    let b = bundle(p);
    let sp = b.state.clone();
    let g = genesis_state(p);
    let (s, _, _) = fold(p, &sp, &g, &ctx(1, 1_000, 1, 0), &[bond_obj(ATTACKER, collateral)], PalwBlockWorkV3::None, Hash64::default())
        .expect("the attacker's bond registers");
    (sp, s)
}

/// **(a) A claim that never binds is not charged — by design past #10 — and pins nobody.** The
/// flooder is sized for its own concurrency under #9: one attempt per block, each live for
/// `window_bind` + 1 DAA.
#[test]
fn repro4_a_unbound_junk_voids_at_bind_timeout_uncharged_and_pins_nobody() {
    let p = t12();
    let (floor, pwu, per) = floor_terms(&p);
    let b = bundle(&p);
    let bind = b.state.window_bind();
    let concurrent = N_UNBOUND.min(bind + 2);
    let collateral = per.collateral * concurrent;
    let (sp, mut s) = setup(&p, collateral);
    let f = flags(&p, 1_000);
    println!("=== t12 fold flags at DAA 1000: unavailable_abstains={} da_court={} capability_bound={} ===", f.unavailable_abstains, f.da_court, f.capability_bound);
    let genesis = genesis_bonds(&p);
    let keys: Vec<PalwBondKeyV2> = genesis.iter().map(|g| g.0).collect();
    let others_before = all_bonds_reserved(&s, &keys);

    let bytes_before = carriage_bytes(&s);
    let mut ids = Vec::with_capacity(N_UNBOUND as usize);
    let mut daa = 1_000u64;
    let mut blue = 1u64;
    let mut escrow_total: u128 = 0;
    let mut peak_reserved = 0u128;
    let mut others_peak = 0u128;
    for i in 0..N_UNBOUND {
        daa += 1;
        blue += 1;
        let (env, key, id) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, 0x4A00 + i, 0x4B00_0000 + i);
        let (next, _, skips) = fold(&p, &sp, &s, &ctx(0x4000 + i, daa, blue, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .unwrap_or_else(|e| panic!("junk #{i}: {e:?}"));
        assert!(skips.is_empty(), "junk #{i}: an attacker sized by #9 is admitted: {skips:?}");
        s = next;
        let c = s.claim(&id).unwrap();
        escrow_total += c.escrowed_reward as u128;
        ids.push(id);
        peak_reserved = peak_reserved.max(s.reserved_exposure(&bond_key(ATTACKER)));
        others_peak = others_peak.max(all_bonds_reserved(&s, &keys));
    }
    // One more attempt past the sized concurrency is what #9 refuses (the live ceiling holds).
    let ceiling = collateral as u128 * u128::from(per.ratio_permille) / 1000;

    // Try the ONE path that can charge a producer for withholding, on an unbound claim still
    // inside its bind window. A seat bond accuses it. The fold refuses: there is no panel.
    let accuser = genesis[0].0;
    let accuse_unbound = fold(
        &p,
        &sp,
        &s,
        &ctx(0x4F00, daa + 1, blue + 1, 0),
        &[PalwConsensusObjectV2::DefaultAccused { claim: ids[N_UNBOUND as usize - 1], missing_event_index: 0, accuser, signature: vec![] }],
        PalwBlockWorkV3::None,
        Hash64::default(),
    );
    let accuse_err = accuse_unbound.as_ref().err().map(|e| format!("{e:?}"));

    daa += bind + 1;
    blue += 1;
    s = fold(&p, &sp, &s, &ctx(0x4E00, daa, blue, 0), &[], PalwBlockWorkV3::None, Hash64::default()).unwrap().0;

    let reasons: std::collections::BTreeMap<String, usize> = ids.iter().fold(Default::default(), |mut m, id| {
        let k = match s.claim(id).map(|c| &c.phase) {
            Some(PalwClaimPhaseV2::Voided { reason, .. }) => format!("Voided({reason:?})"),
            Some(other) => format!("{other:?}"),
            None => "retired".to_string(),
        };
        *m.entry(k).or_default() += 1;
        m
    });
    let rec = s.bond(&bond_key(ATTACKER)).unwrap();
    let (collateral_after, slashed_after) = (rec.collateral, rec.slashed);
    let reserved_after = s.reserved_exposure(&bond_key(ATTACKER));
    let bytes_after = carriage_bytes(&s);

    println!("=== (a) {N_UNBOUND} junk floor claims, none bound, bind window {bind} DAA ===");
    println!(
        "#9 per concurrent floor attempt: reserved {} + escrow {} = {} sompi ({:.2} MSK) -> collateral {:.2} MSK at {} permille",
        per.reserved,
        per.escrow,
        per.reservation,
        msk(per.reservation),
        msk(per.collateral as u128),
        per.ratio_permille
    );
    println!("attacker posts {concurrent} x that = {:.2} MSK (before: 1,000 MSK, which #9 now refuses a single attempt)", msk(collateral as u128));
    println!("attacker reservation at peak = {peak_reserved} sompi ({:.2} MSK), ceiling {ceiling}", msk(peak_reserved));
    println!("DefaultAccused on an unbound claim -> {accuse_err:?}");
    println!("claim phases after window_bind: {reasons:?}");
    println!("attacker collateral {collateral} -> {collateral_after} sompi; bond.slashed = {slashed_after}; reserved after = {reserved_after}");
    println!("escrow burned across the flood (forgone by the attacker, not taken from its bond) = {escrow_total} sompi ({:.2} MSK)", msk(escrow_total));
    println!("carriage bytes {bytes_before} -> {bytes_after} (+{})", bytes_after as i64 - bytes_before as i64);
    println!("--- cost ledger ---");
    println!("attacker: charge 0 (BindTimeout is uncharged by design, b38356fe), locks {:.2} MSK per concurrent claim for {bind} DAA", msk(per.reservation));
    println!("defender: honest reserved {others_before} -> peak {others_peak}: no panel is bound, nothing pinned");

    assert!(peak_reserved <= ceiling, "the flood fits the ceiling #9 holds on the live state");
    assert_eq!(reasons.get("Voided(BindTimeout)").copied().unwrap_or(0), N_UNBOUND as usize, "every unbound junk claim voids BindTimeout");
    assert_eq!(collateral_after, collateral, "a BindTimeout is not charged (b38356fe, by design)");
    assert_eq!(slashed_after, 0, "bond.slashed stays 0");
    assert_eq!(reserved_after, 0, "the whole reservation is handed back at void");
    assert_eq!(others_peak, others_before, "an unbound claim pins no other party's collateral");
    assert!(
        matches!(accuse_unbound, Err(PalwStateV2Error::DaClaimNotAccusable(_))),
        "a DA accusation cannot reach an unbound claim: {accuse_err:?}"
    );
}

struct Withheld {
    collateral: u64,
    per: AdmittedPerAttempt,
    reserved: u128,
    escrow: u64,
    per_seat: u128,
    others_bound: u128,
    others_rebound: u128,
    others_after: u128,
    phase1: String,
    phase2: Option<PalwClaimPhaseV2>,
    pd: Result<(), String>,
    collateral_after: u64,
    slashed_after: u64,
    seats_posted: u64,
    seats_after: u64,
    producer_after: u128,
    book_others: u128,
    book_producer: u128,
    receipt_window: u64,
}

/// One bound junk floor claim whose producer serves nothing through two panels, folded by `go`.
/// The producer posts exactly what #9 admits for one attempt.
fn withheld_through_two_panels(go: FoldFn) -> Withheld {
    let p = t12();
    let (floor, pwu, per) = floor_terms(&p);
    let collateral = at_least_the_floor(&p, per.collateral);
    let (sp, mut s) = setup(&p, collateral);
    let b = bundle(&p);
    let genesis = genesis_bonds(&p);
    let keys: Vec<PalwBondKeyV2> = genesis.iter().map(|g| g.0).collect();
    let seats_posted: u64 = genesis.iter().map(|g| g.2).sum();

    let (env, key, id) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, 0x4C01, 0x4C01_0000);
    let mut daa = 1_001u64;
    let mut blue = 2u64;
    let step = |s: &PalwChainStateV2, daa: u64, blue: u64, objs: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, sub: u64| {
        go(&p, &sp, s, &ctx(0x4C00_0000 + blue, daa, blue, sub), objs, work, key)
    };
    let (next, _, skips) = step(&s, daa, blue, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI).unwrap();
    assert!(skips.is_empty(), "an attacker posting what #9 admits gets its attempt: {skips:?}");
    s = next;
    let claim = s.claim(&id).expect("the junk claim is recorded").clone();
    let others_baseline = all_bonds_reserved(&s, &keys);
    let mut book = Book { last: daa, others: 0, producer: 0 };

    // Panel #1, drawn by the real sortition.
    daa += 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    let seats1 = seated(&p, &s, &id, h(0xA1));
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA1), seats: seats1.clone() }], PalwBlockWorkV3::None, Hash64::default(), 0)
        .unwrap()
        .0;
    let others_bound = all_bonds_reserved(&s, &keys) - others_baseline;
    let receipt_window = sp.receipt_window_for_claim_v1(&s, &floor, daa);

    // ProducerDefaulted with a full Unavailable quorum: the old consensus withholding charge.
    let unavailable: Vec<PalwSeatReceiptV2> = seats1
        .iter()
        .map(|seat| PalwSeatReceiptV2 {
            claim: id,
            verdict: PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: daa },
            seat_bond: seat.bond,
            signed_daa: daa,
            signature: vec![],
        })
        .collect();
    let pd = step(&s, daa + 1, blue + 1, &[PalwConsensusObjectV2::ProducerDefaulted { claim: id, receipts: unavailable }], PalwBlockWorkV3::None, Hash64::default(), 0)
        .map(|_| ())
        .map_err(|e| format!("{e:?}"));

    // Receipt window #1 lapses -> redraw.
    daa += receipt_window + 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap().0;
    let phase1 = format!("{:?}", s.claim(&id).unwrap().phase);

    // Panel #2 on the redraw's anchor.
    daa += 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    let seats2 = seated(&p, &s, &id, h(0xA2));
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA2), seats: seats2 }], PalwBlockWorkV3::None, Hash64::default(), 0)
        .unwrap()
        .0;
    let others_rebound = all_bonds_reserved(&s, &keys) - others_baseline;

    // Receipt window #2 lapses -> void.
    daa += receipt_window + 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    s = step(&s, daa, blue, &[], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap().0;
    let phase2 = s.claim(&id).map(|c| c.phase.clone());
    let rec = s.bond(&bond_key(ATTACKER)).unwrap();
    let per_seat = palw_panel_seat_exposure_v1(claim.reserved, claim.escrowed_reward, b.panel.seat_count() as usize, extras(&p, daa).panel_reward_multiple_permille);
    Withheld {
        collateral,
        per,
        reserved: claim.reserved,
        escrow: claim.escrowed_reward,
        per_seat,
        others_bound,
        others_rebound,
        others_after: all_bonds_reserved(&s, &keys) - others_baseline,
        phase1,
        phase2,
        pd,
        collateral_after: rec.collateral,
        slashed_after: rec.slashed,
        seats_posted,
        seats_after: genesis.iter().map(|g| s.bond(&g.0).unwrap().collateral).sum(),
        producer_after: s.reserved_exposure(&bond_key(ATTACKER)),
        book_others: book.others,
        book_producer: book.producer,
        receipt_window,
    }
}

fn print_withheld(label: &str, w: &Withheld) {
    let p = t12();
    let b = bundle(&p);
    let charge = (w.collateral - w.collateral_after) as u128;
    let pinned_daa = w.book_others / w.others_bound.max(1);
    let held_daa = w.book_producer / w.per.reservation.max(1);
    println!("=== (b) {label}: one BOUND junk floor claim, producer serves nothing ===");
    println!(
        "producer posts {:.2} MSK = what #9 admits for one attempt (reserved {} + escrow {} = {:.2} MSK at {} permille)",
        msk(w.collateral as u128),
        w.reserved,
        w.escrow,
        msk(w.per.reservation),
        w.per.ratio_permille
    );
    println!(
        "seat duty per seat = palw_panel_seat_exposure_v1(reserved, escrow, {}, lambda) = max(3 x reserved = {}, lambda x escrow-share) = {} sompi ({:.2} MSK)",
        b.panel.seat_count(),
        palw_seat_exposure_v1(w.reserved),
        w.per_seat,
        msk(w.per_seat)
    );
    println!("reserved on OTHER bonds: bind #1 +{:.2} MSK, redraw bind #2 +{:.2} MSK, after void +{}", msk(w.others_bound), msk(w.others_rebound), w.others_after);
    println!("ProducerDefaulted (full Unavailable quorum) -> {:?}", w.pd);
    println!("after receipt timeout #1: phase {}", w.phase1);
    println!("after receipt timeout #2: phase {:?}", w.phase2);
    println!("producer: collateral {} -> {}, bond.slashed = {}, reserved after = {}", w.collateral, w.collateral_after, w.slashed_after, w.producer_after);
    println!("seats:    collateral {} -> {}", w.seats_posted, w.seats_after);
    println!("--- cost ledger ---");
    println!("attacker: charged {charge} sompi ({:.2} MSK); locked {} sompi*DAA ({:.2} MSK x {held_daa} DAA)", msk(charge), w.book_producer, msk(w.per.reservation));
    println!("defender: pinned {} sompi*DAA on other bonds = {:.2} MSK x {pinned_daa} DAA", w.book_others, msk(w.others_bound));
    println!(
        "capital x time others / producer = {:.4} (before: {BEFORE_RATIO_TIMEOUT:.0}); peak pinned / charge = {}",
        w.book_others as f64 / w.book_producer.max(1) as f64,
        if charge == 0 { "inf (charge 0)".to_string() } else { format!("{:.4}", w.others_bound as f64 / charge as f64) }
    );
}

/// **(b) A bound claim withholds through two panels and, past #10, forfeits weight + escrow at the
/// second `ReceiptTimeout` — more than the capital it pinned on others.**
#[test]
fn repro4_b_bound_junk_two_receipt_timeouts_forfeits_weight_and_escrow() {
    let w = withheld_through_two_panels(fold);
    print_withheld("FIXED (#9 + #10)", &w);
    let charge = (w.collateral - w.collateral_after) as u128;
    assert!(matches!(w.pd, Err(ref e) if e.starts_with("ProducerDefaultRetired")), "ProducerDefaulted is refused on t12: {:?}", w.pd);
    assert!(w.phase1.contains("Provisional"), "first receipt timeout redraws");
    assert!(matches!(w.phase2, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. })), "second receipt timeout voids");
    assert_eq!(charge, w.per.reservation, "#10: the second ReceiptTimeout takes the escrow-inclusive reservation");
    assert_eq!(w.slashed_after as u128, w.per.reservation);
    assert_eq!(w.seats_after, w.seats_posted, "no seat is charged");
    assert_eq!(w.others_after, 0, "the seats' reservations come back");
    assert_eq!(w.producer_after, 0);
    assert!(w.others_bound > 1_000 * 100_000_000, "more than 1,000 MSK of other parties' collateral pinned per bound junk floor claim");
    assert_eq!(w.others_rebound, w.others_bound, "the redraw pins the same duty again");
    assert!(w.book_others / w.others_bound.max(1) >= 2 * w.receipt_window as u128, "pinned for two receipt windows");
    // **The bound: the attacker pays at least what it pins others for.**
    assert!(charge >= w.others_bound, "charge {:.2} MSK < pinned {:.2} MSK", msk(charge), msk(w.others_bound));
    assert!(w.book_producer >= w.book_others, "and locks at least as much capital x time as it pins");
}

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the same rescaled attacker with the 2026-09-23 audit fence forced off — the second ReceiptTimeout voids through void_claim and charges 0 (report §2 row 4; before option A the capital x time ratio was 3,316,585x); closed by b38356fe (#10)"]
fn repro4_b_pre_fence_record_two_receipt_timeouts_charge_nothing() {
    let w = withheld_through_two_panels(fold_pre_fence);
    print_withheld("PRE-FENCE (#10 off)", &w);
    assert!(matches!(w.phase2, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. })), "second receipt timeout voids");
    assert_eq!(w.collateral_after, w.collateral, "pre-fence: no consensus charge for withholding through two panels");
    assert_eq!(w.slashed_after, 0);
}

struct Accused {
    collateral: u64,
    per: AdmittedPerAttempt,
    reserved: u128,
    phase: Option<PalwClaimPhaseV2>,
    collateral_after: u64,
    slashed_after: u64,
    duty: u128,
    accuser_before: u128,
    accuser_during: u128,
    accuser_after: u128,
    accuser_collateral_before: u64,
    accuser_collateral_after: u64,
    expected_accuser: u128,
    others_during: u128,
    book_others: u128,
    disclose: u64,
}

/// testnet-12 with `palw_rcore_plus` forced off (C7 cleared, mirrors re-synced): the fence-off twin
/// on which ADR-0062's v1 court — the route this repro measured — stands. Past the fence the DA
/// route is ADR-0152 M3's (`rcore_m3_da_court`: sessions in side maps, DA-6's price on the accuser's
/// free half, DA-7's default).
fn t12_rcore_off() -> Params {
    let mut p = t12();
    p.palw_rcore_plus = None;
    p.palw_rcore_conservative_classes = &[];
    p.sync_palw_rcore_plus();
    p
}

/// The DA route, folded by `go`: `DefaultAccused` -> disclose window lapses -> `ProducerWithholding`
/// — ADR-0062's v1 court, on the fence-off twin (ADR-0152 R6).
fn da_accused(go: FoldFn) -> Accused {
    let p = t12_rcore_off();
    let (floor, pwu, per) = floor_terms(&p);
    let collateral = at_least_the_floor(&p, per.collateral);
    let (sp, mut s) = setup(&p, collateral);
    let keys: Vec<PalwBondKeyV2> = genesis_bonds(&p).iter().map(|g| g.0).collect();
    let (env, key, id) = junk_attempt(floor, bond_key(ATTACKER), pubkey_of(ATTACKER), &operator_pubkey_of(ATTACKER), pwu, 0x4D01, 0x4D01_0000);
    let mut daa = 1_001u64;
    let mut blue = 2u64;
    let step = |s: &PalwChainStateV2, daa: u64, blue: u64, objs: &[PalwConsensusObjectV2]| {
        go(&p, &sp, s, &ctx(0x4D00_0000 + blue, daa, blue, 0), objs, PalwBlockWorkV3::None, Hash64::default())
    };
    s = go(&p, &sp, &s, &ctx(0x4D00_0000 + blue, daa, blue, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key).unwrap().0;
    let claim = s.claim(&id).expect("the junk claim is recorded").clone();
    let mut book = Book { last: daa, others: 0, producer: 0 };
    daa += 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    let seats = seated(&p, &s, &id, h(0xA1));
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xA1), seats: seats.clone() }]).unwrap().0;
    let duty = palw_panel_seat_exposure_v1(claim.reserved, claim.escrowed_reward, seats.len(), extras(&p, daa).panel_reward_multiple_permille);

    let accuser = seats[0].bond;
    let accuser_before = s.reserved_exposure(&accuser);
    let accuser_collateral_before = s.bond(&accuser).unwrap().collateral;
    daa += 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    s = step(&s, daa, blue, &[PalwConsensusObjectV2::DefaultAccused { claim: id, missing_event_index: 0, accuser, signature: vec![] }])
        .unwrap_or_else(|e| panic!("DefaultAccused on a bound claim folds: {e:?}"))
        .0;
    let accuser_during = s.reserved_exposure(&accuser);
    let others_during: u128 = all_bonds_reserved(&s, &keys);
    let disclose = palw_da_disclose_window_daa_v1(&sp);
    daa += disclose + 1;
    blue += 1;
    book.tick(&s, &keys, daa);
    s = step(&s, daa, blue, &[]).unwrap().0;
    let rec = s.bond(&bond_key(ATTACKER)).unwrap();
    Accused {
        collateral,
        per,
        reserved: claim.reserved,
        phase: s.claim(&id).map(|c| c.phase.clone()),
        collateral_after: rec.collateral,
        slashed_after: rec.slashed,
        duty,
        accuser_before,
        accuser_during,
        accuser_after: s.reserved_exposure(&accuser),
        accuser_collateral_before,
        accuser_collateral_after: s.bond(&accuser).unwrap().collateral,
        expected_accuser: palw_da_accusation_exposure_v2(claim.reserved, seats.len(), sp.min_collateral_sompi()),
        others_during,
        book_others: book.others,
        disclose,
    }
}

fn print_accused(label: &str, a: &Accused) {
    let charge = (a.collateral - a.collateral_after) as u128;
    let pinned = a.others_during - a.expected_accuser;
    println!("=== (c) {label}: the DA path, on a bound junk floor claim ===");
    println!(
        "accuser (a seat) reserved {} (its seat duty {}) -> {} during the session (+{}) -> {} after void",
        a.accuser_before,
        a.duty,
        a.accuser_during,
        a.accuser_during - a.accuser_before,
        a.accuser_after
    );
    println!("disclose window = {} DAA; phase after: {:?}", a.disclose, a.phase);
    println!("producer posts {:.2} MSK (#9's one attempt); collateral -> {}; bond.slashed = {} sompi ({:.2} MSK)", msk(a.collateral as u128), a.collateral_after, a.slashed_after, msk(a.slashed_after as u128));
    println!("--- cost ledger, DA route ---");
    println!("attacker: charged {charge} sompi ({:.2} MSK) once (before: 38,540 sompi = claim.reserved)", msk(charge));
    println!("defender: pinned {} sompi*DAA on other bonds = {:.2} MSK x {} DAA, plus an accuser who must act by hand", a.book_others, msk(pinned), a.book_others / pinned.max(1));
    println!(
        "capital ratio: pinned {:.2} MSK / charge {:.2} MSK = {} (before: {BEFORE_RATIO_DA:.0}x)",
        msk(pinned),
        msk(charge),
        if charge == 0 { "inf".to_string() } else { format!("{:.4}", pinned as f64 / charge as f64) }
    );
}

/// **(c) The DA route: `DefaultAccused` -> disclose window lapses -> `ProducerWithholding`, which
/// past #10 takes weight + escrow.**
#[test]
fn repro4_c_da_accusation_forfeits_weight_and_escrow() {
    let a = da_accused(fold);
    print_accused("FIXED (#10)", &a);
    let charge = (a.collateral - a.collateral_after) as u128;
    let pinned = a.others_during - a.expected_accuser;
    assert!(matches!(a.phase, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })), "the lapse voids ProducerWithholding");
    assert_eq!(a.reserved, 38_540, "claim.reserved is 38,540 sompi on the t12 floor");
    assert_eq!(charge, a.per.reservation, "#10: ProducerWithholding takes the escrow-inclusive reservation");
    assert_eq!(a.slashed_after as u128, a.per.reservation);
    assert_eq!(a.accuser_before, a.duty, "before the accusation the seat holds its duty only");
    assert_eq!(a.accuser_during - a.accuser_before, a.expected_accuser);
    assert_eq!(a.accuser_after, 0, "the void releases the accuser's duty and its accusation reservation");
    assert_eq!(a.accuser_collateral_after, a.accuser_collateral_before, "the accuser is right and pays nothing");
    assert!(charge >= pinned, "the attacker pays at least what it pinned: charge {:.2} MSK, pinned {:.2} MSK", msk(charge), msk(pinned));
}

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the DA route with the 2026-09-23 audit fence forced off takes claim.reserved only (38,540 sompi; report §2 row 4, 3,322,103x before option A); closed by b38356fe (#10)"]
fn repro4_c_pre_fence_record_da_accusation_takes_claim_reserved_only() {
    let a = da_accused(fold_pre_fence);
    print_accused("PRE-FENCE (#10 off)", &a);
    assert!(matches!(a.phase, Some(PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. })));
    assert_eq!(a.slashed_after as u128, a.reserved, "pre-fence: the charge is exactly claim.reserved");
    assert_eq!(a.reserved, 38_540);
}

/// **(d) Node policy: kaspad builds no `DefaultAccused`. It builds `DefaultAccusedHeld` from one
/// path, the leaf pursuit, which is gated on `held_context && da_court`.** A static scan of the
/// shipped sources, because kaspad is not reachable from a consensus-core test. It is evidence
/// about policy, not a consensus rule.
#[test]
fn repro4_d_kaspad_never_constructs_default_accused() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut constructs = Vec::new();
    let mut held = Vec::new();
    let mut patterns = Vec::new();
    for dir in ["kaspad/src"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                let at = format!("{}:{}", path.file_name().unwrap().to_string_lossy(), n + 1);
                if line.contains("DefaultAccused {") && !line.contains("DefaultAccusedHeld") {
                    if line.contains("{ .. }") { patterns.push(at) } else { constructs.push(at) }
                } else if line.contains("DefaultAccusedHeld {") && !line.contains("{ .. }") {
                    held.push(at);
                }
            }
        }
    }
    let panel = std::fs::read_to_string(root.join("kaspad/src/palw_panel.rs")).unwrap();
    let gate = panel.contains("palw_held_context_active_at(current_daa)") && panel.contains("palw_da_court_in_force_v1(&self.consensus_config, current_daa)");
    // The held demand's `binding` comes off an interval opening the PRODUCER served to this seat;
    // with nothing served, `?` returns None before any accusation is built.
    let needs_served = panel.contains("Base0FpIntervalOpeningV4::decode_v1(bytes).ok().map(|v4| v4.binding))")
        && panel.contains(".filter(|binding| binding.committed_execution_root == duty.execution_root)?;");
    let cli = std::fs::read_to_string(root.join("misaka-cli/src/palw_da.rs")).unwrap();
    let cli_builds = cli.contains("PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index, accuser, signature }");
    println!("=== (d) static scan of kaspad/src (policy, not consensus) ===");
    println!("DefaultAccused constructions in kaspad/src: {constructs:?}");
    println!("DefaultAccused {{ .. }} match arms (name map only): {patterns:?}");
    println!("DefaultAccusedHeld constructions: {held:?}; gated on held_context && da_court: {gate}");
    println!("DefaultAccusedHeld needs a binding off an opening the producer served (pursue_named_leaf_v1): {needs_served}");
    println!("misaka-cli `palw da-accuse` builds DefaultAccused for an operator to file by hand: {cli_builds}");
    assert!(constructs.is_empty(), "kaspad constructs no DefaultAccused");
    assert_eq!(held.len(), 1, "one DefaultAccusedHeld construction");
    assert!(gate);
    assert!(needs_served, "a producer that serves nothing gives the seat no binding, so no held demand is built");
}
