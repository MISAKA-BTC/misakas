//! RFC06 complete population through the actual native writer, fold, producer and RPC read.
//! Private harness only: the complete fence bypasses validation after ConfigBuilder; production
//! activation remains refused. Reference entropy does not prove independence or soundness.
//! The resource drill is ignored by default: it generates 100k distinct valid ML-DSA keys and
//! funded collateral, and requires a dedicated runner with RAM/disk measurement. No throughput,
//! pricing or RAM threshold is approved by this fixture. Block-feed replay is not pruned P2P.
use super::*;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::palw_permissionless_panel_v1::{
    NonFraudReasonV1, PalwPermissionlessPanelCompleteV1, WIRE_VERSION_COMPLETE_V1,
};
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_muhash::MuHash;
use std::time::Instant;

const COMPLETE_PERIOD: u64 = 8;
const COMPLETE_WAIT: u64 = 7;

fn complete_policy(limit: u32) -> PanelPolicyV1 {
    PanelPolicyV1 {
        seal_depth_blocks: 1,
        beacon_period_daa: COMPLETE_PERIOD,
        beacon_wait_daa: COMPLETE_WAIT,
        max_candidates: limit,
        max_assignments_per_block: 1,
        max_admissions_per_block: 1,
        ..engine_policy()
    }
}

fn complete_source() -> PalwPanelV3BeaconSourceV1 {
    let PalwPanelV3BeaconSourceV1::Reference { mut events, eligible_profiles, works, sealed } = reference_source() else {
        unreachable!()
    };
    for (i, event) in events.iter_mut().enumerate() {
        event.accepted_position = COMPLETE_PERIOD * (i as u64 + 1) + 1;
        event.settlement_position = event.accepted_position + 1;
    }
    PalwPanelV3BeaconSourceV1::Reference { events, eligible_profiles, works, sealed }
}

/// Config owns ALL native genesis registrations; the harness bundle intentionally keeps only
/// eight signing cards, since t12_genesis_chain asserts its card/key mapping. Added workers are
/// installed by the real processor from Config, not injected into a tip or eligibility vector.
fn complete_armed(extra_workers: u64, limit: u32) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let started = Instant::now();
    let (mut config, mut cards, mut premine, floats) = t12_with_harness_cards();
    let template = cards.genesis_objects.iter().filter(|o| matches!(o, Obj::BondRegistered { .. })).nth(1).unwrap().clone();
    let mut workers = Vec::new();
    for n in 0..extra_workers {
        let mut seed = [0xD6; 32];
        seed[..8].copy_from_slice(&n.to_le_bytes());
        let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed);
        let pubkey = key.verification_key.as_ref().to_vec();
        // Drop each private key immediately: the worker never signs in this drill. The 16-key
        // modulo helper used for harness cards cannot manufacture 100k distinct registered keys.
        drop(key);
        let payout = kaspa_hashes::blake2b_512_address_payload(&pubkey);
        let bond = PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xD600_0000 + n), index: 0 });
        let mut worker = template.clone();
        let Obj::BondRegistered { bond: b, pubkey: pk, operator_pubkey, payout_payload, collateral, .. } = &mut worker else {
            unreachable!()
        };
        *b = bond;
        *operator_pubkey = pubkey.clone();
        *pk = pubkey;
        *payout_payload = Hash64::from_bytes(payout);
        premine.push((bond.0, UtxoEntry::new(*collateral, p2pkh_mldsa87_spk(&payout), 0, false)));
        workers.push(worker);
    }
    let PalwConsensusMode::ConsensusV2(bundle) = &mut config.params.palw_consensus_mode else { unreachable!() };
    bundle.genesis_objects.extend(workers);
    let mut multiset = MuHash::new();
    for (outpoint, entry) in &premine {
        multiset.add_utxo(outpoint, entry);
    }
    config.params.genesis.utxo_commitment = multiset.finalize();
    config.params.genesis.hash = kaspa_consensus_core::header::Header::from(&config.params.genesis).hash;
    config.params.palw_permissionless_panel_complete_v1 =
        Some(PalwPermissionlessPanelCompleteV1 { activation: ForkActivation::new(1), policy: complete_policy(limit) });
    config.params.sync_palw_permissionless_panel_v1();
    assert!(config.params.validate_palw_permissionless_panel_v1().is_err(), "the real activation guard is closed");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    cards.state = bundle.state.clone();
    eprintln!("[rfc06-resource] generated_funded_workers={extra_workers} setup_ms={}", started.elapsed().as_millis());
    (config, cards, premine, floats)
}

fn complete_proof(bundle: &PalwConsensusParamsV2, epoch: u64) -> BeaconProofV1 {
    let mirror = *bundle.state.panel_v3().unwrap();
    let release = epoch * COMPLETE_PERIOD;
    let request = BeaconRequestV1 {
        network: mirror.network,
        ruleset: mirror.ruleset,
        scheme: mirror.policy.beacon_scheme,
        epoch,
        release_daa: release,
        deadline_daa: release + COMPLETE_WAIT,
    };
    let context = panel_beacon_context_v1(&request, &challenge_policy(), BTreeSet::from([PROFILE]));
    let PalwPanelV3BeaconSourceV1::Reference { events, .. } = complete_source() else { unreachable!() };
    let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &events, release + 5).unwrap() else {
        panic!("reference beacon locks");
    };
    BeaconProofV1 { epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(beacon.beacon()).unwrap() }
}

