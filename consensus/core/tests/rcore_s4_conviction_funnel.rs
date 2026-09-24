//! **ADR-0152 v3.1 §3.6 (S-4): the conviction funnel — on testnet-12's own fold.**
//!
//! Every conviction opens (for every bond it may charge: `C₀` and whether its exit gate was shut,
//! `palw_bond_collateral_is_locked_v6` on the pre-state), runs its legs through `slash_bond` (which
//! returns its debit), and closes with its consumed record LAST: `amount` the nominal tier, `collected`
//! the debit taken from the bonds whose gate was shut (R-2), `claim_id`. The tiers at m = 3:
//! S0′ (RT#2, `UnavailableQuorum`, `NotReplayBacked`: the forfeit, nothing else), S1 (DA-7's
//! withholding: the forfeit and a strike — one per aligned 1,000-DAA epoch, dropped past 7,500 DAA
//! when the next is written, the third inside the window adding `min(10% · C₀, 3 G)`), S2 (a proven
//! fraud before `Final`, and a court default charged as one: the forfeit + `min(10% · C₀, 3 G)`), S3
//! (after `Final`, through the vesting work's burn hook), S4 (a false `Valid`: the lock +
//! `min(25% · C₀, 3 G)`), U3 (an FP claim after `Final`: `min(25% · C₀, 3 G_fp)`) and Eq on t12
//! (`min(C₀, 3 · G_eq)`, no status change).
//!
//! Every block is checked as `Chain::step` checks one — the delta re-applies and reverts, the carriage
//! reloads under its root — and each test runs beside its fence-off twin (`palw_rcore_plus = None`).
//! The real kind-3 / kind-4 convictions (S2 through `CourtFraud`, S4, the court's `CourtConviction`)
//! are driven on a real claim by kaspa-consensus's T46 suite; the funnel's per-leg units and the
//! reporter-reward seam's bases are `palw_state_v2`'s S-4 lib tests.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_s4_conviction_funnel

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1;
use kaspa_consensus_core::palw_offence_v1::{PalwOffenceKindV1, palw_offence_evidence_digest_v1, palw_offence_id_v1};
use kaspa_consensus_core::palw_state_v2::{
    PALW_RCORE_STRIKE_WINDOW_DAA_V1, PALW_RCORE_VESTING_ROWS_LANDED_V1, PalwBondStatusV2, PalwStateV2Error, PalwVoidReasonV2,
    palw_bond_collateral_is_locked_v6, palw_claim_bond_reservation_v1, palw_da_event_index_v1, palw_eq_cap_basis_v1,
    palw_rcore_eq_cap_v1, palw_rcore_s1s2_action_v1, palw_rcore_s3s4_action_v1,
};

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

fn msk_of(sompi: u128) -> f64 {
    sompi as f64 / MSK as f64
}

/// An event accusation of `(row, 0)` (the fold never reads the signature: the acceptance layer's).
fn accuse(claim: Hash64, accuser: PalwBondKeyV2, row: u32) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index: palw_da_event_index_v1(row, 0), accuser, signature: vec![] }
}

/// A floor claim by the genesis producer, bound to the five genesis seats after it.
fn bound_floor_claim(c: &mut Chain, seed: u64) -> Hash64 {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    c.bind(id, &seats);
    id
}

