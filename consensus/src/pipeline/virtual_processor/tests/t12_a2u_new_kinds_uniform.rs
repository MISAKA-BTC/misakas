//! **A-2 uniformity through the real pipeline (the A2U review, generalising X8R's P11 to every kind the live build cannot decode).**
//!
//! testnet-12 declares the audit fence, so the live build (int-12, `rcore/int-12` @ `0b1c11b87`) TOLERATES a lifecycle payload it
//! cannot decode: the block stands, the carrier is skipped. The integration line added nine such kinds (104–111, 120) and one header
//! carriage form (`PFS4`), and below their fences it must read them exactly as the live build does — the same block verdict, the same
//! skip, no charge, no budget, no state write — or a mixed fleet splits before any fence.
//!
//! Node A runs testnet-12 with harness cards, every owning fence unarmed (as shipped). Beside an ordinary chain (heartbeats, a REAL
//! attempt) it carries, on funded 0x4b carriers: every new kind well-formed and signed; every signed kind UNSIGNED; the oversized and
//! malformed forms (an empty kernel encoding, an over-bound Panel proof, an envelope wrapping no registration) — each a may-ride refusal
//! that, before the review, made the block invalid here and valid on the live build; a tag-254 payload no build decodes (the reference);
//! and `ObjectChunk` groups whose assembled bytes are new kinds — three that complete in one chunk and one that completes in its second.
//! Every carrier is in its block, every block is valid, nothing is folded: no kernel route state, no Panel V3 state, no completed group.
//! A `PFS4` receipt header is refused on the header path AND on the pruning-proof path with the live build's own refusal (its gate,
//! [`check_palw_commitment_shape_at`], reads no `PFS4` form). Node B (unarmed) and node C (every owning fence armed far above the chain)
//! replay every block: the same statuses, refusals, sink, PALW state root and UTXO multiset.
//!
//! Set `A2U_INT12_REPLAY_OUT=<file>` to also write the chain (headers, transactions, the refused blocks and their refusals, and the
//! view) for a replay through a build of the live release itself (`docs/design/palw/a2-uniformity-new-kinds.md` §5).
//!
//! [`check_palw_commitment_shape_at`]: kaspa_consensus_core::pow_layer0::check_palw_commitment_shape_at
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_NEW_KINDS_V1, PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleKindOwnerV1, PalwLifecycleTxPayloadV2,
    palw_lifecycle_kind_owner_v1, palw_lifecycle_object_may_ride_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2 as Obj, palw_object_chunk_group_id_v1};
use kaspa_consensus_core::pow_layer0::{POW_ALGO_ID_PALW_RECEIPT_V3, PalwAttemptLaneV1, check_palw_commitment_shape_at};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::sync::Arc;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// Node C's owning fences: far above anything this chain reaches.
const FAR: u64 = 1_000_000;
/// What a carrier pays: enough to open a chunk group's slot wherever its rent is armed.
const FEE: u64 = 200_000_000;

/// testnet-12 with harness cards; `armed` sets every owning fence (the three lifecycle kinds' and the `PFS4` form's) at that DAA on
/// the built config — the documented validation bypass (`validate_palw_v2` refuses each of them at every real height).
fn config_with(armed: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut config, bundle, premine, floats) = t12_with_harness_cards();
    let Some(at) = armed else { return (config, bundle, premine, floats) };
    let fence = ForkActivation::new(at);
    config.params.palw_probabilistic_constraints_v1 = Some(fence);
    config.params.palw_signed_registration_v1 = Some(fence);
    config.params.palw_receipt_spend_v4 = Some(fence);
    config.params.palw_permissionless_panel_v1 = Some(kaspa_consensus_core::palw_permissionless_panel_v1::PalwPermissionlessPanelV1 {
        activation: fence,
        policy: panel_policy(),
    });
    config.params.sync_palw_permissionless_panel_v1();
    assert!(config.params.validate_palw_v2().is_err(), "the real validation refuses what this harness bypasses");
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!("testnet-12 is V2") };
    let bundle = bundle.clone();
    (config, bundle, premine, floats)
}

