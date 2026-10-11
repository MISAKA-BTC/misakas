//! **R8-SLICE / R8-WINDOW / R8-RECOVERY on the real node (lane R8, 2026-10-11): the native root's own Final, its one settlement, every
//! leg's vesting, restart, post-Final public conviction and reorg across the settled root.**
//!
//! Before this module the composed EXEC run stopped at "the root is ready": the native REAL claim never left `Provisional`, because
//! the onboarded class had no ready Panel seat (the probe of this lane: `Voided { reason: NoCapablePanel }` at the anchor). Here the
//! seats prove possession the ordinary way — each card carries a `SeatReadinessProvedV2` whose multiproof opens the span's challenge
//! leaves of the class's public V2 inventory (`cw.ops`, the registered `artifact_root`) under its own bond key — so the node's own
//! draw binds a real Panel at the claim's anchor attempt, the seats sign a Valid quorum, the node assembles and folds the licence, and
//! the claim reaches `Final` through the fold's own clock once every bound kernel computation is Final.
//!
//! **Seams that remain (named, not hidden):** the REAL attempt's admission still rides `exec_v2_test_admit_class_v1` (registry
//! lifecycle / room / seating / share gates, module doc of `exec_slices`) and its trace root is a synthetic lottery commitment; the
//! Panel's seats sign Valid without re-executing (a colluding-Panel fixture, as G14's are); the kernel class's OPV eligibility is the
//! processor's test list. None of these is a Final waiver: Panel binding, licence, the EXEC hold, Final, settlement, vesting, the
//! public conviction and both reorgs run through blocks. This is mechanics evidence for the native settlement path, not soundness of
//! the legacy channel and not an honest-execution claim for the REAL run.
use super::*;
use kaspa_consensus_core::palw_artifact::{artifact_leaf_v1, artifact_root_v1, palw_artifact_multiproof_v1};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, palw_readiness_v2_challenge_seed_v1, palw_readiness_v2_draw_v1, palw_seat_readiness_message_v2,
};
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_work_slice_v2::PalwExecV2RewardFundV1;

/// The root's agreed lifetime: short enough that every leg's release is reachable on this harness, long enough for every kernel Final.
const LIFETIME: u64 = 400;
/// The card that mines the anchor attempt (not the root bond).
const ANCHOR_CARD: usize = 2;

/// A genuine possession proof of the V2 class by card `card` for `span`.
fn readiness(cw: &Cw, card: usize, span: u64) -> Obj {
    let class_id = cw.v2_class;
    let bond = cw.net.bond(card);
    let bond_bytes = borsh::to_vec(&bond).unwrap();
    let leaves: Vec<Hash64> = cw.ops.iter().map(artifact_leaf_v1).collect();
    let draw = palw_readiness_v2_draw_v1(&palw_readiness_v2_challenge_seed_v1(&class_id, &bond_bytes, span), leaves.len() as u32);
    let mut opened: Vec<_> = draw.iter().map(|index| (*index, cw.ops[*index as usize].clone())).collect();
    opened.sort_by_key(|(index, _)| *index);
    let proof = palw_artifact_multiproof_v1(&leaves, &opened).expect("the leaves are the inventory's");
    let message = palw_seat_readiness_message_v2(cw.net.domain, &bond_bytes, &class_id, span, &proof);
    let signature = sign(card, message.as_byte_slice(), PALW_SEAT_READINESS_V2_MLDSA87_CONTEXT, 0x3E);
    Obj::SeatReadinessProvedV2 { bond, class_id, span, proof: Box::new(proof), signature }
}

