//! **F1 — heartbeat transparency stops at the merging block's own selected chain (ADR-0105 §11) — is
//! a post-launch flag day: dormant on every shipped preset, testnet-12 included, until an operator
//! arms it at a height.**
//!
//! `Params::palw_heartbeat_transparent_same_chain` changes which blocks are blue past its height, so it
//! is a consensus fence, and testnet-12 launched without it (release `0e8ec984e`). What this file
//! holds, from the fingerprints to the handshake:
//!
//! * dormant everywhere, so testnet-12's ids are the release's to the byte (and every other preset's
//!   pins, held by their own files, cannot have moved: a Some-only writer writes nothing for `None`);
//! * armed at a height it moves `consensus_params_id` and `consensus_schedule_id` — an operator reading
//!   either sees the build — and NOT `consensus_identity_id`, so an armed build and the launched build
//!   peer for the whole rollout (the four places a Some-only fence needs, the fourth being the
//!   `never()` collapse in `normalize_values_a_scheduled_fence_drags_with_it`);
//! * the fork id names the height, so an armed node keeps the launched build as a peer below it and
//!   refuses it from it — the flag day as a named refusal instead of a silent fork;
//! * `validate_palw_v2` refuses it where ADR-0105's transparency is not in force at or below it.

use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_BOND_MATURITY_WINDOW_DAA, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params,
    mainnet_shipped_params, palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{
    FORK_ID_NO_NEXT_FENCE, ForkIdMismatch, ForkIdVerdict, evaluate_fork_id_v1, fired_fences_digest_v1, fork_id_gate_armed_v1,
    fork_id_gate_fences_v1, fork_id_refusal_height_v1, fork_id_v1,
};

/// testnet-12 as released (`0e8ec984e`, the shipping re-pin `9c717c16`): params, identity, schedule —
/// `palw_clock_lead_cap_is_t12_only`'s `T12_WITH_THE_CAP`, restated so this file fails by itself if
/// the fence ever moves them.
const T12_RELEASED: (&str, &str, &str) = (
    "b8564b888e55bb5f797e708a3f65e7cd122065123a3ab09cbeb8d10c98715d8f",
    "5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5",
    "93da24cc60f7a77e63c43106e96298d2979644a3fc0c3f82529c8849333127fd",
);

/// An illustrative flag-day height. The operator picks one common, independent height for every
/// post-launch fence (a height no other fence uses, or the fork id cannot tell the builds apart).
const H: u64 = 1_234_567;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed_at(at: ForkActivation) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_heartbeat_transparent_same_chain = Some(at);
    p
}

fn shipped_presets() -> Vec<(&'static str, Params)> {
    vec![
        ("testnet-12", palw_t12_shipped_params()),
        ("testnet-11", palw_rc_shipped_params()),
        ("devnet", devnet_shipped_params()),
        ("mainnet", mainnet_shipped_params()),
        ("testnet-10", Params::from(TESTNET_PARAMS.net)),
        ("simnet", Params::from(SIMNET_PARAMS.net)),
    ]
}

/// **Dormant on every preset — testnet-12 included — and testnet-12's ids are the release's.**
#[test]
fn the_same_chain_fence_is_dormant_on_every_preset_and_t12_is_the_release() {
    for (name, p) in shipped_presets() {
        assert_eq!(p.palw_heartbeat_transparent_same_chain, None, "{name}: F1 ships dormant");
        assert_eq!(p.palw_heartbeat_transparent_same_chain_fence(), None, "{name}: and the reader resolves nothing");
        assert!(
            p.palw_fences_v1().contains(&("palw_heartbeat_transparent_same_chain", None)),
            "{name}: registered in the fence list the fork id and the schedule walk read"
        );
        p.validate_palw_v2().unwrap_or_else(|e| panic!("{name}: {e:?}"));
    }
    let t12 = ids(&palw_t12_shipped_params());
    println!("testnet-12: {t12:?}");
    assert_eq!((t12.0.as_str(), t12.1.as_str(), t12.2.as_str()), T12_RELEASED, "testnet-12 is the released ruleset, to the id");
    assert_eq!(
        palw_t12_shipped_params().fence_schedule_v1(),
        vec![PALW_T12_BOND_MATURITY_WINDOW_DAA],
        "the released testnet-12 schedules one height, ADR-0065 D1's bond-maturity window, and F1 adds none"
    );
}

