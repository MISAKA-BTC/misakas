//! **ADR-0175: rule E armed through the real pipeline — every violation above, and the attacks it is built to hold** (FINX,
//! 2026-10-09). INTERNAL, as the module above.
//!
//! Rule E is armed from DAA 1 on testnet-12's fork-choice set (`armed_rule_e`: arming it is refused in this binary, so the Config
//! is set directly after asserting that rule E's own refusal is the only one). Each test names the violation it flips; where the
//! scenario is new it runs both arms on the same blocks, and where it is the status-quo test's own scenario the unarmed arm is that
//! test, which is unchanged.
//!
//! | test | violation | unarmed (status quo) | armed |
//! |---|---|---|---|
//! | `finx_e_v1_…` | V1 | the heavy node never weighs the lighter branch: split | both nodes on the lighter branch more bonds worked on |
//! | `finx_e_v2_…` | V2 | a deep tie keeps both incumbents: split | heals once both tips stand `W_p` above the fork, where bonds stay active; heartbeat-only stays split (residual a) |
//! | `finx_e_v3_…` | V3 | the light node seals a chain the rule would replace | the light node moves before its finality point passes the fork |
//! | `finx_e_v4_v5_…` | V4, V5 | (`finx_p0_c`) IBD and relay disagree; arrival order decides | relay, restart, IBD both ways and fresh nodes in both orders agree |
//! | `finx_e_v6_carrier_…`, `finx_e_v6_clock_…` | V6 | (`finx_p0_e`, `finx_p0_f`) X reversed | X stands |
//! | V7 | V7 | the relay skips every block below the merge-depth root | `palw_fork_choice_rule_e_v1::tests` (the relay's policy and its per-peer budget) |
//! | `finx_e_merge_past_…` | — | X stands | X stands, and the merged public attempts count for neither side (an above-the-fork count would rank the attacker first) |
//! | `finx_e_sybil_…` | V5 (Sybil) | the fresh node stays on a Sybil branch | the fresh node moves to the honest branch |
//! | `finx_e_dos_…` | — | no continuation runs | a heartbeat flood costs no validation; an attempt flood at most `PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1` a resolve |
//!
//! **Weights in this harness.** PoW is skipped, so a heartbeat carries almost no blue work and an attempt header 2^20: the side
//! with more attempt blocks is the GHOSTDAG-heavier one, whatever its producer count. Each scenario below sets "heavier" with
//! attempts by FEWER bonds (one bond attempting repeatedly) and "more bonds" on the other side. A claim's panel binds at the first
//! attempt at or past its anchor slot (acceptance + 20 DAA), which gives it anchored weight; where a scenario must stay an economic
//! tie, no attempt lands 20 DAA after another on the same chain.
use super::*;
use crate::model::services::reachability::ReachabilityService;
use kaspa_consensus_core::palw_fork_authority_v2::PalwIbdCommitV2;
use kaspa_consensus_core::palw_fork_choice_rule_e_v1::{
    PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1, PALW_RULE_E_PARTICIPATION_DEPTH_DAA_V1 as W_P, palw_rule_e_ibd_commit_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2;
use std::collections::BTreeSet;
use std::sync::atomic::Ordering::Relaxed;

/// A partition this many slots long leaves both sides past `W_p` (one DAA a slot).
const SPLIT: usize = 24;
const ARMS: [bool; 2] = [false, true];

fn arm(armed: bool) -> &'static str {
    if armed { "ARMED" } else { "unarmed" }
}

/// One slot on `c`'s own virtual, whatever its sink is: `m` holders (one carrying `txs`) then `m` steps, more steps while none
/// ticked. Nothing about the sink is asserted — under rule E a node's own block need not become its sink.
async fn free_slot(c: &mut T12Chain, nonce: &mut u64, m: usize, txs: Vec<Transaction>) -> Vec<Block> {
    let start = c.daa_of(c.sink());
    let holders = if txs.is_empty() { m } else { 1 };
    let clock = c.ctx.simulated_time + 1_000;
    let mut out = layer(c, nonce, holders, clock, txs).await.expect("the holders");
    for _ in 0..3 {
        let clock = c.ctx.simulated_time + 1_000;
        out.extend(layer(c, nonce, m, clock, Vec::new()).await.expect("the steps"));
        if c.daa_of(c.sink()) > start {
            break;
        }
    }
    out
}

/// Card `card`'s attempt on `c`'s own virtual, the sink not asserted. Returns the block and its claim id.
async fn free_attempt(c: &mut T12Chain, card: usize) -> (Block, Hash64) {
    let (block, claim) = c.build_attempt(card, 1_000, Vec::new(), &|_| true);
    let block = block.to_immutable();
    let hash = block.header.hash;
    c.ctx
        .consensus
        .validate_and_insert_block(block.clone())
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("card {card}'s attempt {hash}: {e}"));
    c.ctx.simulated_time = c.ctx.simulated_time.max(block.header.timestamp);
    (block, claim)
}