/// Empty blocks up to `daa`: the deadline's block, then the first past it.
fn run_to(c: &mut Chain, daa: u64) {
    if daa > c.daa + 1 {
        c.step_at(daa - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    c.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
}

/// Every consumed offence on `s`, through the carriage (the rooted map, in key order).
fn offences(s: &PalwChainStateV2) -> Vec<(Hash64, kaspa_consensus_core::palw_offence_v1::PalwConsumedOffenceV1)> {
    PalwStateCarriageV2::from_state(s).consumed_offences.into_iter().collect()
}

/// The collateral of `bond`.
fn collateral(c: &Chain, bond: &PalwBondKeyV2) -> u64 {
    c.s.bond(bond).expect("a bond").collateral
}

/// `G = g_res + escrowed_reward`, from the claim's liability row (written at its void or its Final).
fn g_of(c: &Chain, id: &Hash64) -> u128 {
    let row = c.s.panel_liability(id).expect("the liability row");
    row.g_res_sompi + u128::from(row.escrowed_reward)
}

/// **T22 / T35 / T36 (S1, X11): DA-confirmed withholding forfeits the commitment and strikes the
/// producer — one strike per aligned 1,000-DAA epoch, a strike older than 7,500 DAA dropped when the
/// next is written, the third inside the window adding `min(10% · C₀, 3 G)`; nothing escalates the
/// producer's status.** The genesis producer defaults on six floor claims (a bystander's DA sessions
/// run out on each, every block checked):
///
/// * A and B default in ONE block (epoch e₀): each forfeits its commitment; one strike `[d₁]`, the
///   second S1 in the epoch writes nothing;
/// * C in a later epoch: `[d₁, d₂]`, no action;
/// * D in a third epoch within 7,500 DAA: `[d₁, d₂, d₃]` — the third strike adds
///   `min(10% · C₀, 3 G)` (C₀ the producer's collateral before the block, G the claim's);
/// * E more than 7,500 DAA after d₁: d₁ is dropped when E's strike is written, `[d₂, d₃, d₄]` — the
///   third live strike, the action again.
///
/// Each default writes one `DaDefault` record: `amount` the nominal tier (the forfeit, plus the action
/// on an escalating strike), `collected` the producer's debit (its exit gate is shut: Active).
/// Strikes ride `withholding_strikes` (rooted, carried, journaled `Strikes`; every block reverts and
/// reloads). The producer stays `Active` (T36: no status, no tombstone). The fence-off twin: ADR-0062's
/// court voids `ProducerWithholding` at the same forfeit; no strike, no escalation, no record.
#[test]
fn t22_t35_t36_s1_strikes_one_per_epoch_and_the_third_inside_the_window_escalates() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        c.step(&[bond_obj(1, 50_000 * MSK)]);
        let (producer, _, _) = floor_producer(&c.p);
        // One S1: the claims accused in one block, run out; returns (default DAA, per-claim debits).
        let default = |c: &mut Chain, ids: &[Hash64]| -> (u64, u128) {
            let commitment: u128 = ids.iter().map(|id| palw_claim_bond_reservation_v1(&c.sp, &c.claim(id)).unwrap()).sum();
            let before = collateral(c, &producer);
            let objects: Vec<_> = ids.iter().map(|id| accuse(*id, bond_key(1), 0)).collect();
            c.step(&objects);
            let deadline = if armed {
                c.s.da_session(&ids[0], &bond_key(1)).expect("the session").deadline_daa
            } else {
                c.s.deadline_of(&ids[0]).expect("the v1 disclose deadline")
            };
            run_to(c, deadline + 1);
            for id in ids {
                assert!(
                    matches!(c.claim(id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
                    "armed={armed}: the default voids {id} for withholding"
                );
            }
            (c.daa, u128::from(before - collateral(c, &producer)) - commitment)
        };
        let record = |c: &Chain, id: &Hash64| c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, id)).cloned();
        // A and B, one block.
        let a = bound_floor_claim(&mut c, 0x5A01);
        let b = bound_floor_claim(&mut c, 0x5A02);
        let (d1, extra) = default(&mut c, &[a, b]);
        assert_eq!(extra, 0, "armed={armed}: the first S1 of an epoch forfeits the commitment and no more");
        if !armed {
            // The twin: the same forfeit, never a strike or a record.
            let mut ids = vec![a, b];
            for seed in [0x5A03, 0x5A04, 0x5A05] {
                let id = bound_floor_claim(&mut c, seed);
                assert_eq!(default(&mut c, &[id]).1, 0, "fence off: no S1 escalates");
                ids.push(id);
            }
            assert_eq!(c.s.withholding_strikes(&producer), None, "fence off: no strike is ever written");
            for id in ids {
                assert!(record(&c, &id).is_none(), "fence off: no DaDefault record");
            }
            break;
        }
        assert_eq!(c.s.withholding_strikes(&producer), Some(&[d1][..]), "one strike for the epoch, not two");
        for id in [a, b] {
            let r = record(&c, &id).expect("a DaDefault record per claim");
            let full = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&id)).unwrap();
            assert_eq!((u128::from(r.amount), u128::from(r.collected), r.claim_id), (full, full, id), "nominal = collected = the forfeit");
        }
        // C, a later epoch.
        let c_id = bound_floor_claim(&mut c, 0x5A03);
        let (d2, extra) = default(&mut c, &[c_id]);
        assert_ne!(d2 / 1_000, d1 / 1_000, "the premise: a new epoch");
        assert_eq!(extra, 0, "the second strike adds nothing");
        assert_eq!(c.s.withholding_strikes(&producer), Some(&[d1, d2][..]));
        // D, a third epoch inside the window: the third strike escalates.
        let d_id = bound_floor_claim(&mut c, 0x5A04);
        let c0 = collateral(&c, &producer);
        let (d3, extra) = default(&mut c, &[d_id]);
        assert!(d3 - d1 <= PALW_RCORE_STRIKE_WINDOW_DAA_V1, "the premise: inside the window");
        let action = palw_rcore_s1s2_action_v1(c0, g_of(&c, &d_id));
        assert!(action > 0);
        assert_eq!(extra, action, "the third strike adds min(10% · C₀, 3 G): {:.4} MSK", msk_of(action));
        assert_eq!(c.s.withholding_strikes(&producer), Some(&[d1, d2, d3][..]));
        let r = record(&c, &d_id).unwrap();
        let full = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&d_id)).unwrap();
        assert_eq!((u128::from(r.amount), u128::from(r.collected)), (full + action, full + action), "the record carries the action");
        // E, past d₁ + 7,500: d₁ leaves when E's strike is written, and three stay live.
        run_to(&mut c, d1 + PALW_RCORE_STRIKE_WINDOW_DAA_V1 - 1_000);
        let e_id = bound_floor_claim(&mut c, 0x5A05);
        let c0 = collateral(&c, &producer);
        let (d4, extra) = default(&mut c, &[e_id]);
        assert!(d4 - d1 > PALW_RCORE_STRIKE_WINDOW_DAA_V1 && d4 - d2 <= PALW_RCORE_STRIKE_WINDOW_DAA_V1, "the premise: d₁ ages out, d₂ does not");
        assert_eq!(c.s.withholding_strikes(&producer), Some(&[d2, d3, d4][..]), "the old strike is dropped when the next is written");
        assert_eq!(extra, palw_rcore_s1s2_action_v1(c0, g_of(&c, &e_id)), "three live strikes: the action again");
        // T36: no status, no tombstone — the producer is Active and produces.
        assert!(matches!(c.s.bond(&producer).unwrap().status, PalwBondStatusV2::Active), "S1 never changes a bond's status");
        c.floor_claim(0x5A06);
        println!(
            "S1: strikes {:?}; the third strike's action {:.4} MSK on C₀ {:.2} MSK",
            c.s.withholding_strikes(&producer).unwrap(),
            msk_of(action),
            msk_of(u128::from(c0))
        );
    }
}

