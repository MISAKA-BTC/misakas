//! **Lane A at the processor: a testnet-12 chain that crosses the operator-anchor fence mid-run**
//! (`Params::palw_operator_anchor`, the post-launch stopgap for the panel-seed CRITICAL, user decision
//! 2026-09-26).
//!
//! testnet-12 with harness cards. The rule trusts a LIST of genesis bonds; testnet-12's armed value is
//! all eight (`palw_operator_anchor_is_t12_only`), and here — so that a real, registered, producing
//! bond can stand outside it — the list is cards 0–5 and cards 6 and 7 play the non-operators: their
//! attempts are valid chain blocks that make claims, and past the fence they may not anchor one. One
//! script, run twice: the released rule (dormant), and lane A with lane F1 (its prerequisite) at the
//! same height `H = 3 × anchor_delay` (the rollout — one common post-launch height):
//!
//! 1. card 0 (an operator) makes claim A; card 7's attempt at A's slot, BELOW `H`, anchors it under
//!    every rule (below the fence a non-operator anchors, as released) and makes claim C, whose slot is
//!    below `H`;
//! 2. heartbeats past `H`; card 6's attempt (a non-operator, past the fence) — the released rule
//!    anchors C there; lane A does not (C stays `Provisional`, and step 4c voids nothing); card 7 again,
//!    likewise;
//! 3. card 1's attempt (an operator) anchors C — a claim whose slot is below `H` and whose first
//!    attempts past its slot were a non-operator's: the key is the ANCHOR's DAA — and makes claim X;
//! 4. heartbeats to X's slot; card 6's attempt anchors nothing under lane A; card 2's (an operator) binds
//!    X and the two non-operators' own claims — non-operators' claims bind, at operator anchors.
//!
//! **Below the fence an armed node IS a released node**: the released chain's own blocks, fed to an
//! armed node, fold the same PALW root after every block up to the first non-operator attempt past
//! `H`, where the released rule binds C and lane A does not; the released chain's next block (which
//! commits the released root) is not followed. **The DAA clock advances throughout** (the heartbeat
//! lane; no attempt past the fence is refused), **a non-operator attempt past the fence never anchors**
//! (`palw_sw8_anchor_delay_for` is `None` on every one, `Some` on every operator attempt), and **the
//! anchor is chain data**: a second armed node fed the chain reaches the same sink, root, seeds and
//! seats. A second test runs a claim through its whole bind window with only non-operator attempts at
//! and after its slot: it voids `BindTimeout` at the backstop `bind_base + window_bind` (S0), not
//! before. A third races the operator's anchor attempt against a non-operator's sibling that wins the
//! selected-parent tie-break (the displacement a non-operator can stage after reading the anchor's
//! panel off its header), five times, with a heartbeat, a non-operator's attempt and another operator's
//! attempt as the next chain block: each merges the displaced attempt and binds the claim there, on the
//! DISPLACED attempt's seed, with the draw's clock at its DAA and the seats it drew while it was the
//! chain — displacement moves the binding one block and buys no draw, and cannot hold a claim off its
//! anchor. A fourth shows such a merging block anchors only the slots the merged attempt reached.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::{Config, ConfigBuilder};
use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_operator_anchor_v1::PalwOperatorAnchorV1;
use kaspa_consensus_core::palw_panel_v2::{palw_panel_anchor_execution_v1, palw_panel_draw_seed_v1};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, PalwPanelSeatV2, PalwVoidReasonV2,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The cards the harness's rule trusts; 6 and 7 stand outside it.
const OPERATORS: [usize; 6] = [0, 1, 2, 3, 4, 5];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// testnet-12 as released: every fence dormant.
    Released,
    /// Lane A and lane F1 (its prerequisite) at the same `H`.
    LaneAWithF1,
}

impl Rule {
    fn operator_anchored(self) -> bool {
        self != Rule::Released
    }
}

/// Card `i`'s bond: the `i`-th `BondRegistered` of the genesis (the harness keeps each outpoint).
fn card_bond(bundle: &PalwConsensusParamsV2, i: usize) -> PalwBondKeyV2 {
    bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        })
        .nth(i)
        .expect("testnet-12 registers eight cards")
}

