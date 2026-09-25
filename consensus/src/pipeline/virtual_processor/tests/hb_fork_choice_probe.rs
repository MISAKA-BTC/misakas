//! **Can a heartbeat miner's private fork carry a double spend on testnet-12? — built and measured
//! through the real pipeline** (the 2026-09-25 question; a probe, no rule is changed).
//!
//! Two nodes on one testnet-12 ruleset (the harness cards and the inert EVM lane of
//! `t12_round_lane_e2e`, every window as shipped): the VICTIM follows the public chain, which pays the
//! merchant with X; the ATTACKER saw the same prefix, then built a private branch from the same
//! parent carrying Y — the same UTXO (card 1's genesis fee float) paid back to itself — and releases
//! it to the victim block by block. After every released block the victim's own stores are read: its
//! sink, the blue work of both tips above the fork point, and the PALW deep-reorg keys
//! (`palw_candidate_order_v2`) that `dns_reorg_outcome` hands `decide_deep_reorg_v2`. The victim's
//! UTXO set is then asked whether X's or Y's output survived.
//!
//! The regimes, as the question poses them:
//! * (a) the public chain carries floor attempts (2^20 of blue work each) — `hb_probe_a_…`;
//! * (b) a heartbeat-only period: a private branch with more beats a slot (sibling layers up to the
//!   mergeset bound) — `hb_probe_b_sibling_…` — or run ahead of wall time by future stamps at the
//!   132 s and 1,620 s tolerances — `hb_probe_b_future_…`;
//! * (c) an attacker who also holds a bond and mints attempts on the private branch — `hb_probe_c_…`;
//! * (iii) a beat refused as too far in the future, offered again once wall time has moved —
//!   `hb_probe_iii_…`;
//! * what stops it: the finality depth (`hb_probe_d_…`) and a PALW Settlement Anchor after X whose
//!   claim is `Final` (`hb_probe_e_…`, ADR-0127/0129).
//!
//! Every block is stamped by the harness's simulated clock (EVM inert, as in every t12 harness that
//! stamps), PoW is skipped (the heartbeat's 2^24 is a price, not a rule this probe exercises) and the
//! class lottery is ground by the harness exactly as `T12Chain::attempt` grinds it — on a live chain
//! each such draw is an inference.
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::model::stores::dns_state::DnsStateStoreReader;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, TemplateBuildMode};
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::palw_fork_authority_v2::decide_deep_reorg_v2;
use kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS as I;
use kaspa_consensus_core::tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use std::collections::HashSet;

/// The victim, the attacker, and the two payments that spend one outpoint.
struct Duel {
    config: Config,
    victim: T12Chain,
    attacker: T12Chain,
    x: Transaction,
    y: Transaction,
    nonce: u64,
    /// The eight cards' genesis fee floats: card 1's is X's and Y's coin, card 0's funds the receipt
    /// carrier in `hb_probe_e_…`.
    floats: Vec<(TransactionOutpoint, UtxoEntry)>,
}

/// testnet-12 as the harness runs it, optionally at another `timestamp_deviation_tolerance` — the one
/// field the mainnet-values branch moves for this question (132 s -> 1,620 s). Both nodes start at
/// the same genesis with the premine imported.
fn duel(tolerance_s: Option<u64>) -> Duel {
    duel_armed(tolerance_s, None)
}

/// [`duel`], with F1's same-chain fence (`Params::palw_heartbeat_transparent_same_chain`, ADR-0105
/// §11) armed at DAA `same_chain` on BOTH nodes — `None` is testnet-12 as launched.
fn duel_armed(tolerance_s: Option<u64>, same_chain: Option<u64>) -> Duel {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let config = match tolerance_s {
        Some(t) if t != config.params.timestamp_deviation_tolerance => {
            let mut params = config.params.clone();
            params.timestamp_deviation_tolerance = t;
            ConfigBuilder::new(params).skip_proof_of_work().build()
        }
        _ => config,
    };
    let config = with_same_chain(&config, same_chain);
    config.params.validate_palw_v2().expect("a runnable ruleset");
    let victim = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let attacker = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let pay = |to: ScriptPublicKey| {
        let (outpoint, entry) = floats[1].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - 300_000, to)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign_spend(&mut tx, entry, 1, config.params.storage_mass_parameter);
        tx
    };
    // X pays the merchant (card 5's address stands in for it); Y pays the same coin back (card 6's).
    let x = pay(card_payout_spk(5));
    let y = pay(card_payout_spk(6));
    assert_eq!(x.inputs[0].previous_outpoint, y.inputs[0].previous_outpoint, "X and Y spend one outpoint");
    assert_ne!(x.id(), y.id());
    Duel { config, victim, attacker, x, y, nonce: 1 << 40, floats }
}

fn bw(c: &T12Chain, h: BlockHash) -> i128 {
    c.vp().ghostdag_store.get_blue_work(h).unwrap().as_u128() as i128
}

fn bs(c: &T12Chain, h: BlockHash) -> u64 {
    c.vp().ghostdag_store.get_blue_score(h).unwrap()
}

fn ts(c: &T12Chain, h: BlockHash) -> u64 {
    c.vp().headers_store.get_timestamp(h).unwrap()
}

/// Is `outpoint` in `c`'s virtual UTXO set?
fn has_utxo(c: &T12Chain, outpoint: TransactionOutpoint) -> bool {
    c.ctx.consensus.get_virtual_utxos(Some(outpoint), 1, false).first().is_some_and(|(o, _)| *o == outpoint)
}

async fn mirror(to: &mut T12Chain, block: &Block) {
    let hash = block.header.hash;
    to.ctx
        .consensus
        .validate_and_insert_block(block.clone())
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("mirror {hash}: {e}"));
    to.ctx.simulated_time = to.ctx.simulated_time.max(block.header.timestamp);
}

/// One honest slot, as the H1 miner mines it: a beat one second after the last block, which the
/// adapter stamps at the slot if the slot is later (the holder), and beats one second apart until one
/// steps the clock. `txs` ride the first beat. Past the first slot it is exactly two beats.
async fn honest_slot(c: &mut T12Chain, txs: Vec<Transaction>) -> Vec<Block> {
    let start = c.daa_of(c.sink());
    let mut out = vec![c.heartbeat(1_000, txs).await];
    while c.daa_of(c.sink()) == start {
        assert!(out.len() < 4, "an honest slot steps within four beats");
        out.push(c.heartbeat(1_000, Vec::new()).await);
    }
    out
}

async fn honest_slot_mirrored(d: &mut Duel) -> Vec<Block> {
    let blocks = honest_slot(&mut d.victim, Vec::new()).await;
    for b in &blocks {
        mirror(&mut d.attacker, b).await;
    }
    blocks
}

/// `m` sibling heartbeats on `a`'s current virtual — one template, `m` nonces, so one set of parents —
/// stamped by the lane's adapter at `max(clock, the slot)` and inserted into `a`'s own node. `Err` is
/// the first sibling's refusal (nothing is inserted then).
async fn layer(a: &mut T12Chain, nonce: &mut u64, m: usize, clock: u64, txs: Vec<Transaction>) -> Result<Vec<Block>, RuleError> {
    let mut t = a
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(txs)), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&a.config.params, &mut t.block.header, clock);
    t.block.header.finalize();
    let (t, _) = a.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
    let mut out = Vec::with_capacity(m);
    for _ in 0..m {
        *nonce += 1;
        let mut b = t.block.clone();
        b.header.nonce = *nonce;
        b.header.finalize();
        let block = b.to_immutable();
        a.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await?;
        a.ctx.simulated_time = a.ctx.simulated_time.max(block.header.timestamp);
        out.push(block);
    }
    Ok(out)
}

/// One private slot on the honest cadence (holder one second after the last block, raised to the
/// slot; steps one second later) with `m` siblings in each layer — `m` = 1 is the honest shape, 4 is
/// the mergeset bound. `txs` ride a single holder.
async fn private_slot(a: &mut T12Chain, nonce: &mut u64, m: usize, txs: Vec<Transaction>) -> Vec<Block> {
    let start = a.daa_of(a.sink());
    let holders = if txs.is_empty() { m } else { 1 };
    let clock = a.ctx.simulated_time + 1_000;
    let mut out = layer(a, nonce, holders, clock, txs).await.expect("the private holders");
    assert_eq!(a.daa_of(out[0].header.hash), start, "a holder does not tick");
    let clock = a.ctx.simulated_time + 1_000;
    let steps = layer(a, nonce, m, clock, Vec::new()).await.expect("the private steps");
    assert!(steps.iter().all(|s| a.daa_of(s.header.hash) == start + 1), "every step ticks once");
    out.extend(steps);
    out
}

/// What the victim did as the private branch arrived.
#[derive(Debug, Default)]
struct Released {
    /// Released blocks the victim refused, with the refusal.
    refused: Vec<(usize, String)>,
    /// The first released block whose blue work put a private tip at the top of the sink search's heap.
    offered_at: Option<usize>,
    /// The first released block after which the victim's sink was a private block.
    flipped_at: Option<usize>,
    /// Where the release alone did not flip it: the honest public beats mined on the victim after the
    /// release until its sink moved to the private branch — each one gives the deep-reorg gate a new
    /// incumbent hash to compare against (`None`: not needed, or not within the allowance).
    flipped_after_public_beats: Option<usize>,
    /// Released blocks that reached the top of the heap and did NOT become the sink.
    vetoed_while_offered: usize,
    /// Blue work above the fork point: the public tip's, and the heaviest private block's.
    public_bw: i128,
    private_bw_max: i128,
    public_blue_score: u64,
    private_blue_score_max: u64,
}

/// Release `private` to the victim block by block, reading the victim after each. If the private
/// branch ends up heavier and the victim still holds the public tip (the deep-reorg gate refused every
/// heavier private candidate on the last key, the block hash), the honest side goes on mining: up to
/// `public_beats` more public beats, each a new incumbent for the gate — the chain a merchant watches
/// does not stop at the release.
async fn release(tag: &str, v: &mut T12Chain, private: &[Block], fork: BlockHash, nonce: &mut u64, public_beats: usize) -> Released {
    let vp = v.vp();
    let public_tip = v.sink();
    let fork_bw = bw(v, fork);
    let fork_bs = bs(v, fork);
    let set: HashSet<BlockHash> = private.iter().map(|b| b.header.hash).collect();
    let mut r = Released {
        public_bw: bw(v, public_tip) - fork_bw,
        public_blue_score: bs(v, public_tip) - fork_bs,
        private_bw_max: i128::MIN,
        ..Default::default()
    };
    eprintln!(
        "[hb-probe {tag}] release: public tip {public_tip} blue work +{} blue score +{} above the fork; {} private blocks follow",
        r.public_bw,
        r.public_blue_score,
        private.len()
    );
    for (i, b) in private.iter().enumerate() {
        let n = i + 1;
        if let Err(e) = v.ctx.consensus.validate_and_insert_block(b.clone()).virtual_state_task.await {
            eprintln!(
                "[hb-probe {tag}]   private block {n} {} (stamp {}) refused by the victim: {e}",
                b.header.hash, b.header.timestamp
            );
            r.refused.push((n, e.to_string()));
            continue;
        }
        let h = b.header.hash;
        let priv_bw = bw(v, h) - fork_bw;
        r.private_bw_max = r.private_bw_max.max(priv_bw);
        r.private_blue_score_max = r.private_blue_score_max.max(bs(v, h) - fork_bs);
        let sink = v.sink();
        let on_private = set.contains(&sink);
        let offered = priv_bw > r.public_bw || (priv_bw == r.public_bw && h > public_tip);
        if offered && r.offered_at.is_none() {
            r.offered_at = Some(n);
            let (op, oc) = (vp.palw_candidate_order_v2(public_tip), vp.palw_candidate_order_v2(h));
            eprintln!(
                "[hb-probe {tag}]   OFFERED at private block {n}: its blue work +{priv_bw} vs the public tip's +{}; PALW keys public {op:?} private {oc:?} -> decide_deep_reorg_v2 {:?}; victim sink now {}",
                r.public_bw,
                op.zip(oc).map(|(p, c)| decide_deep_reorg_v2(&p, &c)),
                if on_private { "PRIVATE" } else { "public" }
            );
        }
        if offered && !on_private {
            r.vetoed_while_offered += 1;
        }
        if on_private && r.flipped_at.is_none() {
            r.flipped_at = Some(n);
            eprintln!(
                "[hb-probe {tag}]   FLIPPED at private block {n}: victim sink {sink} (private blue work +{}, blue score +{}) over the public tip's +{}",
                bw(v, sink) - fork_bw,
                bs(v, sink) - fork_bs,
                r.public_bw
            );
        }
    }
    if r.flipped_at.is_none() && r.private_bw_max > r.public_bw {
        for k in 1..=public_beats {
            let clock = v.ctx.simulated_time + 1_000;
            let beat = layer(v, nonce, 1, clock, Vec::new()).await.expect("an honest public beat").remove(0);
            let sink = v.sink();
            eprintln!(
                "[hb-probe {tag}]   public beat {k} after the release: blue work +{} (the private max +{}); victim sink {}",
                bw(v, beat.header.hash) - fork_bw,
                r.private_bw_max,
                if set.contains(&sink) { "PRIVATE" } else { "public" }
            );
            if set.contains(&sink) {
                r.flipped_after_public_beats = Some(k);
                break;
            }
            if bw(v, beat.header.hash) - fork_bw >= r.private_bw_max {
                break;
            }
        }
    }
    eprintln!(
        "[hb-probe {tag}] released {}: refused {}, offered at {:?}, flipped at {:?} (after {:?} further public beats), offered-but-kept-public {} times; private max blue work +{} (blue score +{}) vs public +{} (+{})",
        private.len(),
        r.refused.len(),
        r.offered_at,
        r.flipped_at,
        r.flipped_after_public_beats,
        r.vetoed_while_offered,
        r.private_bw_max,
        r.private_blue_score_max,
        r.public_bw,
        r.public_blue_score
    );
    r
}

