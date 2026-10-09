//! **RFC-0012 wave 2 — the code items that needed a node: C1 (a claim reaches `Final` through the processor with the EVM lane ON) and
//! C2 (a real pruned join with EVM state).**
//!
//! **C1.** Until now no claim could reach `Final` through the processor on testnet-12 as shipped: the EVM lane executes a block against
//! its header timestamp, so a harness that re-stamps a built template (the only way to run 123+ DAA of 120 s slots on a wall clock) breaks
//! `evm_commitment_root`, and the harness left the lane inert (`stamp_harness_time`'s old assertion). The fix is not to re-stamp but to
//! make the BUILDER read the harness's clock: `VirtualStateProcessor::template_clock` (a `cfg(test)` field read by `template_now()`, the one
//! place the template's stamp takes the wall clock) is set before every build by `T12Chain::arm_clock` - on an EVM-active network only, so no
//! existing test changes. The block the lane executed is then the block the harness submits, and `build == validate` holds with a simulated
//! clock. The claim is real: attempt, bound panel, five real ML-DSA-87 receipts, the challenge window, `Final` - all through
//! `validate_and_insert_block` with the EVM lane as shipped.
//!
//! **What stays a seam, named.** The harness can only produce a FLOOR claim, and the floor is never evidence (`skipped.baseClass`). So the
//! `rfc12_c1_*` tests that follow the claim into the evidence relabel it to a REAL class through `native_relabel_class` (a `cfg(test)`
//! hook in `native_row`): the node's own stored delta, its extraction, the conversion (bond -> operator, canonical weight), the lifecycle
//! closure and the certificate all run unchanged; only the class id of the one record is replaced. The pure conversion of a REAL record is
//! `rfc0012_native_evidence_fold`'s.
use super::rfc12_zero_dns_matrix::parts;
use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, sign_spend, t12_genesis_chain};
use crate::consensus::test_consensus::TestConsensus;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::Block;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
use kaspa_consensus_core::palw_native_readiness_v1::SafeWaitV1;
use kaspa_consensus_core::palw_native_settlement_v1::SettlementStopV1;
use kaspa_consensus_core::palw_panel_v2::{
    PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
};
use kaspa_consensus_core::palw_state_v2::{PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutput};
use kaspa_hashes::Hash64;

/// The test retirement fence of the matrix: DAA 1.
const FENCE: u64 = 1;

/// One claim's journey, as the processor lived it.
struct ClaimLife {
    claim_id: Hash64,
    /// The attempt block that accepted the claim.
    anchor: Block,
    licensed_daa: u64,
    final_daa: u64,
    /// `trace_retention_daa`, read from the claim at `Final`.
    retention_daa: u64,
}

/// Beat `n` DAA (one heartbeat per slot, each stamped at the slot).
async fn beat_to(chain: &mut T12Chain, daa: u64) {
    let step = chain.config.params.target_time_per_block();
    while chain.daa_of(chain.sink()) < daa {
        chain.heartbeat(step, Vec::new()).await;
    }
}