/// **The native Panel of the REAL claim, through blocks**: every card but the root bond proves possession of the class a few DAA
/// before the claim's anchor slot; the first attempt at the slot anchors the node's own draw; the drawn seats sign Valid for their
/// claim; the node assembles the quorum and a lifecycle carrier lands it — the claim is `ReceiptLicensed`.
async fn panel_licence(cw: &mut Cw, native: Hash64) {
    let leaves: Vec<Hash64> = cw.ops.iter().map(artifact_leaf_v1).collect();
    assert_eq!(
        cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().artifact_root,
        artifact_root_v1(&leaves).unwrap(),
        "the seats open the class's registered inventory, nothing synthetic"
    );
    let slot = cw.net.chain.tip_state().1.claim(&native).unwrap().bind_base_daa() + cw.net.chain.bundle.panel.anchor_delay();
    assert!(cw.net.daa() + 3 <= slot, "the readiness proofs land before the anchor slot");
    cw.net.beat_to(slot - 3).await;
    // WIP (unverified fix): the first run refused span = tip + 2 at the template ("names span 224 at span 222"); the gate takes the
    // next span and the fold a past span within 40.
    let span = cw.net.daa() + 1;
    let mut txs = Vec::new();
    for card in (0..cw.net.chain.bonds.len()).filter(|card| *card != ROOT_CARD) {
        let object = readiness(cw, card, span);
        txs.push(cw.net.carrier(card, &object));
    }
    let ttpb = cw.net.ttpb();
    let carrying = cw.net.chain.heartbeat(ttpb, txs.clone()).await;
    assert_eq!(carrying.transactions.iter().filter(|t| txs.iter().any(|x| x.id() == t.id())).count(), txs.len());
    cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    let state = cw.net.chain.tip_state().1;
    let ready = (0..cw.net.chain.bonds.len()).filter(|card| state.seat_readiness(&cw.net.bond(*card), &cw.v2_class).is_some()).count();
    eprintln!("[r8] possession proofs folded: {ready} of {}", txs.len());
    assert_eq!(ready, txs.len(), "every proof folded through the block");
    cw.net.beat_to(slot).await;
    assert!(matches!(cw.net.chain.tip_state().1.claim(&native).unwrap().phase, PalwClaimPhaseV2::Provisional));
    cw.net.chain.attempt(ANCHOR_CARD, ttpb, Vec::new(), &|_| true).await;
    let state = cw.net.chain.tip_state().1;
    let phase = state.claim(&native).unwrap().phase.clone();
    let panel = state.panel(&native).unwrap_or_else(|| panic!("the node's draw binds a real Panel at the anchor: {phase:?}")).clone();
    assert!(matches!(phase, PalwClaimPhaseV2::PanelBound { .. }), "{phase:?}");
    let signed_daa = cw.net.chain.daa_of(cw.net.chain.sink());
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        cw.net.chain.config.params.net.to_string().as_bytes(),
        Some(cw.net.chain.config.params.genesis.hash),
    );
    let receipts: Vec<_> = panel
        .seats
        .iter()
        .take(cw.net.chain.bundle.panel.quorum() as usize)
        .map(|seat| {
            let signer = cw.net.chain.bonds.iter().position(|b| *b == seat.bond).expect("a genesis card");
            let message = palw_receipt_message_v2(domain, native, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = sign(signer, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT, 0x21);
            PalwSeatReceiptV2 { claim: native, verdict: PalwReceiptVerdictV2::Valid, seat_bond: seat.bond, signed_daa, signature }
        })
        .collect();
    let object = cw.net.chain.vp().palw_v2_receipt_quorum_assemble_impl(native, &receipts).expect("the node verifies the signed quorum");
    let tx = cw.net.carrier(ANCHOR_CARD, &object);
    cw.net.chain.heartbeat(ttpb, vec![tx]).await;
    for _ in 0..8 {
        if matches!(cw.net.chain.tip_state().1.claim(&native).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
            break;
        }
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    let phase = cw.net.chain.tip_state().1.claim(&native).unwrap().phase.clone();
    assert!(matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the Panel's quorum licenses the claim: {phase:?}");
    assert!(cw.net.chain.tip_state().1.exec_v2_holds_final_v1(&native), "the unready root holds the licensed claim's Final");
}

/// Beat until the native claim is `Final` (the fold's own deadline), at most `cap` blocks.
async fn until_native_final(cw: &mut Cw, native: Hash64, cap: u64) -> u64 {
    let ttpb = cw.net.ttpb();
    for _ in 0..cap {
        if let PalwClaimPhaseV2::Final { final_daa } = cw.net.chain.tip_state().1.claim(&native).unwrap().phase {
            return final_daa;
        }
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
    }
    panic!("the native claim did not reach Final: {:?}", cw.net.chain.tip_state().1.claim(&native).unwrap().phase);
}

/// The session carried through native Final: the REAL claim licensed by its Panel, every slice carried, every kernel computation
/// Final, then the native claim's own Final. Returns the session and the native Final DAA.
async fn settled(cw: &mut Cw, lie_prefix: bool) -> (Session, u64) {
    let (s, final_daa, _, _) = settled_marking_ready(cw, lie_prefix).await;
    (s, final_daa)
}

/// [`settled`], also returning the selected tip at which the root became ready (licensed, every kernel computation Final, the
/// native claim not yet Final) — a pruning point inside a live composite session.
async fn settled_marking_ready(cw: &mut Cw, lie_prefix: bool) -> (Session, u64, BlockHash, u64) {
    let mut s = opened_with_lifetime(cw, lie_prefix, |_| false, |_, _, _| Vec::new(), LIFETIME).await;
    let native = s.real.claim;
    for index in 0..3u32 {
        s.lane.push(carry_slice(cw, native, index, &s.slices[index as usize].claim).await);
    }
    panel_licence(cw, native).await;
    let api = cw.net.api().unwrap();
    let floor = s
        .slices
        .iter()
        .chain(std::iter::once(&s.prefix))
        .map(|k| api.claim_read_v1(&k.claim.id).unwrap().unwrap().opv.unwrap().final_floor_daa)
        .max()
        .unwrap();
    assert!(floor + 1 < cw.net.chain.tip_state().1.exec_v2_root_v1(&native).unwrap().expiry_daa, "kernel Finals fit the lifetime");
    cw.net.beat_to(floor + 1).await;
    assert!(cw.net.chain.tip_state().1.exec_v2_root_v1(&native).unwrap().ready_for_final(), "every bound computation is Final");
    let (ready, poster_at_ready) = (cw.net.chain.sink(), cw.net.collateral(REGISTRANT));
    assert!(!cw.net.chain.tip_state().1.claim(&native).unwrap().phase.is_terminal(), "ready, not yet native Final");
    let final_daa = until_native_final(cw, native, 2_000).await;
    (s, final_daa, ready, poster_at_ready)
}

/// The money of a settled root as the node reads it.
#[derive(Debug, PartialEq, Eq)]
struct Settlement {
    native_vesting: u128,
    extra_legs: Vec<((PalwBondKeyV2, PalwExecV2RewardFundV1), u64)>,
    liability_until: u64,
    poster_collateral: u64,
}

fn settlement_of(cw: &Cw, native: &Hash64) -> Settlement {
    let state = cw.net.chain.tip_state().1;
    let row = state.exec_v2_reward_vesting_v1(native).expect("the composite receipt");
    Settlement {
        native_vesting: state.vesting_row(native).expect("the forced native vesting row").total_sompi_u128(),
        extra_legs: row.legs.iter().map(|(key, leg)| (*key, leg.amount)).collect(),
        liability_until: row.liability_until,
        poster_collateral: cw.net.collateral(REGISTRANT),
    }
}

/// **The native root Final and its one settlement on the real node; restart; every leg released once.** The REAL claim's own Final
/// (no waiver: Panel, licence, EXEC hold and deadline through blocks) settles the root once: the root is `Settled`; each bound kernel
/// claim's escrow is debited exactly once from the poster's actual collateral (Σ debits = Σ `KernelFee` legs), the rest released;
/// every executor's work share is a `NativeSlice` leg; nothing is paid before the frozen horizon; the reward hold rows are gone.
/// More blocks change nothing (one settlement). The node restarts over the same database and reads the same rows before any new
/// block. Past the inclusive horizon every native and additional leg moves to the payout queue once (created = moved), and a second
/// node replaying the chain agrees.
#[tokio::test]
async fn x8_r8_native_root_final_settles_once_vests_every_leg_and_survives_restart() {
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, receiver) = async_channel::unbounded();
    let mut cw =
        active_world_over(VerificationModeV1::OptimisticPublicVerification, |c| TestConsensus::with_db(db.clone(), c, sender)).await;
    cw.net._keep.push(Box::new(receiver));
    let (s, final_daa, _, poster_before) = settled_marking_ready(&mut cw, false).await;
    let native = s.real.claim;
    let state = cw.net.chain.tip_state().1;
    let root = state.exec_v2_root_v1(&native).unwrap();
    assert!(matches!(root.phase, PalwWorkRootPhaseV2::Settled { settled_daa } if settled_daa == final_daa), "{:?}", root.phase);
    let money = settlement_of(&cw, &native);
    eprintln!("[r8] native Final at {final_daa}: {money:?}");
    let fees: u64 = money.extra_legs.iter().filter(|((_, fund), _)| *fund == PalwExecV2RewardFundV1::KernelFee).map(|(_, a)| *a).sum();
    assert_eq!(poster_before - money.poster_collateral, fees, "the poster is debited exactly the actual fees, once");
    assert!(fees > 0, "the bound computations' fees are paid inside the executor shares");
    for card in [EXECUTORS[0], EXECUTORS[1]] {
        assert!(
            money.extra_legs.iter().any(|((bond, _), a)| *bond == cw.net.bond(card) && *a > 0),
            "executor card {card} holds a funded additional leg (its work share and/or its actual fee)"
        );
    }
    let ledger = cw.net.ledger();
    for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert!(!ledger.job_escrows.contains_key(&k.job.id()), "the escrow is spent or released");
        assert!(!ledger.exec_reward_holds.contains_key(&k.claim.id), "the root's hold is closed");
    }
    assert!(money.liability_until > final_daa, "the legs wait for the frozen consumer horizon");
    assert!(
        !state.pending_payouts_iter().any(|(_, p)| money.extra_legs.iter().any(|((bond, _), _)| {
            state.bond(bond).is_some_and(|b| b.payout_payload == p.payload)
        })),
        "no executor leg is paid before the horizon"
    );
    // One settlement: more blocks change nothing.
    cw.net.beat_to(cw.net.daa() + 3).await;
    assert_eq!(settlement_of(&cw, &native), money, "a settled root never settles again");

    // Restart over the same database; read before any new block.
    let sink = cw.net.chain.sink();
    let root_hash = cw.net.chain.tip_state().1.state_root();
    let utxos = Net::utxos_of(&cw.net.chain);
    cw.net = cw.net.restart(db.clone());
    assert_eq!(cw.net.chain.sink(), sink);
    assert_eq!(cw.net.chain.tip_state().1.state_root(), root_hash);
    assert_eq!(Net::utxos_of(&cw.net.chain), utxos, "the complete UTXO set loads from the same database");
    assert_eq!(settlement_of(&cw, &native), money, "the settled receipts reopen unchanged");

    // Every leg is released once past the inclusive horizon (bounded by the release queue's width).
    let created = cw.net.chain.tip_state().1.vesting_counters().created;
    cw.net.beat_to(money.liability_until).await;
    let at_last = cw.net.chain.tip_state().1;
    assert!(at_last.vesting_row(&native).is_some() && at_last.exec_v2_reward_vesting_v1(&native).is_some(), "the last day still holds");
    let ttpb = cw.net.ttpb();
    for _ in 0..64 {
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
        let now = cw.net.chain.tip_state().1;
        if now.vesting_row(&native).is_none() && now.exec_v2_reward_vesting_v1(&native).is_none() {
            break;
        }
    }
    let released = cw.net.chain.tip_state().1;
    assert!(released.vesting_row(&native).is_none() && released.exec_v2_reward_vesting_v1(&native).is_none(), "every leg left vesting");
    let counters = released.vesting_counters();
    assert_eq!(counters.burned, 0);
    assert_eq!(counters.moved, created, "created = moved: every native and additional leg released exactly once");
    let z = replay_with_lane(&cw.net, &s.lane).await;
    cw.net.assert_same(&z, "native root Final, settlement, restart and release replay");
}

