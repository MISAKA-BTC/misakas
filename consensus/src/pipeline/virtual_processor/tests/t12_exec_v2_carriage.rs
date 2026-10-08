//! **RFC-0008 v2 through the real pipeline** — the unified EXEC lane's weightless carriage, on testnet-12 as launched with harness keys on
//! the eight genesis cards (`t12_with_harness_cards`), the fence `palw_exec_payload_v2` armed at a low DAA **after** the config is built.
//!
//! `Config` construction and `kaspad` both run `validate_palw_v2`, which refuses every real height of the fence (spec section 9's gates are
//! open), so the harness sets the field on the config it owns and mirrors it into the V2 bundle (`sync_palw_exec_payload_v2`) — the
//! documented validation bypass, the way `consensus/core/tests/rfc0010_permissionless_panel.rs` arms what validation refuses. No node can.
//!
//! What is proved here, against the real header, body and virtual processors:
//!
//! * a work session opens through a signed 0x4b carrier on an accepted REAL claim, and its slices are carried by EXEC blocks (algo 10,
//!   `PXE2`, signed in the slice domain) that **hang off the chain without being its parents**;
//! * a chain block's coinbase **anchors** them (trailer `PXA2`: heads, count, root) — the template builds the anchor, the walk recomputes
//!   it, the fold credits each slice by the six admission rules and records the covered set;
//! * **weightless**: every number a chain block carries — blue score, blue work, DAA score, bits, pruning point — is what it is on a twin
//!   chain that never saw the lane, the covered blocks are in no stored mergeset, and the sink never moves for a lane block;
//! * the gates: a chain block naming a lane block as a parent, a `PXR1` envelope past the fence, a slice block carrying a transaction, a
//!   re-hung anchor, a forged payload commitment, a trailer on a lane block and a lying trailer are each refused by name; below the fence
//!   a `PXE2` header is refused and a root declaration is dropped.
use super::t12_round_lane_e2e::{
    AtTheTargetSpan, SeedDraw, T12Chain, a_floor_final_scheduled_with, card_payout_spk, sign_spend, stamp_harness_time,
    t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::headers::HeaderStoreReader;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_exec_v2::{
    PALW_EXEC_V2_WIRE_VERSION, PalwExecSubtypeV2, PalwExecV2Envelope, PalwWorkSliceV1, palw_work_slice_payload_root_v2,
};
use kaspa_consensus_core::palw_exec_v2_anchor::{PalwExecV2AnchorV1, palw_exec_v2_anchor_append, palw_exec_v2_anchor_split};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
use kaspa_consensus_core::palw_work_slice_v2::{PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT, PalwWorkRootDeclarationV2, PalwWorkRootPhaseV2};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;

/// The fence's height: low, so a short chain crosses it, and above the genesis state's first blocks.
const FENCE: u64 = 6;
/// How many canonical work units each slice of the session covers.
const SLICE_WORK: u64 = 1_000;

fn key(card: usize) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    crate::consensus::test_consensus::TestConsensus::palw_v2_registry_keypair(card as u64)
}

fn pubkey(card: usize) -> Vec<u8> {
    key(card).verification_key.as_ref().to_vec()
}

fn sign(card: usize, message: &Hash64, context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&key(card).signing_key, message.as_byte_slice(), context, [0x71u8; 32])
        .expect("ML-DSA-87 sign")
        .as_ref()
        .to_vec()
}

/// testnet-12 as launched with the harness cards, the fence armed at [`FENCE`] (or left dormant) AFTER the build.
fn config_with(fence: Option<u64>) -> (Config, Vec<(TransactionOutpoint, UtxoEntry)>, Vec<(TransactionOutpoint, UtxoEntry)>) {
    let (mut config, _, premine, floats) = t12_with_harness_cards();
    if let Some(height) = fence {
        config.params.palw_exec_payload_v2 = Some(ForkActivation::new(height));
        config.params.sync_palw_exec_payload_v2();
        assert!(config.params.validate_palw_exec_payload_v2().is_err(), "validation refuses the fence: the harness arms past it");
    }
    (config, premine, floats)
}

fn chain_of(config: &Config, premine: &[(TransactionOutpoint, UtxoEntry)], floats: &[(TransactionOutpoint, UtxoEntry)]) -> T12Chain {
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is V2") };
    t12_genesis_chain(config, bundle, premine, floats)
}

fn network_domain(config: &Config) -> Hash64 {
    kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    )
}

/// What a chain block carries that a lane block must not move: the numbers.
#[derive(Debug, PartialEq, Eq, Clone)]
struct Numbers {
    daa_score: u64,
    blue_score: u64,
    blue_work: String,
    bits: u32,
    timestamp: u64,
    pruning_point: BlockHash,
    mergeset_size: usize,
}

fn numbers_of(chain: &T12Chain, block: BlockHash) -> Numbers {
    let vp = chain.vp();
    let header = vp.headers_store.get_header(block).unwrap();
    let data = vp.ghostdag_store.get_data(block).unwrap();
    Numbers {
        daa_score: header.daa_score,
        blue_score: header.blue_score,
        blue_work: header.blue_work.to_string(),
        bits: header.bits,
        timestamp: header.timestamp,
        pruning_point: header.pruning_point,
        mergeset_size: data.mergeset_size(),
    }
}

/// The plan every session here uses, over the claim's own admitted work.
fn declaration_for(chain: &T12Chain, claim: Hash64, expiry: u64) -> PalwWorkRootDeclarationV2 {
    let (_, state) = chain.tip_state();
    let claim_row = state.claim(&claim).expect("the claim");
    let prefix = claim_row.pwu;
    let mut extra = vec![chain.bonds[1], chain.bonds[2]];
    extra.sort();
    PalwWorkRootDeclarationV2 {
        root_claim_id: claim,
        canonical_job_id: claim_row.job_identity,
        input_root: Hash64::from_u64_word(0x31),
        kernel_version: 1,
        plan_root: Hash64::from_u64_word(0x32),
        total_work: prefix + 3 * SLICE_WORK,
        boundaries: vec![prefix, prefix + SLICE_WORK, prefix + 2 * SLICE_WORK, prefix + 3 * SLICE_WORK],
        initial_state_root: Hash64::from_u64_word(0x33),
        evidence_policy_root: Hash64::from_u64_word(0x34),
        extra_executors: extra,
        expiry_daa: expiry,
        signature: Vec::new(),
    }
}

/// The 0x4b carrier of `object`, funded by card `0`'s float and signed by card 0.
fn carrier_of(config: &Config, floats: &[(TransactionOutpoint, UtxoEntry)], object: Obj) -> Transaction {
    carrier_by(config, floats, 0, object)
}

/// The 0x4b carrier of `object`, funded by card `card`'s float and signed by that card.
fn carrier_by(config: &Config, floats: &[(TransactionOutpoint, UtxoEntry)], card: usize, object: Obj) -> Transaction {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    let (float_outpoint, float_entry) = floats[card].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(float_outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(float_entry.amount - 300_000, card_payout_spk(card))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, float_entry, card, config.params.storage_mass_parameter);
    tx
}

/// A chain with an accepted REAL claim (card 0's) and an OPEN work session on it, the fence crossed: the claim is `Provisional`, the
/// session is the declaration `declaration_for` builds, signed by card 0 and carried by a 0x4b transaction.
struct Session {
    chain: T12Chain,
    config: Config,
    claim: Hash64,
    declaration: PalwWorkRootDeclarationV2,
    floats: Vec<(TransactionOutpoint, UtxoEntry)>,
}

async fn open_session(fence: Option<u64>) -> Session {
    kaspa_core::log::try_init_logger("warn");
    let (config, premine, floats) = config_with(fence);
    let chain = chain_of(&config, &premine, &floats);
    open_session_on(config, floats, chain).await
}

