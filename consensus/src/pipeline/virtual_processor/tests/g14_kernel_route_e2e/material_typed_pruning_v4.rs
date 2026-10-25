//! Long real pruning regression under the inherited 74,920-DAA claim lattice.
//! Separate from the four quick filters, never ignored or replaced by fake deltas.
use super::*;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_core::{
    palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2},
    palw_panel_v2::{PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2},
    palw_state_v2::PalwClaimPhaseV2,
};
use std::{io::Write, path::Path};

#[path = "material_typed_pruning_readiness_v4.rs"]
mod readiness;

const INHERITED_DEPTH: u64 = 74_920;
const MAX_BEATS: u64 = 2 * INHERITED_DEPTH + 10_000;

#[test]
fn rfc08_pruned_typed_terminal_real_native_pruning_cold_receipt_and_all_generation_gc() {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &child_name("typed_writer_child"), "--nocapture", "--test-threads=1"])
        .env("MISAKA_TYPED_WRITER_CHILD_V4", "1")
        .env("MISAKA_TYPED_REPLACEMENT_CHILD_V4", "1")
        .env("MISAKA_TYPED_TERMINAL_CHILD_V4", "1")
        .env("MISAKA_TYPED_PRUNING_CHILD_V4", "1")
        .status()
        .unwrap();
    assert!(status.success(), "actual unmodified-depth Typed pruning role failed: {status}");
}

fn progress(line: String) {
    let path = std::env::var_os("MISAKA_TYPED_PRUNING_PROGRESS_V4")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("misaka-rfc08-typed-native-pruning-progress.log"));
    assert!(path.is_absolute());
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
    writeln!(file, "{line}").unwrap();
    file.sync_all().unwrap();
    eprintln!("TYPED_PRUNING_V4 artifact={path:?} {line}");
}
fn disk_bytes(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            // Compaction can unlink an SST after directory enumeration. This is
            // approximate operational telemetry, never a native absence witness.
            let metadata = match entry.metadata() {
                Ok(value) => value,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return 0,
                Err(error) => panic!("DB size telemetry: {error}"),
            };
            if metadata.is_dir() {
                disk_bytes(&entry.path())
            } else if metadata.is_file() {
                metadata.len()
            } else {
                0
            }
        })
        .sum()
}
fn free_bytes(path: &Path) -> u64 {
    let out = std::process::Command::new("df").args(["-Pk"]).arg(path).env("LC_ALL", "C").output().unwrap();
    assert!(out.status.success());
    let text = std::str::from_utf8(&out.stdout).unwrap();
    text.lines().last().unwrap().split_whitespace().nth(3).unwrap().parse::<u64>().unwrap().checked_mul(1024).unwrap()
}
pub(super) fn started(net: &Net, db: &Path) {
    // Opt only this node into the existing harness clock; arm_clock advances
    // it for every subsequent native block without changing any tolerance.
    net.chain.ctx.consensus.set_validation_time_for_test(net.chain.ctx.simulated_time);
    assert!(net.chain.ctx.consensus.has_validation_time_for_test());
    let p = &net.config.params;
    assert_eq!(p.pruning_depth(), INHERITED_DEPTH);
    assert_eq!(p.finality_depth(), 20, "existing private Typed fixture finality, not a new pruning override");
    assert!(!net.config.is_archival);
    assert_eq!(net.config.retention_period_days, None);
    assert_eq!(p.pruning_proof_m, 1000);
    let lattice =
        kaspa_consensus_core::config::params::palw_v2_claim_lattice_daa_v1(&net.bundle, p.palw_da_court, p.palw_class_verify_deadline);
    assert_eq!(lattice, INHERITED_DEPTH);
    assert_eq!(
        kaspa_consensus_core::config::params::palw_v2_pruning_depth_v1(
            &p.blockrate,
            &net.bundle,
            p.palw_da_court,
            p.palw_class_verify_deadline
        ),
        INHERITED_DEPTH
    );
    progress(format!(
        "event=start pid={} depth={} lattice={} finality={} archival=false retention=None proof_m=1000 simulated_clock=true max_beats={} db={db:?} disk_bytes={} free_bytes={}",
        std::process::id(),
        p.pruning_depth(),
        lattice,
        p.finality_depth(),
        MAX_BEATS,
        disk_bytes(db),
        free_bytes(db)
    ));
}

