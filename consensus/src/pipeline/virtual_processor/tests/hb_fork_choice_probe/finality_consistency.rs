//! **FINX (2026-10-08) — P0: is the GHOSTDAG finality guard consistent with the PALW fork choice?
//! Multi-node adversarial runs through the real pipeline.** A probe: no rule is changed.
//!
//! Record: `docs/design/palw/finality-palw-consistency.md`. The model that compares the change
//! candidates is `consensus/core/tests/finality_palw_consistency_model.rs`.
//!
//! The property tested, in two halves:
//! * **(C1) Agreement.** Two honest nodes holding the same DAG, neither of whose finality point
//!   excludes the other's chain, select the same sink — the chain is a function of the information,
//!   not of the order it arrived in.
//! * **(C2) No seal against the authority.** A node's finality point does not pass a fork while the
//!   node holds a fully validated competing branch that its own PALW rule ranks above its chain, or
//!   would once the information it is waiting for arrives.
//!
//! Every test here builds two or more testnet-12 nodes (harness cards, every window as shipped, the
//! fork-choice set testnet-12 runs past DAA 1,700: `palw_reorg_strict_economic_win`, lane A's operator
//! anchor with F1's execution seed, F-W), partitions them by simply not mirroring blocks, heals by
//! mirroring, and asserts either convergence or the permanent split, naming the rule that holds it and
//! — where the finality depth is what makes it permanent — how long the seal takes.
//!
//! Where a test needs the seal inside a debug-build budget it lowers `finality_depth` to 60 blue score
//! (both nodes alike; `validate_palw_v2` bounds the depth only from above) and the record extrapolates
//! with the blue score per slot `finx_p0_facts_*` measures.
use super::super::t12_round_lane_e2e::{t12_genesis_chain_on, t12_reopened_chain};
use super::*;
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_fork_authority_v2::{
    PalwDeepReorgV2, PalwIbdCommitV2, palw_ibd_commit_strict_economic_v1, palw_reorg_strict_economic_win_v1,
};
use kaspa_consensus_core::palw_fork_choice::{PalwCandidateOrderV1, compare_palw_candidates_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2;
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use kaspa_hashes::Hash64;

type Utxos = Vec<(TransactionOutpoint, UtxoEntry)>;

/// testnet-12's fork-choice set past DAA 1,700 armed from DAA 1 (strict-win, lane A + F1's seed, F-W —
/// LIVE-R1's reproduction runs the same), optionally with `finality_depth` lowered.
fn parts(finality_depth: Option<u64>) -> (Config, PalwConsensusParamsV2, Utxos, Utxos) {
    parts_ruled(finality_depth, false)
}

/// [`parts`], and — when `rule_e` — ADR-0178's rule E armed from DAA 1 on top ([`armed_rule_e`]).
fn parts_ruled(finality_depth: Option<u64>, rule_e: bool) -> (Config, PalwConsensusParamsV2, Utxos, Utxos) {
    let (config, _, premine, floats) = t12_with_harness_cards();
    let mut params: Params = config.params.clone();
    params.palw_reorg_strict_economic_win = Some(ForkActivation::new(1));
    for name in ["palw_panel_seed_execution", "palw_operator_anchor"] {
        (post_launch_entry(name).set)(&mut params, Some(ForkActivation::new(1)));
    }
    params.palw_capacity_weight_cap = Some(ForkActivation::new(1));
    params.sync_palw_capacity_weight_cap();
    if let Some(depth) = finality_depth {
        assert!(depth >= params.merge_depth(), "merge depth <= finality depth (the prune-safety argument)");
        params.blockrate.finality_depth = depth;
    }
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("a runnable testnet-12 ruleset");
    let config = if rule_e { armed_rule_e(config) } else { config };
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else {
        unreachable!("ConsensusV2")
    };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

/// **ADR-0178's rule E armed from DAA 1** on a ruleset that validated without it. Arming it is refused in this
/// binary (`PALW_FORK_CHOICE_RULE_E_ARMABLE_V1`), so the Config is set directly — after asserting that rule E's own
/// refusal is the only one the armed ruleset meets (`validate_palw_v2` asks it last).
fn armed_rule_e(mut config: Config) -> Config {
    config.params.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(1));
    assert_eq!(
        config.params.validate_palw_v2(),
        Err(kaspa_consensus_core::palw_mode_v2::PalwModeV2Error::Invalid(
            kaspa_consensus_core::palw_fork_choice_rule_e_v1::PALW_FORK_CHOICE_RULE_E_UNARMABLE_V1
        )),
        "rule E armed is refused by its own name only"
    );
    config
}

/// Two nodes on one ruleset, and the payments X (to the merchant) and Y (back to the payer) that spend
/// card 1's fee float.
struct Net {
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Utxos,
    floats: Utxos,
    /// The node whose side mines more blue work (two producers race every slot).
    heavy: T12Chain,
    /// The node whose side mines one producer's slots.
    light: T12Chain,
    nonce: u64,
    x: Transaction,
    y: Transaction,
}

fn pay(config: &Config, floats: &Utxos, to: ScriptPublicKey) -> Transaction {
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
}

fn net(finality_depth: Option<u64>) -> Net {
    net_ruled(finality_depth, false)
}

fn net_ruled(finality_depth: Option<u64>, rule_e: bool) -> Net {
    let (config, bundle, premine, floats) = parts_ruled(finality_depth, rule_e);
    let heavy = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let light = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (x, y) = (pay(&config, &floats, card_payout_spk(5)), pay(&config, &floats, card_payout_spk(6)));
    Net { config, bundle, premine, floats, heavy, light, nonce: 1 << 40, x, y }
}

impl Net {
    fn fresh_node(&self) -> T12Chain {
        t12_genesis_chain(&self.config, &self.bundle, &self.premine, &self.floats)
    }
}

/// Mirror `blocks` (parents first) into `to`, skipping any it already holds.
async fn feed(to: &mut T12Chain, blocks: &[Block]) {
    for b in blocks {
        if to.ctx.consensus.get_block_status(b.header.hash).is_some_and(|s| s.has_block_body()) {
            continue;
        }
        mirror(to, b).await;
    }
}

/// The PALW keys `c` reads for `tip`.
fn keys(c: &T12Chain, tip: BlockHash) -> PalwCandidateOrderV1 {
    c.vp().palw_candidate_order_v2(tip).unwrap_or_else(|| panic!("{tip} is weighable"))
}

fn econ(o: &PalwCandidateOrderV1) -> (u64, u128, u128) {
    (o.safe_frontier_blue_score, o.safe_weight, o.live_total)
}

/// Whether `node`'s finality point excludes `other_tip` — the sink search refuses every candidate on
/// that branch before the PALW gate is asked (`candidate_at_or_above_finality`).
fn sealed_against(node: &T12Chain, other_tip: BlockHash) -> bool {
    let fp = node.ctx.consensus.finality_point();
    !node.ctx.consensus.is_chain_ancestor_of(fp, other_tip).unwrap_or(false)
}

/// Whether `node`'s finality point stands strictly above `fork` on its own chain: every branch off
/// `fork` is then below it, whether or not the node has seen it yet.
fn sealed_past(node: &T12Chain, fork: BlockHash) -> bool {
    let fp = node.ctx.consensus.finality_point();
    fp != fork && node.ctx.consensus.is_chain_ancestor_of(fork, fp).unwrap_or(false)
}

/// One slot on each side, exchanged: the heavy side races two producers, the light side mines one.
async fn exchange_round(n: &mut Net) -> (Vec<Block>, Vec<Block>) {
    let h = private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await;
    let l = honest_slot(&mut n.light, Vec::new()).await;
    feed(&mut n.light, &h).await;
    feed(&mut n.heavy, &l).await;
    (h, l)
}

/// `count` claims made by cards 2, 3, … on `node`'s chain and bound to panels before the fork (card 7's
/// attempt at the anchor slot), returned in order. The claims are the shared history's; their licences
/// are what a side may or may not carry.
async fn bind_claims(node: &mut T12Chain, count: usize) -> Vec<Hash64> {
    let mut claims = Vec::new();
    for i in 0..count {
        let (_, claim_id) = node.attempt(2 + i, 1_000, Vec::new(), &|_| true).await;
        claims.push(claim_id);
    }
    for claim_id in &claims {
        let (_, state) = node.tip_state();
        if matches!(state.claim(claim_id).expect("made").phase, PalwClaimPhaseV2::Provisional) {
            node.attempt_at_the_anchor_slot(*claim_id, 7).await;
        }
    }
    let (_, state) = node.tip_state();
    for claim_id in &claims {
        assert!(matches!(state.claim(claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }), "{claim_id} is bound");
    }
    claims
}

/// The quorum's Valid receipts for `claim_id` (real ML-DSA-87 under the harness keys of the panel's
/// seats), assembled by `node` into a lifecycle carrier that spends card `float`'s fee float — the
/// carrier `hb_probe_e_…` and LIVE-R1's reproduction build.
fn licence_carrier(node: &T12Chain, floats: &Utxos, float: usize, claim_id: Hash64) -> Transaction {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
    let params = &node.config.params;
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    let (_, state) = node.tip_state();
    let panel = state.panel(&claim_id).expect("bound").clone();
    let signed_daa = node.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .take(node.bundle.panel.quorum() as usize)
        .map(|seat| {
            let card = node.bonds.iter().position(|b| *b == seat.bond).expect("a genesis card");
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
                message.as_byte_slice(),
                PALW_RECEIPT_V2_MLDSA87_CONTEXT,
                [0x11u8; 32],
            )
            .expect("sign")
            .as_ref()
            .to_vec();
            PalwSeatReceiptV2 { claim: claim_id, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    let object = node.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
    assert!(matches!(object, Obj::ReceiptLicensed { .. }));
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    let (outpoint, entry) = floats[float].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(entry.amount - 300_000, card_payout_spk(float))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, entry, float, params.storage_mass_parameter);
    tx
}

/// Three shared slots, mirrored: the fork point.
async fn shared_prefix(n: &mut Net) -> BlockHash {
    for _ in 0..3 {
        let b = honest_slot(&mut n.heavy, Vec::new()).await;
        feed(&mut n.light, &b).await;
    }
    assert_eq!(n.heavy.sink(), n.light.sink());
    n.heavy.sink()
}

/// The blue score each side gains a slot (read off the nodes' own ghostdag stores).
fn blue_per_slot(c: &T12Chain, from: BlockHash, slots: u64) -> f64 {
    (bs(c, c.sink()) - bs(c, from)) as f64 / slots as f64
}

// =====================================================================================================
// The facts
// =====================================================================================================

/// **The numbers the record and the model use**, read off the harness and off testnet-12 as shipped:
/// the depths, the windows, the fork-choice fences' heights, and the blue score a slot each side's
/// production shape makes (which is what converts `finality_depth` into the time to the seal).
#[tokio::test]
async fn finx_p0_facts_depths_windows_fences_and_blue_per_slot() {
    kaspa_core::log::try_init_logger("warn");
    let mut n = net(None);
    let p = n.config.params.clone();
    let s = n.bundle.state.clone();
    eprintln!(
        "[finx facts] finality depth {} blue, merge depth {}, pruning depth {}, k {}, target {} ms",
        p.finality_depth(),
        p.merge_depth(),
        p.pruning_depth(),
        p.ghostdag_k(),
        p.target_time_per_block()
    );
    eprintln!(
        "[finx facts] windows: bind {}, receipt {}, challenge {} (applied to a licence at DAA 100: {}, at DAA 5,000: {}), court {}; anchor delay {}, quorum {}",
        s.window_bind(),
        s.window_receipt(),
        s.window_challenge(),
        s.window_challenge_at(100),
        s.window_challenge_at(5_000),
        s.window_court(),
        n.bundle.panel.anchor_delay(),
        n.bundle.panel.quorum()
    );
    let shipped = palw_t12_shipped_params();
    eprintln!(
        "[finx facts] testnet-12 as shipped: finality {} pruning {}; strict-win {:?}, pruning-proof strict-economic {:?}, F-W {:?}, frontier provenance {:?}, short challenge window {:?}",
        shipped.finality_depth(),
        shipped.pruning_depth(),
        shipped.palw_reorg_strict_economic_win,
        shipped.palw_pruning_proof_strict_economic_win,
        shipped.palw_capacity_weight_cap,
        shipped.palw_frontier_provenance,
        shipped.palw_short_challenge_window,
    );
    eprintln!(
        "[finx facts] testnet-12 as shipped: DNS overlay configured {}, DNS BFT finality gate {:?} — the V2 arm decides every non-extension",
        shipped.dns_params.is_some(),
        shipped.dns_bft_gate.map(|g| g.activation)
    );
    let fork = shared_prefix(&mut n).await;
    for _ in 0..10 {
        private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await;
        honest_slot(&mut n.light, Vec::new()).await;
    }
    let (hb, lb) = (blue_per_slot(&n.heavy, fork, 10), blue_per_slot(&n.light, fork, 10));
    eprintln!(
        "[finx facts] blue score a slot: two racing producers {hb:.2}, one producer {lb:.2}; at finality depth {} a side seals {:.0} / {:.0} slots ({:.1} h / {:.1} h at 122 s a slot) after a fork",
        p.finality_depth(),
        p.finality_depth() as f64 / hb,
        p.finality_depth() as f64 / lb,
        p.finality_depth() as f64 / hb * 122.0 / 3600.0,
        p.finality_depth() as f64 / lb * 122.0 / 3600.0
    );
    assert!(hb > lb, "two racing producers make more blue score a slot than one");
    assert_eq!(p.finality_depth(), s.window_challenge() / 2, "testnet-12's finality depth is window_challenge / 2");
}

// =====================================================================================================
// (a) A partition with nothing economic on either side
// =====================================================================================================

/// Partition two nodes for `k` slots — `heavy_m` producers racing on the heavy side (1: the same shape
/// as the light side, a symmetric split), one on the light side, heartbeats only — heal, and play three
/// more exchanged rounds. Returns whether the sinks agreed after the heal and after each round.
async fn tie_partition(tag: &str, k: usize, heavy_m: usize) -> Vec<bool> {
    tie_partition_ruled(tag, k, heavy_m, false).await
}

/// [`tie_partition`] on a ruleset with rule E armed or not.
async fn tie_partition_ruled(tag: &str, k: usize, heavy_m: usize, rule_e: bool) -> Vec<bool> {
    let mut n = net_ruled(None, rule_e);
    let fork = shared_prefix(&mut n).await;
    let (mut hblocks, mut lblocks) = (Vec::new(), Vec::new());
    for _ in 0..k {
        hblocks.extend(if heavy_m == 1 {
            honest_slot(&mut n.heavy, Vec::new()).await
        } else {
            private_slot(&mut n.heavy, &mut n.nonce, heavy_m, Vec::new()).await
        });
        lblocks.extend(honest_slot(&mut n.light, Vec::new()).await);
    }
    let (ht, lt) = (n.heavy.sink(), n.light.sink());
    let (kh, kl) = (keys(&n.heavy, ht), keys(&n.light, lt));
    assert_eq!(econ(&kh), econ(&kl), "{tag}: nothing economic on either side — the keys tie");
    feed(&mut n.light, &hblocks).await;
    feed(&mut n.heavy, &lblocks).await;
    // Name the sides by what GHOSTDAG's heap sees (blue work, then hash).
    let heavier_is_heavy = (bw(&n.heavy, ht), ht) > (bw(&n.heavy, lt), lt);
    let mut agreed = vec![n.heavy.sink() == n.light.sink()];
    eprintln!(
        "[finx {tag}] healed after {k} slots: heavy tip {ht} (+{} blue work), light tip {lt} (+{}), keys {:?} both; GHOSTDAG-heavier: {}; sinks {} / {} — {}",
        bw(&n.heavy, ht) - bw(&n.heavy, fork),
        bw(&n.heavy, lt) - bw(&n.heavy, fork),
        econ(&kh),
        if heavier_is_heavy { "the heavy side's tip" } else { "the light side's tip" },
        n.heavy.sink(),
        n.light.sink(),
        if agreed[0] { "AGREE" } else { "SPLIT" }
    );
    for round in 0..3 {
        exchange_round(&mut n).await;
        agreed.push(n.heavy.sink() == n.light.sink());
        eprintln!("[finx {tag}] round {round}: {}", if *agreed.last().unwrap() { "AGREE" } else { "SPLIT" });
    }
    agreed
}

/// **V2, through the pipeline: a deep all-economic tie keeps the incumbent on BOTH sides, so an honest
/// partition three slots long with nothing economic anywhere never heals.**
///
/// Two slots is a slot race GHOSTDAG decides (`PALW_REORG_SHALLOW_TIE_DAA_V1` = 2): the light side's
/// node takes the heavier tip and the nodes agree. Three is not: the light side's node refuses the
/// heavier tip as a deep tie (`palw_reorg_strict_economic_win_v1`: keep the incumbent), and the heavy
/// side's node never weighs the light tip at all (it pops its own, an extension). The DAG is one DAG
/// after the heal; three more exchanged rounds change nothing. The same with one producer a side (a
/// symmetric split) and with two racing on one side (the "majority with more nodes"): more nodes buy
/// blue work, and blue work does not move this rule.
#[tokio::test]
async fn finx_p0_a_a_partition_with_nothing_economic_never_heals_past_two_slots() {
    kaspa_core::log::try_init_logger("warn");
    for heavy_m in [1usize, 2] {
        let shape = if heavy_m == 1 { "symmetric" } else { "two producers against one" };
        let two = tie_partition(&format!("a {shape}, 2 slots"), 2, heavy_m).await;
        assert!(two[0], "a two-slot tie is GHOSTDAG's: the nodes agree at the heal ({shape}): {two:?}");
        for k in [3usize, 6] {
            let deep = tie_partition(&format!("a {shape}, {k} slots"), k, heavy_m).await;
            assert!(deep.iter().all(|a| !a), "{shape}, {k} slots: split at the heal and after every round: {deep:?}");
        }
    }
}

// =====================================================================================================
// (b) A Final on one side
// =====================================================================================================

/// **A `Final` on one side of a partition: it heals the split only when it lands on the GHOSTDAG-heavier
/// side.** A claim bound before the fork is licensed on one side only and matures to `Final` there
/// (one applied challenge window later); the partition heals once it has.
///
/// * On the heavier side: the light side's node pops the heavier tip, finds it strictly ahead on the
///   first key (the safe frontier), and takes it — the nodes agree.
/// * On the lighter side: the lighter chain is strictly ahead on the first key, so every comparison the
///   PALW authority could make picks it — and the heavy side's node, holding it and able to weigh it,
///   never asks: its sink search pops its own heavier tip, an extension, and stops (V1). The light
///   side's node refuses the heavy tip as a strict economic loss. Split, with the comparator's maximum
///   on the side that two nodes in nine — devnet r1's B and D6 — would be on.
#[tokio::test]
async fn finx_p0_b_a_final_heals_only_on_the_heavier_side() {
    kaspa_core::log::try_init_logger("warn");
    for final_on_heavy in [true, false] {
        let tag = format!("b Final on the {} side", if final_on_heavy { "heavier" } else { "lighter" });
        let mut n = net(None);
        shared_prefix(&mut n).await;
        let claims = bind_claims(&mut n.heavy, 1).await;
        let c = claims[0];
        let shared = blocks_in_topological_order(&n.heavy);
        feed(&mut n.light, &shared).await;
        let fork = n.heavy.sink();
        assert_eq!(n.light.sink(), fork, "{tag}: the claim's binding is shared history");
        let (mut hblocks, mut lblocks) = (Vec::new(), Vec::new());
        let carrier =
            if final_on_heavy { licence_carrier(&n.heavy, &n.floats, 0, c) } else { licence_carrier(&n.light, &n.floats, 0, c) };
        let (h_txs, l_txs) = if final_on_heavy { (vec![carrier], Vec::new()) } else { (Vec::new(), vec![carrier]) };
        hblocks.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, h_txs).await);
        lblocks.extend(honest_slot(&mut n.light, l_txs).await);
        let holder = if final_on_heavy { &n.heavy } else { &n.light };
        let licensed_daa = match holder.tip_state().1.claim(&c).unwrap().phase.clone() {
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
            other => panic!("{tag}: licensed on its side, is {other:?}"),
        };
        let final_daa = licensed_daa + n.bundle.state.window_challenge_at(licensed_daa) + 1;
        eprintln!("[finx {tag}] licensed at DAA {licensed_daa}, Final due at DAA {final_daa}");
        while n.heavy.daa_of(n.heavy.sink()) < final_daa || n.light.daa_of(n.light.sink()) < final_daa {
            hblocks.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
            lblocks.extend(honest_slot(&mut n.light, Vec::new()).await);
        }
        let (ht, lt) = (n.heavy.sink(), n.light.sink());
        let (kh, kl) = (keys(&n.heavy, ht), keys(&n.light, lt));
        let holder = if final_on_heavy { &n.heavy } else { &n.light };
        assert!(matches!(holder.tip_state().1.claim(&c).unwrap().phase, PalwClaimPhaseV2::Final { .. }), "{tag}: Final on its side");
        let slots = n.heavy.daa_of(ht) - n.heavy.daa_of(fork);
        feed(&mut n.light, &hblocks).await;
        feed(&mut n.heavy, &lblocks).await;
        assert!(bw(&n.heavy, ht) > bw(&n.heavy, lt), "{tag}: the heavy side is GHOSTDAG-heavier");
        let max_is_light = compare_palw_candidates_v1(&kl, &kh) == std::cmp::Ordering::Greater;
        eprintln!(
            "[finx {tag}] healed {slots} slots after the fork: heavy keys {:?}, light keys {:?} — the comparator's maximum is the {} tip; sinks: heavy node {} light node {}; sealed? heavy {} light {}",
            econ(&kh),
            econ(&kl),
            if max_is_light { "LIGHT" } else { "heavy" },
            if n.heavy.sink() == ht { "own" } else { "light's" },
            if n.light.sink() == lt { "own" } else { "heavy's" },
            sealed_against(&n.heavy, lt),
            sealed_against(&n.light, ht),
        );
        assert!(!sealed_against(&n.light, ht) && !sealed_against(&n.heavy, lt), "{tag}: inside the finality depth on both nodes");
        if final_on_heavy {
            assert!(econ(&kh) > econ(&kl), "{tag}: the heavy side is strictly ahead");
            assert_eq!(n.light.sink(), ht, "{tag}: the light node takes the heavier, economically better tip");
            assert_eq!(n.heavy.sink(), ht);
        } else {
            assert!(econ(&kl) > econ(&kh) && max_is_light, "{tag}: the light side is strictly ahead — the comparator's maximum");
            eprintln!(
                "[finx {tag}] the heavy node's own fold of the light tip: {:?} (None = never UTXO-validated: its search stopped at its own extension)",
                n.heavy.vp().palw_candidate_order_v2(lt).map(|o| econ(&o))
            );
            assert_eq!(n.heavy.sink(), ht, "{tag}: V1 — the heavy node keeps its own tip, never asking the comparator");
            assert_eq!(n.light.sink(), lt, "{tag}: the light node refuses the heavy tip (a strict economic loss)");
            for round in 0..2 {
                exchange_round(&mut n).await;
                assert_ne!(n.heavy.sink(), n.light.sink(), "{tag}: round {round}: still split");
            }
        }
    }
}