/// [`open_session`] over a chain the caller built (a database it keeps, to reopen it).
async fn open_session_on(config: Config, floats: Vec<(TransactionOutpoint, UtxoEntry)>, mut chain: T12Chain) -> Session {
    let ttpb = config.params.target_time_per_block();
    // Cross the fence with heartbeats (the clock testnet-12 runs on), then card 0's attempt: a REAL claim.
    for _ in 0..400 {
        if chain.daa_of(chain.sink()) > FENCE {
            break;
        }
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let daa = chain.daa_of(chain.sink());
    assert!(daa > FENCE, "the fence is crossed: DAA {daa}");
    let mut declaration = declaration_for(&chain, claim, daa + 5_000);
    declaration.signature = sign(0, &declaration.signing_message(network_domain(&config)), PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT);
    let carrier = carrier_of(&config, &floats, Obj::ExecWorkRootOpenedV2 { declaration: Box::new(declaration.clone()) });
    let carrying = chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the carrier is in the block");
    chain.heartbeat(ttpb, Vec::new()).await; // accepts the carrying block's transactions
    Session { chain, config, claim, declaration, floats }
}

impl Session {
    /// The session's root, in the tip state.
    fn root(&self) -> Option<kaspa_consensus_core::palw_work_slice_v2::PalwWorkRootV2> {
        let (_, state) = self.chain.tip_state();
        state.exec_v2_root_v1(&self.claim).cloned()
    }

    /// The honest slice a root plans at `index`, executed by card `card`: every binding the root's, the predecessor the root's boundary as
    /// the tip state holds it (for index 0 the initial boundary; a later index the previous accepted slice's result — or, when that slice
    /// is not folded yet, an unlikely value a caller overrides).
    fn planned_slice(&self, card: usize, index: u32) -> PalwWorkSliceV1 {
        let root = self.root().expect("an open session");
        let planned =
            kaspa_consensus_core::palw_work_slice_v2::palw_work_plan_range_v2(&root.boundaries, index).expect("a planned slice");
        let predecessor = if index == 0 {
            root.initial_state_root
        } else {
            let (_, state) = self.chain.tip_state();
            state.exec_v2_slice_v1(&self.claim, index - 1).map(|row| row.result_state_root).unwrap_or(Hash64::from_u64_word(0xBAD))
        };
        PalwWorkSliceV1 {
            root_claim_id: self.claim,
            slice_index: index,
            class_id: root.class_id,
            canonical_job_id: root.canonical_job_id,
            kernel_version: root.kernel_version,
            plan_root: root.plan_root,
            canonical_range: planned,
            predecessor_state_root: predecessor,
            result_state_root: Hash64::from_u64_word(0x6000 + index as u64),
            input_root: Hash64::from_u64_word(0x31),
            output_root: Hash64::from_u64_word(0x7000 + index as u64),
            evidence_root: Hash64::from_u64_word(0x8000 + index as u64),
            da_root: Hash64::from_u64_word(0x9000 + index as u64),
            executor_bond: self.chain.bonds[card],
        }
    }

    /// A signed `PXE2` `EXEC_SLICE` block for slice `index` executed by card `card`, hung from the sink's selected parent (and the lane
    /// tips on its chain), built from the node's own template. `mutate` edits the slice before it is signed.
    fn slice_block(&mut self, card: usize, index: u32, mutate: impl FnOnce(&mut PalwWorkSliceV1)) -> MutableBlock {
        let mut slice = self.planned_slice(card, index);
        mutate(&mut slice);
        self.sign_slice_block(card, slice, |_| {})
    }

    /// Build the lane block for `slice`, then sign it as card `card` under the envelope `edit` may corrupt.
    fn sign_slice_block(&mut self, card: usize, slice: PalwWorkSliceV1, edit: impl FnOnce(&mut PalwExecV2Envelope)) -> MutableBlock {
        let mut template = self
            .chain
            .ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .expect("a template");
        stamp_harness_time(&self.config.params, &mut template.block.header, self.chain.ctx.simulated_time);
        let adapted = self
            .chain
            .vp()
            .exec_v2_slice_adapt_block_template(template, card_payout_spk(card))
            .expect("the lane adapts a template into an EXEC_SLICE block");
        // The anchor is the chain block the lane block hangs from: the adapter's own answer (the selected parent the block resolves to).
        let anchor = adapted.selected_parent_hash;
        let mut block = adapted.block;
        assert_eq!(block.header.pow_algo_id, kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1);
        assert_eq!(block.transactions.len(), 1, "a slice block carries its coinbase alone");
        self.sign_lane_envelope(&mut block, card, slice, anchor, edit);
        block
    }

    fn sign_lane_envelope(
        &self,
        block: &mut MutableBlock,
        card: usize,
        slice: PalwWorkSliceV1,
        anchor: BlockHash,
        edit: impl FnOnce(&mut PalwExecV2Envelope),
    ) {
        block.header.nonce = 0;
        block.header.palw_commitment = Vec::new();
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
        let mut envelope = PalwExecV2Envelope {
            version: PALW_EXEC_V2_WIRE_VERSION,
            network_domain: network_domain(&self.config),
            anchor,
            subtype: PalwExecSubtypeV2::Slice,
            tx_permit: None,
            payload_root: palw_work_slice_payload_root_v2(&slice),
            executor_bond: self.chain.bonds[card],
            work_slice: Some(slice),
            pubkey: pubkey(card),
            signature: vec![0; kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MLDSA87_SIGNATURE_LEN],
        };
        edit(&mut envelope);
        // A malformed carrier (both payloads, or neither) has no message to sign: it is signed over nothing and refused by shape before
        // its signature is ever read.
        let message = envelope.signing_message(pre_pow, block.header.timestamp, block.header.nonce).unwrap_or_default();
        envelope.signature = sign(card, &message, envelope.mldsa87_context());
        block.header.palw_commitment = envelope.encode();
        block.header.finalize();
    }

    /// Insert a lane block and demand the sink did not move.
    async fn insert_lane_block(&mut self, block: &MutableBlock, what: &str) {
        let sink = self.chain.sink();
        let hash = block.header.hash;
        let status = self
            .chain
            .ctx
            .consensus
            .validate_and_insert_block(block.clone().to_immutable())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused: {e}"));
        assert!(status.has_block_body(), "{what} has a body");
        assert_eq!(self.chain.sink(), sink, "{what} never moves the sink");
    }

    async fn insert_refused(&mut self, block: &MutableBlock) -> String {
        match self.chain.ctx.consensus.validate_and_insert_block(block.clone().to_immutable()).virtual_state_task.await {
            Err(e) => e.to_string(),
            Ok(status) => panic!("the block was accepted ({status:?}); a refusal was expected"),
        }
    }
}

/// **Weightless, accepted, anchored**: the session's slices ride EXEC blocks; a chain block anchors them; the fold credits each once;
/// and every number on the chain is what it is on a twin that never saw the lane.
#[tokio::test]
async fn t12_exec_v2_slices_are_carried_anchored_credited_and_weightless() {
    let mut lane = open_session(Some(FENCE)).await;
    let root = lane.root().expect("the session opened through its carrier");
    assert_eq!(root.phase, PalwWorkRootPhaseV2::Open);
    assert_eq!(root.root_bond, lane.chain.bonds[0]);
    assert_eq!(root.next_index, 0);
    let (_, state) = lane.chain.tip_state();
    assert_eq!(root.prefix_work(), state.claim(&lane.claim).unwrap().pwu, "the prefix is the claim's admitted canonical work");

    // The twin: the same claim and session on a chain that never builds a lane block.
    let mut twin = open_session(Some(FENCE)).await;

    let ttpb = lane.config.params.target_time_per_block();
    // A chain block that moves no clock, so its selected parent is the anchor the lane hangs from.
    let (mid, _) = lane.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let (twin_mid, _) = twin.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let anchor = lane.chain.vp().ghostdag_store.get_selected_parent(mid.header.hash).unwrap();

    // Slice 0 by card 1 and slice 1 by card 2 (both authorised by the declaration), each its own EXEC block.
    let s0 = lane.slice_block(1, 0, |_| {});
    lane.insert_lane_block(&s0, "slice 0").await;
    assert_eq!(
        lane.chain.vp().ghostdag_store.get_selected_parent(s0.header.hash).unwrap(),
        anchor,
        "anchored on the chain block beneath the sink"
    );
    // Slice 1 chains from slice 0's result boundary. Its predecessor is the state AFTER slice 0 is accepted — which has not happened yet
    // (nothing anchored it) — so it is built against the committed result of slice 0.
    // (the helper reads the tip state, which does not hold slice 0 yet; the mutation supplies the boundary.)
    let s1 = lane.slice_block(2, 1, |slice| slice.predecessor_state_root = Hash64::from_u64_word(0x6000));
    lane.insert_lane_block(&s1, "slice 1").await;

    // The EXEC blocks are tips off the chain: the virtual merges them as reds in ITS view, no chain block names them.
    {
        let vp = lane.chain.vp();
        let virtual_state = vp.virtual_stores.read().state.get().unwrap();
        assert!(
            !virtual_state.parents.contains(&s0.header.hash) && !virtual_state.parents.contains(&s1.header.hash),
            "virtual names no EXEC block as a parent"
        );
    }

    // The anchoring chain block (card 2's attempt): the node's own template carries the anchor.
    let (anchoring, _) = lane.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let (_, after) = lane.chain.tip_state();
    let (twin_anchoring, _) = twin.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;

    // The trailer names the heads and nothing else about the lane; the covered set is in the fold.
    let coinbase = &anchoring.transactions[0];
    let trailer = palw_exec_v2_anchor_split(&coinbase.payload).expect("well-formed").1.expect("the chain block anchors the lane");
    assert!(trailer.heads.contains(&s1.header.hash), "the lane head is named");
    assert_eq!(trailer.count, 2, "the two EXEC blocks it newly covers");
    assert!(after.exec_v2_anchored_v1(&s0.header.hash) && after.exec_v2_anchored_v1(&s1.header.hash), "the covered set is recorded");

    // Weightless: not in any stored mergeset, and every number equals the twin's.
    let vp = lane.chain.vp();
    let data = vp.ghostdag_store.get_data(anchoring.header.hash).unwrap();
    for lane_block in [s0.header.hash, s1.header.hash] {
        assert!(
            !data.mergeset_blues.contains(&lane_block) && !data.mergeset_reds.contains(&lane_block),
            "an EXEC block is in no stored mergeset"
        );
    }
    assert_eq!(numbers_of(&lane.chain, mid.header.hash), numbers_of(&twin.chain, twin_mid.header.hash));
    assert_eq!(
        numbers_of(&lane.chain, anchoring.header.hash),
        numbers_of(&twin.chain, twin_anchoring.header.hash),
        "blue score, blue work, DAA, bits, pruning point and mergeset: unchanged by any EXEC block"
    );
    assert_eq!(lane.chain.sink(), anchoring.header.hash, "the sink is the chain block");

    // Accounting: slice 0 was credited (card 1, authorised, predecessor = the initial boundary); slice 1 chained from it in the same
    // block and was credited too; the root holds two of three, both pending, none verified.
    let root = after.exec_v2_root_v1(&lane.claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.pending, root.verified_work), (2, 2 * SLICE_WORK, 2, 0), "{root:?}");
    assert_eq!(after.exec_v2_slice_v1(&lane.claim, 0).unwrap().carrier, s0.header.hash);
    assert_eq!(after.exec_v2_slice_v1(&lane.claim, 1).unwrap().carrier, s1.header.hash);
    // No reward, no weight, no permit, no clock: the lane moved none of them.
    let (_, twin_after) = twin.chain.tip_state();
    assert_eq!(after.safe_weight(), twin_after.safe_weight(), "no PALW weight");
    let _ = ttpb;
}

// =============================================================================================
// The gates
// =============================================================================================

fn refused_with(why: &str, got: &str) {
    assert!(got.contains(why), "expected a refusal naming {why:?}, got: {got}");
}

/// A native spend of card `card`'s float (a transaction a lane block may NOT carry when it is a slice).
fn native_spend(config: &Config, floats: &[(TransactionOutpoint, UtxoEntry)], card: usize) -> Transaction {
    let (outpoint, entry) = floats[card].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(entry.amount - 200_000, card_payout_spk(card))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    sign_spend(&mut tx, entry, card, config.params.storage_mass_parameter);
    tx
}