impl Released {
    fn flipped(&self) -> bool {
        self.flipped_at.is_some() || self.flipped_after_public_beats.is_some()
    }
}

/// Which payment the victim's UTXO set holds after the release: `(X's output, Y's output)`.
fn payments(tag: &str, d: &Duel) -> (bool, bool) {
    let x_out = TransactionOutpoint::new(d.x.id(), 0);
    let y_out = TransactionOutpoint::new(d.y.id(), 0);
    let held = (has_utxo(&d.victim, x_out), has_utxo(&d.victim, y_out));
    eprintln!(
        "[hb-probe {tag}] victim UTXO set: X's output {} / Y's output {}{}",
        if held.0 { "PRESENT" } else { "absent" },
        if held.1 { "PRESENT" } else { "absent" },
        if held.1 && !held.0 { " — X was reorged out and Y kept: the double spend landed" } else { "" }
    );
    held
}

/// **The rules the probe rests on, read off testnet-12 and off the victim's own stores.**
fn print_facts(d: &Duel) {
    let p = &d.config.params;
    let vp = d.victim.vp();
    eprintln!(
        "[hb-probe facts] testnet-12: tolerance {} s, interval {} ms, ghostdag_k {}, merge depth {}, finality depth {}, pruning depth {}, mergeset size limit {}",
        p.timestamp_deviation_tolerance,
        I,
        p.ghostdag_k(),
        p.merge_depth(),
        p.finality_depth(),
        p.pruning_depth(),
        p.mergeset_size_limit()
    );
    eprintln!(
        "[hb-probe facts] heartbeat lane {:?} (work_log2 / max_per_mergeset), attempt work {:?}, clock cursor {:?}, clock floor {:?}, frontier provenance {:?}",
        p.palw_heartbeat.as_ref().map(|h| (h.work_log2, h.max_per_mergeset)),
        p.palw_attempt_work.as_ref().map(|a| a.work_log2),
        p.palw_clock_cursor.as_ref().map(|f| f.is_active(0)),
        p.palw_clock_floor.as_ref().map(|f| f.is_active(0)),
        p.palw_frontier_provenance.as_ref().map(|f| f.is_active(0)),
    );
    match &p.dns_params {
        Some(dns) => eprintln!(
            "[hb-probe facts] DNS overlay configured: min_active_validators {}, min_bond {} sompi, min_anchor_attesters {}, required_stake_depth {:?}",
            dns.min_active_validators, dns.min_bond_amount_sompi, dns.min_anchor_attesters, dns.required_stake_depth
        ),
        None => eprintln!("[hb-probe facts] DNS overlay: none"),
    }
    match vp.dns_state_store.read().get() {
        Ok(s) => eprintln!(
            "[hb-probe facts] victim's DnsState: rollout stage {:?}, last confirmed anchor {} (default = nothing confirmed)",
            s.rollout_stage, s.last_dns_confirmed_anchor
        ),
        Err(e) => eprintln!("[hb-probe facts] victim's DnsState: none written ({e})"),
    }
}

/// **(a) A private heartbeat branch against a public branch that carries floor attempts.**
///
/// The public side mines the slot with X in its holder, then three floor attempts (cards 1-3), then
/// two honest slots; the private side mines Y and then twelve slots of four-sibling layers — the most
/// heartbeats a slot can hold (8, four per mergeset), of which testnet-12's `ghostdag_k` = 1 lets
/// four be blue. The per-lane work is MEASURED from the victim's ghostdag store (a child whose
/// mergeset is exactly one block adds that block's work), and the private branch never reaches the
/// heap's top: four blue beats a slot against 2^20 an attempt.
#[tokio::test]
async fn hb_probe_a_a_private_heartbeat_branch_never_outweighs_public_attempts() {
    kaspa_core::log::try_init_logger("warn");
    let mut d = duel(None);
    print_facts(&d);
    for _ in 0..3 {
        honest_slot_mirrored(&mut d).await;
    }
    let fork = d.victim.sink();
    assert_eq!(d.attacker.sink(), fork, "one prefix");

    // Public: X in the holder, the step, three floor attempts, two honest slots.
    let first = honest_slot(&mut d.victim, vec![d.x.clone()]).await;
    assert!(first[0].transactions.iter().any(|t| t.id() == d.x.id()), "X rides the public holder");
    let mut attempts = Vec::new();
    for card in 1..=3usize {
        let (block, _) = d.victim.attempt(card, 1_000, Vec::new(), &|_| true).await;
        attempts.push(block);
    }
    let after = honest_slot(&mut d.victim, Vec::new()).await;
    honest_slot(&mut d.victim, Vec::new()).await;
    // Measured per-block work: a child whose mergeset is its selected parent alone adds that parent's work.
    let hb_work = bw(&d.victim, first[1].header.hash) - bw(&d.victim, first[0].header.hash);
    let attempt_work = bw(&d.victim, attempts[1].header.hash) - bw(&d.victim, attempts[0].header.hash);
    eprintln!(
        "[hb-probe a] measured work: heartbeat {hb_work} (algo {}), attempt {attempt_work} (algo {}); attempts at DAA {:?}, the slot after them ticks to {}",
        first[0].header.pow_algo_id,
        attempts[0].header.pow_algo_id,
        attempts.iter().map(|b| b.header.daa_score).collect::<Vec<_>>(),
        d.victim.daa_of(after.last().unwrap().header.hash)
    );
    assert_eq!(hb_work, 1, "a heartbeat adds ε = 1");
    assert_eq!(attempt_work, 1 << 20, "an attempt adds 2^20 (palw_attempt_work)");
    assert!(
        attempts.iter().all(|a| d.victim.daa_of(a.header.hash) == d.victim.daa_of(attempts[0].header.hash)),
        "attempts do not move the DAA score among themselves (only a grant does)"
    );

    // Private: Y, then twelve slots of four-sibling layers.
    let mut private = private_slot(&mut d.attacker, &mut d.nonce, 1, vec![d.y.clone()]).await;
    private.extend(private_slot(&mut d.attacker, &mut d.nonce, 4, Vec::new()).await);
    let bw_steady = bw(&d.attacker, d.attacker.sink());
    for _ in 0..11 {
        private.extend(private_slot(&mut d.attacker, &mut d.nonce, 4, Vec::new()).await);
    }
    let a_tip = d.attacker.sink();
    let per_slot = (bw(&d.attacker, a_tip) - bw_steady) / 11;
    assert_eq!(bw(&d.attacker, a_tip) - bw_steady, 44, "eleven steady slots of eight beats: 4 blue a slot");
    assert_eq!(
        per_slot, 4,
        "eight beats a slot, four of them blue: ghostdag_k = 1 lets one sibling a layer be blue beside the selected parent"
    );
    eprintln!(
        "[hb-probe a] private: {} beats over 13 slots, attacker-side blue work +{} (steady {per_slot} blue a slot from 8 beats), blue score +{}, DAA +{}",
        private.len(),
        bw(&d.attacker, a_tip) - bw(&d.attacker, fork),
        bs(&d.attacker, a_tip) - bs(&d.attacker, fork),
        d.attacker.daa_of(a_tip) - d.attacker.daa_of(fork)
    );
    let r = release("a", &mut d.victim, &private, fork, &mut d.nonce, 0).await;
    assert!(r.refused.is_empty(), "every private beat is a valid block");
    assert_eq!(r.flipped_at, None, "a heartbeat branch never outweighs attempts");
    assert_eq!(r.offered_at, None, "and never even reaches the top of the heap");
    let deficit = r.public_bw - r.private_bw_max;
    // Net gain a slot against an honest public side (2 blue a slot): per_slot - 2.
    let net = per_slot - 2;
    eprintln!(
        "[hb-probe a] the private branch is {deficit} blue work short; at the measured net +{net} a slot over an honest heartbeat side it needs {} more slots ({:.1} days of 120 s slots) — and every further public attempt adds 2^20",
        deficit / net + 1,
        (deficit / net + 1) as f64 * I as f64 / 86_400_000.0
    );
    let (x, y) = payments("a", &d);
    assert!(x && !y, "X stands");
}

