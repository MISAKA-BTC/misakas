//! **ADR-0172 through testnet-12's own pipeline** (template → GHOSTDAG → fold → coinbase → UTXO state), `palw_accounting_v2` armed at a low DAA with every prerequisite
//! the validator names armed from DAA 1.
//!
//! What the chain does, block by block, once the fence is in force:
//!
//! * every chain block declares ZERO subsidy (blocks create no MSK) — the template's coinbase equals the validator's on every block, or the block would not be the sink;
//! * an algo-8 header must carry a FALLBACK envelope (a bare heartbeat is refused), and below the fence an envelope is refused;
//! * a FALLBACK of an eligible bond is credited (`fallback_weight`) while the floor is Idle, once a bond a DAA, and never while REAL work flows;
//! * a REAL attempt's claim enters its DAA's row at acceptance, the first block of the next DAA closes the row (the allocation), and no coinbase withheld or minted its carve;
//! * a second node fed the same blocks reaches the same PALW state root at every block.
//!
//! **Not covered here, stated:** a `Final` through a real panel (this rig runs no panel, so a REAL claim never resolves — see `t12_anchor_window`'s note). The payout cap at `Final`
//! is held at builder level (`palw_state_v2::tests::accounting_v2`) with 1,000 claims, voids, reorg deltas and the carriage.

use super::*;
use kaspa_consensus_core::config::params::{
    PALW_T12_CAPACITY_FENCES_V1, PALW_T12_DECODE_RULES_FENCES_V1, PALW_T12_INT11_FENCES_V1, PALW_T12_MODEL_COURT_WINDOW_FENCES_V1,
    PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_POST_LAUNCH_FENCES_V4,
    PALW_T12_TIR_FENCE2_FENCES_V1, PALW_T12_TIR_FLAG_DAY_FENCES_V1, PalwPostLaunchFenceV1,
};
use kaspa_consensus_core::palw_accounting_v2::{PalwAccountingKeyV2 as K, PalwAccountingRowV2 as R};

/// The fence's height: past the ten heartbeat slots `Rig::over` mines, a couple of slots into the test.
const F: u64 = 12;

fn every_list() -> Vec<&'static [PalwPostLaunchFenceV1]> {
    vec![
        PALW_T12_POST_LAUNCH_FENCES_V1,
        PALW_T12_POST_LAUNCH_FENCES_V2,
        PALW_T12_POST_LAUNCH_FENCES_V3,
        PALW_T12_POST_LAUNCH_FENCES_V4,
        PALW_T12_CAPACITY_FENCES_V1,
        PALW_T12_DECODE_RULES_FENCES_V1,
        PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_MODEL_COURT_WINDOW_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1,
        PALW_T12_INT11_FENCES_V1,
    ]
}

/// testnet-12 with the Useful Work Transition armed from genesis, `palw_accounting_v2` at `at`, and — by the validator's own words — every prerequisite it names armed from
/// DAA 1 (each through its own entry, mirrors included). Lane A, if the validator pulls it in, keeps its operator set cut to the first four genesis cards.
fn config_accounting(at: u64) -> Parts {
    let (config, bundle, premine, floats) = config_of(true);
    let mut params = config.params.clone();
    params.palw_accounting_v2 = Some(ForkActivation::new(at));
    let mut armed: Vec<&'static str> = Vec::new();
    loop {
        params.sync_palw_accounting_v2();
        let Err(e) = params.validate_palw_v2() else { break };
        let message = format!("{e:?}");
        let dormant: Vec<&'static str> = params.palw_fences_v1().into_iter().filter(|(_, at)| at.is_none()).map(|(name, _)| name).collect();
        let wanted: Vec<&'static str> = dormant.into_iter().filter(|name| message.contains(name) && *name != "palw_accounting_v2").collect();
        let mut progressed = false;
        for name in wanted {
            if armed.contains(&name) {
                continue;
            }
            if let Some(fence) = every_list().into_iter().flatten().find(|f| f.name == name) {
                (fence.set)(&mut params, Some(ForkActivation::new(1)));
                armed.push(name);
                progressed = true;
            }
        }
        assert!(progressed, "accounting v2 cannot be armed: {message} (armed so far: {armed:?})");
    }
    if let Some(rule) = params.palw_operator_anchor.as_mut() {
        let mut ops: Vec<_> = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| match o {
                kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::BondRegistered { bond, .. } => Some(*bond),
                _ => None,
            })
            .take(4)
            .collect();
        ops.sort();
        rule.operators = ops;
    }
    params.validate_palw_v2().expect("testnet-12 with accounting v2 and its prerequisites validates");
    eprintln!("[accounting-v2] prerequisites armed from DAA 1: {armed:?}");
    let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    let bundle = match &config.params.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => unreachable!("testnet-12 is ConsensusV2"),
    };
    assert!(bundle.state.accounting_v2_active_at(at) && !bundle.state.accounting_v2_active_at(at - 1), "the fold's mirror follows the fence");
    (config, bundle, premine, floats)
}