/// **Armed at a height: the params id and the schedule id move, the identity does not** — so the armed
/// build and the launched build peer for the whole rollout. Armed at genesis it is a rule about block 1
/// and the identity moves too; `Some(never())` is absence in the identity (the collapse), and in the
/// schedule walk it normalises out like any unset height.
#[test]
fn arming_moves_the_params_and_schedule_ids_and_not_the_identity() {
    let shipped = palw_t12_shipped_params();
    let armed = armed_at(ForkActivation::new(H));
    armed.validate_palw_v2().expect("testnet-12 validates with F1 scheduled: its transparency is in force from genesis");
    let (s, a) = (ids(&shipped), ids(&armed));
    println!("testnet-12 launched: {s:?}");
    println!("testnet-12 with F1 at {H}: {a:?}");
    assert_ne!(a.0, s.0, "the ruleset a node announces names the scheduled fence");
    assert_ne!(a.2, s.2, "and so does the schedule the operator log prints");
    assert_eq!(a.1, s.1, "a height not yet reached is not yet a rule: the identity two nodes must share is the release's");

    // Two heights, two schedules — the operator log tells them apart — one identity.
    let other = armed_at(ForkActivation::new(H + 1));
    assert_ne!(ids(&other).2, a.2, "a different height is a different schedule");
    assert_eq!(ids(&other).1, s.1);

    // Genesis-armed (a future regenesis or card): a rule about block 1, a different identity.
    let genesis = armed_at(ForkActivation::always());
    genesis.validate_palw_v2().expect("genesis arming validates beside genesis transparency");
    assert_ne!(ids(&genesis).1, s.1, "in force from block one separates identities");

    // `Some(never())`: the collapse keeps it out of the identity (the fourth place a Some-only fence
    // needs; without it the two builds would refuse each other on deploy day).
    let never = armed_at(ForkActivation::never());
    assert_eq!(ids(&never).1, s.1, "Some(never()) is absence in the identity — or the never() collapse is missing");
    never.validate_palw_v2().expect("a never() fence is inert and validates anywhere");
}

