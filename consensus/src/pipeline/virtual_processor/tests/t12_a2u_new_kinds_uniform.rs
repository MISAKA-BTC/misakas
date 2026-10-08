//! **A-2 uniformity through the real pipeline — the mixed-verdict pin over every new kind** (the A2U review, generalising X8R's P11
//! to every kind the live build cannot decode).
//!
//! testnet-12 declares the audit fence, so the live build (int-12, `rcore/int-12` @ `0b1c11b87`) TOLERATES a lifecycle payload it
//! cannot decode: the block stands, the carrier is skipped. Every kind added after it (`PALW_LIFECYCLE_NEW_KINDS_V1`) and every header
//! carriage form (`PalwHeaderFormFenceV1`) must, below its fence, be read exactly as the live build reads it — the same block verdict,
//! the same skip, no charge, no budget, no state write — or a mixed fleet splits before any fence.
//!
//! Node A runs testnet-12 with harness cards and every owning fence unarmed (as shipped), on two rulesets: as LAUNCHED, and with the
//! live release compressed below DAA 330 (every flag day the live chain has crossed, at heights a test reaches — the ruleset live
//! int-12 judges blocks under today). Beside an ordinary chain (heartbeats, REAL attempts) it carries, on funded 0x4b carriers:
//!
//! * **every new kind, generically**: each `PALW_LIFECYCLE_NEW_KINDS_V1` row's kind decoded from a zero-filled body (a kind a lane
//!   adds is in this sweep the moment its row exists), plus a hand-built well-formed, signed one of each;
//! * every may-ride refusal a new kind has: unsigned, an empty kernel encoding, an over-bound Panel proof, an envelope wrapping no
//!   registration — each, before the review, a block the live build accepts and this build refused;
//! * a tag-254 payload no build decodes (the reference) and an int-12 kind carrying a value the live build re-reads (a generative
//!   class whose profile byte is HFX's `Head = 6`);
//! * `ObjectChunk` groups whose assembled bytes are new kinds — three that complete in one chunk, one that completes in its second;
//! * **a mixed block**: a live-build kind that folds (a quorum's `ReceiptLicensed`, which licenses a real claim), a duplicate the
//!   walk refuses, and new kinds riding unjudged, all in one block.
//!
//! Every carrier is in its block, every block is valid, the licence licenses, and nothing else is folded: no kernel route state, no
//! Panel V3 state, no completed group. A `PFS4` receipt header is refused on the header path AND the pruning-proof path with the
//! live build's own refusal. Node B (unarmed) and node C (every owning fence armed far above the chain) replay every block: the same
//! statuses, refusals, sink, PALW state root and UTXO multiset at every block.
//!
//! `A2U_INT12_REPLAY_OUT=<dir>` also writes each ruleset's chain (`<dir>/a2u-<ruleset>.borsh`: the headers, transactions, refusals,
//! per-block roots and the view) for the replay through the live release itself (`a2u_int12_replay.rs.int12`,
//! `scripts/a2u-int12-replay.sh`; `docs/design/palw/a2-uniformity-new-kinds.md` §5).
use super::t12_round_lane_e2e::{
    T12Chain, card_payout_spk, sign_spend, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards,
};
use super::{OnetimeTxSelector, new_miner_data};
use crate::consensus::test_consensus::TestConsensus;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::params::{ForkActivation, Params};
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_NEW_KINDS_V1, PALW_LIFECYCLE_TX_VERSION_V2, PalwKernelInnerFenceV1, PalwLifecycleKindFenceV1,
    PalwLifecycleKindOwnerV1, PalwLifecycleTxPayloadV2, palw_lifecycle_kind_owner_v1, palw_lifecycle_object_may_ride_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBondKeyV2, PalwClaimPhaseV2, PalwConsensusObjectV2 as Obj, palw_object_chunk_group_id_v1,
};
use kaspa_consensus_core::pow_layer0::{
    POW_ALGO_ID_PALW_RECEIPT_V3, PalwAttemptLaneV1, PalwHeaderFormFenceV1, PowLayer0Error, check_palw_commitment_shape_at,
};
use kaspa_consensus_core::tx::{Transaction, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry};
use kaspa_hashes::Hash64;
use std::sync::Arc;

