//! **ADR-0152 §4-quater: class-derived verification deadlines — on testnet-12's own fold.**
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves and is
//! checked three ways (`Chain::step`): its delta re-applies to the child and reverts to the parent,
//! and the child's carriage reloads under its committed root (`into_state`: DL-1's deadlines exactly)
//! — so every phase below is also a restart. Each test runs beside its fence-off twin (testnet-12 with
//! `palw_class_verify_deadline = None`), where the pre-fence rules stand byte for byte.
//!
//! The DA-close re-arm and V6 are pinned at the builder in `palw_state_v2`'s
//! `class_verify_deadline` unit module: a refuted DA session needs a real answer, which only `t46`'s
//! real-claim harness produces.
//!
//! Run: cargo test -p kaspa-consensus-core --test t12_class_verify_deadline

#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_class_verify_deadline_v1::{PalwClaimVerifyShapeV1, palw_derived_verify_daa_v1};
use kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwStateV2Error, palw_class_fp_verify_daa_v1, palw_class_needs_measured_row_v1, palw_class_row_is_long_d_v1,
    palw_class_verify_deadline_v1, palw_v2_apply_one_object_v1,
};

/// testnet-12 with `palw_class_verify_deadline = None` (mirror re-synced) — the fence-off twin.
fn off(p: &Params) -> Params {
    let mut t = p.clone();
    t.palw_class_verify_deadline = None;
    t.sync_palw_class_verify_deadline();
    t
}

/// The `verification_ccu` whose derived deadline is `d` (a multiple of 5).
fn ccu_for(d: u64) -> u128 {
    let ccu = u128::from(d / 5 - 1) * 1_200_000_000_000;
    assert_eq!(palw_derived_verify_daa_v1(ccu), d);
    ccu
}

/// A chain whose short-window row (the "8k" row) is re-registered, through the carriage, with a
/// canonical job that derives `d` DAA — the fixture class of §4-quater T-D4 — made `Active` and
/// readied, with two rich producers.
fn fixture_chain(p: Params, d: u64) -> (Chain, Hash64) {
    let (short, _) = model_classes(&p);
    let mut c = model_chain(p, short, 2);
    c.s = edited(&c.sp, &c.s, |carriage| {
        let row = carriage.model_lifecycles.get_mut(&short).expect("the short row");
        row.work.verification_ccu = ccu_for(d);
        row.profile.verification_window_spans = (d / 5) as u32;
    });
    (c, short)
}