// Same-amount private ownership seam used by canonical/seat-maturity fixtures.
// No public wallet key is implied and no amount/outpoint is minted or rewritten.
pub(super) fn private_funding_config(mut config: Config, mut premine: Premine) -> (Config, Premine) {
    use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, premine_outpoint_for};
    let before = premine.clone();
    let amounts: Vec<_> = before.iter().map(|(o, e)| (*o, e.amount)).collect();
    let total = before.iter().try_fold(0u128, |sum, (_, e)| sum.checked_add(e.amount as u128)).unwrap();
    let main = premine_outpoint_for(config.params.net, MAIN_PREMINE_INDEX);
    let (_, funding) = premine.iter_mut().find(|(o, _)| *o == main).expect("existing main premine UTXO");
    assert!(!funding.is_coinbase);
    let amount = funding.amount;
    funding.script_public_key = card_payout_spk(8);
    assert_eq!(premine.iter().map(|(o, e)| (*o, e.amount)).collect::<Vec<_>>(), amounts);
    assert_eq!(premine.iter().try_fold(0u128, |sum, (_, e)| sum.checked_add(e.amount as u128)).unwrap(), total);
    for ((oldpoint, old), (point, entry)) in before.iter().zip(&premine) {
        assert_eq!(oldpoint, point);
        assert_eq!((old.amount, old.block_daa_score, old.is_coinbase), (entry.amount, entry.block_daa_score, entry.is_coinbase));
        if *point != main {
            assert_eq!(old, entry, "only explicit private main ownership changes");
        }
    }
    let old_genesis = config.params.genesis.hash;
    let mut commitment = kaspa_muhash::MuHash::new();
    for (outpoint, entry) in &premine {
        commitment.add_utxo(outpoint, entry);
    }
    config.params.genesis.utxo_commitment = commitment.finalize();
    config.params.genesis.hash = kaspa_consensus_core::header::Header::from(&config.params.genesis).hash;
    progress(format!(
        "event=private_main_ownership pid={} outpoint={main:?} amount={amount} total={total} amount_preserved=true owner_key=8 public_ownership=false old_genesis={old_genesis} private_genesis={}",
        std::process::id(),
        config.params.genesis.hash
    ));
    (config, premine)
}