type Premine = Vec<(TransactionOutpoint, UtxoEntry)>;

/// Node C's owning fences: far above anything this chain reaches.
const FAR: u64 = 1_000_000;
/// What a carrier pays: enough to open a chunk group's slot wherever its rent is armed.
const FEE: u64 = 200_000_000;
/// Past this DAA the compressed release is the ruleset live testnet-12 runs today (the int-11 list at 40, its last ρ step at +285).
const RELEASE_PRESENT_DAA: u64 = 330;

/// The ruleset the chain runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ruleset {
    /// testnet-12 as launched (`palw_t12_launch_params_v1`): the ruleset of the chain's first 750 DAA.
    Launch,
    /// The live release compressed ([`arm_the_release`]): the ruleset live int-12 judges every new block under.
    Release,
}

impl Ruleset {
    fn name(self) -> &'static str {
        match self {
            Self::Launch => "launch",
            Self::Release => "release",
        }
    }
}

/// **The live testnet-12 release, compressed** — the launch ruleset with every flag day the live chain has crossed, in its order,
/// at heights a test reaches (as `g14_registration_e2e`'s `t12_release_compressed`): the DAA-750 list at 20, the second at 24, the
/// capacity list at 28, `palw_tir_v1` at 32, `palw_tir_fence2` at 36, the int-11 list at 40. Each fence through its own `set`. The
/// replay through int-12 arms the same lists with int-12's own entries.
fn arm_the_release(params: &mut Params) {
    use kaspa_consensus_core::config::params::{
        PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2, PALW_T12_POST_LAUNCH_FENCES_V3, PALW_T12_TIR_FENCE2_FENCES_V1,
        PALW_T12_TIR_FLAG_DAY_FENCES_V1, palw_t12_arm_int11_flag_day_at_v1,
    };
    for (list, at) in [
        (PALW_T12_POST_LAUNCH_FENCES_V1, 20),
        (PALW_T12_POST_LAUNCH_FENCES_V2, 24),
        (PALW_T12_POST_LAUNCH_FENCES_V3, 28),
        (PALW_T12_TIR_FLAG_DAY_FENCES_V1, 32),
        (PALW_T12_TIR_FENCE2_FENCES_V1, 36),
    ] {
        for fence in list {
            (fence.set)(params, Some(ForkActivation::new(at)));
        }
    }
    palw_t12_arm_int11_flag_day_at_v1(params, Some(40));
}

/// **Every owning fence at `at`** — the lifecycle kinds', the header forms' and the kernel-route inner kinds' — through exhaustive
/// matches over their `ALL` lists, so a fence a lane adds does not compile here until this harness arms it. The documented
/// validation bypass: `validate_palw_v2` refuses each of them at every real height.
fn arm_every_owning_fence(params: &mut Params, at: ForkActivation) {
    for fence in PalwLifecycleKindFenceV1::ALL {
        match fence {
            PalwLifecycleKindFenceV1::ProbabilisticConstraintsV1 => params.palw_probabilistic_constraints_v1 = Some(at),
            PalwLifecycleKindFenceV1::SignedRegistrationV1 => params.palw_signed_registration_v1 = Some(at),
            PalwLifecycleKindFenceV1::PermissionlessPanelV1 => {
                params.palw_permissionless_panel_v1 =
                    Some(kaspa_consensus_core::palw_permissionless_panel_v1::PalwPermissionlessPanelV1 {
                        activation: at,
                        policy: panel_policy(),
                    });
                params.sync_palw_permissionless_panel_v1();
            }
            PalwLifecycleKindFenceV1::ProviderCourtV1 => params.palw_provider_court_v1 = Some(at),
        }
    }
    for form in PalwHeaderFormFenceV1::ALL {
        match form {
            PalwHeaderFormFenceV1::ReceiptSpendV4 => params.palw_receipt_spend_v4 = Some(at),
        }
    }
    for fence in PalwKernelInnerFenceV1::ALL {
        match fence {
            PalwKernelInnerFenceV1::PanelFreeV1 => {
                params.palw_panel_free_v1 =
                    Some(kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1::interim_v1(at, Vec::new()));
            }
            PalwKernelInnerFenceV1::TypedRootsV1 => params.palw_typed_roots_v1 = Some(at),
        }
    }
}