/// **T22 / T02 (S0′): the second failed panel forfeits `w + esc + rr` and nothing else** — no strike,
/// no action tier, no record, no reward. A floor claim's first panel is silent (RT#1: redraw, S0: no
/// charge); its second panel is silent too: the claim voids `ReceiptTimeout` and the producer loses
/// exactly its commitment; no `withholding_strikes` entry and no consumed offence appear. The same on
/// the fence-off twin.
#[test]
fn t22_s0_prime_the_second_failed_panel_forfeits_the_commitment_and_nothing_else() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        let (producer, _, _) = floor_producer(&c.p);
        let id = bound_floor_claim(&mut c, 0x5B01);
        let full = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&id)).unwrap();
        let before = collateral(&c, &producer);
        let recorded = offences(&c.s).len();
        // RT#1: the first panel's receipt window closes — redraw, no charge.
        let at = c.s.deadline_of(&id).expect("the receipt deadline");
        run_to(&mut c, at + 1);
        assert!(c.claim(&id).rebound_daa.is_some(), "armed={armed}: RT#1 redraws");
        assert_eq!(collateral(&c, &producer), before, "armed={armed}: S0 charges nothing");
        let seats = c.floor_seats();
        c.bind(id, &seats);
        let at = c.s.deadline_of(&id).expect("the second receipt deadline");
        run_to(&mut c, at + 1);
        assert!(
            matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ReceiptTimeout, .. }),
            "armed={armed}: RT#2 voids"
        );
        assert_eq!(u128::from(before - collateral(&c, &producer)), full, "armed={armed}: S0′ is the commitment exactly");
        assert_eq!(c.s.withholding_strikes(&producer), None, "armed={armed}: S0′ writes no strike");
        assert_eq!(offences(&c.s).len(), recorded, "armed={armed}: and no record");
    }
}

/// A standalone `ExecutorEquivocation` (kind 0) against `bond`, its certificate's job context naming
/// `class` — the fold reads the class from it; the acceptance layer verified the certificate.
fn equivocation(bond: PalwBondKeyV2, class: Hash64) -> PalwConsensusObjectV2 {
    let floor = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
        .expect("the floor profile");
    let mut job_context = kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor, 512, 256);
    job_context.shape_profile_id = class;
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: h(0x1),
        job_context_hash: h(0x2),
        full_logits_trace_root: h(root),
        committed_root: h(root),
        bond_outpoint: bond.0,
        signature: Vec::new(),
    };
    let carriage = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: bond.0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context,
            attestation_a: attestation(0xAA),
            attestation_b: attestation(0xBB),
        },
    };
    let evidence = borsh::to_vec(&carriage).unwrap();
    PalwConsensusObjectV2::ObjectiveOffence {
        kind: PalwOffenceKindV1::ExecutorEquivocation,
        accused: bond,
        evidence_id: palw_offence_evidence_digest_v1(&evidence),
        evidence,
    }
}