/// **A real claim to `Final` through the processor with the EVM lane on**: heartbeats past the fence, card 0's attempt, heartbeats to the
/// claim's anchor slot, card 7's attempt there (which binds the panel), the drawn seats' real receipts in a carrier funded by card 0's
/// float, and heartbeats until the short challenge window has run. Returns once the claim is `Final` at the sink.
async fn claim_to_final(
    chain: &mut T12Chain,
    floats: &[(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)],
) -> ClaimLife {
    assert!(chain.config.params.is_evm_active(0), "the EVM lane is as shipped: this is the point");
    let step = chain.config.params.target_time_per_block();
    beat_to(chain, FENCE + 2).await;
    let (anchor, claim_id) = chain.attempt(0, step, Vec::new(), &|_| true).await;
    let bound = chain.attempt_at_the_anchor_slot(claim_id, 7).await;
    let (_, state) = chain.tip_state();
    let panel = state.panel(&claim_id).expect("the anchor attempt bound the panel").clone();
    let seat_cards: Vec<usize> =
        panel.seats.iter().map(|s| chain.bonds.iter().position(|b| *b == s.bond).expect("a genesis card")).collect();
    let signed_daa = chain.ctx.consensus.get_virtual_daa_score();
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        chain.config.params.net.to_string().as_bytes(),
        Some(chain.config.params.genesis.hash),
    );
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .zip(&seat_cards)
        .take(chain.bundle.panel.quorum() as usize)
        .map(|(seat, card)| {
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(*card as u64).signing_key,
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
    let object = chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts).expect("a signed quorum assembles");
    assert!(matches!(object, Obj::ReceiptLicensed { .. }));
    let carrier = {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
        let (outpoint, entry) = floats[0].clone();
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - 300_000, card_payout_spk(0))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, 0, chain.config.params.storage_mass_parameter);
        tx
    };
    chain.heartbeat(step, vec![carrier]).await;
    chain.heartbeat(step, Vec::new()).await;
    let (_, state) = chain.tip_state();
    let licensed_daa = match state.claim(&claim_id).expect("the claim stands").phase.clone() {
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } => licensed_daa,
        other => panic!("the carried quorum licenses the claim; it is {other:?}"),
    };
    let final_due = licensed_daa + chain.bundle.state.window_challenge_at(licensed_daa) + 1;
    eprintln!(
        "[c1] claim {claim_id}: accepted at DAA {}, panel bound at DAA {}, licensed at {licensed_daa}, Final due at {final_due}",
        anchor.header.daa_score, bound.header.daa_score
    );
    beat_to(chain, final_due).await;
    let (_, state) = chain.tip_state();
    let claim = state.claim(&claim_id).expect("the claim stands");
    let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
        panic!("Final at DAA {final_due}; the claim is {:?}", claim.phase)
    };
    assert!(final_daa <= final_due, "Final at {final_daa}, due {final_due}");
    ClaimLife { claim_id, anchor, licensed_daa, final_daa, retention_daa: claim.trace_retention_daa }
}

fn readiness(chain: &T12Chain) -> kaspa_consensus_core::palw_native_readiness_v1::NativeSafeReadinessV1 {
    chain.ctx.consensus.get_native_safe_readiness().expect("readable").expect("a readiness past the fence")
}

fn snapshot(chain: &T12Chain) -> kaspa_consensus_core::palw_native_settlement_v1::NativeSettlementSnapshotV1 {
    chain.ctx.consensus.get_native_settlement_snapshot().expect("readable").expect("a snapshot past the fence")
}

/// **EXPECTED (C1, the claim as the floor produces it).** On testnet-12 as shipped (EVM lane active from genesis, every block the lane's
/// own, none re-stamped after the build):
/// * a claim is accepted, bound, licensed by five real receipts and reaches `Final` through the processor, every block UTXO-valid and the
///   sink - the thing no harness could do with the lane on;
/// * the node's OWN stored delta of the finalizing block is read by the native-settlement walk: the floor is never evidence, so the
///   readiness counts it under `skipped.baseClass` (1) and has no pending fact - evidence was extracted and refused by name, not missed;
/// * the safe frontier is the claim's accepting block (a `Final` buys a frontier), published in the snapshot;
/// * the explanation agrees with the snapshot: its first stop-bearing wait is the snapshot's stop.
#[tokio::test]
async fn rfc12_c1_a_a_floor_claim_reaches_final_with_the_evm_lane_on_and_the_node_reads_its_delta() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = parts(Some(FENCE));
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let life = claim_to_final(&mut chain, &floats).await;
    eprintln!(
        "[c1-a] Final at DAA {} (+{} after acceptance, +{} after the licence); retention lapses at {}",
        life.final_daa,
        life.final_daa - life.anchor.header.daa_score,
        life.final_daa - life.licensed_daa,
        life.retention_daa
    );
    let s = snapshot(&chain);
    let r = readiness(&chain);
    eprintln!("[c1-a] {}", serde_json::to_string_pretty(&r).unwrap());
    assert_eq!(s.generation, chain.sink());
    eprintln!("[c1-a] frontier {:?}; the claim's accepting block {}", s.frontier, life.anchor.header.hash);
    assert!(s.frontier.is_some(), "a Final buys a frontier");
    assert_eq!(r.skipped.base_class, 1, "the floor claim's Final was read from the node's own delta and refused as the floor");
    assert!(r.skipped.total() >= 1);
    let blocking = r.blocking.as_ref().expect("nothing is certified: a blocking effect");
    assert_eq!(blocking.evidence.pending_facts, 0, "a floor claim leaves no fact to wait for");
    assert_eq!(blocking.waits.iter().find_map(|w| w.stop()), s.stop, "the explanation's first stop is the snapshot's");
    assert_eq!((s.safe, s.finalized), (None, None), "no evidence, no safe");
    assert!(matches!(
        s.stop,
        Some(SettlementStopV1::InsufficientDepth | SettlementStopV1::OpenLifecycle | SettlementStopV1::FrontierNotCovered)
    ));
}