/// **A public conviction after the native root's Final retracts the settled root, and two actual reorgs move across it.** The prefix
/// claim commits a false matrix value and is unrefuted through its computation Final; the native claim then reaches Final and
/// settles. On branch B an outsider — from the read API and the producer's published material — convicts the prefix: the root and the
/// native claim are voided (`WorkSliceProvenFalse`), the root's exact safe-weight contribution leaves, every unreleased native and
/// additional leg is burned once (payment recovered: nothing of it reaches the queue), the root producer's collateral is slashed and
/// the accuser's bounty queued. The node reorgs from A (settled) to B (retracted) and back to A: the settlement, the vesting rows and
/// the safe weight return exactly; the slash and bounty are undone once.
#[tokio::test]
async fn x8_r8_a_public_conviction_after_native_root_final_retracts_every_leg_and_two_reorgs_restore_it() {
    let mut cw = active_world().await;
    let (s, _) = settled(&mut cw, true).await;
    let native = s.real.claim;
    let settled_state = cw.net.chain.tip_state().1;
    let money = settlement_of(&cw, &native);
    let settled_sink = cw.net.chain.sink();
    let root_money = (cw.net.collateral(ROOT_CARD), cw.net.slashed(ROOT_CARD), cw.net.owed(ACCUSER));
    let burned_before = settled_state.vesting_counters().burned;
    let fork = settled_sink;
    let fork_daa = cw.net.chain.daa_of(fork);
    let quiet = cw.net.chain.heartbeat(cw.net.ttpb(), Vec::new()).await;

    // Branch B: from the settled tip, a fresh outsider convicts the prefix.
    let b = t12_genesis_chain(&cw.net.config, &cw.net.bundle, &cw.net.premine, &cw.net.floats);
    let up_to_fork = chain_blocks(&cw.net.chain, fork);
    let fork_timestamp = up_to_fork.last().unwrap().header.timestamp;
    for block in up_to_fork {
        let hash = block.header.hash;
        arrive(&b, block, "the shared prefix through native Final").await;
        for (_, lane) in s.lane.iter().filter(|(under, _)| *under == hash) {
            arrive(&b, lane.clone(), "the shared anchored EXEC lane").await;
        }
    }
    assert_eq!(b.tip_state().1.state_root(), settled_state.state_root());
    let mut bn = cw.net.on_chain(b);
    bn.chain.ctx.simulated_time = fork_timestamp;
    let api = bn.api().unwrap();
    let fresh = Fresh::from_api(&api, api.ledger_root(), 0x7F);
    let OutsiderFindingV1::Prosecute(proof) = fresh.check(s.prefix.claim.id, &s.prefix.claim.published(&cw.cand, &[]), &cw.cand.params)
    else {
        panic!("the fresh outsider proves the false prefix after the native root's Final")
    };
    let object = bn.route(ACCUSER, &K::FileProof { accuser: bn.kid(ACCUSER), claim: s.prefix.claim.id, proof });
    bn.send(vec![(ACCUSER, object)]).await;
    let retracted = bn.chain.tip_state().1;
    assert!(bn.ledger().claims[&s.prefix.claim.id].convicted, "the public proof convicts the prefix");
    assert!(
        matches!(retracted.claim(&native).unwrap().phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::WorkSliceProvenFalse, .. }),
        "{:?}",
        retracted.claim(&native).unwrap().phase
    );
    assert!(matches!(retracted.exec_v2_root_v1(&native).unwrap().phase, PalwWorkRootPhaseV2::Voided { .. }));
    assert!(retracted.safe_weight() < settled_state.safe_weight(), "the root's Final contribution leaves the safe weight");
    assert!(retracted.vesting_row(&native).is_none() && retracted.exec_v2_reward_vesting_v1(&native).is_none());
    let extra: u128 = money.extra_legs.iter().map(|(_, a)| u128::from(*a)).sum();
    assert_eq!(retracted.vesting_counters().burned - burned_before, money.native_vesting + extra, "every unreleased leg burned once");
    assert!(bn.collateral(ROOT_CARD) < root_money.0, "the prefix producer's collateral answers for the lie");
    assert!(bn.owed(ACCUSER) > root_money.2, "the accuser's bounty is queued");
    assert!(bn.daa() - fork_daa <= kaspa_consensus_core::palw_fork_authority_v2::PALW_REORG_SHALLOW_TIE_DAA_V1);

    // A reorgs to B.
    for block in chain_blocks(&bn.chain, bn.chain.sink()).into_iter().skip(chain_blocks(&bn.chain, fork).len()) {
        arrive(&cw.net.chain, block, "the post-Final conviction branch").await;
    }
    assert_eq!(cw.net.chain.sink(), bn.chain.sink(), "the node reorgs onto the retraction");
    arrive(&bn.chain, quiet.clone(), "A's quiet sibling for a complete virtual comparison").await;
    assert_eq!(cw.net.chain.ctx.consensus.get_virtual_parents(), bn.chain.ctx.consensus.get_virtual_parents());
    assert_eq!(Net::utxos_of(&cw.net.chain), Net::utxos_of(&bn.chain));
    cw.net.assert_same(&bn.chain, "after the retraction reorg");

    // And back to A: a fresh replay of A grows past B.
    let a = t12_genesis_chain(&cw.net.config, &cw.net.bundle, &cw.net.premine, &cw.net.floats);
    for block in chain_blocks(&cw.net.chain, settled_sink) {
        let hash = block.header.hash;
        arrive(&a, block, "the settled branch").await;
        for (_, lane) in s.lane.iter().filter(|(under, _)| *under == hash) {
            arrive(&a, lane.clone(), "the settled branch's lane").await;
        }
    }
    let mut an = cw.net.on_chain(a);
    an.chain.ctx.simulated_time = chain_blocks(&an.chain, settled_sink).last().unwrap().header.timestamp;
    let target = chain_blocks(&bn.chain, bn.chain.sink()).len();
    while chain_blocks(&an.chain, an.chain.sink()).len() <= target {
        an.chain.heartbeat(an.ttpb(), Vec::new()).await;
    }
    for block in chain_blocks(&an.chain, an.chain.sink()).into_iter().skip(chain_blocks(&an.chain, settled_sink).len()) {
        arrive(&cw.net.chain, block, "the settled branch returning").await;
    }
    assert_eq!(cw.net.chain.sink(), an.chain.sink(), "a second actual reorg returns to the settled branch");
    let restored = cw.net.chain.tip_state().1;
    assert_eq!(restored.exec_v2_root_v1(&native), settled_state.exec_v2_root_v1(&native));
    assert_eq!(restored.claim(&native).map(|c| c.phase.clone()), settled_state.claim(&native).map(|c| c.phase.clone()));
    assert_eq!(restored.safe_weight(), settled_state.safe_weight(), "the safe weight returns exactly");
    assert_eq!(restored.vesting_counters().burned, burned_before, "the burn is undone once");
    assert_eq!(settlement_of(&cw, &native), money, "the settlement returns exactly");
    assert_eq!(
        (cw.net.collateral(ROOT_CARD), cw.net.slashed(ROOT_CARD), cw.net.owed(ACCUSER)),
        root_money,
        "the slash and the bounty are undone once"
    );
    for block in chain_blocks(&bn.chain, bn.chain.sink()).into_iter().skip(chain_blocks(&bn.chain, fork).len()) {
        arrive(&an.chain, block, "the retraction branch retained as DAG knowledge").await;
    }
    arrive(&an.chain, quiet, "the quiet sibling retained").await;
    assert_eq!(cw.net.chain.ctx.consensus.get_virtual_parents(), an.chain.ctx.consensus.get_virtual_parents());
    assert_eq!(Net::utxos_of(&cw.net.chain), Net::utxos_of(&an.chain));
    cw.net.assert_same(&an.chain, "back on the settled branch");
}

