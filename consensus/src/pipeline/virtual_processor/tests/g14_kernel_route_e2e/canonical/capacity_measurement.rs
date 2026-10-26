//! Native block-budget curve with simultaneous outstanding fixture claims.
//! The processor's cfg(test) cap is FOUR, not the production cap of 64.
//! This is not TIR session capacity, real hardware saturation, or a network campaign.
use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1;
use serde_json::json;
use std::time::Instant;

fn budget(net: &Net) -> (u64, u32, u64) {
    net.chain.tip_state().1.kernel_route().and_then(|k| k.aux_row(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[])).unwrap()
}

async fn measured_send(net: &mut Net, items: Vec<(usize, Obj)>) -> serde_json::Value {
    let before = chain_blocks(&net.chain, net.chain.sink()).len();
    let objects = items.len();
    let object_bytes: usize = items.iter().map(|(_, o)| borsh::to_vec(o).unwrap().len()).sum();
    let from = net.daa();
    let start = Instant::now();
    net.send(items).await;
    let wall_ns = start.elapsed().as_nanos();
    let blocks = chain_blocks(&net.chain, net.chain.sink());
    let transactions: Vec<_> = blocks.iter().skip(before).flat_map(|b| b.transactions.iter().skip(1)).collect();
    // Full local Borsh transaction representation, not P2P wire framing.
    let transaction_borsh_bytes: usize = transactions.iter().map(|t| borsh::to_vec(t.as_ref()).unwrap().len()).sum();
    let (blue, runs, work) = budget(net);
    json!({"objects":objects,"object_bytes":object_bytes,"transaction_borsh_bytes":transaction_borsh_bytes,
        "transactions":transactions.len(),"wall_ns":wall_ns,"daa_before":from,"daa_after":net.daa(),
        "budget_blue":blue,"charged_adjudications":runs,"charged_work":work})
}