/// Is `marker` on the selected chain of `c`'s sink?
fn on_chain(c: &T12Chain, marker: BlockHash) -> bool {
    c.ctx.consensus.is_chain_ancestor_of(marker, c.sink()).unwrap_or(false)
}

/// One slot a side (the heavy side races two producers), attempts by the given cards on each side's own virtual, then both
/// sides mirrored.
async fn round_with_bonds(n: &mut Net, heavy_cards: &[usize], light_cards: &[usize]) {
    let mut h = free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await;
    for card in heavy_cards {
        h.push(free_attempt(&mut n.heavy, *card).await.0);
    }
    let mut l = free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await;
    for card in light_cards {
        l.push(free_attempt(&mut n.light, *card).await.0);
    }
    feed(&mut n.light, &h).await;
    feed(&mut n.heavy, &l).await;
}

/// **The pair as a header-verified client computes it** from fork-choice leaf v2 (`palw_fork_choice_rule_e_leaf_v2`): each tip's and
/// the fork's leaf fields built from `node`'s states, each tip's window opened above the fork and verified, every attempt record's
/// bond proven in or out of the fork's registry, then `palw_rule_e_pair_from_openings_v1`. A test compares it with the node's own.
fn client_pair(node: &T12Chain, a: BlockHash, b: BlockHash) -> kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleEPairV1 {
    use kaspa_consensus_core::palw_fork_choice_rule_e_leaf_v2::*;
    let vp = node.vp();
    let p = &node.config.params;
    let fork = vp.palw_rule_e_fork_v1(a, b).expect("the pair has a fork");
    let leaf = |block: BlockHash| {
        let state = vp.palw_candidate_state_v2(block).expect("weighable");
        let daa = vp.headers_store.get_daa_score(block).unwrap();
        let weightless = p.palw_uncertified_weightless.is_some_and(|f| f.is_active(daa));
        let (floor, window) = palw_rule_e_window_v1(
            &state,
            &node.bundle.state,
            weightless,
            p.palw_canonical_work_daa(),
            bs(node, block),
            p.finality_depth(),
        )
        .expect("a window");
        let keys = palw_rule_e_registry_keys_v1(&state);
        (palw_rule_e_leaf_v2_ext_of(floor, &window, &keys), window, keys)
    };
    let ((ext_a, win_a, _), (ext_b, win_b, _), (ext_f, _, keys_f)) = (leaf(a), leaf(b), leaf(fork.fork));
    let above = |ext: &PalwRuleELeafV2Ext, window: &[kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleEClaimRecordV1]| {
        palw_rule_e_verify_window_above_v1(ext, fork.fork_blue_score, &palw_rule_e_open_window_above_v1(window, fork.fork_blue_score))
            .expect("the opening verifies")
    };
    let (ra, rb) = (above(&ext_a, &win_a), above(&ext_b, &win_b));
    let proven: std::collections::BTreeMap<_, bool> = ra
        .iter()
        .chain(rb.iter())
        .filter(|r| r.attempt)
        .map(|r| {
            let opening = palw_rule_e_open_registry_v1(&keys_f, &r.bond);
            (r.bond, palw_rule_e_verify_registry_v1(&ext_f, &r.bond, &opening).expect("the registry opening verifies"))
        })
        .collect();
    palw_rule_e_pair_from_openings_v1(&ra, &rb, |k| proven.get(k).copied().unwrap_or(false), ext_f.registry_len)
        .expect("the client's pair")
}

/// The node's own pair for `a` against `b`.
fn node_pair(node: &T12Chain, a: BlockHash, b: BlockHash) -> kaspa_consensus_core::palw_fork_choice_rule_e_v1::PalwRuleEPairV1 {
    let mut states = crate::pipeline::virtual_processor::palw_rule_e::PalwRuleEStatesV1::default();
    node.vp().palw_rule_e_pair_v1(&mut states, a, b).expect("both tips weighable").0
}

/// The bonds of `state`'s attempt claims accepted above `fork_blue_score` — the count an "above the fork" definition would take.
fn bonds_above(state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2, fork_blue_score: u64) -> usize {
    state
        .claims_iter()
        .filter(|(_, c)| matches!(c.source, PalwClaimSourceV2::Attempt) && c.accepted_blue_score > fork_blue_score)
        .map(|(_, c)| c.bond)
        .collect::<BTreeSet<_>>()
        .len()
}

// =====================================================================================================
// V1: the lighter branch is weighed
// =====================================================================================================