fn declared_subsidy(chain: &T12Chain, block: &Block) -> u64 {
    chain.vp().coinbase_manager.deserialize_coinbase_payload(&block.transactions[0].payload).unwrap().subsidy
}

fn minted(block: &Block) -> u128 {
    block.transactions[0].outputs.iter().map(|o| o.value as u128).sum()
}

async fn follower_of(rig: &Rig, upto: BlockHash) -> T12Chain {
    let (config, bundle, premine, floats) = config_accounting(F);
    let chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for block in blocks_through(&rig.c, upto) {
        let hash = block.header.hash;
        arrive(&chain, block, "a block of the followed chain").await;
        for (at, planted) in rig.plants.iter().filter(|(at, _)| *at == hash) {
            chain.vp().palw_state_v2_store.write().set_tip_for_tests(*at, planted).expect("the same plant, at the same block");
        }
    }
    assert_eq!(chain.sink(), upto);
    chain
}

#[tokio::test]
async fn accounting_v2_the_fence_crosses_with_the_clock_running_fallbacks_credited_and_blocks_minting_nothing() {
    let mut rig = Rig::over(config_accounting(F)).await;
    let coinbase = rig.c.vp();
    let schedule = move |daa: u64| coinbase.coinbase_manager.calc_block_subsidy(daa);
    assert!(rig.c.daa_of(rig.c.sink()) < F, "the test starts below the fence");

    // Below the fence: unsigned heartbeats, as ever; an envelope on one is refused.
    let mut blocks = Vec::new();
    while rig.c.daa_of(rig.c.sink()) + 1 < F {
        blocks.extend(honest_slot(&mut rig.c).await);
    }
    assert!(blocks.iter().all(|b| b.header.palw_commitment.is_empty()), "below the fence a heartbeat carries nothing");
    assert_eq!(rig.state().fallback_weight_v2(), 0, "nothing is credited below the fence");

    // Cross: from DAA F every beat is a FALLBACK. A bare one is refused (build it as the pre-fence harness did).
    while rig.c.daa_of(rig.c.sink()) < F {
        blocks.extend(honest_slot(&mut rig.c).await);
    }
    let bare = {
        rig.c.ctx.simulated_time += 1_000;
        let mut t = rig
            .c
            .ctx
            .consensus
            .build_block_template(MinerData::new(card_payout_spk(1), vec![]), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .expect("a template");
        stamp_harness_time(&rig.c.config.params, &mut t.block.header, rig.c.ctx.simulated_time);
        t.block.header.nonce = 77;
        t.block.header.finalize();
        let (t, _) = rig.c.vp().heartbeat_adapt_block_template(t).expect("the lane is open");
        t.block.to_immutable()
    };
    assert!(bare.header.daa_score >= F && bare.header.palw_commitment.is_empty());
    let refused = rig.c.ctx.consensus.validate_and_insert_block(bare).virtual_state_task.await;
    assert!(refused.is_err(), "a bare heartbeat at or past the fence is no FALLBACK and no block");

    // FALLBACKs: signed, credited (floor Idle), once a bond a DAA; every block declares zero and the template is the validator's coinbase.
    let before = rig.state().fallback_weight_v2();
    let mut daas = Vec::new();
    for _ in 0..3 {
        let slot = honest_slot(&mut rig.c).await;
        for block in &slot {
            assert!(block.header.daa_score >= F);
            assert!(!block.header.palw_commitment.is_empty(), "a FALLBACK carries its envelope");
            assert_eq!(declared_subsidy(&rig.c, block), 0, "past the fence no block declares a subsidy");
            assert!(minted(block) <= schedule(block.header.daa_score) as u128 * 28 / 100, "a block mints at most the tick's validator + inclusion shares: {}", minted(block));
            daas.push(block.header.daa_score);
        }
    }
    let credited = rig.state().fallback_weight_v2() - before;
    let w = kaspa_consensus_core::palw_accounting_v2::PALW_ACCOUNTING_V2_W_FB_V1 as u128;
    assert!(credited >= w && credited % w == 0, "FALLBACKs of card 1's bond are credited whole: {credited}");
    let distinct: std::collections::BTreeSet<u64> = daas.iter().copied().collect();
    assert!(credited / w <= distinct.len() as u128 + 1, "at most one credit a DAA for one bond ({} credits over {} DAAs)", credited / w, distinct.len());
    assert!(rig.state().candidate_order(Hash64::default()).safe_weight >= credited, "the comparator reads it");

    // A REAL attempt: its claim enters its DAA's row; the block mints and withholds nothing; REAL work flowing refuses the reserve its weight.
    rig.plant();
    let (attempt, claim) = rig.real_now(2, 1_000).await;
    assert_eq!(declared_subsidy(&rig.c, &attempt), 0);
    let claim_row = rig.state().claim(&claim).cloned().expect("the claim exists");
    assert!(claim_row.escrowed_reward > 0, "the claim records its carve as a CEILING");
    let d = claim_row.accepted_daa;
    let open = match rig.state().accounting_v2_for_tests(&K::DaaOpen(d)) {
        Some(R::Daa(row)) => row,
        other => panic!("the claim's DAA has an open row, got {other:?}"),
    };
    assert_eq!((open.count, open.open, open.budget), (1, 1, schedule(d)), "one claim, one budget B_d");
    assert!(open.sum_w > 0);
    let frozen = rig.state().fallback_weight_v2();
    for _ in 0..2 {
        honest_slot(&mut rig.c).await;
    }
    assert_eq!(rig.state().fallback_weight_v2(), frozen, "REAL work is flowing (floor Normal): the reserve earns no weight");
    // The next DAA closes the row: the allocation is fixed.
    let closed = rig.state().daa_allocation_v2(d).expect("DAA d is closed once the chain is past it");
    assert_eq!((closed.count, closed.sum_w, closed.budget), (open.count, open.sum_w, open.budget));
    assert!(rig.state().accounting_v2_for_tests(&K::DaaOpen(d)).is_none(), "an open row never outlives its DAA");

    // A second node fed the same blocks agrees on the PALW root at every block it walks (IBD / arrival order is the rig's: spine with side blocks first).
    let upto = rig.c.sink();
    let follower = follower_of(&rig, upto).await;
    assert_eq!(root_of(&follower, upto), root_of(&rig.c, upto), "the same PALW root");
    assert_eq!(follower.tip_state().1.fallback_weight_v2(), rig.state().fallback_weight_v2());
    assert_eq!(follower.tip_state().1.daa_allocation_v2(d), rig.state().daa_allocation_v2(d));
}

#[tokio::test]
async fn accounting_v2_dormant_the_same_chain_is_the_launched_chain() {
    // The fence at a height the chain never reaches: heartbeats stay bare, nothing is credited, no ledger row exists.
    let mut rig = Rig::over(config_accounting(1_000_000)).await;
    for _ in 0..4 {
        for block in honest_slot(&mut rig.c).await {
            assert!(block.header.palw_commitment.is_empty());
        }
    }
    assert_eq!(rig.state().fallback_weight_v2(), 0);
    assert!(rig.state().accounting_v2_for_tests(&K::DaaOpen(1)).is_none());
}
