//! **LIVE-R1 (2026-10-08): a partition heals on the wire and the chain stays split.** H1's devnet r1
//! (testnet-12's release fences on a salted drill chain) reproduced through the real pipeline.
//!
//! What the devnet showed: a 41-minute partition, minority {B, D6} and majority {A, C, D1–D5}. After
//! the rejoin B refused every heavier majority candidate on every resolve (`DNS reorg gate: virtual
//! settled on sink … after refusing the heavier candidate …, reason DominanceViolation`, 87 times),
//! and A never asked for B's branch. The `DominanceViolation` is NOT the DNS gate: on a V2 network a
//! non-extension candidate goes to `dns_reorg_outcome`'s V2 arm first, and past
//! `palw_reorg_strict_economic_win` that arm refuses a challenger that is not STRICTLY ahead on
//! `(safe frontier, safe weight, live total)`. B's capacity-shadow line put the minority's
//! `bounded_immature` at 92–93 against the majority's 67–69 (no `Final` on either side yet): the
//! minority carried licences of pre-fork claims — receipts its own two seats signed — that the
//! majority's branch did not. Past F-W a licence is 1000‰ of a claim's weight and a `Created` attempt
//! is 0, so the majority's many fresh attempts bought blue work and no economic weight.
//!
//! The two orders a node consults disagree, and each side keeps its own:
//! * the minority pops the majority's tip first (GHOSTDAG's heap, blue work) and the V2 arm refuses it
//!   — a strict economic LOSS, so the depth of the reorg does not matter;
//! * the majority never weighs the minority at all: the heap pops its own, heavier tip, which is an
//!   extension, and the V2 arm is asked about reorgs only. (On the wire it does not even fetch it: the
//!   relay flow skips a block below the virtual's merge-depth root.)
//!
//! This is the shape below: card 2's claim is bound before the fork (shared history); the minority
//! carries the quorum's receipts for it (a licence carrier that reached only its side); the majority
//! mints two attempts and races two heartbeat producers every slot. Both run testnet-12's armed
//! fork-choice set as t12 runs it past DAA 1,700 (strict-win, lane A, F-W).
use super::*;
use kaspa_consensus_core::palw_fork_authority_v2::{PalwDeepReorgV2, palw_reorg_strict_economic_win_v1};
use kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1;
use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
use kaspa_hashes::Hash64;

/// The partition, built and not yet healed.
struct Partition {
    d: Duel,
    /// The last shared block.
    fork: BlockHash,
    /// Card 2's claim: bound before the fork, licensed on the minority's branch only.
    claim_id: Hash64,
    /// Every block the minority (`d.victim`) made above the fork, parents first.
    minority_blocks: Vec<Block>,
    /// Every block the majority (`d.attacker`) made above the fork, parents first.
    majority_blocks: Vec<Block>,
}

/// The quorum's Valid receipts for `claim_id`, assembled into a lifecycle carrier spending card 0's fee
/// float — `capacity_probe_w_t6b_…`'s carrier. Signed at `node`'s virtual DAA.
fn licence_carrier(d: &Duel, node: &T12Chain, claim_id: Hash64) -> Transaction {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        d.config.params.net.to_string().as_bytes(),
        Some(d.config.params.genesis.hash),
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
                &crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
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
}

/// The PALW keys `c` reads for `tip`.
fn keys(c: &T12Chain, tip: BlockHash) -> Option<PalwCandidateOrderV1> {
    c.vp().palw_candidate_order_v2(tip)
}