#[tokio::test]
async fn t12_exec_v2_header_and_body_gates_refuse_each_hostile_lane_block_by_name() {
    let mut s = open_session(Some(FENCE)).await;
    s.chain.attempt(3, 1_000, Vec::new(), &|_| true).await; // the anchor beneath the sink
    let sink = s.chain.sink();

    // ---- header stage ----
    // The envelope names an anchor other than the block's selected parent: re-hung under a signature that names the wrong chain block.
    let slice = s.planned_slice(1, 0);
    let rehung = s.sign_slice_block(1, slice.clone(), |e| e.anchor = Hash64::from_u64_word(0xA0));
    refused_with("envelope names anchor", &s.insert_refused(&rehung).await);

    // A forged payload commitment, another network, a flipped signature byte, a tampered slice under its signature.
    let forged_payload = s.sign_slice_block(1, slice.clone(), |e| e.payload_root = Hash64::from_u64_word(5));
    refused_with("payload commitment", &s.insert_refused(&forged_payload).await);
    let other_network = s.sign_slice_block(1, slice.clone(), |e| e.network_domain = Hash64::from_u64_word(5));
    refused_with("another network", &s.insert_refused(&other_network).await);
    let mut flipped = s.sign_slice_block(1, slice.clone(), |_| {});
    let last = flipped.header.palw_commitment.len() - 1;
    flipped.header.palw_commitment[last] ^= 0x01;
    flipped.header.finalize();
    refused_with("signature", &s.insert_refused(&flipped).await);
    // A signature of the permit domain does not verify a slice envelope: sign the message under the TX context instead.
    let mut wrong_context = s.sign_slice_block(1, slice.clone(), |_| {});
    {
        let mut envelope = PalwExecV2Envelope::decode(&wrong_context.header.palw_commitment).unwrap();
        let pre_pow = {
            let mut bare = wrong_context.header.clone();
            bare.palw_commitment = Vec::new();
            kaspa_consensus_core::hashing::header::pre_pow_hash_64(&bare)
        };
        let message = envelope.signing_message(pre_pow, wrong_context.header.timestamp, wrong_context.header.nonce).unwrap();
        envelope.signature = sign(1, &message, kaspa_consensus_core::palw_exec_v2::PALW_EXEC_V2_TX_MLDSA87_CONTEXT);
        wrong_context.header.palw_commitment = envelope.encode();
        wrong_context.header.finalize();
    }
    refused_with("signature", &s.insert_refused(&wrong_context).await);
    // Both payloads in one carrier, and no payload at all: the whole carrier is rejected.
    let both = s.sign_slice_block(1, slice.clone(), |e| {
        e.tx_permit = Some(kaspa_consensus_core::palw_exec_v2::PalwExecTxPermitV2 { round: 1, permit_index: 0 });
    });
    refused_with("both a permit and a work slice", &s.insert_refused(&both).await);
    let neither = s.sign_slice_block(1, slice.clone(), |e| e.work_slice = None);
    assert!(!s.insert_refused(&neither).await.is_empty());

    // ---- a v1 envelope past the fence ----
    let v1 = {
        use kaspa_consensus_core::palw_execution_lane_v1::{
            PALW_EXEC_ENVELOPE_VERSION_V1, PALW_EXEC_MLDSA87_CONTEXT, PalwExecEnvelopeV1, palw_exec_signing_message_v1,
            palw_execution_round_v1,
        };
        let mut template = s
            .chain
            .ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .unwrap();
        stamp_harness_time(&s.config.params, &mut template.block.header, s.chain.ctx.simulated_time);
        let round = palw_execution_round_v1(s.chain.ctx.simulated_time, s.config.params.genesis.timestamp);
        let mut block =
            s.chain.vp().round_adapt_block_template(template, round, card_payout_spk(0)).expect("a v1 round template").block;
        block.header.nonce = 0;
        block.header.palw_commitment = Vec::new();
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
        let message =
            palw_exec_signing_message_v1(network_domain(&s.config), pre_pow, block.header.timestamp, 0, round, 0, &s.chain.bonds[0]);
        let signature =
            libcrux_ml_dsa::ml_dsa_87::sign(&key(0).signing_key, message.as_byte_slice(), PALW_EXEC_MLDSA87_CONTEXT, [0u8; 32])
                .unwrap()
                .as_ref()
                .to_vec();
        block.header.palw_commitment = PalwExecEnvelopeV1 {
            version: PALW_EXEC_ENVELOPE_VERSION_V1,
            network_domain: network_domain(&s.config),
            round,
            permit_index: 0,
            bond: s.chain.bonds[0],
            pubkey: pubkey(0),
            signature,
        }
        .encode();
        block.header.finalize();
        block
    };
    refused_with("PXR1", &s.insert_refused(&v1).await);

    // ---- body stage ----
    let anchor_of_good =
        PalwExecV2Envelope::decode(&s.sign_slice_block(1, slice.clone(), |_| {}).header.palw_commitment).unwrap().anchor;
    // A slice block that carries a transaction beside its coinbase.
    let mut with_tx = s.sign_slice_block(1, slice.clone(), |_| {});
    {
        with_tx.transactions.push(native_spend(&s.config, &s.floats, 1));
        with_tx.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(with_tx.transactions.iter());
        // The merkle root moved, so the header position moved: re-sign the same slice at the new position.
        s.sign_lane_envelope(&mut with_tx, 1, slice.clone(), anchor_of_good, |_| {});
    }
    refused_with("carries 1 transactions", &s.insert_refused(&with_tx).await);
    // A trailer on a lane block's coinbase: only a chain block anchors the lane.
    let mut trailed = s.sign_slice_block(1, slice.clone(), |_| {});
    {
        let anchor = PalwExecV2AnchorV1 { heads: vec![Hash64::from_u64_word(1)], count: 1, root: Hash64::from_u64_word(2) };
        let extra = palw_exec_v2_anchor_append(&[], &anchor).unwrap();
        trailed.transactions[0].payload.extend_from_slice(&extra);
        trailed.transactions[0].finalize();
        trailed.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(trailed.transactions.iter());
        let anchor_hash = anchor_of_good;
        s.sign_lane_envelope(&mut trailed, 1, slice.clone(), anchor_hash, |_| {});
    }
    refused_with("only a chain block anchors the lane", &s.insert_refused(&trailed).await);

    // Nothing above moved the chain.
    assert_eq!(s.chain.sink(), sink);
    assert_eq!(s.root().unwrap().next_index, 0);

    // ---- the control: the same slice, honestly signed, is a valid lane block ----
    let honest = s.sign_slice_block(1, slice, |_| {});
    s.insert_lane_block(&honest, "the honest slice").await;
}

/// **A chain block may not name an EXEC block as a parent**, and **a trailer that lies disqualifies its block** — while a heartbeat (the
/// lane testnet-12's clock runs on) anchors the lane as an attempt does.
#[tokio::test]
async fn t12_exec_v2_a_chain_block_names_no_lane_block_and_a_lying_trailer_is_disqualified_while_a_heartbeat_anchors() {
    let mut s = open_session(Some(FENCE)).await;
    s.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let ttpb = s.config.params.target_time_per_block();
    let slice = s.slice_block(1, 0, |_| {});
    s.insert_lane_block(&slice, "slice 0").await;

    // ---- a chain block naming the lane block as a parent is refused at the header ----
    let mut naming = {
        s.chain.ctx.simulated_time += ttpb;
        let mut template = s
            .chain
            .ctx
            .consensus
            .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
            .unwrap();
        stamp_harness_time(&s.config.params, &mut template.block.header, s.chain.ctx.simulated_time);
        template.block.header.nonce = 77;
        template.block.header.finalize();
        s.chain.vp().heartbeat_adapt_block_template(template).expect("the heartbeat lane is open").0.block
    };
    let mut parents = naming.header.direct_parents().to_vec();
    parents.push(slice.header.hash);
    naming.header.parents_by_level.set_direct_parents(parents);
    naming.header.finalize();
    refused_with("names no lane block as a parent", &s.insert_refused(&naming).await);
    s.chain.ctx.simulated_time -= ttpb;

    // ---- a heartbeat anchors the lane (the trailer survives the heartbeat adapter) ----
    s.chain.ctx.simulated_time += ttpb;
    let mut template = s
        .chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .unwrap();
    stamp_harness_time(&s.config.params, &mut template.block.header, s.chain.ctx.simulated_time);
    template.block.header.nonce = 99;
    template.block.header.finalize();
    let (template, _) = s.chain.vp().heartbeat_adapt_block_template(template).expect("the heartbeat lane is open");
    let beat = template.block;
    let trailer = palw_exec_v2_anchor_split(&beat.transactions[0].payload).unwrap().1.expect("the heartbeat carries the anchor");
    assert_eq!(trailer.heads, vec![slice.header.hash]);

    // ---- the same beat with a lying count is disqualified from the chain (the block stands in the DAG) ----
    let mut lying = beat.clone();
    {
        let mut lie = trailer.clone();
        lie.count += 1;
        let (prefix, _) = palw_exec_v2_anchor_split(&lying.transactions[0].payload).unwrap();
        let mut payload = prefix.to_vec();
        payload.extend_from_slice(&lie.trailer());
        lying.transactions[0].payload = payload;
        lying.transactions[0].finalize();
        lying.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(lying.transactions.iter());
        lying.header.nonce = 100;
        lying.header.finalize();
    }
    let sink = s.chain.sink();
    let hash = lying.header.hash;
    let _ = s.chain.ctx.consensus.validate_and_insert_block(lying.to_immutable()).virtual_state_task.await;
    assert_eq!(s.chain.sink(), sink, "a block whose anchor lies is not a chain block");
    assert_eq!(
        s.chain.ctx.consensus.block_status(hash),
        BlockStatus::StatusDisqualifiedFromChain,
        "disqualified like any commitment fault"
    );

    // ---- the honest beat is the sink and its fold recorded the slice ----
    let honest = beat.to_immutable();
    let beat_hash = honest.header.hash;
    s.chain.ctx.consensus.validate_and_insert_block(honest).virtual_state_task.await.expect("the honest anchoring beat");
    assert_eq!(s.chain.sink(), beat_hash, "the heartbeat that anchors the lane is the sink");
    let (_, state) = s.chain.tip_state();
    assert!(state.exec_v2_anchored_v1(&slice.header.hash), "covered once");
    assert_eq!(state.exec_v2_root_v1(&s.claim).unwrap().next_index, 1, "and credited once");
    assert_eq!(state.exec_v2_slice_v1(&s.claim, 0).unwrap().carrier, slice.header.hash);
}

