//! **Lane FLEET: below DAA 17,000 this branch runs the public testnet-12 fleet's ruleset** (rev `247ade1f9`, published on
//! `origin/main` as `d2214e057`).
//!
//! `dump_t12_shipped_ids` prints the ids in a form that compiles on both sources, so the same file can be copied into a
//! detached checkout of `247ade1f9` and run there (`scripts/verify/t12-fleet-identity.sh`). The pinned values below were
//! computed that way from `247ade1f9`. `fleet_ids_match_247ade1f9` holds the shipped ruleset to them.
#![allow(warnings)]
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::fork_id_v1::fork_id_v1;

const FORK_ID_DAAS: [u64; 7] = [0, 5_300, 15_000, 15_499, 15_500, 15_870, 17_399];

fn lines() -> Vec<String> {
    let p = palw_t12_shipped_params();
    let mut out = vec![
        format!("params={}", p.consensus_params_id()),
        format!("identity={}", p.consensus_identity_id()),
        format!("schedule={}", p.consensus_schedule_id()),
        format!("schedule_below_17000={:?}", p.fence_schedule_v1().into_iter().filter(|h| *h < 17_400).collect::<Vec<u64>>()),
    ];
    for daa in FORK_ID_DAAS {
        out.push(format!("fork_id@{daa}={:?}", fork_id_v1(&p, daa)));
    }
    let mut fences: Vec<String> = p
        .palw_fences_v1()
        .iter()
        .filter_map(|(n, f)| f.map(|f| (*n, f.daa_score())))
        .filter(|(_, h)| *h < 17_400)
        .map(|(n, h)| format!("{n}@{h}"))
        .collect();
    fences.sort();
    out.push(format!("fences_below_17000={}", fences.join(",")));
    out
}

#[test]
fn dump_t12_shipped_ids() {
    for line in lines() {
        println!("FLEETID {line}");
    }
}

// ---- branch-only below: scripts/verify/t12-fleet-identity.sh copies only the lines above into the fleet checkout ----

/// The public fleet's values, computed from `247ade1f9` by `scripts/verify/t12-fleet-identity.sh` (2026-10-26). `params` is the
/// release fingerprint `ef379378f09bf33f…` that `d2214e057` names.
const FLEET_247ADE1F9: [(&str, &str); 3] = [
    ("params", concat!("ef379378f09bf33f29e97854bbee157a", "8f86fa1dd8248e1a847f08c5c6e7f15e")),
    ("identity", concat!("5de80e64b63572de0cbf1a09679034e3", "a1765166e8249d3a88f8e29891215bb5")),
    ("schedule", concat!("9ef2b1a795c74d6faced545752f2d17c", "7e731e8e94155d078bc00cbee6c878f5")),
];

/// With the G14 17,000 flag day ARMED on the preset (user decision 2026-10-27): the handshake identity is the fleet's; the release's
/// own params fingerprint and schedule id move (a new height is scheduled), and the baseline without the flag day
/// (`palw_t12_release_v7_params`) is the fleet's byte for byte. Prints the new fingerprint (the kit's EXPECT_FP).
#[test]
fn fleet_ids_match_247ade1f9() {
    let shipped = palw_t12_shipped_params();
    let base = kaspa_consensus_core::config::params::palw_t12_release_v7_params();
    let [(_, params), (_, identity), (_, schedule)] = FLEET_247ADE1F9;
    assert_eq!(base.consensus_params_id().to_string(), params, "baseline params = the fleet's");
    assert_eq!(base.consensus_schedule_id().to_string(), schedule, "baseline schedule = the fleet's");
    assert_eq!(base.consensus_identity_id().to_string(), identity);
    assert_eq!(shipped.consensus_identity_id().to_string(), identity, "the release keeps the fleet's handshake identity");
    // The release's fingerprint moves exactly when the flag day is armed on the preset (dormant since F1).
    let armed = kaspa_consensus_core::config::params::PALW_T12_G14_17000_FLAG_DAY_DAA.is_some();
    assert_eq!(shipped.consensus_params_id().to_string() != params, armed, "fingerprint vs armed");
    println!(
        "FLEETDIAG EXPECT_FP={} UPGRADE_FROM_FP={params} schedule={}",
        shipped.consensus_params_id(),
        shipped.consensus_schedule_id()
    );
}