/// **T-D1: testnet-12's genesis rows derive their deadlines in reference spans.** Every genesis model
/// row's `D` is `5 × verification_window_spans` — the short row 15, the 2M row 13,995 (its 2,799
/// spans), and no other row past 65 — every admissible class's receipt window is the global 600, the
/// NM set is exactly the 2M row, and no admissible genesis class is long-D (the floor has no registry
/// row, so no class-derived term). The 8k row's largest free-prompt run (its published profile at
/// `n_ctx` 8,192) derives 75, inside the short window and far inside 600. Below the fence the
/// 2M row's receipt window is §11.3's 2,799 — the unit bug, a fifth of the derivation.
#[test]
fn td1_testnet12s_genesis_rows_derive_their_deadlines_in_reference_spans() {
    let p = t12();
    let (sp, s) = (bundle(&p).state, genesis_state(&p));
    let (short, id2m) = model_classes(&p);
    let a = PalwClaimVerifyShapeV1::Attempt;
    let mut nm = Vec::new();
    for (class, _, _, _) in genesis_classes(&p) {
        let Some(row) = s.model_lifecycle(&class) else { continue };
        let d = sp.claim_verify_daa_v1(&s, &class, a, 0);
        let w = sp.receipt_window_for_claim_v1(&s, &class, a, 0);
        println!("class {class}: {} spans, D {d}, W_r {w}", row.profile.verification_window_spans);
        assert_eq!(d, 5 * u64::from(row.profile.verification_window_spans), "D is the window in 5-DAA spans");
        if palw_class_needs_measured_row_v1(&sp, &s, &class) {
            nm.push(class);
        } else {
            assert!(d <= 65, "a genesis class other than 2M derives at most 65: {d}");
            assert_eq!(w, 600, "an admissible class keeps the global receipt window");
            assert!(!palw_class_row_is_long_d_v1(row), "no admissible genesis class is long-D");
        }
    }
    assert_eq!(nm, vec![id2m], "the NM set at genesis is the 2M row alone (U-D2)");
    assert_eq!(sp.claim_verify_daa_v1(&s, &short, a, 0), 15, "the 8k row");
    assert_eq!((sp.claim_verify_daa_v1(&s, &id2m, a, 0), sp.receipt_window_for_claim_v1(&s, &id2m, a, 0)), (13_995, 13_995));
    let twin = bundle(&off(&p)).state;
    assert_eq!(twin.receipt_window_for_claim_v1(&s, &id2m, a, 0), 2_799, "below the fence: 2,799 spans × the 1-DAA lane span");
    assert_eq!(twin.receipt_window_for_claim_v1(&s, &short, a, 0), 600);
    // The 8k row's largest free-prompt run, from its genesis registration's profile published as the
    // lane certification publishes it.
    let profile = bundle(&p)
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } if *class_id == short => {
                Some(carriage.profile.clone())
            }
            _ => None,
        })
        .expect("the 8k row registers with its profile");
    assert_eq!(profile.n_ctx, 8_192, "the premise: the 8k row");
    let published = edited(&sp, &s, |carriage| {
        carriage.fp_work_profiles.insert(short, Box::new(profile));
    });
    let fp_d = palw_class_fp_verify_daa_v1(&published, &short).expect("the 8k profile derives");
    println!("8k free-prompt D (largest run, n_ctx 8,192): {fp_d}");
    // The golden: 75 DAA (15 spans) — the chain's own economic compute of 8,192 positions' body and
    // logits. §4-quater.4's knee model ESTIMATED 65 (it scaled the canonical job and left out the
    // decode positions' logits); either way past the canonical job's 15 and inside the short window.
    assert_eq!(fp_d, 75, "the 8k row's largest free-prompt run");
    assert!(!palw_class_needs_measured_row_v1(&sp, &published, &short), "the 8k row's 8,192 is not past 8,192");
    let fp = PalwClaimVerifyShapeV1::FreePrompt { work_leaves: 1 };
    assert_eq!(sp.claim_verify_daa_v1(&published, &short, fp, 0), fp_d);
    assert_eq!(sp.receipt_window_for_claim_v1(&published, &short, fp, 0), 600);
}

