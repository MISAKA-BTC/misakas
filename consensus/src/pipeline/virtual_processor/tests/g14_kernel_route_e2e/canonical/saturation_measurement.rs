//! Full-cap transport/court experiment with independently funded carriers.
//! Unlike Net::send's per-card change chain, every offered carrier spends a
//! separate, normally mined UTXO. Template limits are observed, never enlarged.
use super::*;
use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1;
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Instant};

fn block_budget(net: &Net) -> (u64, u32, u64) {
    net.chain.tip_state().1.kernel_route().unwrap().aux_row(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]).unwrap()
}

/// Offer independent transactions until the native template has carried them.
/// Keep a fold observation for every block, including the final flushing block.
async fn offer(net: &mut Net, mut pending: Vec<Transaction>, phase: &str) -> Value {
    let initial = pending.len();
    let mut rows = Vec::new();
    for ordinal in 0..=initial {
        if pending.is_empty() {
            break;
        }
        for tx in &pending {
            net.mempool(tx).expect("ordinary mempool acceptance");
        }
        let start = Instant::now();
        let step = net.ttpb();
        let block = net.chain.heartbeat(step, pending.clone()).await;
        let carried: BTreeSet<_> = block.transactions.iter().map(|tx| tx.id()).collect();
        let taken: Vec<_> = pending.iter().filter(|tx| carried.contains(&tx.id())).collect();
        assert!(!taken.is_empty(), "template made no progress in {phase}; pending {}", pending.len());
        let bytes: usize = taken.iter().map(|tx| borsh::to_vec(tx).unwrap().len()).sum();
        let (blue, runs, work) = block_budget(net);
        let row = json!({"ordinal":ordinal,"daa":net.daa(),"offered":pending.len(),"carried":taken.len(),
            "carried_transaction_borsh_bytes":bytes,"wall_ns":start.elapsed().as_nanos(),
            "fold_budget_blue":blue,"fold_adjudications":runs,"fold_work":work});
        println!("G14_SAT_STEP {}", json!({"phase":phase,"block":row}));
        rows.push(row);
        pending.retain(|tx| !carried.contains(&tx.id()));
    }
    assert!(pending.is_empty());
    let start = Instant::now();
    let step = net.ttpb();
    net.chain.heartbeat(step, Vec::new()).await;
    let (blue, runs, work) = block_budget(net);
    rows.push(json!({"flush":true,"daa":net.daa(),"carried":0,"carried_transaction_borsh_bytes":0,
        "wall_ns":start.elapsed().as_nanos(),"fold_budget_blue":blue,"fold_adjudications":runs,"fold_work":work}));
    json!({"phase":phase,"transactions":initial,"blocks":rows})
}

/// Split each sender's current funding coin on chain, then sign one carrier per
/// independent output. Preserve the separate change coin for later ordinary use.
async fn funded_carriers(net: &mut Net, items: Vec<(usize, Obj)>, phase: &str) -> (Vec<Transaction>, Value) {
    let mut counts = BTreeMap::<usize, usize>::new();
    for (card, _) in &items {
        *counts.entry(*card).or_default() += 1;
    }
    let mut splits = Vec::new();
    let mut pools = BTreeMap::<usize, Vec<(TransactionOutpoint, UtxoEntry)>>::new();
    for (card, count) in counts {
        let (outpoint, entry) = net.funding[card].clone();
        let each = (entry.amount - FEE) / (2 * count as u64);
        assert!(each > 2 * FEE, "funding exhaustion is not waived");
        let change = entry.amount - FEE - each * count as u64;
        let mut outputs = vec![TransactionOutput::new(change, card_payout_spk(card))];
        outputs.extend((0..count).map(|_| TransactionOutput::new(each, card_payout_spk(card))));
        let mut split = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            outputs,
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign_spend(&mut split, entry, card, net.config.params.storage_mass_parameter);
        net.funding[card] = (TransactionOutpoint::new(split.id(), 0), UtxoEntry::new(change, card_payout_spk(card), 0, false));
        pools.insert(
            card,
            (0..count)
                .map(|i| (TransactionOutpoint::new(split.id(), i as u32 + 1), UtxoEntry::new(each, card_payout_spk(card), 0, false)))
                .collect(),
        );
        splits.push(split);
    }
    let funding = offer(net, splits, &format!("{phase}-funding")).await;
    let carriers = items
        .into_iter()
        .map(|(card, object)| {
            let change = net.funding[card].clone();
            net.funding[card] = pools.get_mut(&card).unwrap().pop().unwrap();
            let tx = net.carrier(card, &object);
            net.funding[card] = change;
            tx
        })
        .collect();
    (carriers, funding)
}