/// **EXPECTED (C1, the same claim read as REAL work through the one named seam).** The node's own delta of the finalizing block, relabelled
/// to a REAL class on read, makes a fact: weight = the claim's canonical `pwu`, operator = the bond's, matured at the v1 instant
/// `max(trace_retention, Final + claim_retirement)`. Then, with the claim still `Final` and in state:
/// * `skipped.baseClass` is 0 and the blocking effect has exactly one pending fact (the claim's work) - and `waitingMaturity` names the
///   instant: `readyDaa == trace_retention`, `waitDaa == retention - sinkDaa`, and since nothing else is unmet the promised clock is exactly
///   that many DAA;
/// * mining more heartbeats counts the clock down by exactly the DAA mined;
/// * the snapshot has no `safe`: immature work certifies nothing.
#[tokio::test]
async fn rfc12_c1_b_the_claim_read_as_real_work_waits_for_exactly_its_instant() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = parts(Some(FENCE));
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    *chain.vp().native_relabel_class.lock() = Some(Hash64::from_u64_word(0x12C1_5EA1));
    let life = claim_to_final(&mut chain, &floats).await;
    let sink_daa = chain.daa_of(chain.sink());
    let r = readiness(&chain);
    eprintln!("[c1-b] {}", serde_json::to_string_pretty(&r.blocking).unwrap());
    let s = snapshot(&chain);
    assert_eq!(r.skipped.base_class, 0, "relabelled: not the floor");
    let blocking = r.blocking.as_ref().expect("a blocking effect");
    assert_eq!(blocking.evidence.pending_facts, 1, "the claim's work is the one pending fact");
    let wait = blocking
        .waits
        .iter()
        .find_map(|w| if let SafeWaitV1::WaitingMaturity { .. } = w { Some(w.clone()) } else { None })
        .expect("a maturity wait");
    let SafeWaitV1::WaitingMaturity { facts, earliest_matured_daa, wait_daa, ready_daa, .. } = wait else { unreachable!() };
    let instant = life.retention_daa.max(life.final_daa + bundle.state.claim_retirement_daa());
    assert_eq!((facts, earliest_matured_daa, ready_daa), (1, instant, Some(instant)), "matured at the v1 instant");
    assert_eq!(wait_daa, instant - sink_daa);
    assert_eq!(blocking.earliest_ready_in_daa, Some(instant - sink_daa), "nothing else is unmet: the promise is the clock");
    assert_eq!((s.safe, s.finalized), (None, None));
    // The countdown is exact.
    let mined = 5;
    beat_to(&mut chain, sink_daa + mined).await;
    let again = readiness(&chain).blocking.expect("still blocked");
    assert_eq!(again.earliest_ready_in_daa, Some(instant - (sink_daa + mined)), "{mined} DAA mined, {mined} DAA fewer to wait");
}