/// **The fold judges every covered carrier; a refusal is a lane verdict, never an error of the block.** A slice by an executor the
/// declaration did not name, a slice that skips ahead and a slice borrowed from another job are carried and anchored — and credited nothing.
#[tokio::test]
async fn t12_exec_v2_an_unauthorised_a_skipping_and_a_borrowed_slice_are_anchored_and_credited_nothing() {
    let mut s = open_session(Some(FENCE)).await;
    let (mid, _) = s.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let _ = mid;
    let unauthorised = s.slice_block(5, 0, |_| {});
    let skipping = s.slice_block(1, 2, |_| {});
    let borrowed = s.slice_block(1, 0, |slice| slice.canonical_job_id = Hash64::from_u64_word(0xB0));
    // (the three share the anchor; each is its own lane block, none a parent of another)
    for (what, block) in [("unauthorised", &unauthorised), ("skipping", &skipping), ("borrowed", &borrowed)] {
        s.insert_lane_block(block, what).await;
    }
    let (anchoring, _) = s.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let trailer = palw_exec_v2_anchor_split(&anchoring.transactions[0].payload).unwrap().1.expect("anchors the lane");
    assert_eq!(trailer.count, 3, "all three are covered");
    let (_, state) = s.chain.tip_state();
    for block in [&unauthorised, &skipping, &borrowed] {
        assert!(state.exec_v2_anchored_v1(&block.header.hash), "covered once, whatever the fold decides");
    }
    let root = state.exec_v2_root_v1(&s.claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.pending), (0, 0, 0), "and credited nothing: {root:?}");
    assert!(state.exec_v2_slice_v1(&s.claim, 0).is_none() && state.exec_v2_slice_v1(&s.claim, 2).is_none());
}

/// **Below the fence nothing changes**: a `PXE2` header is refused by name, a root declaration is dropped and the block stands, and a chain
/// block's coinbase is untouched.
#[tokio::test]
async fn t12_exec_v2_below_the_fence_a_pxe2_header_is_refused_and_a_root_declaration_is_dropped() {
    kaspa_core::log::try_init_logger("warn");
    let (config, premine, floats) = config_with(None);
    let mut chain = chain_of(&config, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    for _ in 0..6 {
        chain.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim) = chain.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let daa = chain.daa_of(chain.sink());
    let mut declaration = declaration_for(&chain, claim, daa + 5_000);
    declaration.signature = sign(0, &declaration.signing_message(network_domain(&config)), PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT);
    let carrier = carrier_of(&config, &floats, Obj::ExecWorkRootOpenedV2 { declaration: Box::new(declaration) });
    let carrying = chain.heartbeat(ttpb, vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "the carrier is a valid transaction and the block stands");
    chain.heartbeat(ttpb, Vec::new()).await;
    let (_, state) = chain.tip_state();
    assert!(state.exec_v2_root_v1(&claim).is_none(), "the declaration was dropped by name below the fence");
    assert_eq!(state.exec_v2_counts_v1(), (0, 0, 0));
    // A PXE2 lane header: refused by the shape gate exactly as the build before RFC-0008 v2 refused it (the X8R review: the fence is not
    // armed, so the v2 envelope is never read — it is a malformed v1 one).
    let block = pxe2_tx_header_block(&mut chain, &config);
    let expected = pxe2_refusal_before_rfc8_v2(&block);
    let err =
        chain.ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.expect_err("below the fence");
    assert_eq!(err.to_string(), expected, "the pre-RFC-0008-v2 refusal, byte for byte");
    // And a chain block's coinbase carries no trailer, with or without lane blocks around.
    let (attempt, _) = chain.attempt(2, ttpb, Vec::new(), &|_| true).await;
    assert!(palw_exec_v2_anchor_split(&attempt.transactions[0].payload).unwrap().1.is_none());
}

/// A signed `PXE2` `EXEC_TX` header (the v1 adapter builds the block; the envelope is the v2 one) on `chain`'s sink — not inserted.
fn pxe2_tx_header_block(chain: &mut T12Chain, config: &Config) -> MutableBlock {
    let mut template = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .unwrap();
    stamp_harness_time(&config.params, &mut template.block.header, chain.ctx.simulated_time);
    let round = kaspa_consensus_core::palw_execution_lane_v1::palw_execution_round_v1(
        chain.ctx.simulated_time,
        config.params.genesis.timestamp,
    );
    let mut block = chain.vp().round_adapt_block_template(template, round, card_payout_spk(0)).unwrap().block;
    let anchor = chain.vp().ghostdag_store.get_selected_parent(chain.sink()).unwrap();
    block.header.nonce = 0;
    block.header.palw_commitment = Vec::new();
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let mut envelope = PalwExecV2Envelope {
        version: PALW_EXEC_V2_WIRE_VERSION,
        network_domain: network_domain(config),
        anchor,
        subtype: PalwExecSubtypeV2::Tx,
        tx_permit: Some(kaspa_consensus_core::palw_exec_v2::PalwExecTxPermitV2 { round, permit_index: 0 }),
        work_slice: None,
        payload_root: block.header.hash_merkle_root,
        executor_bond: chain.bonds[0],
        pubkey: pubkey(0),
        signature: vec![0; kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MLDSA87_SIGNATURE_LEN],
    };
    let message = envelope.signing_message(pre_pow, block.header.timestamp, 0).unwrap();
    envelope.signature = sign(0, &message, envelope.mldsa87_context());
    block.header.palw_commitment = envelope.encode();
    block.header.finalize();
    block
}

/// The refusal the build before RFC-0008 v2 gives a `PXE2` algo-10 header: its shape gate reads the bytes as a v1 envelope (the gate
/// that build ran is [`kaspa_consensus_core::pow_layer0::check_palw_commitment_shape_at`], unchanged).
fn pxe2_refusal_before_rfc8_v2(block: &MutableBlock) -> String {
    let shape = kaspa_consensus_core::pow_layer0::check_palw_commitment_shape_at(
        block.header.pow_algo_id,
        &block.header.palw_commitment,
        false,
        kaspa_consensus_core::pow_layer0::PalwAttemptLaneV1::Unfenced,
    )
    .expect_err("a PXE2 payload is no v1 envelope");
    kaspa_consensus_core::errors::block::RuleError::BadPalwCommitmentShape(shape.to_string()).to_string()
}

/// A heartbeat on `chain`'s sink whose miner tag ends in `tail` — built, not inserted. The coinbase payload is the template's with `tail`
/// appended (the miner's extra data is the payload's last field), the merkle root recomputed.
fn beat_with_tag_tail(chain: &mut T12Chain, config: &Config, nonce: u64, tail: &[u8]) -> MutableBlock {
    let ttpb = config.params.target_time_per_block();
    let mut template = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&config.params, &mut template.block.header, chain.ctx.simulated_time + ttpb);
    template.block.header.nonce = nonce;
    template.block.header.finalize();
    let mut beat = chain.vp().heartbeat_adapt_block_template(template).expect("the heartbeat lane is open").0.block;
    let mut payload = beat.transactions[0].payload.clone();
    payload.extend_from_slice(tail);
    beat.transactions[0].payload = payload;
    beat.transactions[0].finalize();
    beat.header.hash_merkle_root = kaspa_consensus_core::merkle::calc_hash_merkle_root(beat.transactions.iter());
    beat.header.finalize();
    beat
}