/// **The fork id names the height: the armed build keeps the launched build as a peer below it and
/// refuses it from it.** testnet-12 as launched already schedules one height — ADR-0065 D1's bond
/// maturity window, armed AT its own window (1,000 DAA) — so its fork-id gate is armed, and which
/// side refuses depends on where F1's height falls against it:
///
/// * **below 1,000** (the fix inside the first ~33 hours): the armed build refuses the launched one
///   from `H`. The launched build never refuses before `H` either: a peer it met below `H` announced a
///   next fence (`H`) lower than its own, so it stores `H` as the refusal height and re-judges there;
///   a peer it meets past `H` announces a fired set it cannot place (`UnknownFiredSet`), which it
///   warns about until its own gate fence (1,000) and refuses from it;
/// * **above 1,000**: the two announce identical fork ids until 1,000, and between 1,000 and `H` the
///   launched build keeps the armed one (`Agree`: the peer has crossed its whole schedule and names
///   a fence it does not carry — this build is the one out of date). From `H` the armed build refuses
///   the launched one; a fresh announcement past `H` is a fired set the launched build cannot place,
///   refused because it is past its own gate fence.
///
/// Either way nobody is refused before `H`, which is what lets the fleet install the armed build
/// ahead of the height. **But above 1,000 a connection an armed node made below 1,000 is never
/// re-judged**: the launched peer announced `next = 1,000`, which the armed schedule also has next,
/// so the stored snapshot agrees at every height — the stale peer stays connected past `H` until the
/// connection is re-made (block validity still refuses what it relays). Below 1,000 there is no such
/// gap. Hence the recommendation: schedule F1 below 1,000, or re-make the armed fleet's connections
/// once the chain is past 1,000.
#[test]
fn an_armed_build_peers_with_the_launched_build_until_the_height() {
    let shipped = palw_t12_shipped_params();
    let maturity = PALW_T12_BOND_MATURITY_WINDOW_DAA;
    assert!(fork_id_gate_armed_v1(&shipped), "the launched testnet-12's gate is armed by the maturity window");
    assert_eq!(fork_id_gate_fences_v1(&shipped), vec![maturity]);
    for h in [maturity / 2, maturity - 1, maturity + 1, H] {
        let armed = armed_at(ForkActivation::new(h));
        let mut schedule = vec![h, maturity];
        schedule.sort_unstable();
        assert_eq!(armed.fence_schedule_v1(), schedule, "F1 at {h}: the armed schedule");
        assert_eq!(fork_id_gate_fences_v1(&armed), schedule, "F1 at {h}: and the gate names both heights");
        // Both nodes on one chain at the same DAA score, each judging the other's announcement.
        let mut probes = vec![0, 1, h - 1, h, h + 1, maturity - 1, maturity, maturity + 1, h.max(maturity) + 1_000];
        probes.sort_unstable();
        probes.dedup();
        for d in probes {
            let (a, l) = (fork_id_v1(&armed, d), fork_id_v1(&shipped, d));
            let armed_view = evaluate_fork_id_v1(&armed, d, l.fired.as_bytes().as_slice(), l.next);
            let launched_view = evaluate_fork_id_v1(&shipped, d, a.fired.as_bytes().as_slice(), a.next);
            assert_eq!(
                armed_view.refuses(),
                d >= h,
                "F1 at {h}, DAA {d}: the armed build refuses the launched one exactly from {h}: {armed_view:?}"
            );
            // Judged on a fresh announcement at the same score: past `H` the armed peer's fired set is
            // one the launched build cannot place, refused once it is past its own gate fence.
            let launched_refuses = d >= h.max(maturity);
            assert_eq!(
                launched_view.refuses(),
                launched_refuses,
                "F1 at {h}, DAA {d}: the launched build refuses a fresh armed announcement from {}: {launched_view:?}",
                h.max(maturity)
            );
            assert!(!launched_view.refuses() || d >= h, "F1 at {h}, DAA {d}: nobody is refused before the height");
            if d < h.min(maturity) && h > maturity {
                assert_eq!(
                    (a.fired, a.next),
                    (l.fired, l.next),
                    "F1 at {h}, DAA {d}: below both heights the announcements are identical"
                );
            }
        }
        let (l0, a0) = (fork_id_v1(&shipped, 0), fork_id_v1(&armed, 0));
        let rejudged_from_below = fork_id_refusal_height_v1(&armed, 0, l0.fired.as_bytes().as_slice(), l0.next);
        if h < maturity {
            assert_eq!(
                rejudged_from_below,
                Some(h),
                "F1 at {h}: a launched peer kept at the handshake is judged again at {h}, and refused there"
            );
        } else {
            // **The gap, measured.** A launched peer met below 1,000 announced `next = 1,000` — this
            // schedule's own first fence — so the snapshot the connection layer keeps agrees with
            // this build at every height, and a connection made below 1,000 is NEVER re-judged to a
            // refusal at `H`. Only a handshake made past 1,000 carries a refusal height.
            assert_eq!(rejudged_from_below, None, "F1 at {h}: a connection made below 1,000 is not re-judged at {h}");
            let lm = fork_id_v1(&shipped, maturity);
            assert_eq!(
                fork_id_refusal_height_v1(&armed, maturity, lm.fired.as_bytes().as_slice(), lm.next),
                Some(h),
                "F1 at {h}: a launched peer met past 1,000 is judged again at {h}, and refused there"
            );
        }
        assert_eq!(
            fork_id_refusal_height_v1(&shipped, 0, a0.fired.as_bytes().as_slice(), a0.next),
            (h < maturity).then_some(h),
            "F1 at {h}: an armed peer the launched build met below both heights is re-judged at {h} if {h} is the lower"
        );
    }

    // Two armed builds: one schedule, seen from wherever each stands.
    let armed = armed_at(ForkActivation::new(H));
    for (mine, theirs) in [(0, 0), (maturity, maturity - 1), (H - 1, H), (H, H - 1), (H + 5, H + 5)] {
        let peer = fork_id_v1(&armed, theirs);
        let verdict = evaluate_fork_id_v1(&armed, mine, peer.fired.as_bytes().as_slice(), peer.next);
        assert_eq!(verdict, ForkIdVerdict::Agree, "local {mine}, peer {theirs}");
    }
    // A build armed at another height is a different schedule, refused from the lower of the two.
    let elsewhere = armed_at(ForkActivation::new(H + 100));
    let peer = fork_id_v1(&elsewhere, maturity);
    assert!(!evaluate_fork_id_v1(&armed, H - 1, peer.fired.as_bytes().as_slice(), peer.next).refuses());
    assert_eq!(
        evaluate_fork_id_v1(&armed, H, peer.fired.as_bytes().as_slice(), peer.next),
        ForkIdVerdict::DisagreePastFence {
            mismatch: ForkIdMismatch::NextFenceDiffers { expected: H, got: H + 100 },
            fired_through: H
        }
    );
    let _ = FORK_ID_NO_NEXT_FENCE;
    let _ = fired_fences_digest_v1;
}