// Native collateral is immutable: spend an existing, explicitly private-owned
// funded UTXO into a newly signed registration, without relaxing PESG caps.
async fn funded_frontier_bond(net: &mut Net) -> usize {
    use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, premine_outpoint_for};
    use kaspa_consensus_core::palw_bond_budget_v1::palw_bond_budget_caps_v1;
    use kaspa_consensus_core::palw_state_v2::{
        PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, palw_bond_registration_message_v2,
        palw_operator_possession_message_v1,
    };
    const CARD: usize = 8;
    const FEE: u64 = 1_000_000_000;
    assert_eq!(net.chain.bonds.len(), CARD);
    let policy = net.config.params.palw_bond_budget_v1.as_ref().unwrap().policy.clone();
    let original = net.chain.ctx.consensus.palw_producer_facts_v2(net.bundle.base_class_id, Some(net.bond(6).0)).unwrap();
    // The anchor itself must be admitted while the first claim remains open.
    let required_weight = (original.pwu as u128).checked_mul(2).unwrap();
    let required =
        required_weight.checked_mul(policy.capital_unit_sompi as u128).unwrap().div_ceil(policy.final_weight_per_unit as u128);
    let required = u64::try_from(required).unwrap().max(net.bundle.bond.min_collateral_sompi());
    let existing = net.chain.tip_state().1.bond(&net.bond(6)).unwrap().collateral;
    progress(format!(
        "event=frontier_capital_needed actual_pwu={} old_collateral={existing} old_final_cap={} needed_collateral={required} needed_final_cap={}",
        original.pwu,
        palw_bond_budget_caps_v1(&policy, existing).final_weight,
        palw_bond_budget_caps_v1(&policy, required).final_weight
    ));
    assert!(palw_bond_budget_caps_v1(&policy, required).final_weight >= required_weight);
    let outpoint = premine_outpoint_for(net.config.params.net, MAIN_PREMINE_INDEX);
    let entry = Net::utxos_of(&net.chain).remove(&outpoint).expect("actual private main premine is still unspent");
    assert!(!entry.is_coinbase, "never infer income from zero-subsidy heartbeats");
    assert_eq!(entry.script_public_key, card_payout_spk(CARD));
    assert!(entry.amount > required.checked_add(FEE).unwrap());
    let key = TestConsensus::palw_v2_registry_keypair(CARD as u64);
    let pubkey = key.verification_key.as_ref().to_vec();
    let payout = Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&pubkey).as_bytes());
    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        net.config.params.net.to_string().as_bytes(),
        Some(net.config.params.genesis.hash),
    );
    let signed = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::default(), 1));
    let classes = std::collections::BTreeSet::from([net.bundle.base_class_id]);
    let message = palw_bond_registration_message_v2(network, &signed, &pubkey, &pubkey, required, &payout, &classes);
    let possession = palw_operator_possession_message_v1(network, &signed, &pubkey, &pubkey);
    let mut signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT,
        [0xBA; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    signature.extend(
        libcrux_ml_dsa::ml_dsa_87::sign(
            &key.signing_key,
            possession.as_byte_slice(),
            PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT,
            [0xBB; 32],
        )
        .unwrap()
        .as_ref(),
    );
    let object = Obj::BondRegistered {
        bond: signed,
        pubkey: pubkey.clone(),
        operator_pubkey: pubkey.clone(),
        collateral: required,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
    let input_value = entry.amount;
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![
            TransactionOutput::new(entry.amount - required - FEE, card_payout_spk(CARD)),
            TransactionOutput::new(required, card_payout_spk(CARD)),
        ],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, CARD, net.config.params.storage_mass_parameter);
    net.mempool(&tx).expect("actual existing private-funded collateral carrier must be accepted");
    let interval = net.ttpb();
    let carrying = net.chain.heartbeat(interval, vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|candidate| candidate.id() == tx.id()));
    net.chain.heartbeat(interval, vec![]).await;
    let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 1));
    let registered = net.chain.tip_state().1.bond(&bond).expect("actual signed funded registration must fold").clone();
    assert_eq!(registered.collateral, required);
    net.chain.bonds.push(bond);
    progress(format!(
        "event=frontier_registration bond={bond:?} input_value={input_value} collateral={required} change={} fee={FEE} registered_daa={}",
        input_value - required - FEE,
        registered.registered_daa
    ));
    let maturity = net.config.params.palw_bond_maturity.as_ref().map_or(0, |f| f.window_daa);
    let mature_at = registered.registered_daa.checked_add(maturity).unwrap();
    // Observed native heartbeats advance one DAA per two blocks here; wait for
    // the actual DAA/readiness, never substitute a loop count for maturity.
    for beat in 0..maturity.saturating_mul(4).saturating_add(100) {
        let facts = net.chain.ctx.consensus.palw_producer_facts_v2(net.bundle.base_class_id, Some(bond.0)).unwrap();
        if net.daa() >= mature_at && facts.ready_to_produce(&pubkey).is_ok() {
            assert!(
                palw_bond_budget_caps_v1(&policy, net.chain.tip_state().1.bond(&bond).unwrap().collateral).final_weight
                    >= (facts.pwu as u128).checked_mul(2).unwrap()
            );
            progress(format!(
                "event=frontier_funded_bond bond={bond:?} daa={} registered_daa={} collateral={required} final_cap={} actual_pwu={}",
                net.daa(),
                registered.registered_daa,
                palw_bond_budget_caps_v1(&policy, required).final_weight,
                facts.pwu
            ));
            return CARD;
        }
        if beat % 1000 == 0 {
            progress(format!(
                "event=frontier_maturity beats={beat} daa={} mature_at={mature_at} readiness={:?}",
                net.daa(),
                facts.ready_to_produce(&pubkey)
            ));
        }
        net.chain.heartbeat(interval, vec![]).await;
    }
    panic!("actual funded ninth bond did not satisfy native maturity/readiness; no policy or balance override");
}

