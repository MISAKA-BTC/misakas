//! **Lane BUDGET (ADR-0176): the bond budget through the real virtual processor on testnet-12** (heartbeats, attempt blocks, GHOSTDAG,
//! the coinbase, IBD, reorg and restart).
//!
//! **These tests bypass `validate_palw_v2`, and say so.** `palw_bond_budget_v1` cannot be armed on any network: its window, its rates
//! and its caps are POLICY the user has not set. Here the fence is set on a copy of testnet-12's params at DAA [`FENCE`] (after
//! `ConfigBuilder::build`, which validates), mirrored by `sync_palw_bond_budget_v1`, with a TEST policy: per a genesis card's whole
//! collateral and a window of [`W`] DAA, `ρ` claims and ONE reward block. Nothing here is evidence that the budget's values are right.
//!
//! 1. card 0 attempts and is admitted; it attempts again at once (the forger's pace) and the block stands with no claim — on an unarmed
//!    twin fed the same script the second attempt is a claim, so the budget alone refused it; ρ ×1 / ×100 / ×1000 refuse it alike;
//! 2. card 1, of the same collateral, attempts at its own slower pace and reaches the same ceiling;
//! 3. card 0's attempt one DAA before its first claim's `accepted + W` is refused, at it admitted;
//! 4. a second node fed A's blocks reaches A's PALW state root block by block, and the tip's carriage reloads under it (restart);
//! 5. a third node sees A's chain, then a heavier sibling branch, then A's again: at every switch its PALW state — the budget included —
//!    is the one the branch it stands on commits.
use super::t12_round_lane_e2e::{T12Chain, t12_genesis_chain, t12_with_harness_cards};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_bond_budget_v1::{
    PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1, PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1, PALW_BOND_BUDGET_POLICY_VERSION_V2,
    PALW_BUDGET_BLOCK_UNIT_V1, PalwBondBudgetFenceV1, PalwBondBudgetPolicyV1, PalwRoundRightsPolicyV1, PalwWeightValuePolicyV1,
    palw_bond_budget_caps_v1,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwChainStateV2, PalwConsensusObjectV2 as Obj, PalwStateCarriageV2, palw_bond_backs_live_duty_v1,
};
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
use kaspa_hashes::Hash64;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// The first DAA the budget is in force.
const FENCE: u64 = 20;
/// The TEST window.
const W: u64 = 24;

/// The collateral of genesis card `i` (the bundle's `BondRegistered`, in order).
fn card_collateral(bundle: &PalwConsensusParamsV2, i: usize) -> u64 {
    bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            Obj::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .nth(i)
        .expect("eight genesis cards")
}

/// A TEST policy: per card 0's whole collateral and [`W`], `ρ` claims and one reward block; reward and weight ample.
fn test_policy(unit: u64, rho: u32) -> PalwBondBudgetPolicyV1 {
    PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
        window_daa: W,
        capital_unit_sompi: unit,
        rho,
        claims_per_unit: 1,
        block_units_per_unit: PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: u64::MAX / 4,
        final_weight_per_unit: u64::MAX / 4,
        max_open_claims_per_bond: 1_000,
        slice_rights_by_rho: false,
        round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 },
        liability_hold_daa: PALW_BOND_BUDGET_LIABILITY_HOLD_INTERIM_DAA_V1,
        export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
        weight_value: PalwWeightValuePolicyV1::Unknown,
    }
}

/// testnet-12 with harness cards and, where `rho` is `Some`, the bond budget armed at [`FENCE`] on a copy of the params (module doc).
fn armed(rho: Option<u32>) -> (Config, PalwConsensusParamsV2, Premine, Premine, u64) {
    let (mut config, bundle, premine, floats) = t12_with_harness_cards();
    let unit = card_collateral(&bundle, 0);
    let Some(rho) = rho else { return (config, bundle, premine, floats, unit) };
    config.params.palw_bond_budget_v1 =
        Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(FENCE), policy: test_policy(unit, rho) });
    config.params.sync_palw_bond_budget_v1();
    assert!(config.params.validate_palw_v2().is_err(), "the real validator still refuses what this test bypasses");
    let PalwConsensusMode::ConsensusV2(armed) = &config.params.palw_consensus_mode else { unreachable!("ConsensusV2") };
    assert!(armed.state.bond_budget().is_some(), "the mirror is set");
    let bundle = armed.clone();
    (config, bundle, premine, floats, unit)
}

fn ttpb(chain: &T12Chain) -> u64 {
    chain.config.params.target_time_per_block()
}

async fn beat_to(chain: &mut T12Chain, daa: u64) {
    while chain.daa_of(chain.sink()) < daa {
        chain.heartbeat(ttpb(chain), Vec::new()).await;
    }
}

fn state_of(chain: &T12Chain) -> PalwChainStateV2 {
    chain.tip_state().1
}