fn panel_policy() -> kaspa_consensus_core::palw_permissionless_panel_v1::PanelPolicyV1 {
    let challenge = kaspa_consensus_core::palw_panel_beacon_v1::challenge::policy::reference_policy_v1(1, 1, 10, 1, 1);
    kaspa_consensus_core::palw_permissionless_panel_v1::PanelPolicyV1 {
        seal_depth_blocks: 2,
        seal_wait_daa: 40,
        bond_maturity_daa: 1,
        beacon_period_daa: 40,
        beacon_wait_daa: 12,
        assignment_delay_daa: 1,
        receipt_window_daa: 12,
        seat_count: 5,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 1,
        max_candidates: 64,
        max_pending: 64,
        max_pending_per_bond: 16,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 256,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 4096,
        beacon_scheme: kaspa_consensus_core::palw_panel_beacon_v1::panel_beacon_scheme_of_v1(&challenge),
    }
}

/// Each card's spendable coin: its fee float, then the change of its last carrier.
struct Wallet {
    coins: Vec<(TransactionOutpoint, UtxoEntry)>,
}

impl Wallet {
    /// A funded 0x4b carrier of `payload` by card `card`, its change the card's next coin.
    fn carrier(&mut self, config: &Config, card: usize, payload: Vec<u8>) -> Transaction {
        let (outpoint, entry) = self.coins[card].clone();
        let change = entry.amount - FEE;
        let mut tx = Transaction::new(
            crate::constants::TX_VERSION,
            vec![TransactionInput::new(outpoint, vec![], 0, 1)],
            vec![TransactionOutput::new(change, card_payout_spk(card))],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        );
        sign_spend(&mut tx, entry, card, config.params.storage_mass_parameter);
        self.coins[card] = (TransactionOutpoint::new(tx.id(), 0), UtxoEntry::new(change, card_payout_spk(card), 0, false));
        tx
    }
}

fn payload_of(object: &Obj) -> Vec<u8> {
    borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).expect("serializes")
}

fn zeros<T: borsh::BorshDeserialize>() -> T {
    T::deserialize(&mut &[0u8; 8192][..]).expect("a zero-filled encoding decodes")
}