/// **Fence off, every RFC-0008 v2 object is judged as the build before it judged it — and a node with the fence armed far above the chain
/// agrees on every verdict and every root** (the X8R review's pin; spec section 9's "below / at / above activation", below half).
///
/// Node A runs testnet-12 with `palw_exec_payload_v2` unarmed. It carries, beside an ordinary chain (heartbeats, a REAL attempt): two
/// 0x4b carriers of a tag-130 root declaration — one well-formed and signed, one MALFORMED (no plan; before the review a ride-time
/// shape check refused its transaction, which the live build tolerates as undecodable) — both valid transactions, both dropped by name,
/// nothing folded; a heartbeat whose miner tag ends in a well-formed `PXA2` trailer, and its twin whose trailer magic is one byte off —
/// both refused with the same over-the-cap payload error (a trailer cannot fit beside the PQ-only 69-byte payout script under the
/// 204-byte cap, so the reader that extends the cap must not exist here); and a `PXE2` algo-10 header, refused by the shape gate with
/// the v1 decode's error. Node B (unarmed) and node C (armed at 1,000,000) replay every block: the same statuses, the same refusals, the
/// same sink, PALW state root and virtual UTXO multiset — with and without the lane objects (the comparison is made after the plain
/// chain and again after the lane objects).
#[tokio::test]
async fn t12_exec_v2_fence_off_every_lane_object_is_judged_as_before_and_an_armed_node_agrees() {
    use kaspa_consensus_core::errors::{block::RuleError, coinbase::CoinbaseError};
    kaspa_core::log::try_init_logger("warn");
    let (config, premine, floats) = config_with(None);
    let (armed_far, _, _) = config_with(Some(1_000_000));
    let mut a = chain_of(&config, &premine, &floats);
    let ttpb = config.params.target_time_per_block();

    // Every node's view, as the comparison reads it.
    fn view(chain: &T12Chain) -> (BlockHash, Hash64, kaspa_hashes::Hash64) {
        let (_, state) = chain.tip_state();
        let multiset = chain.vp().virtual_stores.read().state.get().unwrap().multiset.clone().finalize();
        (chain.sink(), state.state_root(), multiset)
    }
    async fn replay(
        config: &Config,
        premine: &[(TransactionOutpoint, UtxoEntry)],
        floats: &[(TransactionOutpoint, UtxoEntry)],
        blocks: &[Block],
        refused: &[(MutableBlock, String)],
    ) -> T12Chain {
        let node = chain_of(config, premine, floats);
        for block in blocks {
            let hash = block.header.hash;
            node.ctx
                .consensus
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .unwrap_or_else(|e| panic!("block {hash}: {e}"));
        }
        for (block, why) in refused {
            let err = node
                .ctx
                .consensus
                .validate_and_insert_block(block.clone().to_immutable())
                .virtual_state_task
                .await
                .expect_err("refused on every node");
            assert_eq!(&err.to_string(), why, "the same refusal");
        }
        node
    }

    // ---- the plain chain: heartbeats and a REAL attempt ----
    for _ in 0..6 {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim) = a.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let plain = selected_chain_blocks(&a, a.sink());
    for other in [&config, &armed_far] {
        let node = replay(other, &premine, &floats, &plain, &[]).await;
        assert_eq!(view(&node), view(&a), "without the lane objects");
    }

    // ---- the lane objects ----
    let daa = a.daa_of(a.sink());
    let mut good = declaration_for(&a, claim, daa + 5_000);
    good.signature = sign(0, &good.signing_message(network_domain(&config)), PALW_EXEC_V2_ROOT_MLDSA87_CONTEXT);
    let mut malformed = good.clone();
    malformed.boundaries.clear();
    assert!(malformed.validate_shape().is_err());
    let good_carrier = carrier_of(&config, &floats, Obj::ExecWorkRootOpenedV2 { declaration: Box::new(good) });
    let malformed_carrier = carrier_by(&config, &floats, 1, Obj::ExecWorkRootOpenedV2 { declaration: Box::new(malformed) });
    let carrying = a.heartbeat(ttpb, vec![good_carrier.clone(), malformed_carrier.clone()]).await;
    for carrier in [&good_carrier, &malformed_carrier] {
        assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()), "a valid transaction, carried");
    }
    a.heartbeat(ttpb, Vec::new()).await; // accepts them
    let (_, state) = a.tip_state();
    assert!(state.exec_v2_root_v1(&claim).is_none() && state.exec_v2_counts_v1() == (0, 0, 0), "dropped by name: nothing folded");

    // A trailer in a miner's tag — and its twin one byte off the magic: the same refusal, the build before's.
    let cap = config.params.max_coinbase_payload_len;
    let trailer = PalwExecV2AnchorV1 {
        heads: vec![a.vp().ghostdag_store.get_selected_parent(a.sink()).unwrap()],
        count: 1,
        root: Hash64::from_u64_word(0xA2),
    }
    .trailer();
    let mut off_by_one = trailer.clone();
    *off_by_one.last_mut().unwrap() ^= 1;
    let mut refused: Vec<(MutableBlock, String)> = Vec::new();
    for (nonce, tail) in [(0xA0, trailer), (0xA1, off_by_one)] {
        let beat = beat_with_tag_tail(&mut a, &config, nonce, &tail);
        let len = beat.transactions[0].payload.len();
        assert!(len > cap, "a trailer does not fit beside the 69-byte payout script: {len} > {cap}");
        let err =
            a.ctx.consensus.validate_and_insert_block(beat.clone().to_immutable()).virtual_state_task.await.expect_err("over the cap");
        assert_eq!(
            err.to_string(),
            RuleError::BadCoinbasePayload(CoinbaseError::PayloadLenAboveMax(len, cap)).to_string(),
            "the cap is the cap, trailer or not"
        );
        refused.push((beat, err.to_string()));
    }

    // A PXE2 header: the shape gate's v1 refusal.
    let pxe2 = pxe2_tx_header_block(&mut a, &config);
    let expected = pxe2_refusal_before_rfc8_v2(&pxe2);
    let err = a.ctx.consensus.validate_and_insert_block(pxe2.clone().to_immutable()).virtual_state_task.await.expect_err("no carrier");
    assert_eq!(err.to_string(), expected);
    refused.push((pxe2, expected));

    // ---- every node agrees, with the lane objects ----
    let with_objects = selected_chain_blocks(&a, a.sink());
    for other in [&config, &armed_far] {
        let node = replay(other, &premine, &floats, &with_objects, &refused).await;
        assert_eq!(view(&node), view(&a), "with the lane objects");
        for block in &with_objects {
            assert_eq!(node.ctx.consensus.block_status(block.header.hash), a.ctx.consensus.block_status(block.header.hash));
        }
    }
}

// =============================================================================================
// EXEC_TX: the existing permit lane, carried by the same anchor
// =============================================================================================

/// A signed `PXE2` `EXEC_TX` block for `(round, permit_index)` by card 0 (the executor), carrying `spends`, hung from the sink's selected
/// parent. Not inserted.
fn tx_block(at: &mut AtTheTargetSpan, config: &Config, round: u64, permit_index: u16, spends: Vec<Transaction>) -> MutableBlock {
    let template = at
        .chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(spends.clone())), TemplateBuildMode::Standard)
        .expect("a template carrying the spends");
    let adapted = at.chain.vp().round_adapt_block_template(template, round, card_payout_spk(0)).expect("the lane adapts a template");
    let anchor = adapted.selected_parent_hash;
    let mut block = adapted.block;
    for spend in &spends {
        assert!(block.transactions.iter().any(|tx| tx.id() == spend.id()), "the lane block carries the spend");
    }
    block.header.nonce = 0;
    block.header.palw_commitment = Vec::new();
    let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&block.header);
    let mut envelope = PalwExecV2Envelope {
        version: PALW_EXEC_V2_WIRE_VERSION,
        network_domain: network_domain(config),
        anchor,
        subtype: PalwExecSubtypeV2::Tx,
        tx_permit: Some(kaspa_consensus_core::palw_exec_v2::PalwExecTxPermitV2 { round, permit_index }),
        work_slice: None,
        // The existing transaction batch: the header's own merkle root.
        payload_root: block.header.hash_merkle_root,
        executor_bond: at.executor,
        pubkey: pubkey(0),
        signature: vec![0; kaspa_consensus_core::palw_execution_lane_v1::PALW_EXEC_MLDSA87_SIGNATURE_LEN],
    };
    let message = envelope.signing_message(pre_pow, block.header.timestamp, block.header.nonce).expect("a permit payload");
    envelope.signature = sign(0, &message, envelope.mldsa87_context());
    block.header.palw_commitment = envelope.encode();
    block.header.finalize();
    block
}

