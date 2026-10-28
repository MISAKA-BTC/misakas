//! Actual DNS reserve slash/accrual and coinbase drip, including cold and DAG branches.
//! Small private economics fixture, not the full RFC12 campaign.
#![allow(warnings)]
use super::*;
use kaspa_database::prelude::{ConnBuilder, DB};
use std::sync::Arc;

// Ordered fields close the final DB handle before its destruction guard even
// when a native assertion unwinds while a roundtrip is in progress.
struct PersistentDb {
    db: Arc<DB>,
    _life: kaspa_database::utils::DbLifetime,
}

#[tokio::test]
async fn rfc12_reserve_drip_reorg_and_cold_restart_conserve_actual_coinbase_utxos() {
    run(true).await;
}

pub(super) async fn run(reorg: bool) {
    use crate::model::stores::headers::HeaderStoreReader;
    use kaspa_consensus_core::{
        Hash64,
        dns_finality::{
            DNS_PAYLOAD_VERSION_V1, SlashingEvidencePayload, compute_slashing_distribution, ready_epoch_from_tip_blue_score,
        },
    };
    kaspa_core::log::try_init_logger("info");
    let config = config();
    let persistent = reorg.then(|| {
        let (life, db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        PersistentDb { db, _life: life }
    });
    let (tx, _rx) = async_channel::unbounded();
    let node = if let Some(persistent) = &persistent {
        TestConsensus::with_db(persistent.db.clone(), &config, tx)
    } else {
        TestConsensus::new(&config)
    };
    let mut ctx = TestContext::new(node);

    let (va, vb) = (dns_harness::harness_validator([0x42u8; 32]), dns_harness::harness_validator([0x43u8; 32]));
    let payload_a: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&va.pubkey).as_bytes();
    let payload_b: [u8; 64] = kaspa_hashes::blake2b_512_address_payload(&vb.pubkey).as_bytes();
    let (spk_a, spk_b) = (p2pkh_mldsa87_spk(&payload_a), p2pkh_mldsa87_spk(&payload_b));

    // Fund: A needs two coinbases (bond + slashing-evidence tx), B two (bond + attestation shard).
    // A block's coinbase pays its MERGESET miner, so mine a batch per miner and SCAN all coinbases.
    let (miner_a, miner_b) = (MinerData::new(spk_a.clone(), vec![]), MinerData::new(spk_b.clone(), vec![]));
    let mut blocks = Vec::new();
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_a.clone(), vec![]).await);
    }
    for _ in 0..4 {
        blocks.push(ctx.mine_block(miner_b.clone(), vec![]).await);
    }
    for _ in 0..5 {
        blocks.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    let (mut a_funds, mut b_funds) = (Vec::new(), Vec::new());
    for blk in &blocks {
        let cb = &blk.transactions[0];
        for (i, o) in cb.outputs.iter().enumerate() {
            let f = (TransactionOutpoint::new(cb.id(), i as u32), o.value, blk.header.daa_score);
            if o.script_public_key == spk_a {
                a_funds.push(f);
            } else if o.script_public_key == spk_b {
                b_funds.push(f);
            }
        }
    }
    assert!(a_funds.len() >= 2 && b_funds.len() >= 2, "need ≥2 funding coinbases each (a={}, b={})", a_funds.len(), b_funds.len());
    let ((cb_a1, va1, da1), (cb_a2, va2, da2)) = (a_funds[0], a_funds[1]);
    let ((cb_b1, vb1, db1), (cb_b2, vb2, db2)) = (b_funds[0], b_funds[1]);

    let storage = ctx.consensus.params().storage_mass_parameter;
    let genesis_hash = ctx.consensus.params().genesis.hash;

    // ── B bonds and HONESTLY attests the ready canonical epoch E ────────────────────────────────
    // B bonds and attests FIRST: A is bonded only later (below), strictly after E's anchor, so A is
    // not part of E's expected-stake denominator — leaving B the sole included validator at E, which
    // makes the drip pay B the WHOLE reserve (a crisp value-conservation assertion). The stake-
    // proportional split when a slashed peer co-existed at the anchor is exercised separately.
    let (bond_b_tx, _, _) = dns_harness::funded_signed_bond_tx([0x43u8; 32], cb_b1, vb1, db1, vb1 - 100_000, 0, storage);
    let bond_b_id = bond_b_tx.id();
    ctx.mine_block(new_miner_data(), vec![bond_b_tx]).await;
    let bond_b_outpoint = TransactionOutpoint::new(bond_b_id, 0);
    // Bury so a ready, bond-active canonical anchor exists for B's epoch E.
    for _ in 0..8 {
        ctx.mine_block(new_miner_data(), vec![]).await;
    }
    let dns = ctx.consensus.params().dns_params.clone().unwrap();
    let sink = ctx.consensus.get_sink();
    let anchor = {
        let vp = ctx.consensus.virtual_processor();
        let sink_blue = vp.headers_store.get_blue_score(sink).unwrap();
        let lr = ready_epoch_from_tip_blue_score(sink_blue, dns.attestation_epoch_length_blue_score, dns.attestation_lag_blue_score)
            .expect("an epoch is ready");
        vp.canonical_anchor_by_blue_score(lr, sink, &dns).expect("canonical anchor for the ready epoch")
    };
    let epoch_e = anchor.epoch;
    let att_b = dns_harness::build_signed_attestation(
        &vb,
        genesis_hash.as_byte_slice(),
        bond_b_outpoint,
        epoch_e,
        anchor.anchor_hash,
        anchor.anchor_daa_score,
        Hash64::default(),
    );
    let shard_b = dns_harness::funded_signed_shard_tx([0x43u8; 32], cb_b2, vb2, db2, att_b, storage);
    let reward_block = ctx.mine_block(new_miner_data(), vec![shard_b]).await;
    assert!(
        reward_block.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b),
        "B must be rewarded for attesting epoch E (so it joins included[E])"
    );

    // ── Accrue the reserve: bond A (AFTER E's anchor) and slash it for equivocation ─────────────
    let bond_a_amount = va1 - 100_000;
    let (bond_a_tx, _, _) = dns_harness::funded_signed_bond_tx([0x42u8; 32], cb_a1, va1, da1, bond_a_amount, 0, storage);
    let bond_a_id = bond_a_tx.id();
    ctx.mine_block(new_miner_data(), vec![bond_a_tx]).await;
    let bond_a_outpoint = TransactionOutpoint::new(bond_a_id, 0);
    // Bury so A's bond is committed into the active bond view the slashing verifier reads.
    let mut buried = Vec::new();
    for _ in 0..6 {
        buried.push(ctx.mine_block(new_miner_data(), vec![]).await);
    }
    // A equivocates an arbitrary epoch (1) DISJOINT from B's epoch E, so A's slash mints no victim
    // output (epoch 1 has no honest included peer) — only the reserve accrues.
    let target_daa = buried[1].header.daa_score;
    let att_a1 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        1,
        Hash64::from_bytes([0xa1u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let att_a2 = dns_harness::build_signed_attestation(
        &va,
        genesis_hash.as_byte_slice(),
        bond_a_outpoint,
        1,
        Hash64::from_bytes([0xb2u8; 64]),
        target_daa,
        Hash64::default(),
    );
    let evidence = SlashingEvidencePayload {
        version: DNS_PAYLOAD_VERSION_V1,
        bond_outpoint: bond_a_outpoint,
        attestation_a: att_a1,
        attestation_b: att_a2,
        reporter_reward_spk_payload: payload_a,
    };
    let slash_tx = dns_harness::funded_signed_slashing_evidence_tx([0x42u8; 32], cb_a2, va2, da2, evidence, storage);
    let slash_block = ctx.mine_block(new_miner_data(), vec![slash_tx]).await;
    assert_eq!(ctx.consensus.block_status(slash_block.header.hash), BlockStatus::StatusUTXOValid, "the slashing block must validate");
    // Settle so the reserve accrual commits (a block's own txs apply once it becomes a selected parent).
    let reserve_settle = ctx.mine_block(new_miner_data(), vec![]).await;
    let dist = compute_slashing_distribution(
        bond_a_amount,
        dns.reward_params.slashing_reporter_reward_bps,
        dns.reward_params.security_reserve_bps,
        dns.reward_params.victim_epoch_pool_bps,
    );
    let reserve_accrued = ctx.consensus.get_dns_reserve_balance(reserve_settle.header.hash).unwrap();
    // Independent integer expression, not the production distribution helper.
    assert_eq!(reserve_accrued, ((bond_a_amount as u128 * 4000) / 10_000) as u64);
    assert_eq!(reserve_accrued, dist.security_reserve_sompi, "A's slash accrues its reserve share to the pool");
    assert!(reserve_accrued > 0, "the reserve must be non-zero to drip");
    assert_eq!(
        ctx.consensus.get_dns_reserve_balance(reserve_settle.header.hash),
        Some(dist.security_reserve_sompi),
        "RFC12 reads the actual positive balance, independently checked against the slashed principal's split"
    );

    // ── Mine until epoch E's DAA-finalization; the drip pays B in that block's coinbase ─────────
    let target_final_daa =
        (epoch_e + 1) * dns.epoch_length_blocks + dns.reward_uniqueness_window_blocks + dns.max_reorg_horizon_blocks;
    let mut drip_block = None;
    for _ in 0..80 {
        let blk = ctx.mine_block(new_miner_data(), vec![]).await;
        // The reserve drip is appended to the coinbase of the block that finalizes epoch E. B got its
        // one-time participation reward at `reward_block` (cross-block dedup blocks re-payment), and the
        // §D worker bounty pays the includer — so the only later coinbase output to B is the drip.
        if blk.transactions[0].outputs.iter().any(|o| o.script_public_key == spk_b) {
            drip_block = Some(blk);
            break;
        }
        if blk.header.daa_score > target_final_daa + 5 {
            break;
        }
    }
    let drip_block = drip_block.expect("a block after the reward must drip the reserve to B at epoch E's finalization");
    let drip_out = drip_block.transactions[0].outputs.iter().find(|o| o.script_public_key == spk_b).expect("drip pays B");
    // The sole included validator B receives the WHOLE reserve (cap is u64::MAX): accrued-in == dripped-out.
    assert_eq!(drip_out.value, reserve_accrued, "the entire accrued reserve drips to the lone included validator B");
    assert_eq!(
        ctx.consensus.get_dns_reserve_balance(drip_block.header.hash),
        Some(0),
        "RFC12 reports known zero after the independently observed coinbase drip exhausts the reserve"
    );
    assert_eq!(
        ctx.consensus.get_dns_reserve_balance(reserve_settle.header.hash),
        Some(reserve_accrued),
        "a later drip does not change the historical block's reserve balance"
    );
    if let Some(PersistentDb { db, _life }) = persistent {
        reorg_and_reopen(ctx, &config, db, &drip_block, &spk_b, reserve_accrued).await;
        drop(_life);
    }
}

fn config() -> kaspa_consensus_core::config::Config {
    ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.max_block_parents = 4;
            p.mergeset_size_limit = 10;
            p.coinbase_maturity = 2;
            let mut dns = DEVNET_PARAMS.dns_params.clone().unwrap();
            dns.dns_activation_daa_score = 0;
            dns.pos_v2_activation_daa_score = 0;
            // epoch_length_blocks == attestation_epoch_length_blue_score (production reality) so the
            // attestation epoch B signs and the DAA epoch the drip finalizes are the same number.
            dns.epoch_length_blocks = 3;
            dns.attestation_epoch_length_blue_score = 3;
            dns.attestation_lag_blue_score = 2;
            dns.attestation_anchor_backoff_blue_score = 1;
            // Reward recency must comfortably cover the canonical anchor (buried by lag + backoff
            // below the tip); finalization_depth = window + max_reorg_horizon = 52.
            dns.reward_uniqueness_window_blocks = 50;
            dns.max_reorg_horizon_blocks = 2;
            dns.stake_score_window_blue_score = 10_000;
            // Isolate the drip: participation takes the full validator pool (quality-bonus pool = 0), so
            // the only post-attestation coinbase output to B is the reserve drip.
            dns.reward_params.validator_participation_bps = 10_000;
            dns.reward_params.slashing_reporter_reward_bps = 1000;
            dns.reward_params.security_reserve_bps = 4000; // 40% of the slashed bond accrues to the reserve
            dns.reward_params.victim_epoch_pool_bps = 4000;
            dns.reward_params.reserve_drip_per_epoch_cap_sompi = u64::MAX; // the whole reserve drips in one epoch
            p.dns_params = Some(dns);
        })
        .build()
}