/// Shared history, then `slots` partitioned slots on each side (the same number of DAA ticks, so no side
/// runs ahead of the other's clock).
async fn partition(tag: &str, slots: usize) -> Partition {
    let mut d = duel_capacity_armed(Some(1), Some(1));
    // ---- shared history: card 2's claim, bound at its slot by card 7 (an operator) ----------------
    for _ in 0..3 {
        honest_slot(&mut d.victim, Vec::new()).await;
    }
    let (_, claim_id) = d.victim.attempt(2, 1_000, Vec::new(), &|_| true).await;
    d.victim.attempt_at_the_anchor_slot(claim_id, 7).await;
    for b in blocks_in_topological_order(&d.victim) {
        mirror(&mut d.attacker, &b).await;
    }
    assert_eq!(d.attacker.sink(), d.victim.sink(), "{tag}: both nodes hold the shared history");
    let fork = d.victim.sink();

    // ---- the minority: one heartbeat producer; the licence carrier reached only this side --------
    let carrier = licence_carrier(&d, &d.victim, claim_id);
    let mut minority_blocks = private_slot(&mut d.victim, &mut d.nonce, 1, vec![carrier]).await;
    for _ in 1..slots {
        minority_blocks.extend(honest_slot(&mut d.victim, Vec::new()).await);
    }
    let (_, state) = d.victim.tip_state();
    assert!(
        matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "{tag}: licensed on the minority's branch"
    );

    // ---- the majority: two producers mint attempts and race every heartbeat slot ------------------
    let mut majority_blocks = Vec::new();
    for card in [3usize, 4] {
        majority_blocks.push(d.attacker.attempt(card, 1_000, Vec::new(), &|_| true).await.0);
    }
    for _ in 0..slots {
        majority_blocks.extend(private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await);
    }
    let (_, state) = d.attacker.tip_state();
    assert!(
        matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
        "{tag}: still only bound on the majority's branch"
    );
    assert_eq!(d.victim.daa_of(d.victim.sink()), d.attacker.daa_of(d.attacker.sink()), "{tag}: one clock, the same ticks");
    Partition { d, fork, claim_id, minority_blocks, majority_blocks }
}

/// Feed each side the other's blocks.
async fn heal(p: &mut Partition) {
    for b in &p.majority_blocks {
        mirror(&mut p.d.victim, b).await;
    }
    for b in &p.minority_blocks {
        mirror(&mut p.d.attacker, b).await;
    }
}