/// **An `EXEC_TX` block carried by the anchor**: the permit is judged against the parent state's schedule and spent once; its spend is
/// accepted and its fee paid to the bond's payout by the anchoring chain block's coinbase — the existing lane's money, through the new
/// carriage — and the block is in no stored mergeset. A second block of the SAME permit, and a block for a round the schedule does not
/// grant, are covered and refused as lane verdicts (never as an error of the chain block): one spend, one fee, one permit.
#[tokio::test]
async fn t12_exec_v2_a_permitted_tx_block_is_anchored_its_fee_paid_once_and_a_duplicate_permit_or_an_ungranted_round_is_refused() {
    use kaspa_consensus_core::palw_execution_lane_v1::{PALW_EXEC_ROUND_MS, palw_execution_permits_v1};
    use kaspa_consensus_core::palw_execution_quanta_v1::PALW_EXEC_TICKET_LEAD_ROUNDS_V1;
    const FEE: u64 = 500_000;
    const FEE_OTHER: u64 = 400_000;
    const FENCE_TX: u64 = 50;
    let mut at = a_floor_final_scheduled_with(SeedDraw::MintsATicket, Some(FENCE_TX)).await;
    let config = at.chain.config.clone();
    assert!(config.params.palw_exec_payload_v2_active_at(at.chain.daa_of(at.chain.sink())), "the fence is crossed");
    let mine: Vec<_> = at.schedule.quanta.iter().filter(|q| q.final_id == at.claim_id).copied().collect();
    let ticket = mine.iter().min_by_key(|q| q.scheduled_round).copied().expect("the Final minted a ticket");
    let round = ticket.scheduled_round;
    let width = at.lane.width_of_span_len(at.target, at.span_daa);
    let permit = palw_execution_permits_v1(&at.schedule, round, width).into_iter().find(|p| p.bond == at.executor).expect("a permit");
    let untended = at.open_round + PALW_EXEC_TICKET_LEAD_ROUNDS_V1 + at.window + 5;
    assert!(palw_execution_permits_v1(&at.schedule, untended, width).is_empty(), "a round no ticket holds has no permit");

    // The anchor beneath the sink is the opening block.
    at.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let opening = at.opening;
    assert_eq!(at.chain.vp().ghostdag_store.get_selected_parent(at.chain.sink()).unwrap(), opening);

    // Three spends: the permitted block's, a duplicate permit's, an ungranted round's.
    let utxos: std::collections::HashMap<_, _> =
        at.chain.ctx.consensus.get_virtual_utxos(None, 1_000_000, false).into_iter().collect();
    let change = TransactionOutpoint::new(at.carrier.id(), 0);
    let change_entry = utxos.get(&change).cloned().expect("the carrier's change is unspent");
    let (_, _, _, floats) = {
        let (c, b, p, f) = t12_with_harness_cards();
        (c, b, p, f)
    };
    let spend_from = |outpoint: TransactionOutpoint, entry: UtxoEntry, card: usize, fee: u64| {
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(entry.amount - fee, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        sign_spend(&mut tx, entry, card, config.params.storage_mass_parameter);
        tx
    };
    let paying = spend_from(change, change_entry, 0, FEE);
    let (f1_out, f1_entry) = floats[1].clone();
    let duplicate_spend = spend_from(f1_out, f1_entry, 1, FEE_OTHER);
    let (f2_out, f2_entry) = floats[2].clone();
    let ungranted_spend = spend_from(f2_out, f2_entry, 2, FEE_OTHER);

    let sink_before = at.chain.sink();
    let numbers_before = numbers_of(&at.chain, sink_before);
    let good = tx_block(&mut at, &config, round, permit.index, vec![paying.clone()]);
    let duplicate = tx_block(&mut at, &config, round, permit.index, vec![duplicate_spend.clone()]);
    let ungranted = tx_block(&mut at, &config, untended, 0, vec![ungranted_spend.clone()]);
    assert_ne!(good.header.hash, duplicate.header.hash, "two blocks of one permit");
    for (what, block) in [
        ("the permitted TX block", &good),
        ("a duplicate of its permit", &duplicate),
        ("a TX block for a round with no permit", &ungranted),
    ] {
        let hash = block.header.hash;
        at.chain
            .ctx
            .consensus
            .validate_and_insert_block(block.clone().to_immutable())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("{what} {hash} was refused at the door: {e}"));
        assert_eq!(at.chain.sink(), sink_before, "{what} never moves the sink");
    }
    // The node's own verdict view, before anything anchors them: the virtual covers them.
    at.chain.ctx.simulated_time =
        at.chain.ctx.simulated_time.max(config.params.genesis.timestamp + (untended + 1) * PALW_EXEC_ROUND_MS);
    let (merging, _) = at.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let trailer = palw_exec_v2_anchor_split(&merging.transactions[0].payload).unwrap().1.expect("the chain block anchors the lane");
    assert_eq!(trailer.count, 3, "all three EXEC blocks are covered");

    let vp = at.chain.vp();
    let data = vp.ghostdag_store.get_data(merging.header.hash).unwrap();
    for block in [&good, &duplicate, &ungranted] {
        assert!(
            !data.mergeset_blues.contains(&block.header.hash) && !data.mergeset_reds.contains(&block.header.hash),
            "in no stored mergeset"
        );
    }
    // Weightless: the chain block is the next block, one blue score on, by the attempt lane's own constant work.
    let numbers_after = numbers_of(&at.chain, merging.header.hash);
    assert_eq!(numbers_after.mergeset_size, 1, "the stored mergeset is the selected parent alone");
    assert!(numbers_after.blue_score > numbers_before.blue_score);

    // The permit is spent once, the permitted spend is accepted and paid, the others are not.
    let (_, state) = at.chain.tip_state();
    assert!(state.round_permit_used(at.target, round, permit.index), "the permit is recorded as used");
    assert!(!state.round_permit_used(at.target, untended, 0));
    assert_eq!(state.round_permits_accepted(at.target), 1, "one permit accepted in the span");
    for block in [&good, &duplicate, &ungranted] {
        assert!(state.exec_v2_anchored_v1(&block.header.hash), "covered once");
    }
    let accepted: Vec<_> = at
        .chain
        .ctx
        .consensus
        .get_block_acceptance_data(merging.header.hash)
        .unwrap()
        .iter()
        .flat_map(|m| m.accepted_transactions.iter().map(|e| e.transaction_id).collect::<Vec<_>>())
        .collect();
    // Two blocks claim ONE permit: the covered set is judged in canonical order — (key, block hash) — so the LOWER hash takes it, on
    // every node, whichever the miner called the "permitted" one (the signatures are randomised, so the hashes differ run to run).
    let (winner_spend, winner_fee, loser_spend, loser_fee) = if good.header.hash < duplicate.header.hash {
        (&paying, FEE, &duplicate_spend, FEE_OTHER)
    } else {
        (&duplicate_spend, FEE_OTHER, &paying, FEE)
    };
    assert!(accepted.contains(&winner_spend.id()), "the block that sorts first takes the permit: its spend is accepted");
    assert!(!accepted.contains(&loser_spend.id()), "the other block of the permit has its spend refused");
    assert!(!accepted.contains(&ungranted_spend.id()), "the ungranted round's spend is not");
    let payout = card_payout_spk(0);
    let paid: u64 = merging.transactions[0].outputs.iter().filter(|o| o.script_public_key == payout).map(|o| o.value).sum();
    assert!(
        merging.transactions[0].outputs.iter().any(|o| o.value == winner_fee && o.script_public_key == payout),
        "the fee is paid to the bond's registered payout (paid {paid}): {:?}",
        merging.transactions[0].outputs.iter().map(|o| o.value).collect::<Vec<_>>()
    );
    assert!(
        !merging.transactions[0].outputs.iter().any(|o| o.value == loser_fee && o.script_public_key == payout),
        "and the fee of the permit's other block is not paid"
    );
    assert!(
        !merging.transactions[0].outputs.iter().any(|o| o.value == FEE_OTHER && o.script_public_key == card_payout_spk(1)),
        "nor any fee for the ungranted round"
    );
}

// =============================================================================================
// Restart, replay and IBD: the lane survives what a node goes through
// =============================================================================================

/// What `anchor_two_slices` built: the chain block beneath the lane, the two EXEC blocks, and the chain block that anchors them.
struct TwoAnchored {
    mid: Block,
    s0: MutableBlock,
    s1: MutableBlock,
    anchoring: Block,
}

/// The main scenario's blocks: a quiet chain block, slice 0 (card 1) and slice 1 (card 2) as EXEC blocks, then card 2's attempt, which
/// anchors both.
async fn anchor_two_slices(lane: &mut Session) -> TwoAnchored {
    let (mid, _) = lane.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let s0 = lane.slice_block(1, 0, |_| {});
    lane.insert_lane_block(&s0, "slice 0").await;
    // The predecessor of slice 1 is slice 0's result, which nothing has folded yet: it is supplied by hand.
    let s1 = lane.slice_block(2, 1, |slice| slice.predecessor_state_root = Hash64::from_u64_word(0x6000));
    lane.insert_lane_block(&s1, "slice 1").await;
    let (anchoring, _) = lane.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let (_, after) = lane.chain.tip_state();
    assert!(after.exec_v2_anchored_v1(&s0.header.hash) && after.exec_v2_anchored_v1(&s1.header.hash), "both are anchored");
    TwoAnchored { mid, s0, s1, anchoring }
}

/// The selected chain from genesis (exclusive) to `upto` (inclusive), as full blocks.
fn selected_chain_blocks(chain: &T12Chain, upto: BlockHash) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let mut hashes = Vec::new();
    let mut at = upto;
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|h| chain.ctx.consensus.get_block(h).expect("the node holds its chain")).collect()
}

/// **A real restart over the same database, then a replay by a second node.** The first node carries a root, two anchored slices and the
/// chain blocks around them; it is stopped; a new `Consensus` opens the SAME database. What it knows of the lane — the sink, the PALW tip
/// with its `exec_v2` tables, the lane blocks' bodies — came off disk. It then carries on (a third slice, a new anchor), and a node that
/// replays the whole chain agrees with every root: a chain block that reaches it BEFORE the lane blocks it anchors is held back with a
/// retryable `MissingParents` naming the head, and lands once they are in.
#[tokio::test]
async fn t12_exec_v2_lane_state_survives_a_restart_and_a_replaying_node_agrees_whatever_the_arrival_order() {
    use kaspa_consensus_core::errors::block::RuleError;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    kaspa_core::log::try_init_logger("warn");
    let (config, premine, floats) = config_with(Some(FENCE));
    let PalwConsensusMode::ConsensusV2(bundle) = config.params.palw_consensus_mode.clone() else { unreachable!("testnet-12 is V2") };
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _rx) = async_channel::unbounded();
    let first = crate::consensus::test_consensus::TestConsensus::with_db(db.clone(), &config, sender);
    let chain = super::t12_round_lane_e2e::t12_genesis_chain_on(first, &config, &bundle, &premine, &floats);
    let mut lane = open_session_on(config.clone(), floats.clone(), chain).await;
    let claim = lane.claim;
    let a = anchor_two_slices(&mut lane).await;

    let sink = lane.chain.sink();
    assert_eq!(sink, a.anchoring.header.hash);
    let (_, state_before) = lane.chain.tip_state();
    let (root_before, s0_row, s1_row) = (
        state_before.exec_v2_root_v1(&claim).cloned().expect("the root"),
        state_before.exec_v2_slice_v1(&claim, 0).cloned().expect("slice 0"),
        state_before.exec_v2_slice_v1(&claim, 1).cloned().expect("slice 1"),
    );
    let (simulated_time, nonce) = (lane.chain.ctx.simulated_time, lane.chain.nonce_for_reopen());

    // ---- stop the node ----
    drop(lane.chain);
    // ---- start it again on the same database (as a restarting node is configured: its genesis is already in) ----
    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, _rx2) = async_channel::unbounded();
    let second = crate::consensus::test_consensus::TestConsensus::with_db(db.clone(), &resumed, sender);
    let r = super::t12_round_lane_e2e::t12_reopened_chain(second, &resumed, &bundle, simulated_time, nonce);
    assert_eq!(r.sink(), sink, "the restarted node's sink is the stopped node's");
    let (tip, state) = r.tip_state();
    assert_eq!((tip, state.state_root()), (sink, state_before.state_root()), "the PALW tip it loads off disk");
    assert_eq!(state.exec_v2_root_v1(&claim), Some(&root_before), "the work root, off disk");
    assert_eq!(state.exec_v2_slice_v1(&claim, 0), Some(&s0_row));
    assert_eq!(state.exec_v2_slice_v1(&claim, 1), Some(&s1_row));
    assert!(state.exec_v2_anchored_v1(&a.s0.header.hash) && state.exec_v2_anchored_v1(&a.s1.header.hash), "the covered set, off disk");
    for lane_block in [&a.s0, &a.s1] {
        let held = r.ctx.consensus.get_block(lane_block.header.hash).expect("the lane block's body is on disk");
        assert_eq!(held.header.pow_algo_id, kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1);
        assert_ne!(r.ctx.consensus.block_status(lane_block.header.hash), BlockStatus::StatusInvalid);
    }

    // ---- it carries on: slice 2, then a chain block that anchors it ----
    let mut resumed_lane = Session { chain: r, config: resumed.clone(), claim, declaration: lane.declaration, floats: lane.floats };
    let s2 = resumed_lane.slice_block(1, 2, |_| {});
    resumed_lane.insert_lane_block(&s2, "slice 2 after the restart").await;
    let (anchoring2, _) = resumed_lane.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let (_, later) = resumed_lane.chain.tip_state();
    assert!(later.exec_v2_anchored_v1(&s2.header.hash), "the restarted node anchors the new lane block");
    let trailer = palw_exec_v2_anchor_split(&anchoring2.transactions[0].payload).unwrap().1.expect("anchors");
    assert_eq!(trailer.count, 1, "only the new block: the two before the restart are not covered again");
    let root_after = later.exec_v2_root_v1(&claim).unwrap();
    assert_eq!((root_after.next_index, root_after.accepted_work, root_after.pending), (3, 3 * SLICE_WORK, 3), "{root_after:?}");

    // ---- a node replaying the whole chain agrees ----
    let z = chain_of(&config, &premine, &floats);
    let chain_blocks = selected_chain_blocks(&resumed_lane.chain, resumed_lane.chain.sink());
    let (first_anchor, second_anchor) = (a.anchoring.header.hash, anchoring2.header.hash);
    let mut replayed_early = false;
    for block in chain_blocks {
        let hash = block.header.hash;
        if hash == first_anchor {
            // The chain block ahead of the lane blocks it anchors: held back by name, retryable.
            let err = z
                .ctx
                .consensus
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .expect_err("its anchored heads are not here yet");
            match err {
                RuleError::MissingParents(missing) => assert!(missing.contains(&a.s1.header.hash), "names the head: {missing:?}"),
                other => panic!("expected a retryable MissingParents, got {other:?}"),
            }
            replayed_early = true;
            for lane_block in [&a.s0, &a.s1] {
                z.ctx
                    .consensus
                    .validate_and_insert_block(lane_block.clone().to_immutable())
                    .virtual_state_task
                    .await
                    .expect("a lane block arrives");
            }
        }
        if hash == second_anchor {
            z.ctx.consensus.validate_and_insert_block(s2.clone().to_immutable()).virtual_state_task.await.expect("slice 2 arrives");
        }
        z.ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("chain block {hash}: {e}"));
    }
    assert!(replayed_early);
    assert_eq!(z.sink(), resumed_lane.chain.sink(), "the replaying node reaches the restarted node's sink");
    let (_, z_state) = z.tip_state();
    assert_eq!(z_state.state_root(), later.state_root(), "and its PALW state root");
    assert_eq!(z_state.exec_v2_root_v1(&claim), later.exec_v2_root_v1(&claim));
    for index in 0..3 {
        assert_eq!(z_state.exec_v2_slice_v1(&claim, index), later.exec_v2_slice_v1(&claim, index), "slice {index}");
    }
    for chain_block in [a.mid.header.hash, first_anchor, second_anchor] {
        assert_eq!(numbers_of(&z, chain_block), numbers_of(&resumed_lane.chain, chain_block), "the numbers of {chain_block}");
    }
}

