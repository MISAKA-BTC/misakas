//! **lane F03k — T05 on the kernel route: one outsider convicts while colluding Sybil bonds crowd the court** (2026-10-26).
//!
//! The kernel route's prosecution is a NON-INTERACTIVE public fault proof (`K::FileProof`, tag 110–119 objects). It owns no per-claim
//! challenger slot or reservation queue that a Sybil bond could take first, hold silently and refill (F03's TIR-route hole):
//!
//! * a filing reserves nothing across blocks — the only shared resource is the BLOCK's adjudication budget, which every block
//!   starts afresh (`BlockBudgetV1::charged_v1`), and which a dismissed filing pays for out of its filer's collateral (burned);
//! * the court scope (ADR-0177 D2) counts dismissals PER ACCUSER (`court_scope_filing_allows_v1`): a Sybil's dismissals never
//!   spend the outsider's allowance;
//! * a demand is one shared session per (claim, stage, position), joinable, with its deadline fixed when it opens.
//!
//! These cases pin that as an invariant on the native processor: the producer and every seat collude on a lie, two Sybil bonds
//! (colluding cards outside the Panel) saturate a block's court with junk filings and do it again in a later block, and the
//! outsider — a post-genesis bond on a node started after the claim — still convicts from public material alone, before Final and
//! after Final. Without the outsider nothing convicts. FIXTURE LIMITS (reported, not hidden): the canonical world assigns a 3-seat
//! interim Panel (`claimed` asserts 3), not Panel=1; the Sybils are harness cards (their keys prove no independence, and none is
//! needed: they are attackers). No cryptographic proof of computation is used (PRINCIPLES §8).

use super::*;

/// Junk a colluder files to burn the block's court: kernel-proof bytes no court accepts.
fn junk() -> ProsecutionV1 {
    ProsecutionV1::Kernel(vec![0xEE; 32])
}

/// Every Sybil seals, then (one block later) files junk against `claim` — all filings in ONE block of `net`.
async fn crowd(net: &mut Net, sybils: &[usize], claim: Digest) {
    let seals: Vec<(usize, Obj)> = sybils
        .iter()
        .map(|c| {
            let accuser = net.kid(*c);
            let o = net.route(*c, &K::SealProof { accuser, claim, seal: proof_seal_v1(&claim, &accuser, &junk()) });
            (*c, o)
        })
        .collect();
    net.send(seals).await;
    let filings: Vec<(usize, Obj)> = sybils
        .iter()
        .map(|c| {
            let accuser = net.kid(*c);
            (*c, net.route(*c, &K::FileProof { accuser, claim, proof: junk() }))
        })
        .collect();
    net.send(filings).await;
}

