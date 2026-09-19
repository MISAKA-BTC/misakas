//! TEMPORARY audit probe — prints every shipped fence at RUNTIME. Delete before commit.
use kaspa_consensus_core::config::params::{devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params};
use kaspa_consensus_core::config::params::ForkActivation;

#[test]
fn print_every_shipped_fence() {
    for (net, p) in [("testnet-11", palw_rc_shipped_params()), ("mainnet", mainnet_shipped_params()), ("devnet", devnet_shipped_params())] {
        let mut rows: Vec<(String, String)> = p
            .palw_fences_v1()
            .into_iter()
            .map(|(n, f)| {
                let s = match f {
                    None => "DORMANT(None)".to_string(),
                    Some(a) if a == ForkActivation::never() => "NEVER".to_string(),
                    Some(a) => format!("{}", a.daa_score()),
                };
                (n.to_string(), s)
            })
            .collect();
        rows.sort_by_key(|(n, _)| n.clone());
        println!("===== {net} =====");
        for (n, s) in &rows {
            println!("{net:10} {s:>14}  {n}");
        }
    }
}