/// **IBD of a lane the chain anchors.** No block names an EXEC block as a parent, so the two sync lists carry hooks: the syncer's
/// header list puts the blocks a chain block anchors beside its mergeset (before it), and the syncee's body list requests the
/// header-only lane blocks hanging off what it holds (before the chain block that anchors them). A syncee fed the list WITHOUT the
/// lane — what an un-hooked node serves — lands every header but is refused the anchoring body by name; fed as this build serves it,
/// it lands every body and reaches the source's sink and roots.
#[tokio::test]
async fn t12_exec_v2_ibd_carries_the_anchored_lane_blocks_through_both_sync_lists() {
    use crate::model::stores::ghostdag::GhostdagStoreReader as _;
    use kaspa_consensus_core::errors::block::RuleError;
    use kaspa_consensus_core::topological_order::is_parent_first;
    use std::ops::Deref;
    kaspa_core::log::try_init_logger("warn");
    let mut lane = open_session(Some(FENCE)).await;
    let claim = lane.claim;
    let a = anchor_two_slices(&mut lane).await;
    let (config, premine, floats) = config_with(Some(FENCE));
    let source = &lane.chain.ctx.consensus;
    let genesis = config.params.genesis.hash;
    let sink = lane.chain.sink();
    let (s0, s1) = (a.s0.header.hash, a.s1.header.hash);

    // The syncer's header list: parents-first, with the lane blocks in it, ahead of the chain block that anchors them.
    let (served, highest) = source.get_hashes_between(genesis, sink, 1 << 20).unwrap();
    assert_eq!(highest, sink);
    let position = |hash: BlockHash| served.iter().position(|h| *h == hash);
    let (p0, p1, pa) = (position(s0).expect("slice 0 is listed"), position(s1).expect("slice 1 is listed"), position(sink).unwrap());
    assert!(p0 < pa && p1 < pa, "the lane blocks are listed before the chain block that anchors them");
    assert!(
        is_parent_first(&served, |h| *h, |h| source.get_header(*h).unwrap().direct_parents().to_vec()),
        "parents-first throughout"
    );

    // What an un-hooked syncer serves: each chain block's mergeset, in consensus order, and nothing else.
    let store = source.ghostdag_store();
    let mut selected = Vec::new();
    let mut at = sink;
    while at != genesis {
        selected.push(at);
        at = store.get_selected_parent(at).unwrap();
    }
    selected.reverse();
    let mut unhooked: Vec<BlockHash> = Vec::new();
    for chain_block in &selected {
        unhooked.extend(store.get_data(*chain_block).unwrap().consensus_ordered_mergeset(store.deref()).filter(|h| *h != genesis));
    }
    unhooked.push(sink);
    assert!(!unhooked.contains(&s0) && !unhooked.contains(&s1), "a mergeset lists no EXEC block: nothing names it");

    // 1. The un-hooked list: every header lands; the anchoring block's body is refused by name, retryably.
    let before = chain_of(&config, &premine, &floats);
    for hash in &unhooked {
        let header = source.get_header(*hash).unwrap();
        before
            .ctx
            .consensus
            .validate_and_insert_block(Block::from_header_arc(header))
            .virtual_state_task
            .await
            .expect("a header lands");
    }
    let bodies = before.ctx.consensus.get_missing_block_body_hashes(sink).unwrap();
    // The body list this build serves already carries the lane blocks it can see (headers only: none were listed), so it stops
    // short of them; the anchoring block alone is what cannot land.
    let mut refused = None;
    for hash in bodies {
        let block = source.get_block(hash).unwrap();
        match before.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await {
            Ok(_) => {}
            Err(RuleError::MissingParents(missing)) => {
                refused = Some((hash, missing));
                break;
            }
            Err(other) => panic!("body of {hash}: {other:?}"),
        }
    }
    let (refused_at, missing) = refused.expect("the anchoring body cannot land without its lane blocks");
    assert_eq!(refused_at, sink, "it is the anchoring chain block");
    assert!(missing.contains(&s0) || missing.contains(&s1), "names a lane head: {missing:?}");

    // 2. As this build serves and requests it.
    let after = chain_of(&config, &premine, &floats);
    for hash in &served {
        let header = source.get_header(*hash).unwrap();
        after
            .ctx
            .consensus
            .validate_and_insert_block(Block::from_header_arc(header))
            .virtual_state_task
            .await
            .expect("a header lands");
    }
    let missing_bodies = after.ctx.consensus.get_missing_block_body_hashes(sink).unwrap();
    let (b0, b1, bs) = (
        missing_bodies.iter().position(|h| *h == s0).expect("slice 0's body is requested"),
        missing_bodies.iter().position(|h| *h == s1).expect("slice 1's body is requested"),
        missing_bodies.iter().position(|h| *h == sink).expect("the sink's body is requested"),
    );
    assert!(b0 < bs && b1 < bs, "the lane bodies are requested before the chain block that anchors them");
    assert!(
        is_parent_first(&missing_bodies, |h| *h, |h| source.get_header(*h).unwrap().direct_parents().to_vec()),
        "the body list is parents-first"
    );
    for hash in &missing_bodies {
        let block = source.get_block(*hash).unwrap();
        after
            .ctx
            .consensus
            .validate_and_insert_block(block)
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("body of {hash}: {e:?}"));
    }
    assert_eq!(after.sink(), sink, "the synced node's sink is the source's");
    assert_eq!(after.ctx.consensus.block_status(sink), BlockStatus::StatusUTXOValid);
    let (_, synced) = after.tip_state();
    let (_, original) = lane.chain.tip_state();
    assert_eq!(synced.state_root(), original.state_root(), "the synced PALW state is the source's");
    assert_eq!(synced.exec_v2_root_v1(&claim), original.exec_v2_root_v1(&claim));
    assert!(synced.exec_v2_anchored_v1(&s0) && synced.exec_v2_anchored_v1(&s1), "the covered set is folded the same");
    assert_eq!(numbers_of(&after, sink), numbers_of(&lane.chain, sink), "and the numbers");
}