async fn complete_run(extra_workers: u64, limit: u32, expect_refusal: bool, replay: bool) {
    let (config, cards, premine, floats) = complete_armed(extra_workers, limit);
    let total = Instant::now();
    let mut a = t12_genesis_chain(&config, &cards, &premine, &floats);
    *a.vp().panel_v3_test_beacon.lock() = Some((vec![challenge_policy()], complete_source()));
    let genesis = state_of(&a);
    assert_eq!(genesis.bonds_iter().count() as u64, 8 + extra_workers);
    assert!(genesis.bonds_iter().all(|(_, row)| row.pubkey.len() == 2592), "every native registration has a real ML-DSA-87 key");
    drop(genesis);
    eprintln!("[rfc06-resource] native_genesis_ms={}", total.elapsed().as_millis());
    beat_to(&mut a, 1).await;
    let (_, claim) = a.attempt(0, ttpb(&a), Vec::new(), &|_| true).await;
    for _ in 0..10 {
        let state = state_of(&a);
        let row = state.panel_v3().unwrap().claim_rows().get(&claim).unwrap();
        if row.seal.is_some() || matches!(row.phase, ClaimPhaseV3::Voided { .. }) {
            break;
        }
        drop(state);
        a.heartbeat(ttpb(&a), Vec::new()).await;
    }
    let state = state_of(&a);
    let engine = state.panel_v3().unwrap();
    assert_eq!(engine.cursor().version, WIRE_VERSION_COMPLETE_V1);
    let row = engine.claim_rows().get(&claim).unwrap();
    if expect_refusal {
        assert!(matches!(row.phase, ClaimPhaseV3::Voided { reason: NonFraudReasonV1::PopulationResourceLimit, .. }));
        assert!(row.snapshot.is_none() && row.seal.is_none(), "an over-cap population must never freeze a subset");
        let observed = a.vp().palw_panel_v3_observation_v1_impl(vec![claim], 0).unwrap();
        assert_eq!(observed.claims[0].terminal.as_ref().unwrap().reason, "POPULATION_RESOURCE_LIMIT");
    } else {
        assert_eq!(
            row.snapshot.as_ref().unwrap().candidates.len() as u64,
            7 + extra_workers,
            "all capable non-producer bonds enter the seal"
        );
        let epoch = row.seal.as_ref().unwrap().beacon_epoch;
        drop(state);
        let release = epoch * COMPLETE_PERIOD;
        beat_to(&mut a, release + 4).await;
        let proof = complete_proof(&cards, epoch);
        a.heartbeat(ttpb(&a), vec![carrier(&config, &floats, Obj::PanelBeaconProofV3 { proof: Box::new(proof) })]).await;
        a.heartbeat(ttpb(&a), Vec::new()).await;
        beat_to(&mut a, release + COMPLETE_WAIT).await;
        for _ in 0..8 {
            if matches!(state_of(&a).panel_v3().unwrap().claim_rows()[&claim].phase, ClaimPhaseV3::Bound(_)) {
                break;
            }
            a.heartbeat(ttpb(&a), Vec::new()).await;
        }
        let state = state_of(&a);
        assert!(matches!(state.panel_v3().unwrap().claim_rows()[&claim].phase, ClaimPhaseV3::Bound(_)));
        let observed = a.vp().palw_panel_v3_observation_v1_impl(vec![claim], 0).unwrap();
        let snapshot = observed.claims[0].snapshot.as_ref().unwrap();
        assert!(snapshot.population_complete);
        assert_eq!(snapshot.candidates as u64, 7 + extra_workers);
        assert_eq!(observed.overview.retained_candidates, 7 + extra_workers);
        assert_eq!(observed.overview.carriage_version, Some(WIRE_VERSION_COMPLETE_V1));
        assert_eq!(PalwStateCarriageV2::from_state(&state).into_state(&cards.state, Some(state.state_root())).unwrap(), state);
        eprintln!(
            "[rfc06-resource] native_bonds={} candidates={} encoded_engine_bytes={} full_carriage_bytes={} total_ms={}",
            8 + extra_workers,
            snapshot.candidates,
            borsh::to_vec(state.panel_v3().unwrap()).unwrap().len(),
            borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap().len(),
            total.elapsed().as_millis()
        );
    }
    if replay {
        let blocks = chain_blocks(&a, a.sink());
        let mut b = t12_genesis_chain(&config, &cards, &premine, &floats);
        *b.vp().panel_v3_test_beacon.lock() = Some((vec![challenge_policy()], complete_source()));
        for block in blocks {
            arrive(&mut b, block, "complete population replay").await;
        }
        assert_eq!(state_of(&a), state_of(&b));
    }
}

#[tokio::test]
async fn rfc06_complete_population_native_commitment_rpc_restore_and_replay() {
    complete_run(64, 71, false, true).await;
    complete_run(64, 70, true, true).await;
}

/// Resource evidence candidate, not an ordinary unit test. Run with /usr/bin/time -v (Linux)
/// or /usr/bin/time -l (macOS), one test thread, on a dedicated runner with enough disk for native
/// state versions; retain max RSS, times and free-space data. The actual 100k-candidate and
/// 100001-over-cap shapes use real keys and collateral and deliberately do no P2P simulation.
#[tokio::test]
#[ignore = "100k funded native resource drill requires a dedicated measured runner"]
async fn rfc06_complete_population_native_100k_load_and_refusal_boundary() {
    complete_run(99_993, 100_000, false, false).await;
    complete_run(99_994, 100_000, true, false).await;
}