/// **(b) A heartbeat-only period: the private branch mines more beats a slot than the honest two.**
///
/// Ten slots on each side of the fork, X and Y in the first holder of each. The public side is the
/// honest shape (two beats a slot); the private side mines `m` siblings in every layer, m = 1 (the
/// honest shape — a tie on blue work), 2 and 4 (the mergeset bound). Nothing is stamped ahead of wall
/// time here: this is the sibling effect alone.
#[tokio::test]
async fn hb_probe_b_sibling_beats_outweigh_a_heartbeat_only_chain() {
    kaspa_core::log::try_init_logger("warn");
    let mut maxima = Vec::new();
    for m in [1usize, 2, 4] {
        let tag = format!("b m={m}");
        let mut d = duel(None);
        for _ in 0..3 {
            honest_slot_mirrored(&mut d).await;
        }
        let fork = d.victim.sink();
        honest_slot(&mut d.victim, vec![d.x.clone()]).await;
        for _ in 0..9 {
            honest_slot(&mut d.victim, Vec::new()).await;
        }
        let mut private = private_slot(&mut d.attacker, &mut d.nonce, m, vec![d.y.clone()]).await;
        for _ in 0..9 {
            private.extend(private_slot(&mut d.attacker, &mut d.nonce, m, Vec::new()).await);
        }
        eprintln!(
            "[hb-probe {tag}] public 10 slots: DAA +{}, {} blocks; private 10 slots: DAA +{}, {} beats",
            d.victim.daa_of(d.victim.sink()) - d.victim.daa_of(fork),
            bs(&d.victim, d.victim.sink()) - bs(&d.victim, fork),
            d.attacker.daa_of(d.attacker.sink()) - d.attacker.daa_of(fork),
            private.len()
        );
        let r = release(&tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await;
        assert!(r.refused.is_empty(), "{tag}: every private beat is a valid block");
        let (x, y) = payments(&tag, &d);
        assert_eq!(r.flipped(), y && !x, "{tag}: the flip is exactly the double spend");
        if m == 1 {
            assert_eq!(r.private_bw_max, r.public_bw, "{tag}: the honest shape ties on blue work — the block hash decides");
        } else {
            assert!(r.private_bw_max > r.public_bw, "{tag}: siblings out-weigh the honest two a slot");
            assert!(r.flipped() && y && !x, "{tag}: and the double spend lands");
        }
        maxima.push((m, r.private_bw_max, private.len(), r.flipped_at, r.flipped_after_public_beats));
    }
    eprintln!(
        "[hb-probe b] (siblings a layer, private blue work above the fork, beats, flipped at released block, or after public beats) = {maxima:?}; public +20 from 20 beats"
    );
    assert_eq!(maxima[1].1, maxima[2].1, "m = 2 and m = 4 buy the same blue work: ghostdag_k = 1, not the mergeset bound 4, caps it");
}

/// **(b) Running ahead of wall time: the private branch stamps every slot the moment it opens, up to
/// the future-time bound — at 132 s (what testnet-12 runs today, and what the lead cap in flight
/// gives every clock-moving header) and at 1,620 s (mainnet's value, adopted for testnet-12 on the
/// mainnet-values branch).**
///
/// The fork point sits about six slots behind the victim's real clock. The public side mines honest
/// slots up to that clock, X in the first; the attacker mines from the same parent, Y in the first
/// holder, stamping holder and step AT each slot (m = 1, the honest shape) until its own node refuses
/// a holder as too far in the future, then releases everything at once.
///
/// Measured on `2004c588`, before the lead cap: 1,620 s gave thirteen slots of lead and the double
/// spend landed. On the release this branch sits on (`0e8ec984e`) the cap is armed, so both runs stop
/// at 132 s past the clock — one slot of lead — and the assertions below read the bound in force.
#[tokio::test]
async fn hb_probe_b_future_stamped_beats_run_the_private_branch_ahead() {
    kaspa_core::log::try_init_logger("warn");
    for tolerance in [132u64, 1_620] {
        let tag = format!("b-future T={tolerance}s");
        let mut d = duel(Some(tolerance));
        let start = kaspa_core::time::unix_now() - 8 * (I + 1_000);
        d.victim.ctx.simulated_time = start;
        d.attacker.ctx.simulated_time = start;
        for _ in 0..2 {
            honest_slot_mirrored(&mut d).await;
        }
        let fork = d.victim.sink();
        // Public: honest slots while the next slot has opened on the victim's clock.
        let mut public_slots = 0;
        let mut txs = vec![d.x.clone()];
        while ts(&d.victim, d.victim.sink()) + I <= kaspa_core::time::unix_now() {
            honest_slot(&mut d.victim, std::mem::take(&mut txs)).await;
            public_slots += 1;
        }
        assert!(txs.is_empty(), "{tag}: X was mined");
        // Private: every slot claimed the moment it opens, holder and step stamped AT the slot.
        let mut private = Vec::new();
        let mut txs = vec![d.y.clone()];
        let mut private_slots = 0;
        let refusal = loop {
            let reference = d.attacker.ctx.simulated_time;
            match layer(&mut d.attacker, &mut d.nonce, 1, reference, std::mem::take(&mut txs)).await {
                Ok(h) => private.extend(h),
                Err(e) => break e,
            }
            let slot = d.attacker.ctx.simulated_time;
            match layer(&mut d.attacker, &mut d.nonce, 1, slot, Vec::new()).await {
                Ok(s) => private.extend(s),
                Err(e) => break e,
            }
            private_slots += 1;
            assert!(private_slots < 100, "{tag}: the run-ahead stops at the future bound");
        };
        let now = kaspa_core::time::unix_now();
        let ahead = private.iter().filter(|b| b.header.timestamp > now).count();
        let last = private.last().unwrap().header.timestamp;
        eprintln!(
            "[hb-probe {tag}] public {public_slots} slots (tip stamped {:+.1} s from now); private {private_slots} slots, {} beats, {ahead} of them stamped in the future, the last {:+.1} s from now; the attacker's own node stopped at: {refusal}",
            (ts(&d.victim, d.victim.sink()) as f64 - now as f64) / 1_000.0,
            private.len(),
            (last as f64 - now as f64) / 1_000.0
        );
        // **What stops the run-ahead on the RELEASE (0e8ec984e), and it is the beat lead cap.** The
        // probe was written on rcore/hb-fork-choice-probe, whose base carried the 1,620 s tolerance
        // with NO cap, and it asserted the run-ahead was stopped by `TimeTooFarIntoTheFuture` at
        // floor(T / I) slots. The launch release ships `palw_clock_lead_cap` armed on testnet-12
        // (the 2026-09-25 mainnet-values review's HIGH, merged as `ce75beff`), which refuses any
        // clock-moving header stamped more than `PALW_CLOCK_LEAD_CAP_MS` = 132 s past THIS node's
        // clock — tighter than the 1,620 s tolerance. So at 1,620 s the stopper is
        // `ClockLeadTooFarAhead`, not the future bound, and the run-ahead is bounded by the CAP,
        // not the tolerance. This is verdict 4's "the rate is closed": measured, not asserted away.
        let cap_ms = kaspa_consensus_core::palw_clock_cursor_v1::PALW_CLOCK_LEAD_CAP_MS;
        let capped = matches!(refusal, RuleError::ClockLeadTooFarAhead(..));
        assert!(
            matches!(refusal, RuleError::TimeTooFarIntoTheFuture(..)) || capped,
            "{tag}: the run-ahead is stopped by the future bound or the lead cap, and by nothing else — got {refusal}"
        );
        if tolerance == 1_620 {
            assert!(capped, "{tag}: past 132 s the lead cap (not the 1,620 s tolerance) is what stops the run-ahead");
        }
        let r = release(&tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await;
        assert!(r.refused.is_empty(), "{tag}: the victim accepts every beat its bound admitted a moment ago");
        let (x, y) = payments(&tag, &d);
        assert_eq!(r.flipped(), y && !x, "{tag}: the flip is exactly the double spend");
        let lead = private_slots as i64 - public_slots as i64;
        // The run-ahead is bounded by the SMALLER of the tolerance and the lead cap, in whole slots.
        let effective_ms = (tolerance * 1_000).min(cap_ms);
        let budget = (effective_ms / I) as i64;
        eprintln!(
            "[hb-probe {tag}] lead: private {private_slots} slots vs public {public_slots} = {lead} slots ahead (min(T, cap {} s) / I = {budget} slots); blue work +{} vs +{}; the victim's sink now at DAA +{} above the fork after {public_slots} slots of wall time; double spend landed: {}",
            cap_ms / 1_000,
            r.private_bw_max,
            r.public_bw,
            d.victim.daa_of(d.victim.sink()) - d.victim.daa_of(fork),
            y && !x
        );
        // The lead is at most the cap's worth of slots (plus the boundary slot the loop mines before
        // its stamp trips the cap). At 1,620 s this is ~1 slot, NOT the 13 the uncapped tolerance
        // would give — the residual is what verdict 4 calls "partial": a small run-ahead survives the
        // cap, and it still flips a deep reorg whose PALW keys tie (see the fenced fix on this branch).
        assert!(lead <= budget + 2, "{tag}: the run-ahead is bounded by min(tolerance, lead cap) in whole slots, got {lead} > {budget}+2");
    }
}

/// **(iii) A beat refused as too far in the future is not cached invalid: offered again once the
/// victim's clock has moved, the same block is accepted.** Measured at 132 s; the refusal is
/// `TimeTooFarIntoTheFuture` from `check_block_timestamp_in_isolation`, which runs before PoW, and
/// `validate_header` caches only post-PoW errors as `StatusInvalid`.
#[tokio::test]
async fn hb_probe_iii_a_refused_future_beat_is_admitted_once_the_clock_catches_up() {
    kaspa_core::log::try_init_logger("warn");
    let mut d = duel(Some(132));
    d.victim.ctx.simulated_time = kaspa_core::time::unix_now() - 3 * (I + 1_000);
    honest_slot(&mut d.victim, Vec::new()).await;
    honest_slot(&mut d.victim, Vec::new()).await;
    let margin_ms = 2_500;
    let clock = kaspa_core::time::unix_now() + 132_000 + margin_ms;
    // Build on the victim's own virtual, without inserting: the same path `layer` takes.
    let mut t = d
        .victim
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&d.victim.config.params, &mut t.block.header, clock);
    t.block.header.nonce = 77;
    t.block.header.finalize();
    let (t, earliest) = d.victim.vp().heartbeat_adapt_block_template(t).expect("the lane is open");
    let beat = t.block.to_immutable();
    assert_eq!(beat.header.timestamp, clock, "stamped by its clock, past the slot {earliest}");
    let hash = beat.header.hash;
    let first = d.victim.ctx.consensus.validate_and_insert_block(beat.clone()).virtual_state_task.await;
    let status_after_refusal = d.victim.ctx.consensus.get_block_status(hash);
    eprintln!(
        "[hb-probe iii] a beat stamped {:.1} s ahead: first offer -> {first:?}; status afterwards {status_after_refusal:?}",
        (clock - kaspa_core::time::unix_now()) as f64 / 1_000.0
    );
    assert!(matches!(first, Err(RuleError::TimeTooFarIntoTheFuture(..))), "refused for the future bound");
    assert_eq!(status_after_refusal, None, "and not cached: the node holds no verdict on it");
    let wait = (clock - 132_000).saturating_sub(kaspa_core::time::unix_now()) + 300;
    std::thread::sleep(std::time::Duration::from_millis(wait));
    let second = d.victim.ctx.consensus.validate_and_insert_block(beat.clone()).virtual_state_task.await;
    eprintln!("[hb-probe iii] offered again {wait} ms later -> {second:?}; sink is the beat: {}", d.victim.sink() == hash);
    assert!(second.is_ok(), "the same block is admitted once the clock reaches its stamp less the tolerance");
    assert_eq!(d.victim.sink(), hash);
}

/// **(c) An attacker who also holds a bond: attempt parity, and heartbeats as the tie-breaker.**
///
/// Public: X in the holder, card 1's floor attempt steps the slot, then eight honest slots. Private:
/// Y in the holder, card 2's floor attempt (the attacker's own bond) steps it, then eight slots of
/// four-sibling layers. One attempt each: the attempts cancel and the beats decide. Then again with a
/// second public attempt (card 3): one attempt the attacker does not match outweighs every beat.
#[tokio::test]
async fn hb_probe_c_a_bonded_attacker_breaks_attempt_parity_with_beats() {
    kaspa_core::log::try_init_logger("warn");
    for public_attempts in [1usize, 2] {
        let tag = format!("c public-attempts={public_attempts}");
        let mut d = duel(None);
        for _ in 0..3 {
            honest_slot_mirrored(&mut d).await;
        }
        let fork = d.victim.sink();
        d.victim.heartbeat(1_000, vec![d.x.clone()]).await;
        let (pa, _) = d.victim.attempt(1, 1_000, Vec::new(), &|_| true).await;
        assert_eq!(d.victim.daa_of(pa.header.hash), d.victim.daa_of(fork) + 1, "{tag}: the public attempt steps the slot");
        if public_attempts == 2 {
            d.victim.attempt(3, 1_000, Vec::new(), &|_| true).await;
        }
        for _ in 0..8 {
            honest_slot(&mut d.victim, Vec::new()).await;
        }
        let clock = d.attacker.ctx.simulated_time + 1_000;
        let mut private = layer(&mut d.attacker, &mut d.nonce, 1, clock, vec![d.y.clone()]).await.expect("the private holder");
        let (aa, _) = d.attacker.attempt(2, 1_000, Vec::new(), &|_| true).await;
        assert_eq!(d.attacker.daa_of(aa.header.hash), d.attacker.daa_of(fork) + 1, "{tag}: the private attempt steps the slot");
        private.push(aa);
        for _ in 0..8 {
            private.extend(private_slot(&mut d.attacker, &mut d.nonce, 4, Vec::new()).await);
        }
        let r = release(&tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await;
        assert!(r.refused.is_empty(), "{tag}: every private block is valid");
        let (x, y) = payments(&tag, &d);
        assert_eq!(r.flipped(), y && !x, "{tag}: the flip is exactly the double spend");
        if public_attempts == 1 {
            assert!(r.flipped() && y && !x, "{tag}: at attempt parity the beats decide");
        } else {
            assert_eq!(r.flipped_at, None, "{tag}: an unmatched attempt outweighs every beat");
            let deficit = r.public_bw - r.private_bw_max;
            // Measured: 4 blue a slot privately (hb_probe_a) against 2 publicly, net +2 a slot.
            eprintln!(
                "[hb-probe {tag}] short by {deficit}: at the measured net +2 blue a slot, {} slots ({:.1} days) of private beats per unmatched attempt",
                deficit / 2 + 1,
                (deficit / 2 + 1) as f64 * I as f64 / 86_400_000.0
            );
        }
    }
}

/// **(d) What does stop it: the finality depth, counted in blue score on the VICTIM's own chain.**
///
/// A heartbeat-only public chain — X in the first slot after the fork, then honest slots until the
/// payment is buried `2 · slots` blue score deep — against a private branch that mines Y and two
/// siblings a layer (4 blue a slot, the ghostdag_k = 1 maximum) over the same slots. Just under
/// testnet-12's `finality_depth` (600) the heavier private branch is adopted and X is gone; just over
/// it the victim never leaves its chain however heavy the private one is ("Finality Violation"), and
/// X stands. Nothing else in the path — DNS gate (Bootstrap here, nothing confirmed), PALW veto (no
/// Final or immature claim on either side) — refuses it.
#[tokio::test]
async fn hb_probe_d_only_the_finality_depth_stops_a_heartbeat_only_reorg() {
    kaspa_core::log::try_init_logger("error");
    for slots in [290usize, 310] {
        let tag = format!("d {slots} slots");
        let mut d = duel(None);
        let finality_depth = d.config.params.finality_depth();
        for _ in 0..3 {
            honest_slot_mirrored(&mut d).await;
        }
        let fork = d.victim.sink();
        honest_slot(&mut d.victim, vec![d.x.clone()]).await;
        for _ in 1..slots {
            honest_slot(&mut d.victim, Vec::new()).await;
        }
        let mut private = private_slot(&mut d.attacker, &mut d.nonce, 1, vec![d.y.clone()]).await;
        for _ in 1..slots {
            private.extend(private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await);
        }
        let depth = bs(&d.victim, d.victim.sink()) - bs(&d.victim, fork);
        eprintln!(
            "[hb-probe {tag}] X is {depth} blue score deep on the victim ({slots} heartbeat-only slots, {:.1} h); finality depth {finality_depth}",
            slots as f64 * (I + 1_000) as f64 / 3_600_000.0
        );
        let r = release(&tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await;
        assert!(r.refused.is_empty(), "{tag}: every private block is a valid block");
        assert!(r.private_bw_max > r.public_bw, "{tag}: the private branch is the heavier one");
        let (x, y) = payments(&tag, &d);
        if depth < finality_depth {
            assert!(r.flipped() && y && !x, "{tag}: under the finality depth the heavier private branch takes X back");
        } else {
            assert!(!r.flipped() && x && !y, "{tag}: past the finality depth nothing reorgs X");
        }
    }
}

/// **(e) What a merchant can wait for instead: a PALW Settlement Anchor after X whose claim is `Final`
/// (ADR-0127 Decision 1, ADR-0129).**
///
/// The public side mines X in a holder, then card 0's floor attempt over it — the anchor that accepts
/// X — whose claim the chain binds to a panel at the anchor slot (card 7's attempt), a quorum of the
/// drawn seats licenses (their real ML-DSA-87 receipts on a carrier funded by card 0's float), and the
/// short challenge window finalises. The attacker holds bonds too (regime c): its private branch
/// carries Y, two attempts of its own (cards 2 and 3) — attempt parity — and two sibling beats a layer
/// over the same slots, so it is the heavier branch on blue work. It cannot mature a claim: nobody
/// signs receipts on a branch nobody saw (the harness holds every seat key, and does not sign them).
///
/// Released one DAA before the claim is `Final`, the PALW keys tie on the frontier and the victim
/// follows blue work (and the block-hash key) — the double spend lands. Released once it is `Final`,
/// the public frontier is the anchor's blue score, the private one is zero, and `decide_deep_reorg_v2`
/// refuses every private candidate on the first key however heavy — X stands.
#[tokio::test]
async fn hb_probe_e_a_final_anchor_after_x_refuses_the_heavier_private_branch() {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj};
    kaspa_core::log::try_init_logger("error");
    for settled in [false, true] {
        let tag = format!("e {}", if settled { "released after Final" } else { "released one DAA before Final" });
        let mut d = duel(None);
        let bundle = d.victim.bundle.clone();
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            d.config.params.net.to_string().as_bytes(),
            Some(d.config.params.genesis.hash),
        );
        for _ in 0..3 {
            honest_slot_mirrored(&mut d).await;
        }
        let fork = d.victim.sink();

        // ---- public: X, the anchor that accepts it, its panel, its receipts ------------------------
        let holder = d.victim.heartbeat(1_000, vec![d.x.clone()]).await;
        let (anchor, claim_id) = d.victim.attempt(0, 1_000, Vec::new(), &|_| true).await;
        assert_eq!(anchor.header.direct_parents().to_vec(), vec![holder.header.hash], "the anchor merges X's block: it accepts X");
        let bound = d.victim.attempt_at_the_anchor_slot(claim_id, 7).await;
        let (_, state) = d.victim.tip_state();
        let panel = state.panel(&claim_id).expect("bound").clone();
        let seat_cards: Vec<usize> =
            panel.seats.iter().map(|s| d.victim.bonds.iter().position(|b| *b == s.bond).expect("a genesis card")).collect();
        let signed_daa = d.victim.ctx.consensus.get_virtual_daa_score();
        let receipts: Vec<PalwSeatReceiptV2> = panel
            .seats
            .iter()
            .zip(&seat_cards)
            .take(bundle.panel.quorum() as usize)
            .map(|(seat, card)| {
                let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
                let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                    &crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(*card as u64).signing_key,
                    message.as_byte_slice(),
                    PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                    [0x11u8; 32],
                )
                .expect("sign")
                .as_ref()
                .to_vec();
                PalwSeatReceiptV2 {
                    claim: claim_id,
                    verdict: PalwReceiptVerdictV2::Valid,
                    seat_bond: seat.bond,
                    signed_daa,
                    signature,
                }
            })
            .collect();
        let object = d.victim.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
        assert!(matches!(object, Obj::ReceiptLicensed { .. }));
        let carrier = {
            let payload =
                borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
            let (outpoint, entry) = d.floats[0].clone();
            let mut tx = Transaction::new(
                crate::constants::TX_VERSION,
                vec![TransactionInput::new(outpoint, vec![], 0, 1)],
                vec![TransactionOutput::new(entry.amount - 300_000, card_payout_spk(0))],
                0,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload,
            );
            sign_spend(&mut tx, entry, 0, d.config.params.storage_mass_parameter);
            tx
        };
        honest_slot(&mut d.victim, vec![carrier]).await;
        honest_slot(&mut d.victim, Vec::new()).await;
        let (_, state) = d.victim.tip_state();
        let licensed_daa = match state.claim(&claim_id).unwrap().phase.clone() {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
            other => panic!("{tag}: the carried quorum licenses the claim; it is {other:?}"),
        };
        let final_daa = licensed_daa + bundle.state.window_challenge_at(licensed_daa) + 1;
        eprintln!(
            "[hb-probe {tag}] X at DAA {}, accepted by anchor {} (claim {claim_id}); panel bound at DAA {} (anchor delay {}), licensed at DAA {licensed_daa}, Final due at DAA {final_daa} (challenge window {})",
            holder.header.daa_score,
            anchor.header.hash,
            bound.header.daa_score,
            bundle.panel.anchor_delay(),
            bundle.state.window_challenge_at(licensed_daa)
        );
        let stop = if settled { final_daa } else { final_daa - 1 };
        while d.victim.daa_of(d.victim.sink()) < stop {
            honest_slot(&mut d.victim, Vec::new()).await;
        }
        let (_, state) = d.victim.tip_state();
        let phase = state.claim(&claim_id).unwrap().phase.clone();
        assert_eq!(matches!(phase, PalwClaimPhaseV2::Final { .. }), settled, "{tag}: the claim is {phase:?}");
        let public_daa = d.victim.daa_of(d.victim.sink()) - d.victim.daa_of(fork);

        // ---- private: Y, two attempts of the attacker's own, two siblings a layer --------------------
        let clock = d.attacker.ctx.simulated_time + 1_000;
        let mut private = layer(&mut d.attacker, &mut d.nonce, 1, clock, vec![d.y.clone()]).await.expect("the private holder");
        private.push(d.attacker.attempt(2, 1_000, Vec::new(), &|_| true).await.0);
        let mut second = false;
        while d.attacker.daa_of(d.attacker.sink()) - d.attacker.daa_of(fork) < public_daa {
            private.extend(private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await);
            if !second && d.attacker.daa_of(d.attacker.sink()) >= bound.header.daa_score {
                private.push(d.attacker.attempt(3, 1_000, Vec::new(), &|_| true).await.0);
                second = true;
            }
        }
        eprintln!(
            "[hb-probe {tag}] public {public_daa} DAA above the fork, claim {phase:?}; private {} blocks (2 attempts), DAA +{}",
            private.len(),
            d.attacker.daa_of(d.attacker.sink()) - d.attacker.daa_of(fork)
        );
        let r = release(&tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await;
        assert!(r.refused.is_empty(), "{tag}: every private block is valid");
        assert!(r.private_bw_max > r.public_bw, "{tag}: the private branch is the heavier one on blue work");
        let depth = bs(&d.victim, d.victim.sink()) - bs(&d.victim, fork);
        assert!(
            depth < d.config.params.finality_depth(),
            "{tag}: the fork is inside the finality depth ({depth}), so only PALW can refuse"
        );
        let (x, y) = payments(&tag, &d);
        if settled {
            assert!(!r.flipped() && x && !y, "{tag}: a Final anchor after X refuses the heavier private branch");
        } else {
            assert_eq!(r.flipped(), y && !x, "{tag}: the flip is exactly the double spend");
        }
    }
}

// ================================================================================================
// **F1 — the absorb hole and the post-launch fence that closes it (ADR-0105 §11, 2026-09-25).**
//
// The heartbeat double-spend verifier (workflow wf_35c50751-12d) found what the probes above did not
// try: a bondless heartbeat miner does not need more weight than the public chain, it can BORROW it.
// Its private branch merges the public attempts, and under the rule testnet-12 launched with a
// non-heartbeat block ignores every heartbeat in its anticone — the attacker's too — so each public
// attempt is BLUE on the private branch and its 2^20 counts there as well as on the public chain.
// The private branch then out-weighs the public one by its own ε, and the victim reorgs X away (the
// verifier's g2: private +2,097,158 vs public +2,097,157).
//
// testnet-12 launched with that rule, so the fix is a flag day: past
// `Params::palw_heartbeat_transparent_same_chain` (keyed on the CANDIDATE's own DAA score) a
// non-heartbeat block ignores heartbeats only in a block whose selected chain its own selected parent
// is on — the late draw ADR-0105 was written for — and is colored classically, against the other
// branch's beats, anywhere else. Every attack below runs twice: on testnet-12 as launched (the fence
// `None` — the double spend lands, the verifier's measurement kept as the control) and with the fence
// armed below the fork (it does not). Below the height the armed build IS the launched build, block
// for block (`hb_regression_below_the_fence_the_armed_build_is_the_launched_build`), and an honest
// chain crosses the height with its clock running and every late draw blue
// (`hb_regression_the_fence_crosses_an_honest_chain_and_the_clock_runs`).
// ================================================================================================

/// testnet-12's harness config with F1's same-chain fence armed at DAA `at` (`None`: as launched).
/// Every other field is left as it was.
fn with_same_chain(config: &Config, at: Option<u64>) -> Config {
    let mut config = config.clone();
    config.params.palw_heartbeat_transparent_same_chain = at.map(ForkActivation::new);
    config.params.validate_palw_v2().expect("the same-chain fence rides testnet-12's transparency, armed from genesis");
    assert_eq!(config.params.palw_heartbeat_transparent_same_chain_fence(), at.map(ForkActivation::new));
    config
}

/// Where a run puts the fence: below the fork point of every attack here (the harness's third honest
/// slot is DAA 2 or more), so every public attempt is past it.
const ARMED_BELOW_THE_FORK: u64 = 1;

/// What the attacker's own node did with one public attempt, read off the attacker's next block.
#[derive(Debug)]
struct Absorb {
    /// The attacker's sink was on its private branch when that block was built on its virtual.
    sink_private: bool,
    /// The public attempt was BLUE in that block.
    blue: bool,
    /// The attempt's selected parent is the fork point itself: an attempt both branches hang from,
    /// which either may count — the public chain counts it too, so it decides nothing.
    at_the_fork: bool,
    /// The attempt's own DAA score — the key F1's fence is read at.
    daa: u64,
    /// The attacker's next block merged it at all. Past the horizon the template leaves it out — a
    /// block that merged it red below the merge-depth root would be invalid — so it is neither.
    merged: bool,
}

/// One absorb run: what the attacker's node did with each public attempt, the release to the victim,
/// which payment the victim holds, and the two nodes.
struct AbsorbRun {
    seen: Vec<Absorb>,
    released: Released,
    /// `(X's output, Y's output)` in the victim's UTXO set.
    held: (bool, bool),
    duel: Duel,
    private: Vec<Block>,
}

impl AbsorbRun {
    fn double_spent(&self) -> bool {
        !self.held.0 && self.held.1
    }
}

/// **The verifier's `g_absorb2`, kept as it was run and made to report.** A heartbeat-only attacker
/// forks with Y and one slot of `m`-sibling beats; then, for each of `slots` public attempts, it
/// mirrors every public block up to and including the attempt and mines its own slot on its own
/// virtual, whose first block merges the attempt. `x_in_attempt` rides X inside the first public
/// attempt (built at the fork point) instead of a public holder. `release` hands the private branch
/// to the victim at the end (the h2 horizon is measured without). `same_chain` arms F1's fence on both
/// nodes (`None`: testnet-12 as launched).
async fn absorb(tag: &str, slots: usize, m: usize, x_in_attempt: bool, release_it: bool, same_chain: Option<u64>) -> AbsorbRun {
    kaspa_core::log::try_init_logger("warn");
    let mut d = duel_armed(None, same_chain);
    for _ in 0..3 {
        honest_slot_mirrored(&mut d).await;
    }
    let fork = d.victim.sink();
    let mut private = private_slot(&mut d.attacker, &mut d.nonce, m, vec![d.y.clone()]).await;
    private.extend(private_slot(&mut d.attacker, &mut d.nonce, m, Vec::new()).await);
    let mut public: Vec<Block> = Vec::new();
    if !x_in_attempt {
        public.push(d.victim.heartbeat(1_000, vec![d.x.clone()]).await);
    }
    let cards = [1usize, 3, 4, 5, 7, 0];
    let mut seen = Vec::new();
    let mut mirrored = 0usize;
    for s in 0..slots {
        let txs = if x_in_attempt && s == 0 { vec![d.x.clone()] } else { Vec::new() };
        let (attempt, _) = d.victim.attempt(cards[s % cards.len()], 1_000, txs, &|_| true).await;
        let hash = attempt.header.hash;
        let daa = attempt.header.daa_score;
        public.push(attempt);
        for b in &public[mirrored..] {
            mirror(&mut d.attacker, b).await;
        }
        mirrored = public.len();
        let sink_private = private.iter().any(|b| b.header.hash == d.attacker.sink());
        // Once the attacker's own node has left its branch for the heavier public chain, its miner
        // is just another heartbeat miner on that chain: it mines the honest slot there (the rigid
        // `private_slot` shape assumes a slot boundary the public tip need not be on).
        let slot = if sink_private {
            private_slot(&mut d.attacker, &mut d.nonce, m, Vec::new()).await
        } else {
            honest_slot(&mut d.attacker, Vec::new()).await
        };
        let gd = d.attacker.vp().ghostdag_store.get_data(slot[0].header.hash).unwrap();
        let a = Absorb {
            sink_private,
            blue: gd.mergeset_blues.contains(&hash),
            at_the_fork: d.attacker.vp().ghostdag_store.get_selected_parent(hash).unwrap() == fork,
            daa,
            merged: gd.mergeset_blues.contains(&hash) || gd.mergeset_reds.contains(&hash),
        };
        eprintln!(
            "[hb-regression {tag}] public attempt {} (DAA {daa}): {a:?}; attacker tip +{} blue work, public tip +{}",
            s + 1,
            bw(&d.attacker, d.attacker.sink()) - bw(&d.attacker, fork),
            bw(&d.victim, d.victim.sink()) - bw(&d.victim, fork)
        );
        seen.push(a);
        private.extend(slot);
        public.extend(honest_slot(&mut d.victim, Vec::new()).await);
    }
    let released = if release_it { release(tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await } else { Released::default() };
    let held = payments(tag, &d);
    if release_it {
        assert_same_coloring(tag, &d.attacker, &d.victim, &private);
    }
    AbsorbRun { seen, released, held, duel: d, private }
}

/// The rule, stated over what the attacker's node did: past the fence a public attempt is blue in a
/// block the attacker built on its PRIVATE branch only if it was built at the fork point itself.
fn assert_nothing_borrowed(tag: &str, seen: &[Absorb]) {
    for (i, a) in seen.iter().enumerate() {
        if a.sink_private && !a.at_the_fork {
            assert!(!a.blue, "{tag}: public attempt {} (built on the public branch) is RED on the private branch — F1", i + 1);
        }
    }
}

/// How many public attempts built on the public branch the private branch counted blue.
fn borrowed(seen: &[Absorb]) -> usize {
    seen.iter().filter(|a| a.sink_private && !a.at_the_fork && a.blue).count()
}

/// **The rule is a function of the DAG.** The victim saw the public chain first and the private
/// branch in one burst; the attacker built that branch while its own sink moved between the two. Both
/// color every released block identically — selected parent, blue score, blue work, blues and reds.
fn assert_same_coloring(tag: &str, a: &T12Chain, b: &T12Chain, blocks: &[Block]) {
    for block in blocks {
        let h = block.header.hash;
        let (x, y) = (a.vp().ghostdag_store.get_data(h).unwrap(), b.vp().ghostdag_store.get_data(h).unwrap());
        assert_eq!(
            (x.selected_parent, x.blue_score, x.blue_work, &x.mergeset_blues, &x.mergeset_reds),
            (y.selected_parent, y.blue_score, y.blue_work, &y.mergeset_blues, &y.mergeset_reds),
            "{tag}: the two nodes color {h} differently"
        );
    }
}

/// Every block `c` holds but genesis, each after all of its parents — what a second node must be fed
/// to hold the same DAG.
fn blocks_in_topological_order(c: &T12Chain) -> Vec<Block> {
    let genesis = c.config.params.genesis.hash;
    let headers = c.vp().headers_store.clone();
    let mut seen: HashSet<BlockHash> = HashSet::new();
    let mut order = Vec::new();
    let mut stack: Vec<(BlockHash, bool)> = c.ctx.consensus.get_tips().into_iter().map(|h| (h, false)).collect();
    while let Some((h, parents_done)) = stack.pop() {
        if h == genesis {
            continue;
        }
        if parents_done {
            order.push(h);
            continue;
        }
        if !seen.insert(h) {
            continue;
        }
        stack.push((h, true));
        for parent in headers.get_header(h).unwrap().direct_parents() {
            if !seen.contains(parent) {
                stack.push((*parent, false));
            }
        }
    }
    order.into_iter().map(|h| c.ctx.consensus.get_block(h).unwrap_or_else(|e| panic!("{h}: {e}"))).collect()
}

/// Feed every block `from` holds to `to`, parents first, and demand `to` colors each one exactly as
/// `from` did and ends on the same sink. Returns how many blocks were compared.
async fn replay_and_compare(tag: &str, from: &T12Chain, to: &mut T12Chain) -> usize {
    let blocks = blocks_in_topological_order(from);
    for b in &blocks {
        mirror(to, b).await;
    }
    assert_same_coloring(tag, from, to, &blocks);
    assert_eq!(from.sink(), to.sink(), "{tag}: the same sink");
    blocks.len()
}

/// **g2 — the verifier's headline case: X in a public holder, two public attempts, a bondless
/// heartbeat attacker at two siblings a layer.** As launched: both attempts blue on the private
/// branch, +2,097,158 against +2,097,157, the victim flips, X gone and Y kept — the control, and
/// the reason for the fence. Past the fence: the first attempt is red on the private branch (its
/// selected parent is the public holder), the attacker's own node follows the heavier public chain
/// once it has seen it, and X stands. (The victim may still end on a block the attacker mined — one
/// built on the public chain after its node adopted it, which carries X; that is why these assert
/// the payments, not the probe's "sink on a released block".)
#[tokio::test]
async fn hb_regression_g2_a_heartbeat_branch_cannot_borrow_the_public_attempts() {
    let launched = absorb("g2 launched", 2, 2, false, true, None).await;
    assert!(launched.released.refused.is_empty(), "g2 launched: every released block is valid");
    assert_eq!(borrowed(&launched.seen), 2, "g2 launched: the private branch counts both public attempts — the hole");
    assert!(launched.double_spent(), "g2 launched: X reorged away and Y kept — the verifier's measurement, reproduced");

    let fenced = absorb("g2 fenced", 2, 2, false, true, Some(ARMED_BELOW_THE_FORK)).await;
    assert!(fenced.released.refused.is_empty(), "g2 fenced: every released block is valid");
    assert!(
        fenced.seen[0].merged && fenced.seen[0].sink_private && !fenced.seen[0].at_the_fork,
        "g2 fenced: the first attempt is merged on the private branch"
    );
    assert!(!fenced.seen[0].blue, "g2 fenced: and it is RED there — the attacker's beats count against another branch's attempt");
    assert_nothing_borrowed("g2 fenced", &fenced.seen);
    assert!(fenced.held.0 && !fenced.held.1, "g2 fenced: X stands and Y is not in the victim's UTXO set");
}

/// **h1 — X inside the first public attempt, three attempts.** The first attempt is built at the fork
/// point, so both branches hang from its selected parent and either may count it: blue on the private
/// branch in both runs, stated — the public chain counts it too, and on the private branch X loses to
/// the Y it accepted first. As launched the second and third are borrowed and the double spend lands;
/// past the fence the second is red on the private branch (by the third the attacker's own node has
/// left that branch for the heavier public chain) and X stands.
#[tokio::test]
async fn hb_regression_h1_an_attempt_at_the_fork_decides_nothing() {
    let launched = absorb("h1 launched", 3, 2, true, true, None).await;
    assert!(launched.released.refused.is_empty(), "h1 launched: every released block is valid");
    assert!(borrowed(&launched.seen) >= 1, "h1 launched: the private branch borrows an attempt of the public branch");
    assert!(launched.double_spent(), "h1 launched: X (inside a public attempt) reorged away and Y kept");

    let fenced = absorb("h1 fenced", 3, 2, true, true, Some(ARMED_BELOW_THE_FORK)).await;
    assert!(fenced.released.refused.is_empty(), "h1 fenced: every released block is valid");
    assert!(
        fenced.seen[0].merged && fenced.seen[0].at_the_fork && fenced.seen[0].sink_private,
        "h1 fenced: the first attempt hangs from the fork point"
    );
    assert!(fenced.seen[0].blue, "h1 fenced: which the private branch may count, as the public one does");
    assert!(fenced.seen[1..].iter().all(|a| !a.at_the_fork), "h1 fenced: the later attempts are built on the public branch");
    assert_nothing_borrowed("h1 fenced", &fenced.seen);
    assert!(fenced.held.0 && !fenced.held.1, "h1 fenced: X stands and Y is not in the victim's UTXO set");
}

/// **h2 — the horizon: how many public attempts a private branch at the honest shape (one beat a
/// layer) can borrow before the merge depth reddens one.** As launched the verifier measured eight,
/// the ninth red at merge depth 30 (~10 minutes of the public chain); past the fence none.
#[tokio::test]
async fn hb_regression_h2_the_horizon_is_zero_past_the_fence() {
    let launched = absorb("h2 launched", 14, 1, false, false, None).await;
    let horizon = launched.seen.iter().take_while(|a| a.sink_private && a.blue).count();
    let after = launched.seen.get(horizon).map(|a| (a.merged, a.blue));
    eprintln!(
        "[hb-regression h2] launched: the private branch borrowed public attempts 1..={horizon} of 14; attempt {} (merged, blue) = {after:?}",
        horizon + 1
    );
    assert!(horizon >= 2, "h2 launched: the private branch borrows several public attempts in a row — the hole");
    assert!(horizon < 14, "h2 launched: and the merge depth ends the run");

    let fenced = absorb("h2 fenced", 14, 1, false, false, Some(ARMED_BELOW_THE_FORK)).await;
    assert_nothing_borrowed("h2 fenced", &fenced.seen);
    assert_eq!(borrowed(&fenced.seen), 0, "h2 fenced: not one public attempt borrowed");
}

/// **The fence inside an attack: the attempt below the height is borrowed, the one at it is not.**
/// g2's DAG, with the fence armed at the second public attempt's own DAA score — so the first attempt
/// is colored by the launched rule in every block that merges it (keyed on the candidate, not on the
/// merging block: blue on the private branch, stated) and the second by F1's (red there). One borrowed
/// attempt against two public ones: X stands. The attack window a scheduled fence leaves is the
/// attempts minted below it, and it closes as they fall out of the merge depth.
#[tokio::test]
async fn hb_regression_the_fence_splits_an_attack_at_the_candidates_own_daa() {
    let launched = absorb("straddle probe", 2, 2, false, false, None).await;
    let (first, second) = (launched.seen[0].daa, launched.seen[1].daa);
    assert!(first < second, "the harness mints the two public attempts at different DAA scores ({first}, {second})");
    let straddled = absorb("straddle", 2, 2, false, true, Some(second)).await;
    assert_eq!((straddled.seen[0].daa, straddled.seen[1].daa), (first, second), "the public branch is the same DAG");
    assert!(straddled.released.refused.is_empty(), "straddle: every released block is valid");
    assert!(
        straddled.seen[0].sink_private && straddled.seen[0].blue,
        "straddle: the attempt below the height is borrowed, as launched"
    );
    if straddled.seen[1].sink_private {
        assert!(!straddled.seen[1].blue, "straddle: the attempt at the height is red on the private branch");
    }
    assert_eq!(borrowed(&straddled.seen), 1, "straddle: exactly the attempt below the fence is borrowed");
    assert!(straddled.held.0 && !straddled.held.1, "straddle: one borrowed attempt does not out-weigh two — X stands");
}

/// **Below its height the armed build IS the launched build, on the DAG where the two rules differ.**
/// g2's attack as testnet-12 launched it (the double spend lands), replayed block for block into a node
/// that has the fence scheduled far above this chain: every block colored identically — selected
/// parent, blue score, blue work, blues and reds — the same sink, and the same (double-spent) UTXO set.
/// A fleet can install the armed build before the height with nothing to disagree about.
#[tokio::test]
async fn hb_regression_below_the_fence_the_armed_build_is_the_launched_build() {
    let launched = absorb("g2 twin", 2, 2, false, true, None).await;
    assert!(launched.double_spent(), "the attack DAG, as launched");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let scheduled = with_same_chain(&config, Some(1_000_000));
    let mut twin = t12_genesis_chain(&scheduled, &bundle, &premine, &floats);
    let compared = replay_and_compare("scheduled twin", &launched.duel.victim, &mut twin).await;
    let x_out = TransactionOutpoint::new(launched.duel.x.id(), 0);
    let y_out = TransactionOutpoint::new(launched.duel.y.id(), 0);
    assert_eq!((has_utxo(&twin, x_out), has_utxo(&twin, y_out)), launched.held, "the same UTXO set: the twin double-spent too");
    eprintln!(
        "[hb-regression twin] {compared} blocks, {} of them the private branch: colored identically below the fence",
        launched.private.len()
    );
    // …and the same DAG through a node with the fence ARMED below it is a different chain: the
    // private blocks that borrowed are colored otherwise, which is the flag day.
    let armed = with_same_chain(&config, Some(ARMED_BELOW_THE_FORK));
    let mut other = t12_genesis_chain(&armed, &bundle, &premine, &floats);
    let mut differs = 0usize;
    for b in blocks_in_topological_order(&launched.duel.victim) {
        // A block whose header blue work the armed rule does not reproduce is refused as invalid —
        // stated, not avoided: the flag day is where the two builds stop sharing blocks.
        if other.ctx.consensus.validate_and_insert_block(b.clone()).virtual_state_task.await.is_err() {
            differs += 1;
            continue;
        }
        other.ctx.simulated_time = other.ctx.simulated_time.max(b.header.timestamp);
    }
    assert!(differs > 0, "past the fence the absorb DAG is not the armed build's chain ({differs} blocks refused)");
    let x_out_held = has_utxo(&other, x_out);
    assert!(x_out_held, "and the armed node keeps X");
}

/// **f — a bonded attacker: one attempt of its own plus one borrowed against two public attempts.**
/// As launched the public A1 is blue on the private branch, so one bond out-weighs two and the double
/// spend lands; past the fence A1 is red there, one attempt does not out-weigh two, and X stands.
#[tokio::test]
async fn hb_regression_f_one_bond_cannot_borrow_a_second_attempt() {
    kaspa_core::log::try_init_logger("warn");
    for same_chain in [None, Some(ARMED_BELOW_THE_FORK)] {
        let tag = if same_chain.is_some() { "f fenced" } else { "f launched" };
        let mut d = duel_armed(None, same_chain);
        for _ in 0..3 {
            honest_slot_mirrored(&mut d).await;
        }
        let fork = d.victim.sink();
        let holder = d.victim.heartbeat(1_000, vec![d.x.clone()]).await;
        let (a1, _) = d.victim.attempt(1, 1_000, Vec::new(), &|_| true).await;
        let clock = d.attacker.ctx.simulated_time + 1_000;
        let mut private = layer(&mut d.attacker, &mut d.nonce, 1, clock, vec![d.y.clone()]).await.expect("holder");
        let (b1, _) = d.attacker.attempt(2, 1_000, Vec::new(), &|_| true).await;
        private.push(b1);
        private.extend(private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await);
        mirror(&mut d.attacker, &holder).await;
        mirror(&mut d.attacker, &a1).await;
        assert!(private.iter().any(|b| b.header.hash == d.attacker.sink()), "{tag}: the attacker still builds on its private branch");
        let s = private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await;
        let gd = d.attacker.vp().ghostdag_store.get_data(s[0].header.hash).unwrap();
        eprintln!(
            "[hb-regression {tag}] the private block after the mirror: A1 blue {} red {}; attacker tip +{} blue work",
            gd.mergeset_blues.contains(&a1.header.hash),
            gd.mergeset_reds.contains(&a1.header.hash),
            bw(&d.attacker, s[0].header.hash) - bw(&d.attacker, fork)
        );
        let a1_blue = gd.mergeset_blues.contains(&a1.header.hash);
        private.extend(s);
        for _ in 0..8 {
            private.extend(private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await);
        }
        honest_slot(&mut d.victim, Vec::new()).await;
        d.victim.attempt(3, 1_000, Vec::new(), &|_| true).await;
        for _ in 0..7 {
            honest_slot(&mut d.victim, Vec::new()).await;
        }
        let r = release(tag, &mut d.victim, &private, fork, &mut d.nonce, 8).await;
        assert!(r.refused.is_empty(), "{tag}: every released block is valid");
        let (x, y) = payments(tag, &d);
        assert_same_coloring(tag, &d.attacker, &d.victim, &private);
        if same_chain.is_none() {
            assert!(a1_blue, "{tag}: the public A1 is BLUE on the private branch — the hole");
            assert!(r.flipped() && !x && y, "{tag}: one bond plus a borrowed attempt out-weighs two — the double spend lands");
        } else {
            assert!(!a1_blue, "{tag}: the public A1 is RED on the private branch — F1");
            assert!(!r.flipped(), "{tag}: one attempt and a slot's worth of extra beats do not out-weigh two attempts");
            assert!(x && !y, "{tag}: X stands and Y is not in the victim's UTXO set");
        }
    }
}

/// Mines every heartbeat slot that opens before `lands_at` — `m` heartbeat miners, `m` siblings a
/// layer (one template each) — and returns how many heartbeats that was.
async fn beat_until(c: &mut T12Chain, nonce: &mut u64, lands_at: u64, m: usize) -> usize {
    let mut minted = 0usize;
    while ts(c, c.sink()) + I <= lands_at {
        minted += if m == 1 { honest_slot(c, Vec::new()).await.len() } else { private_slot(c, nonce, m, Vec::new()).await.len() };
    }
    minted
}

/// Lands `draw` `delay_ms` after its template on `c` — every heartbeat slot that opens before then
/// mined first by `m` heartbeat miners — and returns the heartbeat that merges it and how many
/// heartbeats were minted while it was drawn.
async fn land_late_m(c: &mut T12Chain, nonce: &mut u64, draw: &Block, delay_ms: u64, m: usize) -> (Block, usize) {
    let lands_at = draw.header.timestamp + delay_ms;
    let minted = beat_until(c, nonce, lands_at, m).await;
    c.ctx.simulated_time = c.ctx.simulated_time.max(lands_at);
    c.ctx.consensus.validate_and_insert_block(draw.clone()).virtual_state_task.await.expect("a late draw is a valid block");
    assert_ne!(c.sink(), draw.header.hash, "a draw on its template's parents lands behind the heartbeat tip");
    (c.heartbeat(1_000, Vec::new()).await, minted)
}

/// [`land_late_m`] with one heartbeat miner.
async fn land_late(c: &mut T12Chain, draw: &Block, delay_ms: u64) -> Block {
    let mut nonce = 1u64 << 44;
    land_late_m(c, &mut nonce, draw, delay_ms, 1).await.0
}

/// A draw that lands `delay_ms` after its template on testnet-12, while the chain goes on at the
/// 120 s heartbeat slots with `m` heartbeat miners, is merged by the next heartbeat. Returns whether
/// that heartbeat colored it BLUE, the blue work the merger added over its selected parent, and how
/// many heartbeats were minted while it was drawn. `transparent: false` is the same network with
/// ADR-0105 taken away — classic GHOSTDAG, the control; `same_chain` arms F1's fence.
async fn a_late_draw_on_t12(transparent: bool, same_chain: Option<u64>, delay_ms: u64, m: usize) -> (bool, i128, usize) {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let config = if transparent {
        with_same_chain(&config, same_chain)
    } else {
        let mut params = config.params.clone();
        params.palw_heartbeat_transparent = None;
        ConfigBuilder::new(params).skip_proof_of_work().build()
    };
    assert_eq!(config.params.palw_heartbeat_transparent_fence().is_some(), transparent);
    assert_eq!(config.params.ghostdag_k(), 1, "testnet-12's k: two blues in an anticone make a block red");
    let mut c = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut nonce = 1u64 << 41;
    for _ in 0..3 {
        honest_slot(&mut c, Vec::new()).await;
    }
    // The producer takes its template now, on the tip it sees, and draws.
    let (draw, _) = c.build_attempt(1, 1_000, Vec::new(), &|_| true);
    let draw = draw.to_immutable();
    let (merger, minted) = land_late_m(&mut c, &mut nonce, &draw, delay_ms, m).await;
    let gd = c.vp().ghostdag_store.get_data(merger.header.hash).unwrap();
    assert!(gd.unordered_mergeset().any(|h| h == draw.header.hash), "the next heartbeat merges the draw");
    let blue = gd.mergeset_blues.contains(&draw.header.hash);
    let added = bw(&c, merger.header.hash) - bw(&c, gd.selected_parent);
    eprintln!(
        "[hb-regression late-draw] transparent {transparent}, same-chain {same_chain:?}, landed {:.0} s after its template, {m} miner(s), {minted} heartbeats minted meanwhile: blue {blue}, the merger added {added}",
        delay_ms as f64 / 1_000.0,
    );
    (blue, added, minted)
}

/// **ADR-0105's property, kept past F1's fence, on testnet-12: a slow producer's bonded block is not
/// turned red by the heartbeats minted on its own chain while it was computing.** An 8k draw takes
/// ~200 s of inference; at 120 s heartbeat slots one slot opens meanwhile (two beats a miner). With one
/// heartbeat miner and with two (two siblings a layer, as the fleet mined), the draw's selected parent
/// is still on the chain when it lands, so the heartbeat that merges it colors it BLUE and carries its
/// whole 2^20 — as launched and past the fence alike. The control — the same network with ADR-0105
/// taken away — colors it RED, which is the 2026-09-10 incident: the test exercises the rule, not a
/// DAG that would be blue anyway.
#[tokio::test]
async fn hb_regression_an_8k_draw_200s_late_stays_blue_on_its_own_chain() {
    let attempt = 1i128 << 20;
    for m in [1usize, 2] {
        for same_chain in [None, Some(ARMED_BELOW_THE_FORK)] {
            let (blue, added, minted) = a_late_draw_on_t12(true, same_chain, 200_000, m).await;
            assert!(
                minted >= 2,
                "{m} miner(s), {same_chain:?}: heartbeats were minted on the draw's chain while it was drawn ({minted})"
            );
            assert!(
                blue,
                "{m} miner(s), {same_chain:?}: the draw hangs from the merger's chain, so heartbeats do not count against it"
            );
            assert!(
                added > attempt,
                "{m} miner(s), {same_chain:?}: the merging heartbeat carries the draw's whole 2^20 (added {added})"
            );
        }
        let (classic_blue, classic_added, _) = a_late_draw_on_t12(false, None, 200_000, m).await;
        assert!(!classic_blue && classic_added < attempt, "{m} miner(s): without ADR-0105 the same DAG makes it red — the control");
    }
}

/// **The fence crosses an honest testnet-12 chain, and nothing but the rule's name changes.** F1 armed
/// at DAA 8 (the operator will pick a common height for every post-launch fence; 8 is what the harness
/// reaches in a few minutes of slots). The chain runs the fleet's shapes across it: one and two
/// heartbeat miners, 200 s draws landing behind the heartbeat tip, a draw that lands before any
/// heartbeat (a chain block), two draws on one template, and heartbeat-only stretches on both sides.
///
/// * **The clock runs**: every honest slot steps the DAA score by exactly one, before, at and after
///   the height, and the chain ends well past it — the flag-day freeze of 2026-09-18 (a fence that
///   stopped whatever ticked the clock) cannot hide here, because the slots are counted one by one.
/// * **Every draw is blue**, on both sides of the height: a late draw hangs from the chain it lands on.
/// * **The launched build agrees, block for block**: a node with the fence absent, fed this chain,
///   colors every block identically and ends on the same sink — on an honest chain the flag day has
///   nothing to disagree about, so a straggler is refused by the fork id, not by a fork.
#[tokio::test]
async fn hb_regression_the_fence_crosses_an_honest_chain_and_the_clock_runs() {
    const H: u64 = 8;
    kaspa_core::log::try_init_logger("warn");
    let (launched, bundle, premine, floats) = t12_with_harness_cards();
    let armed = with_same_chain(&launched, Some(H));
    let mut c = t12_genesis_chain(&armed, &bundle, &premine, &floats);
    let mut nonce = 1u64 << 45;
    let mut draws: Vec<(Block, Block)> = Vec::new();
    // Every honest slot must step the clock by exactly one — read at each slot's last block.
    let mut steps: Vec<(u64, u64)> = Vec::new();
    async fn stepped(c: &mut T12Chain, steps: &mut Vec<(u64, u64)>) {
        let before = c.daa_of(c.sink());
        honest_slot(c, Vec::new()).await;
        steps.push((before, c.daa_of(c.sink())));
    }
    for _ in 0..3 {
        stepped(&mut c, &mut steps).await;
    }
    // Below the height: a 200 s draw with one heartbeat miner, then one with two.
    for m in [1usize, 2] {
        let d = c.build_attempt(m, 1_000, Vec::new(), &|_| true).0.to_immutable();
        let (merger, _) = land_late_m(&mut c, &mut nonce, &d, 200_000, m).await;
        draws.push((d, merger));
        // The merging beat may be a slot's holder: finish the slot, so the next draw's heartbeat
        // miners start on a slot boundary.
        stepped(&mut c, &mut steps).await;
    }
    // A heartbeat-only stretch across the height.
    while c.daa_of(c.sink()) < H + 2 {
        stepped(&mut c, &mut steps).await;
    }
    // Past the height: an attempt that lands before any heartbeat (it becomes the sink), two draws on
    // one template (the loser of the hash tie is a candidate beside the winner), and 200 s draws with
    // one and two heartbeat miners.
    let (chain_attempt, _) = c.attempt(3, 1_000, Vec::new(), &|_| true).await;
    stepped(&mut c, &mut steps).await;
    let (t1, _) = c.build_attempt(4, 1_000, Vec::new(), &|_| true);
    let (t2, _) = c.build_attempt(5, 0, Vec::new(), &|_| true);
    let (t1, t2) = (t1.to_immutable(), t2.to_immutable());
    assert_eq!(t1.header.direct_parents(), t2.header.direct_parents(), "two draws on one template");
    for t in [&t1, &t2] {
        c.ctx.consensus.validate_and_insert_block(t.clone()).virtual_state_task.await.expect("a draw on the tip is valid");
    }
    let twin_merger = c.heartbeat(1_000, Vec::new()).await;
    stepped(&mut c, &mut steps).await;
    for m in [1usize, 2] {
        let d = c.build_attempt(5 + m, 1_000, Vec::new(), &|_| true).0.to_immutable();
        let (merger, _) = land_late_m(&mut c, &mut nonce, &d, 200_000, m).await;
        draws.push((d, merger));
        stepped(&mut c, &mut steps).await;
    }
    // A heartbeat-only stretch to finish.
    for _ in 0..6 {
        stepped(&mut c, &mut steps).await;
    }

    let gd = |h: BlockHash| c.vp().ghostdag_store.get_data(h).unwrap();
    for (i, (d, merger)) in draws.iter().enumerate() {
        let side = if d.header.daa_score < H { "below" } else { "past" };
        assert!(
            gd(merger.header.hash).mergeset_blues.contains(&d.header.hash),
            "draw {i} (DAA {}, {side} the fence) is blue",
            d.header.daa_score
        );
    }
    assert!(draws[..2].iter().all(|(d, _)| d.header.daa_score < H), "two draws minted below the height");
    assert!(draws[2..].iter().all(|(d, _)| d.header.daa_score >= H), "two draws minted past it");
    assert!(chain_attempt.header.daa_score >= H, "the chain attempt is past the height");
    let tm = gd(twin_merger.header.hash);
    for t in [&t1, &t2] {
        assert!(
            tm.selected_parent == t.header.hash || tm.mergeset_blues.contains(&t.header.hash),
            "both draws on one template are blue past the height ({})",
            t.header.hash
        );
    }
    for (before, after) in &steps {
        assert_eq!(after - before, 1, "an honest slot steps the clock by exactly one ({before} -> {after})");
    }
    assert!(steps.iter().any(|(b, _)| *b < H) && steps.iter().any(|(b, _)| *b >= H), "slots on both sides of the height");
    let end = c.daa_of(c.sink());
    assert!(end >= H + 8, "the chain ends well past the height (DAA {end})");
    eprintln!(
        "[hb-regression crossing] fence at DAA {H}; {} slots stepped one by one; the chain ends at DAA {end}; draws at DAA {:?}, all blue",
        steps.len(),
        draws.iter().map(|(d, _)| d.header.daa_score).collect::<Vec<_>>()
    );

    let mut straggler = t12_genesis_chain(&launched, &bundle, &premine, &floats);
    let compared = replay_and_compare("launched straggler", &c, &mut straggler).await;
    eprintln!("[hb-regression crossing] a node without the fence colors all {compared} blocks identically");
}

/// **F1 through testnet-12's pruning proof: built, validated, applied — and colored the same by the
/// node that applied it,** as launched and with the fence armed. testnet-12's ruleset with the depths
/// `t12_a_moved_pruning_point_s_proof_builds_validates_and_applies_at_225` shrinks (finality 20,
/// pruning 50, `m` 10; archival), and below the pruning point two ~200 s draws: one drawn on a tip that
/// stays on the chain (it hangs from its merger's chain: blue either way), and one drawn on the
/// heartbeat that then loses its sibling tie — blue as launched, and past the fence colored
/// classically, i.e. red: the case F1 gives up, stated. A testnet-12 proof's level 0 is the whole
/// history below the pruning point, and a node that applies it recomputes GHOSTDAG over both draws
/// from the GHOSTDAG data the proof rebuilds (`apply_proof`), whose reachability tree is filled by
/// header blue work, not grown block by block: every level-0 header must come out with the source's
/// selected parent, blues and reds, and the staging node must build back the proof it was sent.
#[tokio::test]
async fn hb_regression_the_same_chain_rule_colors_the_same_through_the_pruning_proof() {
    for same_chain in [None, Some(ARMED_BELOW_THE_FORK)] {
        let draw2_blue = proof_round_trip(same_chain).await;
        assert_eq!(
            draw2_blue,
            same_chain.is_none(),
            "{same_chain:?}: the sibling-tie draw is blue as launched and red past the fence"
        );
    }
}

/// One pruning-proof round trip for [`hb_regression_the_same_chain_rule_colors_the_same_through_the_pruning_proof`];
/// returns whether draw 2 (the sibling-tie draw) is blue.
async fn proof_round_trip(same_chain: Option<u64>) -> bool {
    use super::TestContext;
    use super::t12_clock_floor::t12_trusted_set;
    use crate::consensus::test_consensus::TestConsensus;
    use crate::model::services::reachability::ReachabilityService;
    use kaspa_consensus_core::BlockHashSet;
    use kaspa_consensus_core::pruning::PruningProofMetadata;
    kaspa_core::log::try_init_logger("warn");
    let (shipped, bundle, premine, floats) = t12_with_harness_cards();
    let mut config = with_same_chain(&shipped, same_chain);
    config.params.blockrate.finality_depth = 20;
    config.params.blockrate.pruning_depth = 50;
    config.params.pruning_proof_m = 10;
    config.is_archival = true;
    let m = config.params.pruning_proof_m;
    let genesis = config.params.genesis.hash;
    let mut c = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut nonce = 1u64 << 42;
    for _ in 0..3 {
        honest_slot(&mut c, Vec::new()).await;
    }

    // Draw 1: drawn on a tip that stays on the chain.
    let d1 = c.build_attempt(1, 1_000, Vec::new(), &|_| true).0.to_immutable();
    let m1 = land_late(&mut c, &d1, 200_000).await;
    let g1 = c.vp().ghostdag_store.get_data(m1.header.hash).unwrap();
    assert!(g1.mergeset_blues.contains(&d1.header.hash), "draw 1 hangs from its merger's chain: blue");

    // Draw 2: two sibling heartbeats from one template; the draw's template is taken while only the
    // first has arrived, and the second sorts above it (equal blue work, larger hash), so the chain
    // goes on through the second.
    let clock = c.ctx.simulated_time + 1_000;
    let mut t = c
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&c.config.params, &mut t.block.header, clock);
    t.block.header.finalize();
    let (t, _) = c.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
    let sibling = |n: &mut u64| {
        *n += 1;
        let mut b = t.block.clone();
        b.header.nonce = *n;
        b.header.finalize();
        b.to_immutable()
    };
    let (x, y) = (sibling(&mut nonce), sibling(&mut nonce));
    let (loser, winner) = if x.header.hash < y.header.hash { (x, y) } else { (y, x) };
    mirror(&mut c, &loser).await;
    let d2 = c.build_attempt(2, 1_000, Vec::new(), &|_| true).0.to_immutable();
    mirror(&mut c, &winner).await;
    assert_eq!(c.sink(), winner.header.hash, "the second sibling takes the tie");
    let m2 = land_late(&mut c, &d2, 200_000).await;
    let g2 = c.vp().ghostdag_store.get_data(m2.header.hash).unwrap();
    assert_eq!(c.vp().ghostdag_store.get_selected_parent(d2.header.hash).unwrap(), loser.header.hash);
    assert!(!c.ctx.consensus.services.reachability_service.is_chain_ancestor_of(loser.header.hash, m2.header.hash));
    let draw2_blue = g2.mergeset_blues.contains(&d2.header.hash);
    assert!(draw2_blue || g2.mergeset_reds.contains(&d2.header.hash), "draw 2 is merged by the heartbeat after it");

    // Heartbeats until the headers declare a settled pruning point with both mergers below it.
    let headers = c.vp().headers_store.clone();
    let settle = config.params.anticone_finalization_depth() + 4;
    let below = headers.get_blue_score(m2.header.hash).unwrap();
    let mut slots = 0u64;
    let pp = loop {
        honest_slot(&mut c, Vec::new()).await;
        slots += 1;
        let sink = headers.get_header(c.sink()).unwrap();
        let declared = headers.get_blue_score(sink.pruning_point).unwrap();
        if declared >= 6 * m && declared > below && sink.blue_score >= declared + settle {
            break sink.pruning_point;
        }
        assert!(slots < 300, "no header declared a settled pruning point in {slots} slots");
    };
    let sink = c.sink();
    let relay = headers.get_header(sink).unwrap();
    let trusted = t12_trusted_set(&c.ctx.consensus, pp, sink);
    c.ctx.consensus.intrusive_pruning_point_update(pp, sink).unwrap_or_else(|e| panic!("{pp} is not a pruning point: {e}"));
    let proof = c.ctx.consensus.get_pruning_point_proof();
    let level0: BlockHashSet = proof[0].iter().map(|h| h.hash).collect();
    for h in [d1.header.hash, m1.header.hash, loser.header.hash, winner.header.hash, d2.header.hash, m2.header.hash] {
        assert!(level0.contains(&h), "{h} is in the proof's level 0");
    }

    // A node at genesis validates it.
    let fresh = t12_genesis_chain(&config, &bundle, &premine, &floats);
    fresh
        .ctx
        .consensus
        .validate_pruning_proof(&proof, &PruningProofMetadata::new(relay.blue_work))
        .unwrap_or_else(|e| panic!("a node at genesis refused the proof: {e}"));
    // A staging node applies it and colors every level-0 header as the source did.
    let mut staging_config = config.clone();
    staging_config.process_genesis = false;
    staging_config.is_archival = false;
    let staging = TestContext::new(TestConsensus::new(&staging_config));
    staging.consensus.apply_pruning_proof((*proof).clone(), &trusted).unwrap_or_else(|e| panic!("the proof did not apply: {e}"));
    staging
        .consensus
        .import_pruning_points(c.ctx.consensus.pruning_point_headers())
        .unwrap_or_else(|e| panic!("the pruning points: {e}"));
    let (source, applied) = (c.ctx.consensus.ghostdag_store(), staging.consensus.ghostdag_store());
    for header in proof[0].iter().filter(|h| h.hash != genesis) {
        let (s, a) = (source.get_data(header.hash).unwrap(), applied.get_data(header.hash).unwrap());
        assert_eq!(
            (s.selected_parent, &s.mergeset_blues, &s.mergeset_reds),
            (a.selected_parent, &a.mergeset_blues, &a.mergeset_reds),
            "{same_chain:?} {}: the applying node colors it as the source did",
            header.hash
        );
    }
    assert!(applied.get_data(m1.header.hash).unwrap().mergeset_blues.contains(&d1.header.hash));
    assert_eq!(applied.get_data(m2.header.hash).unwrap().mergeset_blues.contains(&d2.header.hash), draw2_blue);
    let rebuilt = staging.consensus.get_pruning_point_proof();
    for (level, (sent, built)) in proof.iter().zip(rebuilt.iter()).enumerate() {
        assert_eq!(
            sent.iter().map(|h| h.hash).collect::<BlockHashSet>(),
            built.iter().map(|h| h.hash).collect::<BlockHashSet>(),
            "{same_chain:?} level {level}: the staging node builds the proof it was sent"
        );
    }
    eprintln!(
        "[hb-regression proof] same-chain {same_chain:?}: pruning point at blue score {} after {slots} slots; level 0 {} headers; draw 1 blue, draw 2 {} — on the source and on the node that applied the proof",
        headers.get_blue_score(pp).unwrap(),
        proof[0].len(),
        if draw2_blue { "blue" } else { "red" }
    );
    draw2_blue
}

// ─────────────────────────────────────────────────────────────────────────────────────────────
// lane: rcore/f1-forkchoice-attacks — the dormant fence `palw_reorg_strict_economic_win`, exercised
// through the real pipeline. Two things a unit test cannot see (the `a-flag-day-needs-a-drill-that-
// crosses-it` lesson: fork-choice/colouring fences have frozen the DAA clock before while every unit
// test was green): (1) a chain that CROSSES the armed fence must keep ticking the DAA, and (2) the
// fence must actually change a reorg outcome end to end — the tie both `hb_probe_b_*` probes above
// measure being Allowed on the shipped rule is Refused once it is armed, and X stands.
// ─────────────────────────────────────────────────────────────────────────────────────────────

/// [`duel`], with `palw_reorg_strict_economic_win` armed at `fence_daa`. Nothing else moves; the
/// genesis, premine and every window are testnet-12's, and the ruleset is re-validated.
fn duel_reorg_armed(fence_daa: u64) -> Duel {
    use kaspa_consensus_core::config::params::ForkActivation;
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut params = config.params.clone();
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(fence_daa));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("arming the reorg fence is a runnable testnet-12 ruleset");
    assert_eq!(
        config.params.palw_reorg_strict_economic_win,
        Some(ForkActivation::new(fence_daa)),
        "the fence is armed at {fence_daa}"
    );
    let victim = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let attacker = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let pay = |to: ScriptPublicKey| {
        let (outpoint, entry) = floats[1].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - 300_000, to)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign_spend(&mut tx, entry, 1, config.params.storage_mass_parameter);
        tx
    };
    let x = pay(card_payout_spk(5));
    let y = pay(card_payout_spk(6));
    Duel { config, victim, attacker, x, y, nonce: 1 << 40, floats }
}