/// **V1 flipped: the lighter branch more bonds worked on is weighed, and taken by both nodes.** A partition `SPLIT` slots long:
/// on the heavy side two producers race and ONE bond (card 5) attempts four times, six slots apart; on the light side one producer,
/// and cards 2 and 3 attempt once each. No panel binds (no attempt lands 20 DAA after another), so the economic keys tie. Healed:
/// unarmed, the heavy node never weighs the light tip (its search stops at its own extension) and the light node keeps its own on a
/// deep economic tie — split. Armed, the heavy node's search goes on past its own tip, finds the light tip ranked by its
/// header-level participation, weighs it over the two exclusive pasts (two bonds against one, both tips past `W_p`) and takes it;
/// the light node refuses the heavier tip on the same order.
#[tokio::test]
async fn finx_e_v1_the_lighter_branch_more_bonds_worked_on_is_weighed_and_taken() {
    kaspa_core::log::try_init_logger("warn");
    for armed in ARMS {
        let tag = format!("E-V1 {}", arm(armed));
        let mut n = net_ruled(None, armed);
        let fork = shared_prefix(&mut n).await;
        let (mut hb, mut lb) = (Vec::new(), Vec::new());
        for slot in 0..SPLIT {
            hb.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
            if slot % 6 == 0 {
                hb.push(free_attempt(&mut n.heavy, 5).await.0);
            }
            lb.extend(free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await);
            if slot < 2 {
                lb.push(free_attempt(&mut n.light, 2 + slot).await.0);
            }
        }
        let (ht, lt) = (n.heavy.sink(), n.light.sink());
        feed(&mut n.light, &hb).await;
        feed(&mut n.heavy, &lb).await;
        assert!(bw(&n.heavy, ht) > bw(&n.heavy, lt), "{tag}: the heavy side is GHOSTDAG-heavier");
        eprintln!(
            "[finx {tag}] healed {SPLIT} slots after the fork: heavy tip +{} blue work (one bond, four attempts), light tip +{} (two bonds); sinks: heavy node on {}, light node on {}; rule E searches on the heavy node {}, most extra validations {}",
            bw(&n.heavy, ht) - bw(&n.heavy, fork),
            bw(&n.heavy, lt) - bw(&n.heavy, fork),
            if n.heavy.sink() == ht { "its own" } else { "the LIGHT tip" },
            if n.light.sink() == lt { "its own" } else { "the heavy tip" },
            n.heavy.vp().palw_rule_e_searches.load(Relaxed),
            n.heavy.vp().palw_rule_e_max_extra_validated.load(Relaxed),
        );
        // The node's pair and a header-verified client's (leaf v2) are one pair.
        // The light node holds both tips UTXO-validated in either arm (it weighed the heavier one at its gate).
        let pair = node_pair(&n.light, lt, ht);
        assert_eq!(client_pair(&n.light, lt, ht), pair, "{tag}: leaf v2 gives a client the node's pair");
        assert_eq!((pair.a.participation, pair.b.participation), (2, 1), "{tag}: two bonds against one");
        assert_eq!((pair.a.economic(), pair.b.economic()), ((0, 0, 0), (0, 0, 0)), "{tag}: an economic tie");
        if armed {
            assert_eq!((n.heavy.sink(), n.light.sink()), (lt, lt), "{tag}: both nodes on the lighter branch more bonds worked on");
        } else {
            assert_eq!((n.heavy.sink(), n.light.sink()), (ht, lt), "{tag}: the status quo — split (V1)");
            assert_eq!(n.heavy.vp().palw_rule_e_searches.load(Relaxed), 0, "{tag}: no continuation runs unarmed");
        }
    }
}

// =====================================================================================================
// V2: a deep tie
// =====================================================================================================

/// **V2 flipped where bonds stay active; the heartbeat-only partition is the named residual.**
///
/// * Heartbeat-only, three and six slots, armed: still split at the heal and after three rounds — nothing unforgeable exists on
///   either side (ADR-0175 residual a; the record's §2.3 bound).
/// * Three slots with bonds on both sides (cards 2 and 3 on the heavy side, card 4 on the light side), heartbeats only after the
///   heal (no panel ever binds, so the economic keys tie for good). Unarmed: a deep economic tie keeps both incumbents — split for
///   good. Armed: the split holds while the fork is shallower than `W_p`, and heals once both tips stand `W_p` above the fork, on
///   the side with more exclusive participation. (Each node merges the other side's blocks that are lighter than its own tip and
///   inside its merge window, and a merged attempt counts for neither side — so the heavy side, whose second attempt the light
///   node never merges, keeps one exclusive bond while the light side's one attempt is merged away; the record states it.)
#[tokio::test]
async fn finx_e_v2_a_deep_tie_heals_once_w_p_deep_where_bonds_stay_active() {
    kaspa_core::log::try_init_logger("warn");
    for k in [3usize, 6] {
        let deep = tie_partition_ruled(&format!("E-V2 heartbeat-only ARMED, {k} slots"), k, 1, true).await;
        assert!(deep.iter().all(|a| !a), "heartbeat-only, {k} slots, armed: still split (residual a): {deep:?}");
    }
    for armed in ARMS {
        let tag = format!("E-V2 bonds on both sides {}", arm(armed));
        let mut n = net_ruled(None, armed);
        shared_prefix(&mut n).await;
        let (mut hb, mut lb) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            hb.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
            lb.extend(free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await);
        }
        for card in [2usize, 3] {
            hb.push(free_attempt(&mut n.heavy, card).await.0);
        }
        lb.push(free_attempt(&mut n.light, 4).await.0);
        let heavy_at_heal = n.heavy.sink();
        feed(&mut n.light, &hb).await;
        feed(&mut n.heavy, &lb).await;
        let mut agreed: Vec<bool> = vec![n.heavy.sink() == n.light.sink()];
        let rounds = W_P as usize + 8;
        for _ in 0..rounds {
            round_with_bonds(&mut n, &[], &[]).await;
            agreed.push(n.heavy.sink() == n.light.sink());
        }
        let first_agree = agreed.iter().position(|a| *a);
        eprintln!(
            "[finx {tag}] healed 3 slots after the fork; agreement per round {:?}; first agreed at round {first_agree:?}; the common sink {} the heavy side's chain",
            agreed.iter().map(|a| if *a { 'A' } else { 's' }).collect::<String>(),
            if on_chain(&n.light, heavy_at_heal) { "is on" } else { "is NOT on" }
        );
        if armed {
            assert!(agreed.last().copied().unwrap_or(false), "{tag}: healed by the end");
            assert!(
                on_chain(&n.light, heavy_at_heal) && on_chain(&n.heavy, heavy_at_heal),
                "{tag}: on the side with the exclusive participation"
            );
            assert!(!agreed[0], "{tag}: not at the heal — the fork is shallower than W_p there");
        } else {
            assert!(agreed.iter().all(|a| !a), "{tag}: the status quo — split for good (V2)");
        }
    }
}