// The kernel claim's successful local material lifecycle is NOT an outer PALW
// Final/frontier. Obtain a separate real native floor claim and the node's own
// selected Panel's signed quorum; never plant or override the pruning frontier.
async fn native_frontier(net: &mut Net, release: Hash64) -> Hash64 {
    let interval = net.ttpb();
    let producer = funded_frontier_bond(net).await;
    let (attempt, id) = net.chain.attempt(producer, interval, vec![], &|_| true).await;
    assert!(
        net.chain.tip_state().1.claim(&id).is_some(),
        "actual funded native attempt {id} in {} must be admitted before any anchor/quorum",
        attempt.header.hash
    );
    net.chain.attempt_at_the_anchor_slot(id, producer).await;
    let state = net.chain.tip_state().1;
    let panel = state.panel(&id).expect("actual selected native panel");
    let signed_daa = net.chain.daa_of(net.chain.sink());
    let network = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        net.config.params.net.to_string().as_bytes(),
        Some(net.config.params.genesis.hash),
    );
    let receipts: Vec<_> = panel
        .seats
        .iter()
        .take(net.bundle.panel.quorum() as usize)
        .map(|seat| {
            let card = net.chain.bonds.iter().position(|bond| *bond == seat.bond).unwrap();
            let message = palw_receipt_message_v2(network, id, PalwReceiptVerdictV2::Valid, signed_daa);
            let key = TestConsensus::palw_v2_registry_keypair(card as u64);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &key.signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0xB9; 32],
            )
            .unwrap()
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    drop(state);
    let object = net
        .chain
        .vp()
        .palw_v2_receipt_quorum_assemble_impl(id, &receipts)
        .expect("own node verifies actual selected quorum signatures");
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
    let (outpoint, entry) = net.funding[6].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(entry.amount - 300_000, card_payout_spk(6))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, 6, net.config.params.storage_mass_parameter);
    net.mempool(&tx).expect("current actual float funds native quorum carrier");
    let carrying = net.chain.heartbeat(interval, vec![tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == tx.id()));
    for _ in 0..(net.bundle.state.window_receipt() + net.bundle.state.window_challenge() + 1000) {
        if matches!(net.chain.tip_state().1.claim(&id).unwrap().phase, PalwClaimPhaseV2::Final { .. }) {
            let state = net.chain.tip_state().1;
            let (frontier, _) = state.safe_frontier();
            let release_blue = net.chain.vp().headers_store.get_blue_score(release).unwrap();
            assert!(frontier > release_blue, "real native Final must permit a point beyond the Release body");
            progress(format!("event=native_final claim={id} frontier_blue={frontier} release_blue={release_blue} daa={}", net.daa()));
            return id;
        }
        net.chain.heartbeat(interval, vec![]).await;
    }
    panic!("actual selected-quorum native claim never became Final; do not bypass pruning frontier");
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn advance(
    w: &mut World,
    db: &Path,
    dir: &Path,
    root: Hash64,
    initial: Hash64,
    replacement: Hash64,
    tail: Hash64,
    release: Hash64,
) {
    started(&w.net, db);
    let retained: Vec<_> = [2, 3, 4, 5]
        .into_iter()
        .flat_map(|card| {
            let file = dir.join(format!("provider-{card}"));
            [file.clone(), file.with_extension("kernel-context-v4")]
        })
        .map(|file| {
            let bytes = std::fs::read(&file).unwrap();
            (file, bytes)
        })
        .collect();
    assert!(w.net.chain.vp().palw_state_v2_store.read().has_delta(release).unwrap());
    assert!(w.net.chain.ctx.consensus.get_block(release).is_ok(), "Release body exists before actual pruning");
    let native_claim = native_frontier(&mut w.net, release).await;
    let interval = w.net.ttpb();
    let start = w.net.daa();
    let release_daa = w.net.chain.vp().headers_store.get_daa_score(release).unwrap();
    let release_blue = w.net.chain.vp().headers_store.get_blue_score(release).unwrap();
    let mut done = false;
    for beat in 0..=MAX_BEATS {
        let point = w.net.chain.ctx.consensus.pruning_point();
        let point_daa = w.net.chain.vp().headers_store.get_daa_score(point).unwrap();
        let delta = w.net.chain.vp().palw_state_v2_store.read().has_delta(release).unwrap();
        let body = w.net.chain.vp().block_transactions_store.has(release).unwrap();
        if beat % 1000 == 0 || !delta || !body {
            let disk = disk_bytes(db);
            let free = free_bytes(db);
            progress(format!(
                "event=progress beats={beat} start_daa={start} daa={} pp={point} pp_daa={point_daa} release_delta={delta} release_body={body} disk_bytes={disk} free_bytes={free}",
                w.net.daa()
            ));
            assert!(
                free >= 8 << 30,
                "actual filesystem free space below explicit local 8GiB long-fixture floor; no quota guarantee claimed"
            );
        }
        if !delta && !body {
            assert!(point_daa > release_daa);
            done = true;
            break;
        }
        assert!(beat < MAX_BEATS, "real pruner did not remove Release body/delta under unmodified claim lattice");
        w.net.chain.heartbeat(interval, vec![]).await;
    }
    assert!(done);
    for (file, bytes) in retained {
        assert_eq!(
            std::fs::read(file).unwrap(),
            bytes,
            "actual native pruning cannot delete retained provider material or its context"
        );
    }
    let state = w.net.chain.tip_state().1;
    assert!(state.safe_frontier().0 > release_blue);
    // Old claim rows may retire naturally before this long lattice expires;
    // the actual Final was required above and its primary frontier persists.
    for consent in [initial, replacement] {
        let (_, receipt) = w
            .net
            .chain
            .ctx
            .consensus
            .palw_material_terminal_receipt_finalized_v2(root, consent)
            .unwrap()
            .expect("positive native receipt survives real physical pruning");
        assert!(receipt.covers(root, consent));
        assert_eq!((receipt.tail_block, receipt.release_block), (tail, release));
    }
    // The target point is published before capture and pruning complete. Stop
    // foreground mining and require exact serving readiness/finished markers.
    let (point, carriage) = readiness::drain(&w.net, db).await;
    assert_eq!(carriage.last_point.as_ref().unwrap().block, point);
    progress(format!(
        "event=physical_prune_complete daa={} pp={point} release={release} native_claim={native_claim} disk_bytes={}",
        w.net.daa(),
        disk_bytes(db)
    ));
}
