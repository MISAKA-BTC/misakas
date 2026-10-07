//! **ADR-0172 §6a: `W_claim` is canonical ACTIVE compute, not parameter count.** A claim's weight is `claim.pwu` (ADR-0149), the expected
//! attempts a win costs times the derived work of one draw; that work is `economic_ccu_per_claim`, the walk of the class's own program
//! (`palw_tir_work_v1`). For a mixture-of-experts program the walk charges only the experts the router's `TopK` selects.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_accounting_v2_work`

use std::path::PathBuf;

use kaspa_consensus_core::palw_canonical_work_v1::PalwCanonicalExecutionFactsV1;
use kaspa_consensus_core::palw_tir_work_v1::palw_tir_canonical_work_v1;
use misaka_palw_tir::{Prim, TirProgramV1};

fn program(name: &str) -> TirProgramV1 {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/programs").join(format!("{name}.json"));
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(dir).unwrap()).unwrap();
    let hex = v["program_borsh_hex"].as_str().unwrap();
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap()).collect();
    TirProgramV1::decode_canonical(&bytes).unwrap()
}

/// The routed-expert work of a job, in the vector's unit (MACs × weight width for an int8 weight).
fn routed(p: &TirProgramV1, prefill: u32, generated: u32) -> u128 {
    palw_tir_canonical_work_v1(p, &PalwCanonicalExecutionFactsV1::uncached(prefill, generated)).unwrap().routed_expert_matmul
}

#[test]
fn a_moe_claim_is_priced_on_the_experts_the_router_selects_not_on_all_of_them() {
    let p = program("moe-top2-shared");
    let layers = p.schedule.layers.len() as u128;
    let k = p
        .blocks
        .iter()
        .flat_map(|b| b.nodes.iter())
        .find_map(|n| if let Prim::TopK { k, .. } = n.prim { Some(k as u128) } else { None })
        .expect("a router");
    // Every stacked-expert weight (`*_exps.w`, leading dimension E): its elements per expert, and E.
    let experts: Vec<(u128, u128)> = p
        .params
        .iter()
        .filter(|d| d.name.ends_with("_exps.w"))
        .map(|d| {
            let e = d.shape[0] as u128;
            (e, d.shape.iter().map(|x| *x as u128).product::<u128>() / e)
        })
        .collect();
    assert_eq!(experts.len(), 3, "gate, up and down stacks");
    let e = experts[0].0;
    assert!(experts.iter().all(|(n, _)| *n == e) && k < e, "top-{k} of {e}");
    let per_expert_elems: u128 = experts.iter().map(|(_, per)| *per).sum();
    let all_experts_elems = per_expert_elems * e;

    // One more executed position adds exactly the ACTIVE experts' MACs (int8: width 1), over every layer…
    let per_position = routed(&p, 3, 1) - routed(&p, 2, 1);
    assert_eq!(per_position, k * per_expert_elems * layers, "k of E experts, not all of them");
    // …which is k/E of what a dense reading of the same parameters would charge.
    assert_eq!(per_position * e, all_experts_elems * layers * k, "the claim weighs the active fraction of the expert parameters");
    assert!(per_position < all_experts_elems * layers, "total expert parameters are not the weight");
}

#[test]
fn work_is_linear_in_executed_positions_so_weight_follows_compute() {
    let p = program("moe-top2-shared");
    let unit = routed(&p, 3, 1) - routed(&p, 2, 1);
    for prefill in 2..9 {
        assert_eq!(routed(&p, prefill, 1) - routed(&p, 2, 1), unit * (prefill as u128 - 2), "prefill {prefill}");
    }
}