/// **T-D2: the 2M row is closed at launch — attempt and free prompt, by name, by the fold and by the
/// outside reader alike (U-D1).** On an `Active`, readied 2M row the producer's gate
/// (`palw_class_admits_claim_v1`), the fold's attempt lane and the fold's free-prompt lane all refuse
/// with `ClassDeadlineUnmeasured { derived 13,995, open 600 }`; the short row still takes an attempt.
/// The twin takes the same 2M attempt and commitment, as testnet-12 did before the fence.
#[test]
fn td2_the_2m_row_is_refused_at_launch_attempt_and_free_prompt() {
    for armed in [true, false] {
        let p = if armed { t12() } else { off(&t12()) };
        let (_, id2m) = model_classes(&p);
        let mut c = model_chain(p, id2m, 2);
        c.s = readied(&c.sp, &c.s, &honest(&c.p), id2m, c.daa);
        let unmeasured = |r: Result<(), &PalwStateV2Error>| matches!(r, Err(PalwStateV2Error::ClassDeadlineUnmeasured { class, derived_daa: 13_995, open_daa: 600 }) if *class == id2m);
        // The outside reader (the producer's facts, op 186's gate).
        let gated = gate(&c.p, &c.sp, &c.s, id2m, c.daa + 1);
        assert_eq!(unmeasured(gated.as_ref().map(|_| ())), armed, "armed {armed}: the gate: {gated:?}");
        // The fold's attempt lane.
        let pwu = class_pwu(&c.p, &c.s, id2m, c.daa + 1);
        let (env, key, id) = junk_attempt(id2m, bond_key(1), pubkey_of(1), &operator_pubkey_of(1), pwu, 0x2D0, 0x5_02D0);
        let x = ctx(0xCA_0000 + c.daa + 1, c.daa + 1, c.daa + 1, T12_BLOCK_SUBSIDY_SOMPI);
        let folded = c.try_fold(&c.s, &x, &[], PalwBlockWorkV3::Attempt(&env), key);
        if armed {
            assert!(unmeasured(folded.as_ref().map(|_| ())), "the fold refuses the 2M attempt by name: {:?}", folded.as_ref().err());
        } else {
            assert!(folded.as_ref().is_ok_and(|(child, _, _)| child.claim(&id).is_some()), "the twin takes it: {:?}", folded.err());
        }
        // The fold's free-prompt lane (priced in leaves, as `panel_room_c7_hold_agrees_with_the_fold_gate`
        // prices it, so the lane's own derivation is not what answers).
        let per_job = c.sp.fp_quanta_per_canonical_job() as u64;
        let job = genesis_classes(&c.p).iter().find(|g| g.0 == id2m).expect("the 2M row").1;
        let commit = PalwConsensusObjectV2::FreePromptCommitted {
            job_pin: Hash64::default(),
            claim: h(0xF6_2D01),
            class_id: id2m,
            bond: bond_key(2),
            executor_pubkey: pubkey_of(2),
            work_leaves: (job / per_job).max(1),
            prompt_token_ids_hash: h(0x7E_2D01),
            prompt_tokens: 0,
            prompt_token_ids: Vec::new(),
            decode_tokens_executed: 1,
            trace_root: h(0x1F2D_0001),
            output_root: h(0x2F2D_0001),
            execution_root: h(0x3F2D_0001),
            trace_chunk_count: 1,
            trace_retention_daa: 9_999_999,
            consumed_prefix_state: PalwFpPrefixStateV1::genesis(id2m),
        };
        let mut e = c.extras_at(c.daa + 1);
        e.canonical_work_daa = None;
        let f = flags(&c.p, c.daa + 1);
        let committed = palw_v2_apply_one_object_v1(
            &c.s,
            &c.sp,
            &x,
            &commit,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &e,
        );
        if armed {
            assert!(
                unmeasured(committed.as_ref().map(|_| ())),
                "the free-prompt lane refuses it by name: {:?}",
                committed.as_ref().err()
            );
        } else {
            assert!(committed.is_ok(), "the twin takes the commitment: {:?}", committed.err());
        }
    }
    // The short row is not NM: it takes an attempt as before.
    let p = t12();
    let (short, _) = model_classes(&p);
    let mut c = model_chain(p, short, 1);
    let id = model_claim(&mut c, short, 1, 0x2D1);
    assert!(c.s.claim(&id).is_some());
}

/// One block at `daa` with `class`'s ready seats re-proved at `daa - 1` (testnet-12's readiness ages
/// out in a few one-DAA spans, and a row whose seats age out is `Held` at the next boundary).
fn ready_step(c: &mut Chain, class: Hash64, daa: u64, objects: &[PalwConsensusObjectV2]) {
    c.s = readied(&c.sp, &c.s, &honest(&c.p), class, daa - 1);
    c.step_at(daa, objects, PalwBlockWorkV3::None, Hash64::default(), 0);
}