/// testnet-12 with harness cards under `rule` at `h`, exactly as an operator's post-launch build would
/// set it (nothing else moves; the bundle is untouched).
fn t12_with(rule: Rule, h: u64) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    assert_eq!(config.params.palw_operator_anchor, None, "testnet-12 ships lane A's fence dormant");
    assert_eq!(config.params.palw_panel_seed_execution, None, "testnet-12 ships lane F1's fence dormant");
    if rule == Rule::Released {
        return (config, bundle, premine, floats);
    }
    let mut params = config.params.clone();
    let mut operators: Vec<PalwBondKeyV2> = OPERATORS.iter().map(|i| card_bond(&bundle, *i)).collect();
    operators.sort();
    params.palw_operator_anchor = Some(PalwOperatorAnchorV1 { activation: ForkActivation::new(h), operators });
    params.palw_panel_seed_execution = Some(ForkActivation::new(h));
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    config.params.validate_palw_v2().expect("testnet-12 with lane A armed is a runnable ruleset");
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert_eq!(armed, &bundle, "the fence is a Params field: the bundle does not move");
    (config, bundle, premine, floats)
}

fn phase(chain: &T12Chain, claim: &Hash64) -> PalwClaimPhaseV2 {
    chain.tip_state().1.claim(claim).expect("the claim stays").phase.clone()
}

fn slot_of(chain: &T12Chain, claim: &Hash64) -> u64 {
    chain.tip_state().1.claim(claim).expect("the claim exists").bind_base_daa() + chain.bundle.panel.anchor_delay()
}

/// `palw_sw8_anchor_delay_for` of a block this chain holds — the processor's one lane answer (the
/// predicate the anchor walk, step 4c and the one-state pre-check read).
fn lane_answer(chain: &T12Chain, block: &Block) -> Option<u64> {
    chain.vp().palw_sw8_anchor_delay_for(&PalwBlockContextV2 {
        block: block.header.hash,
        daa_score: block.header.daa_score,
        blue_score: block.header.blue_score,
        subsidy: 0,
    })
}

/// `H(execution ‖ claim)` of the attempt `source`, spelled from its header's own bytes (lane F1's seed).
fn execution_seed(chain: &T12Chain, source: &Block, claim: &Hash64) -> Hash64 {
    let network = palw_network_domain_v2_for(chain.config.params.net.to_string().as_bytes(), Some(chain.config.params.genesis.hash));
    let execution = palw_panel_anchor_execution_v1(network, &source.header).expect("an attempt anchors past R-core+");
    palw_panel_draw_seed_v1(&execution, claim)
}

/// The panel seed an anchor block keys a claim on under `rule` at `h` (undisturbed: the seed's source
/// is the anchor itself): lane F1's past `h`, the block otherwise.
fn seed_for(chain: &T12Chain, rule: Rule, h: u64, anchor: &Block, claim: &Hash64) -> Hash64 {
    if rule == Rule::LaneAWithF1 && anchor.header.daa_score >= h { execution_seed(chain, anchor, claim) } else { anchor.header.hash }
}

struct Run {
    chain: T12Chain,
    rule: Rule,
    h: u64,
    /// Every chain block in insertion order, with the PALW state root after it.
    blocks: Vec<(BlockHash, u64, Hash64)>,
    /// Every attempt block: (the block, its card).
    attempts: Vec<(Block, usize)>,
}

impl Run {
    fn record(&mut self, block: &Block) {
        let (tip, state) = self.chain.tip_state();
        assert_eq!(tip, block.header.hash, "the recorded block is the tip");
        self.blocks.push((block.header.hash, block.header.daa_score, state.state_root()));
    }

    async fn beat(&mut self) {
        let ttpb = self.chain.config.params.target_time_per_block();
        let beat = self.chain.heartbeat(ttpb, Vec::new()).await;
        self.record(&beat);
    }

    async fn beat_to(&mut self, daa: u64) {
        for _ in 0..(4 * daa + 400) {
            if self.chain.daa_of(self.chain.sink()) >= daa {
                return;
            }
            self.beat().await;
        }
        panic!("the chain does not reach DAA {daa}");
    }

    async fn attempt(&mut self, card: usize) -> (Block, Hash64) {
        let ttpb = self.chain.config.params.target_time_per_block();
        let (block, claim) = self.chain.attempt(card, ttpb, Vec::new(), &|_| true).await;
        self.record(&block);
        // The processor's lane answer for this block, read back: past the fence under lane A only an
        // operator's attempt may anchor; everywhere else every attempt may (R-core+'s lane rule).
        let operator = OPERATORS.contains(&card);
        let under_the_rule = self.rule.operator_anchored() && block.header.daa_score >= self.h;
        let want = (!under_the_rule || operator).then(|| self.chain.bundle.panel.anchor_delay());
        assert_eq!(
            lane_answer(&self.chain, &block),
            want,
            "{:?}: card {card}'s attempt at DAA {} (fence {}) — may it anchor?",
            self.rule,
            block.header.daa_score,
            self.h
        );
        self.attempts.push((block.clone(), card));
        (block, claim)
    }