// =====================================================================================================
// (c) devnet r1: the minority holds the licences — and restart, IBD, and fresh nodes during the split
// =====================================================================================================

/// **Devnet r1's shape, and what every way back does with it.** A claim bound before the fork; its
/// licence carrier reached only the light side (devnet r1's minority, B and D6); the heavy side (the
/// majority) mints two attempts and races two producers every slot. Eight slots, then the heal.
///
/// * The split (LIVE-R1's finding, reproduced): the light node refuses the heavier tip as a strict
///   economic loss; the heavy node weighs the light tip as strictly better and keeps its own (V1).
/// * **Restart**: both nodes stopped and reopened on their databases come back on their own sinks —
///   the wedge is persisted, a restart does not heal it.
/// * **IBD** (the flow's `validate_staging_palw_order` rule, `palw_ibd_commit_strict_economic_v1`, armed
///   on testnet-12 at DAA 750): an old light datadir staging the heavy chain KEEPS ITS OWN; an old heavy
///   datadir staging the light chain COMMITS it — the relay path and the IBD path decide the same pair
///   oppositely on the heavy side (V4).
/// * **Fresh nodes during the split**: two nodes from genesis are handed the same DAG in opposite
///   orders; each ends on the side it heard first (V5).
#[tokio::test]
async fn finx_p0_c_the_minority_holds_the_licence_and_no_way_back_heals_it() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let tag = "c devnet r1";
    let (config, bundle, premine, floats) = parts(None);
    let (_heavy_db_lifetime, heavy_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (_light_db_lifetime, light_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (hs, _hrx) = async_channel::unbounded();
    let (ls, _lrx) = async_channel::unbounded();
    let heavy = t12_genesis_chain_on(TestConsensus::with_db(heavy_db.clone(), &config, hs), &config, &bundle, &premine, &floats);
    let light = t12_genesis_chain_on(TestConsensus::with_db(light_db.clone(), &config, ls), &config, &bundle, &premine, &floats);
    let (x, y) = (pay(&config, &floats, card_payout_spk(5)), pay(&config, &floats, card_payout_spk(6)));
    let mut n = Net { config: config.clone(), bundle: bundle.clone(), premine, floats, heavy, light, nonce: 1 << 40, x, y };
    shared_prefix(&mut n).await;
    let c = bind_claims(&mut n.heavy, 1).await[0];
    let shared = blocks_in_topological_order(&n.heavy);
    feed(&mut n.light, &shared).await;
    let fork = n.heavy.sink();
    let carrier = licence_carrier(&n.light, &n.floats, 0, c);
    let mut lblocks = honest_slot(&mut n.light, vec![carrier]).await;
    let mut hblocks = Vec::new();
    for card in [3usize, 4] {
        hblocks.push(n.heavy.attempt(card, 1_000, Vec::new(), &|_| true).await.0);
    }
    for _ in 0..8 {
        hblocks.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
    }
    for _ in 1..8 {
        lblocks.extend(honest_slot(&mut n.light, Vec::new()).await);
    }
    let (ht, lt) = (n.heavy.sink(), n.light.sink());
    feed(&mut n.light, &hblocks).await;
    feed(&mut n.heavy, &lblocks).await;
    let (kh, kl) = (keys(&n.heavy, ht), keys(&n.light, lt));
    eprintln!(
        "[finx {tag}] healed: heavy tip +{} blue work keys {:?}; light tip +{} keys {:?}; sinks heavy {} light {}",
        bw(&n.heavy, ht) - bw(&n.heavy, fork),
        econ(&kh),
        bw(&n.heavy, lt) - bw(&n.heavy, fork),
        econ(&kl),
        if n.heavy.sink() == ht { "own" } else { "light's" },
        if n.light.sink() == lt { "own" } else { "heavy's" }
    );
    assert!(bw(&n.heavy, ht) > bw(&n.heavy, lt), "{tag}: the heavy side tops every heap");
    assert!(econ(&kl) > econ(&kh), "{tag}: the light side's licence is weight the heavy side lacks");
    assert_eq!(
        palw_reorg_strict_economic_win_v1(&kl, &kh, || true),
        PalwDeepReorgV2::Refuse,
        "{tag}: the light node's gate refuses the heavy tip — a strict economic loss"
    );
    assert_eq!(
        palw_reorg_strict_economic_win_v1(&kh, &kl, || false),
        PalwDeepReorgV2::Allow,
        "{tag}: the heavy node's gate WOULD take the light tip, were it asked"
    );
    assert_eq!((n.heavy.sink(), n.light.sink()), (ht, lt), "{tag}: split — each keeps its own");

    // ---- restart ------------------------------------------------------------------------------------
    let (h_time, h_nonce) = (n.heavy.ctx.simulated_time, n.heavy.nonce_for_reopen());
    let (l_time, l_nonce) = (n.light.ctx.simulated_time, n.light.nonce_for_reopen());
    let (fresh_a, fresh_b) = (n.fresh_node(), n.fresh_node());
    let Net { heavy, light, .. } = n;
    drop(heavy);
    drop(light);
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (hs, _hrx2) = async_channel::unbounded();
    let (ls, _lrx2) = async_channel::unbounded();
    let heavy = t12_reopened_chain(TestConsensus::with_db(heavy_db.clone(), &resumed, hs), &resumed, &bundle, h_time, h_nonce);
    let light = t12_reopened_chain(TestConsensus::with_db(light_db.clone(), &resumed, ls), &resumed, &bundle, l_time, l_nonce);
    eprintln!(
        "[finx {tag}] restarted: heavy node on {} ({}), light node on {} ({})",
        heavy.sink(),
        if heavy.sink() == ht { "its own" } else { "MOVED" },
        light.sink(),
        if light.sink() == lt { "its own" } else { "MOVED" }
    );
    assert_eq!((heavy.sink(), light.sink()), (ht, lt), "{tag}: a restart does not heal the split — the wedge is on disk");

    // ---- IBD (the flow's commit rule, as both nodes' own orders feed it) -----------------------------
    let (oh, ol) =
        (heavy.ctx.consensus.get_palw_candidate_order_v2().unwrap(), light.ctx.consensus.get_palw_candidate_order_v2().unwrap());
    let light_from_heavy = palw_ibd_commit_strict_economic_v1(&ol, &oh);
    let heavy_from_light = palw_ibd_commit_strict_economic_v1(&oh, &ol);
    eprintln!(
        "[finx {tag}] IBD: an old light datadir staging the heavy chain: {light_from_heavy:?}; an old heavy datadir staging the light chain: {heavy_from_light:?}"
    );
    assert_eq!(light_from_heavy, PalwIbdCommitV2::KeepIncumbent, "{tag}: the light node's IBD from a heavy peer is refused");
    assert_eq!(
        heavy_from_light,
        PalwIbdCommitV2::Commit,
        "{tag}: V4 — the heavy node's IBD from a light peer commits what its relay never weighs"
    );
    assert!(palw_t12_shipped_params().palw_pruning_proof_strict_economic_win.is_some(), "testnet-12 ships the IBD rule armed");

    // ---- fresh nodes, opposite arrival orders --------------------------------------------------------
    let (mut first_heavy, mut first_light) = (fresh_a, fresh_b);
    for (node, first, second) in [(&mut first_heavy, &hblocks, &lblocks), (&mut first_light, &lblocks, &hblocks)] {
        feed(node, &shared).await;
        feed(node, first).await;
        feed(node, second).await;
    }
    eprintln!(
        "[finx {tag}] fresh nodes over one DAG: heard the heavy side first -> {}; heard the light side first -> {}",
        if first_heavy.sink() == ht { "heavy tip" } else { "light tip" },
        if first_light.sink() == lt { "light tip" } else { "heavy tip" }
    );
    assert_eq!(first_heavy.sink(), ht, "{tag}: V5 — the node that heard the heavy side first stays there");
    assert_eq!(first_light.sink(), lt, "{tag}: V5 — the node that heard the light side first stays there");
}

// =====================================================================================================
// (d) The seal: finality closes the door the PALW gate would open
// =====================================================================================================

/// **V3: the finality point passes the fork while the PALW gate is refusing, and then the gate is never
/// asked again.** Devnet r1's shape (the light side holds one licence) with `finality_depth` lowered to
/// 60 blue score; three claims bound before the fork. The heavy side's way to win under the PALW rule is
/// to carry MORE licences — the other two claims' — which makes it strictly ahead on `live`:
///
/// * carried right after the heal, the light node takes the heavy tip: the gate allows a strict
///   economic win, the nodes agree;
/// * carried once the light node's finality point has passed the fork (its own heartbeats, two blue a
///   slot, nothing else), the same strict win is never weighed: the candidate fails
///   `candidate_at_or_above_finality` before the gate. The PALW rule would replace the light chain;
///   finality has sealed it.
///
/// The seal is measured in slots after the fork; the record converts it at testnet-12's depth (600).
#[tokio::test]
async fn finx_p0_d_finality_seals_a_chain_the_palw_rule_would_replace() {
    kaspa_core::log::try_init_logger("warn");
    const DEPTH: u64 = 60;
    for late in [false, true] {
        let tag = format!("d heavy side's licences carried {}", if late { "AFTER the seal" } else { "right after the heal" });
        let mut n = net(Some(DEPTH));
        shared_prefix(&mut n).await;
        let claims = bind_claims(&mut n.heavy, 3).await;
        let shared = blocks_in_topological_order(&n.heavy);
        feed(&mut n.light, &shared).await;
        let fork = n.heavy.sink();
        let carrier = licence_carrier(&n.light, &n.floats, 0, claims[0]);
        let mut lblocks = honest_slot(&mut n.light, vec![carrier]).await;
        let mut hblocks = Vec::new();
        for _ in 0..6 {
            hblocks.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
        }
        for _ in 1..6 {
            lblocks.extend(honest_slot(&mut n.light, Vec::new()).await);
        }
        feed(&mut n.light, &hblocks).await;
        feed(&mut n.heavy, &lblocks).await;
        assert_ne!(n.heavy.sink(), n.light.sink(), "{tag}: split at the heal (the light side's licence)");
        let mut rounds = 0u64;
        if late {
            while !sealed_against(&n.light, n.heavy.sink()) {
                exchange_round(&mut n).await;
                rounds += 1;
                assert!(rounds < 200, "{tag}: the light node seals within 200 rounds");
            }
            let slots = n.light.daa_of(n.light.sink()) - n.light.daa_of(fork);
            eprintln!(
                "[finx {tag}] the light node's finality point passed the fork {slots} slots after it ({} blue score above it, depth {DEPTH}) — the heavy branch is now below its finality",
                bs(&n.light, n.light.sink()) - bs(&n.light, fork)
            );
        }
        // The heavy side carries the other two claims' licences. The gate's question is asked of the light
        // node's sink as it stands when the heavy tip arrives — measured here, before the feed.
        let txs = vec![licence_carrier(&n.heavy, &n.floats, 4, claims[1]), licence_carrier(&n.heavy, &n.floats, 5, claims[2])];
        let h = private_slot(&mut n.heavy, &mut n.nonce, 2, txs).await;
        let (ht, lt0) = (n.heavy.sink(), n.light.sink());
        // Each tip as its own node folded it — one fold, one answer (a sealed node never folds the other).
        let (kh, kl) = (keys(&n.heavy, ht), keys(&n.light, lt0));
        let would = palw_reorg_strict_economic_win_v1(&kl, &kh, || false);
        let sealed = sealed_past(&n.light, fork);
        feed(&mut n.light, &h).await;
        let l = honest_slot(&mut n.light, Vec::new()).await;
        feed(&mut n.heavy, &l).await;
        let lt = n.light.sink();
        eprintln!(
            "[finx {tag}] after {rounds} rounds: heavy tip keys {:?} vs the light node's sink {:?} — the PALW gate would {:?}; the light node's finality point past the fork: {sealed}; then: light node on {}",
            econ(&kh),
            econ(&kl),
            would,
            if n.light.ctx.consensus.is_chain_ancestor_of(ht, lt).unwrap_or(false) { "the heavy chain" } else { "its own chain" }
        );
        assert_eq!(
            would,
            PalwDeepReorgV2::Allow,
            "{tag}: the heavy side is now strictly ahead — the PALW rule would replace the light chain"
        );
        let on_heavy = n.light.ctx.consensus.is_chain_ancestor_of(ht, lt).unwrap_or(false);
        if late {
            assert!(sealed, "{tag}: …and finality refuses it first");
            assert!(!on_heavy, "{tag}: V3 — the split is sealed");
            for round in 0..2 {
                exchange_round(&mut n).await;
                assert_ne!(n.heavy.sink(), n.light.sink(), "{tag}: round {round}: sealed");
            }
        } else {
            assert!(!sealed && on_heavy, "{tag}: inside the finality depth the strict win heals the split");
        }
    }
}

// =====================================================================================================
// (e) An attacker's private branch released before, at and after the finality depth
// =====================================================================================================

/// **V6, the stale incumbent — and what bounds it: only the finality depth.** A claim bound before the
/// fork has a quorum of Valid receipts that are public (the seats broadcast them; any node can carry
/// them — the harness signs them as the seats would). The attacker's private branch carries Y and that
/// licence; the victim's chain carries X and has not yet carried the licence when the branch is
/// released. The deep-reorg gate compares the candidate with the victim's PREVIOUS sink, so the branch
/// that carried a public licence first is "strictly ahead" on `live` — no collusion, no bond, only being
/// first (the model, `finx_p1_private_branch_safety`, finds the same opening in every pre-fork claim's
/// licence and `Final` transition, which a branch whose clock runs a tick ahead crosses first). Released
/// with the victim's chain `d` blue score above the fork, `finality_depth` lowered to 60:
///
/// * half-way to the seal, and at the last slot before the victim's finality point passes the fork: the
///   gate takes the private branch — Y lands, X is reversed; the honest side's own next block carrying
///   the same licence (built before the release, delivered after it) ties the keys and changes nothing:
///   a deep tie keeps the new incumbent;
/// * at the first slot past the seal: refused before the gate ("Finality Violation") — X stands.
///
/// So a time-dependent key reverses payments exactly as deep as the finality depth, which a node reaches
/// by its own heartbeats (testnet-12: 600 blue score, the hours `finx_p0_facts` converts).
#[tokio::test]
async fn finx_p0_e_a_private_economic_win_reverses_x_up_to_the_finality_depth_and_no_further() {
    kaspa_core::log::try_init_logger("warn");
    const DEPTH: u64 = 60;
    // The first run measures where the victim seals (its own heartbeats, two blue a slot); the second
    // releases one slot earlier, the third half-way.
    let mut seal_depth: Option<u64> = None;
    for (label, mode) in
        [("at the first slot past the seal", 0u8), ("at the last slot before the seal", 1), ("half-way to the seal", 2)]
    {
        let tag = format!("e released {label}");
        let mut n = net(Some(DEPTH));
        shared_prefix(&mut n).await;
        let c = bind_claims(&mut n.heavy, 1).await[0];
        let shared = blocks_in_topological_order(&n.heavy);
        feed(&mut n.light, &shared).await;
        let fork = n.heavy.sink();
        // The victim (`light`) mines X and honest slots; the attacker (`heavy`) mines Y and the licence.
        let (x, y) = (n.x.clone(), n.y.clone());
        honest_slot(&mut n.light, vec![x.clone()]).await;
        let carrier = licence_carrier(&n.heavy, &n.floats, 0, c);
        let mut private = private_slot(&mut n.heavy, &mut n.nonce, 1, vec![y.clone()]).await;
        private.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, vec![carrier]).await);
        loop {
            let depth = bs(&n.light, n.light.sink()) - bs(&n.light, fork);
            let stop = match mode {
                0 => sealed_past(&n.light, fork),
                1 => depth + 2 >= seal_depth.expect("measured by the first run"),
                _ => depth >= seal_depth.expect("measured by the first run") / 2,
            };
            if stop {
                break;
            }
            honest_slot(&mut n.light, Vec::new()).await;
            private.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
        }
        let past_seal = mode == 0;
        if past_seal {
            seal_depth = Some(bs(&n.light, n.light.sink()) - bs(&n.light, fork));
        }
        assert_eq!(sealed_past(&n.light, fork), past_seal, "{tag}: the victim's finality point is where the run means it to be");
        let depth = bs(&n.light, n.light.sink()) - bs(&n.light, fork);
        let victim_tip = n.light.sink();
        // The honest side's next block, carrying the same claim's licence from the same public receipts:
        // built on the victim's public tip now, delivered after the release.
        let catch_up = {
            let carrier = licence_carrier(&n.light, &n.floats, 0, c);
            let mut t = n
                .light
                .ctx
                .consensus
                .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(vec![carrier])), TemplateBuildMode::Standard)
                .expect("a template");
            stamp_harness_time(&n.light.config.params, &mut t.block.header, n.light.ctx.simulated_time + 1_000);
            t.block.header.finalize();
            let (t, _) = n.light.vp().heartbeat_adapt_block_template(t).expect("the heartbeat lane is open on testnet-12");
            let mut b = t.block.clone();
            n.nonce += 1;
            b.header.nonce = n.nonce;
            b.header.finalize();
            b.to_immutable()
        };
        feed(&mut n.light, &private).await;
        let pt = private.last().unwrap().header.hash;
        let x_out = TransactionOutpoint::new(x.id(), 0);
        let y_out = TransactionOutpoint::new(y.id(), 0);
        let (has_x, has_y) = (has_utxo(&n.light, x_out), has_utxo(&n.light, y_out));
        eprintln!(
            "[finx {tag}] released with X {depth} blue score deep (finality depth {DEPTH}); sealed past the fork before the release: {past_seal}; keys private {:?} vs public {:?}; victim sink {} — X {} Y {}",
            n.heavy.vp().palw_candidate_order_v2(pt).map(|o| econ(&o)),
            econ(&keys(&n.light, victim_tip)),
            if n.light.sink() == victim_tip { "public" } else { "PRIVATE" },
            if has_x { "present" } else { "gone" },
            if has_y { "PRESENT" } else { "absent" }
        );
        if past_seal {
            assert!(has_x && !has_y, "{tag}: past the finality depth X stands");
        } else {
            assert!(!has_x && has_y, "{tag}: inside the finality depth the strict economic win reverses X");
            let hash = catch_up.header.hash;
            n.light
                .ctx
                .consensus
                .validate_and_insert_block(catch_up)
                .virtual_state_task
                .await
                .expect("the honest catch-up block is valid");
            let (has_x, has_y) = (has_utxo(&n.light, x_out), has_utxo(&n.light, y_out));
            eprintln!(
                "[finx {tag}] the honest side's own licence carrier arrives one block later: keys {:?} vs the new incumbent {:?}; victim sink {} — X {} Y {}",
                n.light.vp().palw_candidate_order_v2(hash).map(|o| econ(&o)),
                econ(&keys(&n.light, n.light.sink())),
                if n.light.sink() == hash { "the catch-up block" } else { "private" },
                if has_x { "present" } else { "gone" },
                if has_y { "PRESENT" } else { "absent" }
            );
            assert!(!has_x && has_y, "{tag}: the honest chain catching up does not undo the reversal");
        }
    }
}