/// **T-D4 (i), T-D5, T-D10 on the fold: a long-D claim Finals past its horizon, its seat locks live
/// to the horizon plus the court window, and its class is held to its cap until Final.** A D-400
/// fixture class; a claim bound at `B` and licensed at `B + 11`: DL-1's deadline is `H = B + 401`
/// from the licence on (every block reloading with it); it is still licensed at `L + 121`, at `B + 400`
/// and at `H`, and the sweep Finals it one block past its deadline, at `B + 402`. Each seat's lock at
/// licence expires at `H + 3,000`. While it is licensed the class (long-D, K-1) owes the room and a
/// second claim is refused at the cap (1, its Little's law over 80 spans); after Final the class takes
/// one again. The twin's deadline is `L + 120` (Final at `L + 121`), its locks are dated from `L`, and
/// it releases the room at licence.
#[test]
fn td4_a_long_d_claim_finals_past_its_horizon_and_holds_its_room() {
    for armed in [true, false] {
        let p = if armed { t12() } else { off(&t12()) };
        let (mut c, class) = fixture_chain(p, 400);
        let id = model_claim(&mut c, class, 1, 0x400);
        let seats = honest_seats(&c.p, 5);
        let b = c.daa + 1;
        ready_step(
            &mut c,
            class,
            b,
            &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + b), seats: seats_of(&seats) }],
        );
        let read = palw_class_verify_deadline_v1(&c.s, &c.sp, &id, &c.claim(&id)).expect("a bound claim");
        assert_eq!(read.verify_daa, if armed { 400 } else { 80 }, "armed {armed}: D (below the fence §11.3's product)");
        assert_eq!(read.receipt_window_daa, 600);
        assert_eq!(read.horizon_daa, armed.then_some(b + 401), "armed {armed}: H");
        assert_eq!(c.s.deadline_of(&id), Some(b + 600), "the receipt deadline is the global window's");
        ready_step(&mut c, class, b + 10, &[]);
        let l = b + 11;
        ready_step(
            &mut c,
            class,
            l,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, b)).collect() }],
        );
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } if licensed_daa == l));
        let h = b + 401;
        let deadline = if armed { h } else { l + 120 };
        assert_eq!(c.s.deadline_of(&id), Some(deadline), "armed {armed}: DL-1 arms the floor");
        for (seat, _) in &seats {
            let lock = c.s.slashable_lock(*seat, id).expect("each Valid signer is locked");
            assert_eq!(lock.expiry_daa, if armed { h } else { l } + c.sp.window_court(), "armed {armed}: V5");
        }
        // K-1: the class owes the licensed claim; a second claim is refused at the cap while it lives.
        let second = gate(&c.p, &c.sp, &c.s, class, c.daa + 1);
        if armed {
            assert!(
                matches!(second, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
                "held to its cap: {second:?}"
            );
        } else {
            assert!(second.is_ok(), "the twin releases the room at licence: {second:?}");
        }
        ready_step(&mut c, class, l + 121, &[]);
        if !armed {
            assert!(
                matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == l + 121),
                "the twin Finals at L + 121"
            );
            continue;
        }
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not Final at L + 121: its deadline is H");
        for at in [h - 1, h] {
            ready_step(&mut c, class, at, &[]);
            assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "still licensed at {at}");
            assert!(gate(&c.p, &c.sp, &c.s, class, at + 1).is_err(), "and still owed at {at}");
        }
        ready_step(&mut c, class, h + 1, &[]);
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == h + 1), "Final one past H");
        c.s = readied(&c.sp, &c.s, &honest(&c.p), class, c.daa);
        assert!(gate(&c.p, &c.sp, &c.s, class, c.daa + 1).is_ok(), "after Final the class takes a claim again");
    }
}

/// **T-D4 (ii): a court that closes on a long-D licensed claim re-arms at the horizon, not at
/// `L + 120`.** The D-400 claim licensed at `B + 1`; a court opened on it (through the carriage, as
/// `rcore_m3_da_court`'s M1 opens one) holds the claim with no deadline; its backstop passes first
/// and the session closes challenger-side (`rearm_after_challenger_side_close` →
/// `rearm_claim_after_court_close`, one of the five Final-floor sites). The claim is re-armed at `B + 401`, still licensed at `L + 121` and at
/// `B + 401`, and Finals one block later. The twin re-arms at `max(L + 120, close)`.
#[test]
fn td4_a_court_close_rearms_a_long_d_claim_at_its_horizon() {
    for armed in [true, false] {
        let p = if armed { t12() } else { off(&t12()) };
        let (mut c, class) = fixture_chain(p, 400);
        let id = model_claim(&mut c, class, 1, 0x401);
        let seats = honest_seats(&c.p, 5);
        let b = c.daa + 1;
        ready_step(
            &mut c,
            class,
            b,
            &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + b), seats: seats_of(&seats) }],
        );
        let l = b + 1;
        ready_step(
            &mut c,
            class,
            l,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, b)).collect() }],
        );
        let claim = c.claim(&id);
        let at = c.daa;
        c.s = edited(&c.sp, &c.s, |carriage| {
            let challenger = bond_key(2);
            let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
                &id,
                &claim.trace_root,
                &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&challenger),
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
                    challenger_bond: challenger,
                    opened_daa: at,
                    // The backstop before the rung's clock: the session closes challenger-side
                    // (prosecution is the challenger's burden), which convicts nobody.
                    deadline_daa: at + 10,
                    ladder,
                    dissection: None,
                },
            );
        });
        assert_eq!(c.s.deadline_of(&id), None, "armed {armed}: a court holds the licensed claim");
        // Walk past the backstop one block at a time until the session is gone.
        let mut closed_at = None;
        for daa in c.daa + 1..=at + 60 {
            ready_step(&mut c, class, daa, &[]);
            if c.s.deadline_of(&id).is_some() || c.claim(&id).phase.is_terminal() {
                closed_at = Some(daa);
                break;
            }
        }
        let closed_at = closed_at.expect("the backstop closes the session");
        let phase = c.claim(&id).phase.clone();
        assert!(matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "armed {armed}: the claim stays licensed: {phase:?}");
        let floor = if armed { b + 401 } else { l + 120 };
        assert_eq!(c.s.deadline_of(&id), Some(floor.max(closed_at)), "armed {armed}: re-armed at the floor");
        if armed {
            ready_step(&mut c, class, l + 121, &[]);
            assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "not Final at L + 121");
            ready_step(&mut c, class, b + 402, &[]);
            assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { .. }), "Final one past the horizon");
        }
    }
}