// =====================================================================================================
// V3: the seal
// =====================================================================================================

/// **V3 flipped: the PALW rule heals before finality seals.** `finx_p0_d`'s shape at depth 60: three claims bound before the
/// fork, the light side carries one licence (of a claim both sides hold — under rule E it decides nothing), the heavy side's cards
/// 5 and 6 attempt once right after the heal (no panel of theirs binds before the seal, so the status quo stays on the licence).
/// Unarmed: the light node keeps its own chain on the licence until its finality point passes the fork — sealed. Armed: once both
/// tips stand `W_p` above the fork the light node moves to the side the bonds worked on, before its finality point reaches it.
#[tokio::test]
async fn finx_e_v3_the_rule_heals_before_finality_seals() {
    kaspa_core::log::try_init_logger("warn");
    const DEPTH: u64 = 60;
    for armed in ARMS {
        let tag = format!("E-V3 {}", arm(armed));
        let mut n = net_ruled(Some(DEPTH), armed);
        shared_prefix(&mut n).await;
        let claims = bind_claims(&mut n.heavy, 3).await;
        let shared = blocks_in_topological_order(&n.heavy);
        feed(&mut n.light, &shared).await;
        let fork = n.heavy.sink();
        let carrier = licence_carrier(&n.light, &n.floats, 0, claims[0]);
        let mut lb = free_slot(&mut n.light, &mut n.nonce, 1, vec![carrier]).await;
        let mut hb = Vec::new();
        for _ in 0..6 {
            hb.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
        }
        for _ in 1..6 {
            lb.extend(free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await);
        }
        let heavy_at_heal = n.heavy.sink();
        feed(&mut n.light, &hb).await;
        feed(&mut n.heavy, &lb).await;
        assert_ne!(n.heavy.sink(), n.light.sink(), "{tag}: split at the heal");
        let (mut moved_at, mut sealed_at) = (None, None);
        for round in 0..60usize {
            if sealed_at.is_none() && sealed_past(&n.light, fork) && !on_chain(&n.light, heavy_at_heal) {
                sealed_at = Some(round);
            }
            if moved_at.is_none() && on_chain(&n.light, heavy_at_heal) {
                moved_at = Some(round);
            }
            if moved_at.is_some() || sealed_at.is_some_and(|s| round > s + 2) {
                break;
            }
            let h: &[usize] = if round == 0 { &[5, 6] } else { &[] };
            round_with_bonds(&mut n, h, &[]).await;
        }
        eprintln!(
            "[finx {tag}] the light node moved to the heavy side at round {moved_at:?}; its finality point passed the fork (on its own chain) at round {sealed_at:?}"
        );
        if armed {
            assert!(moved_at.is_some() && sealed_at.is_none(), "{tag}: healed before the seal");
        } else {
            assert!(moved_at.is_none() && sealed_at.is_some(), "{tag}: the status quo — sealed (V3)");
        }
    }
}

// =====================================================================================================
// V4, V5: one comparator for relay and IBD; arrival order decides nothing
// =====================================================================================================