/// testnet-12 with harness cards on `ruleset`; `armed` sets every owning fence at that DAA.
fn config_with(ruleset: Ruleset, armed: Option<u64>) -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut config, _, premine, floats) = t12_with_harness_cards();
    if ruleset == Ruleset::Release {
        arm_the_release(&mut config.params);
        config.params.validate_palw_v2().expect("the live release, compressed, is a runnable ruleset");
    }
    if let Some(at) = armed {
        arm_every_owning_fence(&mut config.params, ForkActivation::new(at));
        assert!(config.params.validate_palw_v2().is_err(), "the real validation refuses what this harness bypasses");
    }
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

/// **The smallest value of `T` a near-zero body decodes to** (the core A2U tests' `minimal_decode`): a zero-filled body with each
/// byte borsh refuses stepped up until it decodes (an enum whose first variant is `= 1` refuses tag 0, and the refused byte is the
/// last one it read).
fn minimal_decode<T: borsh::BorshDeserialize>(prefix: &[u8]) -> Option<T> {
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(&[0u8; 16_384]);
    for _ in 0..4_096 {
        let mut reader = &bytes[..];
        match T::deserialize(&mut reader) {
            Ok(value) => return Some(value),
            Err(_) => {
                let refused = (bytes.len() - reader.len()).checked_sub(1)?;
                if refused < prefix.len() || bytes[refused] == u8::MAX {
                    return None;
                }
                bytes[refused] += 1;
            }
        }
    }
    None
}

fn zeros<T: borsh::BorshDeserialize>() -> T {
    minimal_decode(&[]).expect("a near-zero encoding decodes")
}

/// **The kind with object tag `tag`, every field (nearly) zero** — generic over the enum, so a kind a lane adds to
/// `PALW_LIFECYCLE_NEW_KINDS_V1` is carried here without an edit (a kind no near-zero body decodes to needs a hand-built sample in
/// [`well_formed`], and the panic says so).
fn zero_filled_kind(tag: u8) -> Obj {
    let object: Obj = minimal_decode(&[tag])
        .unwrap_or_else(|| panic!("kind {tag} decodes from no near-zero body; give it a hand-built sample instead"));
    assert_eq!(borsh::to_vec(&object).unwrap()[0], tag);
    object
}

