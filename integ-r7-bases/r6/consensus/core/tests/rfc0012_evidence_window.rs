//! RFC-0012 native settlement: **when can a Final REAL attempt still be read from PALW state?**
//!
//! The dormant snapshot builder used to read its REAL work evidence from the claims the sink's state
//! still holds, skipping every claim whose `trace_retention_daa` is in the future. This file pins the
//! shipped windows that make that combination empty, with the real testnet-12 rulesets:
//!
//! * a producer's trace retention is `accepted + bind + receipt + challenge + court`
//!   ([`palw_min_trace_retention_daa_v1`], 5,400 DAA on testnet-12);
//! * a terminal claim leaves the state at `terminal + claim_retirement` (3,000 DAA), and its DA/court
//!   rows leave with it.
//!
//! A claim is therefore retired before its trace retention can lapse unless it spent (almost) every
//! window it was given. The numbers are printed so the record can quote them.
use kaspa_consensus_core::config::params::{palw_t12_launch_params_v1, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_producer_v2::palw_min_trace_retention_daa_v1;

#[test]
fn rfc0012_gap_a_retained_claim_is_retired_before_its_trace_retention_lapses() {
    for (name, params) in
        [("testnet-12 as launched", palw_t12_launch_params_v1()), ("testnet-12 as shipped", palw_t12_shipped_params())]
    {
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("{name} is ConsensusV2") };
        let s = &bundle.state;
        let retention = palw_min_trace_retention_daa_v1(s); // trace_retention_daa - accepted_daa
        let retire = s.claim_retirement_daa(); // the claim is gone at final + retire
        assert!(retire > 0, "{name}: terminal claims retire");
        // The earliest DAA (relative to acceptance) at which a claim can be read as evidence: it is Final and its retention has lapsed.
        let first_visible = retention;
        for challenge_daa in [0u64, 10_000_000] {
            let challenge = s.window_challenge_at(challenge_daa);
            // Slowest finalisation with one bind: bind + receipt + challenge. With one re-bind: twice (bind + receipt) + challenge.
            let slowest_single_bind = s.window_bind() + s.window_receipt() + challenge;
            let slowest_with_rebind = 2 * (s.window_bind() + s.window_receipt()) + challenge;
            // A claim finalising at offset f is in state during [f, f + retire) and readable (under the old rule) from `retention`.
            let visible_single = (retention as i64 - (slowest_single_bind + retire) as i64).abs();
            eprintln!(
                "[rfc0012-evidence-window] {name}: challenge window {challenge} DAA, retention {retention}, claim retirement {retire}; \
                 slowest single-bind Final at +{slowest_single_bind} retires at +{}, retention lapses at +{first_visible}; \
                 slowest re-bound Final at +{slowest_with_rebind} retires at +{} (|gap| {visible_single})",
                slowest_single_bind + retire,
                slowest_with_rebind + retire,
            );
            assert!(
                slowest_single_bind + retire <= first_visible,
                "{name}: an ordinary claim ({slowest_single_bind}+{retire}) outlives its retention ({first_visible}) - the old reader could see it"
            );
        }
    }
}