#[tokio::test]
#[ignore = "explicit local fixture capacity measurement under buildslot"]
async fn g14_kernel_capacity_measure() {
    kaspa_core::log::try_init_logger("warn");
    let n: usize = std::env::var("G14_CAP_N").unwrap().parse().unwrap();
    let count: usize = std::env::var("G14_CAP_CLAIMS").unwrap().parse().unwrap();
    let positions: u32 = std::env::var("G14_CAP_POSITIONS").unwrap().parse().unwrap();
    assert!(n <= 1 && [1, 2, 4, 6].contains(&count) && [6, 64].contains(&positions));
    let (config, mut bundle, premine, floats) = canonical_config(false);
    let mut params = config.params.clone();
    params.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(0));
    bundle.state = bundle.state.clone().with_kernel_route_panel_one_from_daa(Some(0));
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut params.palw_consensus_mode {
        b.state = b.state.clone().with_kernel_route_panel_one_from_daa(Some(0));
    }
    assert!(params.validate_palw_v2().is_err());
    let setup = Instant::now();
    let mut w = World::on(Net::over_cfg((Config::new(params), bundle, premine, floats), TestConsensus::new)).await;
    let mut claims = Vec::new();
    let mut first_seats = Vec::new();
    for _ in 0..count {
        w.jobs += 1;
        let job = KernelJobV1 {
            class_binding_id: w.class,
            prompt: vec![3, 17, 9],
            max_new_tokens: positions - 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [w.jobs; 64],
        };
        let object = w.net.route(1, &K::PostJob { job: job.clone() });
        w.net.send(vec![(1, object)]).await;
        assert!(w.net.ledger().jobs.contains_key(&job.id()));
        let claim = w.claim_with(0, &job, true, Delivery::Chunked(4_096)).await;
        assert!(w.net.ledger().claims.contains_key(&claim.id));
        let assigned = w.seats(&claim.id);
        assert_eq!(assigned.len(), 1);
        if claims.is_empty() {
            first_seats = assigned;
        }
        w.cover(&claim.id).await;
        claims.push(claim);
    }
    let setup_ns = setup.elapsed().as_nanos();
    let policy = w.policy();
    assert_eq!(policy.max_adjudications_per_block, 4, "explicit reduced test cap");
    let per_work = w.net.ledger().bounds_of(&w.class).unwrap().max_court_work;
    assert!(per_work > 0);
    let capacity = (policy.max_court_work_per_block / per_work).min(policy.max_adjudications_per_block as u64) as usize;
    let outsider = w.outsiders(&claims[0], &first_seats, 1)[0];
    let initial_states: Vec<_> = claims.iter().map(|c| format!("{:?}", w.net.claim_state(&c.id))).collect();
    let sybils: Vec<usize> = (1..8).filter(|c| *c != outsider).take(count).collect();
    assert_eq!(sybils.len(), count);
    let junk = ProsecutionV1::Kernel(vec![0xEE; 32]);
    let mut rounds = Vec::new();
    let mut conviction = serde_json::Value::Null;
    if n == 1 {
        for round in 0..2 {
            let seals = sybils
                .iter()
                .zip(&claims)
                .map(|(card, claim)| {
                    let accuser = w.net.kid(*card);
                    (
                        *card,
                        w.net
                            .route(*card, &K::SealProof { accuser, claim: claim.id, seal: proof_seal_v1(&claim.id, &accuser, &junk) }),
                    )
                })
                .collect();
            let seal_cost = measured_send(&mut w.net, seals).await;
            let before: Vec<_> = sybils.iter().map(|c| w.net.collateral(*c)).collect();
            let filings = sybils
                .iter()
                .zip(&claims)
                .map(|(card, claim)| {
                    let accuser = w.net.kid(*card);
                    (*card, w.net.route(*card, &K::FileProof { accuser, claim: claim.id, proof: junk.clone() }))
                })
                .collect();
            let filing_cost = measured_send(&mut w.net, filings).await;
            let charged: Vec<_> = sybils.iter().zip(before).map(|(c, b)| b - w.net.collateral(*c)).collect();
            let accepted = charged.iter().filter(|v| **v > 0).count();
            assert_eq!(accepted, count.min(capacity));
            assert_eq!(budget(&w.net).1 as usize, accepted);
            assert!(claims.iter().all(|c| !w.net.ledger().claims[&c.id].convicted));
            rounds.push(json!({"round":round,"seal":seal_cost,"filing":filing_cost,"dismissal_fees":charged,
                "adjudicated":accepted,"over_budget":count-accepted}));
        }
        let claim = &claims[0];
        let start = Instant::now();
        let finding = w.fresh(0x75).check(claim.id, &claim.published(&w.fx, &[]), &w.fx.params);
        let check_ns = start.elapsed().as_nanos();
        let OutsiderFindingV1::Prosecute(proof) = finding else { panic!("public material exposes the lie") };
        let accuser = w.net.kid(outsider);
        let object =
            w.net.route(outsider, &K::SealProof { accuser, claim: claim.id, seal: proof_seal_v1(&claim.id, &accuser, &proof) });
        let seal = measured_send(&mut w.net, vec![(outsider, object)]).await;
        let object = w.net.route(outsider, &K::FileProof { accuser, claim: claim.id, proof });
        let filing = measured_send(&mut w.net, vec![(outsider, object)]).await;
        assert!(w.net.ledger().claims[&claim.id].convicted);
        conviction = json!({"local_check_wall_ns":check_ns,"seal":seal,"filing":filing,"convicted":true});
    }
    println!(
        "G14_CAP_MEASURE {}",
        json!({"schema":1,"n":n,"claims":count,"positions":positions,
        "setup_wall_ns":setup_ns,"claim_states_at_measurement":initial_states,"test_adjudication_cap":policy.max_adjudications_per_block,
        "reserved_prosecution_runs":policy.prosecution_reserved_runs(),"work_cap":policy.max_court_work_per_block,
        "per_claim_worst_work":per_work,"calculated_adjudications":capacity,"rounds":rounds,"outsider":conviction,
        "arming_evidence":false,"fixture_only":true,"test_policy":true,"production_cap_measured":false,
        "continuous_adversary_inclusion_guaranteed":false,"gate_c_measured":false,"real_model_measured":false})
    );
}
