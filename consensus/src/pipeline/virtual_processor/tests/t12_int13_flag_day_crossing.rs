//! **testnet-12's int-13 flag day at the processor: EVERY fence of `PALW_T12_INT13_FENCES_V1` armed at ONE height over the whole
//! compressed release, and a chain that crosses it** (`release/t12-daa9000`, the coordinator's brief of 2026-10-08: DAA 9,000 on the
//! shipped ruleset, `PALW_T12_INT13_DAA`).
//!
//! The four tier-1 fences — `palw_audit_1004_v1`, `palw_gen_range_twin_v1`, `palw_model_court_window`, `palw_receipt_spend_v4` — are
//! code changes whose prerequisites are in force on testnet-12 already (the range twin stands on the int-11 list's `palw_gen_v1`). The
//! drill that rehearses the crossing is `--palw-drill-int13-at`; this is the same crossing on the T12 harness, with the real consensus
//! path, so the claims the node folds are the ones the release folds. The ruleset is the release in its order at heights a test can
//! reach (the launch ruleset, then the DAA-750 list at 20, the second at 24, the capacity list at 28, `palw_tir_v1` at 32,
//! `palw_tir_fence2` at 36 and the int-11 list at 40 — each fence through its own `set` so every mirror follows) with the int-13 list at
//! `H`, and:
//!
//! 1. the ruleset validates, and every listed fence reads `H` (the one list, walked), live from `H` and not below, the bundle's
//!    mirrors included;
//! 2. a chain is mined from genesis to well past `H` — heartbeats, and an attempt at every even DAA by the cards in turn. Every block
//!    is a UTXO-valid sink (the harness asserts it), a heartbeat advances the DAA by at most one and never stalls at the height, and no
//!    attempt past the fence is refused;
//! 3. **below the fence an armed node IS a released node**: the armed chain's blocks below `H`, fed to a node running the same ruleset
//!    with the int-13 list dormant (the int-12 release), are each accepted and fold the same PALW root — and the blocks at and past
//!    `H`, which carry nothing the four fences distinguish, are accepted by it too (the fork id, not the block, separates the two
//!    builds: item 5);
//! 4. **no fork, no stall across the fence**: a second armed node fed the whole chain (the IBD order) reaches the same sink, the same
//!    root and the same panels on every bound claim; a third node fed the chain in two halves cut AT the fence (a restart across it) does
//!    too;
//! 5. **the fork id** keeps the released build below `H` in both directions and refuses it from `H` — over this harness's ruleset as well.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards_over};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_INT13_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3,
    PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1, Params, palw_t12_arm_int11_flag_day_at_v1,
    palw_t12_arm_int13_flag_day_at_v1, palw_t12_launch_params_v1,
};
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The int-13 list's height in this harness (the release's is 9,000); the int-11 list the range twin stands on is at 40, and its ρ = 100 step
/// (H + 95 = 135) lies past the end of the script (`H + 2 × anchor_delay + 6 = 126`), so the capacity regime is ρ = 25 throughout.
pub(super) const H: u64 = 80;
/// The compressed release's int-11 height.
const INT11: u64 = 40;

/// The four tier-1 fences' names, in the list's order.
const LIST: [&str; 4] = ["palw_audit_1004_v1", "palw_gen_range_twin_v1", "palw_model_court_window", "palw_receipt_spend_v4"];

/// The release in its order at heights a test can reach — everything BEFORE the int-13 flag day — as the harness's params.
fn compressed_int12() -> Params {
    let mut params = palw_t12_launch_params_v1();
    for (list, at) in [
        (PALW_T12_POST_LAUNCH_FENCES_V1, 20),
        (PALW_T12_POST_LAUNCH_FENCES_V2, 24),
        (PALW_T12_POST_LAUNCH_FENCES_V3, 28),
        (PALW_T12_TIR_FLAG_DAY_FENCES_V1, 32),
        (PALW_T12_TIR_FENCE2_FENCES_V1, 36),
    ] {
        for fence in list {
            (fence.set)(&mut params, Some(ForkActivation::new(at)));
        }
    }
    palw_t12_arm_int11_flag_day_at_v1(&mut params, Some(INT11));
    params.validate_palw_v2().expect("the release up to the int-11 flag day, compressed, is a runnable ruleset");
    params
}

