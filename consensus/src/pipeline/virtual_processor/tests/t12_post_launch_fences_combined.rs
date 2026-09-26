//! **testnet-12's post-launch release at the processor: EVERY fence of
//! `PALW_T12_POST_LAUNCH_FENCES_V1` armed at ONE height, and a chain that crosses it** (the user's
//! decision of 2026-09-26: one post-launch fence height, DAA 500, carrying every CRITICAL/HIGH fix).
//!
//! Each lane drilled its own fence across a crossing; none drilled them together, and the release
//! arms them together (the int-4 phase-1 audit: "no combined crossing exists"). Fences that are each
//! fine alone can freeze the clock or refuse the chain only in combination — the operator anchor
//! over the execution-commitment seed over seat maturity over the same-chain heartbeat rule — so this
//! runs testnet-12 (with harness cards) with every listed fence set through its own entry's `set` to
//! the same height `H`, exactly as the release will set them at 500 (every mirror follows; lane A
//! trusts every genesis bond, testnet-12's armed value), and:
//!
//! 1. the ruleset validates, and every listed fence reads `H` (the one list, walked);
//! 2. a chain is mined from genesis to well past `H` — heartbeats, and an attempt at every even DAA
//!    by the cards in turn (none in the two DAA just below `H`), so claims are made below the fence
//!    and bound on both sides of it (some straddle it: slot below `H`, anchor past it). Every block is a UTXO-valid sink
//!    (the harness asserts it), every heartbeat advances the DAA by exactly one — the clock never
//!    stalls at the height — and no attempt past the fence is refused;
//! 3. below the fence a claim's panel is keyed on its anchor block (the released seed); past it, on
//!    its operator anchor's execution commitment (lane F1 under lane A) — so the fences are LIVE
//!    past `H`, not merely validated;
//! 4. **below the fence an armed node IS a released node**: the armed chain's blocks below `H`, fed
//!    to a node running testnet-12 as released, are each accepted and fold the same PALW root;
//! 5. **the anchors are chain data**: a second armed node fed the whole chain reaches the same sink,
//!    the same root, and the same seed and seats on every bound claim.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1};
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{palw_panel_anchor_execution_v1, palw_panel_draw_seed_v1};
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The one post-launch height every listed fence is set to here (the release's is 500; the combined
/// crossing needs only blocks below and above one height).
const H: u64 = 60;

/// testnet-12 with harness cards, and — when `armed` — every fence of the release's list set to `H`
/// through its own entry, nothing else touched (the bundle's panel does not move; its state carries the
/// fold's mirrors of the fences that have one — lane cap-weight's F-W reads its mirror at every load, so
/// the chain loads its tip under the ARMED bundle, as a node does from its own params).
fn t12_release(armed: bool) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        let (_, at) = config.params.palw_fences_v1().into_iter().find(|(name, _)| *name == fence.name).expect("a listed fence");
        assert_eq!(at, None, "testnet-12 ships {} dormant", fence.name);
    }
    if !armed {
        return (config, bundle, premine, floats);
    }
    let mut params = config.params.clone();
    for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
        (fence.set)(&mut params, Some(ForkActivation::new(H)));
    }
    params.validate_palw_v2().expect("every post-launch fence at one height is a runnable testnet-12 ruleset");
    for (name, at) in params.palw_fences_v1() {
        if PALW_T12_POST_LAUNCH_FENCES_V1.iter().any(|f| f.name == name) {
            assert_eq!(at, Some(ForkActivation::new(H)), "{name} is set to {H} by its entry");
        }
    }
    let operators = params.palw_operator_anchor.as_ref().expect("lane A is listed").operators.len();
    assert_eq!(operators, 8, "lane A trusts every genesis bond, testnet-12's armed value");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let PalwConsensusMode::ConsensusV2(armed_bundle) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert_eq!(armed_bundle.panel, bundle.panel, "the fences are Params fields: the panel shape does not move");
    let bundle = armed_bundle.clone();
    (config, bundle, premine, floats)
}