/// One well-formed, signed object of every kind the live build cannot decode.
fn well_formed(bond: PalwBondKeyV2) -> Vec<Obj> {
    let h = Hash64::from_bytes([3; 64]);
    let registration = Obj::ClassRegistered {
        class_id: h,
        artifact_root: h,
        slash_value_per_pwu: 1,
        pwu_rule: kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(10),
        initial_target: 1,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    vec![
        Obj::ArtifactBoundV1 { v2_class: h, kernel_param_root: h, signer: bond, signature: vec![1; 64] },
        Obj::ArtifactBindingChallengedV1 {
            v2_class: h,
            kernel_param_root: h,
            challenger: bond,
            proof: Box::new(zeros()),
            signature: vec![1; 64],
        },
        Obj::KernelBoundV1 { v2_class: h, kernel_class: h, challenge_policy_id: h, signer: bond, signature: vec![1; 64] },
        Obj::ConformanceCommittedV1 { commitment: Box::new(zeros()), signer: bond, signature: vec![1; 64] },
        Obj::SignedRegistrationV1 {
            registration: Box::new(registration),
            valid_from_daa: 0,
            valid_until_daa: 10_000,
            fork_digest: kaspa_consensus_core::Hash::from_bytes([4; 32]),
            signer: bond,
            signature: vec![1; 64],
        },
        Obj::ConformanceEvidenceV1 { v2_class: h, action: Box::new(zeros()), signer: bond, signature: vec![1; 64] },
        Obj::KernelRouteV1 { bytes: vec![5; 256], signer: bond, signature: vec![1; 64] },
        Obj::KernelConstraintReceiptV1 { receipt: Box::new(zeros()), signature: vec![1; 64] },
        Obj::PanelBeaconProofV3 { proof: Box::new(misaka_palw_panel::BeaconProofV1 { epoch: 1, output: h, proof: vec![6; 32] }) },
    ]
}

/// Every form a new kind's may-ride arm refuses that fits a transaction: unsigned (each signed kind), an empty kernel encoding, an
/// over-bound Panel proof and an envelope wrapping no registration. (An over-bound kernel encoding is past what a block carries; it
/// reaches a node only assembled from chunks, which `ObjectChunk` groups below exercise.)
fn malformed(bond: PalwBondKeyV2) -> Vec<Obj> {
    let h = Hash64::from_bytes([3; 64]);
    let mut out: Vec<Obj> = well_formed(bond)
        .into_iter()
        .filter_map(|mut object| {
            let signature = match &mut object {
                Obj::ArtifactBoundV1 { signature, .. }
                | Obj::ArtifactBindingChallengedV1 { signature, .. }
                | Obj::KernelBoundV1 { signature, .. }
                | Obj::ConformanceCommittedV1 { signature, .. }
                | Obj::SignedRegistrationV1 { signature, .. }
                | Obj::ConformanceEvidenceV1 { signature, .. }
                | Obj::KernelRouteV1 { signature, .. }
                | Obj::KernelConstraintReceiptV1 { signature, .. } => signature,
                _ => return None,
            };
            signature.clear();
            Some(object)
        })
        .collect();
    out.push(Obj::KernelRouteV1 { bytes: Vec::new(), signer: bond, signature: vec![1; 64] });
    out.push(Obj::PanelBeaconProofV3 {
        proof: Box::new(misaka_palw_panel::BeaconProofV1 {
            epoch: 1,
            output: h,
            proof: vec![6; misaka_palw_panel::MAX_BEACON_PROOF_BYTES_V1 as usize + 1],
        }),
    });
    out.push(Obj::SignedRegistrationV1 {
        registration: Box::new(Obj::KernelBoundV1 {
            v2_class: h,
            kernel_class: h,
            challenge_policy_id: h,
            signer: bond,
            signature: vec![1],
        }),
        valid_from_daa: 0,
        valid_until_daa: 10_000,
        fork_digest: kaspa_consensus_core::Hash::from_bytes([4; 32]),
        signer: bond,
        signature: vec![1; 64],
    });
    for object in &out {
        assert!(palw_lifecycle_object_may_ride_v2(object).is_err(), "a may-ride refusal: {object:?}");
    }
    out
}

/// The chunks of a group carrying `inner`, cut in `parts` (each `ObjectChunk` names the group id of the whole).
fn chunks_of(inner: &Obj, parts: usize) -> Vec<Obj> {
    let whole = borsh::to_vec(inner).unwrap();
    let group = palw_object_chunk_group_id_v1(&whole);
    let size = whole.len().div_ceil(parts);
    whole
        .chunks(size)
        .enumerate()
        .map(|(index, bytes)| Obj::ObjectChunk { group, index: index as u8, count: parts as u8, bytes: bytes.to_vec() })
        .collect()
}

fn group_of(chunk: &Obj) -> Hash64 {
    let Obj::ObjectChunk { group, .. } = chunk else { unreachable!("a chunk") };
    *group
}

/// A node's view, as the comparison reads it: the sink, the PALW state root, the virtual UTXO multiset.
fn view(chain: &T12Chain) -> (BlockHash, Hash64, Hash64) {
    let (_, state) = chain.tip_state();
    let multiset = chain.vp().virtual_stores.read().state.get().unwrap().multiset.clone().finalize();
    (chain.sink(), state.state_root(), multiset)
}

fn chain_blocks(chain: &T12Chain) -> Vec<Block> {
    let vp = chain.vp();
    let genesis = chain.config.params.genesis.hash;
    let (mut hashes, mut at) = (Vec::new(), chain.sink());
    while at != genesis {
        hashes.push(at);
        at = vp.ghostdag_store.get_selected_parent(at).expect("a chain block has a selected parent");
    }
    hashes.reverse();
    hashes.into_iter().map(|h| chain.ctx.consensus.get_block(h).expect("the node holds its chain")).collect()
}

/// A receipt-lane (algo-7) block on `chain`'s sink whose carriage is `PFS4` — built, not inserted.
fn pfs4_block(chain: &mut T12Chain, config: &Config) -> MutableBlock {
    let mut template = chain
        .ctx
        .consensus
        .build_block_template(new_miner_data(), Box::new(OnetimeTxSelector::new(Vec::new())), TemplateBuildMode::Standard)
        .expect("a template");
    stamp_harness_time(&config.params, &mut template.block.header, chain.ctx.simulated_time + config.params.target_time_per_block());
    let mut block = template.block;
    block.header.pow_algo_id = POW_ALGO_ID_PALW_RECEIPT_V3;
    let mut commitment = kaspa_consensus_core::palw_receipt_v4::PALW_RECEIPT_V4_CARRIAGE_MAGIC.to_vec();
    commitment.extend_from_slice(&[0xA2; 96]);
    block.header.palw_commitment = commitment;
    block.header.finalize();
    block
}

/// The live build's refusal of a header carriage: its shape gate, which has no arm for a form added after it.
fn live_build_shape_refusal(block: &MutableBlock) -> kaspa_consensus_core::pow_layer0::PowLayer0Error {
    check_palw_commitment_shape_at(block.header.pow_algo_id, &block.header.palw_commitment, false, PalwAttemptLaneV1::Unfenced)
        .expect_err("the live build's gate reads a PFS4 carriage as a PFS3 one, and refuses it")
}

async fn replay(
    config: &Config,
    bundle: &PalwConsensusParamsV2,
    premine: &Premine,
    floats: &Premine,
    blocks: &[Block],
    refused: &[(Block, String)],
) -> T12Chain {
    let node = t12_genesis_chain(config, bundle, premine, floats);
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
        let err =
            node.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect_err("refused on every node");
        assert_eq!(&err.to_string(), why, "the same refusal");
    }
    node
}

