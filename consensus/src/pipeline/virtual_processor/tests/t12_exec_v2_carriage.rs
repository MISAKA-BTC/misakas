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
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
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
    PALW_EXEC_V2_WIRE_VERSION, PalwExecSubtypeV2, PalwExecV2Envelope, PalwWorkRangeV1, PalwWorkSliceV1,
    palw_work_slice_payload_root_v2,
};
use kaspa_consensus_core::palw_exec_v2_anchor::{PalwExecV2AnchorV1, palw_exec_v2_anchor_append, palw_exec_v2_anchor_split};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as Obj};
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
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).expect("serializes");
    let (float_outpoint, float_entry) = floats[0].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(float_outpoint, vec![], 0, 1)],
        vec![TransactionOutput::new(float_entry.amount - 300_000, card_payout_spk(0))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        payload,
    );
    sign_spend(&mut tx, float_entry, 0, config.params.storage_mass_parameter);
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
    let mut chain = chain_of(&config, &premine, &floats);
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
    // A PXE2 lane header: refused by name (the v1 adapter builds the block; the envelope is the v2 one).
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
        network_domain: network_domain(&config),
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
    let err =
        chain.ctx.consensus.validate_and_insert_block(block.to_immutable()).virtual_state_task.await.expect_err("below the fence");
    refused_with("below palw_exec_payload_v2", &err.to_string());
    // And a chain block's coinbase carries no trailer, with or without lane blocks around.
    let (attempt, _) = chain.attempt(2, ttpb, Vec::new(), &|_| true).await;
    assert!(palw_exec_v2_anchor_split(&attempt.transactions[0].payload).unwrap().1.is_none());
}