/// **T76 (D6, t12 only): Eq takes `min(C₀, 3 · G_eq)`, `G_eq = palw_eq_cap_basis_v1` of the class the
/// certificate names (the base class for an unknown one); two genesis Eq slashes, then licensing
/// continues.** Two genesis seats equivocate — one on the floor's job, one on the 2M class's — in a
/// block at testnet-12's subsidy: each loses exactly `min(C₀, 3 G_eq)` of its class (the function,
/// asserted; the genesis values printed beside ADR §3.6's 9,602.89 / 190,797.47 MSK and pinned by
/// their ratio to C₀), stays `Active`, and its record is kind 0 with `amount` the nominal cap,
/// `collected` the debit, `claim_id` and root zero. A certificate naming no registered class is priced
/// on the floor. The two seats then sit on a floor panel that licenses. The fence-off twin: Eq takes
/// the whole collateral, recorded as before (`collected` 0, no claim).
#[test]
fn t76_eq_takes_min_c_3_g_eq_and_licensing_continues() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let (_, id2m) = model_classes(&p);
        let mut c = Chain::new(p);
        let (floor, _, _, _) = genesis_classes(&c.p)[0];
        let seats = c.floor_seats();
        let (first, second) = (seats[0].0, seats[1].0);
        let daa = c.daa + 1;
        let x = ctx(0xCA_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI);
        let e = c.extras_at(daa);
        let g_floor = palw_eq_cap_basis_v1(&c.s, &c.sp, &e, &x, &floor);
        let g_2m = palw_eq_cap_basis_v1(&c.s, &c.sp, &e, &x, &id2m);
        assert_eq!(palw_eq_cap_basis_v1(&c.s, &c.sp, &e, &x, &h(0xDEAD_C1A5)), g_floor, "an unknown class prices the base class");
        let (c1, c2) = (collateral(&c, &first), collateral(&c, &second));
        c.step_at(daa, &[equivocation(first, floor), equivocation(second, id2m)], PalwBlockWorkV3::None, Hash64::default(), T12_BLOCK_SUBSIDY_SOMPI);
        for (bond, before, g, class) in [(first, c1, g_floor, floor), (second, c2, g_2m, id2m)] {
            let debit = u128::from(before - collateral(&c, &bond));
            let PalwConsensusObjectV2::ObjectiveOffence { evidence_id, .. } = equivocation(bond, class) else { unreachable!() };
            let row = c
                .s
                .consumed_offence(&palw_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &bond.0, &evidence_id))
                .expect("one kind-0 record")
                .clone();
            assert!(matches!(c.s.bond(&bond).unwrap().status, PalwBondStatusV2::Active), "armed={armed}: no status change");
            assert_eq!((row.claim_id, row.execution_root), (Hash64::default(), Hash64::default()), "a key, no claim, no root");
            if armed {
                let cap = palw_rcore_eq_cap_v1(before, g);
                assert_eq!(debit, cap, "min(C₀, 3·G_eq)");
                assert_eq!((u128::from(row.amount), u128::from(row.collected)), (cap, cap), "nominal, and collected: the bond's gate is shut");
                assert!(cap < u128::from(before), "a genesis bond keeps most of its stake");
                println!(
                    "T76 {}: G_eq {:.4} MSK, 3·G_eq {:.4} MSK (ADR §3.6: {}); C₀ {:.2} MSK",
                    if class == floor { "floor" } else { "2M" },
                    msk_of(g),
                    msk_of(3 * g),
                    if class == floor { "9,602.89" } else { "190,797.47" },
                    msk_of(u128::from(before))
                );
            } else {
                assert_eq!(debit, u128::from(before), "fence off: the whole collateral");
                assert_eq!((u128::from(row.amount), row.collected), (u128::from(before), 0), "fence off: the record as before");
            }
        }
        if armed {
            // The genesis values, pinned: G_eq(floor) = 3,200.96402740 MSK and G_eq(2M) =
            // 63,599.15856900 MSK — ADR §3.6's 9,602.89 and 190,797.47 MSK at 3·G_eq (the v3 script's,
            // to the cent: 9,602.892 and 190,797.476).
            assert_eq!((g_floor, g_2m), (320_096_402_740, 6_359_915_856_900), "the genesis G_eq");
            assert!((msk_of(3 * g_floor) - 9_602.89).abs() < 0.01 && (msk_of(3 * g_2m) - 190_797.47).abs() < 0.01);
            // Licensing continues: both equivocators sit on a floor panel that licenses.
            let id = bound_floor_claim(&mut c, 0x76A1);
            let bound = c.daa;
            c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
            assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the panel with both equivocators licenses");
            for bond in [first, second] {
                assert!(c.s.slashable_lock(bond, id).is_some(), "each equivocator backs its Valid");
            }
        }
    }
}

/// **R-2 (T39's funnel half): `collected` is 0 for a bond whose withdrawal gate is OPEN at the
/// conviction — the conviction still debits it, and records the nominal tier.** A registrant asks to
/// retire and waits out its delay and the second clock's bound (nothing else holds it): v6 answers
/// "unlocked" on the pre-state, so an Eq against it collects nothing — its collateral may already
/// have left through the UTXO layer, where no burn can reach it — while the debit and `amount` are the
/// cap. The same bond convicted while still locked collects the whole debit (the twin case).
#[test]
fn r2_a_bond_whose_gate_is_open_collects_nothing() {
    let mut c = Chain::new(t12());
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let (open, shut) = (bond_key(1), bond_key(2));
    c.step(&[bond_obj(1, 20_000 * MSK), bond_obj(2, 20_000 * MSK)]);
    c.step(&[PalwConsensusObjectV2::BondRetireRequested { bond: open, signature: Vec::new() }]);
    let since = c.daa;
    // The withdrawal delay (the DA lattice included) plus the second clock's bound past it.
    let released = since + c.sp.withdrawal_delay_daa() + 2 * c.sp.window_court() + 1;
    run_to(&mut c, released);
    let at = c.daa + 1;
    let raw = if c.p.palw_audit_2026_09_23_active_at(at) { c.p.palw_settled_anchor_depth } else { None };
    let gate = |bond: &PalwBondKeyV2| {
        palw_bond_collateral_is_locked_v6(&c.s, &c.sp, bond, c.s.bond(bond).unwrap(), at, c.sp.withdrawal_delay_daa(), raw, true)
    };
    assert!(!gate(&open), "the premise: the retiring bond's gate is open");
    assert!(gate(&shut), "the premise: the active one's is shut");
    let before = [collateral(&c, &open), collateral(&c, &shut)];
    c.step_at(at, &[equivocation(open, floor), equivocation(shut, floor)], PalwBlockWorkV3::None, Hash64::default(), T12_BLOCK_SUBSIDY_SOMPI);
    for (i, bond) in [open, shut].into_iter().enumerate() {
        let debit = u128::from(before[i] - collateral(&c, &bond));
        assert!(debit > 0, "both are debited");
        let PalwConsensusObjectV2::ObjectiveOffence { evidence_id, .. } = equivocation(bond, floor) else { unreachable!() };
        let row = c.s.consumed_offence(&palw_offence_id_v1(PalwOffenceKindV1::ExecutorEquivocation, &bond.0, &evidence_id)).unwrap();
        assert_eq!(u128::from(row.amount), debit, "the nominal cap (a 20,000 MSK bond pays it whole)");
        assert_eq!(u128::from(row.collected), if i == 0 { 0 } else { debit }, "collected: 0 where the gate was open");
    }
}