/// **Below their fences, every kind and form the live build cannot decode is judged as it judges them — and a node with every owning
/// fence armed far above the chain agrees on every verdict, refusal, sink, root and UTXO multiset** (see the module doc).
#[tokio::test]
async fn t12_a2u_every_new_kind_below_its_fence_is_the_live_builds_undecodable_payload_and_every_node_agrees() {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = config_with(None);
    let (armed_far, armed_far_bundle, _, _) = config_with(Some(FAR));
    // A node with every owning fence in force from DAA 1: it judges nothing here but the chunk reader and a pruning proof, to show
    // the reading turns at the fence.
    let (armed_now, armed_now_bundle, _, _) = config_with(Some(1));
    let now = t12_genesis_chain(&armed_now, &armed_now_bundle, &premine, &floats);
    assert_eq!(config.params.palw_lifecycle_kind_fences_v1(), Default::default(), "testnet-12 arms no owning fence");
    assert_eq!(config.params.palw_header_form_fences_v1(), Default::default(), "nor the PFS4 form's");
    assert!(config.params.palw_audit_2026_09_11_fence().is_some(), "testnet-12 declares the audit fence: undecodable is tolerated");
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    let mut wallet = Wallet { coins: floats.clone() };

    // ---- the plain chain: heartbeats and a REAL attempt ----
    for _ in 0..6 {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    a.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let plain_view = view(&a);

    // ---- the payloads: every new kind well-formed, every malformed form, the reference, and chunk groups of new kinds ----
    let bond = a.bonds[1];
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for object in well_formed(bond) {
        payloads.push((format!("well-formed {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    for object in malformed(bond) {
        payloads.push((format!("malformed {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    let mut reference = borsh::to_vec(&PALW_LIFECYCLE_TX_VERSION_V2).unwrap();
    reference.extend_from_slice(&[254, 1, 2, 3]);
    payloads.push(("the reference: tag 254, which no build decodes".into(), reference));
    let route = Obj::KernelRouteV1 { bytes: vec![5; 512], signer: bond, signature: vec![1; 64] };
    let one_chunk: Vec<Obj> = [route.clone(), well_formed(bond)[1].clone(), well_formed(bond)[5].clone()]
        .iter()
        .map(|inner| chunks_of(inner, 1).remove(0))
        .collect();
    for chunk in &one_chunk {
        payloads.push(("a one-chunk group of a new kind".into(), payload_of(chunk)));
    }
    let two_chunks = chunks_of(&Obj::KernelRouteV1 { bytes: vec![7; 2048], signer: bond, signature: vec![1; 64] }, 2);
    let late_part = two_chunks[1].clone();
    payloads.insert(0, ("a two-chunk group's opening part".into(), payload_of(&two_chunks[0])));

    // Carried eight a block (one per card), the second part of the two-chunk group in the last block.
    payloads.push(("a two-chunk group's completing part".into(), payload_of(&late_part)));
    let mut carried = 0usize;
    for batch in payloads.chunks(8) {
        let txs: Vec<Transaction> =
            batch.iter().enumerate().map(|(card, (_, payload))| wallet.carrier(&config, card, payload.clone())).collect();
        let block = a.heartbeat(ttpb, txs.clone()).await;
        for (tx, (what, _)) in txs.iter().zip(batch) {
            assert!(block.transactions.iter().any(|t| t.id() == tx.id()), "carried, its block valid: {what}");
        }
        carried += txs.len();
    }
    a.heartbeat(ttpb, Vec::new()).await; // accepts the last batch
    assert_eq!(carried, payloads.len());

    // ---- nothing was folded, charged or opened past what the live build does ----
    let (_, state) = a.tip_state();
    assert!(state.kernel_route().is_none(), "no kernel route state: every 104–111 object skipped");
    assert!(state.panel_v3().is_none(), "no Panel V3 state: the 120 objects skipped");
    for chunk in &one_chunk {
        assert!(
            state.pending_chunk_group(&group_of(chunk)).is_none(),
            "a one-chunk group of a new kind completes nothing and opens nothing"
        );
    }
    // The two-chunk group: its opening part is an ordinary `ObjectChunk` the live build accepts too (it opens the group where the
    // slot's rent is paid); its completing part assembles bytes that build cannot decode, so it is refused and the group stays open.
    if let Some(pending) = state.pending_chunk_group(&group_of(&two_chunks[0])) {
        assert_eq!(pending.parts.keys().copied().collect::<Vec<_>>(), vec![0], "the completing part was refused, as undecodable");
    }
    // The walk's chunk reader — the certification cap's input — answers "undecodable" below the fence and the kind past it.
    let daa = a.daa_of(a.sink());
    for chunk in &one_chunk {
        assert!(a.vp().palw_kernel_chunk_inner(&state, chunk, daa).is_none(), "below the fence: undecodable to the cap");
        let inner = now.vp().palw_kernel_chunk_inner(&state, chunk, daa).expect("past the fence: the kind");
        assert!(matches!(palw_lifecycle_kind_owner_v1(&inner), PalwLifecycleKindOwnerV1::Fence(_)));
    }
    assert!(
        PALW_LIFECYCLE_NEW_KINDS_V1.iter().all(|(tag, _, _)| well_formed(bond).iter().any(|o| borsh::to_vec(o).unwrap()[0] == *tag))
    );

    // ---- the PFS4 header: the live build's refusal, on the header path and on the pruning-proof path ----
    let pfs4 = pfs4_block(&mut a, &config);
    let live = live_build_shape_refusal(&pfs4);
    let expected = RuleError::BadPalwCommitmentShape(live.to_string()).to_string();
    let err = a.ctx.consensus.validate_and_insert_block(pfs4.clone().to_immutable()).virtual_state_task.await.expect_err("refused");
    assert_eq!(err.to_string(), expected, "the header path refuses a PFS4 header as the live build does");
    let mut proof: Vec<Vec<Arc<kaspa_consensus_core::header::Header>>> = vec![Vec::new(); config.params.max_block_level as usize + 1];
    proof[0].push(Arc::new(pfs4.header.clone()));
    let proof_refusal =
        |chain: &T12Chain| chain.ctx.consensus.validate_pruning_proof_standalone(&proof).expect_err("refused").to_string();
    let expected_proof = kaspa_consensus_core::errors::pruning::PruningImportError::PruningProofBadPalwCommitment(
        pfs4.header.hash,
        0,
        live.to_string(),
    )
    .to_string();
    assert_eq!(proof_refusal(&a), expected_proof, "the pruning-proof path refuses it as the live build does");
    assert_ne!(proof_refusal(&now), expected_proof, "past the fence the form is the V4 carriage, judged by its own decode");
    let refused = vec![(pfs4.to_immutable(), expected)];

    // ---- every node agrees ----
    let blocks = chain_blocks(&a);
    for (other, other_bundle, name) in [(&config, &bundle, "unarmed"), (&armed_far, &armed_far_bundle, "armed far above")] {
        let node = replay(other, other_bundle, &premine, &floats, &blocks, &refused).await;
        assert_eq!(view(&node), view(&a), "the {name} node: sink, PALW root, UTXO multiset");
        for block in &blocks {
            assert_eq!(node.ctx.consensus.block_status(block.header.hash), a.ctx.consensus.block_status(block.header.hash));
        }
        assert_eq!(proof_refusal(&node), expected_proof, "the {name} node's pruning proof");
    }
    assert_ne!(plain_view.0, view(&a).0, "the carriers moved the chain");

    // ---- optionally, the chain for a replay through the live release itself ----
    if let Ok(path) = std::env::var("A2U_INT12_REPLAY_OUT") {
        let dump: (
            BlockHash,
            Vec<(kaspa_consensus_core::header::Header, Vec<Transaction>)>,
            Vec<(kaspa_consensus_core::header::Header, Vec<Transaction>, String)>,
            (BlockHash, Hash64, Hash64),
        ) = (
            config.params.genesis.hash,
            blocks.iter().map(|b| ((*b.header).clone(), (*b.transactions).clone())).collect(),
            refused.iter().map(|(b, why)| ((*b.header).clone(), (*b.transactions).clone(), why.clone())).collect(),
            view(&a),
        );
        std::fs::write(&path, borsh::to_vec(&dump).expect("serializes")).expect("writes the replay file");
        eprintln!("[a2u] wrote {} blocks and {} refusals to {path}", dump.1.len(), dump.2.len());
    }
}