/// The conformance world of `exec_slices` with the bond budget (ADR-0176) also armed at DAA 1 under a TEST policy (not a proposal:
/// generous rates, `market_fee` either way) — armed WITHOUT its validation, as every fence of this harness.
async fn active_world_budgeted(market_fee: kaspa_consensus_core::palw_bond_budget_v1::PalwMarketFeePolicyV1) -> Cw {
    use kaspa_consensus_core::palw_bond_budget_v1::*;
    kaspa_core::log::try_init_logger("warn");
    let policy = PalwBondBudgetPolicyV1 {
        version: PALW_BOND_BUDGET_POLICY_VERSION_V2,
        window_daa: 300,
        capital_unit_sompi: 1,
        rho: 1,
        claims_per_unit: 1,
        block_units_per_unit: PALW_BUDGET_BLOCK_UNIT_V1,
        reward_per_unit_sompi: 1,
        final_weight_per_unit: u64::MAX / 4,
        max_open_claims_per_bond: 1_000,
        slice_rights_by_rho: false,
        round_rights: PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 },
        liability_hold_daa: 7,
        export_cap_permille: PALW_BOND_BUDGET_EXPORT_CAP_MAX_PERMILLE_V1,
        weight_value: PalwWeightValuePolicyV1::Unknown,
        market_fee,
    };
    let mut cw = Cw::over_with_floor_and_kernel_mode(
        SEED,
        |params| {
            params.palw_exec_payload_v2 = Some(ForkActivation::new(EXEC_FENCE));
            params.sync_palw_exec_payload_v2();
            params.palw_bond_budget_v1 = Some(PalwBondBudgetFenceV1 { activation: ForkActivation::new(1), policy });
            params.sync_palw_bond_budget_v1();
            assert!(params.validate_palw_v2().is_err(), "validation refuses the fences: the harness arms past them");
        },
        TestConsensus::new,
        0,
        VerificationModeV1::OptimisticPublicVerification,
    )
    .await;
    cw.commit(0x22).await;
    cw.source_claims(2).await;
    cw.until_locked().await;
    let post = cw.post(|_, _| {});
    cw.send_post(post, Some(1024)).await;
    let posted = cw.attempt().evidence.expect("the fold accepted the evidence");
    cw.net.beat_to(posted.window_end_daa + 1).await;
    let binding = cw.net.api().unwrap().artifact_binding_v1(&cw.v2_class, &cw.kernel_root).unwrap();
    if cw.net.daa() < binding.final_daa {
        cw.net.beat_to(binding.final_daa + 1).await;
    }
    assert!(matches!(cw.net.chain.tip_state().1.class(&cw.v2_class).unwrap().status, PalwClassStatusV2::Active));
    assert!(cw.net.chain.tip_state().1.bond_budget().is_some(), "the budget engine is live");
    cw
}