async fn send_independent(net: &mut Net, items: Vec<(usize, Obj)>, phase: &str) -> Value {
    let (carriers, funding) = funded_carriers(net, items, phase).await;
    let sending = offer(net, carriers, phase).await;
    json!({"funding":funding,"sending":sending})
}

#[tokio::test]
#[ignore = "explicit native transport/court saturation measurement under buildslot"]
async fn g14_kernel_saturation_measure() {
    kaspa_core::log::try_init_logger("warn");
    let n: usize = std::env::var("G14_SAT_N").unwrap().parse().unwrap();
    let count: usize = std::env::var("G14_SAT_CLAIMS").unwrap().parse().unwrap();
    let positions: u32 = std::env::var("G14_SAT_POSITIONS").unwrap().parse().unwrap();
    assert!(n <= 1 && [2, 12].contains(&count) && [6, 65].contains(&positions));
    crate::pipeline::virtual_processor::processor::kernel_route_test_set_adjudication_cap_v1(64);
    let (config, _, premine, floats) = canonical_config(false);
    let mut params = config.params.clone();
    params.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(0));
    params.palw_anchor_window_v1 = Some(ForkActivation::new(0));
    params.sync_palw_g14_gates_v1();
    params.sync_palw_anchor_window_v1();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = params.palw_consensus_mode.clone() else {
        unreachable!()
    };
    assert_eq!(bundle.state.kernel_panel_one_external_gain_bound_v1(), Some(500 * kaspa_consensus_core::constants::SOMPI_PER_KASPA));
    assert!(params.validate_palw_v2().is_err());
    let started = Instant::now();
    let mut w = World::on(Net::over_cfg((Config::new(params), bundle, premine, floats), TestConsensus::new)).await;
    let mut claims = Vec::new();
    for index in 0..count {
        let job = KernelJobV1 {
            class_binding_id: w.class,
            prompt: vec![3, 17, 9],
            max_new_tokens: positions - 3,
            decode: DecodeRuleV1::Greedy,
            nonce: [index as u8 + 1; 64],
        };
        let object = w.net.route(1, &K::PostJob { job: job.clone() });
        w.net.send(vec![(1, object)]).await;
        let ledger = w.net.ledger();
        let generated = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
        let at = matmul_at(&w.fx.program, positions - 2);
        let kid = w.net.kid(0);
        let produced = produce(&w.fx, &ledger, &w.class, &job, kid, generated, |trace| {
            bump(&mut trace.values[at.0 as usize][at.1 as usize][at.2 as usize], 1);
        });
        assert_eq!(produced.trace.values.len(), positions as usize - 1);
        let claim = Claim { id: produced.claim.id(), producer: 0, trace: produced.trace, at };
        let (seal, reveal) = seal_and_reveal(&ledger, kid, &produced.object);
        let object = w.net.route(0, &seal);
        w.net.send(vec![(0, object)]).await;
        let object = w.net.route(0, &reveal);
        let chunks = kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&object, 4096).unwrap();
        send_independent(&mut w.net, chunks.unwrap_or_else(|| vec![object]).into_iter().map(|o| (0, o)).collect(), "claim-reveal")
            .await;
        let committed = w.net.daa();
        seed::until_seated(&mut w.net, &claim.id, committed).await;
        w.cover(&claim.id).await;
        println!("G14_SAT_SETUP {}", json!({"claim_index":index,"daa":w.net.daa(),"wall_ns":started.elapsed().as_nanos()}));
        claims.push(claim);
    }
    let outsider = w.outsiders(&claims[0], &w.seats(&claims[0].id), 1)[0];
    let sybils: Vec<_> = (1..8).filter(|card| *card != outsider).collect();
    assert_eq!(sybils.len(), 6);
    let policy = w.policy();
    assert_eq!(policy.max_adjudications_per_block, 64);
    assert_eq!(policy.prosecution_reserved_runs(), 32);
    let setup_ns = started.elapsed().as_nanos();
    let mut rounds = Vec::new();
    for round in 0..2u8 {
        let junk = ProsecutionV1::Kernel(vec![0xEE, round]);
        let mut seals = Vec::new();
        for claim in &claims {
            for card in &sybils {
                let accuser = w.net.kid(*card);
                let object =
                    w.net.route(*card, &K::SealProof { accuser, claim: claim.id, seal: proof_seal_v1(&claim.id, &accuser, &junk) });
                seals.push((*card, object));
            }
        }
        let sealing = send_independent(&mut w.net, seals, "junk-seals").await;
        let mut items = Vec::new();
        for claim in &claims {
            for card in &sybils {
                let accuser = w.net.kid(*card);
                let object = w.net.route(*card, &K::FileProof { accuser, claim: claim.id, proof: junk.clone() });
                items.push((*card, object));
            }
        }
        let (carriers, funding) = funded_carriers(&mut w.net, items, "junk-filings").await;
        let before: Vec<_> = sybils.iter().map(|card| w.net.collateral(*card)).collect();
        let sending = if n == 1 { offer(&mut w.net, carriers, "junk-filings").await } else { Value::Null };
        let fees: Vec<_> = sybils.iter().zip(before).map(|(card, b)| b - w.net.collateral(*card)).collect();
        assert!(claims.iter().all(|claim| !w.net.ledger().claims[&claim.id].convicted));
        rounds.push(json!({"round":round,"offered_proofs":count*sybils.len(),"sealing":sealing,"funding":funding,
            "sending":sending,"dismissal_fees_by_unique_bond":fees}));
    }
    let mut outsider_result = Value::Null;
    if n == 1 {
        let claim = &claims[0];
        let start = Instant::now();
        let finding = w.fresh(0x79).check(claim.id, &claim.published(&w.fx, &[]), &w.fx.params);
        let check_ns = start.elapsed().as_nanos();
        let OutsiderFindingV1::Prosecute(proof) = finding else { panic!("public material must expose the planted lie") };
        let accuser = w.net.kid(outsider);
        let object =
            w.net.route(outsider, &K::SealProof { accuser, claim: claim.id, seal: proof_seal_v1(&claim.id, &accuser, &proof) });
        let mut seals = vec![(outsider, object)];
        let junk = ProsecutionV1::Kernel(vec![0xEE, 2]);
        for target in &claims {
            for card in &sybils {
                let a = w.net.kid(*card);
                let object =
                    w.net.route(*card, &K::SealProof { accuser: a, claim: target.id, seal: proof_seal_v1(&target.id, &a, &junk) });
                seals.push((*card, object));
            }
        }
        let sealing = send_independent(&mut w.net, seals, "cooffered-proof-seals").await;
        let mut items = Vec::new();
        for target in &claims {
            for card in &sybils {
                let a = w.net.kid(*card);
                let object = w.net.route(*card, &K::FileProof { accuser: a, claim: target.id, proof: junk.clone() });
                items.push((*card, object));
            }
        }
        let object = w.net.route(outsider, &K::FileProof { accuser, claim: claim.id, proof });
        // Offer after junk in the input list; native template/fold ordering is retained.
        items.push((outsider, object));
        let sending = send_independent(&mut w.net, items, "cooffered-proof-filings").await;
        outsider_result = json!({"local_check_wall_ns":check_ns,"sealing":sealing,"sending":sending,
            "cooffered_junk_proofs":count*sybils.len(),"input_order":"outsider_after_junk",
            "convicted":w.net.ledger().claims[&claim.id].convicted,"claim_state":format!("{:?}",w.net.claim_state(&claim.id))});
        assert!(w.net.ledger().claims[&claim.id].convicted, "no success if setup/traffic outlives the actual liability window");
    }
    let max_runs = rounds
        .iter()
        .flat_map(|row| row["sending"]["blocks"].as_array().into_iter().flatten())
        .filter_map(|row| row["fold_adjudications"].as_u64())
        .max()
        .unwrap_or(0);
    assert!(max_runs <= 64);
    assert!(n == 0 || max_runs > 0, "a no-adjudication run is not a capacity observation");
    println!(
        "G14_SAT_MEASURE {}",
        json!({"schema":1,"n":n,"claims":count,"trace_positions":positions-1,
        "setup_wall_ns":setup_ns,"total_wall_ns":started.elapsed().as_nanos(),"cap":64,"reserve":32,
        "max_observed_adjudications_per_block":max_runs,"run_cap_saturated":max_runs==64,
        "rounds":rounds,"outsider":outsider_result,"fixture_only":true,"arming_evidence":false,
        "independent_carrier_utxos":true,"template_limits_unchanged":true,
        "continuous_refill_measured":false,"outsider_same_block_priority_measured":false,"gate_c_measured":false})
    );
}