/// Card `card`'s attempt in the next block: the block stands; its claim id if the chain holds the claim.
async fn attempt(chain: &mut T12Chain, card: usize) -> (Block, Option<Hash64>) {
    let (block, claim) = chain.attempt(card, ttpb(chain), Vec::new(), &|_| true).await;
    let held = state_of(chain).claim(&claim).is_some();
    (block, held.then_some(claim))
}

fn chain_blocks(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = upto;
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|hash| chain.ctx.consensus.get_block(hash).expect("the node holds every block of its chain")).collect()
}

async fn arrive(chain: &T12Chain, block: Block, what: &str) {
    let hash = block.header.hash;
    chain
        .ctx
        .consensus
        .validate_and_insert_block(block)
        .virtual_state_task
        .await
        .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
}

/// Steps 1–3 on node A (module doc); returns A, its config and bundle, and card 0's first claim.
struct Run {
    a: T12Chain,
    config: Config,
    bundle: PalwConsensusParamsV2,
    premine: Premine,
    floats: Premine,
    first: Hash64,
}

async fn run_a() -> Run {
    let (config, bundle, premine, floats, unit) = armed(Some(1));
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    beat_to(&mut a, FENCE).await;
    // 1. Card 0: admitted, then at once again — the block stands, no claim.
    let (_, first) = attempt(&mut a, 0).await;
    let first = first.expect("card 0's first attempt is a claim");
    let (refused_block, again) = attempt(&mut a, 0).await;
    assert_eq!(again, None, "card 0's second attempt inside its window is skipped by the budget");
    assert_eq!(a.sink(), refused_block.header.hash, "and its block stands");
    let state = state_of(&a);
    let budget = state.bond_budget().expect("the engine exists past the fence");
    let row = *budget.claim_row(&first).expect("a budgeted claim");
    let d0 = row.accepted_daa;
    assert_eq!(row.reuse_not_before, d0 + W);
    assert_eq!(row.consumed.block_units, PALW_BUDGET_BLOCK_UNIT_V1, "its block was consumed at acceptance");
    let card0 = a.bonds[0];
    assert!(budget.window_holds(&card0, d0 + W - 1));
    assert!(palw_bond_backs_live_duty_v1(&state, &card0, d0 + W - 1, None), "the window holds the capital");
    // 2. Card 1 at a slower pace, the same ceiling (cards of equal collateral only).
    if card_collateral(&bundle, 1) == unit {
        beat_to(&mut a, d0 + 4).await;
        let (_, one) = attempt(&mut a, 1).await;
        assert!(one.is_some(), "card 1's first attempt is a claim");
        beat_to(&mut a, d0 + 10).await;
        let (_, two) = attempt(&mut a, 1).await;
        assert_eq!(two, None, "card 1 reaches the same ceiling at its own pace");
        let budget = state_of(&a).bond_budget().unwrap().clone();
        let (w0, w1) = (budget.bond_row(&card0).unwrap().window, budget.bond_row(&a.bonds[1]).unwrap().window);
        assert_eq!((w1.claims, w1.block_units), (w0.claims, w0.block_units), "equal ceilings, equal use");
    }
    // 3. One DAA before d + W: refused; at it: admitted.
    // (Attempt blocks do not move testnet-12's DAA — only heartbeats do — so an attempt lands at the sink's DAA.)
    beat_to(&mut a, d0 + W - 1).await;
    let (_, early) = attempt(&mut a, 0).await;
    assert_eq!(a.daa_of(a.sink()), d0 + W - 1);
    assert_eq!(early, None, "no recovery before accepted + W");
    beat_to(&mut a, d0 + W).await;
    let (_, late) = attempt(&mut a, 0).await;
    assert!(a.daa_of(a.sink()) >= d0 + W);
    assert!(late.is_some(), "the room returns at accepted + W");
    let state = state_of(&a);
    state.bond_budget().unwrap().check_consistency().expect("the engine's invariants");
    let caps = palw_bond_budget_caps_v1(&test_policy(unit, 1), unit);
    assert!(state.bond_budget().unwrap().bond_row(&card0).unwrap().window.fits_within(&caps));
    Run { a, config, bundle, premine, floats, first }
}

/// Steps 1–3 (module doc), and the restart: the tip's carriage reloads under its root.
#[tokio::test]
async fn t12_budget_equal_ceilings_no_early_recovery_and_restart() {
    let run = run_a().await;
    let state = state_of(&run.a);
    state.assert_internal_consistency(&run.bundle.state).expect("consistent");
    let reloaded = PalwStateCarriageV2::from_state(&state).into_state(&run.bundle.state, Some(state.state_root())).expect("restart");
    assert_eq!(reloaded, state);
    assert!(state.bond_budget().unwrap().claim_row(&run.first).is_some());
}