/// Two crowding rounds (the "silence, then refill" pattern): every round costs every Sybil at least one dismissal fee, convicts
/// nothing and leaves the claim's state as it was.
async fn crowd_twice(w: &mut World, sybils: &[usize], claim: Digest) {
    let fee = w.policy().dismissed_proof_fee;
    for round in 0..2 {
        let before: Vec<u64> = sybils.iter().map(|c| w.net.collateral(*c)).collect();
        let state = format!("{:?}", w.net.claim_state(&claim));
        crowd(&mut w.net, sybils, claim).await;
        for (c, b) in sybils.iter().zip(&before) {
            assert!(*b - w.net.collateral(*c) >= fee, "round {round}: Sybil {c} paid its dismissal ({b} -> {})", w.net.collateral(*c));
        }
        assert!(!w.net.ledger().claims[&claim].convicted, "round {round}: junk convicts nothing");
        let after = format!("{:?}", w.net.claim_state(&claim));
        assert_eq!(after.split(['{', ' ']).next(), state.split(['{', ' ']).next(), "round {round}: junk moves no lifecycle");
        // Silence between rounds: one empty block; nothing a Sybil filed is held over it.
        let ttpb = w.net.ttpb();
        w.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
}

/// **T05, kernel route**: lie covered by every seat, Sybils crowd the court twice, then `outsider` decides the case.
pub(super) async fn t05_in(panel_one: bool, after_final: bool, outsider: bool) {
    kaspa_core::log::try_init_logger("warn");
    let mode = Mode::Panel;
    let mut w = if panel_one { panel_one_world().await } else { world(mode).await };
    let funding = newcomer_funding(&w.net);
    let before = w.net.collateral(0);
    let (claim, seats) = claimed(&mut w, mode, 0, true).await;
    assert_eq!(seats.len(), if panel_one { 1 } else { 3 }, "the world's seat policy");
    let sybils = w.outsiders(&claim, &seats, 2);
    let floor = final_floor(&w, mode, &claim.id);
    let slashed = reservation(&w, mode);
    let policy = w.policy();

    if after_final {
        w.net.beat_to(floor).await;
        assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
    }
    crowd_twice(&mut w, &sybils, claim.id).await;

    if !outsider {
        // Negative: colluders' junk never convicts; the lie finalizes and is paid, and stays unconvicted.
        w.net.beat_to(floor.max(w.net.daa()) + 2).await;
        assert!(matches!(w.net.claim_state(&claim.id), ClaimStateV1::Final { .. }), "{:?}", w.net.claim_state(&claim.id));
        assert!(!w.net.ledger().claims[&claim.id].convicted, "no outsider: no conviction");
        // (The coinbase queue may already have paid the reward out by now, so read the claim's rewarded flag, not `owed`.)
        let _ = Rules::final_reward(&policy);
        assert!(Rpc::of(&w.net.chain).claim(&claim.id).rewarded, "the lie is rewarded when nobody outside prosecutes");
        assert_eq!(w.net.collateral(0), before, "and the producer is not slashed");
        return;
    }

    let mut node = start_node(&mut w.net, Start::Ibd).await;
    let bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut w.net).await;
    know_newcomer(&mut w.net, bond);
    assert!(!seats.contains(&NEWCOMER) && !sybils.contains(&NEWCOMER));
    let fresh = Rpc::of(&node.chain).verifier(&claim.id, 0x5A);
    let model =
        acquire(&[ModelPeer::Serves(w.fx.params.clone())], &registered_root(&fresh, &claim.id)).expect("a peer serves the model");
    let da = claim.published(&w.fx, &[]);
    let OutsiderFindingV1::Prosecute(proof) = fresh.check(claim.id, &da, &model) else {
        panic!("the outsider's verifier finds the lie from public material")
    };
    // The Sybils refill once more on the producer's node right before the outsider files (its blocks reach the outsider's node).
    crowd(&mut w.net, &sybils, claim.id).await;
    sync(&w.net, &mut node).await;
    Rules::file_proof(&mut node, NEWCOMER, claim.id, proof).await;
    sync(&node, &mut w.net).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&claim.id].convicted, "the outsider convicts through the crowded court");
    assert_eq!(w.net.collateral(0), before - slashed, "the producer bond lost the reservation");
    assert_eq!(w.net.owed(NEWCOMER), Rules::bounty(&policy, slashed), "the outsider's bounty (no Sybil sealed the true bytes)");
    let st = w.net.claim_state(&claim.id);
    if after_final {
        assert!(matches!(st, ClaimStateV1::Final { .. }), "Final stays Final; the conviction is the liability's: {st:?}");
    } else {
        assert!(matches!(st, ClaimStateV1::Convicted { .. }), "{st:?}");
        w.net.beat_to(floor + 2).await;
        assert_eq!(w.net.owed(0), 0, "a convicted claim is never paid");
    }
    let ttpb = w.net.ttpb();
    w.net.chain.heartbeat(ttpb, Vec::new()).await;
    sync(&w.net, &mut node).await;
    agree(&w.net, &node, "the outsider's node");
}

/// **SPINE's entry (S3b')**: T05 in the Panel=1 world.
pub(super) async fn t05(after_final: bool, outsider: bool) {
    t05_in(true, after_final, outsider).await;
}

/// **TEST ONLY (lane F03k2)**: the external gain bound the Panel=1 world's class relation uses. It is not a network value;
/// production reads `palw_g14_values_v1` (typed-missing).
pub(super) const TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI: u64 = 100_000_000;

/// The canonical Panel network with `palw_kernel_route_panel_one_v1` armed at `from` (mirrored onto the bundle, as
/// `Params::sync_palw_g14_gates_v1` does) and the fixture's external gain bound `gain`.
pub(super) fn panel_one_config(from: Option<u64>, gain: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (config, mut bundle, premine, floats) = canonical_config(false);
    let mut params = config.params.clone();
    params.palw_kernel_route_panel_one_v1 = from.map(ForkActivation::new);
    bundle.state =
        bundle.state.clone().with_kernel_route_panel_one_from_daa(from).with_test_only_kernel_panel_one_external_gain_bound_v1(gain);
    if let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) = &mut params.palw_consensus_mode {
        *b = bundle.clone();
    }
    (Config::new(params), bundle, premine, floats)
}