/// The compressed release with — when `armed` — the int-13 list at `H`, each fence through its own entry (mirrors included).
pub(super) fn t12_release(armed: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let mut params = compressed_int12();
    for name in LIST {
        let (_, at) = params.palw_fences_v1().into_iter().find(|(n, _)| *n == name).expect("a fence of the ruleset");
        assert_eq!(at, None, "the int-12 release leaves {name} dormant");
    }
    if armed {
        palw_t12_arm_int13_flag_day_at_v1(&mut params, Some(H));
        params.validate_palw_v2().expect("the int-13 list over the compressed release is a runnable ruleset");
        for fence in PALW_T12_INT13_FENCES_V1 {
            let (_, at) = params.palw_fences_v1().into_iter().find(|(n, _)| *n == fence.name).expect("a fence of the ruleset");
            assert_eq!(at, Some(ForkActivation::new(H)), "{} is set to {H} by its entry", fence.name);
        }
        assert_eq!(PALW_T12_INT13_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>(), LIST);
        assert!(params.palw_audit_1004_active_at(H) && !params.palw_audit_1004_active_at(H - 1), "audit 1004 is live from {H}, not below");
        assert!(params.palw_gen_range_twin_active_at(H) && !params.palw_gen_range_twin_active_at(H - 1), "the range twin, likewise");
        assert!(params.palw_model_court_window_active_at(H) && !params.palw_model_court_window_active_at(H - 1), "the court window");
        assert!(params.palw_receipt_spend_v4_active_at(H) && !params.palw_receipt_spend_v4_active_at(H - 1), "V4 receipt redemption");
        assert!(params.palw_gen_v1_active_at(INT11) && !params.palw_gen_v1_active_at(INT11 - 1), "the range twin's prerequisite stands at {INT11}");
        let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!("testnet-12 is ConsensusV2") };
        assert_eq!(
            (bundle.state.audit_1004_from_daa(), bundle.state.gen_range_twin_from_daa()),
            (Some(H), Some(H)),
            "the fold's mirrors of the two fences it reads"
        );
    }
    t12_with_harness_cards_over(params, false)
}

/// Heartbeats since the DAA last ticked, the blocks in insertion order, and every claim the script made with its binding attempt.
struct Crossing {
    chain: T12Chain,
    /// Every chain block in insertion order: (hash, DAA, the PALW root after it).
    blocks: Vec<(BlockHash, u64, Hash64)>,
    /// Every claim the script made, and — once bound — the attempt block that bound it.
    claims: Vec<(Hash64, Option<Block>)>,
    beats_without_a_tick: u32,
}

impl Crossing {
    fn record(&mut self, block: &Block) {
        let (tip, state) = self.chain.tip_state();
        assert_eq!(tip, block.header.hash, "the recorded block is the tip");
        if let Some((_, daa, _)) = self.blocks.last() {
            assert!(block.header.daa_score >= *daa, "the DAA never runs backwards ({daa} -> {})", block.header.daa_score);
        }
        self.blocks.push((block.header.hash, block.header.daa_score, state.state_root()));
    }

    async fn beat(&mut self) {
        let before = self.chain.daa_of(self.chain.sink());
        let ttpb = self.chain.config.params.target_time_per_block();
        let beat = self.chain.heartbeat(ttpb, Vec::new()).await;
        // The heartbeat clock is paced by stamps (a beat stamped inside the current slot does not tick it), so the property is the
        // harness's honest slot: the DAA steps by one within four beats, below the fence, at it and past it.
        if beat.header.daa_score == before {
            self.beats_without_a_tick += 1;
            assert!(self.beats_without_a_tick < 4, "the DAA clock stalled at {before} (fence {H}): four beats without a tick");
        } else {
            assert_eq!(beat.header.daa_score, before + 1, "a heartbeat ticks the DAA by one at most — at DAA {before} (fence {H}) too");
            self.beats_without_a_tick = 0;
        }
        self.record(&beat);
    }