/// `H(execution ‖ claim)` of the attempt `anchor`, spelled from its header's own bytes (lane F1's seed).
fn execution_seed(chain: &T12Chain, anchor: &Block, claim: &Hash64) -> Hash64 {
    let network = palw_network_domain_v2_for(chain.config.params.net.to_string().as_bytes(), Some(chain.config.params.genesis.hash));
    let execution = palw_panel_anchor_execution_v1(network, &anchor.header).expect("an attempt anchors past R-core+");
    palw_panel_draw_seed_v1(&execution, claim)
}

struct Crossing {
    chain: T12Chain,
    /// Every chain block in insertion order: (hash, DAA, the PALW root after it).
    blocks: Vec<(BlockHash, u64, Hash64)>,
    /// Every claim the script made, and — once bound — the attempt block that bound it.
    claims: Vec<(Hash64, Option<Block>)>,
    /// Heartbeats since the DAA last ticked.
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
        // The heartbeat clock is paced by stamps (a beat stamped inside the current slot does not tick
        // it), so the property is the harness's honest slot: the DAA steps by one within four beats,
        // below the fence, at it and past it — it never stalls at the height.
        if beat.header.daa_score == before {
            self.beats_without_a_tick += 1;
            assert!(self.beats_without_a_tick < 4, "the DAA clock stalled at {before} (fence {H}): four beats without a tick");
        } else {
            assert_eq!(
                beat.header.daa_score,
                before + 1,
                "a heartbeat ticks the DAA by one at most — at DAA {before} (fence {H}) too"
            );
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

/// The script (module doc, step 2): heartbeats from genesis to `2 × anchor_delay + 6` slots past the
/// fence, and an attempt at every even DAA by the cards in turn — except the two DAA just below the
/// fence, so the claims whose slots fall there (below `H`) are bound by the attempt AT `H`: the
/// straddling claims.
async fn cross() -> Crossing {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_release(true);
    let delay = bundle.panel.anchor_delay();
    assert!(delay >= 1 && 2 * delay + 6 < H, "the script needs a few slots below the fence (anchor delay {delay})");
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
        let straddle_gap = daa + 2 >= H && daa < H;
        if daa % 2 == 0 && !straddle_gap {
            run.attempt(card % 8).await;
            card += 1;
        }
        run.beat().await;
    }
    run
}

/// **The combined crossing** (module doc).
#[tokio::test]
async fn every_post_launch_fence_at_one_height_is_crossed_with_the_clock_running_and_the_rules_live() {
    let armed = cross().await;
    let (_, bundle, ..) = t12_with_harness_cards();
    let delay = bundle.panel.anchor_delay();

    // ---- 2 and 3: the clock ran through the height; claims bound on both sides, keyed per rule --------
    let (first, last) = (armed.blocks.first().unwrap().1, armed.blocks.last().unwrap().1);
    assert!(first < H && last >= H + 2 * delay, "the DAA clock ran through the fence ({first} -> {last}, fence {H})");
    let (_, state) = armed.chain.tip_state();
    let mut below = 0usize;
    let mut past = 0usize;
    let mut straddling = 0usize;
    for (claim, anchor) in armed.claims.iter() {
        let Some(anchor) = anchor else { continue };
        let panel = state.panel(claim).unwrap_or_else(|| panic!("claim {claim} was bound, so it has a panel"));
        let slot = state.claim(claim).expect("the claim stays").bind_base_daa() + delay;
        if anchor.header.daa_score < H {
            assert_eq!(panel.anchor, anchor.header.hash, "claim {claim}, anchored below the fence: the released seed (the block)");
            below += 1;
        } else {
            assert_eq!(
                panel.anchor,
                execution_seed(&armed.chain, anchor, claim),
                "claim {claim}, anchored past the fence: lane F1's seed off the operator attempt's execution"
            );
            past += 1;
            if slot < H {
                straddling += 1;
            }
        }
    }
    eprintln!(
        "[t12-post-launch] every listed fence at DAA {H} ({}): {} blocks, DAA {first} -> {last}; {} claims, {below} bound below the fence, \
         {past} past it ({straddling} straddling it)",
        PALW_T12_POST_LAUNCH_FENCES_V1.iter().map(|f| f.name).collect::<Vec<_>>().join(", "),
        armed.blocks.len(),
        armed.claims.len()
    );
    assert!(below >= 1, "a claim was bound below the fence");
    assert!(past >= 2, "claims were bound past the fence");
    assert!(straddling >= 1, "a claim whose slot is below the fence was bound past it");
    // Lane cap-weight (ADR-0160 F-W) is live past the height too: a claim accepted at or past it is staged
    // under its bond's cap, one accepted below it keeps today's raw weight, and the tip's weight is the
    // capped re-derivation under the armed bundle a node loads with (W-I3 at the processor).
    {
        use kaspa_consensus_core::palw_weight_cap_v1::{palw_bounded_immature_v2, palw_weight_cap_applies_v1};
        let armed_state = &armed.chain.bundle.state;
        assert_eq!(armed_state.capacity_weight_cap_from_daa(), Some(H), "the fold's mirror of F-W is the height");
        let (new_rule, old_rule): (Vec<_>, Vec<_>) = state.claims_iter().partition(|(_, c)| palw_weight_cap_applies_v1(armed_state, c));
        assert!(new_rule.len() >= 2 && !old_rule.is_empty(), "claims on both sides of F-W ({} / {})", old_rule.len(), new_rule.len());
        assert_eq!(palw_bounded_immature_v2(&state, armed_state), state.bounded_immature(), "W-I3: the tip's weight is the re-derivation");
        eprintln!(
            "[t12-post-launch] F-W: {} claims past it (staged, capped), {} below it (raw); bounded_immature {}",
            new_rule.len(),
            old_rule.len(),
            state.bounded_immature()
        );
    }

    // ---- 4: below the fence an armed node is a released node -------------------------------------------
    let (config, bundle, premine, floats) = t12_release(false);
    let released = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut compared = 0usize;
    for (i, (hash, daa, armed_root)) in armed.blocks.iter().enumerate().take_while(|(_, (_, daa, _))| *daa < H) {
        let block = armed.chain.ctx.consensus.get_block(*hash).expect("the armed node holds its chain");
        released
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("armed block #{i} (DAA {daa}, below the fence) was refused by a released node: {e}"));
        assert_eq!(released.sink(), *hash, "armed block #{i} (DAA {daa}) is the released node's sink too");
        assert_eq!(released.tip_state().1.state_root(), *armed_root, "block #{i} (DAA {daa}): below the fence the roots agree");
        compared += 1;
    }
    assert!(compared >= H as usize, "every block below the fence was compared ({compared})");

    // ---- 5: the anchors are chain data — a second armed node agrees ------------------------------------
    let (config, bundle, premine, floats) = t12_release(true);
    let follower = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let vp = armed.chain.vp();
    let genesis = armed.chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = armed.chain.sink();
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    for hash in &hashes {
        let block = armed.chain.ctx.consensus.get_block(*hash).expect("the node holds every block of its chain");
        follower
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash} of the armed chain was refused by a second armed node: {e}"));
    }
    assert_eq!(follower.sink(), armed.chain.sink(), "the second node walks the same chain");
    let (_, theirs) = follower.tip_state();
    assert_eq!(theirs.state_root(), state.state_root(), "and folds the same PALW root");
    for (claim, anchor) in armed.claims.iter() {
        if anchor.is_some() {
            let (mine, their) = (state.panel(claim).expect("bound"), theirs.panel(claim).expect("the second node bound it"));
            assert_eq!((their.anchor, &their.seats), (mine.anchor, &mine.seats), "claim {claim}: the same seed, the same seats");
        }
    }
}
