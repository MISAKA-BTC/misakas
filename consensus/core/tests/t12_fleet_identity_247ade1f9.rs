//! **Lane FLEET: below DAA 17,000 this branch runs the public testnet-12 fleet's ruleset** (rev `247ade1f9`, published on
//! `origin/main` as `d2214e057`).
//!
//! `dump_t12_shipped_ids` prints the ids in a form that compiles on both sources, so the same file can be copied into a
//! detached checkout of `247ade1f9` and run there (`scripts/verify/t12-fleet-identity.sh`). The pinned values below were
//! computed that way from `247ade1f9`. `fleet_ids_match_247ade1f9` holds the shipped ruleset to them.
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::fork_id_v1::fork_id_v1;

const FORK_ID_DAAS: [u64; 7] = [0, 5_300, 15_000, 15_499, 15_500, 15_870, 16_999];

fn lines() -> Vec<String> {
    let p = palw_t12_shipped_params();
    let mut out = vec![
        format!("params={}", p.consensus_params_id()),
        format!("identity={}", p.consensus_identity_id()),
        format!("schedule={}", p.consensus_schedule_id()),
        format!("schedule_below_17000={:?}", p.fence_schedule_v1().into_iter().filter(|h| *h < 17_000).collect::<Vec<u64>>()),
    ];
    for daa in FORK_ID_DAAS {
        out.push(format!("fork_id@{daa}={:?}", fork_id_v1(&p, daa)));
    }
    let mut fences: Vec<String> = p
        .palw_fences_v1()
        .iter()
        .filter_map(|(n, f)| f.map(|f| (*n, f.daa_score())))
        .filter(|(_, h)| *h < 17_000)
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

#[test]
fn fleet_ids_match_247ade1f9() {
    let got = lines();
    for (name, want) in FLEET_247ADE1F9 {
        assert!(got.contains(&format!("{name}={want}")), "{name}: {got:?}");
    }
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
        (f.set)(&mut p, Some(ForkActivation::new(17_000)));
        if p.consensus_identity_id() != id {
            moved.push(f.name);
        }
    }
    println!("FLEETDIAG slice entries that move the identity alone: {moved:?}");
    let mut p = base.clone();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(17_000));
    assert_eq!(p.consensus_identity_id(), id, "the slice at 17,000 moves the handshake identity: {moved:?}");
}
