//! audit(liveness-court): print testnet-12's shipped fences (int-12 release shape) and which are live at DAA 13,850.
#![allow(warnings)]
use kaspa_consensus_core::config::params::palw_t12_shipped_params;

#[test]
fn audit_liveness_court_print_t12_fences() {
    let p = palw_t12_shipped_params();
    for (name, f) in p.palw_fences_v1() {
        let h = f.map(|f| f.daa_score());
        let live = matches!(h, Some(x) if x <= 13_850);
        println!("FENCE {name:60} {:>12} {}", h.map(|x| x.to_string()).unwrap_or("None".into()), if live { "LIVE" } else { "-" });
    }
}