// =====================================================================================================
// (f) V6's second form: a branch one tick ahead crosses a pre-fork claim's Final first
// =====================================================================================================

/// **V6 without any carrier: the clock.** A claim bound AND licensed before the fork turns `Final` at a
/// fixed DAA (`licensed + window_challenge_at(licensed) + 1`) on every branch that reaches that DAA. The
/// attacker's private heartbeat branch (no bond, no attempt, no licence of its own) forks a few slots
/// before that height, carries Y, and runs one slot ahead of the victim — as `hb_probe_b_future_*`
/// measures a branch may, within the timestamp tolerance. Released when the private tip stands AT the
/// `Final` height and the victim's sink one tick under it, the private branch is strictly ahead on the
/// first key (the safe frontier) and the second (safe weight) — keys the victim's own next slot would tie
/// — and the gate, comparing with the previous sink, takes it: X, several DAA deep, is reversed.
#[tokio::test]
async fn finx_p0_f_a_branch_one_tick_ahead_crosses_a_pre_fork_final_first_and_reverses_x() {
    kaspa_core::log::try_init_logger("warn");
    let tag = "f V6 by the clock";
    let mut n = net(None);
    shared_prefix(&mut n).await;
    let c = bind_claims(&mut n.heavy, 1).await[0];
    let carrier = licence_carrier(&n.heavy, &n.floats, 0, c);
    honest_slot(&mut n.heavy, vec![carrier]).await;
    let licensed_daa = match n.heavy.tip_state().1.claim(&c).unwrap().phase.clone() {
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
        other => panic!("{tag}: licensed in the shared history, is {other:?}"),
    };
    let final_daa = licensed_daa + n.bundle.state.window_challenge_at(licensed_daa) + 1;
    const LEAD_IN: u64 = 10;
    while n.heavy.daa_of(n.heavy.sink()) < final_daa - LEAD_IN {
        honest_slot(&mut n.heavy, Vec::new()).await;
    }
    let shared = blocks_in_topological_order(&n.heavy);
    feed(&mut n.light, &shared).await;
    let fork = n.heavy.sink();
    assert_eq!(n.light.sink(), fork, "{tag}: the licence is shared history");
    // The victim (`light`) mines X and honest slots up to one tick under the Final height; the attacker
    // (`heavy`) mines Y and two-sibling slots up to the Final height itself — one slot ahead.
    let (x, y) = (n.x.clone(), n.y.clone());
    honest_slot(&mut n.light, vec![x.clone()]).await;
    let x_daa = n.light.daa_of(n.light.sink());
    while n.light.daa_of(n.light.sink()) < final_daa - 1 {
        honest_slot(&mut n.light, Vec::new()).await;
    }
    let mut private = private_slot(&mut n.heavy, &mut n.nonce, 1, vec![y.clone()]).await;
    while n.heavy.daa_of(n.heavy.sink()) < final_daa {
        private.extend(private_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
    }
    let (pt, vt) = (n.heavy.sink(), n.light.sink());
    let private_phase = n.heavy.tip_state().1.claim(&c).unwrap().phase.clone();
    let public_phase = n.light.tip_state().1.claim(&c).unwrap().phase.clone();
    let (kp, kv) = (keys(&n.heavy, pt), keys(&n.light, vt));
    eprintln!(
        "[finx {tag}] Final height {final_daa}: private tip DAA {} ({private_phase:?}) keys {:?}; victim sink DAA {} ({public_phase:?}) keys {:?}; X at DAA {x_daa}",
        n.heavy.daa_of(pt),
        econ(&kp),
        n.light.daa_of(vt),
        econ(&kv)
    );
    assert!(matches!(private_phase, PalwClaimPhaseV2::Final { .. }), "{tag}: Final on the private tip");
    assert!(matches!(public_phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{tag}: one tick short of Final on the victim");
    assert!(econ(&kp) > econ(&kv), "{tag}: the private tip is strictly ahead on keys the victim's next slot would tie");
    assert!(
        n.light.daa_of(vt) - x_daa > kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1,
        "{tag}: X is past the shallow window"
    );
    feed(&mut n.light, &private).await;
    let (x_out, y_out) = (TransactionOutpoint::new(x.id(), 0), TransactionOutpoint::new(y.id(), 0));
    let (has_x, has_y) = (has_utxo(&n.light, x_out), has_utxo(&n.light, y_out));
    eprintln!(
        "[finx {tag}] released with X {} DAA deep: victim sink {} — X {} Y {}",
        n.light.daa_of(vt) - x_daa,
        if n.light.sink() == vt { "public" } else { "PRIVATE" },
        if has_x { "present" } else { "gone" },
        if has_y { "PRESENT" } else { "absent" }
    );
    assert!(!has_x && has_y, "{tag}: the branch one tick ahead reverses X — no bond, no carrier, no collusion");
}

/// ADR-0178's rule E against every violation above, armed — and the attacks it is built to hold.
mod rule_e;