    fn assert_bound_by(&self, name: &str, claim: &Hash64, anchor: &Block) -> (Hash64, Vec<PalwPanelSeatV2>) {
        let (_, state) = self.chain.tip_state();
        let record = state.claim(claim).unwrap_or_else(|| panic!("{name}: the claim stays"));
        let PalwClaimPhaseV2::PanelBound { bound_daa } = record.phase else {
            panic!("{:?}: {name} is bound by its anchor block; it is {:?}", self.rule, record.phase)
        };
        assert_eq!(bound_daa, anchor.header.daa_score, "{:?}: {name} is bound at its anchor's DAA (SW-8)", self.rule);
        let panel = state.panel(claim).unwrap_or_else(|| panic!("{name}: a bound claim has a panel"));
        let seed = seed_for(&self.chain, self.rule, self.h, anchor, claim);
        assert_eq!(panel.anchor, seed, "{:?}: {name}'s stored anchor is its anchor block's seed", self.rule);
        (panel.anchor, panel.seats.clone())
    }
}

struct Crossing {
    run: Run,
    first_past: usize,
    /// (name, claim, anchor block) for every claim the script binds.
    bound: Vec<(&'static str, Hash64, Block)>,
}

/// The script (module doc) under `rule`, crossing `h`.
async fn cross(rule: Rule, h: u64) -> Crossing {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = t12_with(rule, h);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut run = Run { chain, rule, h, blocks: Vec::new(), attempts: Vec::new() };
    let mut bound = Vec::new();

    // 1. A (card 0), anchored below the fence by a non-operator (card 7), which makes C.
    run.beat().await;
    let (_, a) = run.attempt(0).await;
    let slot_a = slot_of(&run.chain, &a);
    run.beat_to(slot_a).await;
    let (anchor_a, c) = run.attempt(7).await;
    assert!(anchor_a.header.daa_score < h, "A is anchored BELOW the fence ({} < {h})", anchor_a.header.daa_score);
    run.assert_bound_by("A (a non-operator's anchor below the fence)", &a, &anchor_a);
    bound.push(("A", a, anchor_a.clone()));
    let slot_c = slot_of(&run.chain, &c);
    assert!(slot_c < h, "C's slot is below the fence ({slot_c} < {h})");

    // 2. Past the fence: two non-operator attempts.
    run.beat_to(h).await;
    let first_past = run.blocks.len();
    let (skip_6, x6) = run.attempt(6).await;
    assert!(skip_6.header.daa_score >= h && skip_6.header.daa_score >= slot_c, "card 6's attempt is past the fence and C's slot");
    if rule == Rule::Released {
        run.assert_bound_by("C (released: the first attempt past its slot)", &c, &skip_6);
        bound.push(("C", c, skip_6.clone()));
    } else {
        assert_eq!(
            phase(&run.chain, &c),
            PalwClaimPhaseV2::Provisional,
            "{rule:?}: a non-operator's attempt past the fence anchors nothing"
        );
    }
    run.beat().await;
    run.beat().await;
    let (_, c7) = run.attempt(7).await;
    if rule.operator_anchored() {
        assert_eq!(
            phase(&run.chain, &c),
            PalwClaimPhaseV2::Provisional,
            "{rule:?}: nor does a second one — and step 4c voided nothing"
        );
    }
    run.beat().await;

    // 3. An operator's attempt anchors C (slot below the fence, anchor past it).
    let (anchor_c, x) = run.attempt(1).await;
    if rule.operator_anchored() {
        run.assert_bound_by("C (slot below the fence, anchor past it: the first OPERATOR attempt)", &c, &anchor_c);
        bound.push(("C", c, anchor_c.clone()));
    }

    // 4. X's slot: a non-operator's attempt, then an operator's.
    let slot_x = slot_of(&run.chain, &x);
    let (slot_x6, slot_c7) = (slot_of(&run.chain, &x6), slot_of(&run.chain, &c7));
    assert!(slot_x6 <= slot_x && slot_c7 <= slot_x, "the non-operators' own claims reach their slots by X's");
    run.beat_to(slot_x).await;
    let (skip_x, _) = run.attempt(6).await;
    if rule == Rule::Released {
        for (name, claim) in [("X", x), ("X6 (card 6's claim)", x6), ("C7 (card 7's claim)", c7)] {
            run.assert_bound_by(name, &claim, &skip_x);
            bound.push((if name == "X" { "X" } else { "non-operator claim" }, claim, skip_x.clone()));
        }
    } else {
        for claim in [x, x6, c7] {
            assert_eq!(
                phase(&run.chain, &claim),
                PalwClaimPhaseV2::Provisional,
                "{rule:?}: card 6's attempt at X's slot anchors nothing"
            );
        }
    }
    let (anchor_x, _) = run.attempt(2).await;
    if rule.operator_anchored() {
        for (name, claim) in [("X", x), ("X6 (card 6's claim)", x6), ("C7 (card 7's claim)", c7)] {
            run.assert_bound_by(name, &claim, &anchor_x);
            bound.push((if name == "X" { "X" } else { "non-operator claim" }, claim, anchor_x.clone()));
        }
    }
    Crossing { run, first_past, bound }
}

/// **The crossing.** Below the fence the armed chain is the released chain (blocks and roots); past it
/// a non-operator's attempt anchors nothing and the first operator attempt at or past each slot binds
/// the claim — the straddling claim and the non-operators' own claims included; the DAA clock runs; a
/// second node agrees.
#[tokio::test]
async fn a_chain_crossing_the_operator_anchor_fence_anchors_only_on_operator_attempts_past_it() {
    let (_, bundle, ..) = t12_with_harness_cards();
    let delay = bundle.panel.anchor_delay();
    assert!(delay >= 2, "the script needs room between two slots");
    let h = 3 * delay;
    let released = cross(Rule::Released, h).await;
    let armed = cross(Rule::LaneAWithF1, h).await;

    // ---- the released chain, fed to an ARMED node: identical below the fence --------------------------
    let (config, bundle, premine, floats) = t12_with(Rule::LaneAWithF1, h);
    let replay = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for (i, (hash, daa, released_root)) in released.run.blocks.iter().enumerate().take(released.first_past + 1) {
        let block = released.run.chain.ctx.consensus.get_block(*hash).expect("the released node holds its chain");
        replay
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("released block #{i} (DAA {daa}) was refused by an armed node: {e}"));
        assert_eq!(replay.sink(), *hash, "released block #{i} (DAA {daa}) is the armed node's sink too");
        let armed_root = replay.tip_state().1.state_root();
        if i < released.first_past {
            assert_eq!(armed_root, *released_root, "block #{i} (DAA {daa}): below the fence the armed node folds the released root");
        } else {
            assert_ne!(
                armed_root, *released_root,
                "block #{i} (DAA {daa}): the first non-operator attempt past the fence anchors C only on the released rule"
            );
        }
    }
    let (_, replayed) = replay.tip_state();
    let (name_a, a, anchor_a) = &released.bound[0];
    assert_eq!(*name_a, "A");
    assert_eq!(
        replayed.panel(a).map(|p| p.anchor),
        Some(anchor_a.header.hash),
        "A (below the fence): the released panel on an armed node"
    );
    let (_, c, _) = released.bound.iter().find(|(n, ..)| *n == "C").expect("C bound");
    assert_eq!(
        replayed.claim(c).map(|r| r.phase.clone()),
        Some(PalwClaimPhaseV2::Provisional),
        "C: the armed node did not bind it there"
    );
    let (next, next_daa, _) = released.run.blocks[released.first_past + 1];
    let block = released.run.chain.ctx.consensus.get_block(next).expect("the released node holds its chain");
    let verdict = replay.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await;
    eprintln!(
        "[t12-lane-a] the released chain's block after the first non-operator attempt past the fence (DAA {next_daa}) on an armed node: {verdict:?}"
    );
    assert_ne!(replay.sink(), next, "an armed node does not follow the released chain past the fence");