#[derive(borsh::BorshSerialize, borsh::BorshDeserialize)]
struct Snapshot {
    sink: BlockHash,
    daa: u64,
    balances: Vec<(BlockHash, u64)>,
    utxos: Vec<(TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>,
}
fn snapshot(node: &TestConsensus, rows: &[BlockHash]) -> Snapshot {
    let mut utxos = node.get_virtual_utxos(None, 10_000, false);
    assert!(utxos.len() < 10_000, "the fixture's complete UTXO census must fit");
    utxos.sort_by_key(|(o, _)| (o.transaction_id, o.index));
    Snapshot {
        sink: node.get_sink(),
        daa: node.get_virtual_daa_score(),
        balances: rows.iter().map(|b| (*b, node.get_dns_reserve_balance(*b).expect("actual retained post-state"))).collect(),
        utxos,
    }
}
fn verify_snapshot(node: &TestConsensus, expected: &Snapshot) {
    let actual = snapshot(node, &expected.balances.iter().map(|(b, _)| *b).collect::<Vec<_>>());
    assert_eq!(borsh::to_vec(&actual).unwrap(), borsh::to_vec(expected).unwrap());
    // Exercise the actual bounded acceptance-read consumer too. Its own coherent
    // sink and point reads must match complete native rows and UTXO entries.
    for chunk in expected.balances.chunks(32) {
        let got = node.get_palw_acceptance_reads_v1(chunk.iter().map(|(b, _)| *b).collect(), vec![]).unwrap();
        assert_eq!((got.sink, got.virtual_daa), (expected.sink, expected.daa));
        assert_eq!(got.reserve_balances, chunk.iter().map(|(b, n)| (*b, Some(*n))).collect::<Vec<_>>());
    }
    for chunk in expected.utxos.chunks(32) {
        let got = node.get_palw_acceptance_reads_v1(vec![], chunk.iter().map(|(o, _)| *o).collect()).unwrap();
        assert_eq!(got.sink, expected.sink);
        assert_eq!(got.outpoints, chunk.iter().map(|(o, e)| (*o, Some(e.clone()))).collect::<Vec<_>>());
    }
}
#[tokio::test]
async fn reserve_reader_child() {
    let Some(dir) = std::env::var_os("MISAKA_RESERVE_READER_DB_V1") else { return };
    let input = std::env::var_os("MISAKA_RESERVE_READER_SNAPSHOT_V1").unwrap();
    let expected: Snapshot = borsh::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let db = ConnBuilder::default().with_db_path(dir.into()).with_create_if_missing(false).with_files_limit(10).build().unwrap();
    let mut cfg = config();
    cfg.process_genesis = false;
    let (tx, _rx) = async_channel::unbounded();
    let ctx = TestContext::new(TestConsensus::with_db(db.clone(), &cfg, tx));
    verify_snapshot(&ctx.consensus, &expected);
    drop(ctx);
    drop(db);
}
fn roundtrip(ctx: TestContext, db: Arc<DB>, cfg: &kaspa_consensus_core::config::Config, rows: &[BlockHash]) -> (TestContext, Arc<DB>) {
    let expected = snapshot(&ctx.consensus, rows);
    let time = ctx.simulated_time;
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let control = dir.path().join("expected.borsh");
    std::fs::write(&control, borsh::to_vec(&expected).unwrap()).unwrap();
    let path = db.path().to_path_buf();
    let weak = Arc::downgrade(&db);
    drop(ctx);
    drop(db);
    assert_eq!(weak.strong_count(), 0, "all native DB owners close before a different process reads");
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pipeline::virtual_processor::tests::rfc12_reserve_settlement::reserve_reader_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("MISAKA_RESERVE_READER_DB_V1", &path)
        .env("MISAKA_RESERVE_READER_SNAPSHOT_V1", &control)
        .status()
        .unwrap();
    assert!(status.success(), "actual reserve/UTXO cold reader failed: {status}");
    let db = ConnBuilder::default().with_db_path(path).with_create_if_missing(false).with_files_limit(10).build().unwrap();
    let mut cfg = cfg.clone();
    cfg.process_genesis = false;
    let (tx, _rx) = async_channel::unbounded();
    let mut ctx = TestContext::new(TestConsensus::with_db(db.clone(), &cfg, tx));
    ctx.simulated_time = time;
    verify_snapshot(&ctx.consensus, &expected);
    (ctx, db)
}
#[allow(clippy::too_many_arguments)]
async fn reorg_and_reopen(
    mut ctx: TestContext,
    cfg: &kaspa_consensus_core::config::Config,
    mut db: Arc<DB>,
    drip: &Block,
    payee: &kaspa_consensus_core::tx::ScriptPublicKey,
    accrued: u64,
) {
    let chain = ctx.consensus.get_virtual_chain_from_block(cfg.params.genesis.hash, None).unwrap();
    assert!(chain.removed.is_empty());
    let mut rows = chain.added;
    let parent = ctx.consensus.get_ghostdag_data(drip.header.hash).unwrap().selected_parent;
    let fork_at = rows.iter().position(|b| *b == parent).expect("fork is the actual drip parent");
    let mut branch = TestContext::new(TestConsensus::new(cfg));
    for b in &rows[..=fork_at] {
        let block = ctx.consensus.get_block(*b).unwrap();
        branch.validate_and_insert_block(block.clone()).await;
        assert_eq!(branch.consensus.block_status(*b), BlockStatus::StatusUTXOValid);
        branch.simulated_time = block.header.timestamp;
    }
    assert_eq!(branch.consensus.get_dns_reserve_balance(parent), Some(accrued));
    let old = drip.transactions[0]
        .outputs
        .iter()
        .enumerate()
        .filter(|(_, o)| &o.script_public_key == payee)
        .map(|(i, _)| TransactionOutpoint::new(drip.transactions[0].id(), i as u32))
        .collect::<Vec<_>>();
    assert_eq!(old.len(), 1);
    // Settle the original coinbase and positively observe its spendable entry.
    ctx.mine_block(new_miner_data(), vec![]).await;
    assert_eq!(
        ctx.consensus.get_palw_acceptance_reads_v1(vec![], old.clone()).unwrap().outpoints[0].1.as_ref().unwrap().amount,
        accrued
    );
    rows.push(ctx.consensus.get_sink());
    (ctx, db) = roundtrip(ctx, db, cfg, &rows);
    let other_miner = MinerData::new(p2pkh_mldsa87_spk(&[0xDD; 64]), vec![0xDD]);
    let mut alternative = Vec::new();
    let mut new_drips = Vec::new();
    for _ in 0..5 {
        let b = branch.mine_block(other_miner.clone(), vec![]).await;
        assert_eq!(branch.consensus.block_status(b.header.hash), BlockStatus::StatusUTXOValid);
        for (i, o) in b.transactions[0].outputs.iter().enumerate() {
            if &o.script_public_key == payee {
                new_drips.push((TransactionOutpoint::new(b.transactions[0].id(), i as u32), o.value));
            }
        }
        alternative.push(b);
    }
    assert_eq!(new_drips.iter().map(|(_, v)| v).sum::<u64>(), accrued, "independent branch also spends its original reserve once");
    for b in &alternative {
        ctx.validate_and_insert_block(b.clone()).await;
        rows.push(b.header.hash);
    }
    assert_eq!(ctx.consensus.get_sink(), branch.consensus.get_sink(), "actual heavier native branch wins");
    for b in &alternative {
        assert_eq!(
            ctx.consensus.block_status(b.header.hash),
            BlockStatus::StatusUTXOValid,
            "all selected branch bodies must actually validate after the sink walk reaches them"
        );
    }
    ctx.simulated_time = ctx.simulated_time.max(branch.simulated_time);
    assert_eq!(ctx.consensus.get_dns_reserve_balance(ctx.consensus.get_sink()), Some(0));
    let points = old.iter().copied().chain(new_drips.iter().map(|(o, _)| *o)).collect::<Vec<_>>();
    let got = ctx.consensus.get_palw_acceptance_reads_v1(vec![], points).unwrap();
    let live: u64 = got.outpoints.iter().filter_map(|(_, e)| e.as_ref().map(|e| e.amount)).sum();
    eprintln!("RESERVE_REORG actual coinbase drips live={live} accrued={accrued} entries={:?}", got.outpoints);
    assert_eq!(live, accrued, "reorg cannot leave two spendable payments of the same reserve obligation");
    drop(branch);
    (ctx, db) = roundtrip(ctx, db, cfg, &rows);
    // No new attestation/slash or reserve may make a second payment after restart.
    for _ in 0..4 {
        let b = ctx.mine_block(new_miner_data(), vec![]).await;
        assert!(!b.transactions[0].outputs.iter().any(|o| &o.script_public_key == payee));
        assert_eq!(ctx.consensus.get_dns_reserve_balance(b.header.hash), Some(0));
    }
    drop(ctx);
    drop(db);
}