/// **The unarmed twin admits what the budget refused** (so the refusal is the budget's), and **ρ ×1 / ×100 / ×1000 refuse it alike**:
/// one reward block a window binds whatever ρ allows in claims, and B, R and F caps do not move with ρ.
#[tokio::test]
async fn t12_budget_refusal_is_the_budgets_and_rho_moves_q_only() {
    let (config, bundle, premine, floats, unit) = armed(None);
    let mut twin = t12_genesis_chain(&config, &bundle, &premine, &floats);
    beat_to(&mut twin, FENCE).await;
    assert!(attempt(&mut twin, 0).await.1.is_some());
    assert!(attempt(&mut twin, 0).await.1.is_some(), "unarmed, card 0's second attempt is a claim");
    assert!(state_of(&twin).bond_budget().is_none(), "no engine where the fence is absent");
    let mut caps = Vec::new();
    for rho in [1u32, 100, 1_000] {
        let (config, bundle, premine, floats, _) = armed(Some(rho));
        let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
        beat_to(&mut a, FENCE).await;
        assert!(attempt(&mut a, 0).await.1.is_some(), "ρ = {rho}: the first");
        assert_eq!(attempt(&mut a, 0).await.1, None, "ρ = {rho}: one block a window, whatever Q");
        let c = palw_bond_budget_caps_v1(&test_policy(unit, rho), unit);
        assert_eq!(c.claims, rho as u64);
        caps.push((c.block_units, c.reward_sompi, c.final_weight));
        let window = state_of(&a).bond_budget().unwrap().bond_row(&a.bonds[0]).unwrap().window;
        assert_eq!((window.claims, window.block_units), (1, PALW_BUDGET_BLOCK_UNIT_V1));
    }
    assert!(caps.windows(2).all(|w| w[0] == w[1]), "B, R and F caps are invariant in ρ: {caps:?}");
}

/// **IBD**: a second node fed A's blocks holds A's PALW state root after every block and A's state — the budget included — at the tip.
#[tokio::test]
async fn t12_a_node_that_syncs_the_chain_reaches_the_same_budget_block_by_block() {
    let run = run_a().await;
    let b = t12_genesis_chain(&run.config, &run.bundle, &run.premine, &run.floats);
    for block in chain_blocks(&run.a, run.a.sink()) {
        let hash = block.header.hash;
        arrive(&b, block, "a block of A's chain").await;
        assert_eq!(b.sink(), hash, "B walks A's chain");
        let root_b = b.vp().palw_state_v2_store.read().state_root_of(hash).expect("B recorded the block's root");
        let root_a = run.a.vp().palw_state_v2_store.read().state_root_of(hash).expect("A recorded the block's root");
        assert_eq!(root_a, root_b, "the PALW state root after block {hash}");
    }
    assert_eq!(state_of(&b), state_of(&run.a));
}

/// **Reorg**: node A2 follows A, then mines its own two attempts (cards 2 and 3); A mines three (cards 4, 5, 6). A third node sees A's
/// chain, then A2's branch (heavier at that moment), then A's again: at every switch its PALW state is the branch's own.
#[tokio::test]
async fn t12_a_reorg_switches_the_budget_with_the_branch() {
    let mut run = run_a().await;
    let fork = run.a.sink();
    let mut a2 = t12_genesis_chain(&run.config, &run.bundle, &run.premine, &run.floats);
    for block in chain_blocks(&run.a, fork) {
        arrive(&a2, block, "A's chain to the fork").await;
    }
    a2.ctx.simulated_time = run.a.ctx.simulated_time;
    let (x1, _) = attempt(&mut a2, 2).await;
    let (x2, _) = attempt(&mut a2, 3).await;
    let a2_state = state_of(&a2);
    let (y1, _) = attempt(&mut run.a, 4).await;
    let (y2, _) = attempt(&mut run.a, 5).await;
    let (y3, _) = attempt(&mut run.a, 6).await;
    let r = t12_genesis_chain(&run.config, &run.bundle, &run.premine, &run.floats);
    for block in chain_blocks(&run.a, fork) {
        arrive(&r, block, "A's chain to the fork").await;
    }
    arrive(&r, x1.clone(), "A2's first").await;
    arrive(&r, x2.clone(), "A2's second").await;
    assert_eq!(r.sink(), x2.header.hash, "A2's branch carries the live work");
    assert_eq!(state_of(&r), a2_state, "R's PALW state — the budget included — is A2's branch's");
    for (block, what) in [(y1, "A's first"), (y2, "A's second"), (y3, "A's third")] {
        arrive(&r, block, what).await;
    }
    assert_eq!(r.sink(), run.a.sink(), "A's branch carries the most live work again");
    assert_eq!(state_of(&r), state_of(&run.a), "…and R's PALW state is A's again");
    assert!(state_of(&r).bond_budget().unwrap().claim_row(&run.first).is_some());
}