/// One well-formed, signed object of every kind the live build cannot decode that has a hand-built sample.
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
        Obj::PanelBeaconProofV3 {
            proof: Box::new(kaspa_consensus_core::palw_permissionless_panel_v1::BeaconProofV1 {
                epoch: 1,
                output: h,
                proof: vec![6; 32],
            }),
        },
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
        proof: Box::new(kaspa_consensus_core::palw_permissionless_panel_v1::BeaconProofV1 {
            epoch: 1,
            output: h,
            proof: vec![6; kaspa_consensus_core::palw_permissionless_panel_v1::MAX_BEACON_PROOF_BYTES_V1 as usize + 1],
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

/// **A live-build kind carrying a value the live build re-reads**: a generative class registration (tag 68, armed on testnet-12)
/// whose hand-read profile byte is HFX's `Head = 6`. The live build decodes it and judges it with its own rules; below
/// `palw_task_heads_v1` every build must give that verdict (`PalwInt12WireChangeV1::CarriedReread`).
/// **A provider's answer (tag 152, lane DA16), signed** — the near-zero kind with a signature present, so its may-ride arm passes and
/// only the fence decides. Below `palw_provider_court_v1` a group of `ObjectChunk`s assembling to it is the live build's undecodable
/// bytes at the completing chunk: no gate judges it, the certification cap counts it as int-12 does, the fold refuses it in int-12's
/// words.
fn provider_answer() -> Obj {
    let mut answer = zero_filled_kind(152);
    let Obj::ProviderAnswerV1 { signature, .. } = &mut answer else { unreachable!("tag 152") };
    *signature = vec![1; 64];
    assert_eq!(palw_lifecycle_object_may_ride_v2(&answer), Ok(()));
    answer
}

fn reread_probes() -> Vec<Obj> {
    let mut class = zero_filled_kind(68);
    let Obj::ClassRegisteredGenV1 { admission, .. } = &mut class else { unreachable!("tag 68") };
    admission.class.profile = 6;
    vec![class]
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

/// **The live build's refusals of a header carriage** — its shape gate (which has no arm for a form added after it) under every
/// binding and lane it can be asked with; each refuses.
fn live_build_shape_refusals(block: &MutableBlock) -> Vec<PowLayer0Error> {
    let mut out = Vec::new();
    for bound in [false, true] {
        for lane in [PalwAttemptLaneV1::Unfenced, PalwAttemptLaneV1::LegacyArm, PalwAttemptLaneV1::ExecutionArm] {
            out.push(
                check_palw_commitment_shape_at(block.header.pow_algo_id, &block.header.palw_commitment, bound, lane)
                    .expect_err("the live build's gate refuses a PFS4 carriage under every binding and lane"),
            );
        }
    }
    out
}

/// What [`replay`] compares at each block, and what the dump carries for the replay through int-12.
type Dump = (
    u8,
    BlockHash,
    Vec<(kaspa_consensus_core::header::Header, Vec<Transaction>)>,
    Vec<(kaspa_consensus_core::header::Header, Vec<Transaction>, String)>,
    String,
    Vec<(BlockHash, Hash64)>,
    (BlockHash, Hash64, Hash64),
);

/// The PALW state root each of `blocks` recorded (its delta's root, as the node stored it).
fn roots_along(chain: &T12Chain, blocks: &[Block]) -> Vec<(BlockHash, Hash64)> {
    let vp = chain.vp();
    let store = vp.palw_state_v2_store.read();
    blocks.iter().map(|b| (b.header.hash, store.state_root_of(b.header.hash).expect("a chain block's delta is stored"))).collect()
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
        assert_eq!(node.ctx.consensus.block_status(hash), BlockStatus::StatusUTXOValid, "block {hash}");
    }
    for (block, why) in refused {
        let err =
            node.ctx.consensus.validate_and_insert_block(block.clone()).virtual_state_task.await.expect_err("refused on every node");
        assert_eq!(&err.to_string(), why, "the same refusal");
    }
    node
}

/// What a block's carriers may weigh, in bytes (payloads plus each carrier's ML-DSA-87 key and signature): the block's transient
/// storage mass is four times its bytes and its limit is 500,000, so 120 KB keeps every block under it.
const BLOCK_CARRIAGE_BYTES: usize = 120_000;
/// One carrier's own bytes beside its payload: the input's ML-DSA-87 signature and key, the output, the header fields.
const CARRIER_OVERHEAD_BYTES: usize = 7_600;

/// Carry `payloads` in order on heartbeats — at most eight a block (one carrier per card) and at most [`BLOCK_CARRIAGE_BYTES`] —
/// and demand each carrier is in its block. Returns how many blocks carried them.
async fn carry(a: &mut T12Chain, wallet: &mut Wallet, config: &Config, payloads: &[(String, Vec<u8>)]) -> usize {
    let ttpb = config.params.target_time_per_block();
    let mut batches: Vec<&[(String, Vec<u8>)]> = Vec::new();
    let (mut start, mut bytes) = (0usize, 0usize);
    for (i, (_, payload)) in payloads.iter().enumerate() {
        let weight = payload.len() + CARRIER_OVERHEAD_BYTES;
        if i > start && (i - start == 8 || bytes + weight > BLOCK_CARRIAGE_BYTES) {
            batches.push(&payloads[start..i]);
            (start, bytes) = (i, 0);
        }
        bytes += weight;
    }
    batches.push(&payloads[start..]);
    for batch in &batches {
        let txs: Vec<Transaction> =
            batch.iter().enumerate().map(|(card, (_, payload))| wallet.carrier(config, card, payload.clone())).collect();
        let block = a.heartbeat(ttpb, txs.clone()).await;
        for (tx, (what, _)) in txs.iter().zip(batch.iter()) {
            assert!(block.transactions.iter().any(|t| t.id() == tx.id()), "carried, its block valid: {what}");
        }
    }
    a.heartbeat(ttpb, Vec::new()).await; // accepts the last batch
    batches.len()
}

/// **Every seat of the claim's panel signs `Valid`** and the node assembles the licence its panel service would submit (the
/// assembler picks the set the ruleset's door takes).
/// `None` where the ruleset licenses the claim otherwise (past RFC-0007's vertex fence a claim licenses by the vertices' tally, and the
/// node's assembler offers no receipt set).
fn licence_for(chain: &T12Chain, claim_id: Hash64) -> Option<Obj> {
    use kaspa_consensus_core::palw_panel_v2::{
        PALW_RECEIPT_V2_MLDSA87_CONTEXT, PalwReceiptVerdictV2, PalwSeatReceiptV2, palw_receipt_message_v2,
    };
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        chain.config.params.net.to_string().as_bytes(),
        Some(chain.config.params.genesis.hash),
    );
    let (_, state) = chain.tip_state();
    let panel = state.panel(&claim_id).expect("a bound claim has a panel").clone();
    let signed_daa = chain.ctx.consensus.get_virtual_daa_score();
    let receipts: Vec<PalwSeatReceiptV2> = panel
        .seats
        .iter()
        .map(|seat| {
            let card = chain.bonds.iter().position(|b| *b == seat.bond).expect("every seat is a genesis card");
            let message = palw_receipt_message_v2(network_domain, claim_id, PalwReceiptVerdictV2::Valid, signed_daa);
            let signature = libcrux_ml_dsa::ml_dsa_87::sign(
                &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
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
    let object = chain.vp().palw_v2_receipt_quorum_assemble_impl(claim_id, &receipts)?;
    assert!(
        matches!(object, Obj::ReceiptLicensed { .. } | Obj::ReceiptLicensedV2 { .. } | Obj::OptimisticLicensed { .. }),
        "every seat's Valid licenses the claim: {object:?}"
    );
    Some(object)
}

/// **Below their fences, every kind and form the live build cannot decode is judged as it judges them — beside live-build kinds that
/// fold and are refused in the same blocks — and a node with every owning fence armed far above the chain agrees on every verdict,
/// refusal, root and UTXO multiset** (see the module doc). Returns the dump for the replay through int-12.
async fn the_mixed_verdict_chain(ruleset: Ruleset) -> Dump {
    kaspa_core::log::try_init_logger("warn");
    let (config, bundle, premine, floats) = config_with(ruleset, None);
    let (armed_far, armed_far_bundle, _, _) = config_with(ruleset, Some(FAR));
    // A node with every owning fence in force from DAA 1: it judges nothing here but the chunk reader and a pruning proof, to show the
    // reading turns at the fence.
    let (armed_now, armed_now_bundle, _, _) = config_with(ruleset, Some(1));
    let now = t12_genesis_chain(&armed_now, &armed_now_bundle, &premine, &floats);
    assert_eq!(config.params.palw_lifecycle_kind_fences_v1(), Default::default(), "testnet-12 arms no owning fence");
    assert_eq!(config.params.palw_header_form_fences_v1(), Default::default(), "nor the PFS4 form's");
    assert!(config.params.palw_audit_2026_09_11_fence().is_some(), "testnet-12 declares the audit fence: undecodable is tolerated");
    let mut a = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let ttpb = config.params.target_time_per_block();
    let mut wallet = Wallet { coins: floats.clone() };
    let tag = ruleset.name();

    // ---- the plain chain, up to the ruleset's present: heartbeats and a REAL attempt (its claim licensed below) ----
    let present = if ruleset == Ruleset::Release { RELEASE_PRESENT_DAA } else { 2 };
    while a.daa_of(a.sink()) < present {
        a.heartbeat(ttpb, Vec::new()).await;
    }
    let (_, claim_id) = a.attempt(0, ttpb, Vec::new(), &|_| true).await;
    let plain_view = view(&a);

    // ---- the payloads: every new kind (zero-filled and hand-built), every malformed form, the reference, the re-read probe ----
    let bond = a.bonds[1];
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for (kind_tag, name, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
        payloads.push((format!("{name} (tag {kind_tag}), zero-filled"), payload_of(&zero_filled_kind(*kind_tag))));
    }
    for object in well_formed(bond) {
        payloads.push((format!("well-formed {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    for object in malformed(bond) {
        payloads.push((format!("malformed {object:?}").chars().take(60).collect(), payload_of(&object)));
    }
    let mut reference = borsh::to_vec(&PALW_LIFECYCLE_TX_VERSION_V2).unwrap();
    reference.extend_from_slice(&[254, 1, 2, 3]);
    payloads.push(("the reference: tag 254, which no build decodes".into(), reference.clone()));
    for probe in reread_probes() {
        payloads.push(("a live-build kind carrying a re-read value (tag 68, profile 6)".into(), payload_of(&probe)));
    }
    let route = Obj::KernelRouteV1 { bytes: vec![5; 512], signer: bond, signature: vec![1; 64] };
    let one_chunk: Vec<Obj> = [route.clone(), well_formed(bond)[1].clone(), well_formed(bond)[5].clone(), provider_answer()]
        .iter()
        .map(|inner| chunks_of(inner, 1).remove(0))
        .collect();
    for chunk in &one_chunk {
        payloads.push(("a one-chunk group of a new kind".into(), payload_of(chunk)));
    }
    let two_chunks = chunks_of(&Obj::KernelRouteV1 { bytes: vec![7; 2048], signer: bond, signature: vec![1; 64] }, 2);
    // Another answer than the one-chunk group's (a group id is the hash of the whole bytes).
    let answer_chunks = chunks_of(
        &{
            let mut answer = provider_answer();
            let Obj::ProviderAnswerV1 { signature, .. } = &mut answer else { unreachable!("tag 152") };
            *signature = vec![2; 64];
            answer
        },
        2,
    );
    payloads.insert(0, ("a two-chunk group's opening part".into(), payload_of(&two_chunks[0])));
    payloads.insert(1, ("a chunked provider answer's opening part (tag 152)".into(), payload_of(&answer_chunks[0])));
    payloads.push(("a two-chunk group's completing part".into(), payload_of(&two_chunks[1])));
    payloads.push(("a chunked provider answer's completing part (tag 152)".into(), payload_of(&answer_chunks[1])));
    for (kind_tag, _, _) in PALW_LIFECYCLE_NEW_KINDS_V1 {
        assert!(
            payloads.iter().any(|(_, p)| p.get(2) == Some(kind_tag)),
            "kind {kind_tag} is carried (the payload's object tag follows its u16 version)"
        );
    }
    carry(&mut a, &mut wallet, &config, &payloads).await;

    // ---- the mixed block: an ATTEMPT block (its claim folds) carrying a licence that folds, a duplicate the walk refuses, and new
    //      kinds riding unjudged ----
    a.attempt_at_the_anchor_slot(claim_id, 7).await;
    let licence = licence_for(&a, claim_id);
    assert_eq!(
        licence.is_some(),
        ruleset == Ruleset::Launch,
        "the launch ruleset licenses by receipts, the release by the vertex tally"
    );
    let mut mixed: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(licence) = &licence {
        mixed.push(("the claim's licence (a live-build kind that folds)".into(), payload_of(licence)));
        mixed.push(("the same licence again (a live-build kind the walk refuses)".into(), payload_of(licence)));
    }
    mixed.extend::<Vec<(String, Vec<u8>)>>(vec![
        ("a kernel route object".into(), payload_of(&well_formed(bond)[6])),
        ("an unsigned onboarding object".into(), payload_of(&malformed(bond)[0])),
        ("a Panel V3 proof".into(), payload_of(&well_formed(bond)[8])),
        ("the reference".into(), reference),
        ("a zero-filled kernel receipt".into(), payload_of(&zero_filled_kind(111))),
        ("a signed provider answer (tag 152)".into(), payload_of(&provider_answer())),
    ]);
    let mixed_bytes: usize = mixed.iter().map(|(_, p)| p.len() + CARRIER_OVERHEAD_BYTES).sum();
    assert!(mixed.len() <= 8 && mixed_bytes <= BLOCK_CARRIAGE_BYTES, "the mixed carriers fit ONE block ({mixed_bytes} bytes)");
    let txs: Vec<Transaction> =
        mixed.iter().enumerate().map(|(card, (_, payload))| wallet.carrier(&config, card, payload.clone())).collect();
    let (attempt_block, mixed_claim) = a.attempt(3, ttpb, txs.clone(), &|_| true).await;
    for (tx, (what, _)) in txs.iter().zip(&mixed) {
        assert!(attempt_block.transactions.iter().any(|t| t.id() == tx.id()), "carried by the attempt block, valid: {what}");
    }
    a.heartbeat(ttpb, Vec::new()).await; // accepts the attempt block's carriers
    let (_, state) = a.tip_state();
    assert!(state.claim(&mixed_claim).is_some(), "the attempt block's own claim folded beside the carriers");
    if licence.is_some() {
        assert!(
            matches!(state.claim(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "the live-build kind in the mixed block folded: the claim is licensed ({:?})",
            state.claim(&claim_id).unwrap().phase
        );
    }

    // ---- nothing was folded, charged or opened past what the live build does ----
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
    for opening in [&two_chunks[0], &answer_chunks[0]] {
        if let Some(pending) = state.pending_chunk_group(&group_of(opening)) {
            assert_eq!(pending.parts.keys().copied().collect::<Vec<_>>(), vec![0], "the completing part was refused, as undecodable");
        }
    }
    // The walk's chunk reader — the certification cap's input — answers "undecodable" below the fence and the kind past it.
    let daa = a.daa_of(a.sink());
    for chunk in &one_chunk {
        assert!(a.vp().palw_kernel_chunk_inner(&state, chunk, daa).is_none(), "below the fence: undecodable to the cap");
        let inner = now.vp().palw_kernel_chunk_inner(&state, chunk, daa).expect("past the fence: the kind");
        assert!(matches!(palw_lifecycle_kind_owner_v1(&inner), PalwLifecycleKindOwnerV1::Fence(_)));
    }

    // ---- the PFS4 header: the live build's refusal, on the header path and on the pruning-proof path ----
    let pfs4 = pfs4_block(&mut a, &config);
    let live = live_build_shape_refusals(&pfs4);
    let err = a.ctx.consensus.validate_and_insert_block(pfs4.clone().to_immutable()).virtual_state_task.await.expect_err("refused");
    let header_refusal = err.to_string();
    assert!(
        live.iter().any(|e| RuleError::BadPalwCommitmentShape(e.to_string()).to_string() == header_refusal),
        "the header path refuses a PFS4 header with the live build's gate's refusal: {header_refusal} (the gate's: {live:?})"
    );
    let mut proof: Vec<Vec<Arc<kaspa_consensus_core::header::Header>>> = vec![Vec::new(); config.params.max_block_level as usize + 1];
    proof[0].push(Arc::new(pfs4.header.clone()));
    let proof_refusal =
        |chain: &T12Chain| chain.ctx.consensus.validate_pruning_proof_standalone(&proof).expect_err("refused").to_string();
    let a_proof = proof_refusal(&a);
    assert!(
        live.iter().any(|e| {
            kaspa_consensus_core::errors::pruning::PruningImportError::PruningProofBadPalwCommitment(
                pfs4.header.hash,
                0,
                e.to_string(),
            )
            .to_string()
                == a_proof
        }),
        "the pruning-proof path refuses it with the live build's gate's refusal: {a_proof}"
    );
    assert_ne!(proof_refusal(&now), a_proof, "past the fence the form is the V4 carriage, judged by its own decode");
    let refused = vec![(pfs4.to_immutable(), header_refusal)];

    // ---- every node agrees, block by block ----
    let blocks = chain_blocks(&a);
    let roots = roots_along(&a, &blocks);
    for (other, other_bundle, name) in [(&config, &bundle, "unarmed"), (&armed_far, &armed_far_bundle, "armed far above")] {
        let node = replay(other, other_bundle, &premine, &floats, &blocks, &refused).await;
        assert_eq!(roots_along(&node, &blocks), roots, "the {name} node: the PALW state root at every block");
        assert_eq!(view(&node), view(&a), "the {name} node: sink, PALW root, UTXO multiset");
        assert_eq!(proof_refusal(&node), a_proof, "the {name} node's pruning proof");
    }
    assert_ne!(plain_view.0, view(&a).0, "the carriers moved the chain");
    eprintln!(
        "[a2u {tag}] {} chain blocks to DAA {}, {} carriers, 1 refused header; every node agrees",
        blocks.len(),
        a.daa_of(a.sink()),
        payloads.len() + mixed.len()
    );

    (
        match ruleset {
            Ruleset::Launch => 0,
            Ruleset::Release => 1,
        },
        config.params.genesis.hash,
        blocks.iter().map(|b| ((*b.header).clone(), (*b.transactions).clone())).collect(),
        refused.iter().map(|(b, why)| ((*b.header).clone(), (*b.transactions).clone(), why.clone())).collect(),
        a_proof,
        roots,
        view(&a),
    )
}

/// Write `dump` for the replay through int-12 when `A2U_INT12_REPLAY_OUT=<dir>` is set.
fn write_dump(ruleset: Ruleset, dump: &Dump) {
    if let Ok(dir) = std::env::var("A2U_INT12_REPLAY_OUT") {
        let path = std::path::Path::new(&dir).join(format!("a2u-{}.borsh", ruleset.name()));
        std::fs::write(&path, borsh::to_vec(dump).expect("serializes")).expect("writes the replay file");
        eprintln!("[a2u] wrote {} blocks and {} refusals to {}", dump.2.len(), dump.3.len(), path.display());
    }
}

#[tokio::test]
async fn t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_launch_ruleset() {
    let dump = the_mixed_verdict_chain(Ruleset::Launch).await;
    write_dump(Ruleset::Launch, &dump);
}

#[tokio::test]
async fn t12_a2u_mixed_verdicts_every_new_kind_below_its_fence_release_ruleset() {
    let dump = the_mixed_verdict_chain(Ruleset::Release).await;
    write_dump(Ruleset::Release, &dump);
}