/// **The flag-day drill: a chain crosses the armed fence and the DAA keeps advancing.** Armed at a
/// LOW height so the drill passes THROUGH it (arrival is not the test — a fence that retires the
/// only clock freezes the DAA at exactly the height it fires). Honest heartbeat slots are mined from
/// genesis to well past the fence; each slot must tick the DAA by one, and every block must stay
/// UTXO-valid and the sink (`honest_slot`/`heartbeat` assert the sink). The fence changes only the
/// deep-reorg comparator, so forward progress must be untouched — this proves it.
#[tokio::test]
async fn hb_probe_fence_the_armed_fence_is_crossed_with_the_daa_advancing() {
    kaspa_core::log::try_init_logger("warn");
    const FENCE_AT: u64 = 5;
    let mut d = duel_reorg_armed(FENCE_AT);
    assert!(d.config.params.palw_reorg_strict_economic_win.unwrap().is_active(FENCE_AT), "active at the fence height");
    let mut last = d.victim.daa_of(d.victim.sink());
    let mut crossed = false;
    for slot in 0..(FENCE_AT + 6) {
        honest_slot(&mut d.victim, if slot == 0 { vec![d.x.clone()] } else { Vec::new() }).await;
        let daa = d.victim.daa_of(d.victim.sink());
        assert_eq!(daa, last + 1, "slot {slot}: the DAA advances by one across the fence — it is not frozen at {FENCE_AT}");
        if daa > FENCE_AT {
            crossed = true;
        }
        last = daa;
    }
    assert!(crossed, "the chain advanced past the armed fence at {FENCE_AT}");
    // The blocks below and above the fence are all real chain: X is still in the UTXO set.
    let x_out = TransactionOutpoint::new(d.x.id(), 0);
    assert!(has_utxo(&d.victim, x_out), "X is accepted and the chain keeps producing across the fence");
    eprintln!(
        "[hb-probe fence-drill] armed at DAA {FENCE_AT}: mined {} slots, sink DAA {} — the clock crossed the fence and never stalled",
        FENCE_AT + 6,
        last
    );
}