/// **The Panel=1 world**: the fence armed from genesis; card 1 registers the class under the relation.
pub(super) async fn panel_one_world() -> World {
    World::on(Net::over_cfg(panel_one_config(Some(0), Some(TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI)), TestConsensus::new)).await
}

#[tokio::test]
async fn g14_f03k_t05_outsider_convicts_before_final_through_a_crowded_court() {
    t05_in(false, false, true).await;
}

#[tokio::test]
async fn g14_f03k_t05_outsider_convicts_after_final_through_a_crowded_court() {
    t05_in(false, true, true).await;
}

#[tokio::test]
async fn g14_f03k_t05_without_the_outsider_nothing_convicts() {
    t05_in(false, false, false).await;
}

#[tokio::test]
async fn g14_f03k2_panel_one_t05_before_final() {
    t05(false, true).await;
}

#[tokio::test]
async fn g14_f03k2_panel_one_t05_after_final() {
    t05(true, true).await;
}

#[tokio::test]
async fn g14_f03k2_panel_one_t05_negative() {
    t05(false, false).await;
}

/// Card 1 sends the fixture's class registration on a fresh net of `cfg` (without `World::on`'s assertion that it registered).
async fn registered(cfg: (Config, PalwConsensusParamsV2, Premine, Premine)) -> Net {
    let mut net = Net::over_cfg(cfg, TestConsensus::new);
    let (fx, d) = (fixture(), k2_tir_v2_descriptor());
    let register = K::RegisterClass {
        descriptor: d.digest(),
        program_bytes: fx.program.encode(),
        plan: fx.plan.clone(),
        param_commitments: fx.pc.clone(),
    };
    let o = net.route(1, &register);
    net.send(vec![(1, o)]).await;
    net
}

/// **The F03k2 relation on the native processor**: past the fence a Panel class is refused when the gain bound is missing
/// (typed-missing) or when saturating the court is cheaper than the gain, and it registers when saturation costs more.
#[tokio::test]
async fn g14_f03k2_panel_one_class_failing_the_censorship_relation_is_refused() {
    kaspa_core::log::try_init_logger("warn");
    // The production bound is the user-approved 500 MSK (2026-10-26); the typed-missing refusal itself is pinned by the pure
    // relation's tests in `palw_kernel_route_panel_one_censorship_v1`.
    assert_eq!(
        kaspa_consensus_core::palw_g14_values_v1::PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_V1,
        Some(kaspa_consensus_core::palw_g14_values_v1::PALW_G14_KERNEL_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI_ESTIMATE_V1),
        "the approved production bound"
    );
    let cheap = registered(panel_one_config(Some(0), Some(u64::MAX / 4))).await;
    assert!(cheap.ledger().classes.is_empty(), "censorship cheaper than the gain: refused");
    let dear = registered(panel_one_config(Some(0), Some(TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI))).await;
    assert_eq!(dear.ledger().classes.len(), 1, "censorship dearer than the gain: registered");
}

/// **Below the fence, byte-identical**: with the fence armed far in the future, the registration folds to the same PALW root
/// whether the relation's input is missing, passing or failing (nothing of the relation runs below the fence); and as on the
/// unarmed network, the class registers. (Arming the fence at all changes the network's hashed params, hence its domain and
/// root; that is the existing fence's, not this relation's, so the unarmed root is not compared.)
#[tokio::test]
async fn g14_f03k2_below_the_fence_registration_is_byte_identical() {
    kaspa_core::log::try_init_logger("warn");
    let unarmed = registered(panel_one_config(None, None)).await;
    assert_eq!(unarmed.ledger().classes.len(), 1, "unarmed: the class registers");
    let later = |gain| registered(panel_one_config(Some(1_000_000), gain));
    let (missing, failing, passing) =
        (later(None).await, later(Some(u64::MAX / 4)).await, later(Some(TEST_ONLY_PANEL_ONE_EXTERNAL_GAIN_BOUND_SOMPI)).await);
    assert_eq!(missing.ledger().classes.len(), 1, "below the fence the class registers as before");
    // Separate nets differ in block timestamps, so compare the kernel route's whole state (rows and aux), not the chain root.
    let route = missing.api();
    assert_eq!(failing.api(), route, "a failing relation changes nothing below the fence");
    assert_eq!(passing.api(), route, "nor a passing one");
}