    // ---- the armed chains: who anchored whom ------------------------------------------------------------
    for crossing in [&armed] {
        let rule = crossing.run.rule;
        let operator_blocks: std::collections::BTreeSet<BlockHash> =
            crossing.run.attempts.iter().filter(|(_, card)| OPERATORS.contains(card)).map(|(b, _)| b.header.hash).collect();
        for (name, claim, anchor) in &crossing.bound {
            if anchor.header.daa_score >= h {
                assert!(
                    operator_blocks.contains(&anchor.header.hash),
                    "{rule:?}: {name} ({claim}) is anchored past the fence by an operator"
                );
            }
        }
        let outsiders_past =
            crossing.run.attempts.iter().filter(|(b, card)| !OPERATORS.contains(card) && b.header.daa_score >= h).count();
        let bound_past = crossing.bound.iter().filter(|(_, _, b)| b.header.daa_score >= h).count();
        let (first, last) = (crossing.run.blocks.first().unwrap().1, crossing.run.blocks.last().unwrap().1);
        assert!(last > h && last > first, "{rule:?}: the DAA clock ran through the crossing ({first} → {last})");
        eprintln!(
            "[t12-lane-a] {rule:?} (fence {h}): {} blocks, DAA {first} → {last}; {outsiders_past} non-operator attempts past the fence anchored nothing; \
             {bound_past} claims bound past it, every one at an operator's attempt (the straddling C and both non-operators' own claims included)",
            crossing.run.blocks.len()
        );
        assert_eq!(outsiders_past, 3, "the script's three non-operator attempts past the fence");
        assert_eq!(bound_past, 4, "C, X and the two non-operators' claims");
    }
    // Past the fence the panel is keyed on the operator attempt's execution, never on a block identity.
    let (_, c_f, anchor_c_f) = armed.bound.iter().find(|(n, ..)| *n == "C").unwrap();
    assert_ne!(
        armed.run.chain.tip_state().1.panel(c_f).unwrap().anchor,
        anchor_c_f.header.hash,
        "lane A + F1: not the block identity"
    );