/// **The fence changes the reorg outcome end to end.** The same heartbeat-only economic tie that
/// `hb_probe_b_sibling_beats_outweigh_a_heartbeat_only_chain` (m = 2) lets flip on the shipped rule —
/// the private branch is heavier on blue work, both sides read `{frontier 0, safe 0, live 0}`, and
/// `decide_deep_reorg_v2` Allows on the candidate hash — is REFUSED once the fence is armed: the
/// private branch is still offered (heavier blue work tops the heap), but the deep-reorg gate keeps
/// the incumbent on the all-economic tie, so the double spend does NOT land and X stands.
#[tokio::test]
async fn hb_probe_fence_refuses_the_tied_reorg_the_shipped_rule_allows() {
    kaspa_core::log::try_init_logger("warn");
    // Armed at DAA 1 — active by the time the fork forms (the incumbent sits well past it).
    let mut d = duel_reorg_armed(1);
    for _ in 0..3 {
        honest_slot_mirrored(&mut d).await;
    }
    let fork = d.victim.sink();
    honest_slot(&mut d.victim, vec![d.x.clone()]).await;
    for _ in 0..9 {
        honest_slot(&mut d.victim, Vec::new()).await;
    }
    let mut private = private_slot(&mut d.attacker, &mut d.nonce, 2, vec![d.y.clone()]).await;
    for _ in 0..9 {
        private.extend(private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await);
    }
    let r = release("fence m=2", &mut d.victim, &private, fork, &mut d.nonce, 8).await;
    assert!(r.refused.is_empty(), "every private beat is a valid block");
    assert!(r.private_bw_max > r.public_bw, "the private branch is heavier on blue work (offered), as in hb_probe_b m=2");
    assert!(r.offered_at.is_some(), "and it does reach the top of the heap — the fence acts at the reorg gate, not the heap");
    let depth = bs(&d.victim, d.victim.sink()) - bs(&d.victim, fork);
    assert!(depth < d.config.params.finality_depth(), "inside the finality depth, so only the reorg gate can refuse it ({depth})");
    let (x, y) = payments("fence m=2", &d);
    assert!(!r.flipped(), "the armed fence keeps the incumbent on the all-economic tie: no flip");
    assert!(x && !y, "X stands — the double spend hb_probe_b lands on the shipped rule does not land past the fence");
    eprintln!(
        "[hb-probe fence m=2] private heavier (+{} vs +{} blue work), offered at {:?}, flipped {:?}: the fence refused the tied reorg and X stands",
        r.private_bw_max, r.public_bw, r.offered_at, r.flipped_at
    );
}