/// **R-2: a pre-drained bond's `collected` is its real remainder, not the nominal tier.** A DA default
/// (S1) against a producer whose collateral was drained below its claim's commitment: the record's
/// `amount` is the commitment, `collected` exactly what was left.
#[test]
fn r2_a_pre_drained_bond_collects_its_remainder() {
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    let (producer, _, _) = floor_producer(&c.p);
    let id = bound_floor_claim(&mut c, 0x5C01);
    let full = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&id)).unwrap();
    c.step(&[accuse(id, bond_key(1), 0)]);
    let remainder = u64::try_from(full / 3).unwrap();
    c.s = edited(&c.sp, &c.s, |k| k.bonds.get_mut(&producer).expect("the producer").collateral = remainder);
    let deadline = c.s.da_session(&id, &bond_key(1)).unwrap().deadline_daa;
    run_to(&mut c, deadline + 1);
    assert_eq!(collateral(&c, &producer), 0, "the bond pays what it has");
    let row = c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, &id)).unwrap();
    assert_eq!((u128::from(row.amount), row.collected), (full, remainder), "nominal the commitment, collected the remainder");
}

/// **T81 (the DA half) / T22 (S3): a DA default after `Final` writes a `DaDefault` record with
/// `claim_id` and `collected`, reverses the `Final` as the withholding, and charges S3 only through
/// the burn hook.** A bystander's session outlives the claim's challenge window; the claim is `Final`
/// with the vesting row the vesting work wrote at `Final`, and defaults at `FinalRow`. The producer's S3
/// `min(25% · C₀, 3 G)` is charged because `burn_vesting_row` burned the row (the vesting work's body,
/// with `PALW_RCORE_VESTING_ROWS_LANDED_V1`), and the record says so (`amount` = `collected` = the S3
/// actually charged). Rights are forfeited by claim; nothing strikes after `Final`.
#[test]
fn t81_a_post_final_da_default_records_its_claim_and_collected_and_s3_rides_the_burn_hook() {
    let mut c = Chain::new(t12());
    let (producer, _, _) = floor_producer(&c.p);
    let id = c.floor_claim(0x81A1);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
    c.step(&[bond_obj(1, 20_000 * MSK)]);
    c.step(&[accuse(id, bond_key(1), 0)]);
    c.finalize(id);
    let PalwClaimPhaseV2::Final { final_daa } = c.claim(&id).phase else { panic!("Final") };
    // The vesting work wrote the claim's row at `Final` (integration: the row writer is on this line,
    // so the test reads the real row instead of writing one through the carriage).
    let row = c.s.vesting_row(&id).expect("the vesting work's row, written at Final").clone();
    assert_eq!((row.producer_bond, row.final_daa), (producer, final_daa), "the claim's own row");
    let before = collateral(&c, &producer);
    let g = g_of(&c, &id);
    let deadline = c.s.da_session(&id, &bond_key(1)).expect("the session outlived the Final").deadline_daa;
    run_to(&mut c, deadline + 1);
    let closed = c.daa;
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, voided_daa } if voided_daa == closed),
        "the Final is reversed as the withholding"
    );
    let debit = u128::from(before - collateral(&c, &producer));
    let s3 = palw_rcore_s3s4_action_v1(before, g);
    assert!(PALW_RCORE_VESTING_ROWS_LANDED_V1, "the vesting rows are on this line");
    assert!(c.s.vesting_row(&id).is_none(), "the row is burned: S3's marker");
    assert_eq!(debit, s3, "S3: the row burned, the producer charged min(25% · C₀, 3 G)");
    let row = c.s.consumed_offence(&palw_da_offence_id_v1(&producer.0, &id)).expect("the DaDefault record").clone();
    assert_eq!(
        (row.kind, u128::from(row.amount), u128::from(row.collected), row.claim_id, row.execution_root, row.accepted_daa),
        (PalwOffenceKindV1::DaDefault, debit, debit, id, Hash64::default(), closed),
        "kind 5, the claim, what was charged and collected, root 0 (by claim)"
    );
    assert_eq!(c.s.withholding_strikes(&producer), None, "no strike after Final: S3 is its tier");
    println!("T81: S3 would be {:.4} MSK (G {:.4} MSK); charged {:.4} MSK", msk_of(s3), msk_of(g), msk_of(debit));
}