    // ---- the anchor is chain data: a second armed node fed the armed chain agrees ----------------------
    let (config, bundle, premine, floats) = t12_with(Rule::LaneAWithF1, h);
    let follower = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let vp = armed.run.chain.vp();
    let genesis = armed.run.chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = armed.run.chain.sink();
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    for hash in &hashes {
        let block = armed.run.chain.ctx.consensus.get_block(*hash).expect("the node holds every block of its chain");
        follower
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("block {hash} of the armed chain was refused by a second armed node: {e}"));
    }
    assert_eq!(follower.sink(), armed.run.chain.sink(), "the second node walks the same chain");
    let (_, theirs) = follower.tip_state();
    let (_, ours) = armed.run.chain.tip_state();
    for (name, claim, _) in &armed.bound {
        let (mine, their) = (ours.panel(claim).expect("bound"), theirs.panel(claim).expect("the second node bound it"));
        assert_eq!((their.anchor, &their.seats), (mine.anchor, &mine.seats), "{name}: the same seed, the same seats");
    }
    assert_eq!(theirs.state_root(), ours.state_root(), "the same state");
}

/// **Bounded by the bind window**: a claim whose slot only non-operators reach waits — every
/// non-operator attempt at or past the slot binds nothing and voids nothing — until the backstop
/// `bind_base + window_bind`, where it voids `BindTimeout` (S0, no forfeit), exactly as a slot no
/// attempt at all reaches did. The chain's clock runs throughout.
#[tokio::test]
async fn a_slot_only_non_operators_reach_voids_at_the_bind_window_backstop() {
    kaspa_core::log::try_init_logger("warn");
    let h = 2;
    let (config, bundle, premine, floats) = t12_with(Rule::LaneAWithF1, h);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut run = Run { chain, rule: Rule::LaneAWithF1, h, blocks: Vec::new(), attempts: Vec::new() };
    run.beat_to(h).await;
    let (_, claim) = run.attempt(0).await;
    let base = run.chain.tip_state().1.claim(&claim).unwrap().bind_base_daa();
    let (slot, backstop) = (base + bundle.panel.anchor_delay(), base + bundle.state.window_bind());
    eprintln!("[t12-lane-a] claim at bind base {base}: slot {slot}, backstop {backstop}");
    // Non-operator attempts every 40 DAA from the slot to just before the backstop.
    let mut next = slot;
    while next < backstop {
        run.beat_to(next).await;
        run.attempt(if (next / 40) % 2 == 0 { 6 } else { 7 }).await;
        assert_eq!(
            phase(&run.chain, &claim),
            PalwClaimPhaseV2::Provisional,
            "DAA {next}: a non-operator's attempt binds and voids nothing"
        );
        next += 40;
    }
    run.beat_to(backstop - 1).await;
    assert_eq!(phase(&run.chain, &claim), PalwClaimPhaseV2::Provisional, "one DAA before the backstop the claim still waits");
    run.beat_to(backstop + 1).await;
    let voided = phase(&run.chain, &claim);
    eprintln!("[t12-lane-a] at DAA {}: {voided:?}", run.chain.daa_of(run.chain.sink()));
    assert!(
        matches!(voided, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BindTimeout, voided_daa } if voided_daa >= backstop),
        "the backstop voids it BindTimeout (S0): {voided:?}"
    );
}

/// Which block follows a displacing sibling onto the chain (and so merges the displaced operator attempt).
#[derive(Clone, Copy, Debug)]
enum Merger {
    /// A heartbeat — anyone's.
    Heartbeat,
    /// An attempt by a non-operator card (the displacer's side).
    NonOperator(usize),
    /// An attempt by another operator card — its own execution is later than the displaced one's.
    Operator(usize),
}