/// **EXPECTED (C1, the whole way - ~5,400 DAA, run with `--ignored`).** The same claim, mined on until the v1 instant: `safe` is absent at
/// `instant - 1` and is the claim's accepting block AT `instant`, and the explanation says so. This is the end-to-end proof that the node's
/// own deltas, through the processor with the lane on, move `safe` at the rule's instant (`rfc0012_safe_maturity_attacks::d1_a` proves the
/// same instant on the fold; this ties the processor to it).
#[tokio::test]
#[ignore = "mines ~5,400 DAA of heartbeats through the EVM lane (tens of minutes); run explicitly"]
async fn rfc12_c1_c_safe_moves_at_the_v1_instant_through_the_processor() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = parts(Some(FENCE));
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    *chain.vp().native_relabel_class.lock() = Some(Hash64::from_u64_word(0x12C1_5EA1));
    let life = claim_to_final(&mut chain, &floats).await;
    let retirement = bundle.state.claim_retirement_daa();
    let instant = life.retention_daa.max(life.final_daa + retirement);
    eprintln!(
        "[c1-c] Final {}, retention {}, retirement at Final+{retirement}: the v1 instant is DAA {instant}",
        life.final_daa, life.retention_daa
    );
    let step = chain.config.params.target_time_per_block();
    let mut last_report = 0;
    while chain.daa_of(chain.sink()) + 1 < instant {
        chain.heartbeat(step, Vec::new()).await;
        let daa = chain.daa_of(chain.sink());
        if daa >= last_report + 500 {
            last_report = daa;
            let r = readiness(&chain);
            eprintln!("[c1-c] DAA {daa}: promise {:?}", r.blocking.as_ref().and_then(|b| b.earliest_ready_in_daa));
        }
    }
    assert_eq!(chain.daa_of(chain.sink()), instant - 1);
    assert_eq!(snapshot(&chain).safe, None, "one DAA before the instant: nothing is safe");
    assert_eq!(readiness(&chain).blocking.expect("blocked").earliest_ready_in_daa, Some(1));
    beat_to(&mut chain, instant).await;
    let s = snapshot(&chain);
    assert_eq!(s.safe, Some(life.anchor.header.hash), "at the instant, safe is the claim's accepting block");
    let r = readiness(&chain);
    assert_eq!(r.safe, s.safe);
    assert_eq!(r.stop, s.stop);
}

// =====================================================================================================================
// C2 — a pruned join with EVM state
// =====================================================================================================================