/// **S2 for a court DEFAULT, and no record: charged exactly as a proven fraud, recorded as the default
/// it is.** A bystander's court on a licensed floor claim runs out on the responder's silence: past
/// `palw_offence_attribution` the claim voids `CourtDefault`, the producer forfeits its commitment
/// plus S2's `min(10% · C₀, 3 G)` plus the court time — the user-approved addition to ADR §3.6's S2
/// table (2026-09-24, deviation 2: silence must not be cheaper than losing) — and NO
/// `CourtConviction` (kind 6) is written: that record is a proven verdict's (the user's decision).
/// The lib test `deviation_2_a_court_default_pays_the_losing_charge_with_no_record_and_no_reward`
/// pins the equality with a lost verdict at the same DAA and that no reward opens. The fence-off
/// twin: the same void, no action.
#[test]
fn s2_a_court_default_is_charged_as_a_fraud_and_writes_no_court_conviction() {
    for armed in [true, false] {
        let p = if armed { t12() } else { twin(&t12()) };
        let mut c = Chain::new(p);
        c.attribution = true;
        c.step(&[bond_obj(2, 20_000 * MSK)]);
        let (producer, _, _) = floor_producer(&c.p);
        let id = c.floor_claim(0x5D01);
        let seats = c.floor_seats();
        let bound = c.bind(id, &seats);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
        let claim = c.claim(&id);
        let at = c.daa;
        c.s = edited(&c.sp, &c.s, |carriage| {
            let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
                &id,
                &claim.trace_root,
                &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&bond_key(2)),
                &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&claim.bond),
                kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
                16,
                at,
                at + 50,
            )
            .expect("a ladder opens");
            carriage.court_sessions.insert(
                ladder.session_id(),
                kaspa_consensus_core::palw_state_v2::PalwCourtSessionStateV2 {
                    claim: id,
                    challenger_bond: bond_key(2),
                    opened_daa: at,
                    deadline_daa: at + c.sp.window_court(),
                    ladder,
                    dissection: None,
                },
            );
            if !armed {
                // Below `palw_rcore_plus` a court's stake is the claim's `reserved` on the challenger's
                // `reserved_exposure` (the opening writes it; A-6 moves it to the accuser ledger past).
                *carriage.reserved_exposure.entry(bond_key(2)).or_insert(0) += claim.reserved;
            }
        });
        let before = collateral(&c, &producer);
        let full = palw_claim_bond_reservation_v1(&c.sp, &claim).unwrap();
        let mut guard = 0;
        while !matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { .. }) {
            c.step(&[]);
            guard += 1;
            assert!(guard < 4_000, "the court ends");
        }
        let voided_at = c.daa;
        assert!(
            matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtDefault, .. }),
            "armed={armed}: the responder's silence is a court default: {:?}",
            c.claim(&id).phase
        );
        let window = u128::from(c.sp.window_court().max(1));
        let court_time = claim.reserved * u128::from((voided_at - at).min(c.sp.window_court())) / window;
        let action = if armed { palw_rcore_s1s2_action_v1(before, g_of(&c, &id)) } else { 0 };
        assert_eq!(
            u128::from(before - collateral(&c, &producer)),
            full + action + court_time,
            "armed={armed}: the forfeit, S2's action ({:.4} MSK) and the court time",
            msk_of(action)
        );
        let kind6 = kaspa_consensus_core::palw_state_v2::palw_court_conviction_offence_id_v1(&producer.0, &id);
        assert!(c.s.consumed_offence(&kind6).is_none(), "armed={armed}: a default writes no CourtConviction");
    }
}

/// **The fence-off twin of the whole funnel: below `palw_rcore_plus` a DA-style accusation, a court
/// default and an equivocation leave `withholding_strikes` empty and every consumed record without
/// `collected` or `claim_id`** — the v22 fields S-4 writes are the dormant defaults there (the
/// accumulated assertions of the tests above, restated on one chain as the parity pin).
#[test]
fn fence_off_the_funnel_writes_no_s4_field() {
    let mut c = Chain::new(twin(&t12()));
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let seats = c.floor_seats();
    c.step_at(c.daa + 1, &[equivocation(seats[0].0, floor)], PalwBlockWorkV3::None, Hash64::default(), T12_BLOCK_SUBSIDY_SOMPI);
    let rows = offences(&c.s);
    assert!(!rows.is_empty());
    for (_, row) in rows {
        assert_eq!((row.collected, row.claim_id), (0, Hash64::default()), "dormant: the v22 fields stay default");
    }
    let _ = PalwStateV2Error::DaCourtDormant;
}

// ---------------------------------------------------------------------------------------------
// The S-4 review's G freeze: a conviction reads the gain the licence priced its locks with.
// ---------------------------------------------------------------------------------------------

/// The live inputs `G_res` is read from, moved after the licence: the execution lane's quantum
/// halved and the permit value tenfold (the lane advances; `R` moves) — the conviction block's extras.
fn moved(e: &mut PalwTransitionExtrasV1) {
    if let Some(lane) = e.round_lane.as_mut() {
        lane.execution_quantum = (lane.execution_quantum / 2).max(1);
    }
    if let Some(safety) = e.economic_safety.as_mut() {
        safety.permit_value_sompi = safety.permit_value_sompi.saturating_mul(10).max(1);
    }
}