/// **Verdict 1, the exact claim: a block whose attempt did NOT win the lottery still earns 2^20
/// blue work.** `hb_probe_a_*` measured that a winning attempt adds 2^20; this builds a shape-valid,
/// correctly signed attempt header by a registered card whose class ticket LOSES its lottery
/// (`class_ticket_v3 > class_target`) as a child of the sink, and inserts it. The lottery is
/// chain-relative, so it cannot gate DAG entry: the header is admitted, while the virtual processor
/// refuses it the SELECTED CHAIN (`palw_v2_check_attempt_admission` fails the lottery →
/// `StatusDisqualifiedFromChain`). A block's own work is carried by the blocks that merge it
/// (`blue_work = selected parent's + Σ work(mergeset blues)`), so its weight is read off the
/// GHOSTDAG data this node's own manager computes for the children it could have:
/// * a child whose only parent is the losing attempt adds exactly 2^20 over it;
/// * the attack shape: an honest heartbeat sibling H of the losing attempt L (same parent), and a
///   block merging {H, L}: L is coloured BLUE (ADR-0105 transparency: a heartbeat never reddens an
///   attempt) and the merging block carries L's 2^20 on top of the sink's — when H is its selected
///   parent, that is a VALID chain block (H → sink) weighing a lottery loss as a full attempt.
#[tokio::test]
async fn hb_probe_verdict1_a_losing_lottery_attempt_still_earns_2_20_blue_work() {
    kaspa_core::log::try_init_logger("warn");
    let mut d = duel(None);
    for _ in 0..2 {
        honest_slot(&mut d.victim, Vec::new()).await;
    }
    let sink = d.victim.sink();
    let sink_bw = bw(&d.victim, sink);
    let vp = d.victim.vp();

    // The LOSING attempt, a child of the sink.
    let (lose_block, _) = d.victim.build_attempt_drawn(2, 1_000, Vec::new(), &|_| true, false);
    let lose = lose_block.header.hash;
    assert!(kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(lose_block.header.pow_algo_id), "on the attempt lane");
    assert_eq!(lose_block.header.direct_parents().to_vec(), vec![sink], "a child of the sink");
    let inserted = d.victim.ctx.consensus.validate_and_insert_block(lose_block.to_immutable()).virtual_state_task.await;
    let lose_status = d.victim.ctx.consensus.block_status(lose);
    assert!(inserted.is_ok(), "the header is admitted: the lottery cannot gate DAG entry ({inserted:?})");
    assert_eq!(
        lose_status,
        kaspa_consensus_core::blockstatus::BlockStatus::StatusDisqualifiedFromChain,
        "and the chain refuses it: it did not win"
    );
    assert_eq!(d.victim.sink(), sink, "so the sink does not move");

    // (1) A child with the losing attempt as its only parent: the losing attempt's own work.
    let only = vp.ghostdag_manager.ghostdag(&[lose]);
    let lose_work = only.blue_work.as_u128() as i128 - bw(&d.victim, lose);
    // (2) The attack shape: an honest heartbeat sibling H of L, then a block merging {H, L}.
    let h = d.victim.heartbeat(1_000, Vec::new()).await;
    assert_eq!(h.header.direct_parents().to_vec(), vec![sink], "H is L's sibling: the virtual does not merge a disqualified tip");
    let both = vp.ghostdag_manager.ghostdag(&[h.header.hash, lose]);
    let l_blue = both.selected_parent == lose || both.mergeset_blues.contains(&lose);
    let added = both.blue_work.as_u128() as i128 - sink_bw;
    let over_honest = both.blue_work.as_u128() as i128 - vp.ghostdag_manager.ghostdag(&[h.header.hash]).blue_work.as_u128() as i128;
    eprintln!(
        "[hb-probe verdict1] LOSING-lottery attempt {lose}: status {lose_status:?}, its own work (carried by a child) +{lose_work}; \
         a block merging {{honest sibling H, L}}: L blue = {l_blue}, selected parent = {} ({}), blue work +{added} over the sink, \
         +{over_honest} over the same block without L",
        if both.selected_parent == h.header.hash { "H" } else { "L" },
        if both.selected_parent == h.header.hash { "a valid chain H -> sink" } else { "chain through L: disqualified" },
    );
    assert_eq!(lose_work, 1 << 20, "a LOSING-lottery attempt carries the same 2^20 as a winning one — header work alone earns blue work");
    assert!(l_blue, "merged beside an honest heartbeat, the losing attempt is coloured blue");
    assert_eq!(over_honest, 1 << 20, "and the merging block weighs exactly 2^20 more for it");
}