/// **The DAA-17,000 slice, armed, keeps the handshake identity** (a future height collapses to `never`). Diagnostic: prints every
/// slice entry that moves the identity on its own.
#[test]
fn slice_at_17000_keeps_the_handshake_identity() {
    use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_KERNEL_SLICE_FENCES_V1, palw_t12_arm_after_int13_at_v1};
    let base = palw_t12_shipped_params();
    let id = base.consensus_identity_id();
    let mut moved = vec![];
    for f in PALW_T12_KERNEL_SLICE_FENCES_V1 {
        let mut p = base.clone();
        (f.set)(&mut p, Some(ForkActivation::new(17_400)));
        if p.consensus_identity_id() != id {
            moved.push(f.name);
        }
    }
    println!("FLEETDIAG slice entries that move the identity alone: {moved:?}");
    let mut p = base.clone();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(17_400));
    assert_eq!(p.consensus_identity_id(), id, "the slice at 17,000 moves the handshake identity: {moved:?}");
}

/// **Lane ACT2: the DAA-17,000 G14 list (slice + six route families), armed, leaves everything below 17,000 as the fleet runs it** —
/// handshake identity, fork id at every probed DAA below 17,000, the schedule below 17,000 and every fence armed below 17,000.
#[test]
fn g14_17000_list_keeps_everything_below_17000() {
    use kaspa_consensus_core::config::params::{
        PALW_T12_G14_17000_DAA_V1, PALW_T12_G14_17000_FENCES_V1, Params, palw_t12_arm_after_int13_at_v1,
    };
    // The fleet's ruleset (the release without its 17,000 flag day) against the release as shipped (the flag day armed).
    let base = kaspa_consensus_core::config::params::palw_t12_release_v7_params();
    let mut p = palw_t12_shipped_params();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_G14_17000_FENCES_V1, Some(PALW_T12_G14_17000_DAA_V1));
    if kaspa_consensus_core::config::params::PALW_T12_G14_17000_FLAG_DAY_DAA.is_some() {
        assert_eq!(p.consensus_params_id(), palw_t12_shipped_params().consensus_params_id(), "the preset arms exactly this list");
    }
    let below = |q: &Params| {
        let mut f: Vec<String> = q
            .palw_fences_v1()
            .iter()
            .filter_map(|(n, f)| f.map(|f| (*n, f.daa_score())))
            .filter(|(_, h)| *h < 17_400)
            .map(|(n, h)| format!("{n}@{h}"))
            .collect();
        f.sort();
        f
    };
    let sched = |q: &Params| q.fence_schedule_v1().into_iter().filter(|h| *h < 17_400).collect::<Vec<u64>>();
    assert_eq!(p.consensus_identity_id(), base.consensus_identity_id(), "handshake identity moved");
    for daa in FORK_ID_DAAS {
        // `next` announces the upcoming fence (17,000 vs none) by design (ADR-0072 SA-2): a peer may differ there. `fired` may not.
        assert_eq!(fork_id_v1(&p, daa).fired, fork_id_v1(&base, daa).fired, "fired fork id at {daa} moved");
    }
    assert_eq!(sched(&p), sched(&base), "schedule below 17,000 moved");
    assert_eq!(below(&p), below(&base), "a fence below 17,000 moved");
    assert!(p.fence_schedule_v1().contains(&17_400), "the list is not at 17,000");
    println!(
        "FLEETDIAG g14-17000 armed: schedule_id armed={} shipped={}; fork_id@17000 armed={:?} shipped={:?}",
        p.consensus_schedule_id(),
        base.consensus_schedule_id(),
        fork_id_v1(&p, 17_400),
        fork_id_v1(&base, 17_400)
    );
    // The fingerprint the preset will print once PALW_T12_G14_17000_FLAG_DAY_DAA = Some(17_400) (the kit's EXPECT_FP then).
    println!("FLEETDIAG armed EXPECT_FP={}", p.consensus_params_id());
}