/// **V4 and V5 flipped.** `finx_p0_c`'s devnet-r1 shape, `SPLIT` slots: a claim bound before the fork, its licence carried only on
/// the light side (both sides hold the claim, so the licence decides nothing under rule E); the heavy side's cards 3 and 4 attempt.
/// Armed:
///
/// * relay: both nodes on the heavy tip at the heal;
/// * restart: both reopened on their databases stay there;
/// * IBD, from each node's own pre-heal state (the flow's rule-E commit over the claim-set difference): an old light datadir staging
///   the heavy chain COMMITS, an old heavy datadir staging the light chain KEEPS ITS OWN — the relay's answer, both ways (the
///   status-quo commit on the same states is printed: the opposite on both);
/// * fresh nodes fed the DAG in opposite orders both end on the heavy tip.
#[tokio::test]
async fn finx_e_v4_v5_relay_ibd_restart_and_arrival_order_agree() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let tag = "E-V4/V5 ARMED";
    let (config, bundle, premine, floats) = parts_ruled(None, true);
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
    let carrier = licence_carrier(&n.light, &n.floats, 0, c);
    let mut lblocks = free_slot(&mut n.light, &mut n.nonce, 1, vec![carrier]).await;
    let mut hblocks = Vec::new();
    for card in [3usize, 4] {
        hblocks.push(free_attempt(&mut n.heavy, card).await.0);
    }
    for _ in 0..SPLIT {
        hblocks.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
    }
    for _ in 1..SPLIT {
        lblocks.extend(free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await);
    }
    let (ht, lt) = (n.heavy.sink(), n.light.sink());
    // Each node's own standing before the heal — what an old datadir on each side holds.
    let (wh, wl) = (
        n.heavy.ctx.consensus.get_palw_rule_e_weighing_v1().expect("the heavy node weighs its own sink"),
        n.light.ctx.consensus.get_palw_rule_e_weighing_v1().expect("the light node weighs its own sink"),
    );
    let (oh, ol) =
        (n.heavy.ctx.consensus.get_palw_candidate_order_v2().unwrap(), n.light.ctx.consensus.get_palw_candidate_order_v2().unwrap());
    feed(&mut n.light, &hblocks).await;
    feed(&mut n.heavy, &lblocks).await;
    eprintln!(
        "[finx {tag}] healed: heavy tip +{} blue work, light tip (the licence) — sinks: heavy node {}, light node {}",
        bw(&n.heavy, ht) - bw(&n.heavy, lt),
        if n.heavy.sink() == ht { "heavy tip" } else { "light tip" },
        if n.light.sink() == ht { "heavy tip" } else { "its own" }
    );
    assert_eq!((n.heavy.sink(), n.light.sink()), (ht, ht), "{tag}: relay — both on the side the bonds worked on");

    // ---- IBD -------------------------------------------------------------------------------------------
    let (light_from_heavy, pair) = palw_rule_e_ibd_commit_v1(&config.params, &wl, &wh).expect("weighable");
    let (heavy_from_light, _) = palw_rule_e_ibd_commit_v1(&config.params, &wh, &wl).expect("weighable");
    eprintln!(
        "[finx {tag}] IBD under rule E: an old light datadir staging the heavy chain: {light_from_heavy:?} (staged participation {}, local {}); an old heavy datadir staging the light chain: {heavy_from_light:?}; the status-quo commit on the same states: {:?} / {:?}",
        pair.a.participation,
        pair.b.participation,
        palw_ibd_commit_strict_economic_v1(&ol, &oh),
        palw_ibd_commit_strict_economic_v1(&oh, &ol),
    );
    assert_eq!(light_from_heavy, PalwIbdCommitV2::Commit, "{tag}: the light datadir commits the heavy chain, as its relay does");
    assert_eq!(heavy_from_light, PalwIbdCommitV2::KeepIncumbent, "{tag}: the heavy datadir keeps its own, as its relay does");

    // ---- restart ---------------------------------------------------------------------------------------
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
    assert_eq!((heavy.sink(), light.sink()), (ht, ht), "{tag}: a restart keeps the healed sink");

    // ---- fresh nodes, opposite arrival orders ----------------------------------------------------------
    let (mut first_heavy, mut first_light) = (fresh_a, fresh_b);
    for (node, first, second) in [(&mut first_heavy, &hblocks, &lblocks), (&mut first_light, &lblocks, &hblocks)] {
        feed(node, &shared).await;
        feed(node, first).await;
        feed(node, second).await;
    }
    eprintln!(
        "[finx {tag}] fresh nodes over one DAG: heard the heavy side first -> {}; heard the light side first -> {}",
        if first_heavy.sink() == ht { "heavy tip" } else { "light tip" },
        if first_light.sink() == ht { "heavy tip" } else { "light tip" }
    );
    assert_eq!((first_heavy.sink(), first_light.sink()), (ht, ht), "{tag}: V5 flipped — arrival order decides nothing");
}

// =====================================================================================================
// V6: the stale incumbent, both forms
// =====================================================================================================