/// A pair opened on the claim's line after its licence (the market input `s`): the claim's root, its
/// owner row and a seeded market, written through the carriage.
fn open_a_pair(c: &mut Chain, id: Hash64, line: u64) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let root = c.s.class(&floor).expect("the floor").artifact_root;
    let at = c.daa;
    c.s = edited(&c.sp, &c.s, |k| {
        k.claim_roots.insert(id, root);
        k.artifact_owners.insert(
            (floor, root),
            kaspa_consensus_core::palw_model_lines_v1::PalwArtifactOwnerV1 { class_id: floor, line_id: h(line), version: 1 },
        );
        k.model_markets.insert(h(line), kaspa_consensus_core::palw_model_market_v1::PalwModelMarketV1::seed_v1(at, 100_000 * MSK, h(line + 1)));
    });
}

/// The block past `claim`'s session deadline (its default) — with signer liability armed (P2-7's
/// constant overridden, so the covering signers' S4 prices `G`) and the inputs [`moved`] — checked as
/// `Chain::step` checks one: the delta re-applies and reverts, the carriage reloads under its root.
fn run_out_moved(c: &mut Chain, claim: Hash64, accuser: PalwBondKeyV2) -> u64 {
    let deadline = c.s.da_session(&claim, &accuser).expect("an open session").deadline_daa;
    if deadline > c.daa + 1 {
        c.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }
    let daa = deadline + 1;
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let mut e = c.extras_at(daa);
    e.seat_da_answer_landed = true;
    moved(&mut e);
    let parent = c.s.clone();
    let (child, delta, skips) =
        fold_with(&c.p, &c.sp, &parent, &x, &[], PalwBlockWorkV3::None, Hash64::default(), &e).expect("the default's block folds");
    assert!(skips.is_empty());
    assert_eq!(kaspa_consensus_core::palw_state_v2::apply_delta_v2(&parent, &delta, &c.sp).expect("re-applies"), child);
    assert_eq!(kaspa_consensus_core::palw_state_v2::revert_delta_v2(&child, &delta, &c.sp).expect("reverts"), parent, "the delta reverts");
    let reloaded = PalwStateCarriageV2::from_state(&child).into_state(&c.sp, Some(child.state_root())).expect("reloads");
    assert_eq!(reloaded, child, "reload is the state");
    c.s = child;
    c.daa = daa;
    daa
}

/// The live `G_res` a price read now would give, under the [`moved`] extras at the next DAA.
fn live_g_res(c: &Chain, id: &Hash64) -> u128 {
    let at = c.daa + 1;
    let mut e = c.extras_at(at);
    moved(&mut e);
    let claim = c.claim(id);
    kaspa_consensus_core::palw_state_v2::palw_rcore_bind_prices_v1(&c.s, &c.sp, &e, id, &claim, 5, at).g_res
}