/// **The one height F1 must not take: 1,000, the maturity window's.** The fork id carries heights,
/// sorted and deduplicated, never names — so an armed build at 1,000 announces exactly what the
/// launched build announces at every DAA, neither side ever refuses the other, and past 1,000 the two
/// color blocks differently with nothing but a schedule-id warning in the log: a silent fork. The
/// operator's common post-launch height must be one no fence already uses.
#[test]
fn at_the_maturity_windows_height_the_fence_is_invisible_to_the_fork_id() {
    let shipped = palw_t12_shipped_params();
    let hidden = armed_at(ForkActivation::new(PALW_T12_BOND_MATURITY_WINDOW_DAA));
    assert_eq!(hidden.fence_schedule_v1(), shipped.fence_schedule_v1(), "one height, deduplicated away");
    for d in [0, PALW_T12_BOND_MATURITY_WINDOW_DAA - 1, PALW_T12_BOND_MATURITY_WINDOW_DAA, PALW_T12_BOND_MATURITY_WINDOW_DAA + 1] {
        let (a, l) = (fork_id_v1(&hidden, d), fork_id_v1(&shipped, d));
        assert_eq!((a.fired, a.next), (l.fired, l.next), "DAA {d}: identical announcements");
        assert!(!evaluate_fork_id_v1(&hidden, d, l.fired.as_bytes().as_slice(), l.next).refuses());
    }
    assert_ne!(
        hidden.consensus_schedule_id(),
        shipped.consensus_schedule_id(),
        "only the operator log's schedule id tells them apart"
    );
    assert_eq!(hidden.consensus_identity_id(), shipped.consensus_identity_id());
}

/// **`validate_palw_v2` refuses it where ADR-0105's transparency is not in force at or below it** —
/// a fence that hashes and restricts nothing is the shape the file's other refusals keep out.
#[test]
fn it_is_refused_without_the_transparency_it_narrows() {
    // testnet-11: the heartbeat lane is armed, the transparency is not.
    let mut rc = palw_rc_shipped_params();
    assert!(rc.palw_heartbeat_transparent_fence().is_none());
    rc.palw_heartbeat_transparent_same_chain = Some(ForkActivation::new(H));
    let e = rc.validate_palw_v2().expect_err("no transparency to narrow on testnet-11");
    assert!(format!("{e:?}").contains("palw_heartbeat_transparent_same_chain"), "{e:?}");
    assert!(rc.palw_heartbeat_transparent_same_chain_fence().is_none(), "and the reader never resolves it there");

    // Transparency scheduled ABOVE the restriction: refused (the restriction would fire first on nothing).
    let mut late = palw_t12_shipped_params();
    late.palw_heartbeat_transparent = Some(ForkActivation::new(H + 1));
    late.palw_heartbeat_transparent_same_chain = Some(ForkActivation::new(H));
    let e = late.validate_palw_v2().expect_err("the transparency must be in force at or below the restriction");
    assert!(format!("{e:?}").contains("palw_heartbeat_transparent_same_chain"), "{e:?}");
    late.palw_heartbeat_transparent = Some(ForkActivation::new(H));
    late.validate_palw_v2().expect("the same height is at or below");

    // devnet / mainnet as shipped have no transparency either.
    for mut p in [devnet_shipped_params(), mainnet_shipped_params()] {
        p.palw_heartbeat_transparent_same_chain = Some(ForkActivation::new(H));
        let e = p.validate_palw_v2().expect_err("no transparency to narrow");
        assert!(format!("{e:?}").contains("palw_heartbeat_transparent_same_chain"), "{}: {e:?}", p.net);
    }
}

/// **Armed on testnet-12 it must be armed at a HEIGHT, never at genesis** — a guard for the day an
/// operator schedules it. `palw_t12_base_params` arms its fences with `at = always()` and then walks
/// every fence height to 0 ("pass 2"), so a line added there — at `at`, or at any height before the
/// walk — would make this a genesis rule: a new identity, and every launched node refused at once
/// instead of at the height. Arm it AFTER the walk, as `Some(ForkActivation::new(H))`.
#[test]
fn on_testnet12_it_is_a_height_never_a_genesis_rule() {
    let t12 = palw_t12_shipped_params();
    if let Some(fence) = t12.palw_heartbeat_transparent_same_chain {
        assert!(
            fence.daa_score() > 0 && fence != ForkActivation::never(),
            "testnet-12 launched without F1: arming it at genesis ({fence:?}) is a re-mint, not a flag day"
        );
        assert_eq!(ids(&t12).1, T12_RELEASED.1, "a scheduled height keeps the released identity");
    }
}
