//! Paired native kernel Panel=3 / Panel=1 measurements on the SAME fixture claim.
//! This is not the legacy model-route shadow, a real model, or a network campaign.
//! Timings include the named native path, not just its cryptographic primitives.
//! The fixture receipts deliberately collude; they are NOT honest verification cost.
use super::*;
use serde_json::{Value, json};
use std::time::Instant;

fn digest(value: &Digest) -> String {
    value.iter().map(|b| format!("{b:02x}")).collect()
}

fn phase(rows: &mut Vec<Value>, name: &str, start: Instant, before: u64, net: &Net, object_bytes: usize) {
    rows.push(json!({"phase": name, "wall_ns": start.elapsed().as_nanos(),
        "daa_before": before, "daa_after": net.daa(), "object_bytes": object_bytes}));
}

async fn send(rows: &mut Vec<Value>, net: &mut Net, name: &str, items: Vec<(usize, Obj)>) {
    let bytes = items.iter().map(|(_, o)| borsh::to_vec(o).unwrap().len()).sum();
    let before = net.daa();
    let start = Instant::now();
    net.send(items).await;
    phase(rows, name, start, before, net, bytes);
}

/// One fresh process per case and seat policy. N=0 constructs the same network,
/// class, job and trace, but sends no claim. Process RSS/CPU must be read externally.
#[tokio::test]
#[ignore = "explicit fixture measurement; use the recorded runner"]
async fn g14_native_panel_measure() {
    kaspa_core::log::try_init_logger("warn");
    let seats: usize = std::env::var("G14_MEASURE_SEATS").unwrap().parse().unwrap();
    assert!([1, 3].contains(&seats));
    let case = std::env::var("G14_MEASURE_CASE").unwrap();
    assert!(["honest", "forged_output", "fake_state", "duplicate", "withheld_reveal"].contains(&case.as_str()));
    let n: usize = std::env::var("G14_MEASURE_N").unwrap().parse().unwrap();
    assert!(n <= 1, "repeat in fresh processes, never reuse mutable claim state");
    let (config, mut bundle, premine, floats) = canonical_config(false);
    let mut params = config.params.clone();
    // Both twins declare the SAME fork schedule and register the class before
    // crossing it. Different schedules bind different ruleset digests into the
    // evidence and therefore cannot produce byte-identical claims.
    const CROSSING_DAA: u64 = 4;
    params.palw_kernel_route_panel_one_v1 = Some(ForkActivation::new(CROSSING_DAA));
    // The fold reads the mirror in its bundle, not the public Params field.
    bundle.state = bundle.state.clone().with_kernel_route_panel_one_from_daa(Some(CROSSING_DAA));
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(node_bundle) = &mut params.palw_consensus_mode {
        node_bundle.state = node_bundle.state.clone().with_kernel_route_panel_one_from_daa(Some(CROSSING_DAA));
    }
    assert!(params.validate_palw_v2().is_err());
    let mut w = World::on(Net::over_cfg((Config::new(params), bundle, premine, floats), TestConsensus::new)).await;
    let job = w.job().await;
    let ledger = w.net.ledger();
    let mut tokens = greedy(&w.fx, &ledger, &w.class, &job.prompt, job.max_new_tokens as usize);
    if case == "forged_output" {
        let last = tokens.len() - 1;
        tokens[last] = (tokens[last] + 1) % misaka_palw_tir_sketch::fixture::FX_V;
    }
    let at = matmul_at(&w.fx.program, 1);
    let produced = produce(&w.fx, &ledger, &w.class, &job, w.net.kid(0), tokens, |trace| {
        if case == "fake_state" {
            bump(&mut trace.values[at.0 as usize][at.1 as usize][at.2 as usize], 1);
        }
    });
    let claim = Claim { id: produced.claim.id(), producer: 0, trace: produced.trace, at };
    let (seal, reveal_object) = seal_and_reveal(&ledger, w.net.kid(0), &produced.object);
    let mut rows = Vec::new();
    let mut verdict = "SETUP_ONLY";
    let mut actual_seats = None;
    let mut final_daa = None;
    let mut admitted_daa = None;
    let mut covered_daa = None;
    let mut final_after_cover_daa = None;
    let mut challenge = Value::Null;
    if seats == 1 {
        w.net.beat_to(CROSSING_DAA).await;
    }
    if n == 1 {
        let o = w.net.route(0, &seal);
        send(&mut rows, &mut w.net, "claim_seal", vec![(0, o)]).await;
        if case == "withheld_reveal" {
            let kid = w.net.kid(0);
            let sealed = w.net.ledger().seals[&(job.id(), kid)].daa;
            let before_collateral = w.net.collateral(0);
            let before = w.net.daa();
            let start = Instant::now();
            w.net.beat_to(sealed + w.policy().seal_ttl_daa + 2).await;
            phase(&mut rows, "seal_expiry", start, before, &w.net, 0);
            assert!(!w.net.ledger().claims.contains_key(&claim.id));
            assert!(!w.net.ledger().seals.contains_key(&(job.id(), kid)));
            assert_eq!(w.net.collateral(0), before_collateral - w.policy().seal_deposit);
            assert_eq!(w.net.owed(0), 0);
            verdict = "UNREVEALED_SEAL_FORFEITED";
        } else {
            let o = w.net.route(0, &reveal_object);
            send(&mut rows, &mut w.net, "claim_reveal_and_assignment", vec![(0, o)]).await;
            assert!(w.net.ledger().claims.contains_key(&claim.id));
            admitted_daa = Some(w.net.daa());
            if seats == 1 {
                assert!(w.net.api().unwrap().assignment_of(&claim.id).is_none(), "no pre-commit seat");
                let before = w.net.daa();
                let start = Instant::now();
                seed::until_seated(&mut w.net, &claim.id, before).await;
                phase(&mut rows, "post_commit_panel_anchor_and_assignment", start, before, &w.net, 0);
            }
            let assigned = w.seats(&claim.id);
            actual_seats = Some(assigned.len());
            assert_eq!(assigned.len(), seats, "measure the requested seat policy, not the test's name");
            let api = w.net.api().unwrap();
            let assignment = api.assignment_of(&claim.id).unwrap();
            challenge = json!({"anchor": format!("{:?}", assignment.challenge_anchor),
                "seed": format!("{:?}", assignment.sample_seed),
                "kind": if seats == 1 { "post_commit_round_seed_anchor" } else { "interim_claim_derived" }});
            let before = w.net.daa();
            let start = Instant::now();
            let receipts = colluding_receipts(&mut w.net, claim.id, before);
            phase(
                &mut rows,
                "receipt_construct_and_sign",
                start,
                before,
                &w.net,
                receipts.iter().map(|(_, o)| borsh::to_vec(o).unwrap().len()).sum(),
            );
            send(&mut rows, &mut w.net, "constraint_receipt_accept", receipts).await;
            assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::ProbabilisticPass { .. }));
            covered_daa = Some(w.net.daa());
            let before = w.net.daa();
            let start = Instant::now();
            let finding = w.fresh(0x66).check(claim.id, &claim.published(&w.fx, &[]), &w.fx.params);
            phase(&mut rows, "outsider_local_check", start, before, &w.net, 0);
            if case == "forged_output" || case == "fake_state" {
                let OutsiderFindingV1::Prosecute(proof) = finding else {
                    panic!("the public verifier detects the lie");
                };
                let outsider = w.outsiders(&claim, &assigned, 1)[0];
                let bytes = borsh::to_vec(&proof).unwrap().len();
                let before = w.net.daa();
                let start = Instant::now();
                Rules::file_proof(&mut w.net, outsider, claim.id, proof).await;
                phase(&mut rows, "public_fault_proof_seal_and_ingress", start, before, &w.net, bytes);
                assert!(w.net.ledger().claims[&claim.id].convicted);
                assert_eq!(w.net.owed(0), 0);
                verdict = "CONVICTED";
            } else {
                assert!(matches!(finding, OutsiderFindingV1::Clean), "honest public check: {finding:?}");
                if case == "duplicate" {
                    let K::CommitClaim { claim: original, evidence, commitments } = produced.object else { unreachable!() };
                    let copied =
                        K::CommitClaim { claim: KernelClaimV1 { producer_bond: w.net.kid(4), ..original }, evidence, commitments };
                    let before = w.net.daa();
                    let start = Instant::now();
                    let (_, accepted) = reveal(&mut w, 4, &copied).await;
                    phase(&mut rows, "duplicate_admission", start, before, &w.net, borsh::to_vec(&copied).unwrap().len());
                    assert!(!accepted);
                    assert_eq!(w.net.ledger().job_claims.get(&job.id()), Some(&claim.id));
                }
                let floor = final_floor(&w, Mode::Panel, &claim.id);
                let before = w.net.daa();
                let start = Instant::now();
                w.net.beat_to(floor).await;
                phase(&mut rows, "advance_to_final", start, before, &w.net, 0);
                assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }));
                final_daa = Some(w.net.daa());
                final_after_cover_daa = Some(w.net.daa() - covered_daa.unwrap());
                verdict = if case == "duplicate" { "COPY_REJECTED_ORIGINAL_FINAL" } else { "FINAL" };
            }
        }
    }
    println!(
        "G14_MEASURE {}",
        json!({"schema": 3, "fixture_only": true, "arming_evidence": false,
        "scope": "native_kernel_interim_panel3_vs_deferred_panel1", "legacy_model_route_compared": false,
        "gate_c_certification_measured": false, "post_commit_beacon_measured": false, "post_commit_panel_anchor_measured": seats == 1 && n == 1 && case != "withheld_reveal",
        "randomness_withholding_measured": false, "class": digest(&w.class), "job": digest(&job.id()),
        "claim": digest(&claim.id), "case": case, "n": n, "requested_seats": seats, "actual_seats": actual_seats,
        "verdict": verdict, "final_daa": final_daa, "admitted_daa": admitted_daa, "covered_daa": covered_daa,
        "final_after_cover_daa": final_after_cover_daa, "panel_one_from_daa": CROSSING_DAA,
        "challenge": challenge, "phases": rows})
    );
}