/// **V6 (carrier form) flipped**: `finx_p0_e` armed. The private branch carries Y and the public licence of a claim both branches
/// hold; released half-way to the seal, at the last slot before it, and past it. Under rule E the licence decides nothing (the
/// claim is both tips'), the attacker's exclusive past holds no bond and no economic key: X stands at every depth, and past the
/// seal as before.
#[tokio::test]
async fn finx_e_v6_carrier_form_x_stands_at_every_depth() {
    kaspa_core::log::try_init_logger("warn");
    const DEPTH: u64 = 60;
    let mut seal_depth: Option<u64> = None;
    for (label, mode) in [("past the seal", 0u8), ("at the last slot before the seal", 1), ("half-way to the seal", 2)] {
        let tag = format!("E-V6 carrier ARMED, released {label}");
        let mut n = net_ruled(Some(DEPTH), true);
        shared_prefix(&mut n).await;
        let c = bind_claims(&mut n.heavy, 1).await[0];
        let shared = blocks_in_topological_order(&n.heavy);
        feed(&mut n.light, &shared).await;
        let fork = n.heavy.sink();
        let (x, y) = (n.x.clone(), n.y.clone());
        free_slot(&mut n.light, &mut n.nonce, 1, vec![x.clone()]).await;
        let carrier = licence_carrier(&n.heavy, &n.floats, 0, c);
        let mut private = free_slot(&mut n.heavy, &mut n.nonce, 1, vec![y.clone()]).await;
        private.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, vec![carrier]).await);
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
            free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await;
            private.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
        }
        if mode == 0 {
            seal_depth = Some(bs(&n.light, n.light.sink()) - bs(&n.light, fork));
        }
        let depth = bs(&n.light, n.light.sink()) - bs(&n.light, fork);
        let victim_tip = n.light.sink();
        feed(&mut n.light, &private).await;
        let (x_out, y_out) = (TransactionOutpoint::new(x.id(), 0), TransactionOutpoint::new(y.id(), 0));
        let (has_x, has_y) = (has_utxo(&n.light, x_out), has_utxo(&n.light, y_out));
        eprintln!(
            "[finx {tag}] X {depth} blue score deep: victim sink {} — X {} Y {}",
            if n.light.sink() == victim_tip { "public" } else { "PRIVATE" },
            if has_x { "present" } else { "gone" },
            if has_y { "PRESENT" } else { "absent" }
        );
        assert!(has_x && !has_y, "{tag}: X stands");
    }
}

/// **V6 (clock form) flipped**: `finx_p0_f` armed. A branch one slot ahead stands on a shared claim's `Final` height; the claim
/// is both tips', so its `Final` decides nothing: X stands.
#[tokio::test]
async fn finx_e_v6_clock_form_x_stands() {
    kaspa_core::log::try_init_logger("warn");
    let tag = "E-V6 clock ARMED";
    let mut n = net_ruled(None, true);
    shared_prefix(&mut n).await;
    let c = bind_claims(&mut n.heavy, 1).await[0];
    let carrier = licence_carrier(&n.heavy, &n.floats, 0, c);
    free_slot(&mut n.heavy, &mut n.nonce, 1, vec![carrier]).await;
    let licensed_daa = match n.heavy.tip_state().1.claim(&c).unwrap().phase.clone() {
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
        other => panic!("{tag}: licensed in the shared history, is {other:?}"),
    };
    let final_daa = licensed_daa + n.bundle.state.window_challenge_at(licensed_daa) + 1;
    while n.heavy.daa_of(n.heavy.sink()) < final_daa - 10 {
        free_slot(&mut n.heavy, &mut n.nonce, 1, Vec::new()).await;
    }
    let shared = blocks_in_topological_order(&n.heavy);
    feed(&mut n.light, &shared).await;
    let (x, y) = (n.x.clone(), n.y.clone());
    free_slot(&mut n.light, &mut n.nonce, 1, vec![x.clone()]).await;
    let x_daa = n.light.daa_of(n.light.sink());
    while n.light.daa_of(n.light.sink()) < final_daa - 1 {
        free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await;
    }
    let mut private = free_slot(&mut n.heavy, &mut n.nonce, 1, vec![y.clone()]).await;
    while n.heavy.daa_of(n.heavy.sink()) < final_daa {
        private.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
    }
    assert!(
        matches!(n.heavy.tip_state().1.claim(&c).unwrap().phase, PalwClaimPhaseV2::Final { .. }),
        "{tag}: Final on the private tip"
    );
    let vt = n.light.sink();
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
    assert!(has_x && !has_y, "{tag}: X stands");
}

// =====================================================================================================
// The merge-past attacker
// =====================================================================================================