/// **One displacement race at `claim`'s slot**: the operator card `operator`'s attempt O at or past the
/// slot, published first (so it is the sink and binds the claim while it is the chain); then the
/// non-operator card `displacer`'s sibling P on the same parents, whose hash wins the tie between equal
/// blue work (the heavier-sibling variant — one extra withheld blue parent — needs no grinding at all),
/// which takes the chain; then `merger`'s block on P. Returns (O, the seed and seats O bound while it
/// was the sink, P, the merging block).
async fn displace(
    run: &mut Run,
    claim: &Hash64,
    operator: usize,
    displacer: usize,
    merger: Merger,
) -> (Block, (Hash64, Vec<PalwPanelSeatV2>), Block, Block) {
    let ttpb = run.chain.config.params.target_time_per_block();
    let slot = slot_of(&run.chain, claim);
    let (o, _) = run.chain.build_attempt(operator, ttpb, Vec::new(), &|_| true);
    let o = o.to_immutable();
    let p = loop {
        let (p, _) = run.chain.build_attempt(displacer, ttpb, Vec::new(), &|_| true);
        let p = p.to_immutable();
        if p.header.hash > o.header.hash {
            break p;
        }
    };
    assert_eq!(o.header.direct_parents(), p.header.direct_parents(), "siblings on the same parents");
    assert!(o.header.daa_score >= slot && p.header.daa_score >= slot, "both stand at or past the slot");
    run.chain.ctx.consensus.validate_and_insert_block(o.clone()).virtual_state_task.await.expect("the operator's attempt is valid");
    assert_eq!(run.chain.sink(), o.header.hash, "published first, the operator's attempt is the sink");
    let at_o =
        run.chain.tip_state().1.panel(claim).map(|x| (x.anchor, x.seats.clone())).expect("while it is the chain, O binds the claim");
    assert_eq!(at_o.0, execution_seed(&run.chain, &o, claim), "on O's execution seed");
    run.chain.ctx.consensus.validate_and_insert_block(p.clone()).virtual_state_task.await.expect("the sibling is valid");
    assert_eq!(run.chain.sink(), p.header.hash, "the sibling wins the selected-parent tie-break: O is displaced");
    assert_eq!(phase(&run.chain, claim), PalwClaimPhaseV2::Provisional, "off the chain, O binds nothing, and P anchors nothing");
    assert_eq!(lane_answer(&run.chain, &p), None, "the displacer merges no operator attempt: it anchors nothing");
    let b = match merger {
        Merger::Heartbeat => run.chain.heartbeat(ttpb, Vec::new()).await,
        Merger::NonOperator(card) | Merger::Operator(card) => run.chain.attempt(card, ttpb, Vec::new(), &|_| true).await.0,
    };
    let merged = run.chain.vp().ghostdag_store.get_data(b.header.hash).expect("ghostdag data");
    assert_eq!(merged.selected_parent, p.header.hash, "the merging block builds on the displacer");
    assert!(
        merged.mergeset_blues.contains(&o.header.hash) || merged.mergeset_reds.contains(&o.header.hash),
        "{merger:?} merges the displaced operator attempt"
    );
    (o, at_o, p, b)
}