/// **The defect, pinned.** After the heal both nodes hold one DAG and keep two sinks: the minority
/// refuses the majority's heavier tip on a strict economic loss (its own licence is weight the majority
/// lacks), and the majority never weighs the minority. More slots on both sides, exchanged, change
/// nothing: neither the economic order nor the blue-work order moves.
///
/// The keys are asserted as the devnet measured them: an equal `(frontier, safe)` (nothing `Final`), the
/// minority ahead on `live`, the majority ahead on blue work — and `palw_reorg_strict_economic_win_v1`
/// refuses whatever the shallow-tie question would answer, so the depth of the partition is not what
/// keeps it apart.
#[tokio::test]
async fn live_r1_a_minority_holding_a_licence_refuses_the_heavier_majority_after_the_heal() {
    kaspa_core::log::try_init_logger("warn");
    let tag = "live-r1 partition";
    let mut p = partition(tag, 8).await;
    let (minority_tip, majority_tip) = (p.d.victim.sink(), p.d.attacker.sink());
    let fork_bw = bw(&p.d.victim, p.fork);
    let minority_keys = keys(&p.d.victim, minority_tip).expect("the minority weighs its own tip");
    let majority_keys = keys(&p.d.attacker, majority_tip).expect("the majority weighs its own tip");

    heal(&mut p).await;
    let d = &mut p.d;
    let (minority_bw, majority_bw) = (bw(&d.victim, minority_tip) - fork_bw, bw(&d.victim, majority_tip) - fork_bw);
    let seen = keys(&d.victim, majority_tip).expect("the minority weighs the majority's tip: its sink search UTXO-validated it");
    eprintln!(
        "[{tag}] healed: minority tip {minority_tip} (+{minority_bw} blue work) keys {minority_keys:?}; majority tip {majority_tip} \
         (+{majority_bw}) keys {majority_keys:?}, as the minority reads it {seen:?}; sinks: minority {} / majority {}",
        d.victim.sink(),
        d.attacker.sink()
    );
    assert_eq!(seen, majority_keys, "{tag}: one fold, one answer on both nodes");
    assert!(majority_bw > minority_bw, "{tag}: the majority is heavier on blue work — it tops the minority's heap");
    assert_eq!(
        (minority_keys.safe_frontier_blue_score, minority_keys.safe_weight),
        (majority_keys.safe_frontier_blue_score, majority_keys.safe_weight),
        "{tag}: nothing is Final on either side"
    );
    assert!(minority_keys.live_total > majority_keys.live_total, "{tag}: the minority's licence is live weight the majority lacks");
    assert_eq!(
        palw_reorg_strict_economic_win_v1(&minority_keys, &majority_keys, || true),
        PalwDeepReorgV2::Refuse,
        "{tag}: a strict economic loss is refused whatever the depth"
    );
    assert_eq!(d.victim.sink(), minority_tip, "{tag}: the minority keeps its own tip");
    assert_eq!(d.attacker.sink(), majority_tip, "{tag}: the majority keeps its own tip");
    // The wedge warning names the rule and the keys (node-local; devnet r1 printed only the DNS word).
    let explained = d.victim.vp().palw_reorg_refusal_explained_v1(minority_tip, majority_tip);
    eprintln!("[{tag}] the minority's wedge warning now adds: {explained}");
    assert!(explained.contains("not DNS") && explained.contains("strict economic LOSS"), "{tag}: {explained}");
    assert!(d.victim.vp().palw_reorg_refusal_explained_v1(minority_tip, minority_tip).is_empty(), "{tag}: silent for an extension");

    // N2's consensus half: the minority records the refusal run (a validated, weighed, heavier candidate
    // refused on the PALW rule); the majority records nothing.
    let refusal = d.victim.vp().palw_partition_refusal_v1().expect("the minority's resolves refused the majority's tip");
    assert_eq!((refusal.sink, refusal.refused), (minority_tip, majority_tip), "{tag}: {refusal:?}");
    assert!(refusal.resolves >= 1);
    assert_eq!(d.attacker.vp().palw_partition_refusal_v1(), None, "{tag}: the majority refuses nothing");

    // Both sides go on producing as they did, and hear each other at once.
    for round in 0..3 {
        let a = honest_slot(&mut d.victim, Vec::new()).await;
        let b = private_slot(&mut d.attacker, &mut d.nonce, 2, Vec::new()).await;
        for blk in &b {
            mirror(&mut d.victim, blk).await;
        }
        for blk in &a {
            mirror(&mut d.attacker, blk).await;
        }
        let agree = sinks_agree(tag, &format!("round {round} after the heal"), d);
        assert!(!agree, "{tag}: round {round}: still two sinks over one DAG");
    }
    let refusal = d.victim.vp().palw_partition_refusal_v1().expect("still refusing");
    assert!(refusal.refused_span_daa() >= 3, "{tag}: the refused branch advanced while refused: {refusal:?}");

    // N3: the `palw_*` readers answer at the node's own sink, not at the tip row. Mid-search the tip row
    // stands wherever the last walk ended — on a wedged node, on the refused branch (devnet r1's B: its
    // class row for the class it had folded read empty). Put it there, as a search leaves it, and read.
    let vp = d.victim.vp();
    let params = d.victim.bundle.state.clone();
    let sink = d.victim.sink();
    let refused_tip = d.attacker.sink();
    let refused_state = vp.palw_candidate_state_v2(refused_tip).expect("the minority can weigh the refused tip");
    vp.palw_state_v2_store.write().set_tip_for_tests(refused_tip, &refused_state).unwrap();
    let (row_block, row_state) = vp.palw_state_v2_store.read().load_tip_cached(&params).unwrap().unwrap();
    assert_eq!(row_block, refused_tip, "{tag}: the tip row stands on the refused branch, as mid-search");
    assert!(
        matches!(row_state.claim(&p.claim_id).unwrap().phase, PalwClaimPhaseV2::PanelBound { .. }),
        "{tag}: and there the minority's licence does not exist — what the readers used to report"
    );
    let (at, state) = vp.palw_v2_reader_state(&params).unwrap().expect("a reader state");
    assert_eq!(at, sink, "{tag}: the readers answer at this node's own sink");
    assert!(
        matches!(state.claim(&p.claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "{tag}: with this node's own fold — the licence it carries"
    );
    let readers = vp.palw_claim_readers_v2_impl(p.claim_id);
    let expected = state.claim_readers_v2(&p.claim_id);
    assert_eq!(readers, expected, "{tag}: an RPC reader answers from the sink's state");
}