/// **A burst of EXEC blocks moves nothing.** Fourteen lane blocks arrive back to back, every one claiming slice 0 of the same root
/// with a different result (a flood — any key can mint a lane block at the constant target): each is accepted into the DAG as a tip
/// without moving the sink, a chain block anchors the whole burst in one trailer, the fold credits exactly ONE of them (one accepted use
/// per `(root, index)`) and the root books exactly one slice's work, and every number on the chain — blue score, blue work, DAA score,
/// bits, pruning point, mergeset size — is what it is on a twin that never saw the burst.
#[tokio::test]
async fn t12_exec_v2_a_burst_of_competing_slice_blocks_is_anchored_once_credited_once_and_weightless() {
    const BURST: u64 = 14;
    let mut lane = open_session(Some(FENCE)).await;
    let mut twin = open_session(Some(FENCE)).await;
    let (mid, _) = lane.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let (twin_mid, _) = twin.chain.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let mut burst = Vec::new();
    for i in 0..BURST {
        // Cards 1 and 2 are both authorised; each submits a different result for slice 0.
        let block = lane.slice_block(1 + (i % 2) as usize, 0, |slice| slice.result_state_root = Hash64::from_u64_word(0xF000 + i));
        lane.insert_lane_block(&block, "a burst block").await;
        burst.push(block.header.hash);
    }
    assert_eq!(lane.chain.sink(), mid.header.hash, "fourteen lane blocks, and the sink is where it was");
    // The node's own view of its lane, before anything anchors it: the whole burst is pending under a live head.
    {
        let (tip, state) = lane.chain.tip_state();
        let health = lane.chain.vp().palw_exec_v2_health(&state, tip, lane.chain.daa_of(tip) + 1).expect("the payload is in force");
        assert_eq!((health.pending, health.stale), (BURST, false), "{health:?}");
        assert!(health.latest_head.is_some_and(|head| burst.contains(&head)));
    }
    let (anchoring, _) = lane.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;
    let (twin_anchoring, _) = twin.chain.attempt(2, 1_000, Vec::new(), &|_| true).await;

    let (_, after) = lane.chain.tip_state();
    let trailer = palw_exec_v2_anchor_split(&anchoring.transactions[0].payload).unwrap().1.expect("the chain block anchors the burst");
    assert_eq!(trailer.count as u64, BURST, "the whole burst is covered by the one trailer");
    assert!(trailer.heads.len() <= kaspa_consensus_core::palw_exec_v2_anchor::PALW_EXEC_V2_MAX_HEADS);
    for hash in &burst {
        assert!(after.exec_v2_anchored_v1(hash), "{hash} is in the covered set");
    }
    let root = after.exec_v2_root_v1(&lane.claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.pending), (1, SLICE_WORK, 1), "one credit for the whole burst: {root:?}");
    let credited = after.exec_v2_slice_v1(&lane.claim, 0).unwrap().carrier;
    assert!(burst.contains(&credited), "the credited slice is one of the burst's");

    let data = lane.chain.vp().ghostdag_store.get_data(anchoring.header.hash).unwrap();
    assert!(burst.iter().all(|h| !data.mergeset_blues.contains(h) && !data.mergeset_reds.contains(h)), "in no stored mergeset");
    assert_eq!(numbers_of(&lane.chain, mid.header.hash), numbers_of(&twin.chain, twin_mid.header.hash));
    assert_eq!(
        numbers_of(&lane.chain, anchoring.header.hash),
        numbers_of(&twin.chain, twin_anchoring.header.hash),
        "no number on the chain moved for the burst"
    );
    let (_, twin_after) = twin.chain.tip_state();
    assert_eq!(after.safe_weight(), twin_after.safe_weight(), "no PALW weight");
    {
        let (tip, state) = lane.chain.tip_state();
        let health = lane.chain.vp().palw_exec_v2_health(&state, tip, lane.chain.daa_of(tip) + 1).expect("in force");
        assert_eq!(health.pending, 0, "nothing is left to anchor: {health:?}");
    }
    // A second chain block anchors nothing new: the covered set is not covered twice.
    let (next, _) = lane.chain.attempt(1, 1_000, Vec::new(), &|_| true).await;
    assert!(palw_exec_v2_anchor_split(&next.transactions[0].payload).unwrap().1.is_none(), "the burst is anchored once");
}

/// **A reorg across a slice's acceptance.** Node 1 anchors slice 0 on branch X (a chain block over the lane block); node 2, which
/// shares X's prefix, mints a longer branch Y that never saw the lane block. Node 1 takes Y: its state at the new sink is the
/// state BEFORE the slice was accepted (the root has taken nothing, the covered set does not hold the lane block), the lane block —
/// still a body tip nothing named — is anchored by the next chain block on Y, and the slice is credited exactly once, on Y. A third
/// party that receives both branches agrees: the credit is branch-local, never doubled, never lost.
#[tokio::test]
async fn t12_exec_v2_a_reorg_unanchors_the_lane_and_the_winning_branch_anchors_and_credits_it_once() {
    let mut a = open_session(Some(FENCE)).await;
    let claim = a.claim;
    let ttpb = a.config.params.target_time_per_block();
    let (config, premine, floats) = config_with(Some(FENCE));
    // Node 2: the same chain up to the point of divergence.
    let mut b = chain_of(&config, &premine, &floats);
    for block in selected_chain_blocks(&a.chain, a.chain.sink()) {
        b.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.expect("the prefix replays");
    }
    let base = a.chain.sink();
    assert_eq!(b.sink(), base, "both nodes stand at the divergence");
    b.ctx.simulated_time = a.chain.ctx.simulated_time;
    b.set_nonce_for_fork(a.chain.nonce_for_reopen() + 1_000);

    // Branch X (node 1): slice 0 as a lane block, then a chain block over it that anchors it.
    let s0 = a.slice_block(1, 0, |_| {});
    a.insert_lane_block(&s0, "slice 0").await;
    let x1 = a.chain.heartbeat(ttpb, Vec::new()).await;
    let trailer = palw_exec_v2_anchor_split(&x1.transactions[0].payload).unwrap().1.expect("X1 anchors the lane block");
    assert_eq!(trailer.count, 1);
    {
        let (_, state) = a.chain.tip_state();
        assert!(state.exec_v2_anchored_v1(&s0.header.hash));
        assert_eq!(state.exec_v2_slice_v1(&claim, 0).map(|row| row.carrier), Some(s0.header.hash), "credited on X");
    }

    // Branch Y (node 2): two chain blocks from the same base, neither knowing the lane block.
    // (Chain weight is what attempts certify, not heartbeats: branch Y opens with an attempt so it outweighs X whatever the hashes.)
    let (y1, _) = b.attempt(3, 1_000, Vec::new(), &|_| true).await;
    let y2 = b.heartbeat(ttpb, Vec::new()).await;
    assert!(palw_exec_v2_anchor_split(&y1.transactions[0].payload).unwrap().1.is_none() && !y2.transactions.is_empty());
    for block in [&y1, &y2] {
        a.chain
            .ctx
            .consensus
            .validate_and_insert_block(block.clone())
            .virtual_state_task
            .await
            .unwrap_or_else(|e| panic!("branch Y block {}: {e}", block.header.hash));
    }
    assert_eq!(a.chain.sink(), y2.header.hash, "the longer branch wins: node 1 reorganises onto Y");
    {
        let (tip, state) = a.chain.tip_state();
        assert_eq!(tip, y2.header.hash);
        assert!(!state.exec_v2_anchored_v1(&s0.header.hash), "on Y the lane block is not covered yet");
        assert!(state.exec_v2_slice_v1(&claim, 0).is_none(), "and the slice is not credited");
        let root = state.exec_v2_root_v1(&claim).unwrap();
        assert_eq!((root.next_index, root.accepted_work, root.pending), (0, 0, 0), "{root:?}");
    }

    // By the lane's design the anchoring window is the block's span and the one before (a span is one DAA on testnet-12), so the lane
    // block — anchored on the stranded branch — has left it: the next chain block on Y covers NOTHING (no trailer, no write), the node's
    // own health view calls the head stale, and the root stays untouched. Nothing is invalidated and nothing is credited twice.
    let y3 = a.chain.heartbeat(ttpb, Vec::new()).await;
    assert_eq!(a.chain.sink(), y3.header.hash);
    assert!(
        palw_exec_v2_anchor_split(&y3.transactions[0].payload).unwrap().1.is_none(),
        "a lane block out of the window is not covered"
    );
    {
        let (tip, state) = a.chain.tip_state();
        assert!(!state.exec_v2_anchored_v1(&s0.header.hash));
        assert_eq!(state.exec_v2_root_v1(&claim).map(|root| root.next_index), Some(0));
        let health = a.chain.vp().palw_exec_v2_health(&state, tip, a.chain.daa_of(tip) + 1).expect("in force");
        assert!(health.stale && health.stale_reason.is_some(), "the node reports its head stale: {health:?}");
    }

    // The executor republishes: a fresh lane block over Y's own anchor, covered by the next chain block, credited once on Y.
    let s0_again = a.slice_block(1, 0, |slice| slice.output_root = Hash64::from_u64_word(0xAAAA));
    assert_ne!(s0_again.header.hash, s0.header.hash);
    a.insert_lane_block(&s0_again, "slice 0 republished on Y").await;
    let y4 = a.chain.heartbeat(ttpb, Vec::new()).await;
    assert_eq!(a.chain.sink(), y4.header.hash);
    let trailer = palw_exec_v2_anchor_split(&y4.transactions[0].payload).unwrap().1.expect("Y4 anchors the republished block");
    assert_eq!(trailer.count, 1, "covered once on this branch");
    let (_, state) = a.chain.tip_state();
    assert!(state.exec_v2_anchored_v1(&s0_again.header.hash) && !state.exec_v2_anchored_v1(&s0.header.hash));
    assert_eq!(state.exec_v2_slice_v1(&claim, 0).map(|row| row.carrier), Some(s0_again.header.hash));
    let root = state.exec_v2_root_v1(&claim).unwrap();
    assert_eq!((root.next_index, root.accepted_work, root.pending), (1, SLICE_WORK, 1), "credited exactly once: {root:?}");
    let data = a.chain.vp().ghostdag_store.get_data(y4.header.hash).unwrap();
    assert!(
        [s0.header.hash, s0_again.header.hash].iter().all(|h| !data.mergeset_blues.contains(h) && !data.mergeset_reds.contains(h)),
        "still in no stored mergeset"
    );

    // A third node that receives everything — the lane block, both branches — agrees with node 1 (X1 is merged by Y3 or a sibling).
    let c = chain_of(&config, &premine, &floats);
    for block in selected_chain_blocks(&a.chain, base) {
        c.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.expect("the prefix");
    }
    // Every block of both branches and both lane blocks, parents first (the republished lane block hangs from Y).
    let delivered: Vec<Block> =
        vec![s0.clone().to_immutable(), x1.clone(), y1.clone(), y2.clone(), y3.clone(), s0_again.clone().to_immutable(), y4.clone()];
    for block in delivered {
        let hash = block.header.hash;
        c.ctx.consensus.validate_and_insert_block(block).virtual_state_task.await.unwrap_or_else(|e| panic!("block {hash}: {e}"));
    }
    assert_eq!(c.sink(), a.chain.sink(), "the third node reaches node 1's sink");
    let (_, c_state) = c.tip_state();
    assert_eq!(c_state.state_root(), state.state_root(), "and its PALW state");
    assert_eq!(c_state.exec_v2_root_v1(&claim), state.exec_v2_root_v1(&claim));
}