/// **Displacing an operator attempt holds no claim off its anchor, and buys no new draw** (verification
/// of lane A, 2026-09-26, finding 1 — its probe `probe_sustained_displacement_voids_a_claim_whose_slot_
/// operator_attempts_reached`, flipped). Under the rule this replaces a claim bound only at an operator
/// attempt that was itself a chain block, so a registered non-operator that displaced every one — a
/// sibling winning the selected-parent tie-break, 50% per race — voided every claim `BindTimeout` at its
/// backstop. Past lane A a chain block anchors a claim iff it IS or MERGES an operator attempt at or past
/// the slot, so the block right after the displacer — whatever it is: a heartbeat, the non-operator
/// side's own attempt, another operator's — merges the displaced attempt and binds the claim, on the
/// DISPLACED attempt's seed (the earliest; an operator merger's own later execution does not replace it),
/// with the draw's clock at the displaced attempt's DAA (the anchor fact names it; finding 2), and with
/// the very seats O drew while it was the chain. Claims by operators and by non-operators alike; the
/// claimant itself displacing its own claim's anchor included (the withdrawal-after-observation shape).
#[tokio::test]
async fn displacing_an_operator_attempt_binds_the_claim_in_the_next_block_on_the_displaced_draw() {
    kaspa_core::log::try_init_logger("warn");
    let h = 2;
    let (config, bundle, premine, floats) = t12_with(Rule::LaneAWithF1, h);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut run = Run { chain, rule: Rule::LaneAWithF1, h, blocks: Vec::new(), attempts: Vec::new() };
    run.beat_to(h).await;
    // (claimant, operator O, displacer P, merger)
    let rounds = [
        (0, 1, 6, Merger::Heartbeat),
        (7, 2, 6, Merger::NonOperator(7)),
        (0, 4, 7, Merger::Operator(3)),
        (6, 5, 6, Merger::Heartbeat),
        (1, 3, 7, Merger::NonOperator(6)),
    ];
    let mut bound = Vec::new();
    for (round, (claimant, operator, displacer, merger)) in rounds.into_iter().enumerate() {
        let ttpb = run.chain.config.params.target_time_per_block();
        let (_, claim) = run.chain.attempt(claimant, ttpb, Vec::new(), &|_| true).await;
        let base = run.chain.tip_state().1.claim(&claim).expect("the claim").bind_base_daa();
        let slot = base + bundle.panel.anchor_delay();
        run.beat_to(slot).await;
        let (o, (seed_o, seats_o), _p, b) = displace(&mut run, &claim, operator, displacer, merger).await;
        let (_, state) = run.chain.tip_state();
        let record = state.claim(&claim).expect("the claim");
        assert_eq!(
            record.phase,
            PalwClaimPhaseV2::PanelBound { bound_daa: b.header.daa_score },
            "round {round}: the block merging the displaced attempt ({merger:?}) binds the claim (SW-8)"
        );
        let panel = state.panel(&claim).expect("bound");
        assert_eq!(panel.anchor, seed_o, "round {round}: on the DISPLACED attempt's seed");
        assert_eq!(
            panel.seats, seats_o,
            "round {round}: the seats O drew while it was the chain — the displacement bought no new panel"
        );
        if let Merger::Operator(_) = merger {
            assert_ne!(
                execution_seed(&run.chain, &b, &claim),
                seed_o,
                "round {round}: the merging operator's own execution is another draw"
            );
        }
        assert_eq!(lane_answer(&run.chain, &b), Some(bundle.panel.anchor_delay()), "round {round}: {merger:?} anchors — it merges O");
        let fact = run
            .chain
            .vp()
            .palw_v2_anchor_fact_of_candidate(b.header.hash, base, &bundle.panel)
            .expect("the claim's anchor on the chain");
        assert_eq!(fact.anchor_block, b.header.hash, "round {round}: the binding block is the anchor (SW-8)");
        assert_eq!(fact.anchor_daa, o.header.daa_score, "round {round}: the draw's clock is the displaced attempt's DAA");
        assert_eq!(fact.panel_seed(&claim), seed_o, "round {round}: the fact keys O's seed");
        eprintln!(
            "[t12-lane-a] round {round}: claim by card {claimant}; O (card {operator}, DAA {}) displaced by card {displacer}; bound by {merger:?} at DAA {} on O's seed {seed_o}, seats {:?}",
            o.header.daa_score,
            b.header.daa_score,
            panel.seats.iter().map(|s| s.bond.0.index).collect::<Vec<_>>()
        );
        bound.push((claim, b.header.daa_score));
    }
    // Nothing the races touched was voided: every claim stays bound where its merging block bound it.
    let (_, state) = run.chain.tip_state();
    for (claim, bound_daa) in &bound {
        assert_eq!(state.claim(claim).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::PanelBound { bound_daa: *bound_daa }));
    }
}