/// **A short class behaves as it did (the other genesis classes):** the 8k row's attempt, bound,
/// licensed at `B + 21` — past its `H = B + 16` — owes `L + 120` and is Final at `L + 121` exactly as on
/// the twin, and the two folds reach the same state root: nothing the fence writes differs for it.
#[test]
fn a_short_class_finals_at_l_plus_121_as_before() {
    let mut roots = Vec::new();
    for armed in [true, false] {
        let p = if armed { t12() } else { off(&t12()) };
        let (short, _) = model_classes(&p);
        let mut c = model_chain(p, short, 1);
        let id = model_claim(&mut c, short, 1, 0x15);
        let seats = honest_seats(&c.p, 5);
        let b = c.daa + 1;
        ready_step(
            &mut c,
            short,
            b,
            &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC_0000 + b), seats: seats_of(&seats) }],
        );
        ready_step(&mut c, short, b + 20, &[]);
        let l = b + 21;
        ready_step(
            &mut c,
            short,
            l,
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|(k, _)| valid(id, *k, b)).collect() }],
        );
        assert_eq!(c.s.deadline_of(&id), Some(l + 120), "armed {armed}");
        ready_step(&mut c, short, l + 121, &[]);
        assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == l + 121), "armed {armed}");
        roots.push(c.s.state_root());
    }
    assert_eq!(roots[0], roots[1], "a short class licensed past its horizon folds to the same root with the fence on and off");
}

/// **§4-quater.3: every timer keeps its disposition.** The fence moves only the C-kind deadline and
/// what floors on it: the R-kind and E-kind globals — bind 600, receipt 600, challenge 1,200 (120
/// from genesis), court 3,000, the 42-DAA turn, `W_disclose` 1,200, retirement 3,000, the free-prompt
/// abandon hold 600 — are the twin's byte for byte, and so is the pruning depth (P-1 is the regenesis
/// params', not this fence's). What moves is per claim: the 2M row's receipt window (2,799 → 13,995,
/// V3) and its `D`; an admissible class's receipt window stays 600.
#[test]
fn every_global_timer_keeps_its_value_and_only_the_class_derived_deadline_moves() {
    let p = t12();
    let (on, twin) = (bundle(&p).state, bundle(&off(&p)).state);
    let globals = |s: &PalwStateParamsV2| {
        (
            s.window_bind(),
            s.window_receipt(),
            s.window_challenge(),
            s.window_challenge_at(0),
            s.window_court(),
            s.turn_deadline_daa(),
            kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(s),
            s.claim_retirement_daa(),
            s.fp_abandon_hold_daa(),
        )
    };
    assert_eq!(globals(&on), globals(&twin), "no global timer moves");
    assert_eq!(globals(&on), (600, 600, 1_200, 120, 3_000, 42, 1_200, 3_000, 600), "testnet-12's values (K6, K7)");
    assert_eq!(p.pruning_depth(), off(&p).pruning_depth(), "the pruning depth is untouched by the fence");
    let s = genesis_state(&p);
    let (short, id2m) = model_classes(&p);
    let a = PalwClaimVerifyShapeV1::Attempt;
    assert_eq!((on.receipt_window_for_claim_v1(&s, &id2m, a, 0), twin.receipt_window_for_claim_v1(&s, &id2m, a, 0)), (13_995, 2_799));
    assert_eq!((on.receipt_window_for_claim_v1(&s, &short, a, 0), twin.receipt_window_for_claim_v1(&s, &short, a, 0)), (600, 600));
}