/// **R8-BUDGET on the native root's settlement (hook H-1 × EXEC, both market-fee policies)**: under the bond budget every bound
/// kernel claim (prefix and slices) reserved its job escrow in its own producer's `R` where the fee counts (nothing where it does
/// not); computation Final consumed nothing (the root still held the claim); the native root's one settlement paid each actual fee
/// as a `KernelFee` leg AND drew exactly that amount from the paid claim's own row, then closed every bound claim. So, per executor,
/// Σ consumed `R` = its `KernelFee` leg (or 0 outside `R`), and no row stays open.
#[tokio::test]
async fn x8_r8_the_native_settlement_draws_each_bound_fee_from_its_own_claims_budget_under_both_market_fee_policies() {
    use kaspa_consensus_core::palw_bond_budget_v1::{PalwBudgetOriginV1, PalwMarketFeePolicyV1};
    for market_fee in [PalwMarketFeePolicyV1::OutsideReward, PalwMarketFeePolicyV1::CountAgainstReward] {
        let mut cw = active_world_budgeted(market_fee).await;
        let (s, _) = settled(&mut cw, false).await;
        let native = s.real.claim;
        let money = settlement_of(&cw, &native);
        let state = cw.net.chain.tip_state().1;
        let budget = state.bond_budget().expect("the engine");
        let mut drawn: std::collections::BTreeMap<PalwBondKeyV2, u64> = Default::default();
        for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
            let row = budget.claim_row(&Hash64::from_bytes(k.claim.id)).expect("every bound kernel claim reserved");
            assert_eq!(row.origin, PalwBudgetOriginV1::KernelRoute);
            assert!(!row.open, "{market_fee:?}: the root's settlement closed its bound claim");
            if market_fee == PalwMarketFeePolicyV1::OutsideReward {
                assert_eq!(row.reserved.reward_sompi, 0);
            }
            *drawn.entry(row.bond).or_default() += row.consumed.reward_sompi;
        }
        for ((bond, fund), amount) in &money.extra_legs {
            if *fund != PalwExecV2RewardFundV1::KernelFee {
                continue;
            }
            let expected = if market_fee == PalwMarketFeePolicyV1::CountAgainstReward { *amount } else { 0 };
            assert_eq!(drawn.get(bond).copied().unwrap_or(0), expected, "{market_fee:?}: {bond:?}'s actual fee drew its own R");
        }
        eprintln!("[r8] {market_fee:?}: per-bond R drawn {drawn:?}; legs {:?}", money.extra_legs);
    }
}