/// **The merge-past attacker: public attempts merged into a private branch count for neither side.** The attacker (one bond, card
/// 5) forks at the shared prefix and MERGES every public block made before X — the attempts of cards 2, 3 and 4 among them — so
/// their claims are accepted above the fork on both branches. Then X on the public chain; the attacker stops merging, carries Y,
/// and its own bond attempts every fourth slot (which keeps its branch the heavier and binds panels on it); the public chain's
/// cards 2 and 3 attempt again after X. Released with both tips `SPLIT` slots past X.
///
/// * Armed — rule E over the exclusive pasts: the merged attempts are in both tips' pasts and cancel; the attacker shows one bond
///   (its own), the victim two (its attempts after X): refused, X stands.
/// * An "above the fork" count — the definition the record warns against — gives the attacker four bonds (the three it merged and
///   its own) against the victim's three: it would rank the attacker first. Measured on the same two states.
/// * Unarmed — the status quo's absolute keys: measured and printed (the record carries it), not asserted.
#[tokio::test]
async fn finx_e_merge_past_merged_public_attempts_count_for_neither_side() {
    kaspa_core::log::try_init_logger("warn");
    for armed in ARMS {
        let tag = format!("E merge-past {}", arm(armed));
        let mut n = net_ruled(None, armed);
        let fork = shared_prefix(&mut n).await;
        let fork_bs = bs(&n.light, fork);
        // The attacker (`heavy`) leads from the fork, then merges each public slot as it appears.
        let mut private = free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await;
        let mut public_attempts = Vec::new();
        for slot in 0..4usize {
            let mut public = free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await;
            if (1..=3).contains(&slot) {
                let (a, _) = free_attempt(&mut n.light, 1 + slot).await;
                public_attempts.push(a.header.hash);
                public.push(a);
            }
            feed(&mut n.heavy, &public).await;
            private.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
        }
        let merged =
            public_attempts.iter().filter(|a| n.heavy.vp().reachability_service.is_dag_ancestor_of(**a, n.heavy.sink())).count();
        assert_eq!(merged, 3, "{tag}: the attacker's branch merged the three public attempts");
        assert!(!on_chain(&n.heavy, public_attempts[0]), "{tag}: …merged, not on its chain");
        // X on the public chain; the attacker stops merging.
        let (x, y) = (n.x.clone(), n.y.clone());
        free_slot(&mut n.light, &mut n.nonce, 1, vec![x.clone()]).await;
        private.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, vec![y.clone()]).await);
        for slot in 0..SPLIT {
            free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await;
            if slot == 2 || slot == 4 {
                free_attempt(&mut n.light, if slot == 2 { 2 } else { 3 }).await;
            }
            private.extend(free_slot(&mut n.heavy, &mut n.nonce, 2, Vec::new()).await);
            if slot % 4 == 0 {
                private.push(free_attempt(&mut n.heavy, 5).await.0);
            }
        }
        let (at, vt) = (n.heavy.sink(), n.light.sink());
        feed(&mut n.light, &private).await;
        assert!(bw(&n.light, at) > bw(&n.light, vt), "{tag}: the attacker's branch is the heavier");
        let (x_out, y_out) = (TransactionOutpoint::new(x.id(), 0), TransactionOutpoint::new(y.id(), 0));
        let (has_x, has_y) = (has_utxo(&n.light, x_out), has_utxo(&n.light, y_out));
        // Both definitions, on the victim's own two states.
        let vp = n.light.vp();
        let pair = node_pair(&n.light, at, vt);
        assert_eq!(client_pair(&n.light, at, vt), pair, "{tag}: leaf v2 gives a client the node's pair");
        let (sa, sv) = (vp.palw_candidate_state_v2(at).unwrap(), vp.palw_candidate_state_v2(vt).unwrap());
        let (naive_a, naive_v) = (bonds_above(&sa, fork_bs), bonds_above(&sv, fork_bs));
        eprintln!(
            "[finx {tag}] released {SPLIT} slots past X: exclusive participation attacker {} / victim {}; above-the-fork bonds attacker {naive_a} / victim {naive_v}; victim sink {} — X {} Y {}",
            pair.a.participation,
            pair.b.participation,
            if n.light.sink() == vt { "public" } else { "PRIVATE" },
            if has_x { "present" } else { "gone" },
            if has_y { "PRESENT" } else { "absent" }
        );
        assert_eq!((pair.a.participation, pair.b.participation), (1, 2), "{tag}: the merged attempts cancel");
        assert!(naive_a > naive_v, "{tag}: an above-the-fork count would rank the attacker first");
        if armed {
            assert!(has_x && !has_y, "{tag}: X stands");
        }
    }
}

// =====================================================================================================
// A Sybil peer flood, and the search's cost
// =====================================================================================================