/// **EXPECTED (C2, what is real and what is a seam - stated before the run).**
///
/// A joiner that arrives by pruned IBD has the headers, the pruning point P's state and the blocks above P, and NOTHING of P's past: no
/// PALW tip, no delta row, no EVM header or state row. It installs, from a peer that captured them at P, the PALW carriage (verified against
/// the committed root of P's child header) and the EVM header + state snapshot (verified against the L1 commitment and the keccak-MPT state
/// root), and then folds and executes the blocks above P against them. This test builds exactly that situation in one process:
///
/// * the SOURCE is x1's scripted run (EVM lane as shipped, a deposit claimed, sells and a withdrawal, the fence crossed); P is the block
///   that carries the sells, so the account's balance is non-trivial and the withdrawal, its settlement and three more blocks lie above P;
/// * the JOINER replays the source's blocks through P and then has everything the pruned joiner does not have removed - the PALW tip and
///   every delta row, the EVM header and state rows of every block through P - leaving only P's child's HEADER (the witness the sidecars are
///   verified against); its pruning point is set to P;
/// * the sidecars are installed through the node's own import functions, the bytes round-tripped through borsh as the wire carries them
///   (`capture_pruning_point_palw_state` / `pruning_point_palw_state`, `get_evm_header_of` / `get_evm_state_snapshot_of`);
/// * the blocks above P are then taken by the joiner, each UTXO-valid and the sink, each with the source's PALW root and the source's EVM
///   header (so the lane re-executed against the IMPORTED state to the source's roots).
///
/// And what the native-settlement stack does on the joiner: the import clears the snapshot (x7), the heads read `latest = P`, `safe` and
/// `finalized` null; after the first block the snapshot is rebuilt from P; `safe`/`finalized` are never invented; the explanation's
/// `finalized.pruningPoint` is P and the walk covers exactly the blocks from P to the sink.
///
/// **Seams, named.** The joiner replays the bodies below P before the cut rather than header-syncing them; the UTXO set is therefore the
/// replayed one, not served in chunks (the source's pruning UTXO set is advanced by the real pruning processor, which cannot move a pruning
/// point on this clock - a PALW safe frontier caps it - so `get_pruning_point_utxos` is not exercised); the DNS overlay snapshot is not
/// imported (the overlay is retired past the fence and its import is unchanged); and nothing here touches the P2P messages. The next step
/// for the rest is the multi-node drill on the shipped binary (policy proposal 11.3).
#[tokio::test]
async fn rfc12_c2_a_pruned_join_with_evm_state_installs_the_sidecars_and_follows_the_source() {
    use super::rfc12_zero_dns_matrix::{Rig, scripted_run};
    use crate::model::stores::evm::{EvmHeaderStore, EvmStateStore};
    use crate::model::stores::pruning::PruningStore;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    kaspa_core::log::try_init_logger("warn");
    let p = parts(Some(FENCE));
    let mut source = Rig::new(&p, 0x12_c200_0000);
    let run = scripted_run(&mut source, "").await;
    let named =
        |what: &str| run.names.iter().find(|(n, _)| *n == what).map(|(_, h)| *h).unwrap_or_else(|| panic!("no block named {what}"));
    let pp = named("e2-sells");
    let all = source.blocks_through(source.chain.sink());
    let k = all.iter().position(|b| b.header.hash == pp).expect("P is on the chain");
    assert!(k >= 3 && all.len() >= k + 4, "blocks on both sides of P ({} below, {} above)", k, all.len() - k - 1);
    let child = all[k + 1].clone();
    let root_at = |rig: &Rig, block| rig.vp().palw_state_v2_store.read().delta_of(block).expect("a delta row").0;

    // ---- the source captures what a peer would serve ---------------------------------------------------------------------
    let source_vp = source.vp();
    source_vp.capture_pruning_point_palw_state(pp);
    let wire_palw = borsh::to_vec(&source.api().pruning_point_palw_state(pp).expect("servable at P")).expect("serializes");
    let evm_header = source.api().get_evm_header_of(pp).expect("readable").expect("P's EVM header");
    let evm_snapshot = source.api().get_evm_state_snapshot_of(pp).expect("readable").expect("P's EVM state");
    let wire_evm_header = borsh::to_vec(&evm_header).expect("serializes");
    let wire_evm_state = borsh::to_vec(&evm_snapshot).expect("serializes");
    assert!(!evm_snapshot.accounts.is_empty(), "P's EVM state is not empty: the deposit is in it");

    // ---- the joiner: the chain through P, then the cut --------------------------------------------------------------------
    let mut joiner = Rig::new(&p, 0x12_c210_0000);
    for b in &all[..=k] {
        joiner.arrive(b.clone(), "a block through P").await;
    }
    joiner
        .api()
        .validate_and_insert_block(Block::from_header_arc(child.header.clone()))
        .virtual_state_task
        .await
        .expect("P's child header, the witness the sidecars are verified against");
    {
        let jvp = joiner.vp();
        let mut palw = jvp.palw_state_v2_store.write();
        palw.delete_tip_for_tests().expect("no PALW tip");
        for blk in std::iter::once(joiner.config.params.genesis.hash).chain(all[..=k].iter().map(|b| b.header.hash)) {
            palw.delete_delta_for_tests(blk).expect("no delta row at or below P");
        }
        drop(palw);
        let mut batch = rocksdb::WriteBatch::default();
        for blk in all[..=k].iter().map(|b| b.header.hash) {
            jvp.evm_header_store.delete_batch(&mut batch, blk).unwrap();
            jvp.evm_state_store.delete_batch(&mut batch, blk).unwrap();
        }
        jvp.db.write(batch).unwrap();
        jvp.pruning_point_store.write().set(pp, 1).unwrap();
        jvp.native_rows.lock().clear();
        jvp.native_readiness_memo.lock().take();
    }
    assert!(joiner.api().get_evm_header_of(pp).unwrap().is_none(), "the joiner holds no EVM row of P before the import");

    // ---- the sidecars, as the wire carries them ---------------------------------------------------------------------------
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire_palw).expect("the wire bytes decode");
    joiner.vp().import_pruning_point_palw_state(pp, carriage).expect("the carriage installs against P's child's committed root");
    let (at, imported) = joiner.chain.tip_state();
    assert_eq!(at, pp);
    assert_eq!(imported.state_root(), root_at(&source, pp), "the imported PALW state is P's, root for root");
    joiner
        .vp()
        .import_pruning_point_evm_state(
            pp,
            borsh::from_slice(&wire_evm_header).expect("decodes"),
            borsh::from_slice(&wire_evm_state).expect("decodes"),
        )
        .expect("a root-verified import of P's EVM state");
    assert_eq!(joiner.api().get_evm_header_of(pp).unwrap(), Some(evm_header.clone()), "P's EVM header is installed byte for byte");
    assert_eq!(
        joiner.api().get_native_settlement_snapshot().unwrap(),
        None,
        "the import clears native evidence until reconstruction supplies proof"
    );
    let heads = joiner.api().get_evm_canonical_heads().unwrap().expect("heads");
    assert_eq!(
        (heads.latest_head(), heads.safe_head(), heads.finalized_head()),
        (Some(pp), None, None),
        "and never invents safe or finalized"
    );

    // ---- the blocks above P ------------------------------------------------------------------------------------------------
    for b in &all[k + 1..] {
        joiner.arrive(b.clone(), "a block above P").await;
        assert_eq!(joiner.chain.sink(), b.header.hash, "the joiner's sink follows the source's");
        assert_eq!(
            joiner.chain.tip_state().1.state_root(),
            root_at(&source, b.header.hash),
            "the joiner folds to the source's PALW root at {}",
            b.header.hash
        );
        assert_eq!(
            joiner.api().get_evm_header_of(b.header.hash).unwrap(),
            source.api().get_evm_header_of(b.header.hash).unwrap(),
            "and its lane executed against the imported state to the source's EVM header at {}",
            b.header.hash
        );
    }
    assert_eq!(joiner.api().get_evm_head_header().unwrap(), source.api().get_evm_head_header().unwrap());

    // ---- the native-settlement stack on the joiner ---------------------------------------------------------------------------
    let sink = joiner.chain.sink();
    let s = joiner.api().get_native_settlement_snapshot().unwrap().expect("rebuilt from P by the next virtual change");
    assert_eq!((s.generation, s.latest), (sink, Some(sink)));
    assert_eq!((s.safe, s.finalized), (None, None), "never invented");
    assert_eq!(
        s.stop,
        source.api().get_native_settlement_snapshot().unwrap().expect("the source's snapshot").stop,
        "the same stop as the source"
    );
    let r = joiner.api().get_native_safe_readiness().unwrap().expect("a readiness");
    assert_eq!(r.generation, sink);
    assert_eq!(r.finalized.pruning_point, pp, "the explanation stands on the imported pruning point");
    assert_eq!(r.executed_effects as usize, all.len() - k, "the walk covers P and the blocks above it, no more");
    eprintln!(
        "[c2] joined at {pp} (block {} of {}); {} blocks followed; {}",
        k + 1,
        all.len(),
        all.len() - k - 1,
        serde_json::to_string(&r.finalized).unwrap()
    );
}