    /// Card `card`'s attempt; every claim it binds is recorded against it.
    async fn attempt(&mut self, card: usize) {
        let ttpb = self.chain.config.params.target_time_per_block();
        let (block, claim) = self.chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
        self.record(&block);
        let (_, state) = self.chain.tip_state();
        for (id, anchor) in self.claims.iter_mut().filter(|(_, anchor)| anchor.is_none()) {
            if let Some(PalwClaimPhaseV2::PanelBound { bound_daa }) = state.claim(id).map(|record| record.phase.clone()) {
                assert_eq!(bound_daa, block.header.daa_score, "claim {id} is bound at its anchor's DAA (SW-8)");
                *anchor = Some(block.clone());
            }
        }
        self.claims.push((claim, None));
    }
}

/// The script: heartbeats from genesis to `2 × anchor_delay + 6` slots past the fence, and an attempt at every even DAA by the cards in
/// turn — claims are made below the fence and bound on both sides of it.
async fn cross() -> Crossing {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_release(true);
    let delay = bundle.panel.anchor_delay();
    assert!(delay >= 1 && 2 * delay + 6 < H, "the script needs a few slots below the fence (anchor delay {delay})");
    assert!(H + 2 * delay + 6 < INT11 + 95, "the script ends before ρ = 100 (the int-11 list's second height)");
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut run = Crossing { chain, blocks: Vec::new(), claims: Vec::new(), beats_without_a_tick: 0 };
    let mut card = 0usize;
    let end = H + 2 * delay + 6;
    run.beat().await;
    loop {
        let daa = run.chain.daa_of(run.chain.sink());
        if daa >= end {
            break;
        }
        if daa % 2 == 0 {
            run.attempt(card % 8).await;
            card += 1;
        }
        run.beat().await;
    }
    run
}

/// The chain's block hashes from genesis to the sink, in order (the IBD order).
fn chain_hashes(run: &Crossing) -> Vec<BlockHash> {
    let vp = run.chain.vp();
    let genesis = run.chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = run.chain.sink();
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes
}

/// Feed `hashes` of `from`'s chain to `to`, each block through the real consensus path.
async fn feed(from: &T12Chain, to: &T12Chain, hashes: &[BlockHash], what: &str) {
    for hash in hashes {
        let block = from.ctx.consensus.get_block(*hash).expect("the armed node holds every block of its chain");
        to.ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash} of the armed chain was refused by {what}: {e}"));
    }
}