/// **A pruned join inside a live composite session folds through the native root's Final, its settlement and its release.** The
/// pruning point P is the tip at which the root became ready (the native claim licensed and held, every bound kernel computation
/// Final, the escrows and reward holds live). The importer follows A's blocks and lanes through P, is left as a pruned join leaves a
/// node (no PALW tip, no delta at or below P), installs the carriage A serves for P, and then folds every later block — the native
/// Final, the one settlement and every leg's release — to A's state root, block for block.
#[tokio::test]
async fn x8_r8_a_pruned_join_inside_a_live_session_folds_through_native_final_settlement_and_release() {
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let mut cw = active_world().await;
    let (s, _, p, _) = settled_marking_ready(&mut cw, false).await;
    let native = s.real.claim;
    let money = settlement_of(&cw, &native);
    cw.net.beat_to(money.liability_until).await;
    let ttpb = cw.net.ttpb();
    for _ in 0..64 {
        cw.net.chain.heartbeat(ttpb, Vec::new()).await;
        if cw.net.chain.tip_state().1.exec_v2_reward_vesting_v1(&native).is_none() {
            break;
        }
    }
    assert!(cw.net.chain.tip_state().1.exec_v2_reward_vesting_v1(&native).is_none(), "every leg released on A");

    let all = chain_blocks(&cw.net.chain, cw.net.chain.sink());
    let k = all.iter().position(|b| b.header.hash == p).expect("P is on the chain");
    let t = all[k + 1].clone();
    let importer = t12_genesis_chain(&cw.net.config, &cw.net.bundle, &cw.net.premine, &cw.net.floats);
    for b in &all[..=k] {
        let hash = b.header.hash;
        arrive(&importer, b.clone(), "a block through the pruning point").await;
        for (_, lane) in s.lane.iter().filter(|(under, _)| *under == hash) {
            arrive(&importer, lane.clone(), "a lane block through the pruning point").await;
        }
    }
    arrive(&importer, Block::from_header_arc(t.header.clone()), "T's header").await;
    let vp = cw.net.chain.vp();
    vp.capture_pruning_point_palw_state(p);
    let wire = borsh::to_vec(&vp.pruning_point_palw_state(p).expect("servable")).expect("serializes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).expect("the wire bytes decode");
    {
        let ivp = importer.vp();
        let mut store = ivp.palw_state_v2_store.write();
        store.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(importer.config.params.genesis.hash).chain(all[..=k].iter().map(|b| b.header.hash)) {
            store.delete_delta_for_tests(blk).expect("no delta row at or below the pruning point");
        }
    }
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("the served carriage installs against T's committed root");
    let (at, imported) = importer.tip_state();
    assert_eq!(at, p);
    assert_eq!(imported.state_root(), root_at(&cw.net.chain, p), "the imported state is P's, root for root");
    let root = imported.exec_v2_root_v1(&native).expect("the live root came through the carriage");
    assert!(root.ready_for_final() && !imported.claim(&native).unwrap().phase.is_terminal());
    let route = imported.kernel_route().expect("the route came through the carriage");
    let ledger = route.ledger().unwrap();
    for k in s.slices.iter().chain(std::iter::once(&s.prefix)) {
        assert!(ledger.exec_reward_holds.contains_key(&k.claim.id), "the reward holds are live at P");
        assert!(ledger.job_escrows.contains_key(&k.job.id()), "and the escrows");
    }
    for blk in &all[k + 1..] {
        arrive(&importer, blk.clone(), "A's block after P").await;
        assert_eq!(
            importer.tip_state().1.state_root(),
            root_at(&cw.net.chain, blk.header.hash),
            "the importer folds to A's root at {}",
            blk.header.hash
        );
    }
    assert_eq!(importer.sink(), cw.net.chain.sink());
    assert_eq!(importer.ctx.consensus.palw_kernel_route_v1(), cw.net.api(), "the same route, rows and aux");
    let end = importer.tip_state().1;
    assert_eq!(end.vesting_counters(), cw.net.chain.tip_state().1.vesting_counters());
    assert!(end.exec_v2_reward_vesting_v1(&native).is_none(), "the importer released every leg exactly as A did");
}