/// **The S-4 review's G freeze: every conviction prices its tiers on the `G_res` the licence priced
/// its locks with — pre-`Final`, after `Final` before retirement, and after retirement — whatever
/// moves after the licence.** Two floor claims licensed by a V1 quorum record `G_res` once
/// (`PalwClaimRcoreV1::g_res_sompi`, exactly the value each lock was priced on). Then the inputs move:
/// a pair opens on the claims' line (`s`: 5% of `E`) and the conviction blocks read a halved lane
/// quantum and a tenfold permit value (`R`) — the live gain moves, the frozen one does not.
///
/// * **Pre-`Final`** (claim A): a seat's DA session pauses the licensed claim and defaults; each
///   covering `Valid` signer's S4 is its lock + `min(25% · C₀, 3 G)` on the licence-time `G`
///   (`C₀/4` does not bind on a genesis seat), and the void's liability row copies it.
/// * **After `Final`** (claim B): the `Final` block writes the licence-time `G_res` into the row —
///   not the live value the pair has moved; DA-6's `FinalRow` accusation price reads it
///   (`palw_da_accusation_admissible_v2`); a bystander's session outliving the challenge window
///   defaults at `FinalRow` and the covering signers' S4 reads it too.
/// * **After retirement** the row alone is left, and `palw_claim_g_v1` reads the licence value.
///
/// Every block re-applies, reverts and reloads exactly.
#[test]
fn g_freeze_every_conviction_reads_the_licence_time_gain() {
    use kaspa_consensus_core::palw_state_v2::{palw_claim_g_v1, palw_rcore_lock_v1};
    let mut c = Chain::new(t12());
    c.step(&[bond_obj(1, 20_000 * MSK), bond_obj(2, 20_000 * MSK)]);
    let seats = c.floor_seats();
    let licence = |c: &mut Chain, seed: u64| -> (Hash64, u128) {
        let id = c.floor_claim(seed);
        let bound = c.bind(id, &seats);
        let g_res = licence_g_res(c, &id);
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats[..3].iter().map(|(k, _)| valid(id, *k, bound)).collect() }]);
        let claim = c.claim(&id);
        assert_eq!(claim.rcore.g_res_sompi, g_res, "the licence records the G_res it priced with");
        for (seat, _) in &seats[..3] {
            assert_eq!(
                c.s.slashable_lock(*seat, id).expect("a backed Valid locks").amount,
                palw_rcore_lock_v1(g_res, claim.escrowed_reward, 0, 3),
                "each lock is priced on the recorded G_res (no pair yet: s = 0)"
            );
        }
        (id, g_res)
    };
    let (a, g_a) = licence(&mut c, 0x6A01);
    let (b, g_b) = licence(&mut c, 0x6A02);
    // The inputs move.
    open_a_pair(&mut c, a, 0x6A_1000);
    open_a_pair(&mut c, b, 0x6A_1000);
    let (e_a, e_b) = (c.claim(&a).escrowed_reward, c.claim(&b).escrowed_reward);
    assert!(live_g_res(&c, &a) > g_a && live_g_res(&c, &b) > g_b, "the live gain has moved (the pair's s, the lane's R)");
    assert_eq!(palw_claim_g_v1(&c.s, &a).unwrap().g_res, g_a, "the frozen gain has not");

    // Pre-Final: a seat of A's panel that signed nothing accuses (its session pauses the licensed claim).
    c.step(&[accuse(a, seats[3].0, 0)]);
    let before: Vec<u64> = seats[..3].iter().map(|(k, _)| collateral(&c, k)).collect();
    let locks: Vec<u128> = seats[..3].iter().map(|(k, _)| c.s.slashable_lock(*k, a).unwrap().amount).collect();
    run_out_moved(&mut c, a, seats[3].0);
    assert!(matches!(c.claim(&a).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    let g = g_a + u128::from(e_a);
    for (i, (seat, _)) in seats[..3].iter().enumerate() {
        let s4 = locks[i] + palw_rcore_s3s4_action_v1(before[i], g);
        assert!(u128::from(before[i]) / 4 > 3 * g, "the premise: 3G binds on a genesis seat");
        assert_eq!(u128::from(before[i] - collateral(&c, seat)), s4, "pre-Final: seat {i}'s S4 on the licence-time G");
    }
    assert_eq!(c.s.panel_liability(&a).expect("the void's row").g_res_sompi, g_a, "the row copies the frozen G_res");

    // After Final, before retirement (B's challenge window closed while A's session ran; its Final
    // block read the pair the carriage opened).
    if matches!(c.claim(&b).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
        c.finalize(b);
    }
    assert!(matches!(c.claim(&b).phase, PalwClaimPhaseV2::Final { .. }), "B is Final");
    assert_eq!(c.s.panel_liability(&b).expect("the Final's row").g_res_sompi, g_b, "the Final block copies the licence value, not the live one");
    assert!(live_g_res(&c, &b) > g_b);
    let (producer, _, _) = floor_producer(&c.p);
    // The Final block wrote B's real vesting row (the vesting flag is landed); it is unmatured.
    let row = c.s.vesting_row(&b).expect("the Final block writes the vesting row");
    assert!(row.matured_at.is_none(), "B's row has not matured");
    assert_eq!(row.producer_bond, producer);
    let at = c.daa + 1;
    let mut e = c.extras_at(at);
    moved(&mut e);
    let admission = kaspa_consensus_core::palw_state_v2::palw_da_accusation_admissible_v2(&c.s, &c.sp, &e, &b, &bond_key(2), at)
        .expect("a Final claim with an unmatured row is accusable");
    let full = palw_claim_bond_reservation_v1(&c.sp, &c.claim(&b)).unwrap();
    let producer_c = collateral(&c, &producer);
    let g = g_b + u128::from(e_b);
    assert_eq!(
        admission.exposure,
        kaspa_consensus_core::palw_da_rcore_v1::palw_da_session_exposure_v1(
            kaspa_consensus_core::palw_da_rcore_v1::palw_da_stage_reward_base_v1(
                kaspa_consensus_core::palw_da_rcore_v1::PalwDaStageV1::FinalRow,
                full,
                producer_c,
                g
            ),
            c.sp.min_collateral_sompi()
        ),
        "DA-6's FinalRow price reads the frozen G"
    );
    // A bystander accuses the Final claim (its row unmatured: FinalRow) and the producer is silent.
    c.step(&[accuse(b, bond_key(1), 0)]);
    let before: Vec<u64> = seats[..3].iter().map(|(k, _)| collateral(&c, k)).collect();
    let locks: Vec<u128> = seats[..3].iter().map(|(k, _)| c.s.slashable_lock(*k, b).unwrap().amount).collect();
    run_out_moved(&mut c, b, bond_key(1));
    assert!(matches!(c.claim(&b).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
    for (i, (seat, _)) in seats[..3].iter().enumerate() {
        assert_eq!(
            u128::from(before[i] - collateral(&c, seat)),
            locks[i] + palw_rcore_s3s4_action_v1(before[i], g),
            "post-Final: seat {i}'s S4 on the licence-time G"
        );
    }

    // After retirement: the row alone, and it holds the licence value.
    let retire = c.s.deadline_of(&b).expect("the retirement");
    c.step_at(retire + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.claim(&b).is_none(), "the claim retired");
    let frozen = palw_claim_g_v1(&c.s, &b).expect("the row outlives the claim");
    assert_eq!((frozen.g_res, frozen.escrowed_reward), (g_b, e_b), "post-retirement: the licence-time G");
}
