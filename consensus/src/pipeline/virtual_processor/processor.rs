use crate::{
    consensus::{
        services::{
            ConsensusServices, DbBlockDepthManager, DbDagTraversalManager, DbGhostdagManager, DbParentsManager, DbPruningPointManager,
            DbWindowManager,
        },
        storage::ConsensusStorage,
    },
    constants::BLOCK_VERSION,
    errors::RuleError,
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            DB,
            acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            block_window_cache::{BlockWindowCacheStore, BlockWindowCacheWriter},
            daa::DbDaaStore,
            depth::{DbDepthStore, DepthStoreReader},
            dns_state::{DbDnsStateStore, DnsStateStoreReader},
            epoch_accumulator::{DbBlockQualityPoolStore, DbEpochAccumulatorStore, DbReserveBalanceStore},
            evm::{
                DbEvmCanonicalHeadsStore, DbEvmDepositLockStore, DbEvmHeaderStore, DbEvmPayloadStore, DbEvmStateStore,
                EvmCanonicalHeadsStoreReader, EvmDepositLockStore, EvmDepositLockStoreReader, EvmHeaderStore, EvmHeaderStoreReader,
                EvmStateStore, EvmStateStoreReader,
            },
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            headers_selected_tip::{DbHeadersSelectedTipStore, HeadersSelectedTipStoreReader},
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningStoreReader},
            pruning_meta::PruningMetaStores,
            pruning_overlay_snapshot::{DbPruningPointOverlaySnapshotStore, PruningPointOverlaySnapshotStoreReader},
            pruning_samples::DbPruningSamplesStore,
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            rewarded_epochs::{DbRewardedEpochsStore, RewardedEpochKeys, RewardedEpochsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore, SelectedChainStoreReader},
            stake_bonds::{DbStakeBondsStore, StakeBondsStoreReader},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            virtual_state::{LkgVirtualState, VirtualState, VirtualStateStoreReader, VirtualStores},
        },
    },
    params::Params,
    pipeline::{
        ProcessingCounters, deps_manager::VirtualStateProcessingMessage, pruning_processor::processor::PruningProcessingMessage,
        virtual_processor::utxo_validation::UtxoProcessingContext,
    },
    processes::{
        coinbase::CoinbaseManager,
        ghostdag::ordering::SortableBlock,
        transaction_validator::{TransactionValidator, errors::TxResult, tx_validation_in_utxo_context::TxValidationFlags},
        window::WindowManager,
    },
};
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::{
    BlockHash, BlockHashMap, BlockHashSet, BlueWorkType, ChainPath, Hash64,
    acceptance_data::AcceptanceData,
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    block::{
        BlockTemplate, EvmClaimStaleKind, MutableBlock, TemplateBuildMode, TemplateTransactionSelector,
        TemplateTransactionSelectorFactory,
    },
    blockstatus::BlockStatus::{StatusDisqualifiedFromChain, StatusUTXOValid},
    coinbase::MinerData,
    config::genesis::GenesisBlock,
    dns_finality::{
        ATTESTATION_MLDSA87_CONTEXT, ActiveBondView, AttestationContribution, BlockEpochContribution, BlockOverlayContribution,
        BondMutation, CanonicalLaggedEpochAnchor, DnsCoinbaseSettlement, DnsParams, DnsReorgMode, DnsReorgOutcome, DnsRolloutStage,
        OverlaySnapshot, PruningPointOverlaySnapshot, StakeBondRecord, StakePreferenceInputs, StakeScore, UNBOND_REQUEST_CONTEXT,
        advance_dns_confirmation, aggregate_epoch_tallies, anchor_cutoff_blue_score, apply_bond_stamp, attestations_from_accepted_txs,
        bond_mutations_from_accepted_txs, canonical_lagged_epoch_anchor, check_dns_reorg_rule, compute_stake_score, derive_dns_health,
        dns_finality_fresh_for_bridge, effective_bond_status, is_bond_active_at, is_dns_confirmed, ready_epoch_from_tip_blue_score,
        recompute_epoch_tallies, reorg_inputs_since_common_ancestor, revert_bond_stamp, stake_attestation_message,
        stake_preference_verdict, total_active_stake_by_epoch, unbond_request_message, unbond_requests_from_accepted_txs,
        validator_id_from_pubkey,
    },
    header::Header,
    merkle::calc_hash_merkle_root,
    mining_rules::MiningRules,
    pruning::PruningPointsList,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry},
    utxo::{
        utxo_diff::{ImmutableUtxoDiff, UtxoDiff},
        utxo_view::{UtxoView, UtxoViewComposition},
    },
};
use kaspa_consensus_notify::{
    notification::{
        NewBlockTemplateNotification, Notification, PalwClassReadinessChangedNotification, PalwPanelAssignmentNotification,
        PalwPanelAssignmentSeatNotification, PalwPanelEligibilityChangedNotification, PalwPanelReceiptNotification,
        SinkBlueScoreChangedNotification, UtxosChangedNotification, VirtualChainChangedNotification,
        VirtualDaaScoreChangedNotification,
    },
    root::ConsensusNotificationRoot,
};
use kaspa_consensusmanager::SessionLock;
use kaspa_core::{debug, error, info, time::unix_now, trace, warn};
use kaspa_database::prelude::{StoreError, StoreResultExt, StoreResultUnitExt};
use kaspa_hashes::ZERO_HASH64;
use kaspa_muhash::MuHash;
use kaspa_notify::{events::EventType, notifier::Notify};
use once_cell::unsync::Lazy;

use super::errors::{PruningImportError, PruningImportResult};
use crossbeam_channel::{Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use itertools::Itertools;
use kaspa_consensus_core::tx::ValidatedTransaction;
use kaspa_txscript::verify_mldsa87_with_context;
use kaspa_utils::binary_heap::BinaryHeapExtensions;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rand::{Rng, seq::SliceRandom};
use rayon::{
    ThreadPool,
    prelude::{IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelIterator},
};
use rocksdb::{WriteBatch, WriteOptions};
use std::{
    cmp::min,
    collections::{BTreeMap, BinaryHeap, HashMap, HashSet, VecDeque},
    ops::Deref,
    sync::{Arc, atomic::Ordering},
};

/// O9 (optimization design v0.1): rolling EVM-lane throughput counters.
/// Recorded only on the `evm` chain-context step, so it is dead on the default
/// (secp-free, non-`evm`) node — silence the dead-code lint there.
#[cfg_attr(not(feature = "evm"), allow(dead_code))]
#[derive(Default)]
pub(super) struct EvmLaneKpi {
    chain_blocks: std::sync::atomic::AtomicU64,
    mergeset_blocks: std::sync::atomic::AtomicU64,
    accepted_gas: std::sync::atomic::AtomicU64,
    // kaspa-pq EVM bridge observability: cumulative deposit-claims APPLIED in
    // accepted chain blocks. Surfaced in the KPI line because accepted-gas
    // utilization rounds to 0.00% even for several successful claims (one claim
    // ≈ 25k gas of the 30M cap ≈ 0.00065%), so "0.00%" must NOT be read as "zero
    // claims succeeded" — this counter is the direct success signal.
    applied_claims: std::sync::atomic::AtomicU64,
}

#[cfg_attr(not(feature = "evm"), allow(dead_code))]
impl EvmLaneKpi {
    /// Record one validated EVM chain block (and the deposit claims it applied);
    /// periodically logs the rolling averages + cumulative applied claims (every
    /// 256 chain blocks).
    pub(super) fn record(&self, mergeset_size: usize, gas_used: u64, claims_applied: usize) {
        use std::sync::atomic::Ordering;
        let n = self.chain_blocks.fetch_add(1, Ordering::Relaxed) + 1;
        let ms = self.mergeset_blocks.fetch_add(mergeset_size as u64, Ordering::Relaxed) + mergeset_size as u64;
        let gas = self.accepted_gas.fetch_add(gas_used, Ordering::Relaxed) + gas_used;
        let claims = self.applied_claims.fetch_add(claims_applied as u64, Ordering::Relaxed) + claims_applied as u64;
        if n.is_multiple_of(256) {
            let cap = kaspa_consensus_core::evm::MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK as f64;
            info!(
                "EVM lane KPI (O9): {} chain blocks, avg mergeset {:.2}, avg accepted-gas utilization {:.2}%, {} deposit-claims applied (cumulative)",
                n,
                ms as f64 / n as f64,
                (gas as f64 / n as f64) / cap * 100.0,
                claims
            );
        }
    }
}

/// RFC-0009 stage D: how far behind the sink (in DAA) a block may be for this node to rebuild the PALW state its header commits and serve a
/// proof of it. Older: the client pins a newer checkpoint.
pub const PALW_STATE_PROOF_MAX_LAG_DAA: u64 = 4_000;

/// RFC-0009 stage D: the most one flat state proof may carry (the preimage plus every row of the collection).
pub const PALW_STATE_PROOF_MAX_BYTES: usize = 24 * 1024 * 1024;

/// **The grading work one block may demand of every node, in FAULT VECTORS** (ADR-0075 Decision 9,
/// widened by the audit of 2026-09-02).
///
/// D9 bounded the work by counting OBJECTS, and that conflated two things a block can carry: an
/// object that made this node re-execute eight merkle-proved refutation steps, and an object the
/// court threw out at its first line without touching one. Both spent the same scarce slot, so two
/// ordinary lifecycle transactions — no signature, no deposit — carrying a `FamilyCertified` with
/// an empty vector list dropped every genuine certification in the block, which is a block-cheap
/// way to keep an honest class weightless forever.
///
/// A vector is what the grader actually spends time on (`certify_e2e_family_v1` walks them, and
/// every per-vector step is a proof check), so the CPU bound is stated in vectors. The number is
/// EXACTLY the worst case the object cap already permitted — `PALW_CERTIFICATION_MAX_PER_BLOCK`
/// objects each carrying the per-object maximum `PALW_CERTIFICATION_MAX_VECTORS` — so nothing a
/// node may be asked to compute grows; what changes is who pays.
const PALW_CERTIFICATION_GRADING_VECTORS_PER_BLOCK: usize = kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_PER_BLOCK
    * kaspa_consensus_core::palw_state_v2::PALW_CERTIFICATION_MAX_VECTORS;

/// **Why this node cannot weigh a PALW candidate — and the two reasons are not one reason.**
///
/// [`VirtualStateProcessor::palw_candidate_state_v2`] used to answer `None` to both, which is the
/// `.ok().flatten()` conflation the pruning ceiling beside it was fixed for. It matters because
/// one consumer reads "nothing to weigh" as ALLOW: `palw_frontier_provenance_outcome` returns
/// `GateInactive` and the deep reorg proceeds. A gate that fails open on its own state fault is
/// the shape ADR-0042 Decision 5 forbids ("reading absent data as nothing is forbidden").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PalwWeighFaultV2 {
    /// An honest absence: not a ConsensusV2 network, no V2 tip written yet, a candidate this
    /// consensus does not hold, or a chain path whose deltas have been pruned. An abstention.
    NoOpinion,
    /// This node's OWN root-verified snapshot, or a delta row on the path to the candidate, would
    /// not read back — a disk that flipped a byte, or a row left by another schema. Not an
    /// absence: a fault, and a gate that can refuse must. Both ends are classified, because a
    /// split at the tip alone leaves the identical fail-open at the walk one line below it.
    StoreUnreadable,
    /// The local quota could not admit an actual typed state/walk request.
    /// Neither honest absence nor evidence of a faulty candidate/store.
    NativeCapacityUnavailable,
}

/// **How one chain block stands as a panel anchor** ([`VirtualStateProcessor::palw_chain_block_as_anchor_v1`]).
struct PalwChainAnchorV1 {
    /// The highest claim slot the block anchors: its own DAA score, or past lane A the latest operator
    /// attempt it is or merges, capped at its own DAA score.
    reach: u64,
    /// Past lane A, the operator attempts the block is or merges, `(DAA score, hash)`; `None` below it,
    /// where the anchor block keys the seed itself. The seed's source among them is
    /// [`VirtualStateProcessor::palw_operator_seed_source_v1`]'s.
    operator_attempts: Option<Vec<(u64, BlockHash)>>,
    /// **RFC-0018 §5 (`palw_panel_anchor_permissionless_v1`)**: past it, every bonded, matured attempt the block is or merges (blue or
    /// red) — the permissionless anchor's contributions; `None` below it.
    permissionless: Option<Vec<kaspa_consensus_core::palw_rfc0018_v1::PalwAnchorContributionV1>>,
}

/// **G14 lane D, TEST ONLY: the artifact attestations a kernel route E2E supplies.** There is no on-chain artifact availability or
/// conformance fact yet — a kernel class registers only over an artifact the consumer attests public, and the consumer has nothing
/// to attest from (GAP: onboarding conformance + availability) — so under `cfg(test)` a test names the roots it treats as public,
/// each from a DAA. Height-gated and append-only so every node of one test, replaying the same blocks, folds the same attestation
/// at the same block; never derived from a declaration.
#[cfg(test)]
static KERNEL_ROUTE_TEST_ATTESTATIONS_V1: std::sync::Mutex<Vec<(kaspa_hashes::Hash64, u64)>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(crate) fn kernel_route_test_attest_artifact_v1(artifact_root: kaspa_hashes::Hash64, from_daa: u64) {
    let mut list = KERNEL_ROUTE_TEST_ATTESTATIONS_V1.lock().unwrap();
    if !list.iter().any(|(root, _)| *root == artifact_root) {
        list.push((artifact_root, from_daa));
    }
}

/// **OPV-BOOT, TEST ONLY: the OPV classes a pre-derivation mechanics test treats as eligible.** RFC-0015 eligibility is DERIVED from
/// chain state (`palw_opv_bootstrap_v1::opv_eligibility_v1`); the OPV mechanics worlds written against the retired manual admission
/// list (`g14_opv_*`, `g14_conformance_*`) name their classes here instead. Like the attestation hook it exists only under
/// `cfg(test)`: the extras field it fills is `Vec::new()` in every build that can run a network (pinned by
/// `opv_test_eligibility_hook_is_test_only`). Append-only and process-wide, so every node of one test folds the same set.
#[cfg(test)]
static KERNEL_ROUTE_TEST_OPV_ELIGIBLE_V1: std::sync::Mutex<Vec<kaspa_hashes::Hash64>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(crate) fn kernel_route_test_opv_eligible_v1(class: kaspa_hashes::Hash64) {
    let mut list = KERNEL_ROUTE_TEST_OPV_ELIGIBLE_V1.lock().unwrap();
    if !list.contains(&class) {
        list.push(class);
    }
}

/// The seam's inverse (`cfg(test)`): a class taken off the list, so a test can make a component lose its eligibility after a typed
/// class derived from it registered (lane G14C: the commit-time re-gate of typed claims).
#[cfg(test)]
pub(crate) fn kernel_route_test_opv_ineligible_v1(class: kaspa_hashes::Hash64) {
    KERNEL_ROUTE_TEST_OPV_ELIGIBLE_V1.lock().unwrap().retain(|c| *c != class);
}

#[cfg(test)]
fn kernel_route_test_opv_eligible_list_v1() -> Vec<kaspa_hashes::Hash64> {
    let mut list = KERNEL_ROUTE_TEST_OPV_ELIGIBLE_V1.lock().unwrap().clone();
    list.sort();
    list
}

#[cfg(test)]
fn kernel_route_test_attestations_v1(daa_score: u64) -> Vec<kaspa_hashes::Hash64> {
    let mut roots: Vec<_> = KERNEL_ROUTE_TEST_ATTESTATIONS_V1
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, from)| *from <= daa_score)
        .map(|(root, _)| *root)
        .collect();
    roots.sort();
    roots
}

/// **RFC-0008 v2 (X8R), TEST ONLY: the classes whose REAL attempts skip the model-registry, seating and per-bond-share gates** — the
/// stand-in, in the composed EXEC-slice × G14 run (`g14_kernel_route_e2e::conformance::exec_slices`), for the admission of the session's
/// REAL root. **Re-scoped by the Lead's GAP-81 decision (2026-10-10):** a V2 REAL claim of an onboarded class earns only on the LEGACY
/// channel, whose old rules need Panel seat readiness (possession proofs) that this harness cannot produce — so this stands for that
/// channel's admission; no reward gate can replace it (the gate grants a V2 root nothing). Named and shaped like
/// [`kernel_route_test_attest_artifact_v1`]: height-gated and
/// append-only, so every node of one test, replaying the same blocks, waives the same gates at the same block; empty in every build
/// that is not a test (`PalwKernelRouteExtrasV1::test_admitted_classes`).
#[cfg(test)]
static EXEC_V2_TEST_ADMITTED_CLASSES_V1: std::sync::Mutex<Vec<(kaspa_hashes::Hash64, u64)>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(crate) fn exec_v2_test_admit_class_v1(class_id: kaspa_hashes::Hash64, from_daa: u64) {
    let mut list = EXEC_V2_TEST_ADMITTED_CLASSES_V1.lock().unwrap();
    if !list.iter().any(|(class, _)| *class == class_id) {
        list.push((class_id, from_daa));
    }
}

#[cfg(test)]
fn exec_v2_test_admitted_classes_v1(daa_score: u64) -> Vec<kaspa_hashes::Hash64> {
    let list = EXEC_V2_TEST_ADMITTED_CLASSES_V1.lock().unwrap();
    let mut classes: Vec<_> = list.iter().filter(|(_, from)| *from <= daa_score).map(|(class, _)| *class).collect();
    classes.sort();
    classes
}

/// Rows that must share a UTXO commit. Genesis has no earlier durable PALW tip
/// to replay from, so its initial materialization must ride the delta and status.
fn native_store_capacity_v1(
    error: kaspa_database::prelude::StoreError,
) -> kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1 {
    match error {
        kaspa_database::prelude::StoreError::LocalResourceUnavailable(reason) => {
            kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
            >("native store capacity", &reason)
        }
        other => panic!("unexpected native store failure: {other}"),
    }
}
struct PalwUtxoCommitV2 {
    root: Hash64,
    delta: kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2,
    _native_delta_lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    genesis: Option<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>,
    _native_genesis_carriages_lease: Option<kaspa_consensus_core::palw_native_resource_v1::NativeDemandLeaseV1>,
    genesis_carriages: Vec<(Hash64, crate::model::stores::palw_class_carriage::PalwClassCarriageRecord)>,
}

pub struct VirtualStateProcessor {
    // Channels
    receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
    pruning_sender: CrossbeamSender<PruningProcessingMessage>,
    pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    pub(super) db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: kaspa_consensus_core::config::params::ForkedParam<u16>,
    pub(super) mergeset_size_limit: kaspa_consensus_core::config::params::ForkedParam<u64>,
    /// ADR-0068 Phase 1 (F3a): the heartbeat width bound's fence (same activation as the lane —
    /// `Params::palw_heartbeat_width_fence`). `Some` = templates keep their mergesets within
    /// `PALW_HEARTBEAT_MAX_PER_MERGESET` heartbeat members, chunking sibling floods exactly as
    /// `mergeset_size_limit` chunks size.
    pub(super) palw_heartbeat_width_fence: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The deepest reorg the sink search will ever offer — ADR-0065 D2's ancestor horizon.
    pub(super) finality_depth: u64,
    /// kaspa-pq Phase 3 PoW (ADR-0007): BLAKE2b-512 ∥ SHA3-512 (`algo_id = 3`) activation — sets the
    /// block template's `pow_algo_id` so miners produce the network-correct Layer-1 algorithm.
    pub(super) pow_blake2b_sha3_activation: kaspa_consensus_core::config::params::ForkActivation,
    /// MISAKA Phase 4 PoW: PALW deterministic-LLM (`algo_id = 4`) activation — supersedes the
    /// BLAKE2b-SHA3 rule for the template's `pow_algo_id` where active.
    pub(super) pow_palw_activation: kaspa_consensus_core::config::params::ForkActivation,
    /// MISAKA Phase 4b PoW: PALW-Ollama (`algo_id = 5`) activation — supersedes everything.
    pub(super) pow_palw_ollama_activation: kaspa_consensus_core::config::params::ForkActivation,
    /// ADR-0042 Decision 1 (PR-08 seam): the algo id a `ConsensusV2` network demands, or `None`
    /// on every non-V2 network. Computed once from `params.palw_consensus_mode`; consulted first
    /// by the header-declaration gate so the mode's demand and the V1 cascade agree in one place.
    pub(super) palw_required_algo_id: Option<u8>,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) daa_excluded_store: Arc<DbDaaStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_point_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub(super) depth_store: Arc<DbDepthStore>,
    pub(super) selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
    pub(super) pruning_samples_store: Arc<DbPruningSamplesStore>,
    /// The HEADER DAG's selected tip. Read only by `pruning_point_witness_child`, which needs an
    /// anchor an attacker cannot grind — see re-audit R-3. Headers are synced before the utxoset
    /// sidecars in every IBD path, so this is populated by the time that runs.
    pub(super) headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,

    // kaspa-pq Phase 10 (ADR-0009): DNS finality overlay. `dns_params` is the
    // dormancy guard — `None` on every current network, so the bond-population
    // pass below is a single `Option` check and a return.
    pub(super) stake_bonds_store: Arc<RwLock<DbStakeBondsStore>>,
    /// Accepted capability declarations. A store rather than a walk product: a declaration
    /// outlives the credit window by three orders of magnitude, so a walk-scoped copy vanishes
    /// while it is still in force and takes the certificate's whole committee with it.
    /// ADR-0067: accepted registrations' declarations, for the serve-from-chain arm.
    pub(super) palw_class_carriage_store: Arc<RwLock<crate::model::stores::palw_class_carriage::DbPalwClassCarriageStore>>,
    /// ADR-0042 Decision 5 / ADR-0044 Unit C: per-chain-block `PalwStateDeltaV2` rows and the
    /// materialized tip. Written in the same `WriteBatch` as the block's UTXO data by the walk in
    /// `calculate_utxo_state_relatively`, so the two can never be half-written relative to each
    /// other. Empty on every shipped preset — the walk is gated on `ConsensusV2`.
    pub(super) palw_state_v2_store: Arc<RwLock<crate::model::stores::palw_state_v2::DbPalwStateV2Store>>,
    /// The V2 state parameters, `Some` exactly when this network's mode is `ConsensusV2`. It is
    /// the ONE gate on the state walk: a network without a V2 bundle has no V2 state to keep, and
    /// a dead handle in a blue-work pipeline would be surface without semantics (ADR-0042
    /// Decision 9's own words about the fork-choice sites).
    pub(super) palw_state_params_v2: Option<kaspa_consensus_core::palw_state_v2::PalwStateParamsV2>,
    /// **RFC-0010, TESTS ONLY: the beacon schemes and source a test chain resolves** in place of the release's (an empty registry and the
    /// chain's own settlements, which can approve and verify nothing). Compiled out of every non-test build, so no running node can
    /// carry it; the fence it serves is itself refused at every real height.
    #[cfg(test)]
    pub(super) panel_v3_test_beacon: parking_lot::Mutex<
        Option<(
            Vec<kaspa_consensus_core::palw_panel_beacon_v1::challenge::PostCommitChallengePolicyV1>,
            kaspa_consensus_core::palw_state_v2::PalwPanelV3BeaconSourceV1,
        )>,
    >,
    /// ADR-0044's free-prompt bundle, `Some` on the same networks as `palw_state_params_v2`. It is
    /// what turns an accepted transaction into a consensus object (Unit C step 3).
    pub(super) palw_freeprompt_params_v3: Option<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptParamsV3>,
    /// ADR-0042 Decision 6's params, `Some` on the same networks as `palw_state_params_v2`.
    pub(super) palw_admission_params_v2: Option<kaspa_consensus_core::palw_admission_v2::PalwAdmissionParamsV2>,
    /// The V2 bond policy — read for one thing: the withdrawal delay a retiring bond's collateral
    /// stays locked for (audit C-08's spend gate).
    pub(super) palw_bond_params_v2: Option<kaspa_consensus_core::palw_mode_v2::PalwBondParamsV2>,
    /// The whole ruleset, kept because `verify_class_admission_v2` takes the bundle rather than a
    /// slice of it (ADR-0049 Decision H): the court's ladder, its three cost ceilings and the
    /// base class id are all consulted in one gate, and handing it four fields would be four
    /// places for one ruleset to be assembled differently.
    pub(super) palw_v2_bundle: Option<kaspa_consensus_core::palw_mode_v2::PalwConsensusParamsV2>,
    /// Decision 7's panel constants. Decision 10's producer carve is NOT here: it moved into
    /// `palw_state_params_v2`, because the escrow it decides is written into claim state at
    /// acceptance, and a number two structs can disagree about is a number the chain enforces
    /// twice. `PalwConsensusParamsV2::validate` holds the bundle's declared carve equal to it.
    pub(super) palw_panel_params_v2: Option<kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2>,
    /// Decision 8's court shape — the ladder depth a challenge is opened over.
    pub(super) palw_court_params_v2: Option<kaspa_consensus_core::palw_mode_v2::PalwCourtParamsV2>,
    /// The bundle's genesis registration list — what the genesis block applies (Decision 11: the
    /// ruleset id covers it, so two networks sharing an id register the same classes).
    pub(super) palw_genesis_objects_v2: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
    /// The network's own name bytes — the ONE source `palw_network_domain_v2` is derived from, so
    /// the domain a payload must bind and the domain the Layer-0 digest binds are one fact.
    pub(super) network_id_bytes: Vec<u8>,
    pub(super) dns_state_store: Arc<RwLock<DbDnsStateStore>>,
    // kaspa-pq ADR-0022: overlay snapshot as-of the pruning point (serve + below-pp window consult).
    pub(super) pruning_overlay_snapshot_store: Arc<RwLock<DbPruningPointOverlaySnapshotStore>>,
    pub(super) dns_params: Option<DnsParams>,
    /// ADR-0128: the DNS validators' BFT vote and the stake reorg gate that follows it, where the
    /// network runs an overlay (`Params::dns_bft_gate_fence`). `None` on every shipped preset.
    pub(super) dns_bft_gate: Option<kaspa_consensus_core::config::params::DnsBftGateV1>,
    /// ADR-0128's node-local runtime state: whether the last evaluation covered its walk (the gate
    /// abstains while it did not), and the memoised signature verdicts and duty evaluation. Nothing
    /// in it is consensus.
    pub(super) dns_bft_runtime: super::dns_bft::DnsBftRuntime,
    /// ADR-0038 Decision A: the network's PALW commitment fence. `None` on every shipped preset.
    pub(super) palw_block_commitment: Option<kaspa_consensus_core::palw_block_commitment::PalwBlockCommitmentParamsV1>,
    /// ADR-0064's fence, `None` on every shipped preset. See
    /// [`Self::palw_v2_check_attempt_admission`] for what it admits and, more to the point, for
    /// what it deliberately does not.
    pub(super) palw_bootstrap_activation: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0065 D4's fence, `None` on every shipped preset. Past it an `Unavailable` receipt
    /// decides neither quorum and is never charged as a dissent — see
    /// [`Self::palw_unavailable_abstains_at`], which is the ONE place this is resolved.
    pub(super) palw_unavailable_abstains: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0065 D1's fence and window, `None` on every shipped preset. See
    /// [`Self::palw_bond_maturity_at`], which is the ONE place this is resolved.
    pub(super) palw_bond_maturity: Option<kaspa_consensus_core::config::params::PalwBondMaturityV1>,
    /// Lane maturity's fence (post-launch, 2026-09-26: ADR-0065 D1 brought forward), MODE-folded
    /// (`Params::palw_bond_maturity_early_fence`), `None` on every shipped preset. Read only through
    /// [`Self::palw_bond_maturity_at`], beside the field above whose window it applies.
    pub(super) palw_bond_maturity_early: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0071 SA-1..SA-4's fence, `None` on every shipped preset. Past it a capability
    /// declaration is bounded, priced and refused for a retiring bond, and a seat is drawn for a
    /// class only after a production fact. See [`Self::palw_capability_bound_at`], which is the
    /// ONE place this is resolved — the fold, the acceptance layer and the panel assembler must
    /// all get the same answer for the same block, or a derived panel and a proposed one differ
    /// and every claim voids.
    pub(super) palw_capability_bound: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0075 SA-1/SA-2's fence, `None` on every shipped preset. Past it a chunk group's opener
    /// and a graded `FamilyCertified` must have been carried by a transaction that paid the rent
    /// their occupancy costs. See [`Self::palw_certification_rent_at`], the ONE place it is
    /// resolved — the fee collection in `calculate_utxo_state` and the acceptance filter that
    /// spends it must agree, or the filter reads an empty map and silently admits everything.
    pub(super) palw_certification_rent: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0082 Decision 3's fence, `None` on every shipped preset.** Past it a block may carry
    /// the three moves of a fused-attention dissection, and the court a session is judged under
    /// takes the DERIVED arity instead of the bundle's binary one. Resolved in exactly one place
    /// ([`Self::palw_kary_court_active_at`]) for the reason every fence above it is: the
    /// acceptance filter that admits a move and the court that grades the close it ends in must
    /// get the same answer for the same block, or one node's session has a phase another node's
    /// does not.
    pub(super) palw_kary_court: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// Per-model finite adjudication horizons, activated independently at the fleet flag day.
    pub(super) palw_model_court_window: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0081 Decision 3 / ADR-0082 Decision 5's fence, `None` on every shipped preset** —
    /// and un-armable on this build (`Params::validate_palw_v2` refuses it, audit D M-2). Held
    /// here so the `ClassRegistered` arm reads the form from the ONE place that decides it
    /// (`Params::palw_prompt_ids_form_at`) rather than spelling `Flat` at the application site.
    pub(super) palw_prompt_ids_merkle: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0077 Decision 16's `PanelDa` fence** (`Params::palw_panel_da_fence`, mode folded in),
    /// `None` on every shipped preset. The extraction walk reads it at the accepting block's DAA:
    /// a mode-2 commitment becomes a claim only past it. Held here because the walk used to pass a
    /// literal `false` — which on a genesis that arms the fence would have refused every private
    /// commitment the door had admitted.
    pub(super) palw_panel_da: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0084 U-08's fence, `None` on every shipped preset.** Past it a court close is walked
    /// at the ruleset's step ladder (`PalwCourtParamsV2::max_step_leaf_count`); before it at
    /// `PALW_STEP_LEG_MAX_LEAVES`. Resolved in exactly one place, `palw_court_step_ladder_at`,
    /// at the BLOCK's own DAA — the same discipline as every fence above it.
    pub(super) palw_court_ladder: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **The fused terminal's responder-coverage fence, `None` on every shipped preset** (mainnet
    /// audit 2026-09-06, C-2/H-5). Past it a fused terminal's unanswered rung ends the session
    /// without a conviction, because no party in the field can file the move it is being clocked
    /// for; before it, it convicts exactly as it does today. Resolved in exactly one place,
    /// `palw_court_responder_coverage_at`, at the BLOCK's own DAA.
    pub(super) palw_court_responder_coverage: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0072 Decision 8's free-prompt half, `None` on every shipped preset** (mainnet audit
    /// 2026-09-06, M-3). Past it the transition DERIVES a free-prompt claim's retention obligation
    /// from this block's DAA and its chunk count from the run's own shape; before it both are the
    /// numbers the producer wrote, one of which bought permanent immunity from the DA court.
    /// Resolved in exactly one place, [`Self::palw_fp_da_pins_at`], at the BLOCK's own DAA.
    pub(super) palw_fp_da_pins: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0107: share growth counts Final work. Resolved in exactly one place,
    /// [`Self::palw_share_growth_final_at`], at the BLOCK's own DAA.
    pub(super) palw_share_growth_final: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0087 Decision 6's fence, `None` on every shipped preset.** Past it `ModelBuy` and
    /// `ModelSell` are accepted (a sell's signature checked here, at acceptance); before it both
    /// are refused by name and the fold never sees them. Resolved at the BLOCK's DAA.
    pub(super) palw_model_market: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0088 Decision 11's fence, `None` on every shipped preset.** Past it the ten registry
    /// objects are accepted (their signatures checked here, against the bond each is attributed
    /// to) and the fold attributes claims to versions; before it all ten are refused by name.
    /// Resolved at the BLOCK's DAA.
    pub(super) palw_model_lines: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_model_immutable_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_class_semantics_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_court_commitment_scope_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_staged_public_court_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_staged_tir_route_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_tir_public_conviction_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// RFC-0002 02-A: the TIR container identity's effective fence; `None` on every shipped preset.
    pub(super) palw_tir_container_identity_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The TIR frame's measured cost model (G14 audit §10); `None` on every shipped preset, so the frame route resolves no rules.
    pub(super) palw_tir_frame_cost_v1: Option<kaspa_consensus_core::palw_tir_frame_cost_v1::PalwTirFrameCostV1>,
    pub(super) palw_tir_bounded_door_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_staged_float_constraint_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0095 §4.11 as corrected: the membership's own fence.
    pub(super) palw_model_benefits: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0114: `Params::palw_model_leg_v2_fence` — the five-percent owner leg's height.
    pub(super) palw_model_leg_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0120: `Params::palw_model_seed_v2_fence` — the one-million-MSK floor's height.
    pub(super) palw_model_seed_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0162: `Params::palw_model_virtual_v1_fence` — the height every line's market opens on the
    /// virtual reserve from. `None` on every preset.
    pub(super) palw_model_virtual_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0124 Decisions 1–5: `Params::palw_panel_economy_fence` — the panel economy's height.
    /// Resolved at the BLOCK's DAA for the fold and the receipt door, and at the claim's ANCHOR
    /// for the draw (the panel is a pure function of the claim).
    pub(super) palw_panel_economy: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0124 Decision 6: `Params::palw_work_priced_reward_fence` — the work price's height.
    pub(super) palw_work_priced_reward: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0125: `Params::palw_execution_lane_fence` — the execution lane's activation and shape.
    pub(super) palw_execution_lane: Option<kaspa_consensus_core::config::params::PalwExecutionLaneV1>,
    /// ADR-0126: `Params::palw_overlay_carve_fence` — the height from which the overlay's full split
    /// pays validators a fifth and a claim escrows the tenth they gave up.
    pub(super) palw_native_ruleset_id: kaspa_consensus_core::Hash,
    /// **C4 round 3, F-C4R3-01: the kernel route's ruleset digest is the network's IDENTITY**, never `consensus_params_id`, which moves
    /// with every SCHEDULED fence: a stored route header binding it made the first node to take the next release disqualify every block
    /// with a live claim ("folded under another policy") while un-upgraded nodes accepted it — a split at the binary swap. Two builds
    /// that differ only in future fences share the identity, exactly as the handshake requires.
    pub(super) palw_kernel_route_ruleset_id: kaspa_consensus_core::Hash,
    /// `Params::fence_schedule_v1()` (the fork id's schedule), once: a tag-108 envelope names the fork-id fired digest at a DAA, and
    /// this node recomputes it at the inclusion block without cloning the params (F-C4R3-01(b)).
    pub(super) palw_fence_schedule: Vec<u64>,
    pub(super) palw_dns_retirement: Option<kaspa_consensus_core::palw_native_settlement_v1::PalwDnsRetirementV1>,
    /// RFC-0012: the per-block rows of the native settlement walk. Memory only and rebuildable; see `native_settlement`.
    pub(super) native_rows: parking_lot::Mutex<super::native_settlement::NativeRowCache>,
    /// RFC-0012 D1: the last `safe`-readiness explanation served (memory only, advisory). Reused only when both
    /// sink and its contained pruning point agree, including context changes without a virtual change.
    /// See `native_settlement::native_safe_readiness`.
    pub(super) native_readiness_memo:
        parking_lot::Mutex<Option<(BlockHash, kaspa_consensus_core::palw_native_readiness_v1::NativeSafeReadinessV1)>>,
    /// Serializes session begin and every pruning-import mutation. No network
    /// wait holds this lock; durable epochs reject delayed/reopened callers.
    pub(crate) pruning_import_session_lock: parking_lot::Mutex<()>,
    /// LIVE-R1 N3: the PALW state AT the committed virtual sink, for the `palw_*` readers
    /// ([`Self::palw_v2_reader_state`]). Memory only, keyed by the sink it was derived at.
    pub(super) palw_reader_snapshot:
        parking_lot::Mutex<Option<(BlockHash, Arc<kaspa_consensus_core::palw_native_resource_v1::NativeChainStateV1>)>>,
    /// LIVE-R1 N2: the consensus half of the partition watchdog — the run of resolves that settled after
    /// refusing a heavier, fully validated candidate on the PALW deep-reorg rule
    /// ([`Self::palw_partition_refusal_v1`]). Memory only; node-local; no verdict reads it.
    pub(super) palw_refusal_streak: parking_lot::Mutex<Option<kaspa_consensus_core::api::PalwPartitionRefusalV1>>,
    /// Test-only: the clock the TEMPLATE BUILDER reads in place of the wall clock (milliseconds; 0 = the wall clock). On an EVM-active
    /// network the builder executes the lane against the stamp it chooses, so a harness cannot re-stamp afterwards; with this it can
    /// make the builder choose a simulated stamp, and `build == validate` holds with a simulated clock. Never set outside a test.
    #[cfg(test)]
    pub(super) template_clock: std::sync::atomic::AtomicU64,
    /// Test-only: relabel every `Final` attempt the native-settlement walk reads to this class, so a floor claim (the only claim the
    /// harness can produce) is read as REAL work. The node's own delta, extraction, conversion and certificate run unchanged.
    #[cfg(test)]
    pub(super) native_relabel_class: parking_lot::Mutex<Option<Hash64>>,
    /// Test-only: facts a test places at a chain block, as if its PALW delta had carried that work.
    #[cfg(test)]
    pub(super) native_fact_override: parking_lot::Mutex<
        std::collections::HashMap<BlockHash, Vec<kaspa_consensus_core::palw_native_settlement_v1::MatureUsefulWorkV1>>,
    >,
    pub(super) palw_overlay_carve: Option<kaspa_consensus_core::config::params::PalwOverlayCarveV1>,
    /// ADR-0130: `Params::palw_panel_exposure_floor_fence` — the seat exposure floor's height and
    /// reward multiple. Resolved at the claim's ANCHOR for the draw and at the BLOCK's DAA for the
    /// fold's reservation, which the duty row stores.
    pub(super) palw_panel_exposure_floor: Option<kaspa_consensus_core::config::params::PalwPanelExposureFloorV1>,
    /// ADR-0134: `Params::palw_compute_overlay_retired` — past it the compute overlay's five
    /// subnetworks are refused, in blocks and in the mempool.
    pub(super) palw_compute_overlay_retired: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0135: `Params::palw_model_registry` — past it the fold walks every class's lifecycle,
    /// seats judge by possession proofs, and the class shares come from admission.
    pub(super) palw_model_registry: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0137: `Params::palw_work_target` — dormant everywhere; past it the lottery reads `W₀`.
    pub(super) palw_work_target: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0143: `Params::palw_artifact_root_ownership` — dormant everywhere; past it an artifact
    /// root has one owner and a duplicate is refused where it enters state.
    pub(super) palw_artifact_root_ownership: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The 2026-09-19 audit: `Params::palw_operator_id_unique` — dormant everywhere; past it one
    /// operator identity backs one bond.
    pub(super) palw_operator_id_unique: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0145: `Params::palw_canonical_work_daa()` — the fence's HEIGHT, dormant everywhere.**
    /// Past it a claim's fork-choice weight, its exposure and its work price read the work the
    /// chain DERIVED from its class's graph instead of the step-leaf count the class's registrant
    /// declared. Stored as a height rather than a `ForkActivation` because no site resolves it at
    /// a block: every one compares it against the CLAIM's own `accepted_daa` (see
    /// `PalwTransitionExtrasV1::canonical_work_daa`).
    pub(super) palw_canonical_work_daa: Option<u64>,
    /// The 2026-09-19 audit (F3): `Params::palw_admission_independence` — dormant everywhere; past
    /// it a registered class's panel and its licensing quorum must each name a seat the registrant
    /// does not hold, and a registered class is a `Candidate` until one is ready for it.
    pub(super) palw_admission_independence: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0145 §5/§6: `Params::palw_fp_derived_work` — dormant everywhere; past it a free-prompt
    /// claim's work is derived from the class's graph and a prefix already paid for is not paid
    /// again.
    pub(super) palw_fp_derived_work: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0144 §9: `Params::palw_objective_offence` — dormant everywhere until the live gates
    /// are met; past it a verified `ObjectiveOffence` debits the accused PALW bond.
    pub(super) palw_objective_offence: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0133 §7: `Params::palw_seat_gate_possession` — past it `required_ready_seats` is the
    /// possession floor and a class heavier than its fleet is admitted rarely, not refused.
    pub(super) palw_seat_gate_possession: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// Spend-once execution-round quanta: `Params::palw_execution_quanta`. Past it a Final mints
    /// N unique 1-second permits from CanonicalWork instead of drawing the ADR-0125 lottery.
    /// ADR-0151: the block cadence in milliseconds, for the economic-safety fold. The transition
    /// holds only `PalwStateParamsV2`, which does not carry it, and the residual pricing needs it to
    /// convert a liability horizon in DAA into the one-second rounds a lie could spend inside it.
    pub(super) palw_target_time_per_block_ms: u64,
    /// ADR-0151: `Params::palw_economic_safety`. Past it a quantum matures before it may be spent,
    /// a convicted Final's unused quanta are forfeit, and a Valid seat's lock prices the rights the
    /// lie could still realize.
    pub(super) palw_economic_safety: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_execution_quanta: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0132 S: `Params::palw_single_lottery` — dormant everywhere; past it the lottery reads `max(W₀, W)`.
    pub(super) palw_single_lottery: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0133 Verification V2: `Params::palw_verification_v2` — past it a segment-scoped receipt set licenses by coverage.
    pub(super) palw_verification_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_verification_s3: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_verification_s2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0133 §11.2: `Params::palw_readiness_v2` — past it a possession proof is a whole-artifact
    /// multiproof, the one-leaf object is refused, and only a V2 row counts a seat ready.
    pub(super) palw_readiness_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0132 Upgrade C: `Params::palw_economic_payout` — past it a claim snapshots its economics
    /// at acceptance and is paid at the rate; the registry reads the cap ceiling.
    pub(super) palw_economic_payout: Option<kaspa_consensus_core::config::params::PalwEconomicPayoutV1>,
    /// ADR-0135: the genesis classes' work, derived once from the bundle's registrations that carry
    /// an admission carriage (every node derives the same map from `Params`). A genesis class that
    /// carries none — every shipped one: its profile lives in the catalog, not the bundle — is
    /// described through the canonical class table instead ([`Self::palw_known_model_works_v1`]).
    pub(super) palw_genesis_model_works:
        std::collections::BTreeMap<kaspa_hashes::Hash64, kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1>,
    /// ADR-0135: the work of every class this node can describe, derived once per class and kept —
    /// genesis classes through the canonical class table the binary compiles, registered classes
    /// through the carriage the chain carried (`palw_class_carriage_store`). `None` is a class the
    /// node cannot describe, which the registry leaves as a legacy row.
    pub(super) palw_model_work_cache: std::sync::Mutex<
        std::collections::BTreeMap<kaspa_hashes::Hash64, Option<kaspa_consensus_core::palw_model_registry_v1::PalwModelWorkV1>>,
    >,
    palw_panel_notify_snapshot: std::sync::Mutex<Option<kaspa_consensus_core::palw_panel_view_v1::PalwPanelNotifySnapshotV1>>,
    /// **ADR-0089 Decision 9's fence, `None` on every shipped preset.** Past it the EVM's
    /// window and hand exist and the block's EVM actions reach its transition. Resolved at the
    /// BLOCK's DAA.
    pub(super) palw_model_evm: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0077 Phase B's fence, `None` on every shipped preset.** Past it a class registration
    /// is judged against `palw_class_ladder_rules_*` — the ladder both leaf counts are enumerated
    /// against, the court the close is priced for and Decision 14's canonical floor — instead of
    /// against the genesis-anchored shape. It was carried in `Params` and read nowhere in this
    /// crate (audit D H-3), so the rules the ADR wrote had no door on the only permissionless
    /// registration path.
    pub(super) palw_context_ladder: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0045 Decision 2's boundary sentence, `None` on every shipped preset** (mainnet audit
    /// 2026-09-06, M-2). Past it the block that CROSSES an epoch boundary derives its own epoch's
    /// budget from the parent state — the ADR's own sentence — instead of being refused
    /// `EpochBudgetUnspecified` and orphaned with its mergeset's claims. Resolved in exactly one
    /// place, `palw_v2_check_attempt_admission`, at the BLOCK's own DAA, which is the same score
    /// admission divides by `epoch_length`.
    pub(super) palw_epoch_boundary_budget: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0123's budget release, `None` on every shipped preset.** Resolved in exactly one
    /// place, [`Self::palw_epoch_budget_release_at`], and read by all three sites that judge an
    /// attempt against its budget — admission, the fold (through the transition extras) and the
    /// producer's own readiness — because three readings of one block's DAA is how a block gets
    /// accepted by one and dropped by another.
    pub(super) palw_epoch_budget_release: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0044 Decision 9's advertised free-prompt caps, `None` on every shipped preset**
    /// (mainnet audit 2026-09-06, L-2). Past it the extraction walk gives the lane's validation the
    /// two numbers the ruleset advertises — `max_prompt_tokens` and `max_decode_tokens`, both
    /// already inside `palw_ruleset_id_v2` — and a carrier above either is skipped. Resolved at the
    /// ACCEPTING block's DAA, beside the ladder it rides with.
    pub(super) palw_fp_ruleset_caps: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0069 Decision 7's fence, `None` on every shipped preset. Past it a block whose class
    /// holds no granted share contributes zero pwu to both chain weights — see
    /// [`Self::palw_uncertified_weightless_at`], which is the ONE place this is resolved.
    pub(super) palw_uncertified_weightless: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0062's fence, `None` on every shipped preset. Past it a block may carry a
    /// `DefaultAccused` / `MaterialDisclosed` and a claim may hold `DefaultDisputed`. See
    /// [`Self::palw_da_court_at`], which is the ONE place this is resolved — the acceptance
    /// rehearsal, the fold and the state walk must all get the same answer for the same block, or
    /// a node that admits an object its fold refuses computes a state root nobody shares.
    pub(super) palw_da_court: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_shard_court` (ADR-0099 Decision 5, built by ADR-0100): the one-move court.
    /// Resolved in ONE place, [`Self::palw_shard_court_at`], for the reason the DA court's field
    /// gives — the acceptance rehearsal, the fold (through the extras' ladder) and the state walk
    /// must get the same answer for the same block.
    pub(super) palw_shard_court: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_memory_routing_court_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0191 step B: `palw_tir_designated_answers_v1`'s mirror (None on every shipped network).
    pub(super) palw_tir_designated_answers_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_shard_licensing` (ADR-0100 Decision 4). Resolved in ONE place,
    /// [`Self::palw_shard_licensing_at`]; the panel draw, its validation and the fold's extras all
    /// read it through that, so a node cannot bind what its fold refuses.
    pub(super) palw_shard_licensing: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_token_lift` (ADR-0102): may a registration reach the per-token lift kernel.
    /// Resolved in ONE place, [`Self::palw_token_lift_at`], at the block the registration is
    /// judged in.
    pub(super) palw_token_lift: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_kimi_k3`: may a registration reach the fenced Kimi K3 kernels.
    pub(super) palw_kimi_k3: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_tir_v1` (RFC-0002 Phase F): may a class be a PALW-TIR program on this chain, and
    /// may the IR court arms be filed. Resolved in ONE place, [`Self::palw_tir_at`], at the block.
    pub(super) palw_tir_v1: Option<kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1>,
    /// `Params::palw_probabilistic_constraints_v1` (RFC-0011 §15.7; G14 lane D): may a kernel route object be folded on this chain.
    /// Resolved in ONE place, [`Self::palw_kernel_route_at`]. Never armable by a real network (its validation refuses every
    /// height); a test builds the config without that validation.
    pub(super) palw_probabilistic_constraints_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_funded_execution_v11: Option<kaspa_consensus_core::palw_funded_execution_v11::PalwFundedExecutionFenceV11>,
    pub(super) palw_stage_execution_v14: Option<kaspa_consensus_core::palw_flat_stage_v14::PalwStageExecutionFenceV14>,
    pub(super) palw_native_final_v27: Option<kaspa_consensus_core::palw_native_final_v27::PalwNativeFinalFenceV27>,
    pub(super) palw_stage_execution_v28: Option<kaspa_consensus_core::palw_flat_stage_v28::PalwStageExecutionFenceV28>,
    /// `Params::palw_native_execution_v39` (owning39 source/class/job lanes, stage actions 41–72).
    pub(super) palw_native_execution_v39: Option<kaspa_consensus_core::palw_native_execution_v39::PalwNativeExecutionFenceV39>,
    /// `Params::palw_native_mining_v39` (stage actions 73–79 and the `PNR9` receipt-block carriage). Dormant everywhere.
    pub(super) palw_native_mining_v39: Option<kaspa_consensus_core::palw_native_mining_v39::PalwNativeMiningFenceV39>,
    pub(super) native_carriage_mass: kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22,
    /// Immutable owning ruleset for independent native proof exporters.
    pub(super) native_redemption_params_v33: Arc<Params>,
    /// `Params::palw_fork_choice_commitment_v1` (RFC-0009 L2): past it a header commits its selected parent's post-state as
    /// `BLAKE2b(fork-choice leaf ‖ ADR-0043 root)`. Every site that produces or checks a header root goes through
    /// [`Self::palw_committed_root_v1`]; `None` everywhere, where that is `state.state_root()` byte for byte.
    pub(super) palw_fork_choice_commitment_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_panel_free_v1` (RFC-0015): the OPV fence, the network's admission list and the policy's terms. Resolved in ONE place
    /// ([`Self::palw_kernel_opv_fence`]). Never armable by a real network (its validation refuses every height); a test builds the
    /// config without that validation.
    pub(super) palw_panel_free_v1: Option<kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1>,
    /// `Params::palw_typed_roots_v1` (RFC-0004 Part II): may a typed-root route object (inner kind 19) be carried, and from which height
    /// the route's schedule holds `K2-TR-v1` Active. Resolved in ONE place ([`Self::palw_kernel_typed_roots_at`]). Never armable by a
    /// real network (its validation refuses every height); a test builds the config without that validation.
    pub(super) palw_typed_roots_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_verifier_pay_v1` (G14R round 3): M*-49 verifier pay and the held default share on the kernel route.
    pub(super) palw_verifier_pay_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_model_refutation_seal_v1` (RFC-0002 P0, RFC02-T): candidate refutations only as sealed reveals (inner 36 / 37).
    pub(super) palw_model_refutation_seal_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_window_class_v37` (RFC-0002 §II.14.60): the typed window class, funded action 45.
    pub(super) palw_window_class_v37: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_beacon_source_opt_in_v1: Option<kaspa_consensus_core::palw_beacon_source_opt_in_v1::PalwBeaconSourceFenceV1>,
    pub(super) palw_beacon_source_fee_v1: Option<kaspa_consensus_core::palw_beacon_source_fee_v1::PalwBeaconSourceFeeFenceV1>,
    /// `Params::palw_signed_registration_v1` (RFC-0009 G-EXPIRY / G-RULESET): may a class registration arrive in a signed-expiry envelope
    /// (tag 108). Resolved in ONE place, [`Self::palw_signed_registration_at`]. Never armable by a real network.
    pub(super) palw_signed_registration_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_provider_court_v1` (lane DA16, RFC-0009 §4.2): may the provider court's objects (tags 150–153) be folded. Resolved in
    /// ONE place, [`Self::palw_provider_court_at`]. Never armable by a real network.
    pub(super) palw_provider_court_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_legacy_held_da_v2` (lane LG14-B, RFC-0014 §4–§5 on the legacy V2 route): may the legacy held DA objects (tags
    /// 157–159) be folded. Resolved in ONE place, [`Self::palw_legacy_held_da_v2_at`]. Never armable by a real network.
    pub(super) palw_legacy_held_da_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_bond_budget_v1` / `palw_model_bond_allocation_v1` (lane BUDGET, ADR-0176 / ADR-0177), their activations: may a
    /// capital assignment (tag 140) be folded. Resolved in ONE place, [`Self::palw_model_bond_allocation_at`]. Never armable by a real
    /// network.
    pub(super) palw_bond_budget_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_model_bond_allocation_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_gen_v1` (RFC-0003): may a class be a pipeline of PALW-TIR version-2 programs on
    /// this chain. Resolved in ONE place, [`Self::palw_gen_at`], at the block.
    pub(super) palw_gen_v1: Option<kaspa_consensus_core::palw_gen_v1::PalwGenFenceV1>,
    /// `Params::palw_task_heads_v1` (the `Head` profile, `kaspa_consensus_core::palw_task_heads_v1`): may a generative class be a task
    /// head. Resolved in ONE place, [`Self::palw_task_heads_at`]. `None` on every shipped preset.
    pub(super) palw_task_heads_v1: Option<kaspa_consensus_core::palw_task_heads_v1::PalwTaskHeadsFenceV1>,
    /// `Params::palw_tir_only_v1` (RFC-0002 Phase H): is a new class admitted only as an IR program or pipeline. Resolved in ONE place,
    /// [`Self::palw_tir_only_at`], at the block the registration is judged in. `None` on every shipped preset.
    pub(super) palw_tir_only_v1: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_gdn_key_heads` (P0a): may a registration carry a version-3 profile. Resolved in
    /// ONE place, [`Self::palw_gdn_key_heads_at`], at the block the registration is judged in.
    pub(super) palw_gdn_key_heads: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_fused_dissectable` (ADR-0093 Decision 6): must a fused registration's output
    /// tile be one head's. Resolved in ONE place, [`Self::palw_fused_dissectable_at`].
    pub(super) palw_fused_dissectable: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_attn_anchored_root` (ADR-0093 Decision 8): may a root claim carry its anchor,
    /// and must it. Resolved in ONE place, [`Self::palw_attn_anchored_root_at`]; the acceptance arm
    /// and the fold's extras both read it through that.
    pub(super) palw_attn_anchored_root: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// `Params::palw_held_context` (ADR-0103). Resolved in ONE place, [`Self::palw_held_context_at`];
    /// the acceptance rehearsal, the fold (through the extras' ladder) and the court's shape all
    /// read it through that, so a node cannot admit a move its fold refuses.
    pub(super) palw_held_context: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0082 D10/D11 + RFC-0001 §A.4: `Params::palw_fp_decode_rules_fence()`** — the height
    /// from which a new free-prompt job is V4 only (and below which it is V3 only), read by the
    /// extraction walk at the accepting block's DAA. `None` on every shipped preset.
    pub(super) palw_fp_decode_rules: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0003 §I.4: `Params::palw_fp_job_v5_fence()`** — the height from which a tensor claim (FP job
    /// version 10) may be accepted, over `palw_gen_v1` ([`Self::palw_gen_lane_open_at`]); read by the
    /// extraction walk at the accepting block's DAA. `None` on every shipped preset.
    pub(super) palw_fp_job_v5: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0001 §2.6 stage 2: `Params::palw_fp_prefix_state_fence()`** — the height from which a prefix-state claim (FP job
    /// version 11) may be accepted. `None` on every shipped preset.
    pub(super) palw_fp_prefix_state: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0001 §2.6 stage 2b: `Params::palw_fp_prefix_inherit_fence()`** — the height from which an inherited-prefix claim (FP job
    /// version 12) may be accepted. `None` on every shipped preset.
    pub(super) palw_fp_prefix_inherit: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0096 Decision 8: `Params::palw_fp_decode_constraint_fence()`** — the height from which a constrained claim (FP
    /// job version 6) may be accepted. `None` on every shipped preset.
    pub(super) palw_fp_decode_constraint: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0001 §2.5: `Params::palw_fp_constraint_v2_fence()`** — the height from which a constraint of the second form
    /// is admitted. `None` on every shipped preset.
    pub(super) palw_fp_constraint_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0001 §2.9: `Params::palw_fp_tokenizer_match_fence()`** — the height from which a commitment's tokenizer must be its
    /// class's listed one. `None` on every shipped preset.
    pub(super) palw_fp_tokenizer_match: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The 2026-09-11 audit fence, resolved once in [`Self::palw_audit_2026_09_11_at`]; the
    /// acceptance arm (A-1, AC-SLOT) and the fold's extras both read it there.
    pub(super) palw_audit_2026_09_11: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The 2026-09-11 audit DEEP fence (B-1/C-01/B-4/court cluster), resolved once in
    /// [`Self::palw_audit_2026_09_11_deep_at`]; the fold's extras read it there. A later flag day
    /// than the shallow fence above.
    pub(super) palw_audit_2026_09_11_deep: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The 2026-09-23 economic audit's fence, resolved once in [`Self::palw_audit_2026_09_23_at`];
    /// the fold's extras and the registration gate read it there.
    pub(super) palw_audit_2026_09_23: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0009: `Params::palw_receipt_spend_v4`**, resolved off a `ConsensusV2` ruleset. `None` on every shipped preset — a `PFS4` header is
    /// then refused by name and no coinbase is ever split.
    pub(super) palw_receipt_spend_v4: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **A-2 uniformity: the header carriage forms' owning fences** (`Params::palw_header_form_fences_v1`), for the UTXO validator's
    /// block admission: a form added after the live build is read as that build reads it below its fence.
    pub(super) palw_header_forms: kaspa_consensus_core::pow_layer0::PalwHeaderFormFencesV1,
    /// **A-2 uniformity: the lifecycle kinds' owning fences** (`Params::palw_lifecycle_kind_fences_v1`). A kind whose fence is not in
    /// force at a block is the live build's undecodable payload there: the objects-of-block walk drops it before the acceptance walk,
    /// a chunk group assembling to it is undecodable to the walk, and the UTXO walk burns no rent for it.
    pub(super) palw_lifecycle_kind_fences: kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleKindFencesV1,
    /// ADR-0152 v2 F2: `Params::palw_offence_attribution` (`Some(0)` on testnet-12 alone), resolved
    /// once in [`Self::palw_offence_attribution_at`]; the `ObjectiveOffence` gate and the fold's
    /// extras both read it there, so a node cannot admit a false-Valid offence its fold then routes
    /// the other way.
    pub(super) palw_offence_attribution: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0152-adjacent (Activation Pool, user decision 2026-09-25): `Params::palw_activation_pool`
    /// (`Some` at genesis on testnet-12 alone), resolved once in [`Self::palw_activation_pool_at`];
    /// the fold's R1, R2 and pool rules and the mempool's pool gate read it there.
    pub(super) palw_activation_pool: Option<kaspa_consensus_core::config::params::PalwActivationPoolParamsV1>,
    /// **ADR-0152 R-core+: `Params::palw_rcore_plus`** (`Some(0)` on testnet-12 alone; genesis-only
    /// by `validate_palw_rcore_plus_v1`), resolved once in [`Self::palw_rcore_plus_at`]. M4 reads it
    /// at a claim's ANCHOR for the stake-weighted draw (`palw_panel_draw_policy_at`, SW-1) and at the
    /// BLOCK for the one-state derivation (`palw_v2_derived_panel_bindings`, SW-8); the fold reads the
    /// bundle's mirror (`PalwStateParamsV2::rcore_plus_active_at`), which `validate_palw_rcore_plus_v1`
    /// keeps equal to it.
    pub(super) palw_rcore_plus: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **Lane F1 (post-launch, 2026-09-25): `Params::palw_panel_seed_execution`** — past it (at a
    /// claim's ANCHOR DAA) the panel is drawn from the anchor attempt's execution commitment, not the
    /// anchor block's identity. Dormant on every shipped preset; resolved once in
    /// [`Self::palw_panel_seed_execution_at`].
    pub(super) palw_panel_seed_execution: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **Lane A (post-launch, 2026-09-26): `Params::palw_operator_anchor`, resolved** — the fence and
    /// each operator bond's genesis key. Past it (at a candidate anchor block's own DAA) a chain block
    /// anchors a claim only if it is, or merges, an operator's attempt at or past the claim's slot, and
    /// that attempt seeds the draw ([`Self::palw_chain_block_as_anchor_v1`]). `None` on every shipped
    /// preset.
    pub(super) palw_operator_anchor: Option<kaspa_consensus_core::palw_operator_anchor_v1::PalwOperatorAnchorRuleV1>,
    /// **RFC-0007 §I.6: the vertices this node has seen, to catch a seat that signed a round twice** — fed by the acceptance walk past
    /// `palw_verification_vertex_v1` (after the signature is checked), read by the node's panel, which carries the evidence. Node
    /// policy; nothing here decides a block.
    pub(super) palw_vertex_watch: std::sync::Mutex<kaspa_consensus_core::palw_vertex_v1::PalwVertexWatchV1>,
    /// `Params::palw_settled_anchor_depth` — the second clock's depth, read only past the fence
    /// above through [`Self::palw_settled_anchor_depth_at`].
    pub(super) palw_settled_anchor_depth: Option<u64>,
    /// `Params::palw_admission_audit_period_daa` — how often a `Candidate` meets its ADR-0147 jury.
    pub(super) palw_admission_audit_period_daa: Option<u64>,
    /// `Params::palw_exec_quantum_maturity_v1` — the maturity ADR-0151's fold serves and prices
    /// (testnet-12: 120 DAA, the challenge window it applies), handed to it as
    /// `PalwEconomicSafetyFoldV1::maturity_daa` wherever `palw_economic_safety` is in force.
    pub(super) palw_exec_quantum_maturity_daa: u64,
    /// **Lane bind-deadlock (post-launch): `Params::palw_anchor_at_ceiling`, resolved** — past it (at
    /// a chain block's own DAA) an attempt block refused only by admission item 8 still binds the
    /// claims it anchors ([`Self::palw_v2_anchor_at_ceiling_binder_v1`]). `None` on every shipped
    /// preset.
    pub(super) palw_anchor_at_ceiling: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// Rate limiter for [`Self::palw_warn_if_maturity_outruns_the_registry`] — the DAA score the
    /// shortfall was last reported at, or `PALW_SHORTFALL_NEVER_REPORTED`. **Log state only**:
    /// nothing consensus-visible reads it, so two nodes that report at different moments still
    /// agree about every block. `AtomicU64` because the warning runs behind `&self`.
    pub(super) palw_maturity_warn_last_daa: std::sync::atomic::AtomicU64,
    /// The severity band last reported (0 = margin gone, 1 = healthy claims cannot bind, 2 = no
    /// claim can bind). Log state only, beside `palw_maturity_warn_last_daa`: a band that has
    /// WORSENED is reported at once instead of waiting out the interval, and both reset when the
    /// registry recovers so a later relapse speaks immediately.
    pub(super) palw_maturity_warn_last_band: std::sync::atomic::AtomicU64,
    /// ADR-0065 D2's fence, `None` on every shipped preset. See
    /// [`Self::palw_frontier_provenance_outcome`].
    pub(super) palw_frontier_provenance: Option<kaspa_consensus_core::config::params::ForkActivation>,

    /// lane: rcore/f1-forkchoice-attacks — the deep-reorg strict-economic-win fence, `None` on
    /// every shipped preset. Past it `dns_reorg_outcome` allows a deep reorg only on a strict
    /// economic win, keeping the incumbent on an all-economic tie rather than the candidate hash —
    /// except a SHALLOW tie (lane rcore/f1-strictwin-tie: at most `PALW_REORG_SHALLOW_TIE_DAA_V1` DAA
    /// ticks on the incumbent's side), which GHOSTDAG's own order decides
    /// ([`Self::palw_reorg_shallow_ghostdag_win_v1`]), so honest slot races converge.
    pub(super) palw_reorg_strict_economic_win: Option<kaspa_consensus_core::config::params::ForkActivation>,

    /// **ADR-0178: rule E** (`Params::palw_fork_choice_rule_e_v1`), `None` on every preset. Past it — at the incumbent's DAA — the
    /// deep-reorg gate weighs the two tips' exclusive pasts ([`Self::palw_rule_e_gate_v1`]) and the sink search takes the best of the
    /// candidates it admits rather than the first ([`Self::sink_search_algorithm`]).
    pub(super) palw_fork_choice_rule_e: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// Rule E's search cost, node-local: the most candidates one search's continuation has UTXO-validated past its first, and how
    /// many continuations ran (`palw_rule_e_finish_search_v1`). Read by the cost tests and the log; no rule reads it.
    pub(crate) palw_rule_e_max_extra_validated: std::sync::atomic::AtomicUsize,
    pub(crate) palw_rule_e_searches: std::sync::atomic::AtomicU64,

    /// **ADR-0018 §E's payout bounds** (mainnet audit 2026-09-06 — H-2/H-3/M-1), mode folded in.
    /// `None` on testnet-11, devnet and simnet; `always()` on a card. Resolved at the BLOCK's DAA
    /// on both the coinbase construction and the validation path — they must agree, or every node
    /// builds a different coinbase.
    pub(super) palw_validator_payout_bounds: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **MSK-26A: the slashing-evidence UTXO genuineness fence.** Past it, `apply_slashing_side_effects`
    /// removes an accused bond's stake only for evidence proved genuine (`proved_slash_targets`),
    /// closing the merge-blue forgery that burns an honest bond. `None`/dormant on every shipped
    /// preset — the side-effect is byte-identical to before the field existed.
    pub(super) palw_slashing_evidence_utxo_genuine: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **Lane accept-order** (`Params::palw_lane_accept_parents_first_fence`): past it a merging block
    /// applies its mergeset parents-first ([`Self::acceptance_ordered_mergeset_without_selected_parent`]).
    /// `None` on every shipped preset — every order below is the consensus order, byte for byte.
    pub(super) palw_lane_accept_parents_first: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **RFC-0008 v2**: `Params::palw_exec_payload_v2_fence`. Past it the EXEC blocks a chain block accepts are the ones its coinbase
    /// anchor covers, not its mergeset's reds. `None` on every shipped preset.
    pub(super) palw_exec_v2: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// RFC-0008 v2: the anchor the virtual state was resolved with, keyed by the virtual's selected parent. The template that virtual
    /// becomes must carry exactly this anchor in its coinbase, or its UTXO commitment (which includes the covered blocks' effects)
    /// would not be the one its own validation recomputes.
    pub(super) palw_exec_v2_virtual_anchor:
        parking_lot::Mutex<Option<(BlockHash, kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorV1)>>,
    /// ADR-0066: the heartbeat lane's fence, mode folded in.
    pub(super) palw_heartbeat_lane: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0138: `Params::palw_anchor_clock` and ADR-0083's receipt fence — the heartbeat miner's
    /// hint reads them for the same reason the slot rule does (`heartbeat_yield_hint_v2`).
    pub(super) palw_anchor_clock: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// ADR-0142: `Params::palw_clock_cursor`. Past it the heartbeat lane's admissibility is the
    /// rooted cursor, and only a heartbeat writes it.
    pub(super) palw_clock_cursor: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The 2026-09-24 heartbeat audit's H3/H5 (`Params::palw_clock_floor`): templates keep a
    /// heartbeat chain paced and stamp a step at or past its slot, as the header stage demands.
    pub(super) palw_clock_floor: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// The 2026-09-25 mainnet-values review's HIGH (`Params::palw_clock_lead_cap`): a template that
    /// would step the clock more than 132 s past this node's clock is refused (the builder waits),
    /// because this node's own header stage — and every peer's — would refuse the block.
    pub(super) palw_clock_lead_cap: Option<kaspa_consensus_core::config::params::ForkActivation>,
    pub(super) palw_receipt_rows_unpriced: kaspa_consensus_core::config::params::ForkActivation,
    /// ADR-0072 SA-3/SA-4: the attempt lane's activation fence. `None` on every shipped preset, so
    /// the lane resolves to `Unfenced` and the template keeps declaring algo-6.
    pub(super) palw_attempt_activation: Option<kaspa_consensus_core::config::params::ForkActivation>,
    /// **ADR-0073 SA-1: the free-prompt beacon's fold width and its fence.** `None` on every
    /// shipped preset, which is the pre-SA-1 single-block beacon. Resolved in exactly ONE place,
    /// [`Self::palw_beacon_fold_k_at`], so the producer's walk and the validator's cannot pick
    /// different widths for one draw.
    pub(super) palw_beacon_fold: Option<kaspa_consensus_core::config::params::PalwBeaconFoldV1>,
    /// ADR-0075 D14's fence, `None` on every shipped preset: what a block's certification slot
    /// may be spent on, and what an ungraded object costs. See
    /// [`Self::palw_chunk_cap_charge_at`], which is the ONE place this is resolved.
    pub(super) palw_chunk_cap_charge: Option<kaspa_consensus_core::config::params::ForkActivation>,

    // kaspa-pq Selected-Parent EVM Lane (ADR-0020, design v0.4). The lazy
    // chain-context EVM step + canonical head pointers. Inert until
    // `evm_activation_daa_score` is finite (`u64::MAX` on every current net).
    pub(super) evm_header_store: Arc<DbEvmHeaderStore>,
    pub(super) evm_state_store: Arc<DbEvmStateStore>,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))] // read by the cfg(evm) chain-context step only
    pub(super) evm_payload_store: Arc<DbEvmPayloadStore>,
    pub(super) evm_heads_store: Arc<RwLock<DbEvmCanonicalHeadsStore>>,
    /// ADR-0109 Decision 1: the deposit-lock index — every lock the virtual set holds, claimed by
    /// every template unasked. Staged in `commit_virtual_state` from the same diff as the set.
    pub(super) evm_deposit_lock_store: Arc<RwLock<DbEvmDepositLockStore>>,
    /// ADR-0109 Decision 3: the PALW bond gate the mempool asks — the locked set it refuses spends
    /// of and the burn obligations a released bond's spend must pay — memoised per (registry tip,
    /// DAA) so a burst of admissions does not rebuild it per transaction.
    pub(super) palw_mempool_locked_cache: parking_lot::Mutex<Option<(BlockHash, u64, Arc<PalwMempoolBondGate>)>>,
    pub(super) evm_receipts_store: Arc<crate::model::stores::evm::DbEvmReceiptsStore>,
    pub(super) evm_tx_index_store: Arc<crate::model::stores::evm::DbEvmTxIndexStore>,
    pub(super) evm_block_hash_map_store: Arc<crate::model::stores::evm::DbEvmBlockHashMapStore>,
    pub(super) evm_number_store: Arc<crate::model::stores::evm::DbEvmNumberStore>,
    pub(super) evm_log_index_store: Arc<crate::model::stores::evm::DbEvmLogIndexStore>,
    pub(super) evm_trace_store: Arc<crate::model::stores::evm::DbEvmTraceReplayStore>,
    // §12 archive: forward state diff (220) / full checkpoint (221) / content-addressed
    // code (222) — written alongside the per-block result so an archive/recent node can
    // reconstruct any canonical block's state. RPC/archive data only, never committed.
    pub(super) evm_state_diff_store: Arc<crate::model::stores::evm::DbEvmStateDiffStore>,
    pub(super) evm_state_checkpoint_store: Arc<crate::model::stores::evm::DbEvmStateCheckpointStore>,
    pub(super) evm_code_store: Arc<crate::model::stores::evm::DbEvmCodeStore>,
    // C-01 state-backend (design v0.1, Stage 1, slice S4): the flat latest-canonical
    // state (234) + block→root index (232) + canonical pointer (231). Written ONLY
    // by the shadow dual-write below, gated on `evm_shadow_state_backend` (off by
    // default). Inert otherwise. The pointer is RwLock-wrapped (its `set_batch` is
    // `&mut self`); the lock is taken only while shadow is on.
    pub(super) evm_flat_account_store: Arc<crate::model::stores::evm::DbEvmFlatAccountStore>,
    pub(super) evm_block_state_root_store: Arc<crate::model::stores::evm::DbEvmBlockStateRootStore>,
    pub(super) evm_latest_state_ptr_store: Arc<RwLock<crate::model::stores::evm::DbEvmLatestStatePtrStore>>,
    // C-01 slice S4: node-local shadow dual-write of the flat state backend +
    // per-block live differential vs the committed snapshot. `false` on every
    // current network and by default — purely a pre-cutover validation aid.
    pub(super) evm_shadow_state_backend: bool,
    // C-01 slice S9: when set (together with `evm_shadow_state_backend`), the EVM executor seeds
    // the parent state from the validated flat/reconstruct source instead of the 206 snapshot. The
    // seed is asserted byte-identical to 206 BEFORE use (HALT on divergence), and 206 is still
    // written — consensus-neutral + reversible. `false` on every current network and by default.
    // Only read by the `#[cfg(feature = "evm")]` chain-context path; without that feature the
    // pre-existing dead-code lint fires (allowed here to unblock the clippy gate).
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_flat_authoritative: bool,
    // C-01 slice S9b: when set (together with `evm_flat_authoritative`), STOP persisting the per-block
    // 206 snapshot. The flat backend — already checked == the executor's in-memory post-state every
    // block by the S4 write-side differential — is the sole persisted post-state; the O12 pipeline is
    // disabled (its gap items 206-seed) and reads fall back to flat-materialize / §12-reconstruct.
    // Node-local, consensus-neutral. `false` on every current network and by default.
    pub(super) evm_retire_206: bool,
    /// ADR-0109 Decision 2 — `Config::evm_bridge_finality_effective()`: under `Label` the template
    /// carries the EVM payload whatever the DNS anchor's distance; under `Pause` a stale anchor
    /// empties it (the behaviour before ADR-0109).
    pub(super) evm_bridge_finality: kaspa_consensus_core::evm::EvmBridgeFinalityPolicy,
    // §12: this node's EVM state-history retention mode (`--evm-history-mode`). In
    // `head` mode the per-block archive diff/checkpoint (220/221) are not written at
    // all; `recent`/`archive` write them (the pruning processor decides how long
    // they survive). Node-local — never affects block validity or any commitment.
    pub(super) evm_history_mode: kaspa_consensus_core::evm::EvmHistoryMode,
    pub(super) evm_activation_daa_score: u64,
    // These activation-score fields are only read by the `#[cfg(feature = "evm")]` chain-context
    // path; without that feature the pre-existing dead-code lint fires (allowed to unblock the gate).
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_gas_pool_v2_activation_daa_score: u64,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_f002_withdraw_cap_activation_daa_score: u64,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_bridge_ledger_activation_daa_score: u64,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_f003_mldsa_verify_activation_daa_score: u64,
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) evm_typed_receipt_root_activation_daa_score: u64,
    // O9 (optimization design v0.1): node-local EVM-lane KPIs — chain-block
    // count / mergeset-size sum / accepted-gas sum. The gas supply is
    // 30M × chain-block rate (NOT DAG width), and the adversarial degradation
    // mode is a widening mergeset (design §2/B7) — these counters make that
    // observable. Logged every 256 chain blocks; never consensus-relevant.
    #[cfg_attr(not(feature = "evm"), allow(dead_code))] // recorded only on the cfg(evm) chain-context step
    pub(super) evm_lane_kpi: EvmLaneKpi,

    // Utxo-related stores
    pub(super) utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    // kaspa-pq DNS overlay (ADR-0009 Addendum B §B.3(c)): per-block rewarded
    // `(bond, epoch)` keys for cross-block reward uniqueness.
    pub(super) rewarded_epochs_store: Arc<DbRewardedEpochsStore>,
    // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): the per-epoch accumulator and
    // its per-block validator quality sub-pool input. Inert until
    // `pos_v2_activation_daa_score` (`u64::MAX` today).
    pub(super) epoch_accumulator_store: Arc<DbEpochAccumulatorStore>,
    pub(super) block_quality_pool_store: Arc<DbBlockQualityPoolStore>,
    pub(super) reserve_balance_store: Arc<DbReserveBalanceStore>,
    pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub(super) virtual_stores: Arc<RwLock<VirtualStores>>,
    pub(super) pruning_meta_stores: Arc<RwLock<PruningMetaStores>>,

    /// The "last known good" virtual state. To be used by any logic which does not want to wait
    /// for a possible virtual state write to complete but can rather settle with the last known state
    pub lkg_virtual_state: LkgVirtualState,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DbDagTraversalManager,
    pub(super) window_manager: DbWindowManager,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) pruning_point_manager: DbPruningPointManager,
    pub(super) parents_manager: DbParentsManager,
    pub(super) depth_manager: DbBlockDepthManager,

    // block window caches
    pub(super) block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub(super) block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,

    // Pruning lock
    pub(super) pruning_lock: SessionLock,

    // Notifier
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Mining Rule
    _mining_rules: Arc<MiningRules>,
}

/// **A fee no rent rule may refuse** (ADR-0075 SA-1/SA-2).
///
/// Stands for "this object was not priced", which is two different facts with one correct
/// treatment: the rent fence is unarmed — every shipped preset — or the object was DERIVED by the
/// chain rather than carried by anyone (a panel binding). In both cases no transaction owes rent,
/// so the sentinel is the maximum rather than zero: a rule written as `paid < owed` must read as
/// absent, never as "the carrier underpaid".
pub(super) const PALW_RENT_UNPRICED: u64 = u64::MAX;

/// One accepted object with its carrier fee available for rent after mandatory bond burn.
///
/// The pairing exists because `AcceptanceData` cannot express it: it records a transaction id and
/// an index and nothing about value, so before this the object walk had no way to read the fee
/// that Decision 1 calls "the rent". The number is resolved in `palw_v2_objects_of_block` from
/// the map `calculate_utxo_state` filled, and spent in `palw_v2_accepted_objects`.
pub(super) struct PalwCarriedObjectV1 {
    pub(super) object: kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    /// Internal only: fee available for rent after mandatory bond burn, or the unpriced sentinel.
    pub(super) carrier_fee: u64,
    /// Original transaction-local companion, never borrowed from another carrier.
    pub(super) history_pledge: Option<kaspa_consensus_core::palw_history_custody_v1::PalwHistoryCustodyPledgeV1>,
    pub(super) tir_public_original: Option<kaspa_consensus_core::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1>,
    /// **G14 item 1 (RFC-0035 A): the same carrier's funded TFO1 pairing** (its own actual transaction). `Some` only for a TFO1
    /// carrier past `palw_tir_public_conviction_v1`; its object is admitted only through the funded-original coordinator.
    pub(super) funded_original: Option<kaspa_consensus_core::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1>,
    pub(super) original_binding: Option<kaspa_consensus_core::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1>,
    /// The same carrier's optional producer custody delegation (HFP1 version 2).
    pub(super) custody_delegation: Option<kaspa_consensus_core::palw_history_custody_v1::PalwProducerCustodyDelegationV1>,
    /// **The 2026-09-23 Position route matrix, P-B1: what this object's carrier is owed if acceptance
    /// refuses it.** `Some` only for a carrier-borne `ModelBuy`/`ModelSeed` past
    /// `palw_audit_2026_09_23`, resolved in `palw_v2_objects_of_block` where the carrier is in hand;
    /// `None` for everything else and everywhere below the fence, where a refused move's payment
    /// stays in its sink exactly as before.
    pub(super) refund: Option<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
    /// P-B1: a carrier-borne `ModelBuy`/`ModelSeed` past the fence whose carrier pays no
    /// P2PKH-ML-DSA-87 output — nobody the chain could refund. `Some(carrier)` so a refusal of it is
    /// LOGGED as the burn it still is (the review: "nothing is logged"); `None` for everything else.
    pub(super) unrefundable: Option<TransactionId>,
}

/// P-B1: the object the acceptance filter is judging, as far as a refund is concerned — how many
/// objects were accepted before it (it is refused iff that count has not grown when the next is
/// judged), and what its carrier is owed or why nobody can be paid.
pub(super) struct PalwPendingCarrierRefundV1 {
    accepted_before: usize,
    refund: Option<kaspa_consensus_core::palw_state_v2::PalwCarrierRefundV1>,
    unrefundable: Option<TransactionId>,
}

/// ADR-0058: one mergeset blue's admitted PALW work, held by value between assembly (against
/// the walk state) and the transition call (which borrows it). Two arms because a header
/// declares exactly one lane by its algorithm id.
pub(super) enum PalwMergedOwnedWorkV1 {
    /// `(carrying block, attempt, the block's own subsidy, the carve resolved at the block's own DAA)`.
    /// The subsidy is `calc_block_subsidy(the block's DAA)` — for an attempt block it equals the
    /// coinbase-declared `mergeset_rewards` subsidy (body validation pins it, and an attempt block is
    /// never a heartbeat) — and is the pool B-1's escrow is carved from past the deep fence. The carve
    /// is ADR-0126's where it is active at the block's DAA and `None` (the bundle's) below. Both are
    /// one value the fold escrows and the coinbase withholds, computed once here from the header this
    /// function already read.
    Attempt(
        BlockHash,
        kaspa_consensus_core::palw_attempt_v2::PalwAttemptEnvelopeV2,
        u64,
        Option<kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2>,
        /// ADR-0132 Upgrade C: the block's own compact `bits`, for its claim's snapshot.
        u32,
        /// The exact header admitted above, retained for the native keys and original EXEC clock.
        Arc<Header>,
    ),
    Spend(BlockHash, kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3),
}

mod dns;
mod evm;
mod palw;
mod palw_accept;
mod palw_validate;
mod template;

impl VirtualStateProcessor {
    #[cfg(test)]
    pub(crate) fn set_template_time_for_test(&self, now: u64) {
        self.template_clock.fetch_max(now, std::sync::atomic::Ordering::Relaxed);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<VirtualStateProcessingMessage>,
        pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        pruning_receiver: CrossbeamReceiver<PruningProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        storage: &Arc<ConsensusStorage>,
        services: &Arc<ConsensusServices>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        mining_rules: Arc<MiningRules>,
        evm_history_mode: kaspa_consensus_core::evm::EvmHistoryMode,
        evm_shadow_state_backend: bool,
        evm_flat_authoritative: bool,
        evm_retire_206: bool,
        evm_bridge_finality: kaspa_consensus_core::evm::EvmBridgeFinalityPolicy,
    ) -> Self {
        // C-01 S9: flat-authoritative seeding needs the shadow backend (which maintains + validates
        // the flat store); without it the flag is a silent no-op (the executor keeps seeding from
        // 206). Warn so the prerequisite isn't missed during a cutover rollout. Fail-safe either way.
        if evm_flat_authoritative && !evm_shadow_state_backend {
            warn!(
                "[C-01] --evm-flat-authoritative is set WITHOUT --evm-shadow-state-backend; it is a no-op (the EVM executor keeps seeding from the 206 snapshot). Enable --evm-shadow-state-backend to use the flat-authoritative seed."
            );
        }
        // C-01 S9b: retiring the 206 persist requires the flat-authoritative seed (so the executor no
        // longer reads 206). Without it, dropping 206 would leave the executor's selected-parent read
        // (and the O12 pipeline) with no seed → a stall. Demote to a no-op + warn rather than enable a
        // half-configured retirement: keep writing 206 so the node stays correct.
        let evm_retire_206 = if evm_retire_206 && !(evm_flat_authoritative && evm_shadow_state_backend) {
            warn!(
                "[C-01] --evm-retire-206 is set WITHOUT --evm-flat-authoritative (+ --evm-shadow-state-backend); it is a no-op (the per-block 206 snapshot keeps being written). Enable the flat-authoritative seed first."
            );
            false
        } else {
            evm_retire_206
        };
        // C-01 S9b: `head` history keeps no §12 diff/checkpoint, so a retired-206 node cannot serve the
        // IBD pruning-point snapshot to peers nor answer historical state RPC (both fall back to
        // §12-reconstruct). Block validation is unaffected (it seeds from the flat HEAD), so this is a
        // loud warning, not a demotion — an operator may knowingly run a non-serving retired node.
        if evm_retire_206 && !evm_history_mode.writes_state_history() {
            warn!(
                "[C-01] --evm-retire-206 with --evm-history-mode=head: the IBD pruning-point export and historical state RPC will be UNAVAILABLE on this node (no §12 history to reconstruct 206 from). Use recent/archive history if this node serves IBD or state queries."
            );
        }
        // The same serving hole exists on `recent` once pruning has deleted the
        // sub-pruning-point rows: with 206 retired, the pruning-point export then
        // depends entirely on an anchor AT the pruning point (a materialized
        // checkpoint/snapshot — see the pruning processor's pp-anchor step and
        // --evm-materialize-pp-anchor for a datadir where the anchor is already
        // missing). testnet-10 ran retire-206+recent with no anchor and silently
        // could not serve pruned IBD to any peer — warn instead of staying quiet.
        if evm_retire_206 && evm_history_mode.writes_state_history() && !evm_history_mode.retains_state_history_past_pruning() {
            warn!(
                "[C-01] --evm-retire-206 with --evm-history-mode=recent: serving the IBD pruning-point export relies on a state anchor AT the pruning point (kept checkpoint/snapshot). If this node's anchor is missing (e.g. the datadir predates the pp-anchor step), run --evm-materialize-pp-anchor once; otherwise peers cannot pruned-IBD from this node."
            );
        }
        Self {
            receiver,
            pruning_sender,
            pruning_receiver,
            thread_pool,

            genesis: params.genesis.clone(),
            pow_blake2b_sha3_activation: params.pow_blake2b_sha3_activation,
            pow_palw_activation: params.pow_palw_activation,
            pow_palw_ollama_activation: params.pow_palw_ollama_activation,
            palw_required_algo_id: params.palw_consensus_mode.required_algo_id(),
            palw_heartbeat_lane: params.palw_heartbeat_lane_fence(),
            palw_anchor_clock: params.palw_anchor_clock,
            palw_clock_cursor: params.palw_clock_cursor,
            palw_clock_floor: params.palw_clock_floor,
            palw_clock_lead_cap: params.palw_clock_lead_cap,
            palw_receipt_rows_unpriced: params
                .palw_receipt_rows_unpriced
                .unwrap_or_else(kaspa_consensus_core::config::params::ForkActivation::never),
            palw_attempt_activation: params.palw_attempt_activation,
            palw_maturity_warn_last_daa: std::sync::atomic::AtomicU64::new(
                kaspa_consensus_core::palw_panel_v2::PALW_SHORTFALL_NEVER_REPORTED,
            ),
            palw_maturity_warn_last_band: std::sync::atomic::AtomicU64::new(0),
            palw_beacon_fold: params.palw_beacon_fold,
            palw_chunk_cap_charge: params.palw_chunk_cap_charge,
            max_block_parents: params.max_block_parents_history(),
            mergeset_size_limit: params.mergeset_size_limit_history(),
            palw_heartbeat_width_fence: params.palw_heartbeat_width_fence(),

            db,
            statuses_store: storage.statuses_store.clone(),
            headers_store: storage.headers_store.clone(),
            ghostdag_store: storage.ghostdag_store.clone(),
            daa_excluded_store: storage.daa_excluded_store.clone(),
            block_transactions_store: storage.block_transactions_store.clone(),
            pruning_point_store: storage.pruning_point_store.clone(),
            past_pruning_points_store: storage.past_pruning_points_store.clone(),
            body_tips_store: storage.body_tips_store.clone(),
            depth_store: storage.depth_store.clone(),
            selected_chain_store: storage.selected_chain_store.clone(),
            pruning_samples_store: storage.pruning_samples_store.clone(),
            headers_selected_tip_store: storage.headers_selected_tip_store.clone(),
            stake_bonds_store: storage.stake_bonds_store.clone(),
            palw_class_carriage_store: storage.palw_class_carriage_store.clone(),
            palw_state_v2_store: storage.palw_state_v2_store.clone(),
            palw_state_params_v2: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.state.clone()),
                _ => None,
            },
            #[cfg(test)]
            panel_v3_test_beacon: parking_lot::Mutex::new(None),
            palw_freeprompt_params_v3: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.freeprompt.clone()),
                _ => None,
            },
            palw_admission_params_v2: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.admission.clone()),
                _ => None,
            },
            palw_bond_params_v2: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.bond),
                _ => None,
            },
            palw_v2_bundle: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.clone()),
                _ => None,
            },
            palw_panel_params_v2: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.panel),
                _ => None,
            },
            palw_court_params_v2: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.court),
                _ => None,
            },
            palw_genesis_objects_v2: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => bundle.genesis_objects.clone(),
                _ => Vec::new(),
            },
            network_id_bytes: params.net.to_string().into_bytes(),
            dns_state_store: storage.dns_state_store.clone(),
            pruning_overlay_snapshot_store: storage.pruning_overlay_snapshot_store.clone(),
            evm_header_store: storage.evm_header_store.clone(),
            evm_state_store: storage.evm_state_store.clone(),
            evm_payload_store: storage.evm_payload_store.clone(),
            evm_heads_store: storage.evm_heads_store.clone(),
            evm_deposit_lock_store: storage.evm_deposit_lock_store.clone(),
            palw_mempool_locked_cache: Default::default(),
            evm_receipts_store: storage.evm_receipts_store.clone(),
            evm_tx_index_store: storage.evm_tx_index_store.clone(),
            evm_block_hash_map_store: storage.evm_block_hash_map_store.clone(),
            evm_number_store: storage.evm_number_store.clone(),
            evm_log_index_store: storage.evm_log_index_store.clone(),
            evm_trace_store: storage.evm_trace_store.clone(),
            evm_state_diff_store: storage.evm_state_diff_store.clone(),
            evm_state_checkpoint_store: storage.evm_state_checkpoint_store.clone(),
            evm_code_store: storage.evm_code_store.clone(),
            evm_flat_account_store: storage.evm_flat_account_store.clone(),
            evm_block_state_root_store: storage.evm_block_state_root_store.clone(),
            evm_latest_state_ptr_store: storage.evm_latest_state_ptr_store.clone(),
            evm_shadow_state_backend,
            evm_flat_authoritative,
            evm_retire_206,
            evm_bridge_finality,
            evm_history_mode,
            evm_activation_daa_score: params.evm_activation_daa_score,
            evm_gas_pool_v2_activation_daa_score: params.evm_gas_pool_v2_activation_daa_score,
            evm_f002_withdraw_cap_activation_daa_score: params.evm_f002_withdraw_cap_activation_daa_score,
            evm_bridge_ledger_activation_daa_score: params.evm_bridge_ledger_activation_daa_score,
            evm_f003_mldsa_verify_activation_daa_score: params.evm_f003_mldsa_verify_activation_daa_score,
            evm_typed_receipt_root_activation_daa_score: params.evm_typed_receipt_root_activation_daa_score,
            evm_lane_kpi: EvmLaneKpi::default(),
            dns_params: params.dns_params.clone(),
            dns_bft_gate: params.dns_bft_gate_fence(),
            dns_bft_runtime: Default::default(),
            palw_block_commitment: params.palw_block_commitment,
            palw_bootstrap_activation: params.palw_bootstrap_activation,
            palw_unavailable_abstains: params.palw_unavailable_abstains,
            palw_bond_maturity: params.palw_bond_maturity,
            palw_bond_maturity_early: params.palw_bond_maturity_early_fence(),
            // The MODE-folded fence: capability declarations, the claim lane and the panel draw
            // exist only under ConsensusV2, so the mode condition is folded once in `Params`
            // rather than remembered at each of the three sites below.
            palw_capability_bound: params.palw_capability_bound_fence(),
            palw_certification_rent: params.palw_certification_rent,
            palw_kary_court: params.palw_kary_court_fence(),
            palw_model_court_window: params.palw_model_court_window,
            palw_prompt_ids_merkle: params.palw_prompt_ids_merkle_fence(),
            palw_panel_da: params.palw_panel_da_fence(),
            palw_court_ladder: params.palw_court_ladder_fence(),
            palw_court_responder_coverage: params.palw_court_responder_coverage_fence(),
            palw_fp_da_pins: params.palw_fp_da_pins_fence(),
            palw_share_growth_final: params.palw_share_growth_final_fence(),
            palw_model_market: params.palw_model_market_fence(),
            palw_model_lines: params.palw_model_lines_fence(),
            palw_model_immutable_v1: params.palw_model_immutable_v1_fence(),
            palw_class_semantics_v1: params.palw_class_semantics_v1_fence(),
            palw_court_commitment_scope_v1: params.palw_court_commitment_scope_v1_fence(),
            palw_staged_public_court_v1: params.palw_staged_public_court_v1_fence(),
            palw_staged_tir_route_v1: params.palw_staged_tir_route_v1_fence(),
            palw_tir_public_conviction_v1: params.palw_tir_public_conviction_v1_fence(),
            palw_tir_container_identity_v1: params.palw_tir_container_identity_v1_fence(),
            palw_tir_frame_cost_v1: params.palw_tir_frame_cost_v1,
            palw_tir_bounded_door_v1: params.palw_tir_bounded_door_v1_fence(),
            palw_staged_float_constraint_v1: params.palw_staged_float_constraint_v1_fence(),
            palw_model_benefits: params.palw_model_benefits_fence(),
            palw_model_leg_v2: params.palw_model_leg_v2_fence(),
            palw_model_seed_v2: params.palw_model_seed_v2_fence(),
            palw_model_virtual_v1: params.palw_model_virtual_v1_fence(),
            palw_panel_economy: params.palw_panel_economy_fence(),
            palw_work_priced_reward: params.palw_work_priced_reward_fence(),
            palw_execution_lane: params.palw_execution_lane_fence(),
            palw_overlay_carve: params.palw_overlay_carve_fence(),
            palw_dns_retirement: params.palw_dns_retirement,
            native_rows: Default::default(),
            native_readiness_memo: Default::default(),
            pruning_import_session_lock: Default::default(),
            palw_reader_snapshot: Default::default(),
            palw_refusal_streak: Default::default(),
            #[cfg(test)]
            template_clock: Default::default(),
            #[cfg(test)]
            native_relabel_class: Default::default(),
            #[cfg(test)]
            native_fact_override: Default::default(),
            palw_native_ruleset_id: params.consensus_params_id(),
            palw_kernel_route_ruleset_id: params.consensus_identity_id(),
            palw_fence_schedule: params.fence_schedule_v1(),
            palw_panel_exposure_floor: params.palw_panel_exposure_floor_fence(),
            palw_compute_overlay_retired: params.palw_compute_overlay_retired,
            palw_model_registry: params.palw_model_registry,
            palw_work_target: params.palw_work_target,
            palw_artifact_root_ownership: params.palw_artifact_root_ownership,
            palw_operator_id_unique: params.palw_operator_id_unique,
            palw_admission_independence: params.palw_admission_independence,
            palw_fp_derived_work: params.palw_fp_derived_work,
            palw_objective_offence: params.palw_objective_offence,
            palw_seat_gate_possession: params.palw_seat_gate_possession,
            palw_target_time_per_block_ms: params.target_time_per_block(),
            palw_economic_safety: params.palw_economic_safety,
            palw_execution_quanta: params.palw_execution_quanta,
            palw_single_lottery: params.palw_single_lottery,
            palw_verification_v2: params.palw_verification_v2,
            palw_verification_s3: params.palw_verification_s3,
            palw_verification_s2: params.palw_verification_s2,
            palw_readiness_v2: params.palw_readiness_v2,
            palw_economic_payout: params.palw_economic_payout_fence(),
            palw_genesis_model_works: match &params.palw_consensus_mode {
                kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                    kaspa_consensus_core::palw_model_registry_v1::palw_genesis_model_works_v1(&bundle.genesis_objects)
                }
                _ => Default::default(),
            },
            palw_model_work_cache: std::sync::Mutex::new(std::collections::BTreeMap::new()),
            palw_panel_notify_snapshot: std::sync::Mutex::new(None),
            palw_model_evm: params.palw_model_evm_fence(),
            palw_context_ladder: params.palw_context_ladder,
            palw_epoch_boundary_budget: params.palw_epoch_boundary_budget,
            palw_epoch_budget_release: params.palw_epoch_budget_release,
            palw_fp_ruleset_caps: params.palw_fp_ruleset_caps,
            palw_uncertified_weightless: params.palw_uncertified_weightless,
            palw_canonical_work_daa: params.palw_canonical_work_daa(),
            palw_da_court: params.palw_da_court,
            palw_shard_court: params.palw_shard_court_fence(),
            palw_memory_routing_court_v2: params.palw_memory_routing_court_v2_fence(),
            palw_tir_designated_answers_v1: params.palw_tir_designated_answers_v1_fence(),
            palw_shard_licensing: params.palw_shard_licensing_fence(),
            palw_token_lift: params.palw_token_lift_fence(),
            palw_kimi_k3: params.palw_kimi_k3_fence(),
            palw_tir_v1: params.palw_tir_v1_fence(),
            palw_probabilistic_constraints_v1: params.palw_probabilistic_constraints_v1,
            palw_funded_execution_v11: params.palw_funded_execution_v11.clone(),
            palw_stage_execution_v14: params.palw_stage_execution_v14.clone(),
            palw_native_final_v27: params.palw_native_final_v27.clone(),
            palw_stage_execution_v28: params.palw_stage_execution_v28.clone(),
            palw_native_execution_v39: params.palw_native_execution_v39.clone(),
            palw_native_mining_v39: params.palw_native_mining_v39.clone(),
            native_redemption_params_v33: Arc::new(params.clone()),
            native_carriage_mass: kaspa_consensus_core::palw_native_carriage_v22::NativeCarriageMassParamsV22::from_consensus_params(
                params,
            ),
            palw_fork_choice_commitment_v1: params.palw_fork_choice_commitment_v1,
            palw_panel_free_v1: params.palw_panel_free_v1.clone(),
            palw_typed_roots_v1: params.palw_typed_roots_v1,
            palw_verifier_pay_v1: params.palw_verifier_pay_v1,
            palw_model_refutation_seal_v1: params.palw_model_refutation_seal_v1,
            palw_window_class_v37: params.palw_window_class_v37,
            palw_beacon_source_opt_in_v1: params.palw_beacon_source_opt_in_v1,
            palw_beacon_source_fee_v1: params.palw_beacon_source_fee_v1,
            palw_signed_registration_v1: params.palw_signed_registration_v1,
            palw_provider_court_v1: params.palw_provider_court_v1,
            palw_legacy_held_da_v2: params.palw_legacy_held_da_v2,
            palw_bond_budget_v1: params.palw_bond_budget_v1.as_ref().map(|fence| fence.activation),
            palw_model_bond_allocation_v1: params.palw_model_bond_allocation_v1.as_ref().map(|fence| fence.activation),
            palw_gen_v1: params.palw_gen_v1_fence(),
            palw_task_heads_v1: params.palw_task_heads_v1_fence(),
            palw_tir_only_v1: params.palw_tir_only_fence(),
            palw_gdn_key_heads: params.palw_gdn_key_heads_fence(),
            palw_fused_dissectable: params.palw_fused_dissectable_fence(),
            palw_attn_anchored_root: params.palw_attn_anchored_root_fence(),
            palw_held_context: params.palw_held_context_fence(),
            palw_fp_decode_rules: params.palw_fp_decode_rules_fence(),
            palw_fp_job_v5: params.palw_fp_job_v5_fence(),
            palw_fp_prefix_state: params.palw_fp_prefix_state_fence(),
            palw_fp_prefix_inherit: params.palw_fp_prefix_inherit_fence(),
            palw_fp_decode_constraint: params.palw_fp_decode_constraint_fence(),
            palw_fp_constraint_v2: params.palw_fp_constraint_v2_fence(),
            palw_fp_tokenizer_match: params.palw_fp_tokenizer_match_fence(),
            palw_audit_2026_09_11: params.palw_audit_2026_09_11_fence(),
            palw_audit_2026_09_11_deep: params.palw_audit_2026_09_11_deep_fence(),
            palw_audit_2026_09_23: params.palw_audit_2026_09_23_fence(),
            palw_receipt_spend_v4: params.palw_receipt_spend_v4_fence(),
            palw_header_forms: params.palw_header_form_fences_v1(),
            palw_lifecycle_kind_fences: params.palw_lifecycle_kind_fences_v1(),
            palw_offence_attribution: params.palw_offence_attribution_fence(),
            palw_activation_pool: params.palw_activation_pool_fence(),
            palw_rcore_plus: params.palw_rcore_plus_fence(),
            palw_panel_seed_execution: params.palw_panel_seed_execution_fence(),
            palw_operator_anchor: params.palw_operator_anchor_rule_v1(),
            palw_vertex_watch: Default::default(),
            palw_settled_anchor_depth: params.palw_settled_anchor_depth,
            palw_admission_audit_period_daa: params.palw_admission_audit_period_daa,
            palw_exec_quantum_maturity_daa: params.palw_exec_quantum_maturity_v1(),
            palw_anchor_at_ceiling: params.palw_anchor_at_ceiling_fence(),
            palw_frontier_provenance: params.palw_frontier_provenance,
            palw_reorg_strict_economic_win: params.palw_reorg_strict_economic_win,
            palw_fork_choice_rule_e: params.palw_fork_choice_rule_e_v1,
            palw_rule_e_max_extra_validated: Default::default(),
            palw_rule_e_searches: Default::default(),
            palw_validator_payout_bounds: params.palw_validator_payout_bounds_fence(),
            palw_slashing_evidence_utxo_genuine: params.palw_slashing_evidence_utxo_genuine,
            palw_lane_accept_parents_first: params.palw_lane_accept_parents_first_fence(),
            palw_exec_v2: params.palw_exec_payload_v2_fence(),
            palw_exec_v2_virtual_anchor: parking_lot::Mutex::new(None),
            finality_depth: params.blockrate.finality_depth,
            utxo_diffs_store: storage.utxo_diffs_store.clone(),
            rewarded_epochs_store: storage.rewarded_epochs_store.clone(),
            epoch_accumulator_store: storage.epoch_accumulator_store.clone(),
            block_quality_pool_store: storage.block_quality_pool_store.clone(),
            reserve_balance_store: storage.reserve_balance_store.clone(),
            utxo_multisets_store: storage.utxo_multisets_store.clone(),
            acceptance_data_store: storage.acceptance_data_store.clone(),
            virtual_stores: storage.virtual_stores.clone(),
            pruning_meta_stores: storage.pruning_meta_stores.clone(),
            lkg_virtual_state: storage.lkg_virtual_state.clone(),

            block_window_cache_for_difficulty: storage.block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time: storage.block_window_cache_for_past_median_time.clone(),

            ghostdag_manager: services.ghostdag_manager.clone(),
            reachability_service: services.reachability_service.clone(),
            relations_service: services.relations_service.clone(),
            dag_traversal_manager: services.dag_traversal_manager.clone(),
            window_manager: services.window_manager.clone(),
            coinbase_manager: services.coinbase_manager.clone(),
            transaction_validator: services.transaction_validator.clone(),
            pruning_point_manager: services.pruning_point_manager.clone(),
            parents_manager: services.parents_manager.clone(),
            depth_manager: services.depth_manager.clone(),

            pruning_lock,
            notification_root,
            counters,
            _mining_rules: mining_rules,
        }
    }

    fn bridge_finality_is_fresh(&self, sink: BlockHash) -> bool {
        if self.headers_store.get_daa_score(sink).is_ok_and(|daa| self.dns_retired_at(daa)) {
            return true;
        }
        let Some(dns_params) = self.dns_params.as_ref() else {
            return false;
        };
        let Ok(state) = self.dns_state_store.read().get() else {
            return false;
        };
        let Ok(sink_blue_score) = self.headers_store.get_blue_score(sink) else {
            return false;
        };
        let dns_confirmed =
            is_dns_confirmed(state.work_depth, state.stake_depth, dns_params.required_work_depth, dns_params.required_stake_depth);
        let anchor_blue_score = self.headers_store.get_blue_score(state.last_dns_confirmed_anchor).ok();
        dns_finality_fresh_for_bridge(dns_confirmed, state.last_dns_confirmed_anchor, anchor_blue_score, sink_blue_score, dns_params)
    }

    /// **ADR-0109 Decision 1: the deposit-lock index follows the virtual UTXO set, in its batch.**
    /// Removed lock outputs leave, added lock outputs enter; a non-lock output on either side costs
    /// one script-class check and touches nothing. Unconditional — a lock output is consensus-legal
    /// on every network, and the template path is what gates on EVM activation.
    pub(super) fn stage_evm_deposit_locks(&self, batch: &mut WriteBatch, diff: &impl ImmutableUtxoDiff) {
        let mut store = self.evm_deposit_lock_store.write();
        for (outpoint, entry) in diff.removed().iter() {
            if deposit_lock_record(entry).is_some() {
                store.delete_batch(batch, *outpoint).unwrap();
            }
        }
        for (outpoint, entry) in diff.added().iter() {
            if let Some(record) = deposit_lock_record(entry) {
                store.insert_batch(batch, *outpoint, record).unwrap();
            }
        }
    }

    /// ADR-0109 Decision 1: (re)build the index from the virtual UTXO set — once for a database from
    /// before the index, and whenever the set is replaced wholesale (a pruning-point import).
    pub(super) fn rebuild_evm_deposit_lock_index(&self, utxo_set: &crate::model::stores::utxo_set::DbUtxoSetStore) {
        let mut store = self.evm_deposit_lock_store.write();
        store.clear().unwrap();
        let mut locks = 0usize;
        for item in utxo_set.iterator() {
            let (outpoint, entry) = item.unwrap();
            if let Some(record) = deposit_lock_record(&entry) {
                store.insert(outpoint, record).unwrap();
                locks += 1;
            }
        }
        store.set_built().unwrap();
        info!("[evm-bridge] deposit-lock index built from the virtual UTXO set: {locks} unclaimed lock(s)");
    }

    /// **ADR-0109 Decision 1 — the template's claims: the index's, oldest lock first, then whatever
    /// the queue holds that the index does not** (a claim relayed for a lock this node's set does
    /// not show yet). A lock already in its refund window is left out — `prepare_deposit_claims`
    /// would refuse it (`RefundWindowOpen`) and report to the queue an invalid claim for an outpoint
    /// the queue never held. Inert below EVM activation.
    fn with_indexed_deposit_claims(
        &self,
        mut data: kaspa_consensus_core::evm::EvmTemplateData,
        daa_score: u64,
    ) -> kaspa_consensus_core::evm::EvmTemplateData {
        if daa_score < self.evm_activation_daa_score {
            return data;
        }
        let mut locks = match self.evm_deposit_lock_store.read().all() {
            Ok(locks) => locks,
            Err(e) => {
                warn!("[evm-bridge] deposit-lock index unreadable ({e}); this template carries only queued claims");
                return data;
            }
        };
        locks.retain(|(_, record)| daa_score < record.timeout_daa_score);
        locks.sort_by(|(a_op, a), (b_op, b)| {
            (a.block_daa_score, a_op.transaction_id, a_op.index).cmp(&(b.block_daa_score, b_op.transaction_id, b_op.index))
        });
        let mut seen: std::collections::HashSet<TransactionOutpoint> = locks.iter().map(|(outpoint, _)| *outpoint).collect();
        let queued = std::mem::take(&mut data.system_ops);
        let mut system_ops: Vec<kaspa_consensus_core::evm::DepositClaim> =
            locks.into_iter().map(|(outpoint, record)| record.claim(outpoint)).collect();
        for claim in queued {
            if seen.insert(claim.deposit_outpoint) {
                system_ops.push(claim);
            }
        }
        data.system_ops = system_ops;
        data
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(msg) = self.receiver.recv() {
            if msg.is_exit_message() {
                break;
            }

            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info

            let messages: Vec<VirtualStateProcessingMessage> = std::iter::once(msg).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", messages.len());

            let resolution = self.resolve_virtual();
            if let Err(ref unavailable) = resolution {
                warn!("virtual resolution deferred by local native capacity: {unavailable}; no block is disqualified");
            }

            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    VirtualStateProcessingMessage::Exit => break 'outer,
                    VirtualStateProcessingMessage::Process(task, virtual_state_result_transmitter) => {
                        // We don't care if receivers were dropped
                        let result = match &resolution {
                            Ok(()) => Ok(statuses_read.get(task.block().hash()).unwrap()),
                            Err(unavailable) => Err(RuleError::LocalNativeCapacityUnavailable(unavailable.clone())),
                        };
                        let _ = virtual_state_result_transmitter.send(result);
                    }
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).unwrap();
    }

    fn resolve_virtual(self: &Arc<Self>) -> Result<(), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let virtual_read = self.virtual_stores.upgradable_read();
        let prev_state = virtual_read.state.get().unwrap();
        let finality_point = self.virtual_finality_point(&prev_state.ghostdag_data, pruning_point);

        // PRUNE SAFETY: in order to avoid locking the prune lock throughout virtual resolving we make sure
        // to only process blocks in the future of the finality point (F) which are never pruned (since finality depth << pruning depth).
        // This is justified since:
        //      1. Tips which are not in the future of F definitely don't have F on their chain
        //         hence cannot become the next sink (due to finality violation).
        //      2. Such tips cannot be merged by virtual since they are violating the merge depth
        //         bound (merge depth <= finality depth).
        // (both claims are true by induction for any block in their past as well)
        let prune_guard = self.pruning_lock.blocking_read();
        let tips = self
            .body_tips_store
            .read()
            .get()
            .unwrap()
            .read()
            .iter()
            .copied()
            // QR reachability hardening: drop a body tip whose reachability is missing (half-pruned);
            // it is below finality and protected by pruning-point finality. Consensus-neutral.
            .filter(|&h| match self.reachability_service.try_is_dag_ancestor_of(finality_point, h) {
                Ok(v) => v,
                Err(_) => {
                    debug!("resolve_virtual: body tip {h} has no reachability vs finality {finality_point} (half-pruned?); dropping tip");
                    false
                }
            })
            .collect_vec();
        drop(prune_guard);
        let prev_sink = prev_state.ghostdag_data.selected_parent;
        let mut accumulated_diff = prev_state.utxo_diff.clone().to_reversed();

        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B): the per-block active-bond
        // view, walked in lockstep with `accumulated_diff` so that at each
        // chain-block UTXO verification it equals the bond set as-of that
        // block's selected parent (the deterministic, as-of-block bond
        // resolution the validator-reward coinbase fan-out needs — PR-10.5′-b3).
        // Seeded from the `StakeBonds` store snapshot (= state at `prev_sink`);
        // empty + untouched on networks without the overlay (`dns_params` None).
        // No consumer yet (b2a): `verify_expected_utxo_state` receives it inert.
        let mut accumulated_bond_view = self.initial_active_bond_view();

        // ADR-0125: round tips are never sink candidates; they are offered to virtual as parents to
        // merge, after the sink is chosen among the chain's own blocks.
        let round_tips = self.palw_round_tips(&tips);
        let (new_sink, virtual_parent_candidates) = self.sink_search_algorithm(
            &virtual_read,
            &mut accumulated_diff,
            &mut accumulated_bond_view,
            prev_sink,
            tips,
            finality_point,
            pruning_point,
        )?;
        let (virtual_parents, virtual_ghostdag_data) =
            self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point, round_tips);
        assert_eq!(virtual_ghostdag_data.selected_parent, new_sink);

        let sink_multiset = self.utxo_multisets_store.get(new_sink).unwrap();
        let chain_path = self.dag_traversal_manager.calculate_chain_path(prev_sink, new_sink, None);
        let sink_ghostdag_data = Lazy::new(|| self.ghostdag_store.get_data(new_sink).unwrap());
        // Cache the DAA and Median time windows of the sink for future use, as well as prepare for virtual's window calculations
        self.cache_sink_windows(new_sink, prev_sink, &sink_ghostdag_data);
        // The lead cap's local policy (`palw_clock_lead_cap`): a granted beat whose step every template
        // would have to stamp past this node's clock plus 132 s is not merged yet. Asked after the sink's
        // windows are cached, so the virtual windows it computes build on them.
        let (virtual_parents, virtual_ghostdag_data) =
            self.palw_lead_cap_virtual_parents(virtual_parents, virtual_ghostdag_data, pruning_point);
        assert_eq!(
            virtual_ghostdag_data.selected_parent, new_sink,
            "the lead cap's policy leaves only merged tips out, never the sink"
        );

        let new_virtual_state = match self.calculate_and_commit_virtual_state(
            virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            sink_multiset,
            &mut accumulated_diff,
            // After `sink_search_algorithm` the walked view equals the bond
            // set as-of the new sink (= the virtual block's selected parent).
            &accumulated_bond_view,
            &chain_path,
        ) {
            Ok(state) => state,
            Err(RuleError::LocalNativeCapacityUnavailable(unavailable)) => return Err(unavailable),
            Err(error) => panic!("all possible rule errors are unexpected here: {error}"),
        };

        let compact_sink_ghostdag_data = if let Some(sink_ghostdag_data) = Lazy::get(&sink_ghostdag_data) {
            // If we had to retrieve the full data, we convert it to compact
            sink_ghostdag_data.to_compact()
        } else {
            // Else we query the compact data directly.
            self.ghostdag_store.get_compact_data(new_sink).unwrap()
        };

        // Update the pruning processor about the virtual state change
        // Empty the channel before sending the new message. If pruning processor is busy, this step makes sure
        // the internal channel does not grow with no need (since we only care about the most recent message)
        let _consume = self.pruning_receiver.try_iter().count();
        self.pruning_sender.send(PruningProcessingMessage::Process { sink_ghostdag_data: compact_sink_ghostdag_data }).unwrap();

        // Emit notifications
        let accumulated_diff = Arc::new(accumulated_diff);
        let virtual_parents = Arc::new(new_virtual_state.parents.clone());
        self.notification_root
            .notify(Notification::NewBlockTemplate(NewBlockTemplateNotification {}))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::UtxosChanged(UtxosChangedNotification::new(accumulated_diff, virtual_parents)))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::SinkBlueScoreChanged(SinkBlueScoreChangedNotification::new(compact_sink_ghostdag_data.blue_score)))
            .expect("expecting an open unbounded channel");
        self.notification_root
            .notify(Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification::new(new_virtual_state.daa_score)))
            .expect("expecting an open unbounded channel");
        self.emit_palw_panel_notifications();
        if self.notification_root.has_subscription(EventType::VirtualChainChanged) {
            // check for subscriptions before the heavy lifting
            let added_chain_blocks_acceptance_data =
                chain_path.added.iter().copied().map(|added| self.acceptance_data_store.get(added).unwrap()).collect_vec();
            self.notification_root
                .notify(Notification::VirtualChainChanged(VirtualChainChangedNotification::new(
                    chain_path.added.into(),
                    chain_path.removed.into(),
                    Arc::new(added_chain_blocks_acceptance_data),
                )))
                .expect("expecting an open unbounded channel");
        }
        Ok(())
    }

    pub(crate) fn virtual_finality_point(&self, virtual_ghostdag_data: &GhostdagData, pruning_point: BlockHash) -> BlockHash {
        let finality_point = self.depth_manager.calc_finality_point(virtual_ghostdag_data, pruning_point);
        // QR reachability hardening: a half-pruned DB can transiently miss the finality point's
        // reachability until pruning recovery completes; treat a missing row as below-pruning-point
        // and fall back to the pruning point (identical to the IBD-start else branch). Consensus-neutral.
        let fp_reachable = match self.reachability_service.try_is_chain_ancestor_of(pruning_point, finality_point) {
            Ok(v) => v,
            Err(_) => {
                debug!(
                    "virtual_finality_point: finality point {finality_point} has no reachability (half-pruned?); falling back to pruning point {pruning_point}"
                );
                false
            }
        };
        if fp_reachable {
            finality_point
        } else {
            // At the beginning of IBD when virtual finality point might be below the pruning point
            // or disagreeing with the pruning point chain, we take the pruning point itself as the finality point
            pruning_point
        }
    }

    /// Calculates the UTXO state of `to` starting from the state of `from`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `from` from virtual.
    /// The function returns the top-most UTXO-valid block on `chain(to)` which is ideally
    /// `to` itself (with the exception of returning `from` if `to` is already known to be UTXO disqualified,
    /// of returning the old-chain block whose PALW delta the backward walk could not revert, and of
    /// returning the block below the one whose stored PALW delta the forward walk could not re-apply).
    /// When returning it is guaranteed that `diff` holds the diff of the returned block from virtual —
    /// and that the bond view and, wherever the walk established one, its PALW state stand there too.
    pub(super) fn calculate_utxo_state_relatively(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        from: BlockHash,
        to: BlockHash,
    ) -> Result<BlockHash, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.1): walk the active-bond
        // view in lockstep with `diff` so it always equals the bond set as-of
        // the block whose UTXO state `diff` represents. No-op on networks
        // without the overlay. No consumer yet (b2a) — the view is passed to
        // `verify_expected_utxo_state` inert.
        let track_bonds = self.dns_params.is_some();

        // ADR-0042 Decision 5 / Unit C. `Some` exactly on a `ConsensusV2` network, where it is
        // loaded from the stored tip ROOT-VERIFIED (`load_tip` runs the full `into_state` rebuild),
        // so a corrupted or hand-edited snapshot refuses to become a sink instead of becoming one
        // quietly. `None` everywhere else, and every leg below is a no-op there — the walk is not a
        // dead handle in the blue-work pipeline, it is absent from it.
        //
        // **The state is derived AT `from`, not read as if the tip already stood there** (launch
        // blockers §7, fourth bullet). This used to take the stored tip's state verbatim, on the
        // invariant that the tip row and virtual's selected parent always agree. They agree only
        // between rounds: the tip is written in its own batch at the end of this walk, and the
        // virtual state commits after it, so a crash in that window leaves the tip one or more
        // blocks AHEAD of the sink. The next start then reverted `from`'s ancestors out of a state
        // that already stood past them, `revert_delta_v2` rejected the value it was asked to
        // replace, and the `.expect` below turned that into a panic — on every subsequent start,
        // with no recovery short of wiping the data directory.
        //
        // Walking the path from the tip to `from` costs nothing when they are equal (the common
        // case: an empty path) and repairs the window when they are not, in either direction. A
        // walk that genuinely cannot be completed is a named refusal that leaves the node up and
        // the sink where it was, not a crash loop.
        let mut palw_state = match self.palw_state_params_v2.as_ref() {
            None => None,
            Some(params) => {
                let loaded = match self.palw_state_v2_store.read().load_tip_retained_v1(params) {
                    Ok(loaded) => loaded,
                    Err(kaspa_database::prelude::StoreError::LocalResourceUnavailable(reason)) => {
                        return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                            kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
                        >("native cold tip capacity", &reason));
                    }
                    Err(e) => {
                        panic!(
                            "the stored PALW V2 tip does not load under this build ({e}). A state written by another \
                         PALW_STATE_V2_VERSION or ruleset is not migrated in place: this network re-genesises on a \
                         ruleset change (ADR-0042 Decision 11, ADR-0075 §7), so wipe the datadir and resync from peers \
                         announcing this build's fingerprint"
                        )
                    }
                };
                match loaded {
                    None => None,
                    Some((tip_block, tip_state)) if tip_block == from => Some(tip_state),
                    Some((tip_block, tip_state)) => {
                        let path = self.dag_traversal_manager.calculate_chain_path(tip_block, from, None);
                        let removed: Vec<BlockHash> = path.removed.to_vec();
                        let added: Vec<BlockHash> = path.added.to_vec();
                        let store = self.palw_state_v2_store.read();
                        match crate::processes::palw_state_walk::walk_chain_path_retained_v1(
                            &store, params, tip_state, &removed, &added,
                        ) {
                            Ok(state) => {
                                warn!(
                                    "PALW V2 state tip stood at {tip_block} while the UTXO walk starts at {from} (an unclean shutdown between the two commits); re-derived the state at {from} over {} reverted and {} applied deltas",
                                    removed.len(),
                                    added.len()
                                );
                                Some(state)
                            }
                            Err(e) => {
                                if let Some(capacity) = e.native_capacity_v1() {
                                    return Err(capacity);
                                }
                                // **A gap between the stored tip and the walk's base is THIS NODE's
                                // problem, and it used to be charged to the chain.**
                                //
                                // Returning `from` here says "the candidate is UTXO-disqualified",
                                // but nothing about the candidate was examined: the failure is that
                                // the deltas between the tip row and `from` are not all present.
                                // The sink search calls this once per candidate, so ONE missing
                                // delta disqualified every block it could reach, virtual walked back
                                // to the finality frontier, and the search then died on an empty
                                // heap — the 2026-09-07 field report's three identical crashes,
                                // which reproduced on every start because the store stayed in that
                                // shape.
                                //
                                // The tip is not the only base. The PRUNING SNAPSHOT is a state at
                                // a block on the chain every candidate descends from, and every
                                // chain block above it has a delta (that is how it was applied), so
                                // walking forward from there rebuilds the state at `from` without
                                // the tip. A node whose tip row is unusable recovers instead of
                                // declaring the network invalid.
                                match store.load_pruning_snapshot_retained_v1(params) {
                                    Ok(Some((snap_block, snap_state))) => {
                                        let snap_path = self.dag_traversal_manager.calculate_chain_path(snap_block, from, None);
                                        let snap_removed: Vec<BlockHash> = snap_path.removed.to_vec();
                                        let snap_added: Vec<BlockHash> = snap_path.added.to_vec();
                                        match crate::processes::palw_state_walk::walk_chain_path_retained_v1(
                                            &store,
                                            params,
                                            snap_state,
                                            &snap_removed,
                                            &snap_added,
                                        ) {
                                            Ok(state) => {
                                                warn!(
                                                    "PALW V2 state tip stands at {tip_block}, the UTXO walk starts at {from}, and the path between them cannot be walked ({e}); \
                                                     rebuilt the state at {from} from the pruning snapshot at {snap_block} instead ({} block(s) reverted, {} applied). \
                                                     The tip row is stale or incomplete; it is rewritten when this round commits.",
                                                    snap_removed.len(),
                                                    snap_added.len()
                                                );
                                                Some(state)
                                            }
                                            Err(snap_err) => {
                                                if let Some(capacity) = snap_err.native_capacity_v1() {
                                                    return Err(capacity);
                                                }
                                                error!(
                                                    "PALW V2 state cannot be established at {from}: the tip at {tip_block} does not walk here ({e}) and neither does the \
                                                     pruning snapshot at {snap_block} ({snap_err}). This node cannot evaluate ANY candidate until its state store is repaired \
                                                     — resync this data directory from peers announcing this build's fingerprint."
                                                );
                                                return Ok(from);
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        error!(
                                            "PALW V2 state cannot be established at {from}: the tip at {tip_block} does not walk here ({e}) and this store holds no pruning \
                                             snapshot to rebuild from. This node cannot evaluate ANY candidate until its state store is repaired — resync this data directory."
                                        );
                                        return Ok(from);
                                    }
                                    Err(kaspa_database::prelude::StoreError::LocalResourceUnavailable(reason)) => {
                                        return Err(kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                                            kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
                                        >(
                                            "native cold pruning snapshot capacity", &reason
                                        ));
                                    }
                                    Err(snap_err) => {
                                        error!(
                                            "PALW V2 state cannot be established at {from}: the tip at {tip_block} does not walk here ({e}) and the pruning snapshot cannot be \
                                             read ({snap_err}). This node cannot evaluate ANY candidate until its state store is repaired — resync this data directory."
                                        );
                                        return Ok(from);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        };

        // Avoid reorging if disqualified status is already known
        if self.statuses_store.read().get(to).unwrap() == StatusDisqualifiedFromChain {
            return Ok(from);
        }

        let mut split_point: Option<BlockHash> = None;

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(from) {
            if self.reachability_service.is_chain_ancestor_of(current, to) {
                split_point = Some(current);
                break;
            }

            // ADR-0042 Unit C: the PALW state walks in lockstep with `diff`, for the same reason
            // the bond view does — a candidate's V2 standing must be a fold over THAT candidate's
            // chain and never a read of the node's sink (P0-4, the partition this layout exists to
            // make unrepresentable). `revert_delta_v2` verifies every value it replaces, so a
            // delta applied to the wrong parent is an error rather than a quiet divergence.
            //
            // **Reverted FIRST, before `diff` and the bond view** (ADR-0152 Phase 2, P2-3: found by
            // `p2_an_undecodable_vesting_delta_is_a_fault_and_the_reorg_it_blocks_holds_the_sink`).
            // This used to run after both had been reversed through `current` and then return
            // `from`, breaking this function's one guarantee — `diff` holds the diff of the block
            // it returns: the caller took `from` as established while `diff` stood one block lower,
            // so a node holding on a delta row it could not revert (an undecodable or pruned row
            // under its sink, with a heavier chain in reach) committed a virtual UTXO set without
            // its sink's own acceptance — spent outputs back, created ones gone — and every later
            // walk from there reversed the same block again. The PALW revert touches neither, so
            // the order of the two is free; running it first means a stop leaves `diff` and the
            // bond view at `current`, which is what is returned — `from` itself when the gap is
            // the first block. A node-local fault path only: on a store that reads, nothing moves.
            if let Some(state) = palw_state.as_mut() {
                // The backward twin of the forward leg's rule: a row this node does not have is a
                // gap in this node. Stopping returns the last point actually established, which is
                // what every other unfinished walk in this function returns.
                let params = self.palw_state_params_v2.as_ref().expect("palw_state is Some only when the params are");
                let reverted = self
                    .palw_state_v2_store
                    .read()
                    .delta_of_retained_v1(current)
                    .map_err(crate::processes::palw_state_walk::PalwStateWalkError::Store)
                    .and_then(|(_, delta)| {
                        state.try_delta_v1(&delta, params, true).map_err(crate::processes::palw_state_walk::PalwStateWalkError::from)
                    });
                match reverted {
                    Ok(previous) => *state = previous,
                    Err(why) => {
                        if let Some(capacity) = why.native_capacity_v1() {
                            return Err(capacity);
                        }
                        error!(
                            "PALW V2 state cannot be reverted through chain block {current} ({why}); the UTXO walk stops at {current} \
                             and virtual will not move onto {to}. This is a gap in THIS node's delta store, not a fault of the chain — \
                             resync this data directory if it persists."
                        );
                        return Ok(current);
                    }
                }
            }
            let mergeset_diff = self.utxo_diffs_store.get(current).unwrap();
            // Apply the diff in reverse
            diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
            if track_bonds {
                // Mirror the reverse on the bond view. `current` is leaving the
                // selected chain, so its acceptance data is committed.
                bond_view.revert(&self.dns_bond_mutations_for_chain_block(current, bond_view));
            }
        }

        let split_point = split_point.expect("chain iterator was expected to reach the reorg split point");
        debug!("VIRTUAL PROCESSOR, found split point: {split_point}");

        // O12 (IBD catch-up): when the walk ahead contains a long run of
        // pending chain blocks, pre-execute their EVM acceptance on a pipeline
        // worker overlapped with this thread's serial UTXO validation. Inert
        // when the lane is inactive, on short walks (steady state: 1 block),
        // and on non-evm builds. Commits stay HERE, in canonical order.
        let evm_pipeline = self.maybe_spawn_evm_pipeline(split_point, to);

        // A variable holding the most recent UTXO-valid block on `chain(to)` (note that it's maintained such
        // that 'diff' is always its UTXO diff from virtual)
        let mut diff_point = split_point;

        // Walk back up to the new virtual selected parent candidate
        let mut chain_block_counter = 0;
        let mut chain_disqualified_counter = 0;
        for (selected_parent, current) in self.reachability_service.forward_chain_iterator(split_point, to, true).tuple_windows() {
            if selected_parent != diff_point {
                // This indicates that the selected parent is disqualified, propagate up and continue
                let statuses_guard = self.statuses_store.upgradable_read();
                if statuses_guard.get(current).unwrap() != StatusDisqualifiedFromChain {
                    RwLockUpgradableReadGuard::upgrade(statuses_guard).set(current, StatusDisqualifiedFromChain).unwrap();
                    chain_disqualified_counter += 1;
                }
                continue;
            }

            match self.utxo_diffs_store.get(current) {
                Ok(mergeset_diff) => {
                    // Unit C, forward leg: this block was validated before, so its delta is on
                    // disk and re-applying it reproduces the transition bit-for-bit. Re-running
                    // the transition here instead would be a second computation of one fact, and
                    // a second chance to disagree with what the chain already committed to.
                    //
                    // **Applied FIRST, before `diff`, `diff_point` and the bond view** — the
                    // forward twin of the backward leg's order above (ADR-0152 Phase 2, P2-3
                    // review; `p2_a_damaged_row_the_forward_leg_must_reapply_holds_the_old_sink`).
                    // This used to run after all three had moved onto `current`, so a row that
                    // would not apply returned `current` — a block whose PALW state this walk
                    // never reached. The sink search took it as established; every later walk
                    // from it had to derive its state through the same damaged row (the tip row
                    // stood elsewhere), failed there and returned it again; no candidate, not
                    // even the previous sink, could be reached from it; and the exhausted-heap
                    // `assert_eq!` killed the virtual processor — on that block and again on
                    // every restart. A node on a chain B that kept an old chain A's rows, one of
                    // them damaged, died the moment A's heavier tail arrived. Now a stop returns
                    // the unchanged `diff_point`, where `diff`, the bond view and the PALW state
                    // all still stand, and the next walk from there reads only rows that apply.
                    // The PALW apply reads neither `diff` nor the bond view, so on a store that
                    // reads the order is free and nothing moves.
                    if let Some(state) = palw_state.as_mut() {
                        // **A delta this node does not have is a gap in this node, not a verdict on
                        // the block.** Both of these used to be `expect`, on the reasoning above:
                        // a validated chain block wrote its delta, so it is there. It is there
                        // until it is not — a prune that raced the walk, an unclean shutdown
                        // between two batches, a datadir carried across builds — and then the
                        // node died on a row it could have simply stopped at. The walk stops here
                        // and returns the last point it did establish, which is the same shape as
                        // every other "this candidate cannot be taken further" answer in this
                        // function; the caller holds virtual where it is and says so.
                        let params = self.palw_state_params_v2.as_ref().expect("palw_state is Some only when the params are");
                        let applied = self
                            .palw_state_v2_store
                            .read()
                            .delta_of_retained_v1(current)
                            .map_err(crate::processes::palw_state_walk::PalwStateWalkError::Store)
                            .and_then(|(_, delta)| {
                                state
                                    .try_delta_v1(&delta, params, false)
                                    .map_err(crate::processes::palw_state_walk::PalwStateWalkError::from)
                            });
                        match applied {
                            Ok(next) => *state = next,
                            Err(why) => {
                                if let Some(capacity) = why.native_capacity_v1() {
                                    return Err(capacity);
                                }
                                error!(
                                    "PALW V2 state cannot advance through chain block {current} ({why}); the UTXO walk stops at {diff_point} \
                                     and virtual will hold there. This is a gap in THIS node's delta store, not a fault of the chain — \
                                     resync this data directory if it persists."
                                );
                                return Ok(diff_point);
                            }
                        }
                    }
                    diff.with_diff_in_place(mergeset_diff.deref()).unwrap();
                    diff_point = current;
                    if track_bonds {
                        // `current` is an already-validated chain block joining
                        // the diff; its acceptance data is committed.
                        bond_view.apply(&self.dns_bond_mutations_for_chain_block(current, bond_view));
                    }
                }
                Err(StoreError::KeyNotFound(_)) => {
                    if self.statuses_store.read().get(current).unwrap() == StatusDisqualifiedFromChain {
                        // A persisted disqualified status is only a cache of a past validation result. Re-run the
                        // deterministic checks when the block becomes a selected-chain candidate again so nodes can
                        // recover after liveness-first rule changes without wiping their local DAG state. Blocks that
                        // are still invalid will be marked disqualified again below.
                        debug!("Revalidating previously disqualified selected-chain block {}", current);
                    }

                    let header = self.headers_store.get_header(current).unwrap();
                    let mergeset_data = self.ghostdag_store.get_data(current).unwrap();
                    let pov_daa_score = header.daa_score;
                    // RFC-0008 v2: past the EXEC payload fence the EXEC blocks this block accepts are the ones its coinbase anchor
                    // covers. A wrong count or root, a malformed trailer or an anchor on a lane block disqualifies the block from the
                    // chain like any commitment fault. A block without a trailer, and every block below the fence, is unchanged.
                    let (mergeset_data, exec_resolved) =
                        match self.palw_exec_v2_augment(current, &header, palw_state.as_deref(), mergeset_data) {
                            Ok(pair) => pair,
                            Err(exec_error) => {
                                info!("Block {} is disqualified from virtual chain (EXEC anchor): {}", current, exec_error);
                                self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                                chain_disqualified_counter += 1;
                                continue;
                            }
                        };

                    let selected_parent_multiset_hash = self.utxo_multisets_store.get(selected_parent).unwrap();
                    let selected_parent_utxo_view = (&stores.utxo_set).compose(&*diff);

                    let mut ctx = UtxoProcessingContext::new(mergeset_data.into(), selected_parent_multiset_hash);

                    // `bond_view` currently equals the bond set as-of `selected_parent`
                    // (the verify point's selected-parent view — Addendum B §B.3),
                    // so it is the same view both `calculate_utxo_state` (slashing
                    // side-effect, PR-16.4-b2) and `verify_expected_utxo_state` read.
                    // ADR-0042 Decision 10: the escrows this block owes, read from the state the
                    // walk currently holds — which IS this block's selected parent, since the
                    // transition below has not run yet. Set before `calculate_utxo_state` because
                    // that is where the coinbase is verified against it.
                    ctx.palw_v2_payout_outputs = palw_state.as_ref().map(|s| self.palw_v2_payout_outputs(s)).unwrap_or_default();
                    // Audit C-08: and the collateral this block may not let anyone spend, from the
                    // same parent state and for the same reason — PLUS every bond this block's own
                    // mergeset declares.
                    //
                    // The parent state alone cannot hold a bond this block registers, and the
                    // registration is extracted from acceptance data AFTER the mergeset is already
                    // accepted. So the collateral output of a bond was spendable in the very block
                    // that registered it: `palw_bond_registration_binds_its_carrier_v2` proves the
                    // named output EXISTS in the carrier, never that it survived, and `apply_object`
                    // checks only `collateral >= min`. Chained transactions are forbidden inside one
                    // block but not across a mergeset, and `composed_view` is recomputed per merged
                    // block — so a transaction in a later merged block spends an output an earlier
                    // one created. The result is an Active bond backed by nothing.
                    //
                    // The DNS half of this same filter has had the treatment since the mergeset
                    // fence: `bond_gate_view` is the selected parent's bonds UNION every bond
                    // declared anywhere in this mergeset, on the argument that including a
                    // declaration that turns out UTXO-invalid is a harmless SAFE SUPERSET — its
                    // output does not exist, so nothing could spend it anyway. The same argument
                    // holds here, and the filter only ever FORBIDS a spend, so a superset cannot
                    // admit anything. Deterministic in the shared (parent state, mergeset) inputs,
                    // so construction and validation compute the same set.
                    ctx.palw_v2_locked_bonds =
                        palw_state.as_ref().map(|s| self.palw_v2_locked_bond_outpoints(s, pov_daa_score)).unwrap_or_default();
                    if palw_state.is_some() {
                        ctx.palw_v2_locked_bonds.extend(self.palw_v2_bonds_declared_in_mergeset(&ctx));
                    }
                    // ADR-0042 Decision 10: and what the selected parent's own claim already
                    // spent of its worker reward, so this coinbase does not pay it twice.
                    ctx.palw_v2_escrow_withheld =
                        palw_state.as_ref().map(|s| self.palw_v2_escrow_withheld_at(s, selected_parent, pov_daa_score)).unwrap_or(0);
                    // Launch blockers §8: and which of the OTHER merged blues this block may not
                    // pay at all, from the same parent state and for the same reason.
                    ctx.palw_v2_unentitled_blues = palw_state
                        .as_ref()
                        .map(|s| {
                            // The same non-DAA set `verify_expected_utxo_state` reads a few frames
                            // down, from the same store — the two must agree or the coinbase this
                            // computes is not the coinbase that is checked.
                            use crate::model::stores::daa::DaaStoreReader;
                            let non_daa = self.palw_exec_v2_non_daa(
                                &self
                                    .daa_excluded_store
                                    .get_mergeset_non_daa(current)
                                    .expect("the DAA window is written before the UTXO walk reaches this block"),
                                &ctx.ghostdag_data,
                                pov_daa_score,
                            );
                            // The SAME evaluation point the transition builds below, so the
                            // admission this asks and the admission that accepts are asked at one
                            // place on the chain (audit3 S-04).
                            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                                block: current,
                                daa_score: header.daa_score,
                                blue_score: header.blue_score,
                                subsidy: self.coinbase_manager.calc_block_subsidy(header.daa_score),
                            };
                            self.palw_v2_unentitled_blues(s, &ctx.ghostdag_data, &non_daa, &point)
                        })
                        .unwrap_or_default();
                    // RFC-0009: and how each merged V4 receipt block is split — the same state, mergeset and non-DAA set. Empty (and
                    // unread) while `palw_receipt_spend_v4` is dormant.
                    ctx.palw_v2_receipt_v4_payouts = palw_state
                        .as_ref()
                        .map(|s| {
                            use crate::model::stores::daa::DaaStoreReader;
                            let non_daa = self.palw_exec_v2_non_daa(
                                &self
                                    .daa_excluded_store
                                    .get_mergeset_non_daa(current)
                                    .expect("the DAA window is written before the UTXO walk reaches this block"),
                                &ctx.ghostdag_data,
                                pov_daa_score,
                            );
                            self.palw_v2_receipt_v4_payouts(s, &ctx.ghostdag_data, &non_daa, &ctx.palw_v2_unentitled_blues)
                        })
                        .unwrap_or_default();
                    // B-1 (deep fence): and how much of each OTHER merged block's carve is withheld
                    // and escrowed rather than paid, from the same parent state and the same
                    // evaluation point. Empty below `palw_audit_2026_09_11_deep` (paid in full, as
                    // before). Needs no `mergeset_rewards` — it derives each block's subsidy from its
                    // own header — so it is safe to compute here, before the reward map is filled.
                    ctx.palw_v2_merged_escrow_withheld = palw_state
                        .as_ref()
                        .map(|s| {
                            use crate::model::stores::daa::DaaStoreReader;
                            let non_daa = self.palw_exec_v2_non_daa(
                                &self
                                    .daa_excluded_store
                                    .get_mergeset_non_daa(current)
                                    .expect("the DAA window is written before the UTXO walk reaches this block"),
                                &ctx.ghostdag_data,
                                pov_daa_score,
                            );
                            let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                                block: current,
                                daa_score: header.daa_score,
                                blue_score: header.blue_score,
                                subsidy: self.coinbase_manager.calc_block_subsidy(header.daa_score),
                            };
                            self.palw_v2_merged_escrow_withheld(s, &ctx.ghostdag_data, &non_daa, &point)
                        })
                        .unwrap_or_default();
                    // Audit C-08 part three: and what a released bond's spend must destroy.
                    ctx.palw_v2_bond_burns =
                        palw_state.as_ref().map(|s| self.palw_v2_bond_burn_obligations(s, pov_daa_score)).unwrap_or_default();
                    // G14 lane escrow-rfc08 (RFC-0035 A): and which funded-escrow outpoints are locked, from the same parent state.
                    ctx.palw_escrow_locks.rows =
                        palw_state.as_ref().map(|s| self.palw_escrow_locked_rows_v1(s, pov_daa_score)).unwrap_or_default();
                    // ADR-0125: which merged round blocks hold their permit, from the same parent state.
                    ctx.palw_round_verdicts = palw_state
                        .as_ref()
                        .and_then(|s| self.palw_round_verdicts_at_v1(s, &ctx.ghostdag_data, pov_daa_score, current));
                    // Lane SCAN: node-local telemetry of each judged round block (never read by consensus).
                    if let Some(verdicts) = ctx.palw_round_verdicts.as_ref() {
                        self.palw_record_round_verdicts(verdicts);
                    }
                    // Funded-original candidate inspection remains disabled here:
                    // no native shared-service/profile activation contract exists.
                    // Native integration tests exercise its synchronous entry with
                    // real validated inputs; ordinary production processing adds
                    // no unfunded decode/signature/graph work.
                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, &*bond_view, pov_daa_score);
                    // ADR-0125: one line a chain block that merges the lane — what an operator (and the
                    // devnet drill) reads to see permits granted and the transactions they carried. The
                    // native (payment) transactions are counted apart from PALW carriers and the first
                    // few named, so a payment can be traced to the lane rather than inferred from a
                    // balance that a chain block could equally have moved.
                    if let Some(verdicts) = ctx.palw_round_verdicts.as_ref().filter(|v| !v.round_blocks.is_empty()) {
                        let mut carried = 0usize;
                        let mut native: Vec<kaspa_consensus_core::tx::TransactionId> = Vec::new();
                        for entry in ctx.mergeset_acceptance_data.iter().filter(|entry| verdicts.permitted.contains(&entry.block_hash))
                        {
                            let txs = self.block_transactions_store.get(entry.block_hash).ok();
                            for accepted in entry.accepted_transactions.iter().filter(|tx| tx.index_within_block != 0) {
                                carried += 1;
                                if txs
                                    .as_ref()
                                    .and_then(|txs| txs.get(accepted.index_within_block as usize))
                                    .is_some_and(|tx| tx.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE)
                                {
                                    native.push(accepted.transaction_id);
                                }
                            }
                        }
                        const NAMED: usize = 4;
                        let mut named: Vec<String> = native.iter().take(NAMED).map(|id| id.to_string()).collect();
                        if native.len() > NAMED {
                            named.push(format!("+{}", native.len() - NAMED));
                        }
                        info!(
                            "[palw-round-lane] chain block {current} merged {} round block(s): {} permit(s) granted, {carried} transaction(s) accepted from them ({} native{}{})",
                            verdicts.round_blocks.len(),
                            verdicts.permitted.len(),
                            native.len(),
                            if named.is_empty() { "" } else { ": " },
                            named.join(", ")
                        );
                    }

                    // kaspa-pq EVM Lane v0.4 (§2.3/§9): the lazy chain-context
                    // EVM step — the FIRST time a block becomes a selected-chain
                    // candidate (this KeyNotFound arm), validate its deposit
                    // claims, execute its mergeset acceptance, verify
                    // `evm_commitment_root`, and fold the bridge's UTXO
                    // side-effects (consumed locks + synthetic withdrawal
                    // outputs) into ctx BEFORE `verify_expected_utxo_state`, so
                    // the header's `utxo_commitment` covers them. A fault
                    // disqualifies the block from the chain exactly like a UTXO
                    // fault (no poison; the block stays in the DAG). A single
                    // u64 compare while the lane is inert.
                    let evm_staged = match self.evm_chain_context_step(
                        current,
                        selected_parent,
                        &header,
                        &mut ctx,
                        &selected_parent_utxo_view,
                        evm_pipeline.as_ref(),
                        palw_state.as_deref(),
                    ) {
                        Ok(staged) => staged,
                        Err(evm_error) => {
                            info!("Block {} is disqualified from virtual chain (EVM): {}", current, evm_error);
                            self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                            chain_disqualified_counter += 1;
                            continue;
                        }
                    };

                    let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &*bond_view, &header);

                    if let Err(rule_error) = res {
                        info!("Block {} is disqualified from virtual chain: {}", current, rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                        chain_disqualified_counter += 1;
                    } else {
                        debug!("VIRTUAL PROCESSOR, UTXO validated for {current}");

                        // Unit C, first-validation leg: RUN the transition. A failure here means
                        // block validation admitted something the state machine refuses — a
                        // rule/state divergence, which no local handling makes safe — so the block
                        // is disqualified rather than skipped, and `palw_state` is moved only on
                        // success.
                        let palw_v2_staged = match palw_state.as_mut() {
                            Some(state) => {
                                // One block's paid whole-root work survives the acceptance walk,
                                // nested rehearsals and final fold, despite other threads' work.
                                let _prompt_roots =
                                    kaspa_consensus_core::palw_attempt_rules_v1::PalwPromptRootBlockGuardV2::enter(current);
                                let state_params =
                                    self.palw_state_params_v2.as_ref().expect("palw_state is Some only when the params are");
                                // Unit C step 5: the header commits to the state this block's
                                // transition STARTS from — the parent's root — so it is checked
                                // BEFORE the transition runs, against the state this node's own
                                // walk reached. Without it the root is something each node
                                // computes privately and nobody can be held to.
                                //
                                // A mismatch disqualifies the block rather than the node: the
                                // producer built on a state this chain does not have, and that is
                                // a fact about the block.
                                let parent_root = self.palw_committed_root_v1(state);
                                if header.palw_state_root != parent_root {
                                    info!(
                                        "Block {} is disqualified from virtual chain (PALW state root): committed {}, this chain is at {}",
                                        current, header.palw_state_root, parent_root
                                    );
                                    self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                                    chain_disqualified_counter += 1;
                                    continue;
                                }
                                let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                                    block: current,
                                    daa_score: header.daa_score,
                                    blue_score: header.blue_score,
                                    // The pool this block's claims escrow from (ADR-0042
                                    // Decision 10), taken from the ONE emission schedule rather
                                    // than a second copy of it.
                                    subsidy: self.coinbase_manager.calc_block_subsidy(header.daa_score),
                                };
                                // Unit C step 3: the block's own objects, in its own acceptance
                                // order. Read from the acceptance data this validation just
                                // produced rather than from the store — the store row is written
                                // by the commit below, so reading it here would be reading a fact
                                // that does not exist yet.
                                let objects = self.palw_v2_objects_of_block(
                                    &ctx.mergeset_acceptance_data,
                                    state,
                                    current,
                                    header.daa_score,
                                    // ADR-0075 SA-1/SA-2: what each 0x4b carrier paid, collected
                                    // by the UTXO walk two frames up, where the fee is knowable.
                                    &ctx.palw_v2_rent_available_fees,
                                );
                                let history_custody_pledges = objects
                                    .iter()
                                    .filter_map(|carried| carried.history_pledge.clone())
                                    .map(|pledge| (pledge.claim, pledge))
                                    .collect();
                                let public_original_bindings = objects
                                    .iter()
                                    .filter_map(|carried| {
                                        carried
                                            .history_pledge
                                            .as_ref()
                                            .zip(carried.original_binding.as_ref())
                                            .map(|(pledge, header)| (pledge.claim, header.clone()))
                                    })
                                    .collect();
                                let producer_custody_delegations = objects
                                    .iter()
                                    .filter_map(|carried| {
                                        carried
                                            .history_pledge
                                            .as_ref()
                                            .zip(carried.custody_delegation.as_ref())
                                            .map(|(pledge, delegation)| (pledge.claim, delegation.clone()))
                                    })
                                    .collect();
                                // Decisions 7/8: every lifecycle object meets its own validator
                                // before the transition folds it — and an object that FAILS is
                                // dropped rather than fatal to the block that accepted it.
                                //
                                // **Audit M-01's other half.** This used to disqualify the block.
                                // Admission on the 0x4b band is purely stateless (decode, version,
                                // may-ride), so a transaction carrying a stateful lie — a bad
                                // signature, a claim that does not exist — relays and mines freely;
                                // the first honest block to accept it lost its candidacy, and the
                                // transaction stayed in the acceptance set for the next candidate
                                // to die on. One ~100-byte transaction, one ordinary fee, and the
                                // chain stops. The module beside this one states the rule already:
                                // "a walk that could panic or reject on a peer-supplied payload
                                // would be a remote denial of service wearing a consensus rule's
                                // clothes" — it just stopped at the malformed carrier and left
                                // every stateful check fatal.
                                //
                                // Dropping is deterministic: the verdict is a pure function of
                                // (state, params, point, object), so every node drops the same
                                // ones. A dropped object simply does not fold, which is what "the
                                // transaction was invalid" ought to mean.
                                // The class of this block's own attempt, read off its header, for the
                                // rehearsal's reservation (2026-09-24 DoS audit review of #11). An
                                // undecodable commitment reserves nothing — admission refuses it below.
                                let own_attempt_class = Self::palw_own_attempt_class_of_header_v1(&header);
                                //
                                // **Except that a dropped market move's carrier stays accepted with
                                // its MSK in the sink** (the 2026-09-23 Position route matrix,
                                // P-B1), so past `palw_audit_2026_09_23` the filter also names what
                                // each such carrier is owed and the transition pays it back.
                                let (objects, folded, carrier_refunds, tir_public_originals, tir_funded_originals) = self.palw_v2_accepted_objects_and_originals(
                                    state,
                                    state_params,
                                    &point,
                                    objects,
                                    current,
                                    own_attempt_class,
                                    &ctx.palw_round_fee_incomes,
                                    &ctx.palw_escrow_settlements,
                                )?;
                                // Unit C step 4: a receipt-lane block spends a quantum, and its
                                // right to do so is a DRAW — so the beacon it draws against is
                                // derived from this candidate's own chain, never read off the
                                // spending block. A block that supplied its own beacon would be
                                // choosing the randomness that decides whether it wins.
                                // **ADR-0064, and it is off unless a fence says otherwise.** With
                                // `palw_bootstrap_activation` unset — every shipped preset — this
                                // is `None` and the behaviour is byte-identical to before the
                                // field existed. Armed, the block's own attempt may name a bond
                                // this block's own accepted objects register — one chain block
                                // earlier than before, for a producer joining a LIVE chain. It
                                // does NOT restart a stopped one: this block's own body is not in
                                // this set (a block's transactions are accepted by a later block),
                                // so the registration still has to arrive in somebody else's
                                // block. See ADR-0064's correction.
                                let bootstrap =
                                    self.palw_bootstrap_activation.filter(|fence| fence.is_active(point.daa_score)).map(|_| &*folded);
                                let attempt =
                                    match self.palw_v2_check_attempt_admission(&header, state, state_params, &point, bootstrap) {
                                        Ok(attempt) => attempt,
                                        // Lane bind-deadlock (`palw_anchor_at_ceiling`): an anchor whose
                                        // bond is at its ceiling still binds the claims due at it.
                                        Err(adm_error) => match self.palw_v2_anchor_at_ceiling_binder_v1(
                                            &header,
                                            state,
                                            state_params,
                                            &point,
                                            bootstrap,
                                        ) {
                                            Some(envelope) => {
                                                info!(
                                                    "Block {} anchors at its bond's exposure ceiling ({}): it binds the claims due at \
                                                     it and its own attempt carries no claim (palw_anchor_at_ceiling)",
                                                    current, adm_error
                                                );
                                                Some(envelope)
                                            }
                                            None => {
                                                info!(
                                                    "Block {} is disqualified from virtual chain (PALW admission): {}",
                                                    current, adm_error
                                                );
                                                self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                                                chain_disqualified_counter += 1;
                                                continue;
                                            }
                                        },
                                    };
                                let receipt_spend = match self.palw_v2_check_receipt_spend(&header, state, state_params, &point) {
                                    Ok(spend) => spend,
                                    Err(fp_error) => {
                                        info!(
                                            "Block {} is disqualified from virtual chain (PALW receipt spend): {}",
                                            current, fp_error
                                        );
                                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                                        chain_disqualified_counter += 1;
                                        continue;
                                    }
                                };
                                // **The block's own work, carried into the fold.**
                                //
                                // This was `None`, unconditionally, and it is the reason a V2
                                // network could not mint weight: the two checks above admitted an
                                // attempt or a spend and then threw the object away, so the
                                // transition folded a block that — as far as the state machine
                                // could tell — had done no work at all. `apply_attempt` never ran,
                                // so no claim was ever created; `apply_receipt_spend` never ran,
                                // so no certified quantum was ever burned and `safe_weight` never
                                // moved off zero. Every downstream mechanism (panel binding,
                                // licensing, courts, payouts, the safe frontier, fork choice)
                                // reads claims, and there were none.
                                //
                                // At most one arm can be `Some`: the lane is chosen by the
                                // header's single declared `pow_algo_id`, and the two constants
                                // differ. The match is written total anyway rather than asserted,
                                // because a panic here would be a remote one.
                                let work = match (attempt.as_ref(), receipt_spend.as_ref()) {
                                    (Some(envelope), _) => kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::Attempt(envelope),
                                    (None, Some(spend)) => {
                                        kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::ReceiptSpend(&spend.spend)
                                    }
                                    (None, None) => kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::None,
                                };
                                // ADR-0058: the anticone's work rides this block's transition.
                                // Assembled against the walk state (the parent's); the transition
                                // re-runs the stateful admission per work against its LIVE fold
                                // state, so order inside the block cannot over-commit a budget.
                                let merged_non_daa = {
                                    use crate::model::stores::daa::DaaStoreReader;
                                    self.palw_exec_v2_non_daa(
                                        &self.daa_excluded_store.get_mergeset_non_daa(current).unwrap_or_default(),
                                        &ctx.ghostdag_data,
                                        pov_daa_score,
                                    )
                                };
                                let (merged_owned, merged_preskips) =
                                    self.palw_v2_merged_works(&ctx.ghostdag_data, state, state_params, &merged_non_daa, &point);
                                let merged_refs: Vec<kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1<'_>> = merged_owned
                                    .iter()
                                    .map(|owned| match owned {
                                        PalwMergedOwnedWorkV1::Attempt(blue, envelope, subsidy, carve, bits, origin_header) => self
                                            .palw_merged_attempt_work_v1(
                                                *blue,
                                                Some(origin_header.as_ref()),
                                                envelope,
                                                *subsidy,
                                                *carve,
                                                *bits,
                                            ),
                                        PalwMergedOwnedWorkV1::Spend(blue, envelope) => {
                                            kaspa_consensus_core::palw_state_v2::PalwMergedWorkV1 {
                                                carrying_block: *blue,
                                                work: kaspa_consensus_core::palw_state_v2::PalwBlockWorkV3::ReceiptSpend(
                                                    &envelope.spend,
                                                ),
                                                // A receipt spend is the free-prompt lane (B-5 dedups it on (claim,
                                                // quantum)); B-4's attempt-execution dedup does not apply, so the key is unread.
                                                execution_key: kaspa_hashes::Hash64::default(),
                                                // A receipt spend escrows nothing (it is not an attempt claim), so the
                                                // subsidy and the carve are unread.
                                                subsidy: 0,
                                                escrow_carve: None,
                                                bits: 0,
                                                // Not an attempt: it answers no anchor.
                                                job_anchor: kaspa_hashes::Hash64::default(),
                                            }
                                        }
                                    })
                                    .collect();
                                // B-4: this block's OWN attempt's execution key, from its own header
                                // (pre_pow + nonce), and (ADR-0152 v3.1 J-1) its execution anchor —
                                // the job identity the claim records. Both `default` when the block
                                // carries no attempt.
                                let (own_execution_key, own_job_anchor) = match attempt.as_ref() {
                                    Some(envelope) => self.palw_attempt_keys_v1(Some(header.as_ref()), &envelope.attempt),
                                    None => (kaspa_hashes::Hash64::default(), kaspa_hashes::Hash64::default()),
                                };
                                match kaspa_consensus_core::palw_state_v2::apply_palw_transition_retained_v7(
                                    state,
                                    state_params,
                                    self.palw_admission_params_v2.as_ref(),
                                    &point,
                                    &objects,
                                    work,
                                    &merged_refs,
                                    own_execution_key,
                                    self.palw_unavailable_abstains_at(point.daa_score),
                                    self.palw_capability_bound_at(point.daa_score),
                                    // ADR-0069 Decision 7, at this BLOCK's DAA. `false` on every
                                    // shipped preset, where the fold is byte-identical.
                                    self.palw_uncertified_weightless_at(point.daa_score),
                                    self.palw_da_court_at(point.daa_score),
                                    // ADR-0088 Decision 11 and ADR-0089 Decision 6, at this BLOCK's
                                    // DAA: the fences, and the actions this block's EVM step queued.
                                    &{
                                        let mut extras = self.palw_transition_extras_for_objects(state, &point, &objects).into_legacy_extras();
                                        extras.history_custody_pledges = history_custody_pledges;
                                        extras.tir_public_originals = tir_public_originals;
                                        extras.tir_funded_originals = tir_funded_originals;
                                        extras.public_original_bindings = public_original_bindings;
                                        extras.producer_custody_delegations = producer_custody_delegations;
                                        if state_params.exec_v2_active_at(point.daa_score)
                                            || state_params.monetary_material_at_v1(point.daa_score).is_some()
                                        {
                                            if attempt.is_some()
                                                && let Ok(bytes) = borsh::to_vec(header.as_ref())
                                            {
                                                extras.exec_v2_origin_headers.insert(current, bytes);
                                            }
                                            for owned in &merged_owned {
                                                if let PalwMergedOwnedWorkV1::Attempt(source, _, _, _, _, origin_header) = owned
                                                    && let Ok(bytes) = borsh::to_vec(origin_header.as_ref())
                                                {
                                                    extras.exec_v2_origin_headers.insert(*source, bytes);
                                                }
                                            }
                                        }
                                        if let Some(staged) = evm_staged.as_ref() {
                                            extras.evm_actions = staged.result.market_actions.clone();
                                        }
                                        // P-B1: what the filter above refused and owes back. Empty
                                        // below `palw_audit_2026_09_23`, and the fold checks again.
                                        extras.carrier_market_refunds = carrier_refunds;
                                        // ADR-0125: the permits this block accepted, as decided above.
                                        if let Some(verdicts) = ctx.palw_round_verdicts.as_ref() {
                                            extras.round_permit_uses = verdicts.uses.clone();
                                        }
                                        extras.round_fee_incomes = ctx.palw_round_fee_incomes.clone();
                                        // G14 lane escrow-rfc08: the qualified escrow spends UTXO acceptance admitted, settled by the fold
                                        // in this block's rooted delta (committed with the UTXO diff in one WriteBatch).
                                        extras.funded_escrow_settlements = ctx.palw_escrow_settlements.clone();
                                        // RFC-0008 v2: and the EXEC blocks this block's anchor covered — the slice carriers (each
                                        // re-judged by the fold against its own running state) and the covered set itself.
                                        if let Some(resolved) = exec_resolved.as_ref() {
                                            let (anchor, slices) = self.palw_exec_v2_fold_inputs(resolved, point.daa_score);
                                            extras.exec_v2_anchor = Some(anchor);
                                            extras.exec_v2_covered = slices;
                                        }
                                        // ADR-0152 v3.1 J-1: the anchor this block's own attempt answers.
                                        extras.own_job_anchor = own_job_anchor;
                                        // **ADR-0165 / ADR-0125 semantic amendment: the genuine reds of this block's mergeset** — the floor state machine reads a merged
                                        // attempt's colour from them (a RED REAL attempt never extends a Probe or Normal).
                                        extras.merged_reds = ctx
                                            .ghostdag_data
                                            .classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member))
                                            .genuine_reds()
                                            .iter()
                                            .copied()
                                            .collect();
                                        // **Lane PA (`palw_audit_1004_v1`, RF-3 / P-F5): the beacon** a draw reads in place of this block's own hash.
                                        extras.audit_1004_draw_seed_source =
                                            self.palw_audit_1004_draw_seed_source(ctx.ghostdag_data.selected_parent, point.daa_score);
                                        extras
                                    },
                                ) {
                                    Ok((next, delta, merged_skips)) => {
                                        // A skipped merged work is the block standing while a
                                        // piece of its anticone is refused — say which and why,
                                        // because a silent skip of real work is the defect class
                                        // ADR-0058 exists to close.
                                        for (blue, why) in merged_preskips.iter().chain(merged_skips.iter()) {
                                            info!(
                                                "PALW: merged blue {blue} carried work this chain point refused (the accepting block stands): {why}"
                                            );
                                        }
                                        // `palw_model_court_window`: a class registered past the fence commits its own court
                                        // window — say which, once per registration, so an operator (and the flag-day drill)
                                        // can read what every node derived. Empty below the fence.
                                        for entry in &delta.entries {
                                            if let kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::ClassCourtWindow {
                                                key,
                                                old: None,
                                                new: Some(window),
                                            } = entry
                                            {
                                                info!(
                                                    "PALW model court window: class {key} registered at DAA {} commits a {window}-DAA court window (the network window is {})",
                                                    point.daa_score,
                                                    state_params.window_court()
                                                );
                                            }
                                        }
                                        // **ADR-0165: the floor state moved** — one `[palw-floor-state]` line per chain block whose
                                        // fold changed it (idle/probe/normal, with the DAA and the numbers it is made of), so the
                                        // drill and an operator read the machine without a debugger. Empty below the fence.
                                        if let Some((from, to)) =
                                            kaspa_consensus_core::palw_real_share_v1::palw_floor_state_change_v1(&delta.entries)
                                        {
                                            info!(
                                                "{}",
                                                kaspa_consensus_core::palw_real_share_v1::palw_floor_state_log_line_v1(
                                                    &current,
                                                    point.daa_score,
                                                    &from,
                                                    &to
                                                )
                                            );
                                        }
                                        // The construction ticket moves with the state; no
                                        // post-construction reservation can authorize its clone.
                                        *state = next;
                                        // Launch blockers §8: say it out loud. A voided claim's
                                        // escrow is burned by don't-mint (Decision 10), and a
                                        // network whose panels never bind burns the whole worker
                                        // carve of every block while looking exactly like one that
                                        // pays it.
                                        let destroyed = kaspa_consensus_core::palw_state_v2::palw_escrow_destroyed_by_delta_v2(&delta);
                                        if destroyed > 0 {
                                            warn!(
                                                "PALW: block {current} voided claims holding {destroyed} sompi of escrowed worker reward; that value is destroyed, not paid (ADR-0042 Decision 10)"
                                            );
                                        }
                                        let (delta, delta_lease) = delta.into_parts_v1();
                                        Some((state.state_root(), delta, delta_lease))
                                    }
                                    Err(kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeResource(why)) => {
                                        return Err(why);
                                    }
                                    Err(palw_error) => {
                                        info!("Block {} is disqualified from virtual chain (PALW state): {}", current, palw_error);
                                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                                        chain_disqualified_counter += 1;
                                        continue;
                                    }
                                }
                            }
                            None => None,
                        };
                        // Everything that can disqualify has run. Only now does the walk MOVE —
                        // `diff` and `diff_point` are advanced after the last refusal, not before
                        // it. Advancing first was a real bug: a block disqualified afterwards left
                        // `diff_point` naming a block whose UTXO data was never committed, and the
                        // next `utxo_multisets_store.get(new_sink)` panicked on the missing row.
                        // The order is the invariant, not the comment.
                        diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        diff_point = current;
                        if track_bonds {
                            // Advance the bond view by THIS block's mutations,
                            // derived from the in-memory acceptance data (its
                            // store entry is written by the commit just below).
                            let bond_muts = self.dns_bond_mutations_from_acceptance(
                                current,
                                &ctx.mergeset_acceptance_data,
                                bond_view,
                                pov_daa_score,
                            );
                            bond_view.apply(&bond_muts);
                        }
                        // Commit UTXO data for current chain block
                        self.commit_utxo_state(
                            current,
                            ctx.mergeset_diff,
                            ctx.multiset_hash,
                            ctx.mergeset_acceptance_data,
                            ctx.pruning_sample_from_pov.expect("verified"),
                            ctx.validator_rewarded_keys,
                            ctx.validator_quality_subpool,
                            ctx.reserve_balance_after,
                            evm_staged,
                            palw_v2_staged.map(|(root, delta, delta_lease)| PalwUtxoCommitV2 {
                                root,
                                delta,
                                _native_delta_lease: delta_lease,
                                genesis: None,
                                _native_genesis_carriages_lease: None,
                                genesis_carriages: Vec::new(),
                            }),
                        )?;
                        // Count the number of UTXO-processed chain blocks
                        chain_block_counter += 1;
                    }
                }
                Err(err) => panic!("unexpected error {err}"),
            }
        }
        // ADR-0042 Unit C: the walked state is the state AT `diff_point`, which is exactly what
        // the tip row means — so it is written where the walk ends rather than at each block. A
        // per-block tip write would be the same value re-derived N times, and the delta rows are
        // already the per-block record; the tip is the materialization the next walk resumes from.
        //
        // Written in its own batch, AFTER every per-block commit above: if this write is lost the
        // deltas are still on disk and the next `load_tip` resumes from the older tip and walks
        // forward over them, which reproduces exactly this state. Losing a delta would not be
        // recoverable, which is why those ride the block's own batch and this does not.
        //
        // It is deliberately NOT folded into the last block's batch: the row carries the whole
        // registry as a carriage, so a per-block tip write would re-serialize the entire state on
        // every chain block. The crash window that ordering opens — this write landing while the
        // virtual state commit does not — is closed at the READ above, which derives the state at
        // the walk's own starting point instead of trusting the tip to stand there.
        if let Some(state) = palw_state.as_ref() {
            let mut batch = WriteBatch::default();
            let mut store = self.palw_state_v2_store.write();
            let _native_tip_write = store.set_tip_batch_tracked_v1(&mut batch, diff_point, state).map_err(native_store_capacity_v1)?;
            drop(store);
            self.db.write(batch).unwrap();
        }

        // Report counters
        self.counters.chain_block_counts.fetch_add(chain_block_counter, Ordering::Relaxed);
        if chain_disqualified_counter > 0 {
            self.counters.chain_disqualified_counts.fetch_add(chain_disqualified_counter, Ordering::Relaxed);
        }

        Ok(diff_point)
    }

    /// C-01 (slice S6/S9/S9b) — compute the flat/reconstruct PARENT seed for
    /// `selected_parent` and validate it against the committed state before the
    /// executor uses it. The snapshot is materialized from the flat store when
    /// `selected_parent` is the canonical head, else §12-reconstructed (root-verified).
    ///
    /// Validation has two equivalent modes, chosen by whether the 206 snapshot is
    /// PRESENT (it is until slice S9b's `--evm-retire-206` stops persisting it):
    ///   - **206 present** (S6/S9): assert the flat/reconstruct seed is BYTE-IDENTICAL
    ///     to 206. This is belt-and-suspenders on top of the S4 write-side check.
    ///   - **206 absent** (S9b retired, or a parent committed while retired): there is
    ///     nothing to byte-compare against, so anchor to the consensus-committed root —
    ///     a FlatHead seed's flat pointer `state_root` must equal `parent_header.state_root`;
    ///     a Reconstructed seed is ALREADY keccak-MPT root-verified against it inside
    ///     `flat_or_reconstruct_parent_snapshot`. Either way the flat CONTENTS were
    ///     already proven == the executor's in-memory post-state when the parent was
    ///     committed (the S4 `shadow_dual_write_flat` differential, which never read 206),
    ///     so the per-block oracle is intact — retiring 206 drops only the redundant copy.
    ///
    /// HALTS the node (design §7) on a DEFINITIVE divergence — the seed differs from a
    /// present 206, a flat-head pointer root disagrees with the committed parent root, or
    /// a §12 reconstruction is corrupt — because feeding the executor a wrong parent state
    /// would falsely disqualify valid blocks. It NEVER returns an unvalidated seed and
    /// NEVER disqualifies.
    ///
    /// Returns `Some((parent_header, snapshot))` for a validated EVM-active parent seed.
    /// Returns `None` when the parent is pre-activation (no EVM header ⇒ the executor's
    /// own store path yields the empty genesis parent) OR the seed is Unavailable
    /// (transient store I/O, or a non-head parent's §12 history GC'd past retention).
    /// In retire-206 mode the caller turns a `None` for an EVM-ACTIVE parent into a HALT
    /// (no 206 fallback); otherwise it falls back to the 206 store path. Node-local; only
    /// meaningful when the shadow backend is on.
    #[cfg(feature = "evm")]
    fn validated_flat_parent_seed(
        &self,
        selected_parent: BlockHash,
    ) -> Option<(kaspa_consensus_core::evm::EvmExecutionHeader, kaspa_consensus_core::evm::EvmStateSnapshot)> {
        use crate::model::stores::evm::{EvmHeaderStoreReader, EvmStateStoreReader};
        use crate::processes::evm::{ParentSeedError, ParentSeedSource, flat_or_reconstruct_parent_snapshot};

        // An EVM-active parent always persists its header; a parent with no EVM header is
        // pre-activation (empty genesis state) — nothing to validate, and the executor's
        // store path supplies the empty parent, so return None.
        let parent_header = match self.evm_header_store.get(selected_parent) {
            Ok(h) => h,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => return None,
            Err(e) => {
                warn!("[evm-shadow-seed] header read failed for {selected_parent}: {e}; falling back to 206");
                return None;
            }
        };
        // The 206 snapshot — the byte-compare oracle WHEN PRESENT. `KeyNotFound` is not an
        // error here: it means 206 was retired (S9b) or this parent was committed while
        // retired. We then validate the seed against the committed root instead (below).
        let snapshot_206 = match self.evm_state_store.get(selected_parent) {
            Ok(s) => Some(s),
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => None,
            Err(e) => {
                warn!("[evm-shadow-seed] 206 read failed for {selected_parent}: {e}; falling back to 206");
                return None;
            }
        };
        // Surface a flat-pointer read failure as a fallback — never silently treat it
        // as "no head" (None), which would misroute the canonical head into the
        // reconstruct path and hide the store error. Carry the pointer's committed
        // `state_root` for the 206-absent FlatHead anchor check.
        let (flat_head, flat_head_root) = match self.evm_latest_state_ptr_store.read().get() {
            Ok(opt) => (opt.map(|p| p.canonical_head), opt.map(|p| p.state_root)),
            Err(e) => {
                warn!("[evm-shadow-seed] flat pointer read failed for {selected_parent}: {e}; falling back to 206");
                return None;
            }
        };

        match flat_or_reconstruct_parent_snapshot(
            selected_parent,
            flat_head,
            &self.evm_flat_account_store,
            &self.evm_code_store,
            &self.evm_header_store,
            &self.evm_state_checkpoint_store,
            &self.evm_state_diff_store,
            // Pre-activation is judged by the L1 DAA score, never by EVM-row
            // presence (pruning erases rows; see gather_reconstruction_inputs).
            |b| self.headers_store.get_compact_header_data(b).map(|c| c.daa_score < self.evm_activation_daa_score),
        ) {
            Ok((snapshot_flat, source)) => {
                match &snapshot_206 {
                    // 206 present (S6/S9): the seed must be byte-identical to it.
                    Some(s206) => {
                        if &snapshot_flat != s206 {
                            panic!(
                                "C-01 shadow seed DIVERGENCE: the {source:?} parent seed for {selected_parent} ({} accounts) does not match \
                                 the committed 206 snapshot ({} accounts). The flat/reconstruct seed source would feed the executor a wrong parent \
                                 state and FALSELY disqualify valid blocks — HALTING this node. 206 stays authoritative (chain integrity intact); \
                                 fix the backend and re-shadow.",
                                snapshot_flat.accounts.len(),
                                s206.accounts.len()
                            );
                        }
                    }
                    // 206 absent (S9b retired): anchor to the consensus-committed root. A
                    // Reconstructed seed is already root-verified inside the helper; a FlatHead
                    // seed's pointer root must equal the committed parent root (guards a stale/
                    // wrong pointer — the flat CONTENTS were already proven == the executor's
                    // post-state at the parent's commit by the S4 write-side differential).
                    None => {
                        if source == ParentSeedSource::FlatHead && flat_head_root != Some(parent_header.state_root) {
                            panic!(
                                "C-01 S9b retired-206 seed DIVERGENCE: the flat head pointer root ({flat_head_root:?}) for {selected_parent} \
                                 does not equal the committed parent state_root ({:?}). The flat pointer is stale/wrong and would seed the \
                                 executor from the wrong head — HALTING this node (chain integrity intact); restore the flat backend.",
                                parent_header.state_root
                            );
                        }
                    }
                }
                Some((parent_header, snapshot_flat))
            }
            // Could not READ the data to validate (transient store I/O, or a non-head
            // parent's §12 history GC'd past retention): NOT a divergence — the caller
            // falls back to 206 (S9) or HALTs for an EVM-active parent (S9b retired).
            Err(ParentSeedError::Unavailable(m)) => {
                debug!("[evm-shadow-seed] seed unavailable for {selected_parent}: {m}; falling back to 206");
                None
            }
            // A broken §12 reconstruction (root mismatch / diff inconsistency / bad
            // checkpoint / absent code) is a real backend fault ⇒ HALT.
            Err(ParentSeedError::Corrupt(m)) => {
                panic!(
                    "C-01 shadow seed CORRUPT for {selected_parent}: {m}. The flat/reconstruct backend is broken — HALTING (206 stays authoritative)."
                );
            }
        }
    }

    /// C-01 (slice S6) post-execution shadow check: validate the flat/reconstruct seed
    /// source against 206 (HALT on divergence), discarding the seed. Used when the
    /// executor was seeded from 206 (every block while the flat-authoritative cutover
    /// is off) — 206 stays authoritative, so this can only HALT on a backend divergence,
    /// never disqualify a valid block.
    #[cfg(feature = "evm")]
    fn shadow_validate_parent_seed(&self, selected_parent: BlockHash) {
        let _ = self.validated_flat_parent_seed(selected_parent);
    }

    /// O12: spawn the EVM pipeline worker for the upcoming forward walk when it
    /// contains a long run of pending EVM-active chain blocks (IBD catch-up).
    /// Steady-state walks (a handful of blocks) skip the pipeline — the thread
    /// + channel overhead outweighs overlapping a single block.
    #[cfg(feature = "evm")]
    fn maybe_spawn_evm_pipeline(&self, split_point: BlockHash, to: BlockHash) -> Option<crate::processes::evm::EvmPipeline> {
        use crate::processes::evm::{EvmPipeline, EvmPipelineItem};
        const MIN_PIPELINE_RUN: usize = 8;
        if self.evm_activation_daa_score == u64::MAX {
            return None;
        }
        // C-01 S9b: the pipeline worker seeds a run's FIRST/gap item from the 206 store (its other
        // items chain in-memory). With 206 retired there is no such seed, so disable the pipeline
        // and let the inline path (which seeds every block from the validated flat store) handle the
        // run. Pure perf/throughput trade — correctness is identical either way (I-3 invariant).
        if self.evm_retire_206 {
            return None;
        }
        let statuses = self.statuses_store.read();
        let mut pending: Vec<EvmPipelineItem> = Vec::new();
        let mut prev_pending: Option<BlockHash> = None;
        for (selected_parent, current) in self.reachability_service.forward_chain_iterator(split_point, to, true).tuple_windows() {
            // Mirror the walk's KeyNotFound arm: only blocks without a committed
            // UTXO diff and not already disqualified will be validated.
            if self.utxo_diffs_store.get(current).is_ok() {
                continue;
            }
            if statuses.get(current).unwrap() == StatusDisqualifiedFromChain {
                continue;
            }
            if self.headers_store.get_daa_score(current).unwrap() < self.evm_activation_daa_score {
                continue; // pre-activation block: the step is inert for it
            }
            let chain_from_prev = prev_pending == Some(selected_parent);
            pending.push(EvmPipelineItem { block: current, selected_parent, chain_from_prev });
            prev_pending = Some(current);
        }
        drop(statuses);
        if pending.len() < MIN_PIPELINE_RUN {
            return None;
        }
        Some(EvmPipeline::spawn(
            self.evm_header_store.clone(),
            self.evm_state_store.clone(),
            self.evm_payload_store.clone(),
            self.headers_store.clone(),
            self.ghostdag_store.clone(),
            pending,
            self.evm_gas_pool_v2_activation_daa_score,
            self.evm_f002_withdraw_cap_activation_daa_score,
            self.evm_bridge_ledger_activation_daa_score,
            self.evm_f003_mldsa_verify_activation_daa_score,
            self.evm_typed_receipt_root_activation_daa_score,
            self.palw_lane_accept_parents_first,
        ))
    }

    /// Non-`evm` builds never pipeline (the step itself is a panic-guard there).
    #[cfg(not(feature = "evm"))]
    fn maybe_spawn_evm_pipeline(&self, _split_point: BlockHash, _to: BlockHash) -> Option<crate::processes::evm::EvmPipeline> {
        None
    }

    /// kaspa-pq EVM Lane v0.4 (§10 / invariant I3): a virtual change only moves
    /// the canonical EVM head POINTERS — never executes. Pre-§16 (RPC) policy:
    /// `latest` = the new sink; `safe` tracks `latest`; `finalized` tracks the
    /// pruning point once it carries an EVM result (consensus-final), else the
    /// previous finalized. The blue-work-depth `safe` + DNS-confirmed-anchor
    /// `finalized` selection lands with the RPC phase that first exposes the
    /// tags. Inert (one u64 compare) on every current network.
    pub(super) fn update_evm_canonical_heads(&self, batch: &mut WriteBatch, sink: BlockHash) {
        use crate::model::stores::evm::{EvmCanonicalHeadsStoreReader, EvmHeaderStoreReader};
        if self.evm_activation_daa_score == u64::MAX {
            return;
        }
        if self.headers_store.get_daa_score(sink).is_ok_and(|daa| self.dns_retired_at(daa)) {
            let policy = self.palw_dns_retirement.expect("retirement is active").settlement.id();
            let previous_finalized = match self.evm_heads_store.read().native_snapshot() {
                Ok(Some(s))
                    if s.version != 1
                        || s.policy_id != policy
                        || s.ruleset_id != self.palw_native_ruleset_id
                        || s.retirement_daa != self.palw_dns_retirement.unwrap().activation.daa_score() =>
                {
                    error!("[native-settlement] incompatible persisted evidence; preserving it for resync");
                    return;
                }
                Ok(previous) => previous.and_then(|s| s.finalized),
                Err(StoreError::KeyNotFound(_)) => None,
                Err(e) => {
                    error!("[native-settlement] unreadable persisted evidence: {e}; preserving it for resync");
                    return;
                }
            };
            let mut publication = match self.native_finalized_publication() {
                Ok(s) => s,
                Err(()) => {
                    error!("[native-settlement] incompatible or unreadable finalized publication; preserving it for resync");
                    return;
                }
            };
            let snapshot = self.native_evm_settlement_snapshot(sink);
            if let Some(finalized) = snapshot.finalized {
                // Never replace the alarm anchor with an older publication. This cannot veto selection.
                let advances = publication
                    .as_ref()
                    .and_then(|s| s.finalized)
                    .is_none_or(|previous| self.reachability_service.try_is_chain_ancestor_of(previous, finalized).unwrap_or(false));
                if advances {
                    publication = Some(snapshot.clone());
                }
            }
            self.note_finalized_withdrawal(sink, previous_finalized, &snapshot);
            let heads = kaspa_consensus_core::evm::CanonicalEvmHeads {
                latest: snapshot.latest.unwrap_or_default(),
                safe: snapshot.safe.unwrap_or_default(),
                finalized: snapshot.finalized.unwrap_or_default(),
            };
            let mut store = self.evm_heads_store.write();
            store.set_native_finalized_publication_batch(batch, publication).unwrap();
            store.set_native_batch(batch, Some(snapshot)).unwrap();
            store.set_batch(batch, heads).unwrap();
            return;
        }
        // A legal rollback across the fence restores the legacy derivation. Reader gates hide native labels.
        // Preserve both native incident snapshot and publication history: re-entry must not erase a sticky
        // conflict, even if it returns to the originally published branch. Only validated resync clears them.
        // The sink carries an EVM result iff the lane is live for it (it may
        // predate activation right after the fork).
        if !self.evm_header_store.has(sink).unwrap_or(false) {
            return;
        }
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let prev_finalized = self.evm_heads_store.read().get().ok().map(|h| h.finalized);
        let finalized =
            if self.evm_header_store.has(pruning_point).unwrap_or(false) { pruning_point } else { prev_finalized.unwrap_or(sink) };
        // ADR-0109 Decision 4: `safe` is the DNS-confirmed anchor when it is in the sink's chain past
        // and carries an EVM result — the two-resource-confirmed prefix a reader asks for by tag —
        // and the sink otherwise (as before).
        let safe = self
            .dns_state_store
            .read()
            .get()
            .ok()
            .map(|state| state.last_dns_confirmed_anchor)
            .filter(|anchor| {
                *anchor != BlockHash::default()
                    && self.evm_header_store.has(*anchor).unwrap_or(false)
                    && self.reachability_service.is_chain_ancestor_of(*anchor, sink)
            })
            .unwrap_or(sink);
        let heads = kaspa_consensus_core::evm::CanonicalEvmHeads { latest: sink, safe, finalized };
        self.evm_heads_store.write().set_batch(batch, heads).unwrap();
    }

    /// kaspa-pq EVM Lane v0.4 (§16 RPC / canonical-index fix): drive the
    /// `evm_number → L1 hash` map from the CANONICAL selected chain. Detached
    /// chain blocks release their number (only if still theirs); attached chain
    /// blocks claim it. Companion to dropping the per-block write in
    /// `commit_utxo_state`: a sink-search loser (UTXO-validated by
    /// `calculate_utxo_state_relatively` but not selected) never touches the
    /// map, so `get_evm_block_by_number` / `get_evm_logs` can't be shadowed by a
    /// non-canonical row. Detach-before-attach mirrors `stage_dns_bond_mutations`
    /// (a number both removed and re-added in one reorg ends at the attached
    /// block: the batch applies the delete, then the put). Inert (one u64
    /// compare) on every current network.
    fn update_evm_canonical_number_map(&self, batch: &mut WriteBatch, chain_path: &ChainPath) {
        use crate::model::stores::evm::EvmHeaderStoreReader;
        if self.evm_activation_daa_score == u64::MAX {
            return;
        }
        // Detach first (most-recent first): release each removed chain block's
        // number iff the row still points to it.
        for removed in chain_path.removed.iter().rev().copied() {
            if let Some(h) = self.evm_header_store.get(removed).optional().unwrap() {
                self.evm_number_store.delete_if_matches_batch(batch, h.evm_number, removed).unwrap();
            }
        }
        // Attach: each added chain block claims its number (canonical-only write).
        for added in chain_path.added.iter().copied() {
            if let Some(h) = self.evm_header_store.get(added).optional().unwrap() {
                self.evm_number_store.write_batch(batch, h.evm_number, added).unwrap();
            }
        }
    }

    // Eleven arguments against a ceiling of ten. Every one is a distinct consensus input and
    // bundling them into a struct would move the coupling rather than remove it -- the call
    // sites would still have to get all eleven right, with one more indirection to read through.
    #[allow(clippy::too_many_arguments)]
    fn commit_utxo_state(
        &self,
        current: BlockHash,
        mergeset_diff: UtxoDiff,
        multiset: MuHash,
        acceptance_data: AcceptanceData,
        pruning_sample_from_pov: BlockHash,
        // kaspa-pq (ADR-0009 Addendum B §B.3(c)): the `(bond, epoch)` keys this
        // block rewarded. Persisted only when non-empty — empty on every block
        // of every current network (the overlay is dormant), so no rows are
        // written there.
        rewarded_keys: RewardedEpochKeys,
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): this block's validator quality
        // sub-pool, the per-epoch accumulator's recompute input. Non-zero (and
        // therefore persisted) only past `pos_v2_activation_daa_score` (`u64::MAX`
        // today), so no row is written on any current network.
        quality_subpool: u64,
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's cumulative reserve balance.
        // Persisted only when non-zero (the 0 default is never stored), so no row on any current
        // network. Children read it as their `parent_balance` for the reserve drip.
        reserve_balance: u64,
        // kaspa-pq EVM Lane v0.4 (§2.3): the validated EVM rows staged by
        // `evm_chain_context_step` — committed in THIS batch so the EVM result
        // and the block's UTXO diff are atomic. `None` on every current
        // network (lane inert) and on non-evm builds.
        evm_staged: Option<crate::processes::evm::EvmStaged>,
        // ADR-0042 Decision 5 / Unit C: this block's PALW V2 transition outcome — the state root
        // it produced and the delta that produces it — staged in THIS batch so the PALW state and
        // the block's UTXO data can never be half-written relative to each other. `None` on every
        // network whose mode is not `ConsensusV2`, which is every shipped preset.
        mut palw_v2_staged: Option<PalwUtxoCommitV2>,
    ) -> Result<(), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        // Carriage source/serialization storage stays reserved until the batch
        // commit even though the staged records move into their store writer.
        let _native_genesis_carriages = palw_v2_staged.as_mut().and_then(|staged| staged._native_genesis_carriages_lease.take());
        let mut batch = WriteBatch::default();
        // Pruning -> PALW matches pruning/import lock order. A long-running
        // candidate must recheck irreversible ancestry at its actual commit.
        let public_pruning_guard = palw_v2_staged
            .as_ref()
            .filter(|staged| {
                staged.delta.entries.iter().any(|entry| {
                    matches!(
                        entry,
                        kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::StagedCourtSeed { .. }
                            | kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::StagedCourtMove { .. }
                    )
                })
            })
            .map(|_| self.pruning_point_store.read());
        // Only a block on the irreversible chain itself archives public material: the pruning
        // point is in its selected chain, or it is one of the pruning point's chain ancestors.
        // A stale fork's block is never archived, whatever its DAA. The pruning clock read
        // under the same lock also freezes already-expired leases inside the store writer.
        let public_irreversible = public_pruning_guard.as_ref().map(|guard| {
            let pruning = guard.pruning_point().expect("native public archive pruning point");
            let pruning_daa = self.headers_store.get_daa_score(pruning).expect("native public archive pruning clock");
            let on_irreversible_chain = self.reachability_service.try_is_chain_ancestor_of(pruning, current).unwrap_or(false)
                || self.reachability_service.try_is_chain_ancestor_of(current, pruning).unwrap_or(false);
            (on_irreversible_chain, pruning_daa)
        });
        let archive_public_material = public_irreversible.is_some_and(|(on_chain, _)| on_chain);
        let public_irreversible_daa = public_irreversible.map_or(0, |(_, daa)| daa);
        // Native archive/lease GC must not race a prepared but uncommitted union.
        // Hold the same store exclusion through the atomic DB commit below.
        let mut palw_archive_guard = palw_v2_staged.as_ref().map(|_| self.palw_state_v2_store.write());
        // Lock order: pruning -> PALW -> class index (the ordinary and the import commits agree).
        let mut genesis_class_store = None;
        let mut native_delta_write = None;
        let mut native_genesis_write = None;
        if let Some(staged) = palw_v2_staged {
            let store = palw_archive_guard.as_mut().expect("PALW accepted archive guard");
            native_delta_write = Some(
                store
                    .insert_delta_batch_tracked_v1(&mut batch, current, staged.root, &staged.delta)
                    .map_err(native_store_capacity_v1)?,
            );
            if let Some(genesis) = staged.genesis {
                assert_eq!(current, self.genesis.hash, "only genesis can install the initial PALW tip");
                native_genesis_write =
                    Some(store.set_tip_batch_tracked_v1(&mut batch, current, &genesis).map_err(native_store_capacity_v1)?);
            }
            if !staged.genesis_carriages.is_empty() {
                assert_eq!(current, self.genesis.hash, "only genesis declarations use this initial commit");
                let mut class_store = self.palw_class_carriage_store.write();
                for (class_id, record) in staged.genesis_carriages {
                    class_store.insert_batch(&mut batch, class_id, record).unwrap();
                }
                genesis_class_store = Some(class_store);
            }
            let delta = &staged.delta;
            if archive_public_material
                && delta.entries.iter().any(|entry| match entry {
                    kaspa_consensus_core::palw_state_v2::PalwDeltaEntryV2::StagedCourtMove { old, .. } => {
                        old.public_prefix_checkpoint_v1().is_some()
                    }
                    _ => false,
                })
            {
                let mut movements = Vec::new();
                for row in acceptance_data.iter() {
                    let transactions = self.block_transactions_store.get(row.block_hash).unwrap();
                    for accepted in &row.accepted_transactions {
                        let transaction = transactions
                            .get(accepted.index_within_block as usize)
                            .filter(|tx| tx.id() == accepted.transaction_id)
                            .expect("accepted native transaction identity");
                        if transaction.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
                            continue;
                        }
                        if let Ok(payload) = borsh::from_slice::<
                            kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2,
                        >(&transaction.payload)
                            && let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::PublicCourtStreamMoveV1 { movement } =
                                payload.object
                        {
                            movements.push(*movement);
                        }
                    }
                }
                // Same commit batch as the rooted successful transitions. Public wire
                // bytes are retained under their native head, independent of body pruning.
                store.archive_prefix_movements_batch_v1(&mut batch, delta, &movements).unwrap();
            }
            if archive_public_material {
                store.archive_recovered_prefix_history_batch_v1(&mut batch, delta).unwrap();
                store.archive_staged_cache_leases_batch_v1(&mut batch, delta, public_irreversible_daa).unwrap();
            }
        }
        if let Some(mut staged) = evm_staged {
            // §12: in a mode that keeps no long-term EVM state history (`head`), drop
            // the archive diff so staging writes no diff/code/checkpoint rows
            // (220/221/222). The hot snapshot (206) + trace body (219) still cover its
            // reorg/trace window.
            if !self.evm_history_mode.writes_state_history() {
                staged.state_diff = None;
            }
            self.evm_header_store.insert_batch(&mut batch, current, staged.result.header.clone()).unwrap();
            // §16: receipts + tx-lookup index rows (store/RPC data only) commit
            // in the SAME batch — atomic with the result and the UTXO diff.
            crate::processes::evm::stage_evm_index_rows(
                &self.evm_receipts_store,
                &self.evm_tx_index_store,
                &self.evm_log_index_store,
                &self.evm_trace_store,
                &self.evm_state_diff_store,
                &self.evm_code_store,
                &self.evm_state_checkpoint_store,
                &mut batch,
                current,
                &staged,
            )
            .unwrap();
            // C-01 (slice S4) shadow dual-write + live differential, node-local,
            // OFF by default. Maintains the flat latest-state store (234/232/231)
            // in THIS batch and HALTS this node if applying the §12 diff to the
            // flat state disagrees with the committed post-state. The 206 snapshot
            // (written just below) stays the source of truth, so the committed
            // bytes are unchanged whether shadow is on or off (consensus-neutral).
            if self.evm_shadow_state_backend {
                use crate::model::stores::evm::{EvmHeaderStoreReader, EvmStateDiffStoreReader};
                // Chain readers for the S5 reorg re-base: a block's §12 diff (220)
                // and its sequential evm_number (from the EVM header, 201).
                let diff_store = &self.evm_state_diff_store;
                let header_store = &self.evm_header_store;
                let get_diff = |b: BlockHash| diff_store.get(b);
                let get_number = |b: BlockHash| match header_store.get(b) {
                    Ok(h) => Ok(Some(h.evm_number)),
                    Err(StoreError::KeyNotFound(_)) => Ok(None),
                    Err(e) => Err(e),
                };
                let mut ptr = self.evm_latest_state_ptr_store.write();
                match crate::processes::evm::shadow_dual_write_flat(
                    &self.evm_flat_account_store,
                    &self.evm_block_state_root_store,
                    &mut ptr,
                    &self.evm_code_store,
                    &mut batch,
                    current,
                    &staged,
                    get_diff,
                    get_number,
                ) {
                    Ok(crate::processes::evm::ShadowOutcome::Reseeded) => {
                        info!("[evm-shadow] flat state backend (re)seeded to block {current}");
                    }
                    Ok(crate::processes::evm::ShadowOutcome::Rebased) => {
                        info!("[evm-shadow] flat state backend re-based across a reorg to block {current}");
                    }
                    Ok(_) => {}
                    // A divergence (or store error) is fatal: never let a node that
                    // would serve a wrong flat-backend root keep running (design §7).
                    Err(e) => panic!("{e}"),
                }
            }
            // C-01 S9b: persist the per-block 206 snapshot UNLESS it is retired. The flat backend
            // (advanced + checked against `staged.snapshot` by the shadow dual-write just above) is
            // then the sole persisted post-state; the executor seeds from it (S9) and reads fall back
            // to flat-materialize / §12-reconstruct. `evm_retire_206` is only ever true together with
            // the shadow backend (the demotion in `new`), so the flat store IS maintained here before
            // the snapshot is dropped — the next block's seed reads a current flat head. Skipping the
            // write changes only what THIS node persists, never a commitment: consensus-neutral.
            if self.evm_retire_206 {
                drop(staged.snapshot);
            } else {
                self.evm_state_store.insert_batch(&mut batch, current, staged.snapshot).unwrap();
            }
            // §16 eth-rpc: map the 32-byte eth block id (first 32 bytes of the
            // 64-byte L1 hash — the truncation `eth_getTransactionReceipt`
            // already exposes as `blockHash`) → this L1 block, so
            // `eth_getBlockByHash` can reverse a client-held 32-byte hash. Upsert
            // (a given L1 block's first-32 is stable). RPC index only.
            let mut rpc_block_id = [0u8; 32];
            rpc_block_id.copy_from_slice(&current.as_bytes()[..32]);
            self.evm_block_hash_map_store.write_batch(&mut batch, kaspa_hashes::EvmH256::from_bytes(rpc_block_id), current).unwrap();
            // NOTE (canonical-index fix): the `evm_number → L1 hash` map is NOT
            // written here. It is the only EVM RPC row keyed by a value shared
            // across DAG side branches, so a UTXO-valid sink-search loser (a
            // candidate `calculate_utxo_state_relatively` validates here but the
            // DNS reorg gate / sink selection then rejects) would overwrite the
            // canonical row and make that number read as absent. It is instead
            // driven by the selected chain in `update_evm_canonical_number_map`
            // at virtual commit. The immutable rows above stay L1-hash-keyed, so
            // detached side branches remain queryable by hash.
        }
        self.utxo_diffs_store.insert_batch(&mut batch, current, Arc::new(mergeset_diff)).unwrap();
        self.utxo_multisets_store.insert_batch(&mut batch, current, multiset).unwrap();
        self.acceptance_data_store.insert_batch(&mut batch, current, Arc::new(acceptance_data)).unwrap();
        let rewarded_commit = if !rewarded_keys.is_empty() {
            Some(self.rewarded_epochs_store.insert_batch_tracked(&mut batch, current, Arc::new(rewarded_keys)).unwrap())
        } else {
            None
        };
        let quality_commit = if quality_subpool > 0 {
            Some(self.block_quality_pool_store.insert_batch_tracked(&mut batch, current, quality_subpool).unwrap())
        } else {
            None
        };
        if reserve_balance > 0 {
            self.reserve_balance_store.insert_batch(&mut batch, current, reserve_balance).unwrap();
        }
        // Note we call idempotent since this field can be populated during IBD with headers proof
        self.pruning_samples_store.insert_batch(&mut batch, current, pruning_sample_from_pov).idempotent().unwrap();
        let write_guard = self.statuses_store.set_batch(&mut batch, current, StatusUTXOValid).unwrap();
        self.db.write(batch).unwrap();
        drop(native_delta_write);
        drop(native_genesis_write);
        drop(genesis_class_store);
        if let Some(ticket) = rewarded_commit {
            ticket.committed();
        }
        if let Some(ticket) = quality_commit {
            ticket.committed();
        }
        drop(palw_archive_guard);
        drop(public_pruning_guard);
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
        Ok(())
    }

    fn calculate_and_commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        virtual_parents: Vec<BlockHash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_multiset: MuHash,
        accumulated_diff: &mut UtxoDiff,
        // kaspa-pq Phase 10/11 (ADR-0016 §D.4): the bond set as-of the virtual
        // selected parent, walked in lockstep with `accumulated_diff`. Forwarded
        // to `calculate_virtual_state`/`calculate_utxo_state` for the slashing
        // side-effect; inert until PR-16.4-b2 consumes it.
        selected_parent_bond_view: &ActiveBondView,
        chain_path: &ChainPath,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let new_virtual_state = self.calculate_virtual_state(
            &virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            selected_parent_multiset,
            accumulated_diff,
            selected_parent_bond_view,
        )?;
        self.commit_virtual_state(virtual_read, new_virtual_state.clone(), accumulated_diff, chain_path);
        Ok(new_virtual_state)
    }

    pub(super) fn calculate_virtual_state(
        &self,
        virtual_stores: &VirtualStores,
        virtual_parents: Vec<BlockHash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_multiset: MuHash,
        accumulated_diff: &mut UtxoDiff,
        // kaspa-pq Phase 10/11 (ADR-0016 §D.4): the bond set as-of the virtual
        // selected parent (= the new sink). Forwarded to `calculate_utxo_state`
        // for the slashing side-effect; inert until PR-16.4-b2 consumes it.
        selected_parent_bond_view: &ActiveBondView,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let selected_parent_utxo_view = (&virtual_stores.utxo_set).compose(&*accumulated_diff);

        // Calc virtual DAA score, difficulty bits and past median time
        let virtual_daa_window = self.window_manager.block_daa_window(&virtual_ghostdag_data)?;

        // **RFC-0008 v2: the anchor the block built on this virtual will carry.** Read from the store tip, which IS virtual's selected
        // parent (the same filter the lock and verdict reads below use). The covered EXEC blocks join virtual's reds, so virtual's UTXO
        // state, rewards and coinbase accept them exactly as the block that anchors them will; the DAG's own window and the stored
        // ghostdag data of every real block are untouched.
        let exec_state = if self.palw_exec_v2_active_at(virtual_daa_window.daa_score) {
            self.palw_state_params_v2
                .as_ref()
                .and_then(|params| self.palw_state_v2_store.read().load_tip_retained_v1(params).ok().flatten())
                .filter(|(block, _)| *block == virtual_ghostdag_data.selected_parent)
                .map(|(_, state)| state)
        } else {
            None
        };
        let (virtual_ghostdag_data, exec_anchor) = match exec_state
            .as_ref()
            .and_then(|state| self.palw_exec_v2_virtual(&virtual_ghostdag_data, state, virtual_daa_window.daa_score))
        {
            Some((augmented, anchor)) => (augmented, Some(anchor)),
            None => (virtual_ghostdag_data, None),
        };
        *self.palw_exec_v2_virtual_anchor.lock() = exec_anchor.map(|anchor| (virtual_ghostdag_data.selected_parent, anchor));
        let virtual_non_daa =
            self.palw_exec_v2_non_daa(&virtual_daa_window.mergeset_non_daa, &virtual_ghostdag_data, virtual_daa_window.daa_score);
        let mut ctx = UtxoProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_multiset);
        let virtual_bits = self.window_manager.calculate_difficulty_bits(&virtual_ghostdag_data, &virtual_daa_window);
        let virtual_past_median_time = self.window_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

        // Audit C-08: virtual acceptance runs the same bond-spend filter the chain walk runs, or
        // the template would build on a UTXO view that includes a spend every validating node
        // skips. Read from the store tip, which IS virtual's selected parent — and only when the
        // tip really names it, because a tip on another branch is not this candidate's registry.
        let virtual_palw_state = self
            .palw_state_params_v2
            .as_ref()
            .and_then(|params| self.palw_state_v2_store.read().load_tip_retained_v1(params).ok().flatten())
            .filter(|(block, _)| *block == virtual_ghostdag_data.selected_parent);
        ctx.palw_v2_locked_bonds = virtual_palw_state
            .as_ref()
            .map(|(_, state)| self.palw_v2_locked_bond_outpoints(state, virtual_daa_window.daa_score))
            .unwrap_or_default();
        // …including the bonds this mergeset itself declares, exactly as the candidate walk above
        // does. The two sides must compute the SAME set or the claim made there — "construction and
        // validation compute the same set" — is false in the direction that hurts: a template built
        // from the smaller set includes a spend every validating node refuses, so the block this
        // node mines is invalid on arrival and the work is thrown away.
        if virtual_palw_state.is_some() {
            ctx.palw_v2_locked_bonds.extend(self.palw_v2_bonds_declared_in_mergeset(&ctx));
        }
        ctx.palw_v2_bond_burns = virtual_palw_state
            .as_ref()
            .map(|(_, state)| self.palw_v2_bond_burn_obligations(state, virtual_daa_window.daa_score))
            .unwrap_or_default();
        // G14 lane escrow-rfc08: virtual locks the funded-escrow rows exactly as the chain walk does.
        ctx.palw_escrow_locks.rows = virtual_palw_state
            .as_ref()
            .map(|(_, state)| self.palw_escrow_locked_rows_v1(state, virtual_daa_window.daa_score))
            .unwrap_or_default();
        // ADR-0125: virtual accepts exactly the round blocks the block built on it will accept.
        ctx.palw_round_verdicts = virtual_palw_state
            .as_ref()
            .and_then(|(_, state)| self.palw_round_verdicts_v1(state, &virtual_ghostdag_data, virtual_daa_window.daa_score));

        // Calc virtual UTXO state relative to selected parent
        self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, selected_parent_bond_view, virtual_daa_window.daa_score);

        // Update the accumulated diff
        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

        // Build the new virtual state
        Ok(Arc::new(VirtualState::new(
            virtual_parents,
            virtual_daa_window.daa_score,
            virtual_bits,
            virtual_past_median_time,
            ctx.multiset_hash,
            ctx.mergeset_diff,
            ctx.accepted_tx_ids,
            ctx.mergeset_rewards,
            virtual_non_daa,
            virtual_ghostdag_data,
        )))
    }

    fn commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        new_virtual_state: Arc<VirtualState>,
        accumulated_diff: &UtxoDiff,
        chain_path: &ChainPath,
    ) {
        let mut batch = WriteBatch::default();
        let mut virtual_write = RwLockUpgradableReadGuard::upgrade(virtual_read);
        let mut selected_chain_write = self.selected_chain_store.write();

        // Apply the accumulated diff to the virtual UTXO set
        virtual_write.utxo_set.write_diff_batch(&mut batch, accumulated_diff).unwrap();
        // ADR-0109 Decision 1: the deposit-lock index follows the set, from the same diff, in the
        // same batch — so a template never sees a lock the set does not hold, or misses one it does.
        self.stage_evm_deposit_locks(&mut batch, accumulated_diff);

        // Update virtual state (capture the new sink first — `set_batch` moves the Arc).
        let dns_sink = new_virtual_state.ghostdag_data.selected_parent;
        virtual_write.state.set_batch(&mut batch, new_virtual_state).unwrap();

        // Update the virtual selected chain
        selected_chain_write.apply_changes(&mut batch, chain_path).unwrap();

        // kaspa-pq Phase 10 (ADR-0009 A.4): stage the DNS stake-bond set
        // changes into the same batch so they commit atomically with the
        // virtual state. Inert unless the overlay is configured.
        self.stage_dns_bond_mutations(&mut batch, chain_path);

        // kaspa-pq Phase 10 (ADR-0009 A.5): recompute the DNS StakeScore over
        // the bounded recent epoch window and stage the updated DnsState into
        // the same batch. Inert unless the overlay is configured.
        self.update_dns_state(&mut batch, dns_sink);

        // kaspa-pq EVM Lane v0.4 (§10 / invariant I3): a virtual change only
        // MOVES the canonical EVM head pointers — no execution happens here.
        self.update_evm_canonical_heads(&mut batch, dns_sink);

        // kaspa-pq EVM Lane v0.4 (§16 RPC / canonical-index fix): the canonical
        // `evm_number → L1 hash` map follows the selected chain (detach/attach),
        // not per-block result-commit — so a sink-search loser can't shadow it.
        self.update_evm_canonical_number_map(&mut batch, chain_path);

        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): recompute the per-epoch
        // accumulator over the bounded selected-chain window ending at the new
        // sink and stage it into the same batch. Inert below the v2 fence
        // (`pos_v2_activation_daa_score`, `u64::MAX` today) — returns after a
        // single header read on every current network.
        self.update_epoch_accumulator(&mut batch, dns_sink);

        // Served native publication history must survive loss of the page cache
        // together with its selected-chain/UTXO generation. Below retirement
        // preserve the historical asynchronous write policy.
        if self.headers_store.get_daa_score(dns_sink).is_ok_and(|daa| self.dns_retired_at(daa)) {
            self.write_pruning_import_barrier(batch);
        } else {
            self.db.write(batch).unwrap();
        }

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(virtual_write);
        drop(selected_chain_write);
    }

    /// kaspa-pq Phase 10 (ADR-0009 Addendum A.4): stage the `StakeBonds`-store
    /// mutations implied by this selected-chain change into `batch`, so they
    /// commit atomically with the virtual state. **Inert** unless the DNS
    /// overlay is configured (`dns_params.is_some()`) — on every current
    /// network this is a single `Option` check and a return.
    ///
    /// Mirrors the UTXO reorg model: blocks leaving the selected chain
    /// (`chain_path.removed`) are reverted, most-recent first, **before**
    /// blocks joining it (`chain_path.added`) are applied. Within a block,
    /// `Insert` reverts by delete and `Slash` by clearing `slashed_at`; a
    /// `Slash` revert whose bond record is already gone (its `Insert` was
    /// reverted in the same range) is skipped gracefully. Acceptance data is
    /// retained on reorg (only pruning deletes it), so removed blocks can be
    /// re-derived deterministically.
    fn stage_dns_bond_mutations(&self, batch: &mut WriteBatch, chain_path: &ChainPath) {
        if self.dns_params.is_none() {
            return;
        }
        // Derive every mutation FIRST, with no lock held, and only then take the write lock to
        // apply them.
        //
        // `dns_bond_mutations_for_chain_block` reads the bond store — `compute_challenge_
        // adjudication_slashes` needs the validator keys to check a challenge's evidence — and
        // `parking_lot::RwLock` is not reentrant, so deriving inside the write guard deadlocks the
        // thread against itself: one thread parked in `lock_shared_slow`, no thread holding
        // anything visible, the virtual processor stopped for good. That was unreachable while the
        // adjudication sat behind the VLT weight fence and became reachable the moment it moved to
        // the shadow fence, which is the first thing a private devnet crosses.
        //
        // Deriving up front also means every block in the path is derived against the bond set as
        // of the previous sink rather than against the partially-applied one. That is not a
        // behaviour change worth guarding: the adjudication judges a challenge at its
        // certificate's anchor, and a bond created inside this same chain path cannot be `Active`
        // at an anchor that old.
        // The evidence view for the SKIP filter. Immutable identity fields plus a strictly-older
        // status query, so any view holding the bond answers identically — see
        // `proved_slash_targets`. The store's current set is the cheapest such view, and it is
        // the same one on both the removed and added passes.
        // The evidence view for the SKIP filter, ADVANCED across the path. A single pre-path view
        // would judge block N's evidence against a bond set that predates every block of the
        // batch, so a bond created earlier in the same virtual advance would be unresolvable and
        // its genuine equivocation silently unslashed — the false-negative half of the same
        // IBD/live divergence the capability strict-prefix loop closes. Removed blocks are walked
        // newest-first against the same starting view: their mutations were derived under it when
        // they were applied, so re-deriving under it is what makes revert the exact inverse.
        let mut evidence_view = self.initial_active_bond_view();
        let removed_muts: Vec<Vec<BondMutation>> =
            chain_path.removed.iter().rev().copied().map(|h| self.dns_bond_mutations_for_chain_block(h, &evidence_view)).collect();
        let mut added_muts: Vec<Vec<BondMutation>> = Vec::with_capacity(chain_path.added.len());
        for h in chain_path.added.iter().copied() {
            let muts = self.dns_bond_mutations_for_chain_block(h, &evidence_view);
            evidence_view.apply(&muts);
            added_muts.push(muts);
        }

        let mut store = self.stake_bonds_store.write();

        // Revert blocks that left the selected chain (most-recent first). The stamp transitions go
        // through `revert_bond_stamp` — the SAME state machine `ActiveBondView::revert` uses, so
        // the persisted store and the in-memory per-block view cannot drift. It undoes a stamp only
        // when this mutation is the one that set it, which is what keeps an earlier slash that is
        // still in the chain prefix from being cleared by reverting a later, duplicate one.
        for muts in removed_muts {
            for mutation in muts.into_iter().rev() {
                match mutation {
                    BondMutation::Insert(outpoint, _) => {
                        store.delete_batch(batch, outpoint).unwrap();
                    }
                    BondMutation::Slash(outpoint, _) | BondMutation::Unbond(outpoint, _, _) => {
                        if let Ok(record) = store.get(&outpoint) {
                            let mut record = (*record).clone();
                            revert_bond_stamp(&mut record, &mutation);
                            store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                        }
                    }
                }
            }
        }

        // Apply blocks that joined the selected chain (in chain order).
        for muts in added_muts {
            for mutation in muts {
                match mutation {
                    BondMutation::Insert(outpoint, record) => {
                        store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                    }
                    BondMutation::Slash(outpoint, _) | BondMutation::Unbond(outpoint, _, _) => {
                        if let Ok(record) = store.get(&outpoint) {
                            let mut record = (*record).clone();
                            apply_bond_stamp(&mut record, &mutation);
                            store.insert_batch(batch, outpoint, Arc::new(record)).unwrap();
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn initial_active_bond_view(&self) -> ActiveBondView {
        if self.dns_params.is_none() {
            return ActiveBondView::new();
        }
        ActiveBondView::from_records(
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (rec.bond_outpoint, (*rec).clone()))),
        )
    }

    fn emit_palw_panel_notifications(&self) {
        let want = self.notification_root.has_subscription(EventType::PalwClassReadinessChanged)
            || self.notification_root.has_subscription(EventType::PalwPanelAssignment)
            || self.notification_root.has_subscription(EventType::PalwPanelReceipt)
            || self.notification_root.has_subscription(EventType::PalwPanelEligibilityChanged);
        if !want {
            return;
        }
        let Some(view) = self.palw_panel_network_view_v1_impl() else {
            return;
        };
        let mut snap = self.palw_panel_notify_snapshot.lock().unwrap_or_else(|e| e.into_inner());
        let (next, diff) = kaspa_consensus_core::palw_panel_view_v1::palw_panel_notify_diff_v1(snap.as_ref(), &view);
        *snap = Some(next);
        drop(snap);
        for row in diff.readiness {
            self.notification_root
                .notify(Notification::PalwClassReadinessChanged(PalwClassReadinessChangedNotification {
                    class_id: row.class_id.to_string(),
                    model_name: row.model_name,
                    registry_state: row.registry_state,
                    previous_registry_state: row.previous_registry_state,
                    ready_seats: row.ready_seats,
                    previous_ready_seats: row.previous_ready_seats,
                    required_ready_seats: row.required_ready_seats,
                    bonded_seats: row.bonded_seats,
                }))
                .expect("expecting an open unbounded channel");
        }
        for row in diff.assignments {
            self.notification_root
                .notify(Notification::PalwPanelAssignment(PalwPanelAssignmentNotification {
                    claim_id: row.claim_id.to_string(),
                    class_id: row.class_id.to_string(),
                    licensed_state: row.licensed_state.to_string(),
                    deadline_daa: row.deadline_daa,
                    coverage_mask: row.coverage_mask,
                    full_seat: kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(row.full_seat),
                    valid_receipt_seats: row.valid_receipt_seats,
                    selected_panel_seats: row.selected_panel_seats,
                    seats: row
                        .seats
                        .into_iter()
                        .map(|s| PalwPanelAssignmentSeatNotification {
                            seat_id: kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(s.seat_id),
                            seat_index: s.seat_index,
                            full_seat: s.full_seat,
                            segment_index: s.segment_index,
                            mask: s.mask,
                            receipt_status: s.receipt_status.to_string(),
                        })
                        .collect(),
                }))
                .expect("expecting an open unbounded channel");
        }
        for row in diff.receipts {
            self.notification_root
                .notify(Notification::PalwPanelReceipt(PalwPanelReceiptNotification {
                    claim_id: row.claim_id.to_string(),
                    class_id: row.class_id.to_string(),
                    coverage_mask: row.coverage_mask,
                    previous_coverage_mask: row.previous_coverage_mask,
                    valid_receipt_seats: row.valid_receipt_seats,
                    previous_valid_receipt_seats: row.previous_valid_receipt_seats,
                    selected_panel_seats: row.selected_panel_seats,
                }))
                .expect("expecting an open unbounded channel");
        }
        for row in diff.eligibility {
            let (hold_code, hold_message) = row.hold.map(|h| (h.code().to_string(), h.message().to_string())).unwrap_or_default();
            self.notification_root
                .notify(Notification::PalwPanelEligibilityChanged(PalwPanelEligibilityChangedNotification {
                    seat_id: kaspa_consensus_core::palw_panel_view_v1::palw_bond_seat_id_v1(row.seat_id),
                    class_id: row.class_id.to_string(),
                    eligible: row.eligible,
                    ready: row.ready,
                    hold_code,
                    hold_message,
                }))
                .expect("expecting an open unbounded channel");
        }
    }

    /// Record (or clear) the streak at the point a sink search settles on `sink`. `refused` is the
    /// heaviest candidate the gate refused in that search, if any. Past rule E (`rule_e`) a refusal is
    /// weighed when rule E weighed the pair over the two exclusive pasts and the refused tip does not
    /// outrank `sink` (ADR-0178); below it, when both absolute orders were read and the refused one is not
    /// strictly ahead.
    fn note_palw_refusal_streak(&self, sink: BlockHash, refused: Option<(BlockHash, DnsReorgOutcome)>, rule_e: bool) {
        use kaspa_consensus_core::api::PalwPartitionRefusalV1;
        let weighed = refused.and_then(|(refused, reason)| {
            (reason == DnsReorgOutcome::DominanceViolation
                && self.palw_state_params_v2.is_some()
                && !matches!(self.reachability_service.try_is_chain_ancestor_of(sink, refused), Ok(true)))
            .then_some(refused)
            .and_then(|refused| {
                if rule_e {
                    let mut states = super::palw_rule_e::PalwRuleEStatesV1::default();
                    let (pair, counts) = self.palw_rule_e_pair_v1(&mut states, refused, sink)?;
                    // RFC-0018 §6: the v2 order past its fence at the sink's DAA (the v1 order exactly below it).
                    let capital_v2 = self.headers_store.get_daa_score(sink).is_ok_and(|daa| self.palw_rule_e_participation_v2_at(daa));
                    return (kaspa_consensus_core::palw_fork_choice_rule_e_v1::palw_rule_e_order_v2(
                        &pair.a, &pair.b, counts, capital_v2,
                    ) != std::cmp::Ordering::Greater)
                        .then_some(refused);
                }
                let (incumbent, challenger) = (self.palw_candidate_order_v2(sink)?, self.palw_candidate_order_v2(refused)?);
                let economic = |o: &kaspa_consensus_core::palw_fork_choice::PalwCandidateOrderV1| {
                    (o.safe_frontier_blue_score, o.safe_weight, o.live_total)
                };
                (economic(&challenger) <= economic(&incumbent)).then_some(refused)
            })
        });
        let mut streak = self.palw_refusal_streak.lock();
        let Some(refused) = weighed else {
            *streak = None;
            return;
        };
        let refused_daa = self.headers_store.get_daa_score(refused).unwrap_or_default();
        let own_daa = self.headers_store.get_daa_score(sink).unwrap_or_default();
        *streak = Some(match *streak {
            Some(prev) => PalwPartitionRefusalV1 {
                sink,
                refused,
                own_daa,
                refused_daa: refused_daa.max(prev.refused_daa),
                first_refused_daa: prev.first_refused_daa.min(refused_daa),
                resolves: prev.resolves.saturating_add(1),
            },
            None => PalwPartitionRefusalV1 { sink, refused, own_daa, refused_daa, first_refused_daa: refused_daa, resolves: 1 },
        });
    }

    /// Objects a test hands the filter directly, carried by nobody: `PALW_RENT_UNPRICED` so the
    /// ADR-0075 rent rules read as absent rather than as "every carrier paid zero", which is what
    /// every pre-SA test means and what an unarmed network does.
    #[cfg(test)]
    pub(super) fn unpriced_for_tests(
        objects: Vec<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2>,
    ) -> Vec<PalwCarriedObjectV1> {
        objects
            .into_iter()
            .map(|object| PalwCarriedObjectV1 {
                object,
                carrier_fee: PALW_RENT_UNPRICED,
                history_pledge: None,
                tir_public_original: None,
                funded_original: None,
                original_binding: None,
                custody_delegation: None,
                refund: None,
                unrefundable: None,
            })
            .collect()
    }

    fn verify_mldsa87_with_context_bool(key: &[u8], message: &[u8], sig: &[u8], context: &[u8]) -> bool {
        verify_mldsa87_with_context(key, message, sig, context).unwrap_or(false)
    }

    fn native_base_withheld_at(&self, carrier: BlockHash, paying_daa: u64, recorded: u64) -> u64 {
        // ADR-0176 C1 (user decision 2026-10-15): past a V3 bond budget the attempt-lane carrier's whole subsidy-derived worker base
        // is withheld (the claim's R-capped escrow is the only subsidy it is ever paid). `recorded` unchanged on every shipped network.
        let recorded = self.palw_r_cap_base_withheld_at(carrier, paying_daa, recorded);
        if !self.dns_retired_at(paying_daa) {
            return recorded;
        }
        let Ok(header) = self.headers_store.get_header(carrier) else {
            return recorded;
        };
        if !kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(header.pow_algo_id) {
            return recorded;
        }
        // Also covers an old 72% earning paid after the fence: no retrospective escrow growth,
        // and no immediate 20% miner windfall. The difference is explicitly never minted.
        let full_base = kaspa_consensus_core::palw_native_settlement_v1::native_worker_base_v1(
            self.coinbase_manager.calc_block_subsidy(header.daa_score),
        );
        recorded.max(full_base)
    }

    /// The 8% inclusion pool is earned by actual useful PALW carriage, never by DNS signatures.
    pub(super) fn native_inclusion_ratio(
        &self,
        ghostdag: &GhostdagData,
        non_daa: &BlockHashSet,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    ) -> (u128, u128) {
        let Some((_, state)) = self.palw_v2_state_at(ghostdag.selected_parent) else {
            return (0, 0);
        };
        let unentitled = self.palw_v2_unentitled_blues(&state, ghostdag, non_daa, point);
        let mut eligible = 0u128;
        let mut total = 0u128;
        let Some(params) = self.palw_state_params_v2.as_ref() else {
            return (0, 0);
        };
        let pricing = kaspa_consensus_core::palw_state_v2::PalwFpPricingV1::of(params, self.palw_canonical_work_daa);
        let classified = ghostdag.classify_palw_mergeset_v1(|member| self.ghostdag_manager.is_round_block(member));
        for member in classified.blues().iter().chain(classified.genuine_reds()) {
            if non_daa.contains(member) {
                continue;
            }
            let Ok(header) = self.headers_store.get_header(*member) else {
                continue;
            };
            // Heartbeat and execution Round have no subsidy to allocate.
            if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_HEARTBEAT_V1 {
                continue;
            }
            let subsidy = self.coinbase_manager.calc_block_subsidy(header.daa_score) as u128;
            total = total.saturating_add(subsidy);
            if unentitled.contains(member) {
                continue;
            }
            let attempt = kaspa_consensus_core::palw_attempt_material_v3::native_attempt_base_v3(&header.palw_commitment)
                .ok()
                .is_some_and(|e| e.attempt.class_id != params.base_class_id());
            let receipt = if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3 {
                let envelope = if kaspa_consensus_core::palw_receipt_v4::palw_receipt_v4_carriage_is_v4(&header.palw_commitment) {
                    kaspa_consensus_core::palw_receipt_v4::PalwReceiptSpendEnvelopeV4::decode(&header.palw_commitment)
                        .ok()
                        .map(|e| e.to_fold_envelope())
                } else {
                    kaspa_consensus_core::palw_freeprompt_v3::PalwReceiptSpendEnvelopeV3::decode(&header.palw_commitment).ok()
                };
                envelope
                    .and_then(|e| state.claim(&e.spend.claim_id))
                    .is_some_and(|c| c.class_id != params.base_class_id() && pricing.prices_in_compute(c))
            } else {
                false
            };
            if attempt || receipt {
                eligible = eligible.saturating_add(subsidy);
            }
        }
        (eligible, total)
    }

    pub(super) fn fee_split_at(&self, daa_score: u64) -> Option<kaspa_consensus_core::dns_finality::FeeSplitParams> {
        let mut split = kaspa_consensus_core::config::params::palw_overlay_fee_split_at_v1(
            self.dns_params.as_ref()?,
            self.palw_overlay_carve,
            daa_score,
        )?;
        if self.dns_retired_at(daa_score) {
            split.subsidy_worker_base_bps = 9200;
            split.subsidy_worker_inclusion_bps = 800;
            split.subsidy_validator_bps = 0;
            split.subsidy_service_bps = 0;
            split.normal_fee_worker_bps = 10000;
            split.normal_fee_validator_bps = 0;
            split.normal_fee_service_bps = 0;
            split.finality_fee_worker_bps = 10000;
            split.finality_fee_validator_bps = 0;
            split.finality_fee_service_bps = 0;
        }
        Some(split)
    }

    /// **The order a merging block at `daa_score` applies its mergeset in** (without the selected
    /// parent): the consensus order, made parents-first past lane accept-order's fence. The ONE order
    /// every consumer that applies a mergeset reads — UTXO acceptance, the fold's merged works, the EVM
    /// lane — so construction and validation, and every node, apply one order at one DAA score.
    pub(super) fn acceptance_ordered_mergeset_without_selected_parent(
        &self,
        ghostdag_data: &GhostdagData,
        daa_score: u64,
    ) -> Vec<BlockHash> {
        ghostdag_data.acceptance_ordered_mergeset_without_selected_parent(
            self.ghostdag_store.as_ref(),
            self.headers_store.as_ref(),
            self.palw_lane_accept_parents_first_at(daa_score),
        )
    }

    /// [`Self::acceptance_ordered_mergeset_without_selected_parent`] behind the selected parent (the EVM
    /// lane's `sorted_mergeset`; its readers are `evm`-only).
    #[cfg_attr(not(feature = "evm"), allow(dead_code))]
    pub(super) fn acceptance_ordered_mergeset(&self, ghostdag_data: &GhostdagData, daa_score: u64) -> Vec<BlockHash> {
        ghostdag_data.acceptance_ordered_mergeset(
            self.ghostdag_store.as_ref(),
            self.headers_store.as_ref(),
            self.palw_lane_accept_parents_first_at(daa_score),
        )
    }

    pub(super) fn accepted_txs_of_chain_block(&self, chain_block: BlockHash) -> Vec<Transaction> {
        match self.acceptance_data_store.get(chain_block) {
            Ok(ad) => self.accepted_txs_from_acceptance_data(&ad),
            Err(StoreError::KeyNotFound(_)) => {
                trace!(
                    "accepted_txs_of_chain_block: no acceptance data for {chain_block} (pruning point / pruned) — treating as no accepted txs"
                );
                Vec::new()
            }
            Err(e) => panic!("accepted_txs_of_chain_block: acceptance_data_store.get({chain_block}) failed: {e}"),
        }
    }

    /// Resolves accepted transactions from already-loaded acceptance data
    /// (`block_transactions_store[index_within_block]`). Split out so the
    /// per-block bond-view walk (ADR-0009 Addendum B) can derive a *not-yet-
    /// committed* block's mutations from the in-memory `ctx.mergeset_acceptance_data`,
    /// whose `acceptance_data_store` entry does not exist until `commit_utxo_state`.
    pub(super) fn accepted_txs_from_acceptance_data(&self, acceptance_data: &AcceptanceData) -> Vec<Transaction> {
        let mut txs = Vec::new();
        for mergeset in acceptance_data.iter() {
            // **A missing entry stops this node, not this walk.** `unwrap()` here turned an absent
            // `BlockTransactions` row into a process abort, and on testnet-11 it did: one block
            // whose transactions the store did not hold crashed the chain's only producer 48 times
            // in a row, deterministically, surviving a datadir wipe because the node re-derived the
            // same reference on every start. A node that cannot read one block's transactions has
            // an incomplete store; that is worth an ERROR an operator can act on, and it is not
            // worth halting a network over.
            //
            // The inner loop already tolerates a missing INDEX (`if let Some`), so tolerating a
            // missing BLOCK is the same posture one level out. On a store that holds the row -- every
            // healthy node -- this is byte-for-byte what it did before.
            let Ok(block_txs) = self.block_transactions_store.get(mergeset.block_hash) else {
                error!(
                    "acceptance data references block {} whose transactions this node does not hold — \
                     skipping it. This node's store is incomplete and its derived PALW objects may \
                     differ from a node that holds it; resync if this repeats.",
                    mergeset.block_hash
                );
                continue;
            };
            for entry in mergeset.accepted_transactions.iter() {
                if let Some(tx) = block_txs.get(entry.index_within_block as usize) {
                    txs.push(tx.clone());
                }
            }
        }
        txs
    }

    /// kaspa-pq Phase 10 (ADR-0009 Addendum A.5): recompute the DNS StakeScore
    /// over the bounded recent epoch window ending at `sink` and stage the
    /// updated [`DnsState`] singleton into `batch`. **Inert** unless the DNS
    /// overlay is configured (`dns_params.is_some()`).
    ///
    /// Bounded-window design (stake_depth is a window quantity, not cumulative):
    /// walk back at most `max_reorg_horizon_blocks` selected-chain blocks from
    /// `sink`, collect on-chain attestation shards, verify each ML-DSA-87
    /// signature against its bond's validator key under
    /// `ATTESTATION_MLDSA87_CONTEXT`, gate by `is_bond_active_at`, then feed the
    /// pure aggregation core. No new store; recompute is reorg-safe.
    fn update_dns_state(&self, batch: &mut WriteBatch, sink: BlockHash) {
        // Preserve historical anchors for replay; no new live DNS confirmations after retirement.
        if self.headers_store.get_daa_score(sink).is_ok_and(|daa| self.dns_retired_at(daa)) {
            return;
        }
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        // The StakeScore recompute below walks the selected chain reading each chain block's
        // acceptance data (`collect_stake_contributions_v2` -> `accepted_txs_of_chain_block`). During
        // pruning-point UTXO import (IBD), the sink IS the imported pruning point, whose acceptance
        // data is deliberately never written — `import_pruning_point_utxo_set` writes only the
        // multiset + UTXO status ("acceptance data and utxo-diff are irrelevant"). There is no chain
        // history to aggregate at that moment, so skip the recompute; `DnsState` is recompute-derived
        // and is rebuilt normally from the first fully-processed block after import. Without this
        // guard the walk panics with `KeyNotFound(AcceptanceData/<pruning point>)`, which surfaces as
        // a tokio runtime panic in the `spawn_blocking` import worker and crashes startup.
        match self.acceptance_data_store.get(sink) {
            Ok(_) => {}
            Err(StoreError::KeyNotFound(_)) => {
                // Missing acceptance data for the sink is EXPECTED only during pruning-point import,
                // where the sink IS the imported pruning point. Anywhere else it signals a store
                // inconsistency, so surface it loudly (still skip rather than panic, but never
                // silently): a genuine bug must be visible in the logs, not swallowed.
                let pp = self.pruning_point_store.read().pruning_point().optional().ok().flatten();
                if pp == Some(sink) {
                    trace!("update_dns_state: skipping recompute during pruning-point import (sink == pruning point {sink})");
                } else {
                    warn!(
                        "update_dns_state: acceptance data missing for sink {sink} (pruning point {pp:?}) — skipping DNS recompute; this is UNEXPECTED outside pruning-point import"
                    );
                }
                return;
            }
            Err(e) => panic!("update_dns_state: acceptance_data_store.get({sink}) failed: {e}"),
        }
        let sink_daa = self.headers_store.get_header(sink).unwrap().daa_score;
        // ADR-0009 Addendum A.3 network_id discriminator := the per-network genesis hash.
        let net_id = self.genesis.hash;

        // PR-10.11 throttle: StakeScore is per-epoch, so recompute DnsState only
        // once per epoch — when the sink's epoch differs from the last-written
        // DnsState's epoch. This bounds the window walk to ~once per
        // `epoch_length_blocks` (O(1) amortized per block) instead of walking
        // `max_reorg_horizon_blocks` on every virtual commit. Deterministic and
        // epoch-granular; safe on devnet/testnet where the gate is dormant
        // (Bootstrap). M-01 / audit #3: the recompute no longer depends on which sink first
        // crosses the boundary. The StakeScore is canonical (`collect_stake_contributions_v2`
        // credits only this chain's canonical lagged anchor per ready epoch), AND the
        // DNS-confirmed anchor is that canonical lagged anchor — NOT the sink (see
        // `confirmable_anchor` below). The reorg gate protects ONLY the confirmed anchor, so two
        // nodes that recompute at different boundary sinks still protect the identical anchor;
        // only `selected_chain_anchor` (read solely by this throttle) differs between them.
        let prev_dns_state = self.dns_state_store.read().get().ok();
        // kaspa-pq DNS v3: throttle the recompute to once per BLUE_SCORE epoch (epochs are
        // blue_score-coordinated now), not the DAA epoch. The recompute is canonical
        // regardless of cadence — this only bounds how often the window walk runs, and must
        // fire at least once per blue_score epoch so confirmations don't lag. `prev`'s
        // blue_score is read from its anchor (recent — at most ~1 epoch old, never pruned).
        let sink_blue = self.headers_store.get_blue_score(sink).unwrap();
        let epoch_len_blue = dns_params.attestation_epoch_length_blue_score.max(1);
        if let Some(prev) = prev_dns_state.as_ref() {
            let prev_blue = self.headers_store.get_blue_score(prev.selected_chain_anchor).unwrap_or(0);
            if sink_blue / epoch_len_blue == prev_blue / epoch_len_blue {
                return;
            }
        }

        // Snapshot the bond set (bounded by the active validator count).
        let bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();

        // Current total active stake + validator count at the sink (rollout gating).
        let active_stakes_at_sink: Vec<_> = bonds.iter().filter(|b| is_bond_active_at(b, sink_daa)).map(|b| b.amount).collect();
        let total_active = active_stakes_at_sink.iter().fold(0u64, |acc, amount| acc.saturating_add(*amount));
        // **DISTINCT validators, not bonds** (audit 2026-08-30). Nothing binds a key to a single
        // bond, and every other counter in this file dedups (`min_anchor_attesters`,
        // `aggregate_epoch_tallies`). This
        // one did not, so one operator holding N bonds read as N validators: a single key could
        // register twelve 50M bonds and flip DNS to `Active` alone, holding 100 % of the voting
        // weight — which is audit H-11's refusal verbatim, and it silently nullified the 3 → 12
        // re-pricing whose whole argument is that corrupting a 2/3 quorum costs `ceil(2n/3)`
        // SEPARATE bonds.
        let active_validators = bonds
            .iter()
            .filter(|b| is_bond_active_at(b, sink_daa))
            .map(|b| b.validator_pubkey_hash)
            .collect::<std::collections::BTreeSet<_>>()
            .len() as u32;
        let rollout_stage = if sink_daa >= dns_params.dns_activation_daa_score
            && total_active >= dns_params.min_active_stake_sompi
            && active_validators >= dns_params.min_active_validators
            // kaspa-pq DNS v3 (PR6): refuse Active unless the blue_score canonical-anchor params
            // are self-consistent. In Active the reorg gate's finality depends entirely on them,
            // so an invalid config fails safe (stay Bootstrap, gate dormant) rather than splitting.
            && dns_params.dns_v3_params_consistent()
        {
            DnsRolloutStage::Active
        } else {
            DnsRolloutStage::Bootstrap
        };

        // kaspa-pq DNS v3: canonical, blue_score-coordinated StakeScore. Credit only
        // attestations naming THIS chain's canonical lagged anchor for their (ready,
        // non-duplicate) epoch, with the per-epoch denominator keyed by the canonical anchor
        // DAA and zero-attestation ready epochs included (`collect_stake_contributions_v2`).
        let credit_rule = dns_params.epoch_credit_rule();
        let (contributions, epoch_anchor_daa) =
            self.collect_stake_contributions_v2(sink, None, &bonds, net_id.as_byte_slice(), dns_params);
        let totals = total_active_stake_by_epoch(&bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        let stake_depth = compute_stake_score(&per_epoch, credit_rule);

        // kaspa-pq Phase 13 (ADR-0018 §C): derive the read-only DnsHealth liveness signal
        // from the same per-epoch tallies that fed the StakeScore. `overlay_active` iff the
        // reorg gate is engaged (`Active`); in Bootstrap there is no DNS finality to judge,
        // so health stays `DisabledBeforeActivation`. Purely a signal — never a
        // block-validity input, so this is inert wherever the gate is dormant.
        let health = derive_dns_health(
            &per_epoch,
            credit_rule,
            dns_params.stake_censorship_floor_bps,
            dns_params.degraded_stake_quality_epochs,
            rollout_stage == DnsRolloutStage::Active,
        );

        // kaspa-pq DNS-finality (§6.5): structured diagnostics for the StakeScore credit
        // path — how many attestations were credited at this sink, the credited
        // (epoch, bond, stake) tuples, and the resulting stake_depth. Inert when there is
        // no attestation traffic this recompute (empty contributions ⇒ no log).
        if !contributions.is_empty() {
            info!(
                "[stake-score] sink={} sink_blue={} credited {} attestation(s) over {} ready epoch(s) → stake_depth={} (rollout={:?}, health={:?})",
                sink,
                sink_blue,
                contributions.len(),
                epoch_anchor_daa.len(),
                stake_depth.0,
                rollout_stage,
                health,
            );
            for c in contributions.iter() {
                debug!(
                    "[stake-score] credited epoch={} bond={} weight={} validator_id={}",
                    c.epoch, c.bond_outpoint.transaction_id, c.signed_weight, c.validator_id
                );
            }
        }

        // audit #3: the canonical lagged anchor of the latest ready epoch — a fixed,
        // blue_score-coordinated selected-chain point every node derives identically. THIS (not
        // the POV-dependent `sink`) is what gets DNS-confirmed and protected by the reorg gate, so
        // nodes that recompute at different boundary sinks still protect the same anchor. `None`
        // until an epoch's anchor is buried and lag-ready (early chain / not yet ready).
        // incident 2026-08-03 §8 ("dead-branch confirm"): the confirmable anchor is the most recent
        // READY epoch that actually carries credited attestation support — NOT merely the most
        // recent ready epoch.
        //
        // `stake_depth` is a WINDOWED sum over several epochs, so a branch whose validators have
        // gone silent still clears `required_stake_depth` from stake accrued earlier. Confirming
        // "the latest ready epoch" would let such a branch keep latching NEW anchors that nothing
        // attests to, arming a reorg veto — evaluated against its own branch-local bond view, and
        // therefore unreleasable — on a branch the network has moved off. Anchoring to the latest
        // SUPPORTED epoch freezes the confirmed point where validators actually signed, and it
        // resumes advancing the moment attestations resume.
        //
        // Monotonic by construction: within the stake-score window the OLDEST epochs age out
        // first, so `max(supported epoch)` never decreases — it only becomes `None` once ALL
        // support has aged out, and `advance_dns_confirmation` then carries the previous confirmed
        // anchor forward unchanged. The value stays a deterministic function of the selected chain
        // (`contributions` is derived from it), so nodes still agree.
        let latest_ready_epoch = ready_epoch_from_tip_blue_score(sink_blue, epoch_len_blue, dns_params.attestation_lag_blue_score);
        let confirmable = latest_ready_epoch
            .and_then(|ready| contributions.iter().map(|c| c.epoch).filter(|&e| e <= ready).max())
            .and_then(|epoch| self.canonical_anchor_by_blue_score(epoch, sink, dns_params));
        let confirmable_anchor = confirmable.map(|a| (a.anchor_hash, a.anchor_daa_score));

        // Invariant restated for `advance_dns_confirmation` (which is pure and independently
        // tested): the anchor it confirms carries live support in its own epoch. True by
        // construction above; passed explicitly so the rule is enforced at the decision point.
        //
        // Counted as DISTINCT `validator_id`s, not raw contributions: `dns_params.min_anchor_attesters`
        // asks how many independent signers back the anchor, and one validator can appear more than
        // once in `contributions` (multiple bonds). The id is bond-bound
        // (`att.validator_id != bond.validator_pubkey_hash` is rejected in the credit walk), so it
        // cannot be varied to fake breadth.
        let anchor_epoch_attesters = confirmable.map_or(0, |a| {
            contributions.iter().filter(|c| c.epoch == a.epoch).map(|c| c.validator_id).collect::<HashSet<_>>().len() as u32
        });

        // true WorkDepth (audit H-02 Option A): WorkDepth(B) is the blue work accumulated SINCE the
        // confirmable anchor B — anchor-relative (`blue_work(sink) − blue_work(anchor)`), NOT the
        // cumulative-from-genesis `blue_work(sink)`. This makes it a real confirmation DEPTH (how much
        // PoW is piled on the confirmed point), so `is_dns_confirmed` genuinely requires BOTH a
        // work-depth AND a stake-depth (two-dimensional confirmation, matching the reorg gate's
        // anchor-relative work∧stake dominance). With `required_work_depth = 0` (devnet/simnet) this is
        // inert (stake-only); on mainnet/testnet (`required_work_depth > 0`) the work term gates too.
        // `ZERO` when no anchor is ready yet (no confirmation happens then anyway).
        let work_depth = confirmable_anchor
            .map(|(anchor_hash, _)| {
                self.ghostdag_store
                    .get_blue_work(sink)
                    .unwrap_or_default()
                    .saturating_sub(self.ghostdag_store.get_blue_work(anchor_hash).unwrap_or_default())
            })
            .unwrap_or_default();
        let depth_state = advance_dns_confirmation(
            prev_dns_state.as_ref(),
            sink,
            sink_daa,
            confirmable_anchor,
            work_depth,
            stake_depth,
            rollout_stage,
            // validator_set_commitment: ADR-0017 dropped the sortition committee, so the
            // StakeScore path binds no committee snapshot — this stays zero.
            BlockHash::default(),
            health,
            dns_params.required_work_depth,
            dns_params.required_stake_depth,
            anchor_epoch_attesters,
            dns_params.min_anchor_attesters,
        );
        // ADR-0128 Decision 5: past the BFT fence (at the sink) the confirmed anchor is the newest
        // DNS-final one instead of the depth rule's; every other field above is kept as computed.
        // Below the fence, and wherever the fence is unset, the depth rule's state is written as is.
        let new_state = match self.dns_bft_gate.filter(|gate| gate.activation.is_active(sink_daa) && !self.dns_retired_at(sink_daa)) {
            Some(gate) => self.dns_bft_confirmed_state(depth_state, prev_dns_state.as_ref(), sink, &bonds, dns_params, &gate),
            None => depth_state,
        };
        self.dns_state_store.write().set_batch(batch, new_state).unwrap();
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): recompute the per-epoch
    /// `EpochTally` accumulator over the bounded selected-chain window ending at
    /// `sink` and stage the live (non-finalized) epochs into `batch`. Gated by the
    /// v2 fence `pos_v2_activation_daa_score`: **inert** (returns after a single
    /// header read) on devnet/simnet (`GENESIS_ACTIVE_DNS_PARAMS`, fence `u64::MAX`);
    /// **active from block 1** on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence `0`)
    /// — also requires the DNS overlay to be configured.
    ///
    /// Recompute design (the `update_dns_state` precedent — reorg-safe with no
    /// incremental delta): the accumulator is a pure function of the selected
    /// chain (each block's persisted rewarded `(bond, epoch)` keys + quality
    /// sub-pool, both block-hash-keyed so only the current chain's rows are read)
    /// and the current bond snapshot, so a reorg simply re-derives the live epochs
    /// from the new chain.
    ///
    /// Window: `finalization_depth = reward_uniqueness_window_blocks +
    /// max_reorg_horizon_blocks` — a non-final epoch's included set stays mutable
    /// up to `window` past its anchor and a reorg can rewrite up to
    /// `max_reorg_horizon` blocks, so burying past their sum makes the tally
    /// immutable. The walk covers `finalization_depth + 2·epoch_length` so every
    /// non-final epoch's contributing blocks are seen. An epoch already `finalized`
    /// in the store is never re-derived (its blocks may lie partly outside the
    /// window — an incomplete recompute).
    ///
    /// NOTE (perf): unlike `update_dns_state` this does not throttle to
    /// once-per-epoch — instead the per-block work is **bounded by design** to the
    /// `walk_bound = finalization_depth + 2·epoch_length` window (a few thousand
    /// header/store reads at production params, all block-hash-keyed and cached), so
    /// it is O(window) per virtual commit, not O(chain). This bounded-window walk is
    /// what makes it reorg-safe (a pure function of the current selected chain, no
    /// incremental delta), and it runs from block 1 on mainnet/testnet (fence `0`).
    fn update_epoch_accumulator(&self, batch: &mut WriteBatch, sink: BlockHash) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        let sink_daa = self.headers_store.get_daa_score(sink).unwrap();
        // The v2 master fence: inert (no walk, no write) on devnet/simnet (`u64::MAX`);
        // the walk runs from block 1 on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence `0`).
        if sink_daa < dns_params.pos_v2_activation_daa_score {
            return;
        }

        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let walk_bound = self.overlay_window_walk_bound(dns_params);

        // Gather this selected chain's per-block contributions within the window, oldest →
        // newest (so the `included` ordering is chain-deterministic). ADR-0022: this goes
        // through `selected_chain_overlay_window`, which merges the persisted below-pruning-
        // point window — so a pruned-IBD node recomputes epochs straddling the pruning point
        // correctly (its walk cannot reach below it). On a from-genesis node the merge is inert.
        let contributions: Vec<BlockEpochContribution> = self
            .selected_chain_overlay_window(sink, sink_daa, walk_bound)
            .into_iter()
            .map(|c| BlockEpochContribution {
                block_daa_score: c.block_daa_score,
                rewarded_keys: c.rewarded_keys,
                quality_subpool: c.quality_subpool,
            })
            .collect();

        // Snapshot the bond set (bounded by the active validator count), as update_dns_state does.
        let bonds: Vec<StakeBondRecord> =
            self.stake_bonds_store.read().iterator().filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone())).collect();

        for (epoch, tally) in recompute_epoch_tallies(sink_daa, epoch_len, finalization_depth, &contributions, &bonds) {
            // Never re-derive a finalized epoch — it is immutable and its blocks may
            // already lie partly outside the walk window (an incomplete recompute).
            if self.epoch_accumulator_store.get(epoch).map(|t| t.finalized).unwrap_or(false) {
                continue;
            }
            self.epoch_accumulator_store.set_batch(batch, epoch, tally).unwrap();
        }
    }

    /// kaspa-pq ADR-0022: build the [`OverlaySnapshot`] **as-of `selected_parent`** —
    /// the exact set of overlay rows a pruned-IBD node needs to validate
    /// `selected_parent`'s descendants. Committed in `Header::overlay_commitment_root`
    /// (template fills it, `verify_expected_utxo_state` re-derives + checks it, c==v).
    ///
    /// Deterministic across the template path (`selected_parent` = sink) and the
    /// validation path (`selected_parent` = the block's selected parent): it reads
    /// only the walked bond view + per-block stores (`reserve_balance_store`,
    /// `rewarded_epochs_store`, `block_quality_pool_store`), never the per-sink
    /// epoch accumulator. Empty (⇒ `OverlaySnapshot::default()`) when the overlay
    /// is dormant; the window walk mirrors `update_epoch_accumulator` (same
    /// `walk_bound`, same pos_v2 fence) but is anchored at `selected_parent` and
    /// keeps only blocks that actually contributed (rewarded keys or quality pool),
    /// so the snapshot stays small on a validator-sparse chain.
    pub(super) fn compute_overlay_snapshot(
        &self,
        selected_parent: BlockHash,
        selected_parent_bond_view: &ActiveBondView,
    ) -> OverlaySnapshot {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return OverlaySnapshot::default();
        };

        let anchor_daa = self.headers_store.get_daa_score(selected_parent).unwrap();

        // Normalize the (non-canonical) stored `status` to the EFFECTIVE status at the
        // anchor. The raw `status` field diverges across reorg paths — `ActiveBondView::revert`
        // restores a reverted-slash bond to `Active` even if it was originally `Pending`, so a
        // never-slashed vs slashed-then-reverted bond can carry different `status` for byte-equal
        // history. `effective_bond_status` is a pure function of the canonical timing fields
        // (`activation_daa_score`/`slashed_at`/`unbond_request`), which the reward path already
        // uses; normalizing here makes the committed bond set deterministic across reorgs without
        // touching consensus-state mutation (the raw field is otherwise vestigial).
        let mut bonds = selected_parent_bond_view.records();
        for b in bonds.iter_mut() {
            b.status = effective_bond_status(b, anchor_daa);
        }
        let reserve_balance = self.reserve_balance_store.get(selected_parent).unwrap_or(0);

        let walk_bound = self.overlay_window_walk_bound(dns_params);
        let window = self.selected_chain_overlay_window(selected_parent, anchor_daa, walk_bound);

        OverlaySnapshot { bonds, reserve_balance, window }
    }

    /// ADR-0022: `reward_uniqueness_window + max_reorg_horizon + 2·epoch_length` — the
    /// selected-chain window that covers BOTH the reward-uniqueness dedup and the
    /// epoch-accumulator recompute. Shared by the overlay snapshot, the epoch
    /// accumulator, and the reward dedup so all three see the same span.
    pub(super) fn overlay_window_walk_bound(&self, dns_params: &DnsParams) -> u64 {
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        finalization_depth.saturating_add(epoch_len.saturating_mul(2))
    }

    /// kaspa-pq ADR-0022: the per-block overlay contributions on `anchor`'s selected
    /// chain within `walk_bound` (rewarded keys + quality sub-pool), oldest → newest,
    /// MERGING the persisted pruning-point snapshot's below-pruning-point window.
    ///
    /// The selected-chain walk cannot traverse below the pruning point (no reachability
    /// there after a prune or a pruned-IBD import), so it stops at the persisted pruning
    /// point and the persisted snapshot supplies everything at/below it. On a node whose
    /// pruning point is far below `anchor` (normal operation) the walk never reaches it
    /// and every persisted entry is outside `walk_bound`, so the merge is a no-op
    /// (byte-identical to a from-genesis node). Empty-contribution blocks are skipped.
    /// The single seam through which all three below-pp consumers (overlay commitment,
    /// epoch accumulator, reward dedup) read the historical window.
    pub(super) fn selected_chain_overlay_window(
        &self,
        anchor: BlockHash,
        anchor_daa: u64,
        walk_bound: u64,
    ) -> Vec<BlockOverlayContribution> {
        let persisted = self.pruning_overlay_snapshot_store.read().get().ok();
        let stop_at = persisted.as_ref().map(|p| p.pruning_point);

        // Above-pruning-point part, collected newest → oldest by the chain walk.
        let mut above: Vec<BlockOverlayContribution> = Vec::new();
        for ancestor in std::iter::once(anchor).chain(self.reachability_service.default_backward_chain_iterator(anchor)) {
            if Some(ancestor) == stop_at {
                break;
            }
            let ancestor_daa = self.headers_store.get_daa_score(ancestor).unwrap();
            if anchor_daa.saturating_sub(ancestor_daa) > walk_bound {
                break;
            }
            let rewarded_keys = self.rewarded_epochs_store.get(ancestor).map(|k| (*k).clone()).unwrap_or_default();
            let quality_subpool = self.block_quality_pool_store.get(ancestor).unwrap_or(0);
            if rewarded_keys.is_empty() && quality_subpool == 0 {
                continue;
            }
            above.push(BlockOverlayContribution {
                block_hash: ancestor,
                block_daa_score: ancestor_daa,
                rewarded_keys,
                quality_subpool,
            });
        }
        above.reverse(); // → oldest → newest

        // Below-pruning-point part: the persisted window (stored oldest → newest), kept
        // to entries still within `walk_bound` of the anchor. These never overlap `above`
        // (the walk stopped AT the pruning point), so prepending yields a single
        // oldest → newest selected-chain ordering.
        let mut window: Vec<BlockOverlayContribution> = Vec::new();
        if let Some(p) = persisted {
            for c in p.snapshot.window {
                if anchor_daa.saturating_sub(c.block_daa_score) <= walk_bound {
                    window.push(c);
                }
            }
        }
        window.extend(above);
        // kaspa-pq ADR-0022 fix: the persisted below-pruning-point window includes the pruning-point
        // boundary block (it is the newest entry of the captured `compute_overlay_snapshot(pp)` walk),
        // and across pruning advances that boundary block can also be re-captured into a later
        // snapshot's window — so a pruned-IBD node's recomputed window carried ONE EXTRA (duplicate)
        // entry at the pruning-point block vs a from-genesis node's clean live walk. That single extra
        // contribution changed the canonicalized overlay snapshot → the first post-pruning block's
        // `overlay_commitment_root` recompute (and the epoch/reward recompute that share this seam)
        // diverged (c != v) and the pruned-IBD node got stuck at "0 valid chain blocks". Dedup by block
        // hash: a from-genesis live walk visits each selected-chain block exactly once, so this is a
        // no-op there and only removes the spurious merge-path duplicate — restoring construction ==
        // validation for pruned-IBD joiners.
        let mut seen = std::collections::HashSet::new();
        window.retain(|c| seen.insert(c.block_hash));
        window
    }

    /// kaspa-pq Phase 13 (ADR-0018 §H) + DNS v3 (PR6): the StakeScore a branch accumulated
    /// **since the common ancestor** — the selected chain from `tip` back to (but excluding)
    /// `ancestor`, scored under `bonds` (that branch's bond set) and this network's `φS`. Uses
    /// the v3 canonical-anchor verifier (`collect_stake_contributions_v2`) with
    /// `stop_at = ancestor`, so the branch is scored only on canonical attestations for the
    /// epochs anchored strictly above the common ancestor (its OWN segment) — byte-identical to
    /// the sink-side StakeScore and immune to a branch inflating its score with non-canonical
    /// (current-sink / fabricated) targets. Inert wherever the overlay is dormant.
    fn stake_score_since_ancestor(
        &self,
        tip: BlockHash,
        ancestor: BlockHash,
        bonds: &[StakeBondRecord],
        dns_params: &DnsParams,
        net_id: &[u8],
    ) -> StakeScore {
        let (contributions, epoch_anchor_daa) = self.collect_stake_contributions_v2(tip, Some(ancestor), bonds, net_id, dns_params);
        let totals = total_active_stake_by_epoch(bonds, &epoch_anchor_daa);
        let per_epoch = aggregate_epoch_tallies(&contributions, &totals);
        compute_stake_score(&per_epoch, dns_params.epoch_credit_rule())
    }

    /// kaspa-pq Phase 13 (ADR-0018 §H): the selected-chain common ancestor of `candidate` and
    /// `canonical` — the first block on **canonical's** selected chain (from `canonical`
    /// inclusive, walking back) that is also a chain ancestor of `candidate`. `None` if none is
    /// found within `horizon` steps.
    ///
    /// The walk is deliberately CANONICAL-side, so `horizon` bounds **how many of this node's own
    /// chain blocks accepting the candidate would rewind** — the quantity "reorg horizon" names.
    /// Walking the candidate side instead (as this did before) bounds how far the *other* branch
    /// ran, which produces the wrong verdict on an asymmetric fork: an attacker who mines 100k
    /// blocks in secret while canonical advances 300 would be measured as "beyond the horizon" and
    /// the gate would ABSTAIN — handing the deep-reorg attacker exactly the pass the veto exists to
    /// deny. The common ancestor found is the same block either way; only the metric differs.
    ///
    /// [`Self::chain_common_ancestor_within`] computes the identical answer in O(log horizon) and
    /// is what the gate calls; this walk is its fallback when the chain index is unavailable.
    pub(crate) fn chain_common_ancestor_walk(&self, candidate: BlockHash, canonical: BlockHash, horizon: u64) -> Option<BlockHash> {
        // `default_backward_chain_iterator` YIELDS `canonical` first, so `walked` is exactly the
        // number of chain blocks that would be rewound and the bound is inclusive of `horizon`
        // (matching the binary search's `canonical_index − horizon` floor). The previous
        // `once(a).chain(iterator(a))` form double-counted the start block, quietly costing one
        // step of the budget.
        for (walked, block) in (0_u64..).zip(self.reachability_service.default_backward_chain_iterator(canonical)) {
            if walked > horizon {
                return None;
            }
            if matches!(self.reachability_service.try_is_chain_ancestor_of(block, candidate), Ok(true)) {
                return Some(block);
            }
        }
        None
    }

    /// The same selected-chain common ancestor as [`Self::chain_common_ancestor_walk`], found in
    /// **O(log horizon)** by binary search over the canonical chain index instead of by walking.
    ///
    /// `canonical` must be a block indexed in `selected_chain_store` (in practice the current
    /// sink). Let `chain[i]` be that store's block at index `i`; then
    /// `chain[i] is_chain_ancestor_of candidate` is **monotone decreasing in `i`** — chain-ancestry
    /// is transitive and `chain[j]` is a chain ancestor of `chain[i]` for every `j < i` — so the
    /// deepest index that still answers `true` is exactly the common ancestor. The predicate is
    /// evaluated on `[canonical_index − horizon, canonical_index]`, so a fork deeper than the
    /// horizon is reported as `None` without ever having touched the intervening blocks.
    ///
    /// Why this replaces the walk in the gate: `sink_search` evaluates the gate for EVERY candidate
    /// it pops, and the walk is O(divergence) each time. That amplification is what turned the
    /// 2026-07-19 wedge into ever-lengthening resolve times, and it is the reason the gate horizon
    /// could not simply be raised. With the search cost logarithmic, the horizon becomes a policy
    /// choice (how deep a fork may DNS finality have an opinion about) rather than a cost ceiling.
    ///
    /// Falls back to the walk when the index is unavailable for these blocks (a freshly-imported or
    /// partially-pruned chain store); both measure the same canonical-side horizon, so the verdict
    /// never depends on which path answered.
    pub(crate) fn chain_common_ancestor_within(&self, candidate: BlockHash, canonical: BlockHash, horizon: u64) -> Option<BlockHash> {
        let index_lookup = |idx: u64| self.selected_chain_store.read().get_by_index(idx).ok();
        let is_ancestor_of_candidate =
            |hash: BlockHash| matches!(self.reachability_service.try_is_chain_ancestor_of(hash, candidate), Ok(true));

        let Ok(canonical_index): Result<u64, _> = self.selected_chain_store.read().get_by_hash(canonical) else {
            return self.chain_common_ancestor_walk(candidate, canonical, horizon);
        };
        let floor_index = canonical_index.saturating_sub(horizon);
        let Some(floor_hash) = index_lookup(floor_index) else {
            return self.chain_common_ancestor_walk(candidate, canonical, horizon);
        };
        // The horizon floor itself must be a chain ancestor of the candidate; if it is not, the
        // branches diverged BELOW the horizon and the gate has nothing to judge on.
        if !is_ancestor_of_candidate(floor_hash) {
            return None;
        }
        let (mut lo, mut hi) = (floor_index, canonical_index);
        while lo < hi {
            // Upper mid: `lo` is known-true, so probing the upper half keeps the loop shrinking.
            let mid = lo + (hi - lo).div_ceil(2);
            let Some(mid_hash) = index_lookup(mid) else {
                // A hole in the index (pruning racing this read) — fall back rather than guess.
                return self.chain_common_ancestor_walk(candidate, canonical, horizon);
            };
            if is_ancestor_of_candidate(mid_hash) {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        index_lookup(lo)
    }

    /// kaspa-pq DNS v3 (Canonical Lagged Anchor): the canonical, blue_score-coordinated
    /// epoch anchor for `epoch` as seen from `tip`'s selected chain — the **most-recent
    /// selected-chain ancestor with `blue_score <= anchor_cutoff(epoch)`** (cutoff =
    /// `epoch_end(epoch) - backoff`). Walks the selected-parent chain from `tip`
    /// (inclusive) reading each block's header `blue_score`/`daa_score`, collecting
    /// `(hash, blue_score, daa_score)` tip-first (blue_score strictly decreasing) until it
    /// buries the *previous* epoch's cutoff (so the pure core can decide the
    /// duplicate-anchor flag) or runs past `stake_score_window_blue_score`, then defers to
    /// the pure [`canonical_lagged_epoch_anchor`] core.
    ///
    /// The selected-chain *position* is read from header-committed `blue_score`, NEVER the
    /// store index (which is store-local: archival numbers from genesis, IBD from its
    /// pruning point), so archival and IBD-synced nodes derive the identical anchor. The
    /// signer (PR3), verifier (PR4), reward path (PR5) and reorg gate all call this so they
    /// agree on which block anchors an epoch. Reads only committed header data → reorg-safe.
    ///
    /// Returns `None` when the epoch's anchor cutoff is not yet buried by the tip
    /// (`cutoff > tip.blue_score` — a future / unburied epoch has no canonical anchor on
    /// this chain yet; the degenerate "most-recent-at-or-below == tip" is suppressed) or
    /// when the chain within the window does not reach the cutoff (epoch too old to
    /// credit). The stronger `attestation_lag_blue_score` readiness gate is applied by the
    /// signer / verifier on top of this.
    pub(crate) fn canonical_anchor_by_blue_score(
        &self,
        epoch: u64,
        tip: BlockHash,
        dns_params: &DnsParams,
    ) -> Option<CanonicalLaggedEpochAnchor> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let backoff = dns_params.attestation_anchor_backoff_blue_score;
        let window = dns_params.stake_score_window_blue_score;

        let tip_blue_score = self.headers_store.get_blue_score(tip).ok()?;
        // The epoch's anchor cutoff must be buried by the tip; otherwise "most-recent
        // at-or-below" would degenerate to the tip itself (a future / unburied epoch has no
        // canonical anchor on this chain yet).
        let cutoff = anchor_cutoff_blue_score(epoch, epoch_len, backoff);
        if cutoff > tip_blue_score {
            return None;
        }
        // Walk the selected-parent chain tip -> down, collecting (hash, blue, daa) until we
        // have buried the PREVIOUS epoch's cutoff (so the duplicate-anchor check is
        // decidable; for epoch 0 this coincides with this epoch's cutoff) or run past the
        // configured stake-score window. Position is read from blue_score, never the index.
        let needed = anchor_cutoff_blue_score(epoch.saturating_sub(1), epoch_len, backoff);
        let mut ancestors: Vec<(BlockHash, u64, u64)> = Vec::new();
        for hash in std::iter::once(tip).chain(self.reachability_service.default_backward_chain_iterator(tip)) {
            let compact = self.headers_store.get_compact_header_data(hash).ok()?;
            if tip_blue_score.saturating_sub(compact.blue_score) > window {
                break; // out of the stake-score window
            }
            ancestors.push((hash, compact.blue_score, compact.daa_score));
            if compact.blue_score <= needed {
                break; // buried the prev cutoff (and a fortiori this one) -> enough to decide
            }
        }
        canonical_lagged_epoch_anchor(epoch, epoch_len, backoff, &ancestors)
    }

    /// kaspa-pq DNS v3: the canonical anchors for every **creditable** epoch within
    /// `window_blue_score` of `tip`, computed in ONE selected-parent-chain walk.
    /// "Creditable" = ready (buried by `attestation_lag_blue_score`), non-duplicate
    /// (`anchor(E) != anchor(E-1)`; a sparse chain that reused the previous anchor earns no
    /// new credit), and recent enough that both `anchor_cutoff(E)` and `anchor_cutoff(E-1)`
    /// fall inside the collected window (so the duplicate flag is reliable). Older / unready
    /// / duplicate epochs are simply absent. Position comes from header-committed
    /// `blue_score`, never the store index, so archival and IBD-synced nodes agree.
    ///
    /// `window_blue_score` is an explicit argument because the two consumers legitimately need
    /// different depths, and silently using the shorter one would truncate the longer walk:
    /// the attestation/StakeScore paths pass `stake_score_window_blue_score`, while the VLT
    /// compute-credit walk passes the much longer `vlt_credit_window_blue_score` (its `C_i(E)`
    /// sum reaches back `credit_window_epochs`, far beyond the attestation window). An epoch
    /// with no anchor here is skipped by its caller, so a too-short window does not fail
    /// loudly — it just silently under-credits.
    pub(crate) fn canonical_anchors_in_window(
        &self,
        tip: BlockHash,
        dns_params: &DnsParams,
        window_blue_score: u64,
    ) -> BTreeMap<u64, CanonicalLaggedEpochAnchor> {
        let epoch_len = dns_params.attestation_epoch_length_blue_score.max(1);
        let backoff = dns_params.attestation_anchor_backoff_blue_score;
        let lag = dns_params.attestation_lag_blue_score;
        let window = window_blue_score;

        let mut anchors: BTreeMap<u64, CanonicalLaggedEpochAnchor> = BTreeMap::new();
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return anchors;
        };
        let Some(latest_ready) = ready_epoch_from_tip_blue_score(tip_blue, epoch_len, lag) else {
            return anchors; // no epoch buried by `lag` yet
        };

        // One walk: collect the selected chain tip-first down to the window bound.
        let mut ancestors: Vec<(BlockHash, u64, u64)> = Vec::new();
        for hash in std::iter::once(tip).chain(self.reachability_service.default_backward_chain_iterator(tip)) {
            let Ok(c) = self.headers_store.get_compact_header_data(hash) else {
                break;
            };
            if tip_blue.saturating_sub(c.blue_score) > window {
                break;
            }
            ancestors.push((hash, c.blue_score, c.daa_score));
        }
        let oldest_blue = ancestors.last().map(|a| a.1).unwrap_or(tip_blue);

        // From the latest ready epoch downward, derive each epoch's anchor over the shared
        // ancestor slice; stop once the PREVIOUS epoch's cutoff falls below the collected
        // window (older epochs aren't reliably decidable, hence not creditable). Skip
        // duplicates (no new credit).
        let mut epoch = latest_ready;
        loop {
            let prev_cutoff = anchor_cutoff_blue_score(epoch.saturating_sub(1), epoch_len, backoff);
            if prev_cutoff < oldest_blue {
                break;
            }
            if let Some(anchor) = canonical_lagged_epoch_anchor(epoch, epoch_len, backoff, &ancestors)
                && !anchor.duplicate_of_previous_anchor
            {
                anchors.insert(epoch, anchor);
            }
            if epoch == 0 {
                break;
            }
            epoch -= 1;
        }
        anchors
    }

    /// kaspa-pq DNS v3 verifier: collect + verify the stake attestations on the selected
    /// chain ending at `tip`, crediting an attestation ONLY if it targets THIS chain's
    /// canonical anchor for its epoch (**GoodAttestation v3**): `att.target_hash` and
    /// `att.target_daa_score` equal the canonical `(anchor_hash, anchor_daa_score)` for
    /// `att.epoch`, the bond is `Active` at the canonical anchor DAA, the self-declared
    /// `validator_id` is bound to the bond (P-1A), and the ML-DSA-87 signature verifies under
    /// `ATTESTATION_MLDSA87_CONTEXT`. The per-epoch denominator (`epoch_anchor_daa`) is keyed
    /// by the CANONICAL anchor DAA (not the v1 first-seen self-reported value) and includes
    /// every creditable (ready, non-duplicate) epoch in the window — **even those with zero
    /// attestations** — so a participation gap is visible to φS / DnsHealth instead of
    /// silently vanishing (the v1 weakness that let honest validators signing divergent
    /// current-sink targets all fall below the φS floor).
    ///
    /// Replaces the v1 self-reported-target `collect_stake_contributions` for the sink-side
    /// StakeScore. For a branch segment (reorg gate, `stop_at = Some(I)`) it credits only
    /// epochs anchored strictly above the common ancestor `I` (the shared prefix belongs to
    /// neither branch's since-`I` delta); the reorg gate itself is migrated to this path in
    /// PR6 (it stays on v1 until then — inert, Active-only). Reads only committed acceptance
    /// + header data, so it is deterministic and reorg-safe; inert wherever the overlay is
    /// dormant.
    pub(crate) fn collect_stake_contributions_v2(
        &self,
        tip: BlockHash,
        stop_at: Option<BlockHash>,
        bonds: &[StakeBondRecord],
        net_id: &[u8],
        dns_params: &DnsParams,
    ) -> (Vec<AttestationContribution>, BTreeMap<u64, u64>) {
        // Canonical anchors for the creditable epoch window, computed from THIS chain's tip.
        let anchors = self.canonical_anchors_in_window(tip, dns_params, dns_params.stake_score_window_blue_score);
        // For a branch segment (`stop_at = Some(I)`), credit only epochs anchored strictly
        // above `I`; the sink-side path (`None`) keeps them all.
        let creditable: BTreeMap<u64, CanonicalLaggedEpochAnchor> = anchors
            .into_iter()
            .filter(|(_, a)| match stop_at {
                Some(i) => a.anchor_hash != i && !self.reachability_service.is_chain_ancestor_of(a.anchor_hash, i),
                None => true,
            })
            .collect();
        let epoch_anchor_daa: BTreeMap<u64, u64> = creditable.iter().map(|(&e, a)| (e, a.anchor_daa_score)).collect();

        let mut contributions: Vec<AttestationContribution> = Vec::new();
        let Ok(tip_blue) = self.headers_store.get_blue_score(tip) else {
            return (contributions, epoch_anchor_daa);
        };
        for chain_block in self.reachability_service.default_backward_chain_iterator(tip) {
            if Some(chain_block) == stop_at {
                break;
            }
            let Ok(bs) = self.headers_store.get_blue_score(chain_block) else {
                break;
            };
            if tip_blue.saturating_sub(bs) > dns_params.stake_score_window_blue_score {
                break;
            }
            let txs = self.accepted_txs_of_chain_block(chain_block);
            for att in attestations_from_accepted_txs(&txs) {
                // v3 canonical gate: the attestation must name THIS chain's canonical anchor
                // for its epoch, and that epoch must be creditable (ready, non-duplicate,
                // in-window — i.e. present in `creditable`).
                let Some(anchor) = creditable.get(&att.epoch) else {
                    continue;
                };
                if att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
                    continue;
                }
                let Some(bond) = bonds.iter().find(|b| b.bond_outpoint == att.bond_outpoint) else {
                    continue;
                };
                // P-1A: the self-declared validator_id (not in the signed digest) must be
                // bound to the bond, else varying it would evade the dedup + inflate stake.
                if att.validator_id != bond.validator_pubkey_hash {
                    continue;
                }
                // The bond must be Active at the CANONICAL anchor DAA (== att.target_daa_score
                // by the gate above), not a self-reported / current value.
                if !is_bond_active_at(bond, anchor.anchor_daa_score) {
                    continue;
                }
                // audit #4: the validator-set commitment is a fixed zero (the rule the acceptance
                // gate `classify_one_attestation` enforces). An attestation carrying anything else
                // earns no credit.
                if att.validator_set_commitment != Hash64::default() {
                    continue;
                }
                let digest = stake_attestation_message(
                    net_id,
                    att.epoch,
                    att.target_hash,
                    att.target_daa_score,
                    att.validator_set_commitment,
                    att.bond_outpoint,
                )
                .as_bytes();
                if matches!(
                    verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
                    Ok(true)
                ) {
                    contributions.push(AttestationContribution {
                        epoch: att.epoch,
                        validator_id: att.validator_id,
                        bond_outpoint: att.bond_outpoint,
                        signed_weight: bond.amount as u128,
                    });
                }
            }
        }
        (contributions, epoch_anchor_daa)
    }

    /// Caches the DAA and Median time windows of the sink block (if needed). Following, virtual's window calculations will
    /// naturally hit the cache finding the sink's windows and building upon them.
    fn cache_sink_windows(
        &self,
        new_sink: BlockHash,
        prev_sink: BlockHash,
        sink_ghostdag_data: &impl Deref<Target = Arc<GhostdagData>>,
    ) {
        // We expect that the `new_sink` is cached (or some close-enough ancestor thereof) if it is equal to the `prev_sink`,
        // Hence we short-circuit the check of the keys in such cases, thereby reducing the access of the read-lock
        if new_sink != prev_sink {
            // this is only important for ibd performance, as we incur expensive cache misses otherwise.
            // this occurs because we cannot rely on header processing to pre-cache in this scenario.
            if !self.block_window_cache_for_difficulty.contains_key(&new_sink) {
                self.block_window_cache_for_difficulty
                    .insert(new_sink, self.window_manager.block_daa_window(sink_ghostdag_data.deref()).unwrap().window);
            };

            if !self.block_window_cache_for_past_median_time.contains_key(&new_sink) {
                self.block_window_cache_for_past_median_time
                    .insert(new_sink, self.window_manager.calc_past_median_time(sink_ghostdag_data.deref()).unwrap().1);
            };
        }
    }

    /// Returns the max number of tips to consider as virtual parents in a single virtual resolve operation.
    ///
    /// Guaranteed to be `>= self.max_block_parents`
    fn max_virtual_parent_candidates(&self, max_block_parents: usize) -> usize {
        // Limit to max_block_parents x 3 candidates. This way we avoid going over thousands of tips when the network isn't healthy.
        // There's no specific reason for a factor of 3, and its not a consensus rule, just an estimation for reducing the amount
        // of candidates considered.
        max_block_parents * 3
    }

    pub(super) fn sink_search_algorithm(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        bond_view: &mut ActiveBondView,
        prev_sink: BlockHash,
        tips: Vec<BlockHash>,
        finality_point: BlockHash,
        pruning_point: BlockHash,
    ) -> Result<(BlockHash, VecDeque<BlockHash>), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        // TODO (relaxed): additional tests

        // ADR-0125: a round block is never a sink. Each round tip stands for its anchor, and the set
        // is reduced to an antichain again — the heap's invariant below.
        let tips = self.palw_project_round_blocks(tips);

        // The initial diff point is the previous sink
        let mut diff_point = prev_sink;

        // Escape-from-a-dead-branch preference: consulted BEFORE the work-max search, because its
        // whole point is to select a sink the work ordering would bury. The result is still
        // UTXO-validated and still passes the reorg gate (whose ConfirmedAnchorStale arm releases
        // the dead anchor's veto), so every sink move continues to flow through one gate.
        //
        // On success the returned virtual is SINGLE-PARENT: merging the heavier dead-branch tips
        // into the mergeset would hand GHOSTDAG's selected-parent rule (max blue work) exactly the
        // branch being escaped, and the preference would undo itself. The dead tips stay unmerged;
        // if hashpower follows the live branch they are progressively orphaned, and once the live
        // branch out-works them the preference stops firing and ordinary work-max selection
        // resumes seamlessly.
        if let Some(preferred) = self.dns_stake_preferred_tip(prev_sink, &tips, finality_point) {
            diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, preferred)?;
            if diff_point == preferred && self.dns_reorg_outcome_checked(preferred, prev_sink, bond_view)?.is_accept() {
                info!(
                    "DNS stake preference: this chain's own overlay is dead (anchor stale past TTL) and tip {preferred} carries a confirmation-grade live overlay within the work-deficit bound; moving the sink there (previous sink {prev_sink}). Templates now extend the live-overlay branch."
                );
                return Ok((preferred, VecDeque::new()));
            }
            warn!(
                "DNS stake preference: preferred tip {preferred} failed UTXO validation or the reorg gate; falling back to work-max sink selection"
            );
        }

        // The heap is ordered by GHOSTDAG's own key, `SortableBlock` (blue work, then hash), on
        // every network. A PALW V2 chain is weighed by the deep-reorg gate after UTXO validation
        // (`dns_reorg_outcome`), the first point both sides of a comparison are weighable; the V1
        // heap key that ranked candidates by the overlay's bond view is gone with the V1 lineage.
        let mut heap = tips
            .into_iter()
            .map(|block| SortableBlock { hash: block, blue_work: self.ghostdag_store.get_blue_work(block).unwrap() })
            .collect::<BinaryHeap<_>>();

        // Self-wedge diagnostics (incident 2026-07-19 §2-1): the heaviest candidate the DNS gate
        // refused during this search, if any. The heap is blue-work ordered, so the first refusal
        // is the heaviest. Reported once per search when virtual settles lower than it.
        let mut gate_rejected: Option<(BlockHash, DnsReorgOutcome, BlueWorkType)> = None;

        // **Why the search failed, counted while it fails.** An exhausted search used to end in
        // `expect("valid sink must exist")`, which killed the process and named nothing: the three
        // ways a candidate leaves this loop unaccepted are told apart only by `debug!` lines, so an
        // operator whose node died at default log level had no way to say WHICH wall it hit. A
        // field report of three identical crashes (2026-09-07, `testnet-main-65e6a80e`) is what
        // this counts for. Cheap: three counters and one hash on a path that ends the search.
        let tips_searched = heap.len();
        let (mut rejected_utxo, mut rejected_finality, mut rejected_gate) = (0usize, 0usize, 0usize);
        let mut first_utxo_invalid: Option<BlockHash> = None;

        // We maintain the following invariant: `heap` is an antichain.
        // It holds at step 0 since tips are an antichain, and remains through the loop
        // since we check that every pushed block is not in the past of current heap
        // (and it can't be in the future by induction)
        loop {
            // **An exhausted heap is a diagnosis, not a reason to die.**
            //
            // The heap is seeded from `tips` and grows only by a candidate's parents that are AT OR
            // ABOVE the finality point, so it empties exactly when the walk reached the finality
            // frontier without finding one block whose UTXO state this node can compute. That is a
            // real and reachable condition — a node whose ruleset disagrees with the chain it is
            // following finds every block invalid, and an unorphaned block can put a tip on a branch
            // that does not contain `prev_sink` — and the answer to it is to stay where we are and
            // SAY SO, not to kill the process and leave a `RUST_BACKTRACE` where the reason should
            // be. Virtual holds at the previous sink; the next block re-runs the search.
            //
            // `prev_sink` is UTXO-valid by construction (it is the previous virtual's selected
            // parent, whose state was committed), so walking the diff back to it restores exactly
            // the state this function was entered with — which is what the caller's
            // `pick_virtual_parents(prev_sink, [])` and `utxo_multisets_store.get(prev_sink)` need.
            let Some(popped) = heap.pop() else {
                error!(
                    "sink search exhausted every candidate: virtual HOLDS at {prev_sink}. \
                     Searched {tips_searched} tip(s) down to finality {finality_point} (pruning {pruning_point}) and rejected \
                     {rejected_utxo} for invalid UTXO state{}, {rejected_finality} for violating finality, {rejected_gate} at the DNS reorg gate. \
                     A node that rejects every block down to finality is not following the same rules as the chain it is fed: \
                     compare the `Consensus params fingerprint` this node printed at startup against the network's, and check \
                     whether this data directory was synced by a different build.",
                    match first_utxo_invalid {
                        Some(h) => format!(" (first {h})"),
                        None => String::new(),
                    }
                );
                let restored = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, prev_sink)?;
                assert_eq!(
                    restored, prev_sink,
                    "the previous sink {prev_sink} is not UTXO-valid — virtual cannot hold anywhere, which is a corrupted store rather than a rejected chain"
                );
                return Ok((prev_sink, VecDeque::new()));
            };
            let candidate = popped.hash;
            // QR reachability hardening: skip a candidate whose reachability is missing (half-pruned)
            // instead of panicking; it is below finality and recovery will complete the prune. Consensus-neutral.
            let candidate_at_or_above_finality = match self.reachability_service.try_is_chain_ancestor_of(finality_point, candidate) {
                Ok(v) => v,
                Err(_) => {
                    debug!(
                        "sink_search: candidate {candidate} has no reachability vs finality {finality_point} (half-pruned?); skipping"
                    );
                    false
                }
            };
            if candidate_at_or_above_finality {
                diff_point = self.calculate_utxo_state_relatively(stores, diff, bond_view, diff_point, candidate)?;
                if diff_point == candidate {
                    // This indicates that candidate has valid UTXO state and that `diff` represents its diff from virtual

                    // kaspa-pq Phase 10 (ADR-0009): the DNS finality reorg gate. Inert
                    // unless the overlay is configured and in the Active stage; it then
                    // rejects a candidate that would abandon a DNS-confirmed anchor. The
                    // rejection is soft — we fall through to push the candidate's parents
                    // and continue, converging on a DNS-valid sink (mirrors the
                    // invalid-UTXO handling below).
                    let dns_outcome = self.dns_reorg_outcome_checked(candidate, prev_sink, bond_view)?;
                    if dns_outcome.is_accept() {
                        // Self-wedge signal (incident 2026-07-19 §2-1). The 7/19 freeze ran 3.5h
                        // with ZERO warnings: the gate refused every block of the network's chain
                        // while the node believed it was correctly repelling a deep reorg, and the
                        // only way to notice was comparing DAA against a peer by hand. Emitting
                        // this at the point virtual settles — once per search, not per candidate —
                        // keeps a healthy node quiet while making a wedged one impossible to miss.
                        let rule_e = self.palw_rule_e_active_at(self.headers_store.get_daa_score(prev_sink).unwrap_or(0));
                        if !rule_e {
                            self.note_palw_refusal_streak(
                                candidate,
                                gate_rejected.map(|(rejected, reason, _)| (rejected, reason)),
                                false,
                            );
                        }
                        if let Some((rejected, reason, rejected_work)) = gate_rejected {
                            warn!(
                                "DNS reorg gate: virtual settled on sink {} (blue_work {}) after refusing the heavier candidate {} (blue_work {}, reason {:?}{}). If this repeats on every resolve, this node is wedged off the network's chain — compare DAA against a peer.",
                                candidate,
                                self.ghostdag_store.get_blue_work(candidate).unwrap_or_default(),
                                rejected,
                                rejected_work,
                                reason,
                                self.palw_reorg_refusal_explained_v1(candidate, rejected),
                            );
                        }
                        // All blocks with lower blue work than filtering_root are:
                        // 1. not in its future (bcs blue work is monotonic),
                        // 2. will be removed eventually by the bounded merge check.
                        // Hence as an optimization we prefer removing such blocks in advance to allow valid tips to be considered.
                        let filtering_root = self.depth_store.merge_depth_root(candidate).unwrap();
                        let filtering_blue_work = self.ghostdag_store.get_blue_work(filtering_root).unwrap_or_default();
                        // **ADR-0178: past rule E's fence (at the incumbent's DAA) the search goes on** for a bounded number of
                        // lighter candidates and keeps the best over the exclusive pasts (`palw_rule_e_finish_search_v1`); where the
                        // first stays best its answer is this one.
                        // LIVE-R1 N2's record is then taken at the sink rule E settles on, and a refusal counts as weighed when
                        // rule E weighed the pair and the refused tip does not outrank it.
                        if rule_e {
                            let (sink, parents, _) = self.palw_rule_e_finish_search_v1(
                                stores,
                                diff,
                                bond_view,
                                prev_sink,
                                candidate,
                                heap,
                                diff_point,
                                finality_point,
                            )?;
                            self.note_palw_refusal_streak(sink, gate_rejected.map(|(rejected, reason, _)| (rejected, reason)), true);
                            return Ok((sink, parents));
                        }
                        // The heap is GHOSTDAG's own order, so the sink is the maximum of every
                        // candidate left in it and `pick_virtual_parents`' assumption holds as is.
                        return Ok((
                            candidate,
                            heap.into_sorted_iter().take_while(|s| s.blue_work >= filtering_blue_work).map(|s| s.hash).collect(),
                        ));
                    }
                    if gate_rejected.is_none() {
                        gate_rejected =
                            Some((candidate, dns_outcome, self.ghostdag_store.get_blue_work(candidate).unwrap_or_default()));
                    }
                    rejected_gate += 1;
                    debug!(
                        "Block candidate {} rejected by the DNS finality reorg gate ({:?}); ignored from Virtual chain.",
                        candidate, dns_outcome
                    );
                } else {
                    rejected_utxo += 1;
                    first_utxo_invalid.get_or_insert(candidate);
                    debug!("Block candidate {} has invalid UTXO state and is ignored from Virtual chain.", candidate)
                }
            } else if finality_point != pruning_point {
                rejected_finality += 1;
                // `finality_point == pruning_point` indicates we are at IBD start hence no warning required
                warn!("Finality Violation Detected. Block {} violates finality and is ignored from Virtual chain.", candidate);
            } else {
                // IBD start (`finality_point == pruning_point`): no warning, but it is still a
                // rejection, and the exhaustion report is the only place it is ever counted.
                rejected_finality += 1;
            }
            // PRUNE SAFETY: see comment within [`resolve_virtual`]
            let prune_guard = self.pruning_lock.blocking_read();
            for parent in self.relations_service.get_parents(candidate).unwrap().iter().copied() {
                // ADR-0125: a round parent stands for its anchor, which is on the candidate's chain.
                let parent = self.palw_project_round_block(parent);
                if self.reachability_service.is_dag_ancestor_of(finality_point, parent)
                    && !self.reachability_service.is_dag_ancestor_of_any(parent, &mut heap.iter().map(|sb| sb.hash))
                {
                    heap.push(SortableBlock { hash: parent, blue_work: self.ghostdag_store.get_blue_work(parent).unwrap() });
                }
            }
            drop(prune_guard);
        }
    }

    /// Picks the virtual parents according to virtual parent selection pruning constrains.
    /// Assumes:
    ///     1. `selected_parent` is a UTXO-valid block
    ///     2. `candidates` are an antichain ordered in descending blue work order
    ///     3. `candidates` do not contain `selected_parent` and `selected_parent.blue work > max(candidates.blue_work)`  
    pub(super) fn pick_virtual_parents(
        &self,
        selected_parent: BlockHash,
        mut candidates: VecDeque<BlockHash>,
        pruning_point: BlockHash,
        // ADR-0125: the round tips, offered after the chain's own candidates. Empty where the lane
        // is not configured.
        round_tips: Vec<BlockHash>,
    ) -> (Vec<BlockHash>, GhostdagData) {
        // TODO (relaxed): additional tests

        // Mergeset increasing might traverse DAG areas which are below the finality point and which theoretically
        // can borderline with pruned data, hence we acquire the prune lock to ensure data consistency. Note that
        // the final selected mergeset can never be pruned (this is the essence of the prunality proof), however
        // we might touch such data prior to validating the bounded merge rule. All in all, this function is short
        // enough so we avoid making further optimizations
        let _prune_guard = self.pruning_lock.blocking_read();
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();
        let max_block_parents = self.max_block_parents.get(parent_daa) as usize;
        let mergeset_size_limit = self.mergeset_size_limit.get(parent_daa);
        let max_candidates = self.max_virtual_parent_candidates(max_block_parents);

        // Prioritize half the blocks with highest blue work and pick the rest randomly to ensure diversity between nodes
        if candidates.len() > max_candidates {
            // make_contiguous should be a no op since the deque was just built
            let slice = candidates.make_contiguous();

            // Keep slice[..max_block_parents / 2] as is, choose max_candidates - max_block_parents / 2 in random
            // from the remainder of the slice while swapping them to slice[max_block_parents / 2..max_candidates].
            //
            // Inspired by rand::partial_shuffle (which lacks the guarantee on chosen elements location).
            for i in max_block_parents / 2..max_candidates {
                let j = rand::thread_rng().gen_range(i..slice.len()); // i < max_candidates < slice.len()
                slice.swap(i, j);
            }

            // Truncate the unchosen elements
            candidates.truncate(max_candidates);
        } else if candidates.len() > max_block_parents / 2 {
            // Fallback to a simpler algo in this case
            candidates.make_contiguous()[max_block_parents / 2..].shuffle(&mut rand::thread_rng());
        }

        let mut virtual_parents = Vec::with_capacity(min(max_block_parents, candidates.len() + 1));
        virtual_parents.push(selected_parent);
        let mut mergeset_size = 1; // Count the selected parent

        // ADR-0068 Phase 1 (F3a/F5): the heartbeat width tracking, `None` where the lane is not
        // armed (no header can be a heartbeat there, so the set is vacuously empty and the
        // per-member header reads are skipped). The selected parent is in the mergeset, so it
        // seeds the set. Admissibility is decided over the WHOLE accumulated set — flat bound,
        // or the F5 chain exemption — mirroring `check_mergeset_heartbeat_width` exactly, so a
        // template never builds what consensus refuses and never refuses what consensus admits.
        let track_heartbeats = self.palw_heartbeat_width_fence.is_some();
        let mut heartbeat_set: Vec<(u64, BlockHash, u64)> = Vec::new();
        // H3: whether the chain exemption must also be paced. Judged at the selected parent's score
        // PLUS ONE — the highest score the block this template becomes can carry — so a template at
        // the fence applies the rule wherever the header stage might, and never builds a block it
        // refuses.
        let mut paced = false;
        if track_heartbeats {
            let sp = self.headers_store.get_header(selected_parent).unwrap();
            if sp.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
                heartbeat_set.push((sp.blue_score, selected_parent, sp.timestamp));
            }
            paced = self.palw_clock_floor.is_some_and(|fence| fence.is_active(sp.daa_score.saturating_add(1)));
        }

        // ADR-0125: one parent slot is kept for the round lane while it has a tip to offer, so a DAG
        // wide with chain tips cannot starve the lane of every merge.
        let round_reserve = usize::from(!round_tips.is_empty() && max_block_parents > 1);
        // Try adding parents as long as mergeset size and number of parents limits are not reached
        while let Some(candidate) = candidates.pop_front() {
            if mergeset_size >= mergeset_size_limit || virtual_parents.len() >= max_block_parents - round_reserve {
                break;
            }
            match self.mergeset_increase(&virtual_parents, candidate, mergeset_size_limit - mergeset_size, track_heartbeats) {
                MergesetIncreaseResult::Accepted { increase_size, heartbeat_members } => {
                    if !heartbeat_members.is_empty() {
                        let mut combined = heartbeat_set.clone();
                        combined.extend_from_slice(&heartbeat_members);
                        if !self.heartbeat_set_admissible(&mut combined, paced) {
                            // Over the flat bound and not one chain: this candidate widens the
                            // heartbeat lane past what consensus admits. Nothing to substitute —
                            // skip it; a later template absorbs it against a fresh mergeset.
                            continue;
                        }
                        heartbeat_set = combined;
                    }
                    mergeset_size += increase_size;
                    virtual_parents.push(candidate);
                }
                MergesetIncreaseResult::Rejected { new_candidate } => {
                    // If we already have a candidate in the past of new candidate then skip.
                    if self.reachability_service.is_any_dag_ancestor(&mut candidates.iter().copied(), new_candidate) {
                        continue; // TODO (optimization): not sure this check is needed if candidates invariant as antichain is kept
                    }
                    // Remove all candidates which are in the future of the new candidate
                    candidates.retain(|&h| !self.reachability_service.is_dag_ancestor_of(new_candidate, h));
                    candidates.push_back(new_candidate);
                }
            }
        }
        assert!(mergeset_size <= mergeset_size_limit);
        assert!(virtual_parents.len() <= max_block_parents);
        self.palw_add_round_parents(&mut virtual_parents, selected_parent, round_tips, max_block_parents);
        self.remove_bounded_merge_breaking_parents(virtual_parents, pruning_point)
    }

    fn mergeset_increase(
        &self,
        selected_parents: &[BlockHash],
        candidate: BlockHash,
        budget: u64,
        track_heartbeats: bool,
    ) -> MergesetIncreaseResult {
        /*
        Algo:
            Traverse past(candidate) \setminus past(selected_parents) and make
            sure the increase in mergeset size is within the available budget —
            and, where the heartbeat lane is fenced in (ADR-0068 Phase 1 / F3a),
            hand the increase's heartbeat members back for the caller's whole-set
            width decision (F5's chain exemption is a property of the final set,
            not of one candidate's increase).
        */

        // `false` = the lane is not armed on this network, so no header can be a heartbeat and
        // the set is vacuously empty — skip the per-member header reads entirely.
        let mut heartbeat_members: Vec<(u64, BlockHash, u64)> = Vec::new();
        let mut note_heartbeat = |hash: BlockHash| {
            if !track_heartbeats {
                return;
            }
            let header = self.headers_store.get_header(hash).unwrap();
            if header.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
                heartbeat_members.push((header.blue_score, hash, header.timestamp));
            }
        };
        note_heartbeat(candidate);

        let candidate_parents = self.relations_service.get_parents(candidate).unwrap();
        let mut queue: VecDeque<_> = candidate_parents.iter().copied().collect();
        let mut visited: BlockHashSet = queue.iter().copied().collect();
        let mut mergeset_increase = 1u64; // Starts with 1 to count for the candidate itself

        while let Some(current) = queue.pop_front() {
            if self.reachability_service.is_dag_ancestor_of_any(current, &mut selected_parents.iter().copied()) {
                continue;
            }
            mergeset_increase += 1;
            if mergeset_increase > budget {
                return MergesetIncreaseResult::Rejected { new_candidate: current };
            }
            note_heartbeat(current);

            let current_parents = self.relations_service.get_parents(current).unwrap();
            for &parent in current_parents.iter() {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        MergesetIncreaseResult::Accepted { increase_size: mergeset_increase, heartbeat_members }
    }

    /// **The width rule's answer, template-side** (ADR-0068 Phase 1, F3a with F5's chain
    /// exemption) — the same predicate `check_mergeset_heartbeat_width` enforces: at most
    /// `PALW_HEARTBEAT_MAX_PER_MERGESET` heartbeats, or any number of them provided they form
    /// ONE chain (sorted by blue score, each adjacent pair ancestor-related; a blue-score tie
    /// is never ancestor-related and fails) — and, where `paced` (H3, past `palw_clock_floor`),
    /// no more than the chain's own timestamps pay for (`heartbeat_chain_capacity_v1`). Elements
    /// are `(blue_score, hash, timestamp)`. Sorts the given buffer in place.
    fn heartbeat_set_admissible(&self, set: &mut [(u64, BlockHash, u64)], paced: bool) -> bool {
        let bound = kaspa_consensus_core::pow_layer0::PALW_HEARTBEAT_MAX_PER_MERGESET;
        if set.len() as u64 <= bound {
            return true;
        }
        set.sort_unstable_by_key(|(blue_score, _, _)| *blue_score);
        let one_chain = set.windows(2).all(|pair| {
            let ((_, older, _), (_, newer, _)) = (pair[0], pair[1]);
            self.reachability_service.is_dag_ancestor_of(older, newer)
        });
        if !one_chain || !paced {
            return one_chain;
        }
        let (oldest, newest) =
            set.iter().fold((u64::MAX, 0u64), |(lo, hi), (_, _, timestamp)| (lo.min(*timestamp), hi.max(*timestamp)));
        set.len() as u64 <= kaspa_consensus_core::palw_heartbeat_v1::heartbeat_chain_capacity_v1(newest.saturating_sub(oldest), bound)
    }

    fn remove_bounded_merge_breaking_parents(
        &self,
        mut virtual_parents: Vec<BlockHash>,
        current_pruning_point: BlockHash,
    ) -> (Vec<BlockHash>, GhostdagData) {
        let mut ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(&ghostdag_data, current_pruning_point);
        // Find red blocks violating the merge bound and which are not kosherized by any blue (the validator's own predicate).
        let bad_reds = self.depth_manager.merge_breaking_reds(&ghostdag_data, merge_depth_root);

        if !bad_reds.is_empty() {
            // Remove all parents which lead to merging a bad red
            virtual_parents.retain(|&h| !self.reachability_service.is_any_dag_ancestor(&mut bad_reds.iter().copied(), h));
            // Recompute ghostdag data since parents changed
            ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        }

        (virtual_parents, ghostdag_data)
    }

    fn validate_mempool_transaction_impl(
        &self,
        mutable_tx: &mut MutableTransaction,
        virtual_utxo_view: &impl UtxoView,
        virtual_daa_score: u64,
        virtual_past_median_time: u64,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        self.transaction_validator.validate_tx_in_isolation(&mutable_tx.tx)?;
        self.transaction_validator.validate_tx_in_header_context_with_args(
            &mutable_tx.tx,
            virtual_daa_score,
            virtual_past_median_time,
        )?;
        // ADR-0109 Decision 3: a spend of a PALW bond the registry holds locked at the virtual tip is
        // refused here, with the merge's own error, instead of being carried by a block and skipped
        // at the merge where nobody hears it. The same set the acceptance path builds
        // (`palw_v2_locked_bonds`), read at the tip the mempool judges against. Its burn half — a
        // released, slashed bond's spend must leave what the bond lost unclaimed — needs the input
        // amounts, so it is asked in the UTXO context below, before the script check, with the
        // block path's own rule (`palw_bond_burn_paid`).
        let bond_gate = self.palw_mempool_bond_gate(virtual_daa_score);
        if let Some(gate) = bond_gate.as_ref()
            && let Some(outpoint) = first_locked_input(&mutable_tx.tx, &gate.locked)
        {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::SpendsNonReleasableBond(outpoint));
        }
        // **The 2026-09-23 Position route matrix, P-B3:** a seed or a buy the fold would refuse on its
        // class's registry lifecycle is refused here, before it relays; the template asks again
        // (`validate_block_template_transaction`), because the lifecycle can move after admission.
        if let Some(refusal) = self.palw_mempool_market_refusal(&mutable_tx.tx, virtual_daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwModelMarketNotEligible(refusal));
        }
        // **MSK-26A: this fleet never relays a DNS slashing/precommit evidence that is not genuine at
        // the tip** — a forged one merged into a block burns an honest validator's staked UTXO until
        // `palw_slashing_evidence_utxo_genuine` is armed. Node policy, not a block rule.
        if let Some(refusal) = self.palw_slashing_evidence_mempool_refusal(&mutable_tx.tx, virtual_daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwSlashingEvidenceNotGenuine(refusal));
        }
        self.validate_mempool_transaction_in_utxo_context(
            mutable_tx,
            virtual_utxo_view,
            virtual_daa_score,
            args,
            bond_gate.as_ref().map(|gate| &gate.burns),
            bond_gate.as_ref().map(|gate| &gate.escrow),
        )?;
        // **ADR-0152 H-1 (P2-9 review, finding 5):** an H-1 carrier the fold would refuse at the tip
        // is refused here, AFTER the UTXO context, so only a funded, signed carrier costs this node a
        // rehearsal — and the template asks again (`validate_block_template_transaction`).
        if let Some(refusal) = self.palw_mempool_h1_carrier_refusal(&mutable_tx.tx, virtual_daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwH1CarrierRefused(refusal));
        }
        // **G14 GAP 10:** a kernel route carrier the acceptance gate refuses is refused here too (and at the template).
        if let Some(refusal) = self.palw_mempool_kernel_route_refusal(&mutable_tx.tx, virtual_daa_score) {
            return Err(kaspa_consensus_core::errors::tx::TxRuleError::PalwKernelRouteRefused(refusal));
        }
        Ok(())
    }

    /// [`Self::validate_mempool_transaction`] as if the virtual stood at `virtual_daa_score` — the
    /// virtual UTXO set, median time and PALW tip as they are, only the DAA the policy reads moved.
    /// Test-only: ADR-0152 T23/T05 ask the mempool's bond gate at the DAAs B-3 turns on, and T25
    /// asks the coinbase-maturity policy at a minted leg's `M + 599` and `M + 600`, without mining
    /// the six hundred blocks between.
    #[cfg(test)]
    pub(super) fn validate_mempool_transaction_at_daa_for_tests(
        &self,
        mutable_tx: &mut MutableTransaction,
        virtual_daa_score: u64,
    ) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        self.validate_mempool_transaction_impl(
            mutable_tx,
            &virtual_read.utxo_set,
            virtual_daa_score,
            virtual_state.past_median_time,
            &Default::default(),
        )
    }

    pub fn validate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;
        let virtual_daa_score = virtual_state.daa_score;
        let virtual_past_median_time = virtual_state.past_median_time;
        // Run within the thread pool since par_iter might be internally applied to inputs
        self.thread_pool.install(|| {
            self.validate_mempool_transaction_impl(mutable_tx, virtual_utxo_view, virtual_daa_score, virtual_past_median_time, args)
        })
    }

    pub fn validate_mempool_transactions_in_parallel(
        &self,
        mutable_txs: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;
        let virtual_daa_score = virtual_state.daa_score;
        let virtual_past_median_time = virtual_state.past_median_time;

        self.thread_pool.install(|| {
            mutable_txs
                .par_iter_mut()
                .map(|mtx| {
                    self.validate_mempool_transaction_impl(
                        mtx,
                        &virtual_utxo_view,
                        virtual_daa_score,
                        virtual_past_median_time,
                        args.get(&mtx.id()),
                    )
                })
                .collect::<Vec<TxResult<()>>>()
        })
    }

    fn populate_mempool_transaction_impl(
        &self,
        mutable_tx: &mut MutableTransaction,
        virtual_utxo_view: &impl UtxoView,
    ) -> TxResult<()> {
        self.populate_mempool_transaction_in_utxo_context(mutable_tx, virtual_utxo_view)?;
        Ok(())
    }

    pub fn populate_mempool_transaction(&self, mutable_tx: &mut MutableTransaction) -> TxResult<()> {
        let virtual_read = self.virtual_stores.read();
        let virtual_utxo_view = &virtual_read.utxo_set;
        self.populate_mempool_transaction_impl(mutable_tx, virtual_utxo_view)
    }

    pub fn populate_mempool_transactions_in_parallel(&self, mutable_txs: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        let virtual_read = self.virtual_stores.read();
        let virtual_utxo_view = &virtual_read.utxo_set;
        self.thread_pool.install(|| {
            mutable_txs
                .par_iter_mut()
                .map(|mtx| self.populate_mempool_transaction_impl(mtx, &virtual_utxo_view))
                .collect::<Vec<TxResult<()>>>()
        })
    }

    /// **Re-commit the EVM lane for a heartbeat the adapter stamped later than its builder did.**
    ///
    /// The lane executes a chain block's mergeset acceptance against the header's timestamp in
    /// whole seconds (`kaspa_evm::env::derive_env`), so a stamp in the same second as the builder's
    /// changes nothing and anything else changes `evm_commitment_root` — and, through the bridge's
    /// withdrawals, possibly `utxo_commitment`. Both are re-derived by the builder's OWN step,
    /// [`Self::evm_template_fields`], over the virtual the template was built on and the payload it
    /// already carries: construction and validation keep running one function.
    ///
    /// Refused, never guessed, when that virtual is gone (the template is stale and the miner
    /// rebuilds) or when the re-run would carry a different payload (a claim that went stale in
    /// between). Inert below the lane's activation, which covers every build without the `evm`
    /// feature: such a build cannot produce an EVM-active template at all.
    fn heartbeat_recommit_evm_for_stamp(&self, template: &mut BlockTemplate, built_at: u64) -> Result<(), RuleError> {
        let header = &template.block.header;
        if header.daa_score < self.evm_activation_daa_score || header.timestamp / 1000 == built_at / 1000 {
            return Ok(());
        }
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let built_on_this_virtual = virtual_state.daa_score == header.daa_score
            && virtual_state.ghostdag_data.selected_parent == template.selected_parent_hash
            && virtual_state.parents.len() == header.direct_parents().len()
            && virtual_state.parents.iter().all(|p| header.direct_parents().contains(p));
        if !built_on_this_virtual {
            return Err(RuleError::EvmTemplateExecutionFailed(format!(
                "the heartbeat adapter stamped this template for its slot ({built_at} -> {}), which moves its EVM commitment, and \
                 the virtual it was built on has moved since — build a fresh template",
                header.timestamp
            )));
        }
        let payload = &template.block.evm_payload;
        let claims: Vec<kaspa_consensus_core::evm::DepositClaim> = payload
            .system_ops
            .iter()
            .filter_map(|op| match op {
                kaspa_consensus_core::evm::EvmSystemOp::DepositClaim(claim) => Some(claim.clone()),
                kaspa_consensus_core::evm::EvmSystemOp::MarketSettle(_) => None,
            })
            .collect();
        let prepared_claims = crate::processes::evm::prepare_deposit_claims(&claims, &virtual_read.utxo_set, virtual_state.daa_score);
        drop(virtual_read);
        let evm_template_data = kaspa_consensus_core::evm::EvmTemplateData {
            evm_coinbase: payload.evm_coinbase,
            transactions: payload.transactions.clone(),
            system_ops: claims,
        };
        // The builder's starting point: the virtual's multiset, before the lane folds the bridge's
        // effects in (`evm_template_fields` re-folds them when there are any).
        let mut restamped = template.block.header.clone();
        restamped.utxo_commitment = virtual_state.multiset.clone().finalize();
        let (restamped, payload, stale_claims) =
            self.evm_template_fields(restamped, &virtual_state, evm_template_data, prepared_claims)?;
        if payload != template.block.evm_payload || !stale_claims.is_empty() {
            return Err(RuleError::EvmTemplateExecutionFailed(format!(
                "the heartbeat adapter stamped this template for its slot ({built_at} -> {}), and the EVM payload it carries no longer \
                 re-derives on its virtual — build a fresh template",
                restamped.timestamp
            )));
        }
        template.block.header = restamped;
        Ok(())
    }

    /// **ADR-0105 Decision 2: whether a heartbeat miner should stand aside for a bonded block.**
    ///
    /// Node policy, not a rule — nothing validates against it, and the slot rule the template
    /// adapter applies is unchanged. It answers from the current virtual: the selected parent's lane,
    /// and the lane and timestamp of every other block the virtual merges (the tips a template would
    /// carry as parents, and whatever they bring that the sink has not merged yet). One block deep,
    /// the slot rule's shape. See [`kaspa_consensus_core::palw_heartbeat_v1::heartbeat_yield_hint_v1`].
    ///
    /// **Every failure answers `NothingToYieldTo`**, i.e. "mine as before": a missing header here is
    /// a node-local fact, and the one thing this hint must never do is hold the clock on one.
    ///
    /// **Past `palw_clock_cursor` it answers `SlotTaken(next)` while the next slot is still in the
    /// future** (the 2026-09-24 heartbeat audit, H1): the clock has already advanced into the slot a
    /// beat minted now would claim, so the beat could earn nothing. The miner waits for `next`
    /// instead of grinding a block that weighs ε, adds a blue score and ticks no DAA.
    pub fn heartbeat_yield_hint(&self) -> kaspa_consensus_core::palw_heartbeat_v1::HeartbeatYieldHintV1 {
        self.heartbeat_yield_hint_at(unix_now())
    }

    /// [`Self::heartbeat_yield_hint`] at an explicit wall clock — the node passes `unix_now()`, a
    /// test passes its simulated time.
    pub fn heartbeat_yield_hint_at(&self, now_ms: u64) -> kaspa_consensus_core::palw_heartbeat_v1::HeartbeatYieldHintV1 {
        use kaspa_consensus_core::palw_heartbeat_v1 as hb;
        let virtual_state = self.virtual_stores.read().state.get().unwrap();
        if !self.palw_heartbeat_lane.is_some_and(|fence| fence.is_active(virtual_state.daa_score)) {
            return hb::HeartbeatYieldHintV1::NothingToYieldTo;
        }
        let ghostdag = &virtual_state.ghostdag_data;
        let Ok(selected_parent) = self.headers_store.get_header(ghostdag.selected_parent) else {
            return hb::HeartbeatYieldHintV1::NothingToYieldTo;
        };
        let merged: Vec<(u8, u64)> = ghostdag
            .unordered_mergeset_without_selected_parent()
            .filter_map(|hash| self.headers_store.get_header(hash).ok().map(|header| (header.pow_algo_id, header.timestamp)))
            .collect();
        // ADR-0138 §3c: the same question the slot rule asks — is someone else pacing the clock?
        // A miner that kept the old hint would answer "bonded parent, sleep an hour" to every
        // attempt-lane parent past the fence, on a chain where nothing else advances the DAA.
        let anchor_clock_active = self.palw_anchor_clock.is_some_and(|fence| fence.is_active(virtual_state.daa_score));
        let parent_advances_daa = crate::processes::difficulty::palw_lane_advances_daa_v1(
            selected_parent.pow_algo_id,
            selected_parent.daa_score,
            self.palw_anchor_clock,
            self.palw_single_lottery,
            self.palw_receipt_rows_unpriced,
        );
        let attempt_advances_daa = crate::processes::difficulty::palw_lane_advances_daa_v1(
            kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            virtual_state.daa_score,
            self.palw_anchor_clock,
            self.palw_single_lottery,
            self.palw_receipt_rows_unpriced,
        );
        let yield_hint = hb::heartbeat_yield_hint_v2(
            selected_parent.pow_algo_id,
            anchor_clock_active,
            parent_advances_daa,
            attempt_advances_daa,
            merged,
        );
        // H1: the virtual's own clock decision — the one the next block's DAA score will be
        // computed from. A failure to read it is a node-local fact and answers as before.
        let clock = if self.palw_clock_cursor.is_some_and(|fence| fence.is_active(virtual_state.daa_score)) {
            self.window_manager.block_daa_window(ghostdag).ok().map(|window| window.clock)
        } else {
            None
        };
        hb::heartbeat_slot_hint_v1(yield_hint, clock.as_ref(), now_ms)
    }

    /// Make sure pruning point-related stores are initialized
    pub fn init(self: &Arc<Self>) {
        let pruning_point_read = self.pruning_point_store.upgradable_read();
        if pruning_point_read.pruning_point().optional().unwrap().is_none() {
            let mut pruning_point_write = RwLockUpgradableReadGuard::upgrade(pruning_point_read);
            let mut pruning_meta_write = self.pruning_meta_stores.write();
            let mut batch = WriteBatch::default();
            self.past_pruning_points_store.insert_batch(&mut batch, 0, self.genesis.hash).idempotent().unwrap();
            pruning_point_write.set_batch(&mut batch, self.genesis.hash, 0).unwrap();
            pruning_point_write.set_retention_checkpoint(&mut batch, self.genesis.hash).unwrap();
            pruning_point_write.set_retention_period_root(&mut batch, self.genesis.hash).unwrap();
            pruning_meta_write.set_utxoset_position(&mut batch, self.genesis.hash).unwrap();
            self.db.write(batch).unwrap();
            drop(pruning_point_write);
            drop(pruning_meta_write);
        }
        // ADR-0109 Decision 1: a database from before the deposit-lock index builds it once from the
        // virtual UTXO set it already holds; from then on the diff keeps it.
        let built = self.evm_deposit_lock_store.read().is_built();
        if !built {
            let virtual_read = self.virtual_stores.read();
            self.rebuild_evm_deposit_lock_index(&virtual_read.utxo_set);
        }
    }

    /// Initializes UTXO state of genesis and points virtual at genesis.
    /// Note that pruning point-related stores are initialized by `init`
    pub fn process_genesis(self: &Arc<Self>) -> Result<(), kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        // ADR-0042 Decision 5 / Unit C: the V2 state's zero point. Installed beside the genesis
        // UTXO commit so a fresh database has a tip to walk from, and skipped entirely on a
        // network with no V2 bundle (every shipped preset), where the store stays empty.
        let palw_genesis = self
            .palw_state_params_v2
            .as_ref()
            .map(|state_params| {
                // The genesis block APPLIES the bundle's registration list. A V2 network has no class
                // and no bond until something registers them, and the only block that can is this one:
                // admission refuses an attempt naming a bond the chain does not have, so a genesis
                // that registers nothing produces a network that boots and then cannot make a block.
                // Found by measurement — the harness wedged exactly there the moment admission was
                // wired, with every block refused for a bond that existed nowhere.
                let objects = &self.palw_genesis_objects_v2;
                let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
                    block: self.genesis.hash,
                    daa_score: self.genesis.daa_score,
                    blue_score: 0,
                    // Genesis funds no escrow: its coinbase pays the premine, and its object list
                    // registers — it creates no claim, so there is nothing for a carve to attach to.
                    subsidy: 0,
                };
                let (genesis_state, delta) = kaspa_consensus_core::palw_state_v2::apply_palw_transition_retained_v2_with_extras(
                    &kaspa_consensus_core::palw_state_v2::PalwChainStateV2::genesis(),
                    state_params,
                    &point,
                    &objects,
                    None,
                    self.palw_unavailable_abstains_at(self.genesis.daa_score),
                    self.palw_capability_bound_at(self.genesis.daa_score),
                    self.palw_uncertified_weightless_at(self.genesis.daa_score),
                    self.palw_da_court_at(self.genesis.daa_score),
                    // The genesis block opens no court session, so this cannot change what it folds —
                    // and "cannot change anything" is exactly how a fence resolved differently in one
                    // face survives until the block where it matters.
                    &self.palw_transition_extras_for(&point),
                )
                .map_err(|why| match why {
                    kaspa_consensus_core::palw_state_v2::PalwStateV2Error::NativeResource(capacity) => capacity,
                    other => {
                        panic!("the bundle's genesis registrations must apply — validate_palw_v2 ran them at construction: {other}")
                    }
                })?;
                let (delta, delta_lease) = delta.into_parts_v1();
                // Admission declarations are raw canonical metadata here, not
                // decoded tensor artifacts. Count their exact canonical vectors
                // independently of the state's native heap. Covers carriage Vec,
                // bincode record and batch copies (including old/new Vec buffers)
                // plus concrete outer Vec/Arc/record framing before allocation.
                let mut carriage_requests = 0u64;
                for object in objects {
                    if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                        admission: Some(carriage),
                        ..
                    } = object
                    {
                        let bytes = borsh::object_length(carriage.as_ref()).map_err(|e| {
                            kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                                kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
                            >("genesis declaration canonical length", &e.to_string())
                        })? as u64;
                        let frames =
                            8 * std::mem::size_of::<(Hash64, crate::model::stores::palw_class_carriage::PalwClassCarriageRecord)>()
                                as u64
                                + 2 * std::mem::size_of::<crate::model::stores::palw_class_carriage::PalwClassCarriageRecord>() as u64
                                + 2 * std::mem::size_of::<[usize; 2]>() as u64;
                        carriage_requests = bytes
                            .checked_mul(6)
                            .and_then(|n| n.checked_add(frames))
                            .and_then(|n| n.checked_add(carriage_requests))
                            .ok_or_else(|| {
                                kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::overflow::<
                                    kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
                                >("genesis declaration workspace")
                            })?;
                    }
                }
                let carriage_lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_requests_v1(carriage_requests)?;
                let mut genesis_carriages = Vec::new();
                for object in objects {
                    if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                        class_id,
                        artifact_root,
                        admission: Some(carriage),
                        ..
                    } = object
                    {
                        let accepted = genesis_state.class(class_id).expect("the genesis class registration was accepted");
                        assert_eq!(accepted.artifact_root, *artifact_root, "genesis declaration must name the accepted artifact");
                        assert_eq!(accepted.registered_daa, point.daa_score);
                        assert_eq!(
                            carriage.profile.shape_profile_id(),
                            *class_id,
                            "genesis declaration must authenticate its class profile"
                        );
                        assert_eq!(carriage.canonical.shape_profile_id, *class_id, "genesis canonical job must name the same profile");
                        genesis_carriages.push((
                            *class_id,
                            crate::model::stores::palw_class_carriage::PalwClassCarriageRecord {
                                registered_daa: accepted.registered_daa,
                                artifact_root: accepted.artifact_root,
                                carriage: {
                                    let length =
                                        borsh::object_length(carriage.as_ref()).expect("already counted canonical declaration");
                                    let mut bytes = Vec::with_capacity(length);
                                    borsh::to_writer(&mut bytes, carriage.as_ref()).expect("accepted declaration serializes");
                                    bytes
                                },
                            },
                        ));
                    }
                }
                Ok(PalwUtxoCommitV2 {
                    root: genesis_state.state_root(),
                    delta,
                    _native_delta_lease: delta_lease,
                    _native_genesis_carriages_lease: carriage_lease,
                    genesis: Some(genesis_state),
                    genesis_carriages,
                })
            })
            .transpose()?;

        // Derive the complete registration transition before any UTXO-valid write.
        // Unlike later tips, genesis has no older materialization from which a
        // restart can replay a missing tip. Its delta, tip and UTXO-valid marker
        // therefore commit in one RocksDB batch, including on a process kill.
        self.commit_utxo_state(
            self.genesis.hash,
            UtxoDiff::default(),
            MuHash::new(),
            AcceptanceData::default(),
            ZERO_HASH64,
            Vec::new(),
            0,    // kaspa-pq ADR-0018 "本格版": genesis has no validator quality sub-pool.
            0,    // kaspa-pq ADR-0018 "本格版" (Phase 4): genesis reserve balance is 0.
            None, // kaspa-pq ADR-0020 v0.4: genesis is EVM-inert (v0 header).
            palw_genesis,
        )?;

        // Init the virtual selected chain store
        let mut batch = WriteBatch::default();
        let mut selected_chain_write = self.selected_chain_store.write();
        selected_chain_write.init_with_pruning_point(&mut batch, self.genesis.hash).unwrap();
        self.db.write(batch).unwrap();
        drop(selected_chain_write);

        // Init virtual state
        self.commit_virtual_state(
            self.virtual_stores.upgradable_read(),
            Arc::new(VirtualState::from_genesis(&self.genesis, self.ghostdag_manager.ghostdag(&[self.genesis.hash]))),
            &Default::default(),
            &Default::default(),
        );
        Ok(())
    }

    /// Import recovery barriers must survive loss of the process and its page
    /// cache. In particular, an asynchronous incomplete marker is insufficient
    /// before clearing a previously stable UTXO set. The closing barrier also
    /// flushes preceding sidecar/copy writes to this DB's WAL before publishing
    /// the stable generation. A failed barrier is fatal: staged store caches
    /// cannot be served after an unsuccessful durable commit.
    pub(crate) fn write_pruning_import_barrier(&self, batch: WriteBatch) {
        let mut options = WriteOptions::default();
        options.set_sync(true);
        self.db.write_opt(batch, &options).expect("persisting a pruning import recovery barrier cannot fail");
    }

    /// Test-only OS-kill rendezvous — and (RFC-0012 acceptance, checkbox 3) in the unpublished private-integration build the drill
    /// runs (`private-integration-config`): exact crash points between the pruning import's WAL barriers. Production builds contain
    /// no environment switch or pause path. The exact child process owns the rendezvous directory.
    #[cfg(any(test, feature = "private-integration-config"))]
    fn pruning_import_crash_checkpoint(&self, label: &str) {
        if std::env::var("MISAKA_RFC12_IMPORT_CRASH_STAGE").ok().as_deref() != Some(label) {
            return;
        }
        let Ok(root) = std::env::var("MISAKA_RFC12_IMPORT_CRASH_DIRECTORY") else { return };
        // Genesis startup also imports its premine. The worker arms only after
        // its baseline and expected cold-recovery manifest are durable.
        if std::fs::read(std::path::Path::new(&root).join("armed")).ok().as_deref() != Some(label.as_bytes()) {
            return;
        }
        let mut file = std::fs::File::create(std::path::Path::new(&root).join("ready")).unwrap();
        use std::io::Write;
        file.write_all(label.as_bytes()).unwrap();
        file.sync_all().unwrap();
        // Parent Child::kill supplies the crash. No DB close or flush executes.
        loop {
            std::thread::park();
        }
    }

    pub fn pruning_import_session(&self) -> PruningImportResult<kaspa_consensus_core::pruning::PruningImportSessionStateV1> {
        self.pruning_meta_stores.read().import_session().map_err(|e| {
            PruningImportError::PruningPointImportContextUnavailable(BlockHash::default(), format!("import session: {e}"))
        })
    }

    pub fn pruning_import_status(&self) -> PruningImportResult<(kaspa_consensus_core::pruning::PruningImportSessionStateV1, bool)> {
        let meta = self.pruning_meta_stores.read();
        let state = meta.import_session().map_err(|e| {
            PruningImportError::PruningPointImportContextUnavailable(BlockHash::default(), format!("import session: {e}"))
        })?;
        let stable = meta.try_pruning_utxoset_stable_flag().map_err(|e| {
            PruningImportError::PruningPointImportContextUnavailable(BlockHash::default(), format!("import stable marker: {e}"))
        })?;
        Ok((state, stable))
    }

    pub fn begin_pruning_point_import(
        &self,
        point: BlockHash,
    ) -> PruningImportResult<kaspa_consensus_core::pruning::PruningImportSessionV1> {
        use kaspa_consensus_core::pruning::{PruningImportSessionStateV1, PruningImportSessionV1};
        let _mutation = self.pruning_import_session_lock.lock();
        let unavailable =
            |e: StoreError| PruningImportError::PruningPointImportContextUnavailable(point, format!("begin session: {e}"));
        let pruning = self.pruning_point_store.read();
        if pruning.pruning_point().map_err(unavailable)? != point {
            return Err(PruningImportError::PruningImportSessionRefused {
                point,
                epoch: 0,
                reason: "target is not the established pruning point".into(),
            });
        }
        self.headers_store.get_header(point).map_err(unavailable)?;
        let mut meta = self.pruning_meta_stores.write();
        let old = meta.import_session().map_err(unavailable)?;
        let epoch = old.epoch.checked_add(1).ok_or_else(|| PruningImportError::PruningImportSessionRefused {
            point,
            epoch: old.epoch,
            reason: "durable import epoch exhausted".into(),
        })?;
        let token = PruningImportSessionV1 { point, epoch };
        let state = PruningImportSessionStateV1 { epoch, active: Some(token), utxos_started: false, utxos_imported: false };
        let mut batch = WriteBatch::default();
        meta.set_import_session(&mut batch, &state).map_err(unavailable)?;
        meta.set_pruning_utxoset_stable_flag(&mut batch, false).map_err(unavailable)?;
        self.write_pruning_import_barrier(batch);
        Ok(token)
    }

    /// A legacy mutation is accepted only before this DB has issued a session.
    /// Once session-aware IBD starts, old public callers cannot bypass its token.
    pub(crate) fn with_pruning_import_session<T>(
        &self,
        token: Option<kaspa_consensus_core::pruning::PruningImportSessionV1>,
        operation: impl FnOnce(&kaspa_consensus_core::pruning::PruningImportSessionStateV1) -> PruningImportResult<T>,
    ) -> PruningImportResult<T> {
        let _mutation = self.pruning_import_session_lock.lock();
        let state = self.pruning_import_session()?;
        let point = token.map(|t| t.point).unwrap_or_default();
        let epoch = token.map(|t| t.epoch).unwrap_or(0);
        let valid = match token {
            Some(token) => state.active == Some(token) && state.epoch == token.epoch,
            None => state.epoch == 0 && state.active.is_none(),
        };
        if !valid {
            return Err(PruningImportError::PruningImportSessionRefused {
                point,
                epoch,
                reason: "caller does not own the current durable import session".into(),
            });
        }
        if let Some(token) = token {
            let target =
                self.pruning_point_store.read().pruning_point().map_err(|e| {
                    PruningImportError::PruningPointImportContextUnavailable(point, format!("session pruning point: {e}"))
                })?;
            if target != point {
                return Err(PruningImportError::PruningImportSessionRefused {
                    point,
                    epoch: token.epoch,
                    reason: "established target changed after this session began".into(),
                });
            }
        }
        operation(&state)
    }

    pub(crate) fn set_pruning_import_phase(
        &self,
        state: &kaspa_consensus_core::pruning::PruningImportSessionStateV1,
    ) -> PruningImportResult<()> {
        let point = state.active.map(|t| t.point).unwrap_or_default();
        let mut meta = self.pruning_meta_stores.write();
        let mut batch = WriteBatch::default();
        meta.set_import_session(&mut batch, state)
            .map_err(|e| PruningImportError::PruningPointImportContextUnavailable(point, format!("import phase: {e}")))?;
        self.write_pruning_import_barrier(batch);
        Ok(())
    }

    pub(crate) fn append_pruning_import_utxos(
        &self,
        point: Option<BlockHash>,
        chunk: &[(TransactionOutpoint, UtxoEntry)],
        multiset: &mut MuHash,
    ) -> PruningImportResult<()> {
        use crate::model::stores::utxo_set::UtxoSetStore;
        let pruning = self.pruning_point_store.read();
        if let Some(point) = point
            && pruning.pruning_point().map_err(|e| PruningImportError::PruningPointImportContextUnavailable(point, e.to_string()))?
                != point
        {
            return Err(PruningImportError::PruningImportSessionRefused {
                point,
                epoch: 0,
                reason: "target changed before UTXO append".into(),
            });
        }
        let mut meta = self.pruning_meta_stores.write();
        meta.utxo_set.write_many(chunk).expect("writing an authorized UTXO import chunk cannot fail");
        let inner = self.install(|| {
            chunk.par_iter().map(|(outpoint, entry)| MuHash::from_utxo(outpoint, entry)).reduce(MuHash::new, |mut a, b| {
                a.combine(&b);
                a
            })
        });
        multiset.combine(&inner);
        Ok(())
    }

    pub(crate) fn clear_pruning_import_utxos(
        &self,
        state: Option<&kaspa_consensus_core::pruning::PruningImportSessionStateV1>,
    ) -> PruningImportResult<()> {
        let pruning = self.pruning_point_store.read();
        if let Some(token) = state.and_then(|s| s.active)
            && pruning
                .pruning_point()
                .map_err(|e| PruningImportError::PruningPointImportContextUnavailable(token.point, e.to_string()))?
                != token.point
        {
            return Err(PruningImportError::PruningImportSessionRefused {
                point: token.point,
                epoch: token.epoch,
                reason: "target changed before UTXO clear".into(),
            });
        }
        let mut meta = self.pruning_meta_stores.write();
        let mut batch = WriteBatch::default();
        if let Some(state) = state {
            meta.set_import_session(&mut batch, state).expect("staging the UTXO import phase cannot fail");
        }
        meta.set_pruning_utxoset_stable_flag(&mut batch, false).expect("staging an incomplete UTXO import cannot fail");
        self.write_pruning_import_barrier(batch);
        #[cfg(any(test, feature = "private-integration-config"))]
        self.pruning_import_crash_checkpoint("utxo-before-clear");
        meta.utxo_set.clear().expect("clearing an authorized UTXO import cannot fail");
        #[cfg(any(test, feature = "private-integration-config"))]
        self.pruning_import_crash_checkpoint("utxo-after-clear");
        Ok(())
    }

    /// Finalizes the pruning point utxoset state and imports the pruning point utxoset *to* virtual utxoset
    pub fn import_pruning_point_utxo_set(
        &self,
        new_pruning_point: BlockHash,
        mut imported_utxo_multiset: MuHash,
    ) -> PruningImportResult<()> {
        info!("Importing the UTXO set of the pruning point {}", new_pruning_point);
        let unavailable = |where_: &str, e: StoreError| {
            PruningImportError::PruningPointImportContextUnavailable(new_pruning_point, format!("UTXO {where_}: {e}"))
        };
        let require_target = |established: BlockHash| {
            if new_pruning_point == established {
                Ok(())
            } else {
                Err(PruningImportError::ImportedUtxoPruningPointMismatch(new_pruning_point, established))
            }
        };
        require_target(self.pruning_point_store.read().pruning_point().map_err(|e| unavailable("pruning-point store", e))?)?;
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).map_err(|e| unavailable("L1 header", e))?;
        let imported_utxo_multiset_hash = imported_utxo_multiset.finalize();
        if imported_utxo_multiset_hash != new_pruning_point_header.utxo_commitment {
            return Err(PruningImportError::ImportedMultisetHashMismatch(
                new_pruning_point_header.utxo_commitment,
                imported_utxo_multiset_hash,
            ));
        }

        {
            // Persist incomplete recovery with the position BEFORE a virtual UTXO copy.
            let pruning_point_read = self.pruning_point_store.read();
            require_target(pruning_point_read.pruning_point().map_err(|e| unavailable("pruning-point store", e))?)?;
            let mut batch = WriteBatch::default();
            let mut pruning_meta_write = self.pruning_meta_stores.write();
            pruning_meta_write.set_utxoset_position(&mut batch, new_pruning_point).unwrap();
            pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
            self.write_pruning_import_barrier(batch);
        }

        // Preserve the existing metadata -> virtual copy lock order. Never wait for a
        // pruning writer while holding virtual: that writer may be waiting on virtual.
        let pruning_meta_read = self.pruning_meta_stores.read();
        let mut virtual_write = self.virtual_stores.write();
        let pruning_point_read = self.pruning_point_store.try_read().ok_or_else(|| {
            PruningImportError::PruningPointImportContextUnavailable(
                new_pruning_point,
                "UTXO pruning target is changing; retry import".into(),
            )
        })?;
        require_target(pruning_point_read.pruning_point().map_err(|e| unavailable("pruning-point store", e))?)?;
        // The supplied incremental hash describes a stream, not necessarily the
        // shared staging table after another append stream interleaved. Verify the
        // actual locked table before clearing a single virtual UTXO. Memory stays
        // bounded to 1,000 rows; this extra scan runs only during an import.
        let mut staged_multiset = MuHash::new();
        for chunk in &pruning_meta_read.utxo_set.iterator().chunks(1000) {
            let rows: Vec<_> = chunk
                .map(|row| {
                    row.map_err(|e| {
                        PruningImportError::PruningPointImportContextUnavailable(
                            new_pruning_point,
                            format!("UTXO staged table row: {e}"),
                        )
                    })
                })
                .collect::<PruningImportResult<_>>()?;
            let chunk_multiset = self.install(|| {
                rows.par_iter().map(|(outpoint, entry)| MuHash::from_utxo(outpoint, entry)).reduce(MuHash::new, |mut a, b| {
                    a.combine(&b);
                    a
                })
            });
            staged_multiset.combine(&chunk_multiset);
        }
        let staged_hash = staged_multiset.finalize();
        if staged_hash != new_pruning_point_header.utxo_commitment {
            return Err(PruningImportError::ImportedMultisetHashMismatch(new_pruning_point_header.utxo_commitment, staged_hash));
        }
        {
            // Copy the pruning-point UTXO set into virtual's UTXO set.
            #[cfg(any(test, feature = "private-integration-config"))]
            self.pruning_import_crash_checkpoint("utxo-before-virtual-copy");
            virtual_write.utxo_set.clear().unwrap();
            #[cfg(any(test, feature = "private-integration-config"))]
            self.pruning_import_crash_checkpoint("utxo-after-virtual-clear");
            for chunk in &pruning_meta_read.utxo_set.iterator().map(|iter_result| iter_result.unwrap()).chunks(1000) {
                virtual_write.utxo_set.write_from_iterator_without_cache(chunk).unwrap();
                #[cfg(any(test, feature = "private-integration-config"))]
                self.pruning_import_crash_checkpoint("utxo-mid-virtual-copy");
            }
            // ADR-0109 Decision 1: the set was replaced wholesale; the index follows it wholesale.
            self.rebuild_evm_deposit_lock_index(&virtual_write.utxo_set);
            #[cfg(any(test, feature = "private-integration-config"))]
            self.pruning_import_crash_checkpoint("utxo-after-virtual-copy");
        }

        drop(pruning_meta_read);

        // Validate transactions of the pruning point itself while its target stays pinned.
        let new_pruning_point_transactions =
            self.block_transactions_store.get(new_pruning_point).map_err(|e| unavailable("block transactions", e))?;
        let validated_transactions = self.validate_transactions_in_parallel(
            &new_pruning_point_transactions,
            &virtual_write.utxo_set,
            new_pruning_point_header.daa_score,
            TxValidationFlags::Full,
        );
        if validated_transactions.len() < new_pruning_point_transactions.len() - 1 {
            // Some non-coinbase transactions are invalid
            return Err(PruningImportError::NewPruningPointTxErrors);
        }

        {
            // Submit partial UTXO state for the pruning point.
            // Note we only have and need the multiset; acceptance data and utxo-diff are irrelevant.
            let mut batch = WriteBatch::default();
            self.utxo_multisets_store.set_batch(&mut batch, new_pruning_point, imported_utxo_multiset.clone()).unwrap();

            let statuses_write = self.statuses_store.set_batch(&mut batch, new_pruning_point, StatusUTXOValid).unwrap();
            self.write_pruning_import_barrier(batch);
            drop(statuses_write);
        }

        // Head publication reads pruning again; release before that call to avoid
        // a recursive read behind a waiting pruning writer. Completion rechecks the
        // whole target/generation before it can expose this import as stable.
        drop(pruning_point_read);
        let virtual_read = parking_lot::RwLockWriteGuard::downgrade_to_upgradable(virtual_write);

        // Calculate the virtual state, treating the pruning point as the only virtual parent
        let virtual_parents = vec![new_pruning_point];
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);

        self.calculate_and_commit_virtual_state(
            virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            imported_utxo_multiset.clone(),
            &mut UtxoDiff::default(),
            // Pruning-point UTXO import (IBD): the `StakeBonds` store snapshot is
            // the bond set as-of the imported pruning point. Empty on every
            // current network (overlay dormant), so this is inert.
            &self.initial_active_bond_view(),
            &ChainPath::default(),
        )?;

        // Genesis initialization imports its pinned premine directly, without a
        // later IBD completion stage or non-genesis sidecars.
        if new_pruning_point == self.genesis.hash && self.pruning_import_session()?.active.is_none() {
            self.complete_pruning_point_import(new_pruning_point)?;
        }
        Ok(())
    }

    /// Complete IBD only when the target, executed virtual generation and every
    /// enabled sidecar still refer to the imported point. Readers cannot observe
    /// a true marker for a mixed generation; transient lock contention is retryable.
    pub fn complete_pruning_point_import(&self, point: BlockHash) -> PruningImportResult<()> {
        let unavailable = |where_: &str, e: StoreError| {
            PruningImportError::PruningPointImportContextUnavailable(point, format!("completion {where_}: {e}"))
        };
        let mismatch = |what: &str| PruningImportError::PruningPointImportGenerationMismatch(point, what.into());
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().map_err(|e| unavailable("virtual state", e))?;
        if virtual_state.ghostdag_data.selected_parent != point {
            return Err(mismatch("virtual sink does not match the import target"));
        }
        // Use try guards below virtual: a writer may itself need virtual. A busy
        // generation is incomplete, not permission to publish a stale import.
        let pruning_read = self.pruning_point_store.try_read().ok_or_else(|| mismatch("pruning target is changing; retry"))?;
        if pruning_read.pruning_point().map_err(|e| unavailable("pruning-point store", e))? != point {
            return Err(mismatch("established pruning target changed"));
        }
        let palw_read = self.palw_state_v2_store.try_read().ok_or_else(|| mismatch("PALW sidecar is changing; retry"))?;
        if self.palw_state_params_v2.is_some()
            && palw_read.tip_key().map_err(|e| unavailable("PALW tip", e))?.map(|(at, _)| at) != Some(point)
        {
            return Err(mismatch("PALW tip does not match the import target"));
        }
        let overlay_read =
            self.pruning_overlay_snapshot_store.try_read().ok_or_else(|| mismatch("overlay sidecar is changing; retry"))?;
        if point != self.genesis.hash
            && self.dns_params.is_some()
            && overlay_read.get().optional().map_err(|e| unavailable("overlay snapshot", e))?.map(|row| row.pruning_point)
                != Some(point)
        {
            return Err(mismatch("overlay snapshot does not match the import target"));
        }
        let heads_read = self.evm_heads_store.try_read().ok_or_else(|| mismatch("EVM heads are changing; retry"))?;
        let header = self.headers_store.get_header(point).map_err(|e| unavailable("L1 header", e))?;
        if point != self.genesis.hash && header.daa_score >= self.evm_activation_daa_score {
            let execution = self.evm_header_store.get(point).map_err(|e| unavailable("EVM execution", e))?;
            if execution.commitment_root() != header.evm_commitment_root
                || heads_read.get().map_err(|e| unavailable("EVM heads", e))?.latest != point
            {
                return Err(mismatch("EVM execution or latest head does not match the import target"));
            }
            self.evm_state_store.get(point).map_err(|e| unavailable("EVM snapshot", e))?;
        }
        let mut meta_write = self.pruning_meta_stores.try_write().ok_or_else(|| mismatch("UTXO import is changing; retry"))?;
        if meta_write.utxoset_position().map_err(|e| unavailable("UTXO position", e))? != point {
            return Err(mismatch("UTXO position does not match the import target"));
        }
        let mut batch = WriteBatch::default();
        let mut session = meta_write.import_session().map_err(|e| unavailable("import session", e))?;
        if let Some(token) = session.active {
            if token.point != point || !session.utxos_started || !session.utxos_imported {
                return Err(mismatch("session has not finalized this UTXO import"));
            }
            session.active = None;
            session.utxos_started = false;
            session.utxos_imported = false;
            meta_write.set_import_session(&mut batch, &session).map_err(|e| unavailable("closing import session", e))?;
        }
        meta_write.set_pruning_utxoset_stable_flag(&mut batch, true).map_err(|e| unavailable("stable marker", e))?;
        // Store writers update caches as they stage the batch. A failed durable
        // commit is fatal, like the other consensus writes; returning would expose
        // the cached true marker without its durable completion.
        #[cfg(any(test, feature = "private-integration-config"))]
        self.pruning_import_crash_checkpoint("import-before-complete");
        self.write_pruning_import_barrier(batch);
        #[cfg(any(test, feature = "private-integration-config"))]
        self.pruning_import_crash_checkpoint("import-after-complete");
        Ok(())
    }

    /// kaspa-pq ADR-0022: import the pruning point's EVM execution state during
    /// headers-proof IBD. Without this, the first post-pruning block re-executes the
    /// EVM lane against an empty genesis state (the pruning point has no
    /// `evm_header_store` row on a fresh node), so its recomputed `evm_commitment_root`
    /// mismatches the header and the whole chain is disqualified.
    ///
    /// Verification (trustless): the supplied [`EvmExecutionHeader`] must reproduce
    /// the L1 header's `evm_commitment_root` (a pure, secp-free keyed-BLAKE2b check),
    /// and — on an `evm` build — the supplied [`EvmStateSnapshot`] must reproduce that
    /// EVM header's `state_root` (the keccak-MPT root over the account set). Then the
    /// two rows are persisted and the canonical **finalized** EVM head is set to the
    /// pruning point, so `evm_execute_acceptance_with_parent` finds the real parent
    /// state for `pp`'s children.
    pub fn import_pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
        evm_header: kaspa_consensus_core::evm::EvmExecutionHeader,
        snapshot: kaspa_consensus_core::evm::EvmStateSnapshot,
    ) -> PruningImportResult<()> {
        info!("Importing the EVM state of the pruning point {}", pruning_point);
        let context_error = |where_: &str, e: StoreError| {
            PruningImportError::PruningPointImportContextUnavailable(pruning_point, format!("{where_}: {e}"))
        };
        let require_target = |established: BlockHash| {
            if pruning_point == established {
                Ok(())
            } else {
                Err(PruningImportError::ImportedEvmPruningPointMismatch(pruning_point, established))
            }
        };
        // Root validity is not import authority. The IBD proof/catch-up installs this target first.
        require_target(self.pruning_point_store.read().pruning_point().map_err(|e| context_error("pruning-point store", e))?)?;
        let l1_header = self.headers_store.get_header(pruning_point).map_err(|e| context_error("L1 header", e))?;

        // (1) The EVM header must reproduce the L1 commitment (pure; works on any build).
        let got = evm_header.commitment_root();
        if got != l1_header.evm_commitment_root {
            return Err(PruningImportError::ImportedEvmCommitmentMismatch(pruning_point, got, l1_header.evm_commitment_root));
        }

        // (2) The state snapshot must reproduce the EVM header's keccak-MPT state root.
        // Requires the EVM executor; an `evm`-active network can only be synced by an
        // `--features evm` build (a default build rejects its v2 headers earlier), so
        // skipping this on a non-evm build never weakens a chain it actually follows.
        #[cfg(feature = "evm")]
        {
            let db = kaspa_evm::snapshot::seed_cachedb(&snapshot)
                .map_err(|e| PruningImportError::ImportedEvmSnapshotInvalid(pruning_point, e.to_string()))?;
            let computed = kaspa_hashes::EvmH256::from_bytes(kaspa_evm::state::state_root(&db).0);
            if computed != evm_header.state_root {
                return Err(PruningImportError::ImportedEvmStateRootMismatch(pruning_point, computed, evm_header.state_root));
            }
        }

        // (3) Serialize retries, then recheck and pin the target through the durable batch.
        // Memo -> pruning -> heads matches the readiness reader's lock order. No mutation precedes
        // validation of every existing row; a retry reuses identical immutable data, never overwrites it.
        let mut readiness_memo = self.native_readiness_memo.lock();
        let pruning_point_read = self.pruning_point_store.read();
        require_target(pruning_point_read.pruning_point().map_err(|e| context_error("pruning-point store", e))?)?;
        let insert_header =
            match self.evm_header_store.get(pruning_point).optional().map_err(|e| context_error("existing execution header", e))? {
                None => true,
                Some(existing) if existing == evm_header => false,
                Some(_) => return Err(PruningImportError::ImportedEvmExistingStateMismatch(pruning_point, "execution header".into())),
            };
        let insert_snapshot =
            match self.evm_state_store.get(pruning_point).optional().map_err(|e| context_error("existing snapshot", e))? {
                None => true,
                Some(existing) if existing == snapshot => false,
                Some(_) => return Err(PruningImportError::ImportedEvmExistingStateMismatch(pruning_point, "snapshot".into())),
            };
        use crate::model::stores::evm::EvmStateCheckpointStoreReader;
        let checkpoint = kaspa_consensus_core::evm::EvmStateCheckpointV1::build(
            pruning_point,
            evm_header.evm_number,
            evm_header.state_root,
            &snapshot,
        );
        let insert_checkpoint =
            match self.evm_state_checkpoint_store.get(pruning_point).map_err(|e| context_error("existing checkpoint", e))? {
                None => true,
                Some(existing) if existing == checkpoint => false,
                Some(_) => return Err(PruningImportError::ImportedEvmExistingStateMismatch(pruning_point, "checkpoint".into())),
            };
        let state_root = evm_header.state_root; // captured before `evm_header` is moved below
        let mut batch = WriteBatch::default();
        // C-01 S8 (audit M-01): also seed the flat latest-canonical state from the verified
        // snapshot, so a pruned-IBD node starts with a flat store materialized at the pruning point
        // (the basis the S7 flat fast-path and the S9 cutover read). Gated on the shadow backend,
        // matching the per-block dual-write (S4) — the flat store is a node-local shadow until
        // cutover. Same atomic batch as the 206 write; flat/code/root/pointer are state data only
        // (never a commitment) ⇒ consensus-neutral. Done before `snapshot`/`evm_header` are moved.
        if self.evm_shadow_state_backend {
            let mut ptr = self.evm_latest_state_ptr_store.write();
            crate::processes::evm::seed_flat_from_snapshot(
                &self.evm_flat_account_store,
                &self.evm_code_store,
                &self.evm_block_state_root_store,
                &mut ptr,
                &mut batch,
                pruning_point,
                state_root,
                &snapshot,
            )
            .map_err(|e| PruningImportError::ImportedEvmSnapshotInvalid(pruning_point, format!("flat seed: {e}")))?;
        }
        if insert_header {
            self.evm_header_store.insert_batch(&mut batch, pruning_point, evm_header).unwrap();
        }
        // F2a (t10 recovery): the imported, root-verified snapshot is also this
        // node's FIRST pruning-point state anchor — persist it as a checkpoint so
        // the pruning processor's pp-anchor induction (`ensure_pp_evm_anchor`) has
        // a base once the pp advances, even on a retired-206 node (which writes no
        // per-block 206 rows of its own; §12 gathering anchors on checkpoints).
        if insert_checkpoint {
            self.evm_state_checkpoint_store.insert_batch(&mut batch, pruning_point, checkpoint).unwrap();
        }
        if insert_snapshot {
            self.evm_state_store.insert_batch(&mut batch, pruning_point, snapshot).unwrap();
        }
        {
            let mut heads_write = self.evm_heads_store.write();
            let prev = heads_write.get().ok();
            let latest = prev.as_ref().map(|h| h.latest).unwrap_or(pruning_point);
            let safe = prev.as_ref().map(|h| h.safe).unwrap_or(pruning_point);
            let retired = self.headers_store.get_daa_score(pruning_point).is_ok_and(|daa| self.dns_retired_at(daa));
            let heads = if retired {
                kaspa_consensus_core::evm::CanonicalEvmHeads {
                    latest: pruning_point,
                    safe: BlockHash::default(),
                    finalized: BlockHash::default(),
                }
            } else {
                kaspa_consensus_core::evm::CanonicalEvmHeads { latest, safe, finalized: pruning_point }
            };
            heads_write.set_native_finalized_publication_batch(&mut batch, None).unwrap();
            heads_write.set_native_batch(&mut batch, None).unwrap();
            heads_write.set_batch(&mut batch, heads).unwrap();
        }
        // Importing a sidecar starts a cross-ledger recovery transaction. Persist the
        // incomplete marker WITH the state, before any UTXO clear/copy can occur. A
        // crash here must make IBD retry all sidecars, never reuse old stable UTXOs.
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
        self.write_pruning_import_barrier(batch);
        drop(pruning_meta_write);
        *readiness_memo = None;
        drop(pruning_point_read);
        Ok(())
    }

    /// kaspa-pq ADR-0022 (serving side): the pruning point's EVM execution header +
    /// state snapshot, for a peer to stream during another node's headers-proof IBD.
    /// `None` if the overlay/EVM rows are absent (pre-activation or not yet computed).
    pub fn pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
    ) -> Option<(kaspa_consensus_core::evm::EvmExecutionHeader, kaspa_consensus_core::evm::EvmStateSnapshot)> {
        // EvmHeaderStoreReader / EvmStateStoreReader are in module scope.
        let header = self.evm_header_store.get(pruning_point).ok()?;
        // Hot path: the persisted 206[pp] snapshot.
        match self.evm_state_store.get(pruning_point) {
            Ok(snapshot) => return Some((header, snapshot)),
            Err(StoreError::KeyNotFound(_)) => {} // retired (S9b) ⇒ serve from the flat backend below
            Err(e) => {
                warn!("[evm] pruning-point 206 read failed for {pruning_point}: {e}");
                return None;
            }
        }
        // C-01 S9b: 206[pp] retired. Serve the pruning-point state from the flat backend so peers can
        // still IBD from this node — materialize it when the pp IS the flat head (a freshly pruned-IBD
        // -imported node pins the flat pointer to the pp), else §12-reconstruct (a full-sync serving
        // node whose head is far ahead of the buried pp; needs recent/archive history — `head` keeps
        // none, hence the startup warning). `None` if neither yields it (the peer tries another server).
        //
        // **Only for a hash this node actually calls a pruning point** (mainnet audit H-1's sibling
        // sweep). The argument is peer-chosen and reaches this function for any block whose EVM
        // header is stored, so without this a forty-byte request could aim `materialize_snapshot`
        // or the §12 forward-diff walk at an arbitrary chain block. The persisted 206 row above is
        // a point lookup and needs no such gate; these two are not. The gate is THIS node's current
        // pruning point, not the whole past-pruning-points set: `DbPastPruningPointsStore` exposes
        // only `get(index)`, so a set-membership test here would itself be a per-request walk —
        // an amplification of the same shape as the one being closed. Historical points keep their
        // serve path through the persisted 206 row above, which returns before this block.
        #[cfg(feature = "evm")]
        {
            use crate::model::stores::evm::{EvmCodeStoreReader, EvmStateCheckpointStoreReader, EvmStateDiffStoreReader};
            if self.pruning_point_store.read().pruning_point().ok() != Some(pruning_point) {
                return None;
            }
            if let Ok(Some(ptr)) = self.evm_latest_state_ptr_store.read().get()
                && ptr.canonical_head == pruning_point
            {
                return match crate::processes::evm::materialize_snapshot(&self.evm_flat_account_store, &self.evm_code_store) {
                    Ok(snapshot) => Some((header, snapshot)),
                    Err(e) => {
                        warn!("[evm] pruning-point flat materialize failed for {pruning_point}: {e}");
                        None
                    }
                };
            }
            let (seed, forward_diffs) = match crate::processes::evm::gather_reconstruction_inputs(
                pruning_point,
                |b| self.evm_state_checkpoint_store.get(b),
                |b| self.evm_state_diff_store.get(b),
                // Pre-activation is judged by the L1 DAA score. Sub-pruning-point
                // blocks have no EVM rows (pruned), and reading that absence as
                // pre-activation is exactly the t10 empty-seed bug — fail closed.
                |b| self.headers_store.get_compact_header_data(b).map(|c| c.daa_score < self.evm_activation_daa_score),
            ) {
                Ok(v) => v,
                Err(e) => {
                    warn!("[evm] pruning-point §12 reconstruct gather failed for {pruning_point}: {e}");
                    return None;
                }
            };
            match kaspa_evm::reconstruct::reconstruct_evm_state(
                &seed,
                &forward_diffs,
                |h| self.evm_code_store.get(*h).ok().flatten(),
                header.state_root,
            ) {
                Ok(snapshot) => Some((header, snapshot)),
                Err(e) => {
                    warn!("[evm] pruning-point §12 reconstruct failed for {pruning_point}: {e}");
                    None
                }
            }
        }
        #[cfg(not(feature = "evm"))]
        None
    }

    /// kaspa-pq ADR-0022: import the pruning point's DNS/PoS-v2 overlay snapshot during
    /// headers-proof IBD. Persists the bond set (so `initial_active_bond_view` and the
    /// reward path read it), the pruning point's cumulative reserve balance (read by the
    /// first post-pruning finalizing block's §F drip), and the whole snapshot in the
    /// `pruning_overlay_snapshot_store` — which `selected_chain_overlay_window` consults
    /// for the below-pruning-point window (the selected-chain walk cannot traverse below
    /// the pruning point). Verification is trustless and automatic: the first post-pruning
    /// block's existing coinbase/overlay `c == v` re-derives this state and checks it
    /// against the committed `overlay_commitment_root`; a wrong snapshot disqualifies that
    /// block and the (staging) IBD is discarded.
    /// **Install the PALW V2 state at a pruning point** (launch blockers §1, the import half).
    ///
    /// `PalwChainStateV2` was written only by `process_genesis`, so a node joining by pruned IBD
    /// had none — and absent state was read as "no policy", silently disabling every PALW rule.
    /// The startup guard in `Consensus::new` now refuses to RUN in that state; this is what lets
    /// such a node exist at all.
    ///
    /// **The root is the gate, and it runs before the write.** `into_state` rebuilds the carriage
    /// and demands `state_root` back, so a peer that forges one byte of bonds, class shares or
    /// claims produces a state whose root is not the one the header committed — and it is refused
    /// here, not detected later. Detection after a durable write is not a defence: forged bonds are
    /// block-production rights and forged claims are `safe_weight`.
    ///
    /// The expected root comes from the pruning point's OWN header, never from the peer's message.
    pub fn import_pruning_point_palw_state(
        &self,
        pruning_point: BlockHash,
        carriage: kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
    ) -> PruningImportResult<()> {
        self.import_pruning_point_palw_snapshot(pruning_point, carriage, None)
    }

    /// Install state and its execution declarations as one recovery generation.
    /// Validation of the complete offered set precedes every store mutation.
    pub(crate) fn import_pruning_point_palw_snapshot(
        &self,
        pruning_point: BlockHash,
        carriage: kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2,
        declarations: Option<(Vec<(kaspa_hashes::Hash64, Vec<u8>)>, bool)>,
    ) -> PruningImportResult<()> {
        let Some(params) = self.palw_state_params_v2.as_ref() else {
            return Ok(()); // not a ConsensusV2 network — nothing reads this store.
        };
        let declaration_rows = if let Some((offered, complete)) = declarations {
            self.palw_validate_snapshot_declarations_v1_impl(&carriage, &offered, complete).map_err(|why| {
                PruningImportError::PruningPointImportContextUnavailable(pruning_point, format!("snapshot declarations: {why}"))
            })?;
            offered
                .into_iter()
                .map(|(id, bytes)| {
                    let class = &carriage.classes[&id]; // membership was checked above
                    (
                        id,
                        crate::model::stores::palw_class_carriage::PalwClassCarriageRecord {
                            registered_daa: class.registered_daa,
                            artifact_root: class.artifact_root,
                            carriage: bytes,
                        },
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let context_error = |e: StoreError| match e {
            StoreError::LocalResourceUnavailable(reason) => PruningImportError::PruningImportRuleError(
                kaspa_consensus_core::errors::block::RuleError::LocalNativeCapacityUnavailable(
                    kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::unsupported::<
                        kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
                    >("native pruning snapshot store", &reason),
                ),
            ),
            other => {
                PruningImportError::PruningPointImportContextUnavailable(pruning_point, format!("PALW pruning-point store: {other}"))
            }
        };
        let require_target = |established: BlockHash| {
            if pruning_point == established {
                Ok(())
            } else {
                Err(PruningImportError::ImportedPalwPruningPointMismatch(pruning_point, established))
            }
        };
        // A valid historic root is not permission to replace the live PALW tip.
        // Every IBD caller establishes this target through its pruning history first.
        require_target(self.pruning_point_store.read().pruning_point().map_err(context_error)?)?;
        // **Which header commits this state.** `Header::palw_state_root` commits the root of the
        // state as-of the block's SELECTED PARENT — the chain walk compares exactly that
        // (`header.palw_state_root != parent_root` disqualifies). This carriage is the state as-of
        // `pruning_point`, so the header that commits it is any child whose selected parent IS the
        // pruning point. The pruning point's OWN header commits its parent's state, which is a
        // different value and would refuse every honest carriage.
        //
        // Headers are synced before the utxoset sidecars in every IBD path, so such a child
        // normally exists by now, and it arrived under PoW plus the headers proof.
        // **One witness, the child on the selected chain** — see `pruning_point_witness_child`.
        //
        // Which child is the whole security of this gate, and it took three tries. Taking the FIRST
        // qualifying child let the peer pick: every header here arrived from the IBD peer,
        // `palw_state_root` is not checked at header validation (the only check is in the
        // selected-chain walk, long after this import), and `get_children` returns a hash set whose
        // iteration order is a function of the hashes — grindable. Requiring UNANIMITY closed that
        // and opened the mirror image: the same one-block poisoning audit M1-2 found on the overlay
        // twin applied verbatim, since one side block committing a garbage root made every
        // child-disagreement fatal, and the disagreement is a fact of the DAG no retry and no other
        // peer can remove. The HEAVIEST child (M1-2's repair) still let the peer pick, because
        // siblings tie on blue work as a matter of course and the tiebreak was the block hash.
        //
        // The selected-chain child is at most one by construction, and displacing it costs a chain
        // that out-works the pruning point forward.
        // The witness child's DAA rides along: ADR-0069 Decision 7's fence is a chain-point rule,
        // so the consistency check has to be told the rule in force AT THE SNAPSHOT, not at
        // genesis. The child is the nearest header this import trusts that stands above the
        // pruning point, and a fence is monotone in DAA, so resolving at the child is the honest
        // reading of "the rule this state was built under".
        let witness = self
            .pruning_point_witness_child(pruning_point)
            .and_then(|child| self.headers_store.get_header(child).ok())
            .map(|h| (h.palw_state_root, h.daa_score));
        // No child header to check against yet: REFUSE rather than write on trust. An unverifiable
        // carriage is exactly the one an attacker supplies, and the IBD can be retried once the
        // child header is in hand.
        let (expected_root, witness_daa) = witness.ok_or(PruningImportError::ImportedPalwStateHeaderMissing(pruning_point))?;
        // **A frontier above the point it is the state of is a lie on its face** (mainnet audit,
        // 2026-09-05). `safe_frontier_blue_score` is fork choice's FIRST key and the frontier only
        // ever advances (`claim.accepted_blue_score > old_frontier`), so a carriage declaring one
        // the chain never reached pins this node to the peer's chain for the rest of its life — and
        // the root check below cannot see it, because the peer authored the header that commits
        // the root. What the root check cannot see, the chain can: every claim this state counts
        // was accepted at or below `pruning_point`, so no frontier can stand above the point's own
        // blue score. Checked first, before a byte is rebuilt, because it costs one comparison.
        let point_blue_score = self
            .ghostdag_store
            .get_blue_score(pruning_point)
            .map_err(|_| PruningImportError::ImportedPalwStateHeaderMissing(pruning_point))?;
        if carriage.safe_frontier_blue_score > point_blue_score {
            return Err(PruningImportError::ImportedPalwStateInvalid(
                pruning_point,
                expected_root,
                format!(
                    "the carriage declares a safe frontier at blue score {} above the pruning point's own {point_blue_score}: no \
                     claim accepted by this point can stand above it",
                    carriage.safe_frontier_blue_score
                ),
            ));
        }
        // **RFC-0009 L2: the witness commits the state in the form its point's fence says** (`palw_committed_root_v1`). Dormant
        // (every shipped network): the carriage's own check against the flat root, exactly as before. Armed: the carriage is decoded and
        // checked for consistency, and the committed form of the decoded state is compared with the witness — the same helper the
        // chain walk uses, so an import and a validation cannot disagree about one state.
        let flat_check = if self.palw_fork_choice_commitment_v1.is_none() { Some(expected_root) } else { None };
        // Direct callers may supply a typed carriage without the protocol-107
        // receiver's decode lease. Reserve canonical intake and actual schema
        // reconstruction before into_state, then retain its owner to commit.
        let state = kaspa_consensus_core::palw_native_resource_v1::restore_native_carriage_v1(
            pruning_point,
            carriage,
            params,
            flat_check,
            self.palw_uncertified_weightless_at(witness_daa),
            self.palw_canonical_work_daa,
        )
        .map_err(|e| match e {
            kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Resource(why) => {
                PruningImportError::PruningImportRuleError(
                    kaspa_consensus_core::errors::block::RuleError::LocalNativeCapacityUnavailable(why),
                )
            }
            kaspa_consensus_core::palw_native_resource_v1::NativeDecodeErrorV1::Decode(other) => {
                PruningImportError::ImportedPalwStateInvalid(pruning_point, expected_root, other.to_string())
            }
        })?;
        if flat_check.is_none() {
            let got = self.palw_committed_root_v1(&state);
            if got != expected_root {
                return Err(PruningImportError::ImportedPalwStateInvalid(
                    pruning_point,
                    expected_root,
                    format!("carriage state commits {got} in the header form, not the witness's {expected_root}"),
                ));
            }
        }
        // Install uses the actual typed, authenticated state and its rebuilt
        // indices, never the sender's byte length or local Serve metadata. Keep
        // this operation's demand reserved through the durable WAL barrier.
        let _native_install_lease = kaspa_consensus_core::palw_native_resource_v1::reserve_native_install_in_database_v1(
            pruning_point,
            &state,
            self.db.native_single_directory_quota_path_v1(),
        )
        .map_err(|why| {
            PruningImportError::PruningImportRuleError(kaspa_consensus_core::errors::block::RuleError::LocalNativeCapacityUnavailable(
                why,
            ))
        })?;
        // Pin the established target after validation and through the durable batch.
        // Pruning -> PALW -> metadata matches pruning advancement's lock order.
        let pruning_point_read = self.pruning_point_store.read();
        require_target(pruning_point_read.pruning_point().map_err(context_error)?)?;
        // Written through `set_tip_batch`, which RE-DERIVES the root from the state it is handed —
        // so what becomes durable is a function of what was verified, and a caller cannot store a
        // snapshot under a root it did not compute. The peer's bytes never reach the database.
        let mut batch = WriteBatch::default();
        // Match ordinary block commit's pruning -> PALW -> class-index lock order, and
        // retain the guards until the complete generation is durable.
        let mut store = self.palw_state_v2_store.write();
        let mut class_store = self.palw_class_carriage_store.write();
        for (id, row) in declaration_rows {
            class_store.insert_batch(&mut batch, id, row).map_err(context_error)?;
        }
        let _native_tip_write = store.set_tip_batch_tracked_v1(&mut batch, pruning_point, &state).map_err(context_error)?;
        // Live claim/owner holds come from the verified state itself, written in the same batch
        // as its tip. Holds already below this imported irreversible clock are not recreated.
        let imported_irreversible_daa = self
            .headers_store
            .get_daa_score(pruning_point)
            .ok()
            .or_else(|| state.last_point().map(|point| point.daa_score))
            .unwrap_or(0);
        store.archive_staged_cache_leases_state_batch_v1(&mut batch, &state, imported_irreversible_daa).map_err(context_error)?;
        // The same state, also as this node's servable snapshot. Without it a node that just
        // joined by a pruned sync holds the pruning-point state but cannot HAND it on until
        // its own pruning point next advances — so the fix would not propagate past the first
        // hop, and a young network would have exactly one node able to serve.
        let _native_snapshot_write =
            store.set_pruning_snapshot_batch_tracked_v1(&mut batch, pruning_point, &state).map_err(context_error)?;
        // The PALW sidecar is installed before IBD replaces the UTXO set. An OS
        // crash between those steps must leave a durable incomplete-import flag.
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, false).map_err(context_error)?;
        self.write_pruning_import_barrier(batch);
        drop(pruning_meta_write);
        drop(class_store);
        drop(store);
        Ok(())
    }

    /// The PALW state this node holds AT `pruning_point`, for a peer syncing from it.
    ///
    /// Served only when the stored tip really names that block: the store holds ONE materialized
    /// snapshot, and answering with a different point's state would hand a peer a carriage whose
    /// root its header does not commit — which the importer would refuse anyway, but the honest
    /// answer to "I do not have that" is `None`.
    pub fn pruning_point_palw_state(
        &self,
        pruning_point: BlockHash,
    ) -> Option<kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2> {
        let params = self.palw_state_params_v2.as_ref()?;
        // **The SNAPSHOT, not the tip.** The tip is rewritten to the sink on every virtual walk, so
        // asking whether it equals the pruning point was a question whose answer was always no on
        // a running node — and the reply was always `found: false`, which aborts the requester's
        // whole IBD. `capture_pruning_point_palw_state` materialises this row at pruning-advance.
        //
        // **The peer's hash is compared BEFORE anything is decoded** (mainnet audit H-1). This
        // used to run `load_pruning_snapshot` — borsh decode of the whole carriage, both index
        // rebuilds, both consistency walks, a full `state_root()` recompute — and ask afterwards
        // whether the block it had just rebuilt was the one the peer named. Forty bytes from any
        // peer past the handshake, no operator opt-in, on the blocking pool block processing
        // shares. The store now holds the check and the memo; the answer is the same bytes.
        let carriage = self.palw_state_v2_store.read().load_pruning_snapshot_carriage_cached(params, pruning_point).ok().flatten()?;
        carriage.try_clone_v1().ok()?.into_unreserved_legacy_v1().ok()
    }

    /// **The child of `pruning_point` whose header is allowed to witness the pruning point's own
    /// sidecar state — chosen by BLUE WORK, never by hash-set order.**
    ///
    /// Both sidecar imports (the DNS overlay snapshot and the PALW state carriage) verify the
    /// peer's bytes against a child header, because a child's commitment is a commitment to its
    /// SELECTED PARENT's state, which is exactly this pruning point's. The question is which child.
    ///
    /// It used to be "all of them, and any disagreement is fatal". That is a one-block, permanent
    /// denial of service (audit M1-2): a pruning sample is deterministic, so an attacker mines ONE
    /// valid block whose sole parent is the block about to become the pruning point, carrying a
    /// garbage root. The block is never a chain candidate — `verify_expected_utxo_state` is the only
    /// place either root is checked, and it runs only for UTXO-valid chain blocks — but its header
    /// is a fact of the DAG that every node stores and serves, so every joining node aborts its
    /// import against it, forever, against every peer.
    ///
    /// It cannot be "any child that agrees" either: a peer supplying both the snapshot and a child
    /// header that agrees with it would then verify its own forgery.
    ///
    /// **And it cannot be the heaviest child** (re-audit R-3), which is what this did first. A
    /// block's blue work is `selected_parent.blue_work + Σ work(mergeset blues)`, so an attacker
    /// whose block takes the pruning point as its selected parent can merge the very same public
    /// blocks the honest child merges and TIE it — or merge one more and exceed it. On a tie the
    /// old rule broke to the larger hash, which a miner producing a block anyway can grind for a
    /// few extra attempts. One cheap block still chose the examiner. The disqualification filter
    /// did not help either: this runs during a headers-first IBD, where the pruning point's
    /// children are `HeaderOnly` and nothing has been disqualified yet.
    ///
    /// The discriminator that is actually out of an attacker's reach is the **selected chain**: the
    /// honest child of the pruning point is a chain ancestor of the header DAG's selected tip, and
    /// the side block is not. To make its block the one on that chain the attacker must out-work
    /// the honest chain all the way to the tip, which is the assumption the whole ledger already
    /// rests on. The tip comes from this node's own header store — the syncer's claimed sink,
    /// already received under PoW and the headers proof — not from the peer's answer, and headers
    /// are synced before the utxoset sidecars in every IBD path.
    ///
    /// Exactly one child can satisfy that. If none does — the tip is not yet known, or the pruning
    /// point is not on the chain to it — this returns `None` and both callers REFUSE rather than
    /// write on trust, which is the same posture they already took when no child header was in
    /// hand: an unverifiable carriage is exactly the one an attacker supplies, and the IBD can be
    /// retried.
    fn pruning_point_witness_child(&self, pruning_point: BlockHash) -> Option<BlockHash> {
        let Ok(tip) = self.headers_selected_tip_store.read().get() else {
            warn!("no header selected tip yet: refusing to pick a pruning-point witness on anything an attacker can grind");
            return None;
        };
        let children: Vec<BlockHash> = RelationsStoreReader::get_children(&self.relations_service, pruning_point)
            .map(|c| c.read().iter().copied().collect())
            .unwrap_or_default();
        let mut witness = None;
        for child in children {
            if self.headers_store.get_header(child).is_err() {
                continue;
            }
            if self.ghostdag_store.get_selected_parent(child).ok() != Some(pruning_point) {
                continue; // commits a different parent's state — not this one
            }
            if self.statuses_store.read().get(child).ok() == Some(StatusDisqualifiedFromChain) {
                continue; // the local consensus already refused this block; it witnesses nothing
            }
            // On the selected chain to the tip. `is_chain_ancestor_of` is inclusive, so a child
            // that IS the tip qualifies.
            if !self.reachability_service.try_is_chain_ancestor_of(child, tip.hash).unwrap_or(false) {
                continue;
            }
            match witness {
                None => witness = Some(child),
                Some(seen) => {
                    // Two children of one block cannot both be on one selected chain. If the stores
                    // ever say otherwise, that is corruption, not a choice to make quietly.
                    warn!("pruning point {pruning_point} has two selected-chain children ({seen}, {child}); refusing to witness");
                    return None;
                }
            }
        }
        witness
    }

    pub fn import_pruning_point_overlay_snapshot(
        &self,
        pruning_point: BlockHash,
        snapshot: OverlaySnapshot,
    ) -> PruningImportResult<()> {
        if self.dns_params.is_none() {
            return Ok(()); // overlay dormant — the snapshot is empty and nothing reads it
        }
        let unavailable = |where_: &str, e: StoreError| {
            PruningImportError::PruningPointImportContextUnavailable(pruning_point, format!("legacy overlay {where_}: {e}"))
        };
        let require_target = |established: BlockHash| {
            if pruning_point == established {
                Ok(())
            } else {
                Err(PruningImportError::ImportedOverlayPruningPointMismatch(pruning_point, established))
            }
        };
        require_target(self.pruning_point_store.read().pruning_point().map_err(|e| unavailable("pruning-point store", e))?)?;

        // TRUSTLESS GATE — this runs BEFORE the write, and the write is to the LIVE consensus
        // store (all three IBD call sites hand this a live session; the headers-proof one
        // deliberately re-obtains it after `staging.commit()`). The doc above used to argue the
        // first post-pruning block's `c == v` would catch a forged snapshot, and it would —
        // *after* peer-supplied bond records and a peer-supplied `reserve_balance` were already
        // durable, with no rollback. Forged bonds are voting weight and reward eligibility; a
        // forged reserve balance is minted coin in the §F drip. Detection after the write is not
        // a defence.
        //
        // What makes it checkable: `Header::overlay_commitment_root` commits to the overlay
        // snapshot as-of the block's SELECTED PARENT, and this snapshot is as-of `pruning_point`
        // — so any header whose selected parent is the pruning point commits to exactly this
        // value. Headers are synced before the utxoset sidecars in every IBD path, so such a
        // child normally exists by now, and it arrived under PoW + the headers proof.
        let got = snapshot.commitment_root();
        // **One witness, the child on the selected chain** — see `pruning_point_witness_child`.
        // Checking every child and aborting on any disagreement (what this did until audit M1-2) let
        // one cheap side block poison a pruning point permanently; checking the first child the hash
        // set yielded let the peer choose its own examiner. So did the HEAVIEST child (M1-2's own
        // repair, corrected by re-audit R-3): siblings tie on blue work as a matter of course and
        // the tiebreak was the block hash, which is grindable. Moving the selected chain off the
        // honest child instead costs an attacker a chain that out-works the pruning point forward.
        // Pinned by `the_pruning_point_witness_is_the_selected_chain_child_not_a_side_block`, which
        // fails against the heaviest-child rule with the attacker's planted root demanded.
        let verified_against = self.pruning_point_witness_child(pruning_point);
        if let Some(child) = verified_against {
            let committed = self.headers_store.get_header(child).map(|h| h.overlay_commitment_root).unwrap_or_default();
            if committed != got {
                return Err(PruningImportError::ImportedOverlayCommitmentMismatch(pruning_point, got, committed));
            }
        }
        if verified_against.is_none() {
            // No child header to check against yet. Refuse rather than write on trust: an
            // unverifiable snapshot is exactly the one an attacker supplies, and the IBD can be
            // retried once the child header is in hand.
            warn!(
                "[overlay-import] refusing the pruning-point overlay snapshot for {pruning_point}: no header whose selected parent is \
                 the pruning point is available to verify its commitment {got} against"
            );
            return Err(PruningImportError::ImportedOverlayCommitmentMismatch(pruning_point, got, Hash64::default()));
        }
        info!(
            "Importing the overlay snapshot of the pruning point {} ({} bonds, {} window blocks, reserve {})",
            pruning_point,
            snapshot.bonds.len(),
            snapshot.window.len(),
            snapshot.reserve_balance
        );
        // Historic monetary state is installed only for the authorized import target.
        // Recheck and hold that target before acquiring any monetary store write lock.
        let pruning_point_read = self.pruning_point_store.read();
        require_target(pruning_point_read.pruning_point().map_err(|e| unavailable("pruning-point store", e))?)?;
        let mismatch = |what: String| PruningImportError::ImportedOverlayExistingStateMismatch(pruning_point, what);
        let anchor_daa = self.headers_store.get_daa_score(pruning_point).map_err(|e| unavailable("anchor header", e))?;
        let mut canonical = snapshot;
        canonical.canonicalize();
        let mut bonds_write = self.stake_bonds_store.write();
        let mut overlay_write = self.pruning_overlay_snapshot_store.write();
        // Verify ALL existing monetary rows before staging any write. A retry is
        // reusable state, not an overwrite of an owner's exit/slashing history.
        let mut new_bonds = Vec::new();
        let mut seen_bonds = std::collections::BTreeSet::new();
        for rec in &canonical.bonds {
            if !seen_bonds.insert((rec.bond_outpoint.transaction_id, rec.bond_outpoint.index)) {
                return Err(mismatch(format!("duplicate bond {}", rec.bond_outpoint)));
            }
            match bonds_write.get(&rec.bond_outpoint).optional().map_err(|e| unavailable("bond row", e))? {
                None => new_bonds.push(rec.clone()),
                Some(existing) => {
                    // A historic snapshot omits later canonical mutations; keep
                    // those mutations when the as-of-anchor facts agree.
                    let mut at_anchor = (*existing).clone();
                    if at_anchor.slashed_at_daa_score.is_some_and(|d| d > anchor_daa) {
                        at_anchor.slashed_at_daa_score = None;
                    }
                    if at_anchor.unbond_request_daa_score.is_some_and(|d| d > anchor_daa) {
                        at_anchor.unbond_request_daa_score = None;
                    }
                    at_anchor.status = effective_bond_status(&at_anchor, anchor_daa);
                    if at_anchor != *rec {
                        return Err(mismatch(format!("bond {}", rec.bond_outpoint)));
                    }
                }
            }
        }
        let insert_reserve =
            match self.reserve_balance_store.get(pruning_point).optional().map_err(|e| unavailable("reserve row", e))? {
                None => canonical.reserve_balance > 0,
                Some(existing) if existing == canonical.reserve_balance => false,
                Some(_) => return Err(mismatch("reserve balance".into())),
            };
        if let Some(existing) = overlay_write.get().optional().map_err(|e| unavailable("snapshot", e))?
            && existing.pruning_point == pruning_point
            && existing.snapshot.commitment_preimage() != canonical.commitment_preimage()
        {
            return Err(mismatch("snapshot".into()));
        }
        let mut batch = WriteBatch::default();
        for rec in new_bonds {
            bonds_write.insert_batch(&mut batch, rec.bond_outpoint, std::sync::Arc::new(rec)).unwrap();
        }
        if insert_reserve {
            self.reserve_balance_store.insert_batch(&mut batch, pruning_point, canonical.reserve_balance).unwrap();
        }
        let overlay_commit =
            overlay_write.set_batch_tracked(&mut batch, PruningPointOverlaySnapshot { pruning_point, snapshot: canonical }).unwrap();
        // Legacy monetary/bond rights are another sidecar of the same import.
        // Mark recovery incomplete in its batch, even if no UTXOs were cleared yet.
        let mut pruning_meta_write = self.pruning_meta_stores.write();
        pruning_meta_write.set_pruning_utxoset_stable_flag(&mut batch, false).unwrap();
        self.write_pruning_import_barrier(batch);
        overlay_commit.committed();
        drop(pruning_meta_write);
        Ok(())
    }

    /// kaspa-pq ADR-0022 (serving side): the persisted pruning-point overlay snapshot, for
    /// a peer to stream during another node's headers-proof IBD. `None` if the overlay is
    /// dormant or no snapshot has been captured yet (captured at pruning-advance).
    pub fn pruning_point_overlay_snapshot(&self) -> Option<PruningPointOverlaySnapshot> {
        self.pruning_overlay_snapshot_store.read().get().ok()
    }

    /// kaspa-pq ADR-0022: reconstruct the bond set as-of `pp_daa` from the never-pruned
    /// `stake_bonds_store`. A bond belongs to the as-of-pp set iff it was created
    /// (`created_daa_score`) at/below `pp_daa`; mutations stamped after `pp_daa`
    /// (slash / unbond) did not apply yet, so they are nulled. The `status` field is
    /// left as-is — `compute_overlay_snapshot` normalizes it via `effective_bond_status`
    /// at the anchor. Exact (records are never deleted, only revert-of-Insert), O(bondset).
    fn bonds_as_of(&self, pp_daa: u64) -> Vec<StakeBondRecord> {
        self.stake_bonds_store
            .read()
            .iterator()
            .filter_map(|r| r.ok().map(|(_, rec)| (*rec).clone()))
            .filter(|rec| rec.created_daa_score <= pp_daa)
            .map(|mut rec| {
                if rec.slashed_at_daa_score.is_some_and(|d| d > pp_daa) {
                    rec.slashed_at_daa_score = None;
                }
                if rec.unbond_request_daa_score.is_some_and(|d| d > pp_daa) {
                    rec.unbond_request_daa_score = None;
                }
                rec
            })
            .collect()
    }

    /// kaspa-pq ADR-0022: capture the overlay snapshot as-of `pruning_point` into the
    /// persisted store, for serving + the below-pruning-point window consult. MUST be
    /// called BEFORE pruning deletes the below-pruning-point overlay rows (the window walk
    /// reads them). The reconstructed as-of-pp bond view + the still-present per-block
    /// rows reproduce exactly what a node computed when it validated the pruning point's
    /// child (so the first post-pruning block's `c == v` on an importer matches).
    /// **Materialise the PALW state AT the pruning point, while the deltas below it still exist.**
    ///
    /// The server used to answer a peer's `RequestPruningPointPalwState` only when the singleton
    /// TIP row happened to name the pruning point — but the tip is rewritten to the sink on every
    /// virtual walk, so on any running node that equality is permanently false and the reply was
    /// always `found: false`. Every pruned IBD therefore hard-aborted, and the only way to join
    /// was a replay from genesis with an LLM inference verified per header. The overlay lanes grew
    /// this sibling long ago (`capture_pruning_point_overlay_snapshot`, right below); PALW never
    /// did.
    ///
    /// Must run BEFORE `prune` deletes the below-pruning-point delta rows — the walk back from the
    /// tip is what needs them — which is why the call site sits beside the overlay one.
    pub fn capture_pruning_point_palw_state(&self, pruning_point: BlockHash) {
        let Some(params) = self.palw_state_params_v2.as_ref() else {
            return;
        };
        let store = self.palw_state_v2_store.read();
        let loaded = match store.load_tip_retained_v1(params) {
            Ok(Some(loaded)) => loaded,
            Ok(None) => {
                warn!("PALW: no V2 state tip to capture a pruning-point snapshot from; peers cannot be served a pruned sync yet");
                return;
            }
            Err(e) => {
                warn!("PALW: the V2 state tip does not load ({e}); no pruning-point snapshot captured");
                return;
            }
        };
        let (tip_block, tip_state) = loaded;
        let state = if tip_block == pruning_point {
            tip_state
        } else {
            let path = self.dag_traversal_manager.calculate_chain_path(tip_block, pruning_point, None);
            let removed: Vec<BlockHash> = path.removed.to_vec();
            let added: Vec<BlockHash> = path.added.to_vec();
            match crate::processes::palw_state_walk::walk_chain_path_retained_v1(&store, params, tip_state, &removed, &added) {
                Ok(state) => state,
                Err(e) => {
                    warn!(
                        "PALW: cannot walk the V2 state from tip {tip_block} back to pruning point {pruning_point} ({e}); no snapshot captured, so this node cannot serve a pruned sync"
                    );
                    return;
                }
            }
        };
        drop(store);
        let mut batch = WriteBatch::default();
        let mut store = self.palw_state_v2_store.write();
        let _native_snapshot_write = match store.set_pruning_snapshot_batch_tracked_v1(&mut batch, pruning_point, &state) {
            Ok(ticket) => ticket,
            Err(e) => {
                warn!("PALW: the pruning-point snapshot did not stage ({e})");
                return;
            }
        };
        drop(store);
        self.db.write(batch).unwrap();
        info!("PALW: captured the V2 state at pruning point {pruning_point} — peers can now sync from it");
    }

    pub fn capture_pruning_point_overlay_snapshot(&self, pruning_point: BlockHash) {
        if self.dns_params.is_none() {
            return;
        }
        let pp_daa = self.headers_store.get_daa_score(pruning_point).unwrap();
        let view = ActiveBondView::from_records(self.bonds_as_of(pp_daa).into_iter().map(|r| (r.bond_outpoint, r)));
        let snapshot = self.compute_overlay_snapshot(pruning_point, &view);
        let mut batch = WriteBatch::default();
        let overlay_commit = self
            .pruning_overlay_snapshot_store
            .write()
            .set_batch_tracked(&mut batch, PruningPointOverlaySnapshot { pruning_point, snapshot })
            .unwrap();
        self.db.write(batch).unwrap();
        overlay_commit.committed();
    }

    pub fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        // Ideally we would want to check if the last known pruning point has the finality point
        // in its chain, but in some cases it's impossible: let `lkp` be the last known pruning
        // point from the list, and `fup` be the first unknown pruning point (the one following `lkp`).
        // fup.blue_score - lkp.blue_score ≈ finality_depth (±k), so it's possible for `lkp` not to
        // have the finality point in its past. So we have no choice but to check if `lkp`
        // has `finality_point.finality_point` in its chain (in the worst case `fup` is one block
        // above the current finality point, and in this case `lkp` will be a few blocks above the
        // finality_point.finality_point), meaning this function can only detect finality violations
        // in depth of 2*finality_depth, and can give false negatives for smaller finality violations.
        let current_pp = self.pruning_point_store.read().pruning_point().unwrap();
        let vf = self.virtual_finality_point(&self.lkg_virtual_state.load().ghostdag_data, current_pp);
        let vff = self.depth_manager.calc_finality_point(&self.ghostdag_store.get_data(vf).unwrap(), current_pp);

        let last_known_pp = pp_list.iter().rev().find(|pp| match self.statuses_store.read().get(pp.hash).optional().unwrap() {
            Some(status) => status.is_valid(),
            None => false,
        });

        if let Some(last_known_pp) = last_known_pp {
            !self.reachability_service.is_chain_ancestor_of(vff, last_known_pp.hash)
        } else {
            // If no pruning point is known, there's definitely a finality violation
            // (normally at least genesis should be known).
            true
        }
    }

    /// Executes `op` within the thread pool associated with this processor.
    pub fn install<OP, R>(&self, op: OP) -> R
    where
        OP: FnOnce() -> R + Send,
        R: Send,
    {
        self.thread_pool.install(op)
    }
}

enum MergesetIncreaseResult {
    Accepted {
        increase_size: u64,
        /// ADR-0068 Phase 1 (F3a/F5): the increase's heartbeat members as
        /// `(blue_score, hash, timestamp)`, empty when the lane is not armed. The CALLER decides
        /// admissibility over the whole accumulated set (`heartbeat_set_admissible`) —
        /// width with the chain exemption is a property of the final mergeset, not of one
        /// candidate's increase, and a rejected candidate is simply skipped (the excess IS
        /// a heartbeat or reaches one; there is nothing to substitute).
        heartbeat_members: Vec<(u64, BlockHash, u64)>,
    },
    Rejected {
        new_candidate: BlockHash,
    },
}

/// The name of a lifecycle object's kind, for logging what a block carried.
///
/// Total on purpose — no catch-all — so a new object kind has to decide what it is called here
/// rather than disappear into "lifecycle object", which is exactly the shape that made a live
/// court drill unreadable on the panel side.
fn palw_object_kind_name(object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2) -> &'static str {
    use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as O;
    match object {
        O::BondRegistered { .. } => "BondRegistered",
        O::SeatReadinessProved { .. } => "SeatReadinessProved",
        O::ClassManifestV2 { .. } => "ClassManifestV2",
        O::ReceiptLicensedV2 { .. } => "ReceiptLicensedV2",
        O::ReceiptLicensedBatchV1 { .. } => "ReceiptLicensedBatchV1",
        O::VerificationVertexV1 { .. } => "VerificationVertexV1",
        O::ExecWorkRootOpenedV2 { .. } => "ExecWorkRootOpenedV2",
        O::ExecOriginRecordedV1 { .. } => "ExecOriginRecordedV1",
        O::VertexEquivocationV1 { .. } => "VertexEquivocationV1",
        O::TrapCommittedV1 { .. } => "TrapCommittedV1",
        O::TrapRevealedV1 { .. } => "TrapRevealedV1",
        O::PanelBeaconProofV3 { .. } => "PanelBeaconProofV3",
        O::AuditReceiptBatchV1 { .. } => "AuditReceiptBatchV1",
        O::AttemptRidersV1 { .. } => "AttemptRidersV1",
        O::ClassRegisteredTirV1 { .. } => "ClassRegisteredTirV1",
        O::ClassRegisteredGenV1 { .. } => "ClassRegisteredGenV1",
        O::TirShardCourtAccused { .. } => "TirShardCourtAccused",
        O::TirShardPlanDeclared { .. } => "TirShardPlanDeclared",
        O::TirShardReceiptLicensed { .. } => "TirShardReceiptLicensed",
        O::TirSeatReadinessProved { .. } => "TirSeatReadinessProved",
        O::GenShardCourtAccused { .. } => "GenShardCourtAccused",
        O::HeldLeafChallengeDeclared { .. } => "HeldLeafChallengeDeclared",
        O::AdapterClassListed { .. } => "AdapterClassListed",
        O::KernelRouteV1 { .. } => "KernelRouteV1",
        O::KernelConstraintReceiptV1 { .. } => "KernelConstraintReceiptV1",
        O::KernelRouteChunkV1 { .. } => "KernelRouteChunkV1",
        O::ArtifactBoundV1 { .. } => "ArtifactBoundV1",
        O::ArtifactBindingChallengedV1 { .. } => "ArtifactBindingChallengedV1",
        O::KernelBoundV1 { .. } => "KernelBoundV1",
        O::ConformanceCommittedV1 { .. } => "ConformanceCommittedV1",
        O::SignedRegistrationV1 { .. } => "SignedRegistrationV1",
        O::ConformanceEvidenceV1 { .. } => "ConformanceEvidenceV1",
        O::ProviderLeaseV1 { .. } => "ProviderLeaseV1",
        O::ProviderChallengeV1 { .. } => "ProviderChallengeV1",
        O::ProviderAnswerV1 { .. } => "ProviderAnswerV1",
        O::DaTransferV1 { .. } => "DaTransferV1",
        O::DisputeReservedV1 { .. } => "DisputeReservedV1",
        O::DisputeReleasedV1 { .. } => "DisputeReleasedV1",
        O::DisputeReacquiredV1 { .. } => "DisputeReacquiredV1",
        O::LegacyHeldDemandedV2 { .. } => "LegacyHeldDemandedV2",
        O::LegacyHeldAnsweredV2 { .. } => "LegacyHeldAnsweredV2",
        O::LegacyHeldAnswerDeclaredV2 { .. } => "LegacyHeldAnswerDeclaredV2",
        O::LegacyHeldAnswerChunkV2 { .. } => "LegacyHeldAnswerChunkV2",
        O::ObjectiveEvidenceDeclaredV1 { .. } => "ObjectiveEvidenceDeclaredV1",
        O::ObjectiveEvidenceChunkV1 { .. } => "ObjectiveEvidenceChunkV1",
        O::PublicCourtStreamMoveV1 { .. } => "PublicCourtStreamMoveV1",
        O::PublicCourtStreamLeafV1 { .. } => "PublicCourtStreamLeafV1",
        O::PublicCourtStreamOpenedV1 { .. } => "PublicCourtStreamOpenedV1",
        O::PublicCourtStreamOpenedNativeV1 { .. } => "PublicCourtStreamOpenedNativeV1",
        O::TirClassLayoutPublishedV1 { .. } => "TirClassLayoutPublishedV1",
        O::TirClassContainerBoundV1 { .. } => "TirClassContainerBoundV1",
        O::LegacyLeafRecomputedV2 { .. } => "LegacyLeafRecomputedV2",
        O::BondCapitalAssignedV1 { .. } => "BondCapitalAssignedV1",
        O::MonetaryMaterialEnrolledV1 { .. } => "MonetaryMaterialEnrolledV1",
        O::MonetaryMaterialReplacedV1 { .. } => "MonetaryMaterialReplacedV1",
        O::MonetaryMaterialActionV1 { .. } => "MonetaryMaterialActionV1",
        O::ClassSemanticsDeclaredV1 { .. } => "ClassSemanticsDeclaredV1",
        O::GenTensorCommitted { .. } => "GenTensorCommitted",
        O::ClassLaneCertifiedTirV1 { .. } => "ClassLaneCertifiedTirV1",
        O::CourtTirRootClaimed { .. } => "CourtTirRootClaimed",
        O::CourtGenRootClaimed { .. } => "CourtGenRootClaimed",
        O::CourtEvalRootClaimed { .. } => "CourtEvalRootClaimed",
        O::CourtTirDissected { .. } => "CourtTirDissected",
        O::CourtTirChildChosen { .. } => "CourtTirChildChosen",
        O::DefaultAccusedTirStep { .. } => "DefaultAccusedTirStep",
        // Phase F (tags 83–85): pipeline-claim data availability; 84 and 85 are reserved and uninhabited.
        O::DefaultAccusedPipelineStep { .. } => "DefaultAccusedPipelineStep",
        O::ReservedPipelineDa84(never) | O::ReservedPipelineDa85(never) => match *never {},
        // RFC-0004 (tags 70–82): dropped at acceptance until each admission lands.
        O::ModelLineImprovementPolicySet { .. } => "ModelLineImprovementPolicySet",
        O::HardCaseSubmitted { .. } => "HardCaseSubmitted",
        O::DataUseOptIn { .. } => "DataUseOptIn",
        O::SetterSetCommitted { .. } => "SetterSetCommitted",
        O::SetterSetRevealed { .. } => "SetterSetRevealed",
        O::SetterKeysRevealed { .. } => "SetterKeysRevealed",
        O::HardCaseKeyRevealed { .. } => "HardCaseKeyRevealed",
        O::DatasetRegistered { .. } => "DatasetRegistered",
        O::TeachingArtifactCommitted { .. } => "TeachingArtifactCommitted",
        O::TeachingArtifactRevealed { .. } => "TeachingArtifactRevealed",
        O::TeacherLicenceRegistered { .. } => "TeacherLicenceRegistered",
        O::CandidateSubmitted { .. } => "CandidateSubmitted",
        O::LineageHeadRolledBack { .. } => "LineageHeadRolledBack",
        O::ImprovementPoolFunded { .. } => "ImprovementPoolFunded",
        O::OptimisticLicensed { .. } => "OptimisticLicensed",
        // ADR-0152 v22 skeleton: declared, dropped at acceptance until landed.
        O::ReporterCommitted { .. } => "ReporterCommitted",
        O::ReporterRevealed { .. } => "ReporterRevealed",
        O::MaterialDisclosedV2 { .. } => "MaterialDisclosedV2",
        O::PanelUnavailableQuorum { .. } => "PanelUnavailableQuorum",
        O::SeatReadinessProvedV2 { .. } => "SeatReadinessProvedV2",
        O::ObjectiveOffence { .. } => "ObjectiveOffence",
        O::ModelBuy { .. } => "ModelBuy",
        O::ModelSeed { .. } => "ModelSeed",
        O::ShardCourtAccused { .. } => "ShardCourtAccused",
        O::GdnStepAccusedV2 { .. } => "GdnStepAccusedV2",
        O::MemoryRoutingV2 { .. } => "MemoryRoutingV2",
        O::TirDesignatedEventV1 { .. } => "TirDesignatedEventV1",
        O::SeatReadinessRootV1 { .. } => "SeatReadinessRootV1",
        O::MaterialIntakeV1 { .. } => "MaterialIntakeV1",
        O::ClassShardPlanDeclared { .. } => "ClassShardPlanDeclared",
        O::BondShardsDeclared { .. } => "BondShardsDeclared",
        O::ShardReceiptLicensed { .. } => "ShardReceiptLicensed",
        O::CheckpointAccused { .. } => "CheckpointAccused",
        O::DefaultAccusedHeld { .. } => "DefaultAccusedHeld",
        O::MaterialDisclosedHeld { .. } => "MaterialDisclosedHeld",
        O::RoundPermitEquivocated { .. } => "RoundPermitEquivocated",
        O::ModelSell { .. } => "ModelSell",
        O::ModelLineFounded { .. } => "ModelLineFounded",
        O::ModelVersionPublished { .. } => "ModelVersionPublished",
        O::ModelVersionPromoted { .. } => "ModelVersionPromoted",
        O::ModelVersionWithdrawn { .. } => "ModelVersionWithdrawn",
        O::ModelLineBenefitsDeclared { .. } => "ModelLineBenefitsDeclared",
        O::ModelLineRolesSet { .. } => "ModelLineRolesSet",
        O::ModelLineOwnerTransferred { .. } => "ModelLineOwnerTransferred",
        O::ModelLineRetired { .. } => "ModelLineRetired",
        O::ModelProposalPosted { .. } => "ModelProposalPosted",
        O::ModelProposalClosed { .. } => "ModelProposalClosed",
        O::ModelEvaluationPosted { .. } => "ModelEvaluationPosted",
        O::BondRetireRequested { .. } => "BondRetireRequested",
        O::BondCapabilityDeclared { .. } => "BondCapabilityDeclared",
        O::ClassRegistered { .. } => "ClassRegistered",
        O::ClassFrozen { .. } => "ClassFrozen",
        O::PanelBound { .. } => "PanelBound",
        O::ReceiptLicensed { .. } => "ReceiptLicensed",
        O::CourtOpened { .. } => "CourtOpened",
        O::CourtClosed { .. } => "CourtClosed",
        O::CourtDisclosed { .. } => "CourtDisclosed",
        O::CourtVerdictPosted { .. } => "CourtVerdictPosted",
        O::ProducerDefaulted { .. } => "ProducerDefaulted",
        O::FreePromptCommitted { .. } => "FreePromptCommitted",
        O::FamilyCertified { .. } => "FamilyCertified",
        O::ClassLaneCertified { .. } => "ClassLaneCertified",
        O::ObjectChunk { .. } => "ObjectChunk",
        O::DerivedArtifactV1 { .. } => "DerivedArtifactV1",
        O::DefaultAccused { .. } => "DefaultAccused",
        O::MaterialDisclosed { .. } => "MaterialDisclosed",
        O::CourtCloseDeclared { .. } => "CourtCloseDeclared",
        O::CourtCloseChunk { .. } => "CourtCloseChunk",
        // ADR-0082 Decision 2 — the fused-attention dissection's three moves.
        O::CourtAttnRootClaimed { .. } => "CourtAttnRootClaimed",
        // ADR-0093 Decision 8 — move 1 with its anchor.
        O::CourtAttnRootClaimedAnchored { .. } => "CourtAttnRootClaimedAnchored",
        // ADR-0152 §4-ter C2 — move 1 of a held class, with its anchor's slice sub-roots (tag 57).
        O::CourtAttnRootClaimedHeld { .. } => "CourtAttnRootClaimedHeld",
        O::CourtAttnDissected { .. } => "CourtAttnDissected",
        O::CourtAttnChildChosen { .. } => "CourtAttnChildChosen",
        // ADR-0152-adjacent — a sink-bound top-up of a class's Activation Pool (tag 58).
        O::ActivationPoolFunded { .. } => "ActivationPoolFunded",
        // G14 — the original paid TIR control (tag 170) and its atomic Valid grant / Ready (tag 171).
        O::TirPublicControlV1 { .. } => "TirPublicControlV1",
        O::TirPublicGrantReadyV1 { .. } => "TirPublicGrantReadyV1",
    }
}

/// ADR-0109 Decision 1: the index row of `entry` if its script is an `EVM_DEPOSIT_LOCK`, decided
/// with the parser the claim's validity uses (`validate_one_deposit_claim`) — so what the index
/// calls a lock and what the chain calls one cannot diverge.
pub(super) fn deposit_lock_record(entry: &UtxoEntry) -> Option<kaspa_consensus_core::evm::EvmDepositLockRecord> {
    let lock = kaspa_txscript::script_class::parse_evm_deposit_lock(&entry.script_public_key)?;
    Some(kaspa_consensus_core::evm::EvmDepositLockRecord {
        evm_address: kaspa_consensus_core::evm::EvmAddress::from_bytes(lock.evm_address),
        amount_sompi: entry.amount,
        claim_tip_sompi: lock.claim_tip_sompi,
        timeout_daa_score: lock.timeout_daa_score,
        block_daa_score: entry.block_daa_score,
    })
}

/// **ADR-0109 Decision 3's mempool gate, both halves** — what the chain walk and the template hand
/// the per-transaction check as `BondSpendFilter`'s PALW side, read at the virtual tip: the bonds a
/// spend may not touch, and what a spend of a released, slashed bond must leave unclaimed
/// (`BondBurnNotPaid`). The lock half alone left a signed, burn-evading spend of a released bond
/// admitted and relayed, then dropped by every template and skipped at every merge (the PROC-A
/// review's probe); the burn half closes that the way the lock half closed the locked spend.
pub(super) struct PalwMempoolBondGate {
    pub(super) locked: std::collections::HashSet<TransactionOutpoint>,
    pub(super) burns: std::collections::HashMap<TransactionOutpoint, u64>,
    /// **G14 lane escrow-rfc08 round 2: the funded-escrow locks at the tip** (rooted rows only — a mempool has no mergeset, so no
    /// provisional lock; a chained spend of a not-yet-accepted TFO1 output is refused at the merge by the provisional lock).
    pub(super) escrow: super::utxo_validation::PalwEscrowSpendLocksV1,
}

/// ADR-0109 Decision 3: the first input of `tx` that spends a locked PALW bond, if any.
pub(super) fn first_locked_input(
    tx: &Transaction,
    locked: &std::collections::HashSet<TransactionOutpoint>,
) -> Option<TransactionOutpoint> {
    tx.inputs.iter().map(|input| input.previous_outpoint).find(|outpoint| locked.contains(outpoint))
}

// ---------------------------------------------------------------------------------------------
// RFC-0008 v2: the weightless carriage
// ---------------------------------------------------------------------------------------------

/// **RFC-0008 v2: a chain block's anchor, resolved** — the closure its heads cover and the trailer itself.
pub(super) struct PalwExecV2ResolvedV1 {
    pub closure: kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2ClosureV1,
    #[allow(dead_code)]
    pub anchor: kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorV1,
}

/// **RFC-0008 v2: the DAG a closure walks**, read from the header and GHOSTDAG stores and reachability, for the block whose selected
/// parent is `selected_parent`.
struct ExecV2DagViewV1<'a> {
    processor: &'a VirtualStateProcessor,
    selected_parent: BlockHash,
    span_daa: u64,
}

impl kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2DagV1 for ExecV2DagViewV1<'_> {
    fn node(&self, hash: &BlockHash) -> Option<kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2NodeV1> {
        use kaspa_consensus_core::palw_exec_v2::PalwExecV2Envelope;
        use kaspa_consensus_core::palw_exec_v2_anchor::{PalwExecV2MemberKeyV1, PalwExecV2NodeV1};
        use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1;
        let p = self.processor;
        let header = p.headers_store.get_header(*hash).ok()?;
        let chain =
            PalwExecV2NodeV1 { is_lane: false, legacy: false, key: None, anchor_span: 0, anchor_on_chain: true, parents: Vec::new() };
        let is_lane = header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1
            && p.palw_execution_lane.is_some_and(|lane| lane.activation.is_active(header.daa_score));
        if !is_lane {
            return Some(chain);
        }
        let anchor = p.ghostdag_store.get_selected_parent(*hash).ok()?;
        let anchor_daa = p.headers_store.get_daa_score(anchor).ok()?;
        let anchor_span = palw_execution_span_v1(anchor_daa, self.span_daa);
        let anchor_on_chain = p.reachability_service.try_is_chain_ancestor_of(anchor, self.selected_parent).unwrap_or(false);
        let parents = header.direct_parents().to_vec();
        let lane_node = |legacy: bool, key: Option<PalwExecV2MemberKeyV1>| PalwExecV2NodeV1 {
            is_lane: true,
            legacy,
            key,
            anchor_span,
            anchor_on_chain,
            parents: parents.clone(),
        };
        if !PalwExecV2Envelope::is_v2_carriage(&header.palw_commitment) {
            return Some(lane_node(true, None));
        }
        let Ok(envelope) = PalwExecV2Envelope::decode(&header.palw_commitment) else {
            return Some(lane_node(true, None));
        };
        let key = match (&envelope.tx_permit, &envelope.work_slice) {
            (Some(permit), None) => Some(PalwExecV2MemberKeyV1::Tx { round: permit.round, permit_index: permit.permit_index }),
            (None, Some(slice)) => {
                Some(PalwExecV2MemberKeyV1::Slice { root_claim_id: slice.root_claim_id, slice_index: slice.slice_index })
            }
            _ => None,
        };
        Some(lane_node(false, key))
    }
}

impl VirtualStateProcessor {
    /// **RFC-0008 v2: is the EXEC payload in force for a block at `daa_score`?**
    pub(super) fn palw_exec_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_exec_v2.is_some_and(|fence| fence.is_active(daa_score))
    }

    /// **RFC-0008 v2: the closure an anchor with `heads` covers for a block at `daa_score` whose selected parent is
    /// `selected_parent` and whose parent state is `state`** — the one function the template and the validating walk both call.
    pub(super) fn palw_exec_v2_closure(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        selected_parent: BlockHash,
        daa_score: u64,
        heads: &[BlockHash],
    ) -> Result<
        kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2ClosureV1,
        kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorErrorV1,
    > {
        use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1;
        let lane = self
            .palw_execution_lane_at(daa_score)
            .ok_or(kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorErrorV1::BeforeFence)?;
        let span_daa = lane.schedule_span_daa_at(daa_score);
        let span_now = palw_execution_span_v1(daa_score, span_daa);
        let view = ExecV2DagViewV1 { processor: self, selected_parent, span_daa };
        kaspa_consensus_core::palw_exec_v2_anchor::palw_exec_v2_closure_v1(
            &view,
            heads,
            |block| state.exec_v2_anchored_v1(block),
            span_now,
            lane.max_per_mergeset as usize,
        )
    }

    /// **RFC-0008 v2: the chain block `current`'s mergeset with the EXEC blocks its anchor covers.** Returns the ghostdag data
    /// unchanged where the payload is not in force or the coinbase carries no anchor. Otherwise the covered blocks are appended to the
    /// reds in canonical order — every consumer downstream (the verdicts, the acceptance loop, the coinbase payouts) then sees them
    /// exactly as it saw a red round block, and the stored ghostdag data (the DAG's own) never changes: the EXEC block adds no blue
    /// work, no blue score, no DAA term, no pruning level and no k-anticone term to any block, because no block's stored data
    /// contains it.
    pub(super) fn palw_exec_v2_augment(
        &self,
        current: BlockHash,
        header: &Header,
        state: Option<&kaspa_consensus_core::palw_state_v2::PalwChainStateV2>,
        mergeset_data: std::sync::Arc<GhostdagData>,
    ) -> Result<
        (std::sync::Arc<GhostdagData>, Option<PalwExecV2ResolvedV1>),
        kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorErrorV1,
    > {
        use kaspa_consensus_core::palw_exec_v2_anchor::{
            PalwExecV2AnchorErrorV1, palw_exec_v2_anchor_split, palw_exec_v2_anchor_verify,
        };
        if !self.palw_exec_v2_active_at(header.daa_score) {
            return Ok((mergeset_data, None));
        }
        let Ok(transactions) = self.block_transactions_store.get(current) else {
            return Ok((mergeset_data, None));
        };
        let Some(coinbase) = transactions.first() else {
            return Ok((mergeset_data, None));
        };
        let Some(anchor) = palw_exec_v2_anchor_split(&coinbase.payload)?.1 else {
            return Ok((mergeset_data, None));
        };
        if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_ROUND_V1 {
            return Err(PalwExecV2AnchorErrorV1::LaneBlockAnchors);
        }
        // No PALW state means no lane to cover: the anchor is refused rather than ignored.
        let state = state.ok_or(PalwExecV2AnchorErrorV1::BeforeFence)?;
        let closure = self.palw_exec_v2_closure(state, mergeset_data.selected_parent, header.daa_score, &anchor.heads)?;
        palw_exec_v2_anchor_verify(&anchor, &closure)?;
        let mut augmented = (*mergeset_data).clone();
        {
            let reds = kaspa_consensus_core::blockhash::BlockHashes::make_mut(&mut augmented.mergeset_reds);
            reds.extend(closure.members.iter().map(|member| member.hash));
        }
        Ok((std::sync::Arc::new(augmented), Some(PalwExecV2ResolvedV1 { closure, anchor })))
    }

    /// **RFC-0008 v2: the mergeset's non-DAA set with the covered EXEC blocks in it.** The window manager stores the set for the DAG's
    /// own mergeset; a covered EXEC block is outside the DAA exactly as a red round block always was, so it joins the set wherever the
    /// augmented mergeset is read. A clone only where there is something to add.
    ///
    /// Below the fence (and where it is not armed) this is the stored set, untouched — the read of the build before RFC-0008 v2 (the
    /// X8R review: gated on the block's own DAA, not on the fence being armed, so an armed node agrees with an unarmed one below it).
    pub(super) fn palw_exec_v2_non_daa(&self, stored: &BlockHashSet, ghostdag_data: &GhostdagData, daa_score: u64) -> BlockHashSet {
        if !self.palw_exec_v2_active_at(daa_score) {
            return stored.clone();
        }
        let mut out = stored.clone();
        out.extend(self.palw_round_blocks_of(ghostdag_data));
        out
    }

    /// **RFC-0008 v2: what the fold records of a resolved anchor** — the covered blocks with their anchor spans, the covered slice
    /// carriers in canonical order, and the clock the window is read at.
    pub(super) fn palw_exec_v2_fold_inputs(
        &self,
        resolved: &PalwExecV2ResolvedV1,
        daa_score: u64,
    ) -> (
        kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorFoldV1,
        Vec<kaspa_consensus_core::palw_work_slice_v2::PalwExecV2CoveredSliceV1>,
    ) {
        use kaspa_consensus_core::palw_exec_v2::PalwExecV2Envelope;
        use kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2MemberKeyV1;
        use kaspa_consensus_core::palw_execution_lane_v1::palw_execution_span_v1;
        let span_daa = self.palw_execution_lane_at(daa_score).map(|lane| lane.schedule_span_daa_at(daa_score)).unwrap_or(1).max(1);
        let anchor_span = |hash: BlockHash| {
            self.ghostdag_store
                .get_selected_parent(hash)
                .ok()
                .and_then(|anchor| self.headers_store.get_daa_score(anchor).ok())
                .map(|daa| palw_execution_span_v1(daa, span_daa))
                .unwrap_or(0)
        };
        let mut slices = Vec::new();
        for member in &resolved.closure.members {
            if !matches!(member.key, PalwExecV2MemberKeyV1::Slice { .. }) {
                continue;
            }
            let Ok(header) = self.headers_store.get_header(member.hash) else { continue };
            let Ok(envelope) = PalwExecV2Envelope::decode(&header.palw_commitment) else { continue };
            if let Some(slice) = envelope.work_slice {
                slices.push(kaspa_consensus_core::palw_work_slice_v2::PalwExecV2CoveredSliceV1 {
                    carrier: member.hash,
                    slice,
                    pubkey: envelope.pubkey,
                });
            }
        }
        (
            kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorFoldV1 {
                members: resolved.closure.members.iter().map(|member| (member.hash, anchor_span(member.hash))).collect(),
                span_now: palw_execution_span_v1(daa_score, span_daa),
            },
            slices,
        )
    }

    /// **RFC-0008 v2: the lane's tips this node holds** — algo-10 body tips, newest first, with their (header) timestamps.
    fn palw_exec_v2_lane_tips(&self) -> Vec<(u64, BlockHash)> {
        let mut tips: Vec<(u64, BlockHash)> = self
            .body_tips_store
            .read()
            .get()
            .unwrap()
            .read()
            .iter()
            .copied()
            .filter(|tip| self.ghostdag_manager.is_round_block(*tip))
            .filter_map(|tip| self.headers_store.get_header(tip).ok().map(|header| (header.timestamp, tip)))
            .collect();
        tips.sort_by_key(|(timestamp, tip)| std::cmp::Reverse((*timestamp, *tip)));
        tips
    }

    /// **RFC-0008 v2: the lane heads a block built on `selected_parent` should anchor** — this node's lane tips (blocks no block of this
    /// node has built on), newest first, those whose own closure still covers something, while the union stays inside the lane's leaf
    /// bound; at most [`kaspa_consensus_core::palw_exec_v2_anchor::PALW_EXEC_V2_MAX_HEADS`]. A stale or foreign tip covers nothing and
    /// is left out, so the lane's checkpoint — not its wedged head — is what gets anchored.
    pub(super) fn palw_exec_v2_pick_heads(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        selected_parent: BlockHash,
        daa_score: u64,
    ) -> Vec<BlockHash> {
        use kaspa_consensus_core::palw_exec_v2_anchor::PALW_EXEC_V2_MAX_HEADS;
        let Some(lane) = self.palw_execution_lane_at(daa_score) else { return Vec::new() };
        let mut heads: Vec<BlockHash> = Vec::new();
        for (_, tip) in self.palw_exec_v2_lane_tips().into_iter().take(4 * PALW_EXEC_V2_MAX_HEADS) {
            if heads.len() >= PALW_EXEC_V2_MAX_HEADS {
                break;
            }
            let Ok(alone) = self.palw_exec_v2_closure(state, selected_parent, daa_score, &[tip]) else { continue };
            if alone.members.is_empty() {
                continue;
            }
            let mut tentative = heads.clone();
            tentative.push(tip);
            match self.palw_exec_v2_closure(state, selected_parent, daa_score, &tentative) {
                Ok(union) if union.members.len() <= lane.max_per_mergeset as usize => heads = tentative,
                _ => {}
            }
        }
        heads.sort();
        heads.dedup();
        heads
    }

    /// **RFC-0008 v2: virtual's ghostdag data with the blocks the best anchor covers appended to its reds**, and the anchor itself (what
    /// the template's coinbase will carry). `None` when there is nothing to cover.
    pub(super) fn palw_exec_v2_virtual(
        &self,
        ghostdag_data: &GhostdagData,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        daa_score: u64,
    ) -> Option<(GhostdagData, kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorV1)> {
        let heads = self.palw_exec_v2_pick_heads(state, ghostdag_data.selected_parent, daa_score);
        if heads.is_empty() {
            return None;
        }
        let closure = self.palw_exec_v2_closure(state, ghostdag_data.selected_parent, daa_score, &heads).ok()?;
        if closure.members.is_empty() {
            return None;
        }
        let anchor = kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2AnchorV1::of_closure(&heads, &closure).ok()?;
        let mut augmented = ghostdag_data.clone();
        kaspa_consensus_core::blockhash::BlockHashes::make_mut(&mut augmented.mergeset_reds)
            .extend(closure.members.iter().map(|member| member.hash));
        Some((augmented, anchor))
    }

    /// **RFC-0008 v2: the lane's health at `sink`** — this node's newest lane head and whether the next anchoring block could still
    /// cover it. Node-local; `None` where the payload is not in force.
    pub(super) fn palw_exec_v2_health(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        sink: BlockHash,
        daa_score: u64,
    ) -> Option<kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2LaneHealthV1> {
        if !self.palw_exec_v2_active_at(daa_score) {
            return None;
        }
        let mut health = kaspa_consensus_core::palw_exec_v2_anchor::PalwExecV2LaneHealthV1::default();
        if let Some((_, head)) = self.palw_exec_v2_lane_tips().first().copied() {
            health.latest_head = Some(head);
            if let Ok(closure) = self.palw_exec_v2_closure(state, sink, daa_score, &[head]) {
                health.pending = closure.members.len() as u64;
                if let Some(reason) = closure.dropped.get(&head) {
                    health.stale = true;
                    health.stale_reason = Some(*reason);
                }
            }
        }
        Some(health)
    }
}