/// **A fresh node fed by Sybil peers first.** Three Sybil branches from the fork — each carrying ONE bond's attempts (card 6, four
/// times, which makes each heavier than the honest branch) — reach a fresh node before the honest branch does (cards 2, 3 and 4
/// attempt on it, once each). Unarmed, the node stays on a Sybil branch (V5: heard first; V1: the lighter honest branch is never
/// weighed). Armed, the honest branch ranks first by header-level participation, is weighed, and is taken (three bonds against
/// one).
#[tokio::test]
async fn finx_e_sybil_peers_first_cannot_hold_a_fresh_node() {
    kaspa_core::log::try_init_logger("warn");
    for armed in ARMS {
        let tag = format!("E Sybil {}", arm(armed));
        let mut n = net_ruled(None, armed);
        shared_prefix(&mut n).await;
        let shared = blocks_in_topological_order(&n.light);
        let mut sybils = vec![n.fresh_node(), n.fresh_node()];
        for s in sybils.iter_mut() {
            feed(s, &shared).await;
        }
        let mut honest = Vec::new();
        for slot in 0..SPLIT {
            honest.extend(free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await);
            if slot < 3 {
                honest.push(free_attempt(&mut n.light, 2 + slot).await.0);
            }
        }
        let mut branches: Vec<Vec<Block>> = Vec::new();
        for s in std::iter::once(&mut n.heavy).chain(sybils.iter_mut()) {
            let mut b = Vec::new();
            for slot in 0..SPLIT {
                b.extend(free_slot(s, &mut n.nonce, 2, Vec::new()).await);
                if slot % 6 == 0 {
                    b.push(free_attempt(s, 6).await.0);
                }
            }
            branches.push(b);
        }
        let ht = n.light.sink();
        let mut fresh = n.fresh_node();
        feed(&mut fresh, &shared).await;
        for b in &branches {
            feed(&mut fresh, b).await;
        }
        let on_sybil = fresh.sink();
        feed(&mut fresh, &honest).await;
        let vp = fresh.vp();
        assert!(bw(&fresh, on_sybil) > bw(&fresh, ht), "{tag}: the Sybil branch the node heard first is the heavier");
        eprintln!(
            "[finx {tag}] fresh node on {} after the honest branch arrived (it was on a Sybil tip with {:+} blue work over the honest tip); continuations {}, most extra validations in one {}",
            if fresh.sink() == ht { "the HONEST tip" } else { "a Sybil tip" },
            bw(&fresh, on_sybil) - bw(&fresh, ht),
            vp.palw_rule_e_searches.load(Relaxed),
            vp.palw_rule_e_max_extra_validated.load(Relaxed),
        );
        if armed {
            assert_eq!(fresh.sink(), ht, "{tag}: the honest branch, weighed and taken");
            assert!(vp.palw_rule_e_max_extra_validated.load(Relaxed) <= PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1);
        } else {
            assert_ne!(fresh.sink(), ht, "{tag}: the status quo — the Sybil branch heard first holds");
        }
    }
}

/// **The search's cost is bounded.** An honest node four slots past a fork point F is handed a flood of light tips on F:
///
/// 1. thirty sibling heartbeats — heartbeat-only branches score no header-level participation: armed, no validation at all;
/// 2. twelve sibling attempt blocks by a registered bond (card 6) — each scores one: armed, at most
///    `PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1` validations in any one resolve however many there are, and the honest tip stays the
///    sink (each is refused: a one-block exclusive past with no economic key against a deep incumbent).
///
/// Unarmed, no continuation runs. The wall-clock per inserted block is printed (a measurement, not asserted).
#[tokio::test]
async fn finx_e_dos_a_flood_of_light_tips_costs_a_bounded_search() {
    kaspa_core::log::try_init_logger("warn");
    for armed in ARMS {
        let tag = format!("E DoS {}", arm(armed));
        let mut n = net_ruled(None, armed);
        shared_prefix(&mut n).await;
        for _ in 0..4 {
            free_slot(&mut n.light, &mut n.nonce, 1, Vec::new()).await;
        }
        let honest_tip = n.light.sink();
        // Twelve sibling attempts by card 6, built on F and never inserted on their maker (so all twelve stand on F)…
        let attempts: Vec<Block> = (0..12).map(|_| n.heavy.build_attempt(6, 1_000, Vec::new(), &|_| true).0.to_immutable()).collect();
        // …and thirty sibling heartbeats on F.
        let clock = n.heavy.ctx.simulated_time + 1_000;
        let beats = layer(&mut n.heavy, &mut n.nonce, 30, clock, Vec::new()).await.expect("thirty siblings");
        // (1) the heartbeats first.
        let t0 = std::time::Instant::now();
        feed(&mut n.light, &beats).await;
        let per_beat = t0.elapsed() / beats.len() as u32;
        let vp = n.light.vp();
        let after_beats = vp.palw_rule_e_max_extra_validated.load(Relaxed);
        // (2) then the attempts.
        let t0 = std::time::Instant::now();
        feed(&mut n.light, &attempts).await;
        let per_attempt = t0.elapsed() / attempts.len() as u32;
        let after_attempts = vp.palw_rule_e_max_extra_validated.load(Relaxed);
        eprintln!(
            "[finx {tag}] sink {}; continuations {}; most extra validations in one resolve: {after_beats} after 30 heartbeat tips, {after_attempts} after 12 attempt tips; per inserted block {per_beat:?} (heartbeat) / {per_attempt:?} (attempt)",
            if n.light.sink() == honest_tip { "the honest tip" } else { "MOVED" },
            vp.palw_rule_e_searches.load(Relaxed),
        );
        assert_eq!(n.light.sink(), honest_tip, "{tag}: the honest tip stays the sink");
        if armed {
            assert_eq!(after_beats, 0, "{tag}: a heartbeat-only flood costs no validation");
            assert_eq!(
                after_attempts, PALW_RULE_E_MAX_EXTRA_CANDIDATES_V1,
                "{tag}: an attempt flood costs at most the bound, and reaches it"
            );
        } else {
            assert_eq!(vp.palw_rule_e_searches.load(Relaxed), 0, "{tag}: no continuation runs unarmed");
        }
    }
}