/// **A block merging a displaced operator attempt anchors only the slots that attempt reached** (step
/// 4c's `sw8_anchor_reach`), and binds however late it merges it. Claims X and Y with slots
/// `slot_x < slot_y`; the operator attempt O at X's slot — before Y's — loses the race to a
/// non-operator sibling P and reaches the node only after P's chain has passed Y's slot (withheld, or
/// kept out of the next blocks' past), so the heartbeat that finally merges it stands past both slots.
/// That heartbeat binds X on O's seed with the draw's clock at O's DAA; Y's slot came after O, so O may
/// not seed Y (a claim is keyed only on an execution produced at or past its slot): the heartbeat
/// neither binds nor voids Y, and the next operator attempt binds Y on its own execution.
#[tokio::test]
async fn a_block_merging_a_displaced_operator_attempt_anchors_only_the_slots_it_reached() {
    kaspa_core::log::try_init_logger("warn");
    let h = 2;
    let (config, bundle, premine, floats) = t12_with(Rule::LaneAWithF1, h);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let mut run = Run { chain, rule: Rule::LaneAWithF1, h, blocks: Vec::new(), attempts: Vec::new() };
    run.beat_to(h).await;
    let ttpb = run.chain.config.params.target_time_per_block();
    let (_, x) = run.attempt(0).await;
    let slot_x = slot_of(&run.chain, &x);
    run.beat_to(run.chain.daa_of(run.chain.sink()) + 1).await;
    let (_, y) = run.attempt(3).await;
    let slot_y = slot_of(&run.chain, &y);
    assert!(slot_x < slot_y, "X's slot before Y's ({slot_x} < {slot_y})");
    run.beat_to(slot_x).await;

    // O at X's slot and P on the same parents; the node sees P first, and P's chain runs past Y's slot.
    let (o, _) = run.chain.build_attempt(1, ttpb, Vec::new(), &|_| true);
    let o = o.to_immutable();
    let (p, _) = run.chain.build_attempt(6, ttpb, Vec::new(), &|_| true);
    let p = p.to_immutable();
    assert_eq!(o.header.direct_parents(), p.header.direct_parents(), "siblings on the same parents");
    assert!(o.header.daa_score >= slot_x && o.header.daa_score < slot_y, "O stands at X's slot, before Y's ({})", o.header.daa_score);
    run.chain.ctx.consensus.validate_and_insert_block(p.clone()).virtual_state_task.await.expect("the sibling is valid");
    assert_eq!(run.chain.sink(), p.header.hash);
    run.beat_to(slot_y).await;
    assert_eq!(phase(&run.chain, &x), PalwClaimPhaseV2::Provisional, "no operator attempt on P's chain: X waits");
    assert_eq!(phase(&run.chain, &y), PalwClaimPhaseV2::Provisional, "and Y");
    // O arrives; P's chain is heavier, so O stays off it until the next block merges it.
    run.chain.ctx.consensus.validate_and_insert_block(o.clone()).virtual_state_task.await.expect("the operator's attempt is valid");
    assert_ne!(run.chain.sink(), o.header.hash, "O arrives late and is not the chain");
    let b = run.chain.heartbeat(ttpb, Vec::new()).await;
    let merged = run.chain.vp().ghostdag_store.get_data(b.header.hash).expect("ghostdag data");
    assert!(merged.mergeset_blues.contains(&o.header.hash) || merged.mergeset_reds.contains(&o.header.hash), "the heartbeat merges O");
    assert!(b.header.daa_score >= slot_y, "the merging heartbeat stands past Y's slot ({} >= {slot_y})", b.header.daa_score);
    assert_eq!(lane_answer(&run.chain, &b), Some(bundle.panel.anchor_delay()), "the heartbeat anchors: it merges an operator attempt");

    let seed_o = execution_seed(&run.chain, &o, &x);
    let (_, state) = run.chain.tip_state();
    assert_eq!(
        state.claim(&x).map(|c| c.phase.clone()),
        Some(PalwClaimPhaseV2::PanelBound { bound_daa: b.header.daa_score }),
        "X binds at the block that merges O"
    );
    assert_eq!(state.panel(&x).map(|p| p.anchor), Some(seed_o), "X: on O's seed");
    let fact = run
        .chain
        .vp()
        .palw_v2_anchor_fact_of_candidate(b.header.hash, slot_x - bundle.panel.anchor_delay(), &bundle.panel)
        .expect("X's anchor");
    assert_eq!((fact.anchor_block, fact.anchor_daa), (b.header.hash, o.header.daa_score), "bound by the heartbeat, drawn at O's DAA");
    assert_eq!(
        state.claim(&y).map(|c| c.phase.clone()),
        Some(PalwClaimPhaseV2::Provisional),
        "Y: its slot came after O — the heartbeat past it neither binds nor voids it"
    );
    let (o2, _) = run.chain.attempt(2, ttpb, Vec::new(), &|_| true).await;
    let (_, state) = run.chain.tip_state();
    assert_eq!(state.claim(&y).map(|c| c.phase.clone()), Some(PalwClaimPhaseV2::PanelBound { bound_daa: o2.header.daa_score }));
    assert_eq!(
        state.panel(&y).map(|p| p.anchor),
        Some(execution_seed(&run.chain, &o2, &y)),
        "Y: on the next operator attempt's own seed"
    );
    eprintln!(
        "[t12-lane-a] reach: O at DAA {} (X's slot {slot_x}) kept off the chain until a heartbeat at DAA {} merges it: X binds there on O's seed, drawn at DAA {}; Y (slot {slot_y}) waits and binds at card 2's attempt, DAA {}",
        o.header.daa_score, b.header.daa_score, fact.anchor_daa, o2.header.daa_score
    );
}