/// **The combined crossing** (module doc).
#[tokio::test]
async fn every_int13_fence_at_one_height_is_crossed_with_the_clock_running_and_the_chain_agreed_on() {
    let armed = cross().await;
    let (_, bundle, ..) = t12_release(true);
    let delay = bundle.panel.anchor_delay();

    // ---- 2: the clock ran through the height; claims bound on both sides ---------------------------------------------
    let (first, last) = (armed.blocks.first().unwrap().1, armed.blocks.last().unwrap().1);
    assert!(first < INT11 && last >= H + 2 * delay, "the DAA clock ran from below the int-11 list through the fence ({first} -> {last}, fence {H})");
    let (_, state) = armed.chain.tip_state();
    let (mut below, mut past) = (0usize, 0usize);
    for (_, anchor) in armed.claims.iter() {
        match anchor {
            Some(a) if a.header.daa_score < H => below += 1,
            Some(_) => past += 1,
            None => {}
        }
    }
    eprintln!(
        "[t12-int13] {} at DAA {H}: {} blocks, DAA {first} -> {last}; {} claims, {below} bound below the fence, {past} past it",
        PALW_T12_INT13_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>().join(", "),
        armed.blocks.len(),
        armed.claims.len()
    );
    assert!(below >= 1, "a claim was bound below the fence");
    assert!(past >= 2, "claims were bound past the fence: the attempt lane runs under the int-13 rules");
    assert!(
        armed.blocks.iter().filter(|(_, daa, _)| *daa >= H).count() > 2 * delay as usize,
        "and the chain kept producing blocks past the fence"
    );

    // ---- 3: below the fence an armed node is a released node; the blocks past it carry nothing the fences distinguish ----
    let (config, bundle, premine, floats) = t12_release(false);
    let released = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (mut compared, mut past_fence) = (0usize, 0usize);
    for (i, (hash, daa, armed_root)) in armed.blocks.iter().enumerate() {
        let block = armed.chain.ctx.consensus.get_block(*hash).expect("the armed node holds its chain");
        released
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("armed block #{i} (DAA {daa}) was refused by the released node: {e}"));
        assert_eq!(released.sink(), *hash, "armed block #{i} (DAA {daa}) is the released node's sink too");
        assert_eq!(released.tip_state().1.state_root(), *armed_root, "block #{i} (DAA {daa}): the roots agree");
        if *daa < H {
            compared += 1;
        } else {
            past_fence += 1;
        }
    }
    assert!(compared >= H as usize && past_fence >= 1, "blocks on both sides of the fence were compared ({compared} below, {past_fence} past)");

    // ---- 4: the anchors are chain data — a second armed node (IBD from genesis) agrees ---------------------------------
    let hashes = chain_hashes(&armed);
    let (config, bundle, premine, floats) = t12_release(true);
    let follower = t12_genesis_chain(&config, &bundle, &premine, &floats);
    feed(&armed.chain, &follower, &hashes, "a second armed node").await;
    assert_eq!(follower.sink(), armed.chain.sink(), "the second node walks the same chain");
    let (_, theirs) = follower.tip_state();
    assert_eq!(theirs.state_root(), state.state_root(), "and folds the same PALW root");
    for (claim, anchor) in armed.claims.iter() {
        if anchor.is_some() {
            let (mine, their) = (state.panel(claim).expect("bound"), theirs.panel(claim).expect("the second node bound it"));
            assert_eq!((their.anchor, &their.seats), (mine.anchor, &mine.seats), "claim {claim}: the same seed, the same seats");
        }
    }

    // …and a node that stops AT the fence and comes back (a restart across it) walks the rest to the same place: the chain is fed in
    // two halves cut at the first block past `H`, the tip and root checked at the cut.
    let cut = armed.blocks.iter().position(|(_, daa, _)| *daa >= H).expect("blocks past the fence");
    let (config, bundle, premine, floats) = t12_release(true);
    let restarted = t12_genesis_chain(&config, &bundle, &premine, &floats);
    feed(&armed.chain, &restarted, &hashes[..cut], "a node stopped at the fence").await;
    assert_eq!(restarted.sink(), hashes[cut - 1], "the first half ends where the armed chain stood below the fence");
    assert_eq!(restarted.tip_state().1.state_root(), armed.blocks[cut - 1].2, "…at the same root");
    feed(&armed.chain, &restarted, &hashes[cut..], "a node resumed across the fence").await;
    assert_eq!(restarted.sink(), armed.chain.sink(), "the resumed node reaches the same sink");
    assert_eq!(restarted.tip_state().1.state_root(), state.state_root(), "and the same root");

    // ---- 5: the fork id keeps the released build below H in both directions and refuses it from H ----------------------
    let (armed_config, ..) = t12_release(true);
    let (released_config, ..) = t12_release(false);
    let (new, old) = (&armed_config.params, &released_config.params);
    for daa in [INT11, H - 20, H - 1] {
        let (o, n) = (fork_id_v1(old, daa), fork_id_v1(new, daa));
        assert_eq!(n.next, H, "the armed build announces {H} next at {daa}");
        assert!(!evaluate_fork_id_v1(new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "armed keeps released at {daa}");
        assert!(!evaluate_fork_id_v1(old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "released keeps armed at {daa}");
    }
    for daa in [H, H + 1, H + 40] {
        let (o, n) = (fork_id_v1(old, daa), fork_id_v1(new, daa));
        assert!(evaluate_fork_id_v1(new, daa, o.fired.as_bytes().as_slice(), o.next).refuses(), "armed refuses released at {daa}");
        assert!(evaluate_fork_id_v1(old, daa, n.fired.as_bytes().as_slice(), n.next).refuses(), "released refuses armed at {daa}");
    }
    assert_ne!(new.consensus_params_id(), old.consensus_params_id(), "the handshake's fingerprint names the list");
    assert_eq!(new.consensus_identity_id(), old.consensus_identity_id(), "the identity is not the list's");
}
