//! **RFC-0009 stage B — `palw_evidence_court_v1`** — dormant on every ruleset (in no release), fingerprinted where armed (its response window
//! with it), named by the fork id, refused without `palw_da_court` or off ConsensusV2. NOTE: no fold reads it yet.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_evidence_court_fence`

use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, PALW_T12_INT11_FENCES_V1, Params, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS,
    devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params, palw_t12_release_v5_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_gate_fences_v1, fork_id_v1};
use kaspa_consensus_core::palw_evidence_court_v1::{PALW_EVIDENCE_COURT_RESPONSE_WINDOW_DAA_V1, palw_evidence_court_value_v1};

const AT: u64 = 7_927;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn armed(at: Option<ForkActivation>) -> Params {
    let mut p = palw_t12_release_v5_params();
    // The DA court is a genesis rule on testnet-12's release ruleset; make sure it stands at or below the fence under test.
    p.palw_da_court = Some(ForkActivation::always());
    p.palw_evidence_court_v1 = at;
    p
}

#[test]
fn it_is_in_no_release_and_dormant_on_every_ruleset() {
    assert!(PALW_T12_INT11_FENCES_V1.iter().all(|f| f.name != "palw_evidence_court_v1"));
    for (name, p) in [
        ("MAINNET_PARAMS", MAINNET_PARAMS),
        ("TESTNET_PARAMS", TESTNET_PARAMS),
        ("TESTNET11_PARAMS", TESTNET11_PARAMS),
        ("DEVNET_PARAMS", DEVNET_PARAMS),
        ("SIMNET_PARAMS", SIMNET_PARAMS),
        ("mainnet_shipped_params", mainnet_shipped_params()),
        ("devnet_shipped_params", devnet_shipped_params()),
        ("palw_rc_shipped_params", palw_rc_shipped_params()),
        ("palw_t12_release_v5_params", palw_t12_release_v5_params()),
        ("palw_t12_shipped_params", palw_t12_shipped_params()),
    ] {
        assert_eq!(p.palw_evidence_court_v1, None, "{name}");
        assert_eq!(p.palw_evidence_court_fence(), None, "{name}");
        assert!(p.palw_fences_v1().contains(&("palw_evidence_court_v1", None)), "{name}: the exhaustive list names it");
        p.validate_palw_evidence_court_v1().unwrap_or_else(|e| panic!("{name}: dormant validates: {e:?}"));
    }
}

#[test]
fn armed_it_moves_the_ruleset_and_the_schedule_never_the_identity_and_the_fork_id_gates_it() {
    let base = armed(None);
    let p = armed(Some(ForkActivation::new(AT)));
    p.validate_palw_evidence_court_v1().expect("with the DA court below it");
    let (a, b) = (ids(&p), ids(&base));
    assert_ne!(a.0, b.0, "the params id names it");
    assert_ne!(a.2, b.2, "and the schedule");
    assert_eq!(a.1, b.1, "the identity does not move for a future height");
    assert!(fork_id_gate_fences_v1(&p).contains(&AT));
    let old = fork_id_v1(&base, AT);
    assert!(evaluate_fork_id_v1(&p, AT, old.fired.as_bytes().as_slice(), old.next).refuses());
    let below = fork_id_v1(&base, AT - 1);
    assert!(!evaluate_fork_id_v1(&p, AT - 1, below.fired.as_bytes().as_slice(), below.next).refuses());
    let never = armed(Some(ForkActivation::never()));
    assert_eq!(ids(&never).1, b.1);
    assert_eq!(never.palw_evidence_court_fence(), None);
    assert_eq!(palw_evidence_court_value_v1(), [PALW_EVIDENCE_COURT_RESPONSE_WINDOW_DAA_V1]);
}

#[test]
fn it_is_refused_without_the_da_court_and_off_consensus_v2() {
    let mut no_da = armed(Some(ForkActivation::new(AT)));
    no_da.palw_da_court = None;
    assert!(format!("{:?}", no_da.validate_palw_evidence_court_v1().unwrap_err()).contains("palw_evidence_court_v1"));
    let mut later = armed(Some(ForkActivation::new(AT)));
    later.palw_da_court = Some(ForkActivation::new(AT + 1));
    assert!(later.validate_palw_evidence_court_v1().is_err());
    let mut v1 = TESTNET11_PARAMS;
    v1.palw_evidence_court_v1 = Some(ForkActivation::new(AT));
    assert!(v1.validate_palw_evidence_court_v1().is_err());
}
